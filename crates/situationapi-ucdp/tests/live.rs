use situationapi_ucdp::{CachePolicy, Client};

#[test]
#[ignore = "downloads the current public UCDP GED and Candidate bulk releases"]
fn current_bulk_releases_decode() {
    let dataset = Client::new()
        .expect("client builds")
        .fetch(CachePolicy::Refresh)
        .expect("UCDP releases download and decode");
    assert_ne!(dataset.ged.len(), 0);
    assert_ne!(dataset.candidate.len(), 0);
    eprintln!(
        "LIVE_EVIDENCE GED events={} Candidate events={}",
        dataset.ged.len(),
        dataset.candidate.len()
    );
}
