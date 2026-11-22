# situationapi-usaspending

A small blocking Rust client for retrieving and decoding selected public USAspending.gov API v2 search responses.

## Install

```toml
[dependencies]
situationapi-usaspending = { git = "https://github.com/davidawad/situationapi", package = "situationapi-usaspending" }
```

## Usage

The same request is implemented in [`examples/basic.rs`](examples/basic.rs):

```rust,no_run
use serde_json::json;
use situationapi_usaspending::{Client, Endpoint};

let client = Client::builder().cache_dir("./usaspending-cache").build()?;
let request = json!({
    "filters": {"time_period": [{"start_date": "2026-01-01", "end_date": "2026-01-07"}]},
    "limit": 10
});
let response = client.post(Endpoint::SpendingByAwardingAgency, &request)?;
println!("{}: {}", response.url, response.body);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Authentication and cache

USAspending's official endpoint documentation says these API endpoints do not currently require authorization. The client is keyless and reads no credentials or environment variables. Caching is optional; callers supply a directory with `ClientBuilder::cache_dir`, and can use `offline(true)` to serve only cached requests.

## Endpoints covered

This client posts JSON and decodes JSON for:

- `POST /api/v2/search/spending_by_category/awarding_agency/`
- `POST /api/v2/search/spending_by_award/`
- `POST /api/v2/search/spending_by_geography/`

It does not download bulk files.

Retries use exponential backoff with jitter for 429, 5xx, and transport errors. Other unsuccessful HTTP statuses are returned as errors.

## Terms (read 2026-11-22)

| Question | Finding |
| --- | --- |
| Automated access allowed? | Yes. The official API is intended for public access, and the endpoints documentation describes programmatic API access. The API documentation does not state a numeric request limit. |
| Redistribution of data | USAspending's data information page says data may be copied, adapted, redistributed, or otherwise used, subject to the D&B data limitation. Do not reuse restricted Dun & Bradstreet data outside its terms. |
| Attribution required? | No general attribution requirement is stated in the API documentation. Preserve USAspending source attribution in downstream publications as good practice. |
| Rate limits | No numeric limit is published in the official endpoint documentation. The client backs off on 429 and server errors. |

Sources: [USAspending API](https://api.usaspending.gov/), [endpoint documentation](https://api.usaspending.gov/docs/endpoints), and [data information and reuse terms](https://www.usaspending.gov/db_info). The API is a U.S. federal source.
