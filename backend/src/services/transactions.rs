#![allow(unused_imports, unused_variables, unused_assignments)]
use crate::models::wallet::TransactionResponse;
use crate::services::APP_STATE;
use ark_client::Blockchain;
use anyhow::{Result, Context};

use std::sync::Arc;
use std::str::FromStr;

pub async fn get_transaction_history() -> Result<Vec<TransactionResponse>> {
    let mut all_transactions = Vec::new();
    
    // Ark related tx
    let grpc_client = APP_STATE.grpc_client.lock().await;
    match grpc_client.get_transaction_history().await {
        Ok(ark_history) => {
            let ark_transactions = ark_history.into_iter().map(|(txid, amount, timestamp, type_name, is_settled)| {
                TransactionResponse {
                    txid,
                    amount,
                    timestamp,
                    type_name,
                    is_settled: Some(is_settled),
                }
            }).collect::<Vec<_>>();
            all_transactions.extend(ark_transactions);
        },
        Err(e) => {
            tracing::warn!("Failed to get Ark transactions: {}", e);
        }
    }
    
    // regular on-chain tx
    match get_onchain_transactions().await {
        Ok(onchain_transactions) => {
            all_transactions.extend(onchain_transactions);
        },
        Err(e) => {
            tracing::warn!("Failed to get on-chain transactions: {}", e);
        }
    }
    
    // sort by timestamp (newest first)
    all_transactions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    
    Ok(all_transactions)
}

async fn get_onchain_transactions() -> Result<Vec<TransactionResponse>> {
    let esplora_url = std::env::var("ESPLORA_URL")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());
    let blockchain = Arc::new(crate::services::ark_grpc::EsploraBlockchain::new(&esplora_url)?);
    
    let address_str = crate::services::wallet::get_onchain_address().await?;
    let address = bitcoin::Address::from_str(&address_str)?.assume_checked();
    
    let mut onchain_transactions = Vec::new();
    let existing_txids: std::collections::HashSet<String> = {
        let app_transactions = APP_STATE.transactions.lock().await;
        app_transactions.iter().map(|tx| tx.txid.clone()).collect()
    };
    
    let all_transactions = get_all_address_transactions(&blockchain, &address).await?;
    
    for (txid, net_amount, timestamp) in all_transactions {
        if existing_txids.contains(&txid) {
            continue;
        }
        
        if net_amount != 0 {
            let tx_response = TransactionResponse {
                txid: txid.clone(),
                amount: net_amount,
                timestamp,
                type_name: "OnChain".to_string(),
                is_settled: Some(true),
            };
            
            onchain_transactions.push(tx_response);
        }
    }
    
    // save to APP_STATE and db
    if !onchain_transactions.is_empty() {
        let mut app_transactions = APP_STATE.transactions.lock().await;
        for tx in &onchain_transactions {
            app_transactions.push(tx.clone());
            
            if let Err(e) = save_transaction_to_db(tx).await {
                tracing::error!("Failed to save transaction {} to database: {}", tx.txid, e);
            }
        }
    }
    
    Ok(onchain_transactions)
}

async fn get_all_address_transactions(
    blockchain: &Arc<crate::services::ark_grpc::EsploraBlockchain>,
    address: &bitcoin::Address,
) -> Result<Vec<(String, i64, i64)>> {
    let script_pubkey = address.script_pubkey();
    let mut transactions = std::collections::HashMap::new();
    
    // get all UTXOs (both spent and unspent) for this address
    let explorer_utxos = blockchain.find_outpoints(address).await
        .map_err(|e| anyhow::anyhow!("Failed to find outpoints: {}", e))?;
    
    for utxo in explorer_utxos {
        let txid = utxo.outpoint.txid.to_string();
        let timestamp = utxo.confirmation_blocktime.unwrap_or(chrono::Utc::now().timestamp() as u64) as i64;
        
        //  incoming
        let entry = transactions.entry(txid.clone()).or_insert((0i64, timestamp));
        entry.0 += utxo.amount.to_sat() as i64;
        
        // for spend UTXO => what tx spent it
        if utxo.is_spent {
            if let Ok(spend_status) = blockchain.get_output_status(&utxo.outpoint.txid, utxo.outpoint.vout).await {
                if let Some(spending_txid) = spend_status.spend_txid {
                    let spending_txid_str = spending_txid.to_string();
                    
                    if spending_txid_str != txid {
                        // outgoing
                        let spend_entry = transactions.entry(spending_txid_str).or_insert((0i64, chrono::Utc::now().timestamp()));
                        spend_entry.0 -= utxo.amount.to_sat() as i64;
                    }
                }
            }
        }
    }
    
    let result: Vec<(String, i64, i64)> = transactions
        .into_iter()
        .map(|(txid, (amount, timestamp))| (txid, amount, timestamp))
        .collect();
    
    Ok(result)
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