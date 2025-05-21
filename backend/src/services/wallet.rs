#![allow(unused_imports, unused_variables, unused_assignments)]
use crate::models::wallet::*;
use crate::services::APP_STATE;
use anyhow::{Result, Context};
use ark_core::ArkAddress;
use bitcoin::Amount;
use std::sync::RwLock;
use once_cell::sync::Lazy;
use bitcoin::hashes::{Hash, HashEngine};
use bitcoin::hashes::sha256;
use bitcoin::hashes::ripemd160;

use std::str::FromStr;

pub async fn get_wallet_info() -> Result<WalletInfo> {
    let grpc_client = APP_STATE.grpc_client.lock().await;
    
    let network = std::env::var("BITCOIN_NETWORK")
        .unwrap_or_else(|_| "regtest".into());
    let server_url = std::env::var("ARK_SERVER_URL")
        .unwrap_or_else(|_| "http://localhost:7070".into());

    let connected = grpc_client.is_connected();

    let info = WalletInfo {
        network,
        server_url,
        connected,
    };
    
    Ok(info)
}

pub async fn get_balance() -> Result<WalletBalance> {
    // use real impl
    let grpc_client = APP_STATE.grpc_client.lock().await;
    
    // get balance from Ark client
    match grpc_client.get_balance().await {
        Ok((confirmed, pending, total)) => {
            Ok(WalletBalance {
                confirmed,
                trusted_pending: pending,
                untrusted_pending: 0,
                immature: 0,
                total,
            })
        },
        Err(_) => {
            // fallback to app state
            let balance = APP_STATE.balance.lock().await.clone();
            Ok(balance)
        }
    }
}

pub async fn get_available_balance() -> Result<u64> {
    APP_STATE.recalculate_balance().await?;

    let balance = APP_STATE.balance.lock().await;
    let available = balance.confirmed;

    tracing::info!("Available balance: {}", available);

    Ok(available)
}

pub async fn get_offchain_address() -> Result<AddressResponse> {
    let grpc_client = APP_STATE.grpc_client.lock().await;
    
    match grpc_client.get_address().await {
        Ok(address) => Ok(AddressResponse { address }),
        Err(e) => Err(anyhow::anyhow!("Failed to get offchain address: {}", e))
    }
}

pub async fn get_boarding_address() -> Result<AddressResponse> {
    let grpc_client = APP_STATE.grpc_client.lock().await;
    
    match grpc_client.get_boarding_address().await {
        Ok(address) => Ok(AddressResponse { address }),
        Err(e) => Err(anyhow::anyhow!("Failed to get boarding address: {}", e))
    }
}

pub async fn check_deposits() -> Result<serde_json::Value> {
    let grpc_client = APP_STATE.grpc_client.lock().await;
    
    match grpc_client.check_deposits().await {
        Ok(true) => {
            APP_STATE.recalculate_balance().await?;
            Ok(serde_json::json!({
                "message": "Successfully processed deposits",
                "success": true
            }))
        },
        Ok(false) => Ok(serde_json::json!({
            "message": "No deposits to process",
            "success": false
        })),
        Err(e) => Err(anyhow::anyhow!("Failed to check deposits: {}", e))
    }
}

