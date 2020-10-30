use futures::future::TryFutureExt;
use reqwest::{header::CONTENT_TYPE, ClientBuilder, StatusCode};
use serde_json::{json, Value};
use ssz::Decode;
use std::ops::Range;
use std::time::Duration;

const START_BLOCK: u64 = 3085928;
const END_BLOCK: u64 = 3666393;
const DEPOSIT_CONTRACT: &'static str = "0x07b39F4fDE4A38bACe212b546dAc87C58DfE3fDC";
const ENDPOINT: &'static str = "http://localhost:8545/";
// const ENDPOINT: &'static str = "https://goerli.infura.io/v3/be3fb7ed377c418087602876a40affa1";

/// `keccak("DepositEvent(bytes,bytes,bytes,bytes,bytes)")`
pub const DEPOSIT_EVENT_TOPIC: &str =
    "0x649bbc62d0e31342afea4e5cd82d4049e7e1ee912fc0889aa790803be39038c5";
/// `keccak("get_deposit_root()")[0..4]`
pub const DEPOSIT_ROOT_FN_SIGNATURE: &str = "0xc5f2892f";
/// `keccak("get_deposit_count()")[0..4]`
pub const DEPOSIT_COUNT_FN_SIGNATURE: &str = "0x621fd130";

/// Number of bytes in deposit contract deposit root response.
pub const DEPOSIT_COUNT_RESPONSE_BYTES: usize = 96;
/// Number of bytes in deposit contract deposit root (value only).
pub const DEPOSIT_ROOT_BYTES: usize = 32;

/// A reduced set of fields from an Eth1 contract log.
#[derive(Debug, PartialEq, Clone)]
pub struct Log {
    pub(crate) block_number: u64,
    pub(crate) data: Vec<u8>,
}

/// Returns logs for the `DEPOSIT_EVENT_TOPIC`, for the given `address` in the given
/// `block_height_range`.
///
/// It's not clear from the Ethereum JSON-RPC docs if this range is inclusive or not.
///
/// Uses HTTP JSON RPC at `endpoint`. E.g., `http://localhost:8545`.
pub async fn get_deposit_logs_in_range(
    endpoint: &str,
    address: &str,
    block_height_range: Range<u64>,
    timeout: Duration,
) -> Result<Vec<Log>, String> {
    let params = json! ([{
        "address": address,
        "topics": [DEPOSIT_EVENT_TOPIC],
        "fromBlock": format!("0x{:x}", block_height_range.start),
        "toBlock": format!("0x{:x}", block_height_range.end),
    }]);

    let response_body = send_rpc_request(endpoint, "eth_getLogs", params, timeout).await?;
    response_result(&response_body)?
        .ok_or_else(|| "No result field was returned for deposit logs".to_string())?
        .as_array()
        .cloned()
        .ok_or_else(|| "'result' value was not an array".to_string())?
        .into_iter()
        .map(|value| {
            let block_number = value
                .get("blockNumber")
                .ok_or_else(|| "No block number field in log")?
                .as_str()
                .ok_or_else(|| "Block number was not string")?;

            let data = value
                .get("data")
                .ok_or_else(|| "No block number field in log")?
                .as_str()
                .ok_or_else(|| "Data was not string")?;

            Ok(Log {
                block_number: hex_to_u64_be(&block_number)?,
                data: hex_to_bytes(data)?,
            })
        })
        .collect::<Result<Vec<Log>, String>>()
        .map_err(|e| format!("Failed to get logs in range: {}", e))
}

/// Sends an RPC request to `endpoint`, using a POST with the given `body`.
///
/// Tries to receive the response and parse the body as a `String`.
pub async fn send_rpc_request(
    endpoint: &str,
    method: &str,
    params: Value,
    timeout: Duration,
) -> Result<String, String> {
    let body = json! ({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1
    })
    .to_string();

    // Note: it is not ideal to create a new client for each request.
    //
    // A better solution would be to create some struct that contains a built client and pass it
    // around (similar to the `web3` crate's `Transport` structs).
    let response = ClientBuilder::new()
        .timeout(timeout)
        .build()
        .expect("The builder should always build a client")
        .post(endpoint)
        .header(CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .map_err(|e| format!("Request failed: {:?}", e))
        .await?;
    if response.status() != StatusCode::OK {
        return Err(format!(
            "Response HTTP status was not 200 OK:  {}.",
            response.status()
        ));
    };
    let encoding = response
        .headers()
        .get(CONTENT_TYPE)
        .ok_or_else(|| "No content-type header in response".to_string())?
        .to_str()
        .map(|s| s.to_string())
        .map_err(|e| format!("Failed to parse content-type header: {}", e))?;

    response
        .bytes()
        .map_err(|e| format!("Failed to receive body: {:?}", e))
        .await
        .and_then(move |bytes| match encoding.as_str() {
            "application/json" => Ok(bytes),
            "application/json; charset=utf-8" => Ok(bytes),
            other => Err(format!("Unsupported encoding: {}", other)),
        })
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|e| format!("Failed to receive body: {:?}", e))
}

