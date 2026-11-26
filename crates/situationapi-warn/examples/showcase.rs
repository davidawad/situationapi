use sha2::{Digest, Sha256};
use situationapi_warn::{Client, Notice, DATA_URL};
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let vintage = "2026-11-22";
    let url = format!("{DATA_URL}?vintage={vintage}");
    let cache =
        std::env::temp_dir().join(format!("situationapi-warn-showcase-{}", std::process::id()));
    fs::create_dir_all(&cache)?;
    fs::write(
        cache.join(format!("{:x}.csv", Sha256::digest(url.as_bytes()))),
        include_bytes!("../tests/fixtures/warn-notices.csv"),
    )?;
    let notices = Client::new()?
        .with_cache_dir(&cache)
        .fetch_offline(vintage)?;

    println!("WARN synthetic sample");
    println!("{:<6} {:<22} {:>7}", "State", "Company", "Workers");
    for notice in notices.iter().take(3) {
        print_notice(notice);
    }
    fs::remove_dir_all(cache)?;
    Ok(())
}

fn print_notice(notice: &Notice) {
    println!(
        "{:<6} {:<22} {:>7}",
        notice.state, notice.company, notice.workers
    );
}
