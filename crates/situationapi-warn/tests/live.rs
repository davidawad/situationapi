#[test]
#[ignore = "requests the public WARN Feed endpoint"]
fn live_feed_has_decodable_rows() {
    let vintage = chrono::Utc::now().date_naive().to_string();
    let notices = situationapi_warn::Client::new()
        .expect("client should build")
        .fetch(&vintage)
        .expect("public WARN Feed should respond");
    eprintln!("LIVE_WARN_FEED row_count={}", notices.len());
    assert!(!notices.is_empty(), "live WARN Feed should contain notices");
}
