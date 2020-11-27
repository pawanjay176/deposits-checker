//! Provides a very minimal set of functions for interfacing with the eth2 deposit contract via an
//! eth1 HTTP JSON-RPC endpoint.
//!
//! All remote functions return a future (i.e., are async).
//!
//! Does not use a web3 library, instead it uses `reqwest` (`hyper`) to call the remote endpoint
//! and `serde` to decode the response.
//!
//! ## Note
//!
//! There is no ABI parsing here, all function signatures and topics are hard-coded as constants.

use ethereum_types::H256 as Hash256;
use futures::future::TryFutureExt;
use reqwest::{header::CONTENT_TYPE, ClientBuilder, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::ops::Range;
use std::str::FromStr;
use std::time::Duration;

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

/// Represents an eth1 chain/network id.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum Eth1Id {
    Goerli,
    Mainnet,
    Custom(u64),
}

/// Used to identify a block when querying the Eth1 node.
#[derive(Clone, Copy)]
pub enum BlockQuery {
    Number(u64),
    Latest,
}

impl Into<u64> for Eth1Id {
    fn into(self) -> u64 {
        match self {
            Eth1Id::Mainnet => 1,
            Eth1Id::Goerli => 5,
            Eth1Id::Custom(id) => id,
        }
    }
}

impl From<u64> for Eth1Id {
    fn from(id: u64) -> Self {
        let into = |x: Eth1Id| -> u64 { x.into() };
        match id {
            id if id == into(Eth1Id::Mainnet) => Eth1Id::Mainnet,
            id if id == into(Eth1Id::Goerli) => Eth1Id::Goerli,
            id => Eth1Id::Custom(id),
        }
    }
}

impl FromStr for Eth1Id {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        u64::from_str_radix(s, 10)
            .map(Into::into)
            .map_err(|e| format!("Failed to parse eth1 network id {}", e))
    }
}

/// Get the eth1 network id of the given endpoint.
pub async fn get_network_id(endpoint: &str, timeout: Duration) -> Result<Eth1Id, String> {
    let response_body = send_rpc_request(endpoint, "net_version", json!([]), timeout).await?;
    Eth1Id::from_str(
        response_result(&response_body)?
            .ok_or_else(|| "No result was returned for network id".to_string())?
            .as_str()
            .ok_or_else(|| "Data was not string")?,
    )
}

/// Get the eth1 chain id of the given endpoint.
pub async fn get_chain_id(endpoint: &str, timeout: Duration) -> Result<Eth1Id, String> {
    let response_body = send_rpc_request(endpoint, "eth_chainId", json!([]), timeout).await?;
    hex_to_u64_be(
        response_result(&response_body)?
            .ok_or_else(|| "No result was returned for chain id".to_string())?
            .as_str()
            .ok_or_else(|| "Data was not string")?,
    )
    .map(Into::into)
}

#[derive(Debug, PartialEq, Clone)]
pub struct Block {
    pub hash: Hash256,
    pub timestamp: u64,
    pub number: u64,
}

/// Returns the current block number.
///
/// Uses HTTP JSON RPC at `endpoint`. E.g., `http://localhost:8545`.
pub async fn get_block_number(endpoint: &str, timeout: Duration) -> Result<u64, String> {
    let response_body = send_rpc_request(endpoint, "eth_blockNumber", json!([]), timeout).await?;
    hex_to_u64_be(
        response_result(&response_body)?
            .ok_or_else(|| "No result field was returned for block number".to_string())?
            .as_str()
            .ok_or_else(|| "Data was not string")?,
    )
    .map_err(|e| format!("Failed to get block number: {}", e))
}

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
) -> Result<usize, String> {
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
        // .cloned()
        .ok_or_else(|| "'result' value was not an array".to_string())
        .map(|a| a.len())
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

/// Removes the `0x` prefix from some bytes. Returns an error if the prefix is not present.
fn strip_prefix(hex: &str) -> Result<&str, String> {
    if hex.starts_with("0x") {
        Ok(&hex[2..])
    } else {
        Err("Hex string did not start with `0x`".to_string())
    }
}
