//! Reusable blocking clients for mempool.space large-transfer queries.
use reqwest::blocking::Client as HttpClient;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const BTC_API: &str = "https://mempool.space/api";
const BTC_PRICE: &str = "https://mempool.space/api/v1/prices";
const ETH_RPC: &str = "https://ethereum-rpc.publicnode.com";
const ETH_PRICE: &str = "https://api.coinbase.com/v2/prices/ETH-USD/spot";
const CACHE_TTL: u64 = 60;

/// A source request or response decoding failure.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// HTTP client initialization failed.
    #[error("could not build HTTP client: {0}")]
    HttpClient(#[source] reqwest::Error),
    /// Network or cache operation failed.
    #[error("source request failed: {0}")]
    Request(String),
    /// The upstream response did not match its documented shape.
    #[error("invalid source response: {0}")]
    Decode(String),
}

/// Transfer totals at two caller-chosen thresholds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TransferSummary {
    /// Counts for values over $1m and $10m USD.
    pub counts: [u64; 2],
    /// USD totals for values over $1m and $10m USD.
    pub totals_usd: [f64; 2],
    /// USD value of the largest output or transaction.
    pub largest_usd: f64,
    /// Native amount of the largest output or transaction.
    pub largest_amount: f64,
    /// Native unit of the largest transfer.
    pub largest_unit: &'static str,
    /// Identifier of the largest transfer.
    pub largest_txid: String,
    /// Describes the fetched source window.
    pub vintage: String,
}

impl TransferSummary {
    fn add(&mut self, amount: f64, unit: &'static str, price: f64, txid: &str) {
        let usd = amount * price;
        for (index, threshold) in [1_000_000.0, 10_000_000.0].iter().enumerate() {
            if usd > *threshold {
                self.counts[index] += 1;
                self.totals_usd[index] += usd;
            }
        }
        if usd > self.largest_usd {
            self.largest_usd = usd;
            self.largest_amount = amount;
            self.largest_unit = unit;
            txid.clone_into(&mut self.largest_txid);
        }
    }
}

/// Blocking access to mempool.space, public Ethereum JSON-RPC, and Coinbase spot prices.
/// Persistent caching is optional and its directory is always supplied by the caller.
#[derive(Debug)]
pub struct Client {
    http: HttpClient,
    cache_dir: Option<PathBuf>,
    offline: bool,
}

impl Client {
    /// Create a client with no persistent cache.
    ///
    /// # Errors
    /// Returns an error if the TLS-backed HTTP client cannot be initialized.
    pub fn new() -> Result<Self, Error> {
        let http = HttpClient::builder()
            .timeout(Duration::from_secs(60))
            .user_agent(concat!("situationapi-mempool/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(Error::HttpClient)?;
        Ok(Self {
            http,
            cache_dir: None,
            offline: false,
        })
    }

    /// Cache responses under the caller-provided directory.
    #[must_use]
    pub fn with_cache_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.cache_dir = Some(path.as_ref().to_path_buf());
        self
    }

    /// Use only cached responses; requires a configured cache directory.
    #[must_use]
    pub fn offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }

