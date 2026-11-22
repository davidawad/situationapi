use crate::{Error, Result, RetryPolicy};
use chrono::NaiveDate;
use reqwest::blocking::{Client as HttpClient, Response};
use reqwest::header::RETRY_AFTER;
use serde_json::Value;
use sha2::Digest;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

/// `ArcGIS` `FeatureServer` layer containing the chokepoints daily table.
pub const DEFAULT_LAYER_URL: &str = "https://services9.arcgis.com/weJ1QsnbMYJlCHdG/ArcGIS/rest/services/Daily_Chokepoints_Data/FeatureServer/0";

/// A `PortWatch` client. Each instance owns its optional cache and retry policy.
#[derive(Clone)]
pub struct Client {
    http: HttpClient,
    layer_url: String,
    cache_dir: Option<PathBuf>,
    offline: bool,
    retry: RetryPolicy,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("layer_url", &self.layer_url)
            .field("cache_dir", &self.cache_dir)
            .field("offline", &self.offline)
            .field("retry", &self.retry)
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Creates a client for the public, keyless `PortWatch` `ArcGIS` layer.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be configured.
    pub fn new() -> Result<Self> {
        Self::builder().build()
    }

    /// Starts a builder for custom cache, retry, or test endpoint settings.
    #[must_use]
    pub fn builder() -> ClientBuilder {
        ClientBuilder::default()
    }

    /// Finds the newest published feature date with a one-row query.
    ///
    /// # Errors
    /// Returns an error when the request, response, or optional cache fails.
    pub fn latest_date(&self) -> Result<NaiveDate> {
        let mut url = format!("{}/query", self.layer_url);
        url.push_str("?where=1%3D1&outFields=date&returnGeometry=false&f=json&orderByFields=date%20DESC&resultRecordCount=1");
        let response = self.get(&url)?;
        response_date(&response)
    }

    /// Fetches only the current and comparable prior-year seven-day windows.
    ///
    /// The query bounds retrieval to at most fourteen dates per chokepoint.
    ///
    /// # Errors
    /// Returns an error when date arithmetic, the request, response, or optional cache fails.
    pub fn window_data(&self, anchor: NaiveDate) -> Result<Value> {
        let prior = anchor - chrono::Duration::days(364);
        let recent_start = anchor - chrono::Duration::days(6);
        let prior_start = prior - chrono::Duration::days(6);
        let clause = format!(
            "(date >= DATE '{recent_start}' AND date <= DATE '{anchor}') OR (date >= DATE '{prior_start}' AND date <= DATE '{prior}')"
        );
        let mut url = reqwest::Url::parse(&format!("{}/query", self.layer_url))
            .map_err(|error| Error::Upstream(error.to_string()))?;
        url.query_pairs_mut()
            .append_pair("where", &clause)
            .append_pair("outFields", "date,portid,portname,n_total,capacity")
            .append_pair("returnGeometry", "false")
            .append_pair("f", "json");
        self.get(url.as_str())
    }

    fn get(&self, url: &str) -> Result<Value> {
        let cache_path = self
            .cache_dir
            .as_deref()
            .map(|directory| cache_path(directory, url));
        if let Some(path) = &cache_path {
            if self.offline {
                return Ok(serde_json::from_slice(&fs::read(path)?)?);
            }
            if let Ok(bytes) = fs::read(path) {
                if let Ok(value) = serde_json::from_slice(&bytes) {
                    return Ok(value);
                }
            }
        } else if self.offline {
            return Err(Error::Upstream(
                "offline mode requires a cache directory".into(),
            ));
        }

        let response = self.send_with_retry(url)?;
        let value: Value = response.json()?;
        if let Some(error) = value.get("error") {
            return Err(Error::Upstream(error.to_string()));
        }
        if let Some(path) = cache_path {
            write_cache(&path, &value)?;
        }
        Ok(value)
    }

