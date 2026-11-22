//! A reusable blocking client for normalized public WARN layoff notices.

use chrono::NaiveDate;
use rand::Rng;
use reqwest::blocking::Client as HttpClient;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use thiserror::Error;

/// Raw CSV endpoint for the consolidated WARN Feed dataset.
pub const DATA_URL: &str =
    "https://raw.githubusercontent.com/APVentureEngine/warn-act-notices/main/data/warn_notices.csv";
/// Dataset home page and attribution target.
pub const DATASET_URL: &str = "https://github.com/APVentureEngine/warn-act-notices";
const MAX_ATTEMPTS: u32 = 4;

/// One normalized notice, with a date only when the source provides a valid date.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Notice {
    /// Two-letter reporting jurisdiction.
    pub state: String,
    /// Employer name as published by the consolidated feed.
    pub company: String,
    /// Notice location as published by the consolidated feed.
    pub location: String,
    /// Reported affected worker count; invalid or blank values are zero.
    pub workers: u64,
    /// Parsed notice date, or `None` when the source date is absent or invalid.
    pub filed_on: Option<NaiveDate>,
}

/// Failure while requesting, caching, or decoding WARN Feed data.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// HTTP client construction failed.
    #[error("could not construct HTTP client: {0}")]
    HttpClient(#[source] reqwest::Error),
    /// A request failed after retries or returned a non-retryable HTTP status.
    #[error("WARN Feed request failed: {0}")]
    Request(#[source] reqwest::Error),
    /// A cache read or write failed.
    #[error("WARN Feed cache I/O failed: {0}")]
    Cache(#[source] std::io::Error),
    /// The CSV is malformed or lacks a required field.
    #[error("WARN Feed CSV is invalid: {0}")]
    Csv(#[source] csv::Error),
    /// The selected cache path does not have a parent directory.
    #[error("cache path has no parent directory")]
    CachePath,
}

#[derive(Deserialize)]
struct CsvNotice {
    state: String,
    company: String,
    location: String,
    employees_affected: Option<String>,
    notice_date: Option<String>,
}

/// Blocking client for the WARN Feed CSV, with an optional caller-owned cache directory.
#[derive(Clone, Debug)]
pub struct Client {
    endpoint: String,
    cache_dir: Option<PathBuf>,
    http: HttpClient,
}

impl Client {
    /// Creates a client for the public WARN Feed CSV.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be built.
    pub fn new() -> Result<Self, Error> {
        Self::with_endpoint(DATA_URL)
    }

    /// Creates a client targeting a compatible CSV endpoint, useful for mirrors and tests.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be built.
    pub fn with_endpoint(endpoint: impl Into<String>) -> Result<Self, Error> {
        let http = HttpClient::builder()
            .user_agent("situationapi-warn/0.1 (public WARN data client)")
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(Error::HttpClient)?;
        Ok(Self {
            endpoint: endpoint.into(),
            cache_dir: None,
            http,
        })
    }

    /// Sets an optional caller-owned directory for vintage-scoped downloaded CSV files.
    #[must_use]
    pub fn with_cache_dir(mut self, directory: impl AsRef<Path>) -> Self {
        self.cache_dir = Some(directory.as_ref().to_path_buf());
        self
    }

    /// Downloads and decodes all notices for the requested vintage.
    ///
    /// Cached files are keyed by the endpoint and vintage. Failed transport and 429/5xx
    /// responses are retried up to four total attempts with randomized exponential delay.
    ///
    /// # Errors
    ///
    /// Returns an error for exhausted requests, cache I/O, or invalid CSV structure.
    pub fn fetch(&self, vintage: &str) -> Result<Vec<Notice>, Error> {
        let url = self.request_url(vintage);
        let cache_file = self.cache_file(&url);
        let bytes = if let Some(path) = &cache_file {
            if let Ok(bytes) = fs::read(path) {
                bytes
            } else {
                let bytes = self.download(&url)?;
                write_cache(path, &bytes)?;
                bytes
            }
        } else {
            self.download(&url)?
        };
        decode(&bytes)
    }

    /// Reads and decodes a cached vintage without making a network request.
    ///
    /// # Errors
    ///
    /// Returns an error when no cache directory or matching file exists, or the CSV is invalid.
    pub fn fetch_offline(&self, vintage: &str) -> Result<Vec<Notice>, Error> {
        let url = self.request_url(vintage);
        let path = self.cache_file(&url).ok_or_else(|| {
            Error::Cache(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no cache directory was configured",
            ))
        })?;
        let bytes = fs::read(path).map_err(Error::Cache)?;
        decode(&bytes)
    }

    fn request_url(&self, vintage: &str) -> String {
        format!("{}?vintage={vintage}", self.endpoint)
    }

    fn cache_file(&self, url: &str) -> Option<PathBuf> {
        self.cache_dir.as_ref().map(|directory| {
            let digest = format!("{:x}", Sha256::digest(url.as_bytes()));
            directory.join(format!("{digest}.csv"))
        })
    }

    fn download(&self, url: &str) -> Result<Vec<u8>, Error> {
        let mut attempt = 0;
        loop {
            let result = self
                .http
                .get(url)
                .send()
                .and_then(reqwest::blocking::Response::error_for_status)
                .and_then(|response| response.bytes().map(|bytes| bytes.to_vec()));
            match result {
                Ok(bytes) => return Ok(bytes),
                Err(error) => {
                    attempt += 1;
                    if !retryable(&error) || attempt >= MAX_ATTEMPTS {
                        return Err(Error::Request(error));
                    }
                    retry_delay(attempt - 1);
                }
            }
        }
    }
}

fn retryable(error: &reqwest::Error) -> bool {
    error.is_timeout()
        || error.is_connect()
        || error
            .status()
            .is_some_and(|status| status.as_u16() == 429 || status.is_server_error())
}

fn retry_delay(attempt: u32) {
    let base_ms = 250_u64.saturating_mul(2_u64.pow(attempt));
    let jitter_ms = rand::thread_rng().gen_range(0..=base_ms);
    thread::sleep(Duration::from_millis(base_ms + jitter_ms));
}

fn write_cache(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let Some(parent) = path.parent() else {
        return Err(Error::CachePath);
    };
    fs::create_dir_all(parent).map_err(Error::Cache)?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temporary, bytes).map_err(Error::Cache)?;
    fs::rename(temporary, path).map_err(Error::Cache)
}

fn decode(bytes: &[u8]) -> Result<Vec<Notice>, Error> {
    let mut reader = csv::Reader::from_reader(bytes);
    reader
        .deserialize::<CsvNotice>()
        .map(|row| {
            let row = row.map_err(Error::Csv)?;
            Ok(Notice {
                state: row.state,
                company: row.company,
                location: row.location,
                workers: row
                    .employees_affected
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .parse()
                    .unwrap_or(0),
                filed_on: row
                    .notice_date
                    .as_deref()
                    .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_notice_rows_and_preserves_missing_dates() {
        let rows = decode(include_bytes!("../tests/fixtures/warn-notices.csv"))
            .expect("synthetic CSV fixture should decode");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].workers, 37);
        assert_eq!(rows[0].filed_on, NaiveDate::from_ymd_opt(2026, 11, 2));
        assert_eq!(rows[2].workers, 11);
        assert_eq!(rows[2].filed_on, None);
    }
}
