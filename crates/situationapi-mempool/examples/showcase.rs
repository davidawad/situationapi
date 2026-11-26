use serde_json::Value;
use situationapi_mempool::{decode_bitcoin_page, decode_ethereum_block, TransferSummary};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bitcoin: Value =
        serde_json::from_slice(include_bytes!("../tests/fixtures/bitcoin-txs.json"))?;
    let ethereum: Value =
        serde_json::from_slice(include_bytes!("../tests/fixtures/ethereum-block.json"))?;
    let mut bitcoin_summary = TransferSummary::default();
    decode_bitcoin_page(
        bitcoin
            .as_array()
            .ok_or("synthetic Bitcoin fixture is not an array")?,
        100_000.0,
        &mut bitcoin_summary,
    )?;
    let mut ethereum_summary = TransferSummary::default();
    decode_ethereum_block(
        ethereum
            .pointer("/result/transactions")
            .and_then(Value::as_array)
            .ok_or("synthetic Ethereum fixture has no transactions")?,
        2_000.0,
        &mut ethereum_summary,
    )?;

    println!("Mempool synthetic sample");
    println!(
        "{:<8} {:>6} {:>7} {:>14}",
        "Asset", ">$1m", ">$10m", "Largest USD"
    );
    print_summary("BTC", &bitcoin_summary);
    print_summary("ETH", &ethereum_summary);
    Ok(())
}

fn print_summary(asset: &str, summary: &TransferSummary) {
    println!(
        "{asset:<8} {:>6} {:>7} {:>14.0}",
        summary.counts[0], summary.counts[1], summary.largest_usd
    );
}
