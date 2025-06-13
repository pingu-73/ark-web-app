#![allow(unused_imports, unused_variables, unused_assignments)]
use crate::models::wallet::TransactionResponse;
use crate::services::offchain::ArkOffChainService;
use crate::services::APP_STATE;
use ark_client::Blockchain;
use ark_core::ArkAddress;
use anyhow::{Result, Context, anyhow};

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

pub async fn participate_in_round() -> Result<Option<String>> {
    let grpc_client = APP_STATE.grpc_client.lock().await;
    grpc_client.participate_in_round().await
}

pub async fn create_redeem_transaction(recipient_address: String, amount: u64) -> Result<String> {
    let grpc_client = APP_STATE.grpc_client.lock().await;
    grpc_client.send_vtxo(recipient_address, amount).await
}

pub async fn unilateral_exit(vtxo_txid: String) -> Result<String> {
    let grpc_client = APP_STATE.grpc_client.lock().await;
    let tx = grpc_client.unilateral_exit(vtxo_txid).await?;
    Ok(tx.txid)
}