use situationapi_portwatch::Client;

#[test]
#[ignore = "live public IMF PortWatch query"]
fn latest_date_is_available() -> Result<(), Box<dyn std::error::Error>> {
    let date = Client::new()?.latest_date()?;
    println!("PortWatch latest date: {date}");
    Ok(())
}
