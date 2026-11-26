# situationapi-portwatch

This crate provides bounded, retrying access to IMF PortWatch daily chokepoint observations.

## Install

```toml
[dependencies]
situationapi-portwatch = { git = "https://github.com/davidawad/situationapi", package = "situationapi-portwatch" }
```

## Usage

See [`examples/basic.rs`](examples/basic.rs) for the same compiling example.

```rust,no_run
use situationapi_portwatch::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let latest = client.latest_date()?;
    let observations = client.window_data(latest)?;
    println!("{latest}: {} features", observations["features"].as_array().map_or(0, Vec::len));
    Ok(())
}
```

## Authentication and coverage

PortWatch's public ArcGIS feature service is keyless; no credential is required or read from the environment. The client queries the latest date (one feature) and requests only `Daily_Chokepoints_Data/FeatureServer/0/query` rows for the latest seven days and the seven-day comparison window 364 days earlier. It selects `date`, `portid`, `portname`, `n_total`, and `capacity`, without geometry. It does not expose a full-layer download operation.

Caching is disabled by default. Call `Client::builder().cache_dir(path)` to use a caller-owned content-addressed cache. `offline(true)` reads only from that cache. Retry policy defaults to four total attempts with full jitter on transport failures, HTTP 408, 429, and 5xx responses; `Retry-After` delta seconds are honored within the configured cap.

## Terms read 2026-11-22

| Question | Terms summary |
| --- | --- |
| Automated access allowed? | IMF terms prohibit automated bulk downloads without explicit permission. This client makes bounded one-row and date-window queries, not bulk downloads; confirm with IMF before expanding to bulk retrieval. PortWatch access: [portwatch.imf.org](https://portwatch.imf.org/). |
| Redistribution of data | IMF statistical Data may be extracted, transformed, published, and distributed subject to attribution, data integrity, transformation disclosure, and downstream notice conditions. Commercial reuse requires contacting IMF. |
| Attribution required? | Yes. Cite “International Monetary Fund, IMF PortWatch, Daily Chokepoints Data” and link the source dataset. |
| Rate limits | The IMF terms reserve the right to restrict or terminate access and prohibit disrupting or overburdening the site. No numeric request rate is stated. Use bounded queries and backoff. |

Sources: [PortWatch](https://portwatch.imf.org/) and [IMF Copyright and Usage](https://www.imf.org/en/about/copyright-and-terms), checked 2026-11-22. IMF data terms allow redistribution with conditions; the general automated bulk download restriction still applies. No upstream observations are included in this crate's fixtures.

## Tests

Offline tests use synthesized ArcGIS-layout fixtures under `tests/fixtures/`. The ignored live check queries the latest published date from the public, keyless endpoint.

