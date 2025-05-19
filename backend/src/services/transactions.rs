#![allow(unused_imports, unused_variables, unused_assignments)]
use crate::models::wallet::TransactionResponse;
use crate::services::APP_STATE;
use anyhow::{Result, Context};
use bitcoin::opcodes::all;
use std::collections::HashSet;

pub async fn get_transaction_history() -> Result<Vec<TransactionResponse>> {
    tracing::info!("Service: Starting to fetch transaction history");
    
    let grpc_client = APP_STATE.grpc_client.lock().await;
    tracing::info!("Service: Acquired gRPC client lock");
    
    match grpc_client.get_transaction_history().await {
        Ok(history) => {
            tracing::info!("Service: Successfully fetched {} transactions from gRPC client", history.len());

            let transactions = history.into_iter().map(|(txid, amount, timestamp, type_name, is_settled)| {
                TransactionResponse {
                    txid,
                    amount,
                    timestamp,
                    type_name,
                    is_settled: Some(is_settled),
                }
            }).collect();
            
            Ok(transactions)
        },
        Err(e) => {
            tracing::error!("Service: Error fetching transactions from gRPC client: {}", e);

            // Fallback to app state
            tracing::info!("Service: Falling back to app state for transactions");
            let transactions = APP_STATE.transactions.lock().await.clone();
            tracing::info!("Service: Retrieved {} transactions from app state", transactions.len());

            Ok(transactions)
        }
    }
}

pub async fn get_transaction(txid: String) -> Result<TransactionResponse> {
    // tx from the app state
    let transactions = APP_STATE.transactions.lock().await;
    
    // find tx with given txid
    let transaction = transactions.iter()
        .find(|tx| tx.txid == txid)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Transaction not found: {}", txid))?;
    
    Ok(transaction)
}

pub async fn participate_in_round() -> Result<Option<String>> {
    let grpc_client = APP_STATE.grpc_client.lock().await;
    
    match grpc_client.participate_in_round().await {
        Ok(txid) => Ok(txid),
        Err(e) => Err(anyhow::anyhow!("Failed to participate in round: {}", e))
    }
}

pub async fn create_redeem_transaction(
    recipient_address: String,
    amount: u64,
) -> Result<TransactionResponse> {
    // [TODO!!] In the Ark protocol, a redeem transaction:
    // 1. Spends one or more VTXOs
    // 2. Creates new VTXOs for the recipient and change
    // 3. Is signed by the sender and the Ark server
    
    let available_balance = crate::services::wallet::get_available_balance().await?;
    if available_balance < amount {
        return Err(anyhow::anyhow!(
            "Insufficient balance: have {} available, need {}",
            available_balance, amount
        ));
    }
    
    let txid = format!("redeem_{}", chrono::Utc::now().timestamp());
    
    // Add tx to history
    let mut transactions = APP_STATE.transactions.lock().await;
    let tx = TransactionResponse {
        txid: txid.clone(),
        amount: -(amount as i64), // -ve amount for outgoing tx
        timestamp: chrono::Utc::now().timestamp(),
        type_name: "Redeem".to_string(),
        is_settled: Some(false), // initially pending
    };
    transactions.push(tx.clone());
    
    drop(transactions);
    
    APP_STATE.recalculate_balance().await?; // for consistency
    
    Ok(tx)
}

pub async fn receive_redeem_transaction(
    sender_address: String,
    amount: u64,
    txid: String,
) -> Result<TransactionResponse> {
    // add the tx to history
    let mut transactions = APP_STATE.transactions.lock().await;
    let tx = TransactionResponse {
        txid: txid.clone(),
        amount: amount as i64, // +ve for incoming tx
        timestamp: chrono::Utc::now().timestamp(),
        type_name: "Redeem".to_string(),
        is_settled: Some(false), // pending initially
    };
    transactions.push(tx.clone());
    
    drop(transactions);
    
    APP_STATE.recalculate_balance().await?; // for consistency
    
    Ok(tx)
}

pub async fn unilateral_exit(vtxo_txid: String) -> Result<TransactionResponse> {
    let grpc_client = APP_STATE.grpc_client.lock().await;
    
    match grpc_client.unilateral_exit(vtxo_txid).await {
        Ok(tx) => Ok(tx),
        Err(e) => Err(anyhow::anyhow!("Failed to perform unilateral exit: {}", e))
    }
}