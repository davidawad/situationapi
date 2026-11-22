use situationapi_warn::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let vintage = std::env::args()
        .nth(1)
        .unwrap_or_else(|| chrono::Utc::now().date_naive().to_string());
    let notices = Client::new()?.fetch(&vintage)?;
    println!("Fetched {} WARN notices for {vintage}.", notices.len());
    Ok(())
}
