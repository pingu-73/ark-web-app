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

// static keypair for ark and bitcoin addr generation
static WALLET_KEYPAIR: Lazy<RwLock<Option<bitcoin::key::Keypair>>> = Lazy::new(|| RwLock::new(None));

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
    let balance = APP_STATE.balance.lock().await.clone();
    
    Ok(balance)
}

pub async fn get_available_balance() -> Result<u64> {
    let balance = APP_STATE.balance.lock().await;
    Ok(balance.confirmed)
}

fn get_or_create_keypair() -> Result<bitcoin::key::Keypair> {
    let mut keypair_guard = WALLET_KEYPAIR.write().map_err(|_| anyhow::anyhow!("Failed to acquire write lock"))?;
    
    if keypair_guard.is_none() {
        // generate a new keypair if one doesn't exist
        let secp = bitcoin::secp256k1::Secp256k1::new();
        let mut rng = bitcoin::key::rand::thread_rng();
        *keypair_guard = Some(bitcoin::key::Keypair::new(&secp, &mut rng));
    }
    
    // clone the keypair to return it
    Ok(keypair_guard.as_ref().unwrap().clone())
}

pub async fn get_offchain_address() -> Result<AddressResponse> {
    // get the shared keypair
    let keypair = get_or_create_keypair()?;
    
    // find network from env variables
    let network = match std::env::var("BITCOIN_NETWORK").unwrap_or_else(|_| "regtest".into()).as_str() {
        "mainnet" => bitcoin::Network::Bitcoin,
        "testnet" => bitcoin::Network::Testnet,
        "regtest" => bitcoin::Network::Regtest,
        _ => bitcoin::Network::Regtest,
    };
    
    // [TODO!!] get the server's public key (this would come from the server)
    // for implementatgion purposes using a fixed key
    let secp = bitcoin::secp256k1::Secp256k1::new();
    let server_secret_key = bitcoin::secp256k1::SecretKey::from_slice(
        &hex::decode("0101010101010101010101010101010101010101010101010101010101010101").unwrap()
    ).unwrap();
    let server_keypair = bitcoin::key::Keypair::from_secret_key(&secp, &server_secret_key);
    let (server_xonly_pk, _) = server_keypair.x_only_public_key();
    
    // derive VTXO taproot key from the user's keypair
    let (user_xonly_pk, _) = keypair.x_only_public_key();
    let vtxo_tap_key = bitcoin::key::TweakedPublicKey::dangerous_assume_tweaked(user_xonly_pk);
    
    // creating ark addrs
    let ark_address = ark_core::ArkAddress::new(
        network,
        server_xonly_pk,
        vtxo_tap_key,
    );
    
    // encode addr to a string
    let address = ark_address.encode();
    
    Ok(AddressResponse { address })
}

pub async fn get_boarding_address() -> Result<AddressResponse> {
    let keypair = get_or_create_keypair()?;
    
    let network = match std::env::var("BITCOIN_NETWORK").unwrap_or_else(|_| "regtest".into()).as_str() {
        "mainnet" => bitcoin::Network::Bitcoin,
        "testnet" => bitcoin::Network::Testnet,
        "regtest" => bitcoin::Network::Regtest,
        _ => bitcoin::Network::Regtest,
    };
    
    // create a P2WPKH script
    let pubkey = keypair.public_key();
    
    let pubkey_hash = bitcoin::hashes::hash160::Hash::hash(&pubkey.serialize());
    
    // becasue new_p2wpkh uses it
    let wpubkey_hash = bitcoin::WPubkeyHash::from_slice(&pubkey_hash[..])
        .map_err(|e| anyhow::anyhow!("Failed to create WPubkeyHash: {}", e))?;
    
    
    let script = bitcoin::blockdata::script::ScriptBuf::new_p2wpkh(&wpubkey_hash);
    
    // bitcoin addr from the script
    let address = bitcoin::Address::from_script(&script, network)?;
    
    Ok(AddressResponse {
        address: address.to_string(),
    })
}

pub async fn check_deposits() -> Result<serde_json::Value> {
    // [TODO!!] In a real implementation, you would check the blockchain for deposits to your addresses
    // for demonstration purposes we'll just add a dummy deposit to the tx history
    
    // add a "Boarding" tx to the history
    let mut transactions = APP_STATE.transactions.lock().await;
    transactions.push(crate::models::wallet::TransactionResponse {
        txid: format!("deposit_{}", chrono::Utc::now().timestamp()),
        amount: 100000000, // 1 BTC in satoshis
        timestamp: chrono::Utc::now().timestamp(),
        type_name: "Boarding".to_string(),
        is_settled: Some(true),
    });
    
    // recalculate balance
    drop(transactions);
    APP_STATE.recalculate_balance().await?;
    
    Ok(serde_json::json!({
        "message": "Successfully added 1 BTC deposit",
        "amount": "1 BTC"
    }))
}

pub async fn send_vtxo(address: String, amount: u64) -> Result<SendResponse> {
    // calculate available balance (confirmed minus pending outgoing)
    let transactions = APP_STATE.transactions.lock().await;
    
    let mut available_balance = 0;
    let mut confirmed_balance = 0;
    
    // calculate confirmed balance
    for tx in transactions.iter() {
        if tx.is_settled == Some(true) {
            if tx.amount > 0 {
                confirmed_balance += tx.amount as u64;
            } else {
                confirmed_balance = confirmed_balance.saturating_sub(tx.amount.abs() as u64);
            }
        }
    }
    
    // subtract pending outgoing tx
    let mut pending_outgoing = 0;
    for tx in transactions.iter() {
        if tx.is_settled == Some(false) && tx.amount < 0 {
            pending_outgoing += tx.amount.abs() as u64;
        }
    }
    
    available_balance = confirmed_balance.saturating_sub(pending_outgoing);
    
    // release tx lock
    drop(transactions);
    
    // Check if there's enough available balance
    if available_balance < amount {
        return Err(anyhow::anyhow!(
            "Insufficient balance: have {} available (confirmed: {}, pending outgoing: {}), need {}",
            available_balance, confirmed_balance, pending_outgoing, amount
        ));
    }
    
    // for demonstration return a dummy response
    let txid = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string();
    
    // add tx to history
    let mut transactions = APP_STATE.transactions.lock().await;
    transactions.push(crate::models::wallet::TransactionResponse {
        txid: txid.clone(),
        amount: -(amount as i64),  // Negative amount for outgoing transaction
        timestamp: chrono::Utc::now().timestamp(),
        type_name: "Send".to_string(),
        is_settled: Some(false),  // Mark as pending initially
    });
    
    // release tx lock
    drop(transactions);
    
    // recalculate balance for consistency
    APP_STATE.recalculate_balance().await?;
    
    let response = crate::models::wallet::SendResponse {
        txid,
    };
    
    Ok(response)
}