    fn send_with_retry(&self, url: &str) -> Result<Response> {
        let attempts = self.retry.max_attempts.max(1);
        for attempt in 0..attempts {
            match self.http.get(url).send() {
                Ok(response) if RetryPolicy::retryable_status(response.status().as_u16()) => {
                    if attempt + 1 == attempts {
                        return Ok(response.error_for_status()?);
                    }
                    let hint = response
                        .headers()
                        .get(RETRY_AFTER)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.parse::<u64>().ok())
                        .map(Duration::from_secs);
                    thread::sleep(self.retry.delay(attempt, hint));
                }
                Ok(response) => return Ok(response.error_for_status()?),
                Err(error)
                    if attempt + 1 < attempts && (error.is_timeout() || error.is_connect()) =>
                {
                    thread::sleep(self.retry.delay(attempt, None));
                }
                Err(error) => return Err(error.into()),
            }
        }
        unreachable!("the retry loop returns on its final attempt")
    }
}

/// Builder for [`Client`].
#[derive(Debug, Clone)]
pub struct ClientBuilder {
    layer_url: String,
    cache_dir: Option<PathBuf>,
    offline: bool,
    retry: RetryPolicy,
    timeout: Duration,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            layer_url: DEFAULT_LAYER_URL.to_owned(),
            cache_dir: None,
            offline: false,
            retry: RetryPolicy::default(),
            timeout: Duration::from_secs(45),
        }
    }
}

impl ClientBuilder {
    /// Sets a caller-owned cache directory; caching is disabled by default.
    #[must_use]
    pub fn cache_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(path.into());
        self
    }

    /// Uses only cached responses. Requires [`Self::cache_dir`].
    #[must_use]
    pub fn offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }

    /// Sets the retry policy.
    #[must_use]
    pub fn retry_policy(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Sets the request timeout.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets a compatible `ArcGIS` layer root, primarily for mirrors and tests.
    #[must_use]
    pub fn layer_url(mut self, url: impl Into<String>) -> Self {
        let url: String = url.into();
        url.trim_end_matches('/').clone_into(&mut self.layer_url);
        self
    }

    /// Builds the client.
    ///
    /// # Errors
    /// Returns an error if the HTTP client cannot be configured.
    pub fn build(self) -> Result<Client> {
        let http = HttpClient::builder()
            .timeout(self.timeout)
            .user_agent(concat!(
                "situationapi-portwatch/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()?;
        Ok(Client {
            http,
            layer_url: self.layer_url,
            cache_dir: self.cache_dir,
            offline: self.offline,
            retry: self.retry,
        })
    }
}

fn response_date(response: &Value) -> Result<NaiveDate> {
    response
        .pointer("/features/0/attributes/date")
        .and_then(Value::as_str)
        .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
        .ok_or(Error::MissingDate)
}

fn cache_path(directory: &Path, url: &str) -> PathBuf {
    let name = format!("{:x}", sha2::Sha256::digest(url.as_bytes()));
    directory.join(name)
}

fn write_cache(path: &Path, value: &Value) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Upstream("cache path has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("cache"),
        std::process::id()
    ));
    fs::write(&temp, serde_json::to_vec(value)?)?;
    fs::rename(temp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{response_date, Error};
    use chrono::NaiveDate;
    use serde_json::Value;

    #[test]
    fn decodes_latest_date_using_arcgis_feature_layout() {
        let fixture: Value = serde_json::from_str(include_str!("../tests/fixtures/latest.json"))
            .expect("fixture is valid JSON");
        assert_eq!(
            response_date(&fixture).expect("fixture has a date"),
            NaiveDate::from_ymd_opt(2036, 3, 17).expect("valid date"),
            "fixture date should match the synthesized feature"
        );
    }

    #[test]
    fn rejects_missing_or_malformed_dates() {
        assert!(
            matches!(response_date(&Value::Null), Err(Error::MissingDate)),
            "missing feature list is rejected"
        );
        let invalid = serde_json::json!({"features":[{"attributes":{"date":"not-a-date"}}]});
        assert!(
            matches!(response_date(&invalid), Err(Error::MissingDate)),
            "invalid feature date is rejected"
        );
    }
}
