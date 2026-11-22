//! Download and decode UCDP GED and GED Candidate event releases.

use rand::Rng;
use reqwest::blocking::Client as HttpClient;
use reqwest::header::RETRY_AFTER;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use thiserror::Error;
use zip::ZipArchive;

/// GED global release 26.1 bulk CSV archive.
pub const GED_URL: &str = "https://ucdp.uu.se/downloads/ged/ged261-csv.zip";
/// October 2026 global GED Candidate release 26.0.10.
pub const CANDIDATE_URL: &str = "https://ucdp.uu.se/downloads/candidateged/GEDEvent_v26_0_10.csv";
const GED_MEMBER: &str = "GEDEvent_v26_1.csv";
const MAX_ATTEMPTS: u32 = 4;
const RETRY_BASE: Duration = Duration::from_millis(500);
const RETRY_CAP: Duration = Duration::from_secs(20);

/// Errors returned by the UCDP download and decoding operations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// A local cache could not be read or written.
    #[error("cache I/O failed: {0}")]
    Cache(#[from] std::io::Error),
    /// An HTTP request failed after retries or returned an unsuccessful status.
    #[error("UCDP download failed: {0}")]
    Http(#[from] reqwest::Error),
    /// A release did not match its expected archive or CSV layout.
    #[error("invalid UCDP release: {0}")]
    Decode(String),
    /// The requested release was not available in offline mode.
    #[error("offline cache miss for {0}")]
    OfflineMiss(String),
}

/// A decoded UCDP event row containing fields used by common consumers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Event {
    /// Gleditsch–Ward numeric country identifier used by GED.
    pub country_id: String,
    /// Country label, when included in the release.
    pub country: Option<String>,
    /// Event end date in the upstream `YYYY-MM-DD` layout.
    pub date_end: String,
    /// Best estimate of direct fatalities; malformed or missing values decode as zero.
    pub best: u64,
    /// Stable conflict identifier, when present.
    pub conflict_new_id: Option<String>,
    /// Whether the conflict was active during the release year.
    pub active_year: bool,
}

/// The two event collections consumed by the situation adapter.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Dataset {
    /// Annual GED 26.1 events.
    pub ged: Vec<Event>,
    /// Monthly GED Candidate 26.0.10 events.
    pub candidate: Vec<Event>,
}

/// Controls how the optional on-disk cache is used.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CachePolicy {
    /// Use cached content and download missing releases.
    PreferCache,
    /// Ignore cached content and download fresh releases.
    Refresh,
    /// Read only cached content; missing files are errors.
    Offline,
}

/// Blocking UCDP bulk-release client.
#[derive(Clone, Debug)]
pub struct Client {
    http: HttpClient,
    cache_dir: Option<PathBuf>,
}

impl Client {
    /// Build a client with the default timeout and retry policy.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be configured.
    pub fn new() -> Result<Self, Error> {
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(120))
            .build()?;
        Ok(Self {
            http,
            cache_dir: None,
        })
    }

    /// Use a caller-owned directory for cached release files.
    #[must_use]
    pub fn with_cache_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.cache_dir = Some(path.as_ref().to_path_buf());
        self
    }

    /// Download and decode GED 26.1 and Candidate 26.0.10 releases.
    ///
    /// # Errors
    ///
    /// Returns an error when a release cannot be downloaded, cached, or decoded.
    pub fn fetch(&self, policy: CachePolicy) -> Result<Dataset, Error> {
        let ged = self.get_release(GED_URL, policy)?;
        let candidate = self.get_release(CANDIDATE_URL, policy)?;
        Ok(Dataset {
            ged: decode_ged_zip(&ged)?,
            candidate: decode_candidate_csv(&candidate)?,
        })
    }

    fn get_release(&self, url: &str, policy: CachePolicy) -> Result<Vec<u8>, Error> {
        let cached_path = self.cache_dir.as_ref().map(|directory| {
            let key = format!("{:x}", Sha256::digest(url.as_bytes()));
            directory.join(key)
        });
        if policy != CachePolicy::Refresh {
            if let Some(path) = &cached_path {
                match fs::read(path) {
                    Ok(bytes) => return Ok(bytes),
                    Err(error)
                        if policy == CachePolicy::Offline
                            && error.kind() == std::io::ErrorKind::NotFound =>
                    {
                        return Err(Error::OfflineMiss(url.to_owned()));
                    }
                    Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                        return Err(error.into())
                    }
                    Err(_) => {}
                }
            } else if policy == CachePolicy::Offline {
                return Err(Error::OfflineMiss(url.to_owned()));
            }
        }
        if policy == CachePolicy::Offline {
            return Err(Error::OfflineMiss(url.to_owned()));
        }
        let bytes = self.download(url)?;
        if let Some(path) = cached_path {
            let parent = path
                .parent()
                .ok_or_else(|| Error::Decode("cache path has no parent directory".to_owned()))?;
            fs::create_dir_all(parent)?;
            let temporary = parent.join(format!(
                ".{}.{}.tmp",
                path.file_name().unwrap_or_default().to_string_lossy(),
                std::process::id()
            ));
            fs::write(&temporary, &bytes)?;
            fs::rename(temporary, path)?;
        }
        Ok(bytes)
    }

    fn download(&self, url: &str) -> Result<Vec<u8>, Error> {
        let mut last_error = None;
        for attempt in 0..MAX_ATTEMPTS {
            match self.http.get(url).send() {
                Ok(response) if response.status().is_success() => {
                    return Ok(response.bytes()?.to_vec());
                }
                Ok(response) => {
                    let status = response.status();
                    let retry_after = response
                        .headers()
                        .get(RETRY_AFTER)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.parse::<u64>().ok())
                        .map(Duration::from_secs);
                    let error = match response.error_for_status() {
                        Ok(response) => return Ok(response.bytes()?.to_vec()),
                        Err(error) => error,
                    };
                    if !is_retryable_status(status) || attempt + 1 == MAX_ATTEMPTS {
                        return Err(error.into());
                    }
                    last_error = Some(Error::Http(error));
                    thread::sleep(retry_after.unwrap_or_else(|| retry_delay(attempt)));
                }
                Err(error) => {
                    let retryable = error.is_timeout() || error.is_connect() || error.is_request();
                    if !retryable || attempt + 1 == MAX_ATTEMPTS {
                        return Err(error.into());
                    }
                    last_error = Some(Error::Http(error));
                    thread::sleep(retry_delay(attempt));
                }
            }
        }
        Err(last_error.unwrap_or_else(|| Error::Decode("download attempts exhausted".to_owned())))
    }
}