/// Accepts an entire HTTP body (as a string) and returns the `result` field, as a serde `Value`.
fn response_result(response: &str) -> Result<Option<Value>, String> {
    let json = serde_json::from_str::<Value>(&response)
        .map_err(|e| format!("Failed to parse response: {:?}", e))?;

    if let Some(error) = json.get("error") {
        Err(format!("Eth1 node returned error: {}", error))
    } else {
        Ok(json
            .get("result")
            .cloned()
            .map(Some)
            .unwrap_or_else(|| None))
    }
}

/// Parses a `0x`-prefixed, **big-endian** hex string as a u64.
///
/// Note: the JSON-RPC encodes integers as big-endian. The deposit contract uses little-endian.
/// Therefore, this function is only useful for numbers encoded by the JSON RPC.
///
/// E.g., `0x01 == 1`
fn hex_to_u64_be(hex: &str) -> Result<u64, String> {
    u64::from_str_radix(strip_prefix(hex)?, 16)
        .map_err(|e| format!("Failed to parse hex as u64: {:?}", e))
}

/// Parses a `0x`-prefixed, big-endian hex string as bytes.
///
/// E.g., `0x0102 == vec![1, 2]`
fn hex_to_bytes(hex: &str) -> Result<Vec<u8>, String> {
    hex::decode(strip_prefix(hex)?).map_err(|e| format!("Failed to parse hex as bytes: {:?}", e))
}

/// Removes the `0x` prefix from some bytes. Returns an error if the prefix is not present.
fn strip_prefix(hex: &str) -> Result<&str, String> {
    if hex.starts_with("0x") {
        Ok(&hex[2..])
    } else {
        Err("Hex string did not start with `0x`".to_string())
    }
}

/// The following constants define the layout of bytes in the deposit contract `DepositEvent`. The
/// event bytes are formatted according to the  Ethereum ABI.
const PUBKEY_START: usize = 192;
const CREDS_START: usize = PUBKEY_START + 64 + 32;
const AMOUNT_START: usize = CREDS_START + 32 + 32;
const SIG_START: usize = AMOUNT_START + 32 + 32;
const INDEX_START: usize = SIG_START + 96 + 32;
const INDEX_LEN: usize = 8;

/// Attempts to parse a raw `Log` from the deposit contract into a `DepositLog`.
pub fn from_log(log: &Log) -> Result<u64, String> {
    let bytes = &log.data;

    let index = bytes
        .get(INDEX_START..INDEX_START + INDEX_LEN)
        .ok_or_else(|| "Insufficient bytes for index".to_string())?;

    u64::from_ssz_bytes(index).map_err(|e| format!("Invalid index ssz: {:?}", e))
}

use std::cmp::Ordering;
#[tokio::main]
async fn main() {
    let range_chunks = (START_BLOCK..END_BLOCK)
        .collect::<Vec<u64>>()
        .chunks(100)
        .map(|vec| {
            let first = vec.first().cloned().unwrap_or_else(|| 0);
            let last = vec.last().map(|n| n + 1).unwrap_or_else(|| 0);
            first..last
        })
        .collect::<Vec<Range<u64>>>();

    println!(
        "Searching for deposit logs in range {}..{}",
        START_BLOCK, END_BLOCK,
    );

    let mut indices: Vec<u64> = Vec::new();
    for range in range_chunks {
        let logs = get_deposit_logs_in_range(
            ENDPOINT,
            DEPOSIT_CONTRACT,
            range.clone(),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        // println!(
        //     "Found {} logs in range {} to {}",
        //     logs.len(),
        //     range.start,
        //     range.end
        // );
        for log in logs {
            let index = from_log(&log).unwrap();
            match index.cmp(&(indices.len() as u64)) {
                Ordering::Equal => {
                    indices.push(index);
                    // println!("Index {}", index);
                }
                Ordering::Less => {
                    // println!("Expected: {}, got: {}", indices.len(), index);
                    if indices[index as usize] != index {
                        panic!("Duplicate distinct log {}", index);
                    }
                }
                Ordering::Greater => {
                    println!("Non consecutive: {} {}", index, indices.len());
                    panic!("Non consecutive");
                }
            }
        }
    }
}
