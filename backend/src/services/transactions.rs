#![allow(unused_imports, unused_variables, unused_assignments)]
use crate::models::wallet::TransactionResponse;
use crate::services::APP_STATE;
use anyhow::{Result, Context};

use std::sync::Arc;

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
    tracing::info!("Starting round participation");
    let grpc_client = APP_STATE.grpc_client.lock().await;
    tracing::info!("Acquired gRPC client lock");
    let mut rng = bip39::rand::rngs::OsRng;
    
    // Clone the Arc to avoid holding lock
    let client = {
        let client_opt = grpc_client.get_ark_client();
        client_opt.as_ref().map(|c| Arc::clone(c))
    };
    
    if let Some(client) = client {
        tracing::info!("Got Ark client");
        
        // try to board
        tracing::info!("Attempting to board funds");
        match client.board(&mut rng).await {
            Ok(_) => {
                tracing::info!("Successfully participated in round");
                
                // update app state after round participation
                match grpc_client.update_app_state().await {
                    Ok(_) => tracing::info!("Successfully updated app state after round participation"),
                    Err(e) => tracing::warn!("Failed to update app state after round participation: {}", e),
                }
                
                // recalculate balance
                match APP_STATE.recalculate_balance().await {
                    Ok(_) => tracing::info!("Successfully recalculated balance after round participation"),
                    Err(e) => tracing::warn!("Failed to recalculate balance after round participation: {}", e),
                }
                
                // return a placeholder txid for now
                let txid = format!("round_{}", chrono::Utc::now().timestamp());
                
                // create a tx record
                let tx = crate::models::wallet::TransactionResponse {
                    txid: txid.clone(),
                    amount: 0, // rounds don't change the total balance
                    timestamp: chrono::Utc::now().timestamp(),
                    type_name: "Round".to_string(),
                    is_settled: Some(true),
                };
                
                // save to in-memory state
                let mut transactions = APP_STATE.transactions.lock().await;
                transactions.push(tx.clone());
                drop(transactions);
                
                // save to db
                match save_transaction_to_db(&tx).await {
                    Ok(_) => tracing::info!("Successfully saved round transaction to database"),
                    Err(e) => tracing::error!("Error saving transaction to database: {}", e),
                }
                
                Ok(Some(txid))
            },
            Err(e) => {
                if e.to_string().contains("No boarding outputs") && e.to_string().contains("No VTXOs") {
                    tracing::info!("No outputs to include in round");
                    Ok(None)
                } 
                else {
                    tracing::error!("Error participating in round: {}", e);
                    Err(anyhow::anyhow!("Error participating in round: {}", e))
                }
            }
        }
    } 
    else {
        tracing::error!("Ark client not available");
        Err(anyhow::anyhow!("Ark client not available"))
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

pub async fn save_transaction_to_db(tx: &crate::models::wallet::TransactionResponse) -> Result<()> {
    let conn = APP_STATE.db_manager.get_conn()?;
    
    conn.execute(
        "INSERT OR REPLACE INTO transactions (
            txid, amount, timestamp, type_name, is_settled, raw_tx
        ) VALUES (?, ?, ?, ?, ?, ?)",
        rusqlite::params![
            tx.txid,
            tx.amount,
            tx.timestamp,
            tx.type_name,
            tx.is_settled,
            Option::<String>::None, // raw_tx (optional)
        ],
    )?;
    
    Ok(())
}