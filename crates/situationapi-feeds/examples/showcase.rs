use sha2::{Digest, Sha256};
use situationapi_feeds::Client;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = "https://example.test/synthetic-feed.xml";
    let directory = std::env::temp_dir().join(format!(
        "situationapi-feeds-showcase-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory)?;
    let filename = format!("{:x}.json", Sha256::digest(url.as_bytes()));
    fs::write(
        directory.join(filename),
        include_bytes!("../tests/fixtures/showcase-cache.json"),
    )?;
    let client = Client::builder().cache_dir(&directory).build()?;
    let items = client.fetch_offline(url)?;

    println!("Situation feeds synthetic values");
    println!("Title                     Published");
    for item in items.iter().take(2) {
        println!("{:<25.25} {}", item.title, item.published);
    }
    fs::remove_dir_all(directory)?;
    Ok(())
}