/// Decode the named GED CSV member from an upstream ZIP release.
///
/// # Errors
///
/// Returns an error if the archive is invalid, lacks the expected member, or its CSV is malformed.
pub fn decode_ged_zip(bytes: &[u8]) -> Result<Vec<Event>, Error> {
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|error| Error::Decode(error.to_string()))?;
    let mut csv_file = archive
        .by_name(GED_MEMBER)
        .map_err(|error| Error::Decode(error.to_string()))?;
    decode_csv(&mut csv_file)
}

/// Decode an upstream-layout GED or Candidate CSV file.
///
/// # Errors
///
/// Returns an error if required columns are absent or CSV decoding fails.
pub fn decode_candidate_csv(bytes: &[u8]) -> Result<Vec<Event>, Error> {
    decode_csv(Cursor::new(bytes))
}

fn decode_csv(reader: impl Read) -> Result<Vec<Event>, Error> {
    let mut csv = csv::ReaderBuilder::new().flexible(true).from_reader(reader);
    let headers = csv
        .headers()
        .map_err(|error| Error::Decode(error.to_string()))?
        .clone();
    let column = |name: &str| {
        headers
            .iter()
            .position(|header| header == name)
            .ok_or_else(|| Error::Decode(format!("UCDP file has no {name} column")))
    };
    let country_id = column("country_id")?;
    let date_end = column("date_end")?;
    let best = column("best")?;
    let country = headers.iter().position(|header| header == "country");
    let conflict = headers
        .iter()
        .position(|header| header == "conflict_new_id");
    let active = headers.iter().position(|header| header == "active_year");
    csv.records()
        .map(|row| {
            let row = row.map_err(|error| Error::Decode(error.to_string()))?;
            Ok(Event {
                country_id: row.get(country_id).unwrap_or_default().to_owned(),
                country: country.and_then(|index| row.get(index)).map(str::to_owned),
                date_end: row.get(date_end).unwrap_or_default().to_owned(),
                best: row.get(best).unwrap_or_default().parse().unwrap_or(0),
                conflict_new_id: conflict
                    .and_then(|index| row.get(index))
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned),
                active_year: active.and_then(|index| row.get(index)) == Some("true"),
            })
        })
        .collect()
}

fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status.as_u16() == 408 || status.as_u16() == 429 || status.is_server_error()
}

fn retry_delay(attempt: u32) -> Duration {
    let multiplier = 1_u32.checked_shl(attempt).unwrap_or(u32::MAX);
    let ceiling = RETRY_BASE.saturating_mul(multiplier).min(RETRY_CAP);
    let ceiling_ms = u64::try_from(ceiling.as_millis()).unwrap_or(u64::MAX);
    Duration::from_millis(rand::rng().random_range(0..=ceiling_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GED: &[u8] = include_bytes!("../tests/fixtures/ged/GEDEvent_v26_1.csv");
    const GED_ZIP: &[u8] = include_bytes!("../tests/fixtures/ged/ged261-csv.zip");
    const CANDIDATE: &[u8] = include_bytes!("../tests/fixtures/candidate/GEDEvent_v26_0_10.csv");

    #[test]
    fn decodes_synthetic_upstream_csv_layouts() {
        let ged = decode_candidate_csv(GED).expect("synthetic GED CSV decodes");
        let candidate = decode_candidate_csv(CANDIDATE).expect("synthetic Candidate CSV decodes");
        assert_eq!(ged.len(), 2);
        assert_eq!(ged[0].country_id, "987");
        assert_eq!(ged[0].best, 4);
        assert!(ged[0].active_year);
        assert_eq!(candidate[0].country, Some("Example Republic".to_owned()));
    }

    #[test]
    fn decodes_the_named_ged_archive_member() {
        let events = decode_ged_zip(GED_ZIP).expect("synthetic GED archive decodes");
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].country_id, "654");
    }

    #[test]
    fn offline_without_cache_returns_a_clear_error() {
        let client = Client::new().expect("client builds");
        assert!(matches!(
            client.fetch(CachePolicy::Offline),
            Err(Error::OfflineMiss(_))
        ));
    }
}
