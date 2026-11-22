//! A small RSS and Atom client with conditional requests and an optional local cache.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const CACHE_FLOOR_SECS: u64 = 600;
const RETRIES: usize = 3;

/// A feed entry reduced to its title, canonical link, and publication time.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Item {
    /// Entry title.
    pub title: String,
    /// Alternate link supplied by the feed.
    pub link: String,
    /// Unix timestamp in seconds, falling back to fetch time when absent.
    pub published: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct CachedFeed {
    fetched_at: u64,
    etag: Option<String>,
    last_modified: Option<String>,
    items: Vec<Item>,
}

/// Errors returned while fetching, parsing, or caching a feed.
#[derive(Debug)]
pub enum Error {
    /// The HTTP operation failed after retries.
    Http(reqwest::Error),
    /// The response body was not a supported RSS or Atom document.
    Parse(String),
    /// Cache persistence or decoding failed.
    Cache(String),
    /// Offline mode was requested but no cache entry exists.
    OfflineCacheMiss,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(error) => write!(f, "feed request failed: {error}"),
            Self::Parse(error) => write!(f, "feed parse failed: {error}"),
            Self::Cache(error) => write!(f, "feed cache failed: {error}"),
            Self::OfflineCacheMiss => f.write_str("offline cache miss"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            _ => None,
        }
    }
}

/// Fetches generic RSS and Atom documents, optionally using a caller-owned cache directory.
#[derive(Clone, Debug)]
pub struct Client {
    http: reqwest::blocking::Client,
    cache_dir: Option<PathBuf>,
    cache_floor: Duration,
}

impl Client {
    /// Creates a client with no disk cache and a descriptive default user agent.
    ///
    /// # Errors
    /// Returns an error if reqwest cannot construct its HTTP client.
    pub fn new() -> Result<Self, reqwest::Error> {
        Self::builder().build()
    }

    /// Creates a configurable client builder.
    #[must_use]
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// Fetches a feed, conditionally revalidating a stale cached response.
    ///
    /// # Errors
    /// Returns an error for failed requests, invalid feed documents, or cache I/O.
    pub fn fetch(&self, url: &str) -> Result<Vec<Item>, Error> {
        self.fetch_with_mode(url, false)
    }

    /// Reads a feed from a fresh cache entry without making a network request.
    ///
    /// # Errors
    /// Returns [`Error::OfflineCacheMiss`] when no cached result is available.
    pub fn fetch_offline(&self, url: &str) -> Result<Vec<Item>, Error> {
        self.fetch_with_mode(url, true)
    }

    fn fetch_with_mode(&self, url: &str, offline: bool) -> Result<Vec<Item>, Error> {
        let now = unix_now();
        let cache_path = self
            .cache_dir
            .as_ref()
            .map(|root| root.join(cache_key(url)));
        let previous = cache_path
            .as_deref()
            .and_then(read_cache)
            .unwrap_or_default();
        if previous.fetched_at > 0
            && now.saturating_sub(previous.fetched_at) < self.cache_floor.as_secs()
        {
            return Ok(previous.items);
        }
        if offline {
            return if previous.fetched_at > 0 {
                Ok(previous.items)
            } else {
                Err(Error::OfflineCacheMiss)
            };
        }

        let mut request = self.http.get(url);
        if let Some(etag) = &previous.etag {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        if let Some(modified) = &previous.last_modified {
            request = request.header(reqwest::header::IF_MODIFIED_SINCE, modified);
        }
        let response = send_with_retry(request)?;
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            let updated = CachedFeed {
                fetched_at: now,
                ..previous
            };
            if let Some(path) = cache_path.as_deref() {
                write_cache(path, &updated)?;
            }
            return Ok(updated.items);
        }
        let response = response.error_for_status().map_err(Error::Http)?;
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let last_modified = response
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let body = response.bytes().map_err(Error::Http)?;
        let items = parse(&body, i64::try_from(now).unwrap_or(i64::MAX))?;
        if let Some(path) = cache_path.as_deref() {
            write_cache(
                path,
                &CachedFeed {
                    fetched_at: now,
                    etag,
                    last_modified,
                    items: items.clone(),
                },
            )?;
        }
        Ok(items)
    }
}

/// Builder for [`Client`].
#[derive(Clone, Debug)]
pub struct ClientBuilder {
    cache_dir: Option<PathBuf>,
    user_agent: String,
    cache_floor: Duration,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            cache_dir: None,
            user_agent: "situationapi-feeds/0.1 (+RSS reader)".into(),
            cache_floor: Duration::from_secs(CACHE_FLOOR_SECS),
        }
    }
}

