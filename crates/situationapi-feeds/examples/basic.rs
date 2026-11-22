use situationapi_feeds::Client;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::builder()
        .cache_dir(PathBuf::from("./feed-cache"))
        .user_agent("situationapi-feeds-example/0.1 (contact: feeds@example.test)")
        .build()?;
    let items = client.fetch("https://example.test/feed.xml")?;
    for item in items {
        println!("{} {}", item.title, item.link);
    }
    Ok(())
}
