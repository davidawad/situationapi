//! Blocking client for querying and decoding the arXiv Atom API.

use chrono::NaiveDate;
use reqwest::blocking::Client as HttpClient;
use roxmltree::{Document, Node};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use url::form_urlencoded::Serializer;

const API: &str = "https://export.arxiv.org/api/query";

/// Settings for an arXiv client.
#[derive(Clone, Debug, Default)]
pub struct Config {
    /// Optional caller-owned directory for response caching.
    pub cache_dir: Option<PathBuf>,
    /// When true, cache misses return an error without making a request.
    pub offline: bool,
    /// User-Agent identifying the calling application.
    pub user_agent: Option<String>,
}

/// One parsed Atom entry from arXiv.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Date the paper was published to arXiv.
    pub published: NaiveDate,
    /// arXiv categories attached to the entry.
    pub categories: Vec<String>,
    /// Whitespace-normalized abstract text.
    pub summary: String,
}

/// A page returned by one API request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedPage {
    /// URL requested for this page.
    pub url: String,
    /// Parsed entries in API order.
    pub entries: Vec<Entry>,
}

/// Errors returned by the client and Atom decoder.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// HTTP transport or status failure.
    #[error("arXiv request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// Filesystem cache operation failed.
    #[error("arXiv cache error: {0}")]
    Io(#[from] std::io::Error),
    /// The API response was not a valid Atom feed.
    #[error("invalid arXiv Atom feed: {0}")]
    Xml(String),
    /// An entry omitted or malformed its publication date.
    #[error("invalid arXiv publication date: {0}")]
    Date(String),
    /// Offline mode had no matching cached response.
    #[error("offline cache miss for {0}")]
    CacheMiss(String),
    /// The bounded retry policy was exhausted unexpectedly.
    #[error("arXiv retry attempts exhausted")]
    RetriesExhausted,
    /// A zero page size cannot advance API pagination.
    #[error("page size must be greater than zero")]
    InvalidPageSize,
    /// The server returned a non-retryable or final unsuccessful HTTP status.
    #[error("arXiv returned HTTP status {0}")]
    HttpStatus(u16),
}

/// Reusable, rate-limited client for arXiv's legacy Atom query API.
pub struct Client {
    http: HttpClient,
    config: Config,
    last_request: Option<Instant>,
}

impl Client {
    /// Creates a client using the supplied cache and offline settings.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be constructed.
    pub fn new(config: Config) -> Result<Self, Error> {
        let user_agent = config
            .user_agent
            .as_deref()
            .unwrap_or("situationapi-arxiv/0.1 (arXiv API client)");
        let http = HttpClient::builder()
            .user_agent(user_agent)
            .timeout(Duration::from_secs(45))
            .build()?;
        Ok(Self {
            http,
            config,
            last_request: None,
        })
    }

    /// Queries and automatically paginates until the API returns a short page.
    ///
    /// The API is limited to one request every three seconds per caller's
    /// machines. The client serially spaces requests and retries transient
    /// transport, 429, and 5xx failures with exponential jitter.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero page size, transport or HTTP failures,
    /// malformed XML, invalid publication dates, cache I/O failures, or offline
    /// cache misses.
    pub fn query(&mut self, search_query: &str, page_size: usize) -> Result<Vec<FeedPage>, Error> {
        if page_size == 0 {
            return Err(Error::InvalidPageSize);
        }
        let mut pages = Vec::new();
        let mut start = 0;
        loop {
            let url = query_url(search_query, start, page_size);
            let body = self.fetch(&url)?;
            let entries = parse_feed(&body)?;
            let count = entries.len();
            pages.push(FeedPage { url, entries });
            if count < page_size {
                return Ok(pages);
            }
            start += count;
        }
    }

    fn fetch(&mut self, url: &str) -> Result<String, Error> {
        if let Some(cache_dir) = self.config.cache_dir.as_ref() {
            let path = cache_path(cache_dir, url);
            if let Ok(body) = fs::read_to_string(&path) {
                return Ok(body);
            }
            if self.config.offline {
                return Err(Error::CacheMiss(url.to_owned()));
            }
        } else if self.config.offline {
            return Err(Error::CacheMiss(url.to_owned()));
        }

        for attempt in 0..4_u32 {
            self.wait_for_rate_limit();
            let response = self.http.get(url).send();
            match response {
                Ok(response) if response.status().is_success() => {
                    self.last_request = Some(Instant::now());
                    let body = response.text()?;
                    self.write_cache(url, &body)?;
                    return Ok(body);
                }
                Ok(response)
                    if response.status().as_u16() != 429
                        && !response.status().is_server_error() =>
                {
                    self.last_request = Some(Instant::now());
                    return Err(Error::HttpStatus(response.status().as_u16()));
                }
                Ok(_) | Err(_) if attempt < 3 => {
                    self.last_request = Some(Instant::now());
                    thread::sleep(retry_delay(attempt));
                }
                Ok(response) => {
                    self.last_request = Some(Instant::now());
                    return Err(Error::HttpStatus(response.status().as_u16()));
                }
                Err(error) => return Err(Error::Http(error)),
            }
        }
        Err(Error::RetriesExhausted)
    }