    /// Fetch recent Bitcoin blocks, their transaction pages, and the BTC/USD price.
    ///
    /// # Errors
    /// Returns an error when a request fails or an upstream response cannot be decoded.
    pub fn scan_bitcoin(&self, block_count: usize) -> Result<(TransferSummary, String), Error> {
        let price_doc = self.get_json(BTC_PRICE)?;
        let price = price_doc
            .get("USD")
            .and_then(Value::as_f64)
            .filter(|n| *n > 0.0)
            .ok_or_else(|| {
                Error::Decode("mempool.space response has no positive BTC USD price".into())
            })?;
        let blocks_doc = self.get_json(&format!("{BTC_API}/v1/blocks"))?;
        let blocks = blocks_doc
            .as_array()
            .ok_or_else(|| Error::Decode("Bitcoin block response is not an array".into()))?;
        let count = block_count.max(1).min(blocks.len());
        let mut summary = TransferSummary {
            vintage: "Bitcoin blocks".into(),
            ..TransferSummary::default()
        };
        let mut evidence_url = BTC_API.to_owned();
        for block in blocks.iter().take(count) {
            let hash = field_str(block, "id", "Bitcoin block has no id")?;
            let height = field_u64(block, "height", "Bitcoin block has no height")?;
            let tx_count = field_u64(block, "tx_count", "Bitcoin block has no tx_count")?;
            summary.vintage = format!("Bitcoin block height {height} (latest {count} blocks)");
            let mut start = 0_u64;
            while start < tx_count {
                let url = format!("{BTC_API}/block/{hash}/txs/{start}");
                evidence_url.clone_from(&url);
                let page = self.get_json(&url)?;
                let txs = page.as_array().ok_or_else(|| {
                    Error::Decode("Bitcoin transaction page is not an array".into())
                })?;
                if txs.is_empty() {
                    break;
                }
                decode_bitcoin_page(txs, price, &mut summary)?;
                start +=
                    u64::try_from(txs.len()).map_err(|error| Error::Decode(error.to_string()))?;
            }
        }
        Ok((summary, evidence_url))
    }

