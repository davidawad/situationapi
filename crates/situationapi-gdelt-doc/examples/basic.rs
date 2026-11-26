use situationapi_gdelt_doc::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let url = Client::article_list_url("climate", "20250101000000", "20250101235959", 10)?;
    let response = client.fetch_json(&url)?;
    println!(
        "GDELT response keys: {}",
        response.as_object().map_or(0, serde_json::Map::len)
    );
    Ok(())
}
