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
    let mut transactions = APP_STATE.transactions.lock().await;
    
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
    let balance = APP_STATE.balance.lock().await;
    let confirmed_balance = balance.confirmed;
    drop(balance);
    
    // ensure there is enough balance
    if confirmed_balance < total_outgoing as u64 {
        return Err(anyhow::anyhow!(
            "Insufficient balance for round: have {} confirmed, need {}",
            confirmed_balance, total_outgoing
        ));
    }
    
    // mark all pending tx as settled
    let mut settled_txids = Vec::new();
    for tx in transactions.iter_mut() {
        if tx.is_settled == Some(false) {
            tx.is_settled = Some(true);
            settled_txids.push(tx.txid.clone());
        }
    }
    
    let round_txid = format!("round_{}_{}", chrono::Utc::now().timestamp(), rand::random::<u32>());
    
    // add round tx to the history
    transactions.push(crate::models::wallet::TransactionResponse {
        txid: round_txid.clone(),
        amount: 0, // rounds don't change balance directly
        timestamp: chrono::Utc::now().timestamp(),
        type_name: "Round".to_string(),
        is_settled: Some(true),
    });
    
    drop(transactions);
    
    // recalculate balance for consistency
    APP_STATE.recalculate_balance().await?;
    
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
    // find tx
    let transactions = APP_STATE.transactions.lock().await;
    
    // check if tx exists
    let tx_exists = transactions.iter().any(|tx| tx.txid == vtxo_txid);
    if !tx_exists {
        drop(transactions);
        return Err(anyhow::anyhow!("Transaction not found: {}", vtxo_txid));
    }
    
    // check it's in right state
    let vtxo = transactions.iter()
        .find(|tx| tx.txid == vtxo_txid && tx.is_settled == Some(false))
        .cloned();
    
    if vtxo.is_none() {
        // tx exists but is not in pending state
        let tx = transactions.iter()
            .find(|tx| tx.txid == vtxo_txid)
            .unwrap();
            
        if tx.is_settled == Some(true) {
            drop(transactions);
            return Err(anyhow::anyhow!("Transaction is already settled: {}", vtxo_txid));
        } else if tx.is_settled == None {
            drop(transactions);
            return Err(anyhow::anyhow!("Transaction is already cancelled: {}", vtxo_txid));
        } else {
            drop(transactions);
            return Err(anyhow::anyhow!("Transaction is in an unknown state: {}", vtxo_txid));
        }
    }
    
    let vtxo = vtxo.unwrap();
    drop(transactions);
    
    // generate a unique exit tx ID
    let exit_txid = format!("exit_{}_{}", chrono::Utc::now().timestamp(), rand::random::<u32>());
    
    // add exit tx to the history
    let mut transactions = APP_STATE.transactions.lock().await;
    
    // mark original tx as cancelled
    for tx in transactions.iter_mut() {
        if tx.txid == vtxo_txid {
            tx.is_settled = None;
            break;
        }
    }
    
    // add exit tx
    let tx = TransactionResponse {
        txid: exit_txid.clone(),
        // only deducting network fees (assuming it as 100 sats)
        amount: -100, 
        timestamp: chrono::Utc::now().timestamp(),
        type_name: "Exit".to_string(),
        is_settled: Some(true),
    };
    transactions.push(tx.clone());
    
    drop(transactions);
    
    // recalculate balance for consistency
    APP_STATE.recalculate_balance().await?;
    
    Ok(tx)
}