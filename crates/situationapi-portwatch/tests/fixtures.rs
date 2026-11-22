use serde_json::Value;

#[test]
fn synthesized_fixture_preserves_arcgis_feature_layout() {
    let response: Value =
        serde_json::from_str(include_str!("fixtures/window.json")).expect("fixture is valid JSON");
    let attributes = &response["features"][0]["attributes"];
    assert_eq!(attributes["portid"], "chokepoint1", "passage id field");
    assert_eq!(attributes["n_total"], 14, "transit count field");
    assert_eq!(attributes["capacity"], 100, "capacity field");
}
