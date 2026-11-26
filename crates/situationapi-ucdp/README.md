# situationapi-ucdp

Download and decode UCDP GED and GED Candidate event data with a reusable blocking Rust client.

## Install

```toml
[dependencies]
situationapi-ucdp = { git = "https://github.com/davidawad/situationapi", package = "situationapi-ucdp" }
```

## Usage

The same example is in [`examples/basic.rs`](examples/basic.rs). These public bulk files do not require an API key; the separate UCDP REST API requires a token in the `x-ucdp-access-token` header.

```rust,no_run
use situationapi_ucdp::{CachePolicy, Client};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?.with_cache_dir("./ucdp-cache");
    let dataset = client.fetch(CachePolicy::PreferCache)?;
    println!("GED: {} events", dataset.ged.len());
    Ok(())
}
```

`CachePolicy::Offline` reads only the optional caller-supplied cache. `Refresh` downloads fresh files, while `PreferCache` uses cached files and downloads misses. The client retries HTTP 408, 429, server errors, and transport failures with exponential full jitter and honors integer `Retry-After` values.

## Covered releases

| Data | Source | Decoding |
| --- | --- | --- |
| UCDP Georeferenced Event Dataset (GED) global 26.1 | [GED CSV archive](https://ucdp.uu.se/downloads/ged/ged261-csv.zip) | Opens `GEDEvent_v26_1.csv` in the ZIP |
| UCDP Candidate Events global 26.0.10 | [Candidate CSV](https://ucdp.uu.se/downloads/candidateged/GEDEvent_v26_0_10.csv) | Parses the upstream CSV layout |

Decoded rows retain the fields the consuming situation adapter uses: country ID and name, event end date, best fatality estimate, conflict ID, and active-year flag.

## Terms

Terms reviewed 2026-11-22 against the [UCDP API documentation](https://ucdp.uu.se/apidocs/) and [UCDP Dataset Download Center](https://ucdp.uu.se/downloads/).

| Question | Finding |
| --- | --- |
| Automated access allowed? | Yes. UCDP describes programmatic and bulk access; REST API use requires an issued token. This crate downloads the public bulk files. |
| Redistribution of data | Yes, the datasets are CC BY 4.0 and may be used and redistributed. |
| Attribution required? | Yes. Cite the publications listed with each dataset. GED 26.1 asks for Davies et al. (2026) and Sundberg & Melander (2013); Candidate asks for Hegre et al. (2020). |
| Rate limits | REST API quota is 5,000 requests per day (errors count). No limit is stated for the bulk download files. |

## Tests

Offline tests use synthesized rows with the upstream GED and Candidate CSV column layout. The ignored live test downloads both public releases.

```sh
cargo test -p situationapi-ucdp
cargo test -p situationapi-ucdp -- --ignored
```

