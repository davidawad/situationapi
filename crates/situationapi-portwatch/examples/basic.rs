use situationapi_portwatch::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let latest = client.latest_date()?;
    let observations = client.window_data(latest)?;
    println!(
        "{latest}: {} features",
        observations["features"].as_array().map_or(0, Vec::len)
    );
    Ok(())
}
