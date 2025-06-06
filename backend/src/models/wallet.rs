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

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Deserialize)]
pub struct SendOnchainRequest {
    pub address: String,
    pub amount: u64,
    pub priority: Option<String>, // "fastest", "fast", "normal", "slow"
}

#[derive(Debug, Deserialize)]
pub struct EstimateFeeDetailedRequest {
    pub address: String,
    pub amount: u64,
}

#[derive(Debug, Serialize)]
pub struct FeeEstimateResponse {
    pub estimates: crate::services::onchain::fee_estimator::FeeEstimates,
    pub transaction_fees: Vec<TransactionFeeEstimate>,
}

#[derive(Debug, Serialize)]
pub struct TransactionFeeEstimate {
    pub priority: String,
    pub blocks: String,
    pub fee_rate: u64,
    pub total_fee: u64,
}