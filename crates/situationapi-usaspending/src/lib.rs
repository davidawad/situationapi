//! A blocking client for selected USAspending.gov API v2 search endpoints.

use rand::Rng;
use reqwest::StatusCode;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use thiserror::Error;

const DEFAULT_BASE_URL: &str = "https://api.usaspending.gov/api/v2/search";
const MAX_ATTEMPTS: usize = 4;

/// Search endpoint supported by this client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endpoint {
    /// Spending aggregated by awarding agency.
    SpendingByAwardingAgency,
    /// Awards matching search filters.
    SpendingByAward,
    /// Spending aggregated by geography.
    SpendingByGeography,
}

impl Endpoint {
    fn path(self) -> &'static str {
        match self {
            Self::SpendingByAwardingAgency => "spending_by_category/awarding_agency",
            Self::SpendingByAward => "spending_by_award",
            Self::SpendingByGeography => "spending_by_geography",
        }
    }
}

/// A successfully decoded endpoint response and its request URL.
#[derive(Clone, Debug, PartialEq)]
pub struct Response {
    /// Decoded JSON response, preserving the upstream document structure.
    pub body: Value,
    /// Full URL used for the request.
    pub url: String,
}

/// Client errors returned while fetching or decoding API responses.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// HTTP transport failed after retries.
    #[error("USAspending request failed: {0}")]
    Transport(#[source] reqwest::Error),
    /// The server returned a non-retryable HTTP status.
    #[error("USAspending returned HTTP {status} for {url}")]
    HttpStatus { status: StatusCode, url: String },
    /// The successful response body was not valid JSON.
    #[error("USAspending returned invalid JSON: {0}")]
    Decode(#[source] serde_json::Error),
    /// A cache file could not be read or written.
    #[error("USAspending cache I/O failed: {0}")]
    Cache(#[source] std::io::Error),
    /// The offline cache did not contain this request.
    #[error("offline cache miss for {0}")]
    OfflineCacheMiss(String),
    /// A request failed after all retry attempts.
    #[error("USAspending request failed after retries: {0}")]
    RetriesExhausted(String),
}

/// Blocking, keyless `USAspending` API v2 client.
#[derive(Clone, Debug)]
pub struct Client {
    http: reqwest::blocking::Client,
    base_url: String,
    cache_dir: Option<PathBuf>,
    offline: bool,
}

impl Client {
    /// Creates a client using the public API and no cache.
    ///
    /// The API does not currently require authorization. The cache can be
    /// enabled with [`Client::with_cache_dir`].
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be configured.
    pub fn new() -> Result<Self, Error> {
        Self::builder().build()
    }

    /// Starts building a client with optional cache configuration.
    #[must_use]
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// Posts a JSON request to one of the supported API endpoints.
    ///
    /// Responses are decoded as JSON, cached by URL and request body when a
    /// cache directory is configured, and retried on transport errors, 429,
    /// and server errors.
    ///
    /// # Errors
    ///
    /// Returns an error for cache misses in offline mode, failed requests,
    /// unsuccessful HTTP responses, or invalid JSON.
    pub fn post(&self, endpoint: Endpoint, request: &Value) -> Result<Response, Error> {
        let url = format!(
            "{}/{}/",
            self.base_url.trim_end_matches('/'),
            endpoint.path()
        );
        let cache_path = self
            .cache_dir
            .as_deref()
            .map(|dir| cache_path(dir, &url, request));

        if let Some(path) = cache_path.as_deref() {
            if let Ok(bytes) = fs::read(path) {
                if let Ok(body) = serde_json::from_slice(&bytes) {
                    return Ok(Response { body, url });
                }
            }
        }
        if self.offline {
            return Err(Error::OfflineCacheMiss(url));
        }

        let bytes = self.request_with_retry(&url, request)?;
        let body = serde_json::from_slice(&bytes).map_err(Error::Decode)?;
        if let Some(path) = cache_path {
            write_cache(&path, &bytes).map_err(Error::Cache)?;
        }
        Ok(Response { body, url })
    }

    /// Retrieves spending grouped by awarding agency for the supplied filters.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or its response is invalid.
    pub fn spending_by_awarding_agency(
        &self,
        filters: &Value,
        limit: u32,
    ) -> Result<Response, Error> {
        self.post(
            Endpoint::SpendingByAwardingAgency,
            &serde_json::json!({"filters": filters, "limit": limit}),
        )
    }

    /// Searches awards with the supplied fields, sort order, and filters.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or its response is invalid.
    pub fn spending_by_award(
        &self,
        filters: &Value,
        fields: &[&str],
        page: u32,
        limit: u32,
        sort: &str,
        order: &str,
    ) -> Result<Response, Error> {
        self.post(
            Endpoint::SpendingByAward,
            &serde_json::json!({
                "filters": filters,
                "fields": fields,
                "page": page,
                "limit": limit,
                "sort": sort,
                "order": order
            }),
        )
    }

    /// Retrieves spending for a geographic layer and place-of-performance locations.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or its response is invalid.
    pub fn spending_by_geography(
        &self,
        scope: &str,
        geo_layer: &str,
        filters: &Value,
        locations: &Value,
    ) -> Result<Response, Error> {
        self.post(
            Endpoint::SpendingByGeography,
            &serde_json::json!({
                "scope": scope,
                "geo_layer": geo_layer,
                "filters": filters,
                "place_of_performance_locations": locations
            }),
        )
    }

    fn request_with_retry(&self, url: &str, request: &Value) -> Result<Vec<u8>, Error> {
        for attempt in 0..MAX_ATTEMPTS {
            let response = self.http.post(url).json(request).send();
            match response {
                Ok(response) if should_retry(response.status()) && attempt + 1 < MAX_ATTEMPTS => {
                    sleep_with_jitter(attempt);
                }
                Ok(response) if !response.status().is_success() => {
                    return Err(Error::HttpStatus {
                        status: response.status(),
                        url: url.to_owned(),
                    });
                }
                Ok(response) => {
                    return response
                        .bytes()
                        .map(|body| body.to_vec())
                        .map_err(Error::Transport)
                }
                Err(error) if attempt + 1 < MAX_ATTEMPTS => {
                    let _ = error;
                    sleep_with_jitter(attempt);
                }
                Err(error) => return Err(Error::Transport(error)),
            }
        }
        Err(Error::RetriesExhausted(url.to_owned()))
    }
}

/// Builder for a `USAspending` client.
#[derive(Clone, Debug)]
pub struct ClientBuilder {
    base_url: String,
    cache_dir: Option<PathBuf>,
    offline: bool,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_owned(),
            cache_dir: None,
            offline: false,
        }
    }
}