    /// Fetch recent Ethereum native transactions, their ETH/USD price, and aggregate values.
    ///
    /// # Errors
    /// Returns an error when a request fails or an upstream response cannot be decoded.
    pub fn scan_ethereum(&self, block_count: u64) -> Result<(TransferSummary, String), Error> {
        let price_doc = self.get_json(ETH_PRICE)?;
        let price = price_doc
            .pointer("/data/amount")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Decode("Coinbase ETH spot response has no amount".into()))?
            .parse::<f64>()
            .map_err(|error| Error::Decode(error.to_string()))?;
        if price <= 0.0 {
            return Err(Error::Decode(
                "Coinbase ETH spot price is not positive".into(),
            ));
        }
        let latest = hex_u64(
            self.rpc("eth_blockNumber", &json!([]))?
                .get("result")
                .ok_or_else(|| Error::Decode("Ethereum RPC has no block number".into()))?,
        )?;
        let count = block_count.max(1);
        let first = latest.saturating_sub(count - 1);
        let mut summary = TransferSummary {
            vintage: format!("Ethereum blocks {first}/{latest}"),
            ..TransferSummary::default()
        };
        let mut evidence_url = ETH_RPC.to_owned();
        for height in first..=latest {
            let tag = format!("0x{height:x}");
            let block = self.rpc("eth_getBlockByNumber", &json!([tag, true]))?;
            evidence_url = format!("{ETH_RPC} eth_getBlockByNumber({tag}, true)");
            let txs = block
                .pointer("/result/transactions")
                .and_then(Value::as_array)
                .ok_or_else(|| Error::Decode("Ethereum block has no transaction list".into()))?;
            decode_ethereum_block(txs, price, &mut summary)?;
        }
        Ok((summary, evidence_url))
    }

    /// Fetch a GET endpoint and decode its JSON response.
    ///
    /// # Errors
    /// Returns an error when the request fails or the response is not valid JSON.
    pub fn get_json(&self, url: &str) -> Result<Value, Error> {
        let key = format!("GET {url}");
        if let Some(bytes) = self.read_cache(&key) {
            return parse_json(&bytes);
        }
        if self.offline {
            return Err(Error::Request(format!("offline cache miss for {url}")));
        }
        let response = Self::retry(|| {
            let response = self
                .http
                .get(url)
                .send()
                .map_err(|error| (error.to_string(), true))?;
            let status = response.status();
            if status.is_success() {
                Ok(response)
            } else {
                Err((
                    format!("HTTP status {status}"),
                    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error(),
                ))
            }
        })?;
        let bytes = response
            .bytes()
            .map_err(|error| Error::Request(error.to_string()))?
            .to_vec();
        self.write_cache(&key, &bytes)?;
        parse_json(&bytes)
    }

    fn rpc(&self, method: &str, params: &Value) -> Result<Value, Error> {
        let payload = json!({"jsonrpc":"2.0","method":method,"params":params,"id":1});
        let key = format!("POST {ETH_RPC} {payload}");
        if let Some(bytes) = self.read_cache(&key) {
            return parse_json(&bytes);
        }
        if self.offline {
            return Err(Error::Request(format!("offline cache miss for {key}")));
        }
        let response = Self::retry(|| {
            let response = self
                .http
                .post(ETH_RPC)
                .json(&payload)
                .send()
                .map_err(|error| (error.to_string(), true))?;
            let status = response.status();
            if status.is_success() {
                Ok(response)
            } else {
                Err((
                    format!("HTTP status {status}"),
                    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error(),
                ))
            }
        })?;
        let doc: Value = response
            .json()
            .map_err(|error| Error::Request(error.to_string()))?;
        if let Some(error) = doc.get("error") {
            return Err(Error::Request(format!("Ethereum RPC {method}: {error}")));
        }
        let bytes = serde_json::to_vec(&doc).map_err(|error| Error::Decode(error.to_string()))?;
        self.write_cache(&key, &bytes)?;
        Ok(doc)
    }

    fn retry<T>(mut request: impl FnMut() -> Result<T, (String, bool)>) -> Result<T, Error> {
        let mut last_error = String::new();
        for attempt in 0_u32..3 {
            match request() {
                Ok(value) => return Ok(value),
                Err((error, retryable)) => {
                    last_error = error;
                    if !retryable {
                        break;
                    }
                }
            }
            if attempt < 2 {
                let jitter = (now_secs() + u64::from(attempt) * 37) % 75;
                thread::sleep(Duration::from_millis(100 * (1_u64 << attempt) + jitter));
            }
        }
        Err(Error::Request(last_error))
    }

    fn read_cache(&self, key: &str) -> Option<Vec<u8>> {
        let Some(root) = &self.cache_dir else {
            return None;
        };
        let name = digest(key.as_bytes());
        let Ok(bytes) = fs::read(root.join(&name)) else {
            return None;
        };
        let fetched = fs::read_to_string(root.join(format!("{name}.fetched")))
            .ok()
            .and_then(|value| value.parse::<u64>().ok());
        if self.offline || fetched.is_some_and(|at| now_secs().saturating_sub(at) <= CACHE_TTL) {
            Some(bytes)
        } else {
            None
        }
    }

    fn write_cache(&self, key: &str, bytes: &[u8]) -> Result<(), Error> {
        let Some(root) = &self.cache_dir else {
            return Ok(());
        };
        fs::create_dir_all(root).map_err(|error| Error::Request(error.to_string()))?;
        let name = digest(key.as_bytes());
        let body = root.join(&name);
        let temp = root.join(format!(".{name}.{}.tmp", std::process::id()));
        fs::write(&temp, bytes).map_err(|error| Error::Request(error.to_string()))?;
        fs::rename(temp, body).map_err(|error| Error::Request(error.to_string()))?;
        fs::write(root.join(format!("{name}.fetched")), now_secs().to_string())
            .map_err(|error| Error::Request(error.to_string()))
    }
}

/// Decode a mempool.space transaction page.
///
/// # Errors
/// Returns an error when a transaction has no identifier.
pub fn decode_bitcoin_page(
    txs: &[Value],
    price: f64,
    summary: &mut TransferSummary,
) -> Result<(), Error> {
    for tx in txs {
        let txid = field_str(tx, "txid", "Bitcoin transaction has no txid")?;
        if let Some(outputs) = tx.get("vout").and_then(Value::as_array) {
            for output in outputs {
                if let Some(sats) = output.get("value").and_then(Value::as_u64) {
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "Bitcoin supply is below f64's exact integer range"
                    )]
                    let amount = sats as f64 / 100_000_000.0;
                    summary.add(amount, "BTC", price, txid);
                }
            }
        }
    }
    Ok(())
}

