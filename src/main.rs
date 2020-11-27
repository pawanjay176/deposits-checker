use deposits_script::*;
use std::ops::Range;
use std::time::Duration;

const START_BLOCK: u64 = 3743587;
const DEPOSIT_CONTRACT: &'static str = "0x8c5fecdC472E27Bc447696F431E425D02dd46a8c"; // Pyrmont
const ENDPOINT: &'static str = "http://192.168.1.10:8545";
const TIMEOUT: Duration = Duration::from_millis(15000);

async fn get_logs_and_drop(range: Range<u64>) {
    println!(
        "Getting deposit logs in range {}..{}",
        range.start, range.end,
    );

    match get_deposit_logs_in_range(ENDPOINT, DEPOSIT_CONTRACT, range.clone(), TIMEOUT).await {
        Ok(logs) => println!("Got {} logs in range {}..{}", logs, range.start, range.end),
        Err(e) => println!("Got error: {:?}", e),
    }
}

#[tokio::main]
async fn main() {
    let end_block = get_block_number(ENDPOINT, TIMEOUT).await.unwrap();
    let range_chunks = (START_BLOCK..end_block)
        .collect::<Vec<u64>>()
        .chunks(1000)
        .map(|vec| {
            let first = vec.first().cloned().unwrap_or_else(|| 0);
            let last = vec.last().map(|n| n + 1).unwrap_or_else(|| 0);
            first..last
        })
        .collect::<Vec<Range<u64>>>();

    for range in range_chunks {
        let chain_id = get_chain_id(ENDPOINT, TIMEOUT).await;
        println!("Chain id: {:?}", chain_id);
        let network_id = get_network_id(ENDPOINT, TIMEOUT).await;
        println!("Network id: {:?}", network_id);
        get_logs_and_drop(range).await;
    }
}