    fn wait_for_rate_limit(&self) {
        if let Some(previous) = self.last_request {
            let wait = Duration::from_secs(3).saturating_sub(previous.elapsed());
            if !wait.is_zero() {
                thread::sleep(wait);
            }
        }
    }

    fn write_cache(&self, url: &str, body: &str) -> Result<(), Error> {
        let Some(cache_dir) = self.config.cache_dir.as_ref() else {
            return Ok(());
        };
        fs::create_dir_all(cache_dir)?;
        let path = cache_path(cache_dir, url);
        let hash = format!("{:x}", Sha256::digest(url.as_bytes()));
        let temporary = cache_dir.join(format!(".{hash}.{}.tmp", std::process::id()));
        fs::write(&temporary, body)?;
        fs::rename(temporary, path)?;
        Ok(())
    }
}

/// Builds the API URL for a single page.
#[must_use]
pub fn query_url(search_query: &str, start: usize, page_size: usize) -> String {
    let mut serializer = Serializer::new(String::new());
    serializer.append_pair("search_query", search_query);
    serializer.append_pair("start", &start.to_string());
    serializer.append_pair("max_results", &page_size.to_string());
    serializer.append_pair("sortBy", "submittedDate");
    serializer.append_pair("sortOrder", "ascending");
    format!("{API}?{}", serializer.finish())
}

/// Parses an arXiv Atom feed while retaining the upstream entry layout.
///
/// # Errors
///
/// Returns an error when the XML is malformed or an entry's publication date
/// is absent or invalid.
pub fn parse_feed(xml: &str) -> Result<Vec<Entry>, Error> {
    let document = Document::parse(xml).map_err(|error| Error::Xml(error.to_string()))?;
    document
        .descendants()
        .filter(|node| node.has_tag_name("entry"))
        .map(|entry| parse_entry(entry))
        .collect()
}

fn parse_entry(entry: Node<'_, '_>) -> Result<Entry, Error> {
    let published = child_text(entry, "published")
        .ok_or_else(|| Error::Date("entry has no published date".to_owned()))?;
    let published = published.get(..10).unwrap_or_default();
    let published = NaiveDate::parse_from_str(published, "%Y-%m-%d")
        .map_err(|error| Error::Date(error.to_string()))?;
    let categories = entry
        .children()
        .filter(|node| node.has_tag_name("category"))
        .filter_map(|node| node.attribute("term").map(str::to_owned))
        .collect();
    let summary = child_text(entry, "summary")
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    Ok(Entry {
        published,
        categories,
        summary,
    })
}

fn child_text<'a>(node: Node<'a, 'a>, tag: &str) -> Option<&'a str> {
    node.children()
        .find(|child| child.has_tag_name(tag))
        .and_then(|child| child.text())
}

fn cache_path(cache_dir: &std::path::Path, url: &str) -> PathBuf {
    let hash = format!("{:x}", Sha256::digest(url.as_bytes()));
    cache_dir.join(format!("{hash}.xml"))
}

fn retry_delay(attempt: u32) -> Duration {
    let base_ms = 500_u64.saturating_mul(2_u64.pow(attempt));
    let jitter_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_millis()
        % u32::try_from(base_ms / 2 + 1).unwrap_or(u32::MAX);
    Duration::from_millis(base_ms + u64::from(jitter_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_synthetic_atom_fixture() {
        let entries = parse_feed(include_str!("../tests/fixtures/arxiv.xml")).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].published.to_string(), "2026-11-21");
        assert_eq!(entries[0].categories, ["cs.AI", "cs.LG"]);
        assert_eq!(
            entries[0].summary,
            "Synthetic abstract for an offline parser check."
        );
    }

    #[test]
    fn query_url_encodes_search_and_pagination() {
        let url = query_url(
            "cat:cs.AI AND submittedDate:[202601010000 TO 202601312359]",
            10,
            25,
        );
        assert!(url.starts_with(API));
        assert!(url.contains("start=10"));
        assert!(url.contains("max_results=25"));
        assert!(url.contains("search_query=cat%3Acs.AI"));
    }

    #[test]
    fn offline_cache_reads_without_network() {
        let directory = tempfile::tempdir().unwrap();
        let url = query_url("all:test", 0, 5);
        fs::write(
            cache_path(directory.path(), &url),
            include_str!("../tests/fixtures/arxiv.xml"),
        )
        .unwrap();
        let mut client = Client::new(Config {
            cache_dir: Some(directory.path().to_owned()),
            offline: true,
            user_agent: None,
        })
        .unwrap();
        assert_eq!(client.fetch(&url).unwrap().matches("<entry>").count(), 2);
    }

    #[test]
    #[ignore = "requires live arXiv API access"]
    fn live_query_returns_synthetic_query_page() {
        let mut client = Client::new(Config::default()).unwrap();
        let pages = client
            .query(
                "cat:cs.AI AND submittedDate:[202601010000 TO 202601012359]",
                2000,
            )
            .unwrap();
        assert_ne!(pages.len(), 0, "live query should return at least one page");
    }
}
