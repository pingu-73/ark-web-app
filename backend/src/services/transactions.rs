#![allow(unused_imports, unused_variables, unused_assignments)]
use crate::models::wallet::TransactionResponse;
use crate::services::APP_STATE;
use anyhow::{Result, Context};
use bitcoin::opcodes::all;
use std::collections::HashSet;

pub async fn get_transaction_history() -> Result<Vec<TransactionResponse>> {
    // tx from the app state
    let transactions = APP_STATE.transactions.lock().await.clone();
    
    Ok(transactions)
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
    // [TODO!!] As per Ark protocol, participating in a round involves:
    // 1. Registering inputs (VTXOs to spend)
    // 2. Registering outputs (new VTXOs to create)
    // 3. Signing the round tx
    // 4. Submitting signatures
    // 5. Waiting for the round to be finalized
    
    // for implementation, we'll:
    // 1. Check if we have any pending tx
    // 2. Validate that we have enough balance for all pending outgoing txs
    // 3. Mark valid txs as settled
    // 4. Return a tx ID for the round
    
    // get txs from app state
    let transactions = APP_STATE.transactions.lock().await;
    
    // find all pending txs
    let pending_txs: Vec<_> = transactions.iter()
        .filter(|tx| tx.is_settled == Some(false))
        .collect();
    
    if pending_txs.is_empty() {
        // no pending tx to settle
        return Ok(None);
    }
    
    // calculate total outgoing amount
    let total_outgoing: i64 = pending_txs.iter()
        .filter(|tx| tx.amount < 0)
        .map(|tx| tx.amount.abs())
        .sum();
    
    // get confirmed balance
    // release lock before calling another function that might need it
    drop(transactions); 
    let confirmed_balance = crate::services::wallet::get_available_balance().await?;
    
    // check if enough balance is present
    if confirmed_balance < total_outgoing as u64 {
        return Err(anyhow::anyhow!(
            "Insufficient balance for round: have {} confirmed, need {}",
            confirmed_balance, total_outgoing
        ));
    }
    
    // [TODO!!] In the Ark protocol, we would now:
    // 1. Create a round transaction
    // 2. Sign it
    // 3. Submit it to the Ark server
    
    // for implementation we'll just mark all pending transactions as settled
    let mut transactions = APP_STATE.transactions.lock().await;
    let mut settled_txids = HashSet::new();
    for tx in transactions.iter_mut() {
        if tx.is_settled == Some(false) {
            tx.is_settled = Some(true);
            settled_txids.insert(tx.txid.clone());
        }
    }
    
    // release tx lock
    drop(transactions);
    
    // recalculate balance to ensure consistency
    APP_STATE.recalculate_balance().await?;
    
    // "Round" tx to represent settlement
    let round_txid = format!("round_{}", chrono::Utc::now().timestamp());
    
    // Add the round tx to the history
    let mut transactions = APP_STATE.transactions.lock().await;
    transactions.push(TransactionResponse {
        txid: round_txid.clone(),
        amount: 0, // Rounds don't change the balance directly
        timestamp: chrono::Utc::now().timestamp(),
        type_name: "Round".to_string(),
        is_settled: Some(true),
    });
    
    // Log the settled transactions
    tracing::info!(
        "Round {} settled {} transactions: {:?}",
        round_txid, settled_txids.len(), settled_txids
    );
    
    Ok(Some(round_txid))
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

// new function to handle incoming redeem transactions
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
    // [TODO!!] In the Ark protocol, a unilateral exit:
    // 1. Spends a VTXO on-chain
    // 2. Uses the exit path in the VTXO script
    // 3. Is subject to a timelock
    
    // find VTXO to exit
    let transactions = APP_STATE.transactions.lock().await;
    let vtxo = transactions.iter()
        .find(|tx| tx.txid == vtxo_txid)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("VTXO not found: {}", vtxo_txid))?;
    drop(transactions);
    
    let exit_txid = format!("exit_{}", chrono::Utc::now().timestamp());
    
    // add exit tx to the history
    let mut transactions = APP_STATE.transactions.lock().await;
    let tx = TransactionResponse {
        txid: exit_txid.clone(),
        amount: vtxo.amount, // same amount as VTXO
        timestamp: chrono::Utc::now().timestamp(),
        type_name: "Exit".to_string(),
        is_settled: Some(false), // pending initially
    };
    transactions.push(tx.clone());
    
    drop(transactions);
    
    APP_STATE.recalculate_balance().await?; // for consistency
    
    Ok(tx)
}