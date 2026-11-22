# situationapi-mempool

A reusable blocking Rust client fetches and decodes public blockchain and spot-price data for situation feeds.

## Install

```toml
[dependencies]
situationapi-mempool = { git = "https://github.com/davidawad/situationapi", package = "situationapi-mempool" }
```

## Use

This example is mirrored by [examples/basic.rs](examples/basic.rs):

```rust
use situationapi_mempool::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let (bitcoin, btc_source) = client.scan_bitcoin(1)?;
    println!("{}: {}", bitcoin.vintage, btc_source);
    let (ethereum, eth_source) = client.scan_ethereum(1)?;
    println!("{}: {}", ethereum.vintage, eth_source);
    Ok(())
}
```

The client needs no API key. Use `Client::with_cache_dir(path)` to opt into a caller-owned cache, or add `.offline(true)` to use that cache without network access. Cached responses expire after 60 seconds unless offline mode is enabled. Network errors and malformed responses are returned as `Error`; transient requests receive up to three attempts with exponential delay and jitter.

## Endpoints and data

- mempool.space: `GET /api/v1/prices`, `GET /api/v1/blocks`, and `GET /api/block/:hash/txs/:start`; scans recent Bitcoin outputs.
- PublicNode: `eth_blockNumber` and `eth_getBlockByNumber` at `https://ethereum-rpc.publicnode.com`; scans native Ethereum transaction values.
- Coinbase: `GET https://api.coinbase.com/v2/prices/ETH-USD/spot`; supplies the ETH spot price.

The Bitcoin and Ethereum decoders are public for reuse, and fixtures preserve these response shapes with synthetic values.

## Terms checked 2026-11-22

Reviewed the [mempool.space REST API documentation](https://mempool.space/docs/api/rest) and [Terms of Service](https://mempool.space/terms-of-service) on 2026-11-22. The ToS URL returned the site's application shell without readable terms text; no prohibition on read-only automated API access or this client's purpose was found in the API documentation. This client consumes public chain data and does not redistribute a source dataset.

| Question | Finding |
| --- | --- |
| Automated access allowed? | The REST API documents public endpoints and describes enforced rate limits, so automated access is supported subject to those limits. |
| Redistribution of data | The reviewed API docs and ToS page do not state a general redistribution rule. This client does not republish response bodies; output summaries are derived from public blockchain data. |
| Attribution required? | No attribution requirement was stated in the reviewed API docs or the readable ToS material. The adapter attributes its sources as a courtesy. |
| Rate limits | Enforced; numeric limits are unpublished. Excess returns HTTP 429, and repeated excess may result in a ban. |

The client honors rate limits with bounded retries for transient errors. Avoid high-volume use; repeated 429s should be treated as a stop signal. This crate also calls the public keyless [PublicNode JSON-RPC](https://ethereum-rpc.publicnode.com) and [Coinbase spot-price endpoint](https://api.coinbase.com/v2/prices/ETH-USD/spot); consult those services' own terms for their requests.
