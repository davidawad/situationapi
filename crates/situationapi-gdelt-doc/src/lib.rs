//! Blocking access to GDELT DOC 2.0 article lists.

use reqwest::StatusCode;
use serde_json::Value;
use std::fmt;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// GDELT DOC API endpoint.
pub const API_URL: &str = "https://api.gdeltproject.org/api/v2/doc/doc";
const MAX_ATTEMPTS: u32 = 2;
const MIN_REQUEST_INTERVAL: Duration = Duration::from_secs(6);
static LAST_REQUEST: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

/// Errors returned by GDELT DOC requests.
#[derive(Debug)]
pub enum Error {
    /// The URL could not be constructed.
    Url(String),
    /// The HTTP request failed.
    Http(reqwest::Error),
    /// GDELT returned a non-success status or rate limit.
    Status(StatusCode),
    /// GDELT returned its plain-text rate-limit response.
    RateLimited(String),
    /// The response was not valid JSON.
    Json(serde_json::Error),
    /// The process-wide pacing lock was poisoned.
    PacingPoisoned,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Url(error) => write!(formatter, "GDELT URL error: {error}"),
            Self::Http(error) => write!(formatter, "GDELT HTTP request failed: {error}"),
            Self::Status(status) => write!(formatter, "GDELT HTTP status: {status}"),
            Self::RateLimited(error) => write!(formatter, "GDELT rate limit: {error}"),
            Self::Json(error) => write!(formatter, "GDELT response is invalid JSON: {error}"),
            Self::PacingPoisoned => formatter.write_str("GDELT request pacing lock poisoned"),
        }
    }
}

impl std::error::Error for Error {}

/// A blocking GDELT DOC client.
#[derive(Clone, Debug)]
pub struct Client {
    http: reqwest::blocking::Client,
}

impl Client {
    /// Creates a keyless DOC client.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be initialized.
    pub fn new() -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(concat!(
                "situationapi-gdelt-doc/",
                env!("CARGO_PKG_VERSION")
            ))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(Error::Http)?;
        Ok(Self { http })
    }

    /// Builds an article-list URL for an inclusive UTC window.
    ///
    /// # Errors
    ///
    /// Returns an error when the fixed GDELT endpoint cannot be parsed.
    pub fn article_list_url(
        query: &str,
        start: &str,
        end: &str,
        max_records: usize,
    ) -> Result<String, Error> {
        let mut url =
            reqwest::Url::parse(API_URL).map_err(|error| Error::Url(error.to_string()))?;
        url.query_pairs_mut()
            .append_pair("query", query)
            .append_pair("mode", "artlist")
            .append_pair("format", "json")
            .append_pair("maxrecords", &max_records.to_string())
            .append_pair("startdatetime", start)
            .append_pair("enddatetime", end)
            .append_pair("sort", "datedesc");
        Ok(url.into())
    }

    /// Fetches and decodes one DOC JSON response.
    ///
    /// # Errors
    ///
    /// Returns an error when GDELT cannot serve or decode the response.
    pub fn fetch_json(&self, url: &str) -> Result<Value, Error> {
        let mut delay = retry_delay(0);
        let mut last_status = None;
        for attempt in 0..MAX_ATTEMPTS {
            if attempt > 0 {
                thread::sleep(delay);
                delay = retry_delay(attempt);
            }
            let _request_slot = request_slot()?;
            let response = match self.http.get(url).send() {
                Ok(response) => response,
                Err(error) if attempt + 1 < MAX_ATTEMPTS => {
                    last_status = Some(Error::Http(error));
                    continue;
                }
                Err(error) => return Err(Error::Http(error)),
            };
            let status = response.status();
            let body = response.bytes().map_err(Error::Http)?;
            let rate_limited = status == StatusCode::TOO_MANY_REQUESTS || is_rate_limit_body(&body);
            if (rate_limited || status.is_server_error()) && attempt + 1 < MAX_ATTEMPTS {
                last_status = Some(if rate_limited {
                    Error::RateLimited(response_preview(&body))
                } else {
                    Error::Status(status)
                });
                continue;
            }
            if rate_limited {
                return Err(Error::RateLimited(response_preview(&body)));
            }
            if !status.is_success() {
                return Err(Error::Status(status));
            }
            return serde_json::from_slice(&body).map_err(Error::Json);
        }
        Err(last_status.unwrap_or(Error::Status(StatusCode::SERVICE_UNAVAILABLE)))
    }
}

fn is_rate_limit_body(body: &[u8]) -> bool {
    let body = String::from_utf8_lossy(body).to_ascii_lowercase();
    body.contains("please limit requests to one every 5 seconds")
        || body.contains("please limit requests to one every five seconds")
}

fn response_preview(body: &[u8]) -> String {
    String::from_utf8_lossy(body).chars().take(180).collect()
}

fn request_slot() -> Result<MutexGuard<'static, Option<Instant>>, Error> {
    let mut last = LAST_REQUEST
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| Error::PacingPoisoned)?;
    if let Some(previous) = *last {
        if let Some(wait) = MIN_REQUEST_INTERVAL.checked_sub(previous.elapsed()) {
            thread::sleep(wait);
        }
    }
    *last = Some(Instant::now());
    Ok(last)
}

fn retry_delay(attempt: u32) -> Duration {
    let base = Duration::from_secs(5_u64 * (1_u64 << attempt.min(3)));
    let jitter = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    base + Duration::from_millis(u64::from(jitter) % if attempt == 0 { 3_001 } else { 1_001 })
}
