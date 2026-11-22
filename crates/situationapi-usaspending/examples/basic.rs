use serde_json::json;
use situationapi_usaspending::{Client, Endpoint};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::builder().cache_dir("./usaspending-cache").build()?;
    let request = json!({
        "filters": {"time_period": [{"start_date": "2026-01-01", "end_date": "2026-01-07"}]},
        "limit": 10
    });
    let response = client.post(Endpoint::SpendingByAwardingAgency, &request)?;
    println!("{}: {}", response.url, response.body);
    Ok(())
}