impl ClientBuilder {
    /// Uses an alternate API root, primarily for compatible deployments and tests.
    #[must_use]
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// Enables a caller-supplied on-disk cache directory.
    #[must_use]
    pub fn cache_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(path.into());
        self
    }

    /// Uses only cached responses and returns cache misses without network access.
    #[must_use]
    pub fn offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }

    /// Builds the configured client.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be configured.
    pub fn build(self) -> Result<Client, Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(Error::Transport)?;
        Ok(Client {
            http,
            base_url: self.base_url,
            cache_dir: self.cache_dir,
            offline: self.offline,
        })
    }
}

fn should_retry(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn sleep_with_jitter(attempt: usize) {
    let base_ms = 200_u64.saturating_mul(1_u64 << attempt.min(5));
    let jitter_ms = rand::thread_rng().gen_range(0..=base_ms);
    thread::sleep(Duration::from_millis(base_ms + jitter_ms));
}

fn cache_path(directory: &Path, url: &str, request: &Value) -> PathBuf {
    let key = format!("{url}:{request}");
    let hash = format!("{:x}", Sha256::digest(key.as_bytes()));
    directory.join(format!("{hash}.json"))
}

fn write_cache(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let Some(directory) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(directory)?;
    let temp = path.with_extension("tmp");
    fs::write(&temp, bytes)?;
    fs::rename(temp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_includes_endpoint_url_and_request_body() {
        let dir = Path::new("cache");
        let first = cache_path(dir, "https://example.test/a", &serde_json::json!({"x": 1}));
        let second = cache_path(dir, "https://example.test/a", &serde_json::json!({"x": 2}));
        assert_ne!(first, second);
    }

    #[test]
    fn endpoint_paths_match_api_v2_routes() {
        assert_eq!(
            Endpoint::SpendingByAwardingAgency.path(),
            "spending_by_category/awarding_agency"
        );
        assert_eq!(Endpoint::SpendingByAward.path(), "spending_by_award");
        assert_eq!(
            Endpoint::SpendingByGeography.path(),
            "spending_by_geography"
        );
    }
}
