use serde_json::Value;
use situationapi_gdelt_doc::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request_url =
        Client::article_list_url("synthetic topic", "20361126000000", "20361126235959", 10)?;
    let document: Value =
        serde_json::from_slice(include_bytes!("../tests/fixtures/article-list.json"))?;
    let articles = document
        .get("articles")
        .and_then(Value::as_array)
        .ok_or("synthetic GDELT fixture has no articles")?;

    println!("GDELT synthetic sample");
    println!("{:<16} {:<34} {:>5}", "Date", "Headline", "Count");
    for article in articles.iter().take(3) {
        let date = article
            .get("seendate")
            .and_then(Value::as_str)
            .ok_or("synthetic GDELT article has no date")?;
        let title = article
            .get("title")
            .and_then(Value::as_str)
            .ok_or("synthetic GDELT article has no title")?;
        println!("{date:<16} {title:<34} {:>5}", articles.len());
    }
    if !request_url.contains("mode=artlist") {
        return Err("GDELT URL is missing its article-list mode".into());
    }
    Ok(())
}