pub async fn send_vtxo(address: String, amount: u64) -> Result<SendResponse> {
    let available_balance = get_available_balance().await?;
    if available_balance < amount {
        return Err(anyhow::anyhow!(
            "Insufficient balance: have {} available, need {}",
            available_balance, amount
        ));
    }

    let grpc_client = APP_STATE.grpc_client.lock().await;
    
    tracing::info!("Attempting to send {} satoshis to address: {}", amount, address);
    
    // validate the address format
    match ArkAddress::decode(&address) {
        Ok(ark_address) => {
            tracing::info!("Successfully parsed Ark address");
            
            match grpc_client.send_vtxo(address, amount).await {
                Ok(txid) => {
                    tracing::info!("Successfully sent VTXO with txid: {}", txid);
                    
                    // create tx record
                    let tx = TransactionResponse {
                        txid: txid.clone(),
                        amount: -(amount as i64),
                        timestamp: chrono::Utc::now().timestamp(),
                        type_name: "Redeem".to_string(),
                        is_settled: Some(false),
                    };
                    
                    // save to in-memory state
                    let mut transactions = APP_STATE.transactions.lock().await;
                    transactions.push(tx.clone());
                    drop(transactions);
                    
                    // save to db
                    if let Err(e) = crate::services::transactions::save_transaction_to_db(&tx).await {
                        tracing::error!("Error saving transaction to database: {}", e);
                    }
                    
                    // recalculate balance
                    APP_STATE.recalculate_balance().await?;
                    
                    Ok(SendResponse { txid })
                },
                Err(e) => {
                    tracing::error!("Failed to send VTXO: {}", e);
                    Err(anyhow::anyhow!("Failed to send VTXO: {}", e))
                }
            }
        },
        Err(e) => {
            tracing::error!("Failed to parse Ark address: {}", e);
            Err(anyhow::anyhow!("Failed to parse Ark address: {}", e))
        }
    }
}

pub async fn receive_vtxo(from_address: String, amount: u64) -> Result<TransactionResponse> {
    // unique tx ID
    let txid = format!("rx_{}_{}", chrono::Utc::now().timestamp(), rand::random::<u32>());
    
    // add tx to the history
    let transactions = APP_STATE.transactions.lock().await;
    let tx = TransactionResponse {
        txid: txid.clone(),
        amount: amount as i64, // +ve amount for incoming transaction
        timestamp: chrono::Utc::now().timestamp(),
        type_name: "Receive".to_string(),
        is_settled: Some(false), // initially pending
    };
    
    // save to in-memory state
    let mut transactions = APP_STATE.transactions.lock().await;
    transactions.push(tx.clone());
    
    // release tx lock
    drop(transactions);

    // save to db
    crate::services::transactions::save_transaction_to_db(&tx).await?;
    
    // recalculate balance for consistency
    APP_STATE.recalculate_balance().await?;
    
    Ok(tx)
}


pub async fn send_on_chain(address: String, amount: u64) -> Result<SendResponse> {
    let available_balance = get_available_balance().await?;
    if available_balance < amount {
        return Err(anyhow::anyhow!(
            "Insufficient balance: have {} available, need {}",
            available_balance, amount
        ));
    }
    
    let grpc_client = APP_STATE.grpc_client.lock().await;
    
    tracing::info!("Attempting to send {} satoshis on-chain to address: {}", amount, address);
    
    // validate Bitcoin address
    let bitcoin_address = match bitcoin::Address::from_str(&address) 
        .and_then(|a| a.require_network(bitcoin::Network::Bitcoin)) 
    {
        Ok(addr) => {
            tracing::info!("Successfully parsed Bitcoin address");
            addr
        },
        Err(e) => {
            tracing::error!("Failed to parse Bitcoin address: {}", e);
            return Err(anyhow::anyhow!("Invalid Bitcoin address: {}", e));
        }
    };
    
    // send on-chain
    match grpc_client.send_on_chain(bitcoin_address, amount).await {
        Ok(txid) => {
            tracing::info!("Successfully sent on-chain with txid: {}", txid);
            
            // create tx record
            let tx = TransactionResponse {
                txid: txid.to_string(),
                amount: -(amount as i64),
                timestamp: chrono::Utc::now().timestamp(),
                type_name: "OnChain".to_string(),
                is_settled: Some(false),
            };
            
            // save to in-memory state
            let mut transactions = APP_STATE.transactions.lock().await;
            transactions.push(tx.clone());
            drop(transactions);
            
            // save to db
            if let Err(e) = crate::services::transactions::save_transaction_to_db(&tx).await {
                tracing::error!("Error saving transaction to database: {}", e);
            }
            
            // recalculate balance
            APP_STATE.recalculate_balance().await?;
            
            Ok(SendResponse { txid: txid.to_string() })
        },
        Err(e) => {
            tracing::error!("Failed to send on-chain: {}", e);
            Err(anyhow::anyhow!("Failed to send on-chain: {}", e))
        }
    }
}