use situationapi_arxiv::{Client, Config};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = Client::new(Config::default())?;
    let query = "cat:cs.AI AND submittedDate:[202601010000 TO 202601012359]";
    for page in client.query(query, 2000)? {
        println!("{}: {} entries", page.url, page.entries.len());
    }
    Ok(())
}
