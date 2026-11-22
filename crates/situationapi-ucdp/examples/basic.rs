use situationapi_ucdp::{CachePolicy, Client};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let dataset = client.fetch(CachePolicy::PreferCache)?;
    println!(
        "GED events: {}; Candidate events: {}",
        dataset.ged.len(),
        dataset.candidate.len()
    );
    Ok(())
}
