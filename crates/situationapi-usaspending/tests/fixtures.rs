use serde_json::Value;

#[test]
fn synthetic_responses_keep_the_upstream_json_layout() {
    let category: Value = serde_json::from_str(include_str!("fixtures/awarding_agency.json"))
        .expect("synthetic category fixture");
    let award: Value =
        serde_json::from_str(include_str!("fixtures/award.json")).expect("synthetic award fixture");
    let geography: Value = serde_json::from_str(include_str!("fixtures/geography.json"))
        .expect("synthetic geography fixture");

    assert_eq!(category["results"][0]["code"], "ZZ");
    assert_eq!(award["results"][0]["Award ID"], "SYNTHETIC-0001");
    assert_eq!(geography["results"][0]["shape_code"], "ZZ001");
}