impl ClientBuilder {
    /// Sets an optional caller-owned cache directory.
    #[must_use]
    pub fn cache_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.cache_dir = Some(path.as_ref().to_owned());
        self
    }
    /// Sets the HTTP user agent.
    #[must_use]
    pub fn user_agent(mut self, value: impl Into<String>) -> Self {
        self.user_agent = value.into();
        self
    }
    /// Sets the minimum interval before a cached feed is revalidated.
    #[must_use]
    pub fn cache_floor(mut self, value: Duration) -> Self {
        self.cache_floor = value;
        self
    }
    /// Builds the configured client.
    ///
    /// # Errors
    /// Returns an error if reqwest cannot construct its HTTP client.
    pub fn build(self) -> Result<Client, reqwest::Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(&self.user_agent)
            .build()?;
        Ok(Client {
            http,
            cache_dir: self.cache_dir,
            cache_floor: self.cache_floor,
        })
    }
}

fn send_with_retry(
    mut request: reqwest::blocking::RequestBuilder,
) -> Result<reqwest::blocking::Response, Error> {
    for attempt in 0..=RETRIES {
        let retry_request = request
            .try_clone()
            .ok_or_else(|| Error::Cache("request could not be cloned for retry".into()))?;
        match retry_request.send() {
            Ok(response)
                if (response.status().as_u16() == 429 || response.status().is_server_error())
                    && attempt < RETRIES =>
            {
                let millis = jitter_millis(attempt);
                drop(response);
                thread::sleep(Duration::from_millis(millis));
                request = request
                    .try_clone()
                    .ok_or_else(|| Error::Cache("request could not be cloned for retry".into()))?;
            }
            Ok(response) => return Ok(response),
            Err(error) if attempt < RETRIES => {
                let millis = jitter_millis(attempt);
                drop(error);
                thread::sleep(Duration::from_millis(millis));
                request = request
                    .try_clone()
                    .ok_or_else(|| Error::Cache("request could not be cloned for retry".into()))?;
            }
            Err(error) => return Err(Error::Http(error)),
        }
    }
    Err(Error::Cache("retry loop exhausted".into()))
}

fn jitter_millis(attempt: usize) -> u64 {
    let base = 100_u64.saturating_mul(1_u64 << attempt.min(6));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::from(d.subsec_nanos()));
    base + nanos % base.max(1)
}

fn parse(bytes: &[u8], now: i64) -> Result<Vec<Item>, Error> {
    let feed = feed_rs::parser::parse(bytes).map_err(|error| Error::Parse(error.to_string()))?;
    Ok(feed
        .entries
        .iter()
        .filter_map(|entry| {
            let title = entry.title.as_ref()?.content.trim();
            if title.is_empty() {
                return None;
            }
            let link = entry
                .links
                .iter()
                .find(|link| link.rel.as_deref().is_none_or(|rel| rel == "alternate"))?
                .href
                .clone();
            let published = entry
                .published
                .or(entry.updated)
                .map_or(now, |date| date.timestamp());
            Some(Item {
                title: title.to_owned(),
                link,
                published,
            })
        })
        .collect())
}

fn cache_key(url: &str) -> String {
    format!("{:x}.json", Sha256::digest(url.as_bytes()))
}
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}
fn read_cache(path: &Path) -> Option<CachedFeed> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}
fn write_cache(path: &Path, cache: &CachedFeed) -> Result<(), Error> {
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| Error::Cache("cache path has no parent".into()))?,
    )
    .map_err(|error| Error::Cache(error.to_string()))?;
    let bytes = serde_json::to_vec(cache).map_err(|error| Error::Cache(error.to_string()))?;
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&tmp, bytes).map_err(|error| Error::Cache(error.to_string()))?;
    fs::rename(&tmp, path).map_err(|error| Error::Cache(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_synthesized_rss_and_atom_fixtures() {
        let rss = include_bytes!("../tests/fixtures/rss.xml");
        let atom = include_bytes!("../tests/fixtures/atom.xml");
        assert_eq!(
            parse(rss, 1).expect("RSS fixture")[0].title,
            "Synthesized RSS headline"
        );
        assert_eq!(
            parse(atom, 1).expect("Atom fixture")[0].title,
            "Synthesized Atom headline"
        );
    }

    #[test]
    fn offline_fetch_reads_the_caller_cache() {
        let directory = tempfile::tempdir().expect("temporary cache directory");
        let url = "https://example.test/synthesized-feed.xml";
        let cached = CachedFeed {
            fetched_at: unix_now(),
            etag: Some("synthetic-etag".into()),
            last_modified: None,
            items: vec![Item {
                title: "Synthesized cached headline".into(),
                link: "https://example.test/synthetic-item".into(),
                published: 1_700_000_000,
            }],
        };
        write_cache(&directory.path().join(cache_key(url)), &cached)
            .expect("write synthesized cache entry");
        let client = Client::builder()
            .cache_dir(directory.path())
            .build()
            .expect("build HTTP client");

        assert_eq!(
            client.fetch_offline(url).expect("cached result"),
            cached.items
        );
    }

    #[test]
    #[ignore = "live public RSS evidence"]
    fn live_bbc_world_feed() -> Result<(), Box<dyn std::error::Error>> {
        let items = Client::new()?.fetch("https://feeds.bbci.co.uk/news/world/rss.xml")?;
        if items.is_empty() {
            return Err(std::io::Error::other("live feed has no entries").into());
        }
        eprintln!("live items: {}", items.len());
        Ok(())
    }
}
