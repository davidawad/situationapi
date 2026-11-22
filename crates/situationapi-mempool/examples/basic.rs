use situationapi_mempool::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new()?;
    let (bitcoin, btc_source) = client.scan_bitcoin(1)?;
    println!("{}: {}", bitcoin.vintage, btc_source);
    let (ethereum, eth_source) = client.scan_ethereum(1)?;
    println!("{}: {}", ethereum.vintage, eth_source);
    Ok(())
}
