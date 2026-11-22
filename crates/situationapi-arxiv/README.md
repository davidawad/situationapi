# situationapi-arxiv

This crate provides a reusable blocking Rust client for querying and decoding arXiv's Atom API.

## Install

```toml
[dependencies]
situationapi-arxiv = { git = "https://github.com/davidawad/situationapi", package = "situationapi-arxiv" }
```

## Usage

The same example is in [`examples/basic.rs`](examples/basic.rs).

```rust,no_run
use situationapi_arxiv::{Client, Config};

let mut client = Client::new(Config::default())?;
let query = "cat:cs.AI AND submittedDate:[202601010000 TO 202601012359]";
for page in client.query(query, 2000)? {
    println!("{} entries from {}", page.entries.len(), page.url);
}
# Ok::<(), situationapi_arxiv::Error>(())
```

Authentication is not required by the arXiv API. The client sends a descriptive
User-Agent; callers may set it in `Config`. `Config::cache_dir` is an optional,
caller-owned response cache. When set, responses are keyed by request URL and
committed atomically. `Config::offline` returns an error on a cache miss.

## Coverage

The client covers the legacy query endpoint, `https://export.arxiv.org/api/query`:
search query construction, sorted pagination, Atom feed decoding, URL-keyed cache,
rate limiting, and retry for transport failures, HTTP 429, and HTTP 5xx responses.
It returns each page's URL and entries, including publication date, categories,
and abstract summary. It does not fetch PDFs or source archives.

## arXiv API terms

Terms reviewed 2026-11-22: [Terms of Use for arXiv APIs](https://info.arxiv.org/help/api/tou.html).

| Question | Terms and client behavior |
| --- | --- |
| Automated access allowed? | Yes. arXiv supports API-powered discovery services; this client spaces requests at least three seconds apart and uses one connection at a time. |
| Redistribution of data | Descriptive metadata is available under CC0. E-prints remain copyright-protected; redistribution requires permission. This client returns metadata and abstracts to the caller and does not serve paper files. |
| Attribution required? | The terms encourage linking users to arXiv. Callers should attribute arXiv and preserve source URLs; arXiv requests acknowledgment for use of its open access interoperability. |
| Rate limits | No more than one legacy API request every three seconds across all machines controlled by the caller, with a single connection at a time. The client serializes and spaces its own requests. |
