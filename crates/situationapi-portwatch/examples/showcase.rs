use chrono::{Duration, NaiveDate};
use serde_json::Value;
use sha2::{Digest, Sha256};
use situationapi_portwatch::{Client, DEFAULT_LAYER_URL};
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let anchor = NaiveDate::from_ymd_opt(2036, 3, 17).ok_or("invalid fixture date")?;
    let url = window_url(anchor)?;
    let cache = std::env::temp_dir().join(format!(
        "situationapi-portwatch-showcase-{}",
        std::process::id()
    ));
    fs::create_dir_all(&cache)?;
    fs::write(
        cache.join(format!("{:x}", Sha256::digest(url.as_bytes()))),
        include_bytes!("../tests/fixtures/window.json"),
    )?;

    let observations = Client::builder()
        .cache_dir(&cache)
        .offline(true)
        .build()?
        .window_data(anchor)?;
    let features = observations
        .get("features")
        .and_then(Value::as_array)
        .ok_or("synthetic PortWatch fixture has no features")?;

    println!("PortWatch synthetic sample");
    println!(
        "{:<10} {:<18} {:>5} {:>8}",
        "Date", "Passage", "Count", "Capacity"
    );
    for feature in features.iter().take(2) {
        let attributes = feature
            .get("attributes")
            .ok_or("synthetic PortWatch feature has no attributes")?;
        println!(
            "{:<10} {:<18} {:>5} {:>8}",
            attributes["date"].as_str().ok_or("missing date")?,
            attributes["portname"].as_str().ok_or("missing passage")?,
            attributes["n_total"].as_i64().ok_or("missing count")?,
            attributes["capacity"].as_i64().ok_or("missing capacity")?
        );
    }
    fs::remove_dir_all(cache)?;
    Ok(())
}

fn window_url(anchor: NaiveDate) -> Result<String, Box<dyn std::error::Error>> {
    let prior = anchor - Duration::days(364);
    let recent_start = anchor - Duration::days(6);
    let prior_start = prior - Duration::days(6);
    let clause = format!(
        "(date >= DATE '{recent_start}' AND date <= DATE '{anchor}') OR (date >= DATE '{prior_start}' AND date <= DATE '{prior}')"
    );
    let mut url = reqwest::Url::parse(&format!("{DEFAULT_LAYER_URL}/query"))?;
    url.query_pairs_mut()
        .append_pair("where", &clause)
        .append_pair("outFields", "date,portid,portname,n_total,capacity")
        .append_pair("returnGeometry", "false")
        .append_pair("f", "json");
    Ok(url.into())
}