/// Decode an Ethereum JSON-RPC transaction list.
///
/// # Errors
/// Returns an error when a transaction is missing its value or identifier.
pub fn decode_ethereum_block(
    txs: &[Value],
    price: f64,
    summary: &mut TransferSummary,
) -> Result<(), Error> {
    for tx in txs {
        let amount = hex_eth(
            tx.get("value")
                .ok_or_else(|| Error::Decode("Ethereum transaction has no value".into()))?,
        )?;
        let txid = field_str(tx, "hash", "Ethereum transaction has no hash")?;
        summary.add(amount, "ETH", price, txid);
    }
    Ok(())
}

fn field_str<'a>(value: &'a Value, key: &str, message: &str) -> Result<&'a str, Error> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Decode(message.into()))
}
fn field_u64(value: &Value, key: &str, message: &str) -> Result<u64, Error> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| Error::Decode(message.into()))
}
fn hex_u64(value: &Value) -> Result<u64, Error> {
    let raw = value
        .as_str()
        .ok_or_else(|| Error::Decode("Ethereum quantity is not hex text".into()))?;
    u64::from_str_radix(raw.strip_prefix("0x").unwrap_or(raw), 16)
        .map_err(|error| Error::Decode(error.to_string()))
}
fn hex_eth(value: &Value) -> Result<f64, Error> {
    let raw = value
        .as_str()
        .ok_or_else(|| Error::Decode("Ethereum quantity is not hex text".into()))?;
    let wei = raw
        .strip_prefix("0x")
        .unwrap_or(raw)
        .chars()
        .try_fold(0.0, |total, digit| {
            digit
                .to_digit(16)
                .map(|nibble| total * 16.0 + f64::from(nibble))
        })
        .ok_or_else(|| Error::Decode("Ethereum quantity contains a non-hex digit".into()))?;
    Ok(wei / 1e18)
}
fn parse_json(body: &[u8]) -> Result<Value, Error> {
    serde_json::from_slice(body).map_err(|error| Error::Decode(error.to_string()))
}
fn digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut out, byte| {
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 0x0f)]));
            out
        })
}
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decodes_synthetic_bitcoin_fixture() {
        let txs: Value = serde_json::from_str(include_str!("../tests/fixtures/bitcoin-txs.json"))
            .expect("fixture parses");
        let mut summary = TransferSummary::default();
        decode_bitcoin_page(
            txs.as_array().expect("fixture is an array"),
            100_000.0,
            &mut summary,
        )
        .expect("page decodes");
        assert_eq!(summary.counts, [2, 1]);
        assert_eq!(summary.largest_txid, "fedcba9876543210");
    }
    #[test]
    fn decodes_synthetic_ethereum_fixture() {
        let doc: Value =
            serde_json::from_str(include_str!("../tests/fixtures/ethereum-block.json"))
                .expect("fixture parses");
        let txs = doc
            .pointer("/result/transactions")
            .and_then(Value::as_array)
            .expect("fixture transaction list");
        let mut summary = TransferSummary::default();
        decode_ethereum_block(txs, 2_000.0, &mut summary).expect("block decodes");
        assert_eq!(summary.counts, [2, 1]);
        assert_eq!(summary.largest_txid, "0xaaaaaaaaaaaaaaaa");
    }
    #[test]
    #[ignore = "live unauthenticated mempool.space API smoke test"]
    fn live_mempool_api_returns_blocks() {
        let client = Client::new().expect("client builds");
        let rows = client
            .get_json(&format!("{BTC_API}/v1/blocks"))
            .expect("mempool.space responds");
        assert!(rows.as_array().is_some_and(|blocks| !blocks.is_empty()));
    }
}
