# situationapi-gdelt-doc

`situationapi-gdelt-doc` is a blocking client for GDELT DOC 2.0 article-list searches.

## Install

```toml
[dependencies]
situationapi-gdelt-doc = { git = "https://github.com/davidawad/situationapi", package = "situationapi-gdelt-doc" }
```

## Usage

The same flow is compiled from [`examples/basic.rs`](examples/basic.rs). The keyless request runs only when the example is executed.

```rust,no_run
use situationapi_gdelt_doc::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let url = Client::article_list_url("climate", "20250101000000", "20250101235959", 10)?;
    let response = client.fetch_json(&url)?;
    println!("{} response fields", response.as_object().map_or(0, |v| v.len()));
    Ok(())
}
```

Authentication and an appname are not required. The client supplies its own descriptive user agent and applies process-wide pacing and retries.

## Coverage

| Upstream endpoint or file | Supported access |
| --- | --- |
| GDELT DOC 2.0 `/doc` | Article-list mode, query, UTC time window, descending date sort, JSON format, and bounded record count |
| DOC JSON response | Blocking decode with handling for HTTP errors, rate-limit bodies, and transient retries |

## GDELT terms

Terms reviewed 2026-11-26: [GDELT Project terms of use](https://www.gdeltproject.org/about.html#termsofuse).

| Question | Terms summary |
| --- | --- |
| Automated access allowed? | Yes. GDELT describes the datasets as available for unrestricted academic, commercial, or governmental research and analysis. |
| Redistribution of data | Datasets may be redistributed, rehosted, republished, and mirrored in any form, subject to the required citation. Publisher content linked by articles remains subject to publisher terms. |
| Attribution required? | Every use or redistribution must cite the GDELT Project and link to its website. |
| Rate limits | DOC asks clients to limit requests to one every five seconds or contact the project for larger queries; this client spaces requests by six seconds. |
