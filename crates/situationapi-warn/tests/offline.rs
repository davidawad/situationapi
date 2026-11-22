use sha2::{Digest, Sha256};
use situationapi_warn::Client;
use std::fs;

#[test]
fn fetch_offline_reads_synthetic_upstream_layout() {
    let root = std::env::temp_dir().join(format!("situationapi-warn-{}", std::process::id()));
    let url = format!("{}?vintage=2026-11-22", situationapi_warn::DATA_URL);
    let digest = format!("{:x}", Sha256::digest(url.as_bytes()));
    fs::create_dir_all(&root).expect("fixture cache directory should be created");
    fs::write(
        root.join(format!("{digest}.csv")),
        include_bytes!("fixtures/warn-notices.csv"),
    )
    .expect("fixture should be cached");

    let notices = Client::new()
        .expect("client should build")
        .with_cache_dir(&root)
        .fetch_offline("2026-11-22")
        .expect("offline fixture should decode");
    assert_eq!(notices.len(), 3, "fixture has three synthetic rows");
    assert_eq!(notices[0].company, "Example Works");
    assert_eq!(notices[0].workers, 37);
    assert_eq!(notices[2].filed_on, None);
    fs::remove_dir_all(root).expect("temporary fixture cache should be removed");
}
