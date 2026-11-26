use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use situationapi_usaspending::{Client, Endpoint};
use std::fs;

const BASE_URL: &str = "https://api.usaspending.gov/api/v2/search";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cache = std::env::temp_dir().join(format!(
        "situationapi-usaspending-showcase-{}",
        std::process::id()
    ));
    fs::create_dir_all(&cache)?;

    let agency_request = json!({"filters": {"time_period": []}, "limit": 10});
    let award_request = json!({
        "filters": {"time_period": []},
        "fields": ["Award ID", "Recipient Name", "Award Amount"],
        "page": 1,
        "limit": 1,
        "sort": "Award Amount",
        "order": "desc"
    });
    let geography_request = json!({
        "scope": "place_of_performance",
        "geo_layer": "county",
        "filters": {"time_period": []},
        "place_of_performance_locations": []
    });
    cache_response(
        &cache,
        "spending_by_category/awarding_agency",
        &agency_request,
        include_bytes!("../tests/fixtures/awarding_agency.json"),
    )?;
    cache_response(
        &cache,
        "spending_by_award",
        &award_request,
        include_bytes!("../tests/fixtures/award.json"),
    )?;
    cache_response(
        &cache,
        "spending_by_geography",
        &geography_request,
        include_bytes!("../tests/fixtures/geography.json"),
    )?;

    let client = Client::builder()
        .base_url(BASE_URL)
        .cache_dir(&cache)
        .offline(true)
        .build()?;
    let agency = client.post(Endpoint::SpendingByAwardingAgency, &agency_request)?;
    let award = client.post(Endpoint::SpendingByAward, &award_request)?;
    let geography = client.post(Endpoint::SpendingByGeography, &geography_request)?;

    println!("USAspending synthetic sample");
    println!("{:<10} {:<22} {:>12}", "Kind", "Label", "Amount");
    println!(
        "{:<10} {:<22} {:>12.2}",
        "Agency",
        agency.body["results"][0]["code"]
            .as_str()
            .ok_or("missing agency")?,
        agency.body["results"][0]["amount"]
            .as_f64()
            .ok_or("missing amount")?
    );
    println!(
        "{:<10} {:<22} {:>12.2}",
        "Award",
        award.body["results"][0]["Award ID"]
            .as_str()
            .ok_or("missing award")?,
        award.body["results"][0]["Award Amount"]
            .as_f64()
            .ok_or("missing amount")?
    );
    println!(
        "{:<10} {:<22} {:>12.2}",
        "Geography",
        geography.body["results"][0]["shape_code"]
            .as_str()
            .ok_or("missing geography")?,
        geography.body["results"][0]["aggregated_amount"]
            .as_f64()
            .ok_or("missing amount")?
    );
    fs::remove_dir_all(cache)?;
    Ok(())
}

fn cache_response(
    cache: &std::path::Path,
    endpoint: &str,
    request: &Value,
    body: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{BASE_URL}/{endpoint}/");
    let key = format!("{url}:{request}");
    let name = format!("{:x}.json", Sha256::digest(key.as_bytes()));
    fs::write(cache.join(name), body)?;
    Ok(())
}
