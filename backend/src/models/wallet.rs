#![allow(unused_imports, unused_variables)]
use serde::{Deserialize, Serialize};
use bitcoin::{opcodes::all, Amount};

#[derive(Debug, Serialize)]
pub struct WalletInfo {
    pub network: String,
    pub server_url: String,
    pub connected: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct WalletBalance {
    pub confirmed: u64,
    pub trusted_pending: u64,
    pub untrusted_pending: u64,
    pub immature: u64,
    pub total: u64,
}

#[derive(Debug, Serialize)]
pub struct AddressResponse {
    pub address: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct TransactionResponse {
    pub txid: String,
    pub amount: i64,
    pub timestamp: i64,
    pub type_name: String,
    pub is_settled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SendRequest {
    pub address: String,
    pub amount: u64,
}

#[derive(Debug, Serialize)]
pub struct SendResponse {
    pub txid: String,
}

#[derive(Debug, Deserialize)]
pub struct ReceiveRequest {
    pub from_address: String,
    pub amount: u64,
}

#[derive(Debug, Deserialize)]
pub struct ExitRequest {
    pub vtxo_txid: String,
}