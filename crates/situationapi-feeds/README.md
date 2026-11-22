# situationapi-feeds

Fetch and parse generic RSS and Atom feeds with conditional HTTP requests and an optional caller-owned cache.

## Install

```toml
situationapi-feeds = { git = "https://github.com/davidawad/situationapi", package = "situationapi-feeds" }
```

## Usage

```rust,no_run
use situationapi_feeds::Client;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::builder()
        .cache_dir(PathBuf::from("./feed-cache"))
        .user_agent("my-feed-reader/1.0 (contact: feeds@example.test)")
        .build()?;
    let entries = client.fetch("https://example.test/feed.xml")?;
    for entry in entries {
        println!("{} {}", entry.title, entry.link);
    }
    Ok(())
}
```

The same example is in [`examples/basic.rs`](examples/basic.rs). No API key is needed for ordinary RSS or Atom feeds. If a publisher protects a feed, provide credentials using its documented URL or request requirements; this crate does not read credentials from the environment. `fetch_offline(url)` serves a fresh cached result or reports a cache miss without network access.

## Coverage

The client fetches generic RSS 2.0 and Atom documents by HTTP GET. It parses titles, alternate links, and publication/update timestamps, and supports ETag and Last-Modified conditional requests. It does not include publisher catalogs or publisher-specific endpoints; it ships no feed catalog.

## Terms

Feed terms belong to each publisher. Check the feed's current terms before use. The client only implements fetching and parsing; it does not grant rights to feed contents.

| Question | Finding |
| --- | --- |
| Automated access allowed? | Publisher-specific. BBC publishes RSS feeds for use subject to its terms; those terms prohibit scraping outside the feed and require permission for business use. Le Monde reserves its feeds for personal, non-professional, non-collective use. |
| Redistribution of data | Not granted by this client. Publisher-specific; treat fetched content as non-redistributable absent permission. |
| Attribution required? | Preserve publisher attribution and links; BBC RSS use requires attribution and a link to BBC News. |
| Rate limits | No common RSS standard rate limit. Follow each publisher's limits and caching guidance; this client conditionally revalidates and defaults to a ten-minute cache floor. |

Terms reviewed 2026-11-22: [BBC Terms of Use, metadata and RSS feeds](https://www.bbc.co.uk/usingthebbc/terms-of-use/), [BBC News Feeds](https://operations.bbc.co.uk/platform/feeds/NewsFeeds.htm), and [Le Monde RSS terms](https://www.lemonde.fr/en/about-us/article/2026/03/27/le-monde-rss-feeds_6751860_115.html). The generic client is suitable only where the chosen publisher permits the intended access and display. Any feed catalog must be checked against each publisher's terms before operational use.
