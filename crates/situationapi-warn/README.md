# situationapi-warn

A reusable Rust client fetches and decodes the public WARN Feed notices CSV with retries and an optional caller-owned cache.

## Install

```toml
[dependencies]
situationapi-warn = { git = "https://github.com/davidawad/situationapi", package = "situationapi-warn" }
```

## Usage

```rust,no_run
use situationapi_warn::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?.with_cache_dir("./cache/warn");
    let notices = client.fetch("2026-11-22")?;
    println!("{} notices", notices.len());
    Ok(())
}
```

The same call is in [`examples/basic.rs`](examples/basic.rs). Pass the vintage as a date string; the client adds it to the request URL and uses it as part of the cache key. `fetch_offline` reads only a previously cached vintage. Cache storage is disabled unless the caller supplies a directory. This source is keyless and needs no API key or environment variable.

## Endpoints and returned data

The client reads `data/warn_notices.csv` from [WARN Feed](https://github.com/APVentureEngine/warn-act-notices), a consolidated daily CSV derived from state labor department WARN listings. It decodes state, employer, location, affected workers, and notice date. Missing or invalid notice dates remain `None`; invalid or blank worker counts become zero. Each state agency publishes a different page or file layout, so this client uses the consolidated feed rather than making 48 direct state requests.

## Terms checked 2026-11-22

The underlying official listings are public records, but state-specific page terms differ and do not provide one shared license. The client accesses the consolidated feed, whose publisher expressly licenses the `data/` dataset under CC BY 4.0. The linked primary pages below identify representative source agencies and their published access pages; they are not independently scraped by this client.

| Source / terms | Automated access allowed? | Redistribution | Attribution | Rate limits |
| --- | --- | --- | --- | --- |
| [WARN Feed API and data license](https://github.com/APVentureEngine/warn-act-notices/blob/main/README.md) | Yes; stable raw CSV, no login or API key is required. The publisher documents daily refreshes, not a request quota. | Yes under CC BY 4.0 for the consolidated dataset. | Required: credit “WARN Feed” and link to its repository. | No numeric limit published; this client caches by vintage and retries transient errors with bounded jitter. |
| [California EDD WARN page](https://edd.ca.gov/en/jobs_and_training/Layoff_Services_WARN/) | Not accessed directly; page lists reports, with no general automated-access rule stated there. | No separate data license stated on the cited page. | No page-level attribution rule stated. | No page-level request limit stated. |
| [New York DOL WARN dashboard](https://dol.ny.gov/warn-dashboard) | Not accessed directly; dashboard access conditions apply to direct use. | No separate data license stated on the cited page. | No page-level attribution rule stated. | No page-level request limit stated. |
| [Texas Workforce Commission WARN dataset](https://data.texas.gov/dataset/Worker-Adjustment-and-Retraining-Notification-WARN/8w53-c4f6) | Not accessed directly; the state open-data page exposes its dataset record. | No terms are relied on for the consolidated CSV. | No page-level attribution rule stated. | No page-level request limit stated. |
| [New Jersey DOL WARN page](https://www.nj.gov/labor/employer-services/warn/index.shtml) | Not accessed directly; page posts state WARN information. | No separate data license stated on the cited page. | No page-level attribution rule stated. | No page-level request limit stated. |
| [Washington ESD WARN page](https://esd.wa.gov/employer-requirements/layoffs-and-employee-notifications/worker-adjustment-and-retraining-notification-warn-layoff-and-closure-database) | Not accessed directly; database page is an official listing. | No separate data license stated on the cited page. | No page-level attribution rule stated. | No page-level request limit stated. |

The WARN Feed README says its source records originate from state labor department listings, and its dataset license permits use for any purpose with credit. It also says the upstream is unaffiliated with the agencies and recommends checking original records for consequential decisions. The source collector is Apache-2.0, but this client does not incorporate its software. Terms were checked on 2026-11-22; re-check them before changing the endpoint or fetching state sites directly.
