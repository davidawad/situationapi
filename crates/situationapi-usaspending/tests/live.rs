use serde_json::json;
use situationapi_usaspending::{Client, Endpoint};

#[test]
#[ignore = "requires network access to USAspending.gov"]
fn live_public_endpoints_return_json_results() {
    let client = Client::new().expect("build keyless USAspending client");
    let request = json!({
        "filters": {"time_period": [{"start_date": "2026-01-01", "end_date": "2026-01-07"}]},
        "limit": 1
    });
    let response = client
        .post(Endpoint::SpendingByAwardingAgency, &request)
        .expect("call public awarding agency endpoint");
    assert!(response.body["results"].is_array(), "{}", response.body);
    println!("{} returned a JSON result array", response.url);
}
