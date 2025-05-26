#![allow(unused_imports, unused_variables, unused_assignments)]
use crate::models::wallet::*;
use crate::services::APP_STATE;
use crate::services::onchain::OnChainPaymentService;
use anyhow::{Result, Context};
use ark_core::ArkAddress;
use bitcoin::Amount;
use std::sync::Arc;

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
    let tx = TransactionResponse {
        txid: txid.clone(),
        amount: amount as i64, // +ve amount for incoming tx
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


pub async fn get_onchain_address() -> Result<String> {
    let (keypair, _) = APP_STATE.key_manager.load_or_create_wallet()?;
    
    let network = match std::env::var("BITCOIN_NETWORK").unwrap_or_else(|_| "regtest".to_string()).as_str() {
        "mainnet" => bitcoin::Network::Bitcoin,
        "testnet" => bitcoin::Network::Testnet,
        "signet" => bitcoin::Network::Signet,
        _ => bitcoin::Network::Regtest,
    };

    let pubkey = keypair.public_key();
    let pubkey_bytes = pubkey.serialize();
    let wpkh = bitcoin::key::CompressedPublicKey::from_slice(&pubkey_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to create WPKH: {}", e))?;
    let address = bitcoin::Address::p2wpkh(&wpkh, network);

    Ok(address.to_string())
}

pub async fn debug_vtxos() -> Result<serde_json::Value> {
    let grpc_client = APP_STATE.grpc_client.lock().await;
    
    // Clone the Arc to avoid holding lock
    let client = {
        let client_opt = grpc_client.get_ark_client();
        client_opt.as_ref().map(|c| Arc::clone(c))
    };
    
    if let Some(client) = client {
        match client.spendable_vtxos().await {
            Ok(vtxos) => {
                Ok(serde_json::json!({
                    "count": vtxos.len(),
                    "vtxos": vtxos.iter().map(|(outpoints, vtxo)| {
                        serde_json::json!({
                            "outpoints": outpoints.len(),
                            "vtxo_address": vtxo.address().to_string(),
                            "outpoint_details": outpoints.iter().map(|o| {
                                serde_json::json!({
                                    "outpoint": o.outpoint.to_string(),
                                    "amount": o.amount.to_sat(),
                                    "is_pending": o.is_pending,
                                    "expire_at": o.expire_at,
                                })
                            }).collect::<Vec<_>>()
                        })
                    }).collect::<Vec<_>>()
                }))
            },
            Err(e) => {
                Ok(serde_json::json!({
                    "error": format!("Failed to get spendable VTXOs: {}", e)
                }))
            }
        }
    } 
    else {
        Ok(serde_json::json!({
            "error": "Ark client not available"
        }))
    }
}


pub async fn send_onchain_payment(address: String, amount: u64) -> Result<SendResponse> {
    let bitcoin_address = match bitcoin::Address::from_str(&address) {
        Ok(addr) => {
            let network = match std::env::var("BITCOIN_NETWORK").unwrap_or_else(|_| "regtest".to_string()).as_str() {
                "mainnet" => bitcoin::Network::Bitcoin,
                "testnet" => bitcoin::Network::Testnet,
                "signet" => bitcoin::Network::Signet,
                _ => bitcoin::Network::Regtest,
            };
            
            match addr.clone().require_network(network) {
                Ok(addr) => addr,
                Err(_) => addr.assume_checked()
            }
        },
        Err(e) => {
            return Err(anyhow::anyhow!("Invalid Bitcoin address: {}", e));
        }
    };

    // create blockchain instance
    let esplora_url = std::env::var("ESPLORA_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let blockchain = Arc::new(crate::services::ark_grpc::EsploraBlockchain::new(&esplora_url)?);
    
    // create payment service
    let payment_service = OnChainPaymentService::new(blockchain);
    
    // send payment
    let amount = bitcoin::Amount::from_sat(amount);
    let txid = payment_service.send_payment(bitcoin_address, amount, None).await?;
    
    // tx record
    let tx = TransactionResponse {
        txid: txid.to_string(),
        amount: -(amount.to_sat() as i64),
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
    
    Ok(SendResponse { txid: txid.to_string() })
}

pub async fn get_onchain_balance() -> Result<u64> {
    let esplora_url = std::env::var("ESPLORA_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let blockchain = Arc::new(crate::services::ark_grpc::EsploraBlockchain::new(&esplora_url)?);
    
    let payment_service = OnChainPaymentService::new(blockchain);
    let balance = payment_service.get_balance().await?;
    
    Ok(balance.to_sat())
}

pub async fn estimate_onchain_fee(address: String, amount: u64) -> Result<u64> {
    let bitcoin_address = bitcoin::Address::from_str(&address)?
        .assume_checked();
    
    let esplora_url = std::env::var("ESPLORA_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let blockchain = Arc::new(crate::services::ark_grpc::EsploraBlockchain::new(&esplora_url)?);
    
    let payment_service = OnChainPaymentService::new(blockchain);
    let amount = bitcoin::Amount::from_sat(amount);
    let fee = payment_service.estimate_fee(bitcoin_address, amount).await?;
    
    Ok(fee.to_sat())
}