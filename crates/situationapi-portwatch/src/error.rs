use thiserror::Error;

/// Errors returned by the `PortWatch` client.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// The HTTP request failed.
    #[error("portwatch request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// The response could not be decoded as JSON.
    #[error("portwatch response JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    /// The optional response cache could not be read or written.
    #[error("portwatch cache failed: {0}")]
    Io(#[from] std::io::Error),
    /// The upstream returned an `ArcGIS` error payload.
    #[error("portwatch returned an ArcGIS error: {0}")]
    Upstream(String),
    /// The latest-date response did not contain a usable date.
    #[error("portwatch latest-date response has no valid feature date")]
    MissingDate,
}

/// A result from the `PortWatch` client.
pub type Result<T> = std::result::Result<T, Error>;
