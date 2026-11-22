use std::collections::BTreeMap;

use situationapi_ucdp::{decode_candidate_csv, decode_ged_zip, Event};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ged = decode_ged_zip(include_bytes!("../tests/fixtures/ged/ged261-csv.zip"))?;
    let candidate = decode_candidate_csv(include_bytes!(
        "../tests/fixtures/candidate/GEDEvent_v26_0_10.csv"
    ))?;
    let mut countries: BTreeMap<String, (usize, u64)> = BTreeMap::new();

    for event in ged.iter().chain(&candidate) {
        add_event(&mut countries, event);
    }

    println!("UCDP synthetic sample");
    println!("{:<24} {:>8} {:>12}", "Country", "Events", "Fatalities");
    for (country, (events, fatalities)) in countries.iter().take(16) {
        println!("{country:<24} {events:>8} {fatalities:>12}");
    }
    Ok(())
}

fn add_event(countries: &mut BTreeMap<String, (usize, u64)>, event: &Event) {
    let country = event
        .country
        .as_deref()
        .filter(|country| !country.is_empty())
        .unwrap_or(&event.country_id);
    let totals = countries.entry(country.to_owned()).or_default();
    totals.0 += 1;
    totals.1 += event.best;
}
