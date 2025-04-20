#![allow(unused_imports, unused_variables, dead_code)]
pub mod wallet;
pub mod transactions;
pub mod ark_grpc;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use ark_client::{Client, OfflineClient, wallet::Persistence, error::Error as ArkError};
use ark_bdk_wallet::Wallet as BdkWallet;
use ark_core::BoardingOutput;
use bitcoin::Network;
use bitcoin::secp256k1::SecretKey;
use bitcoin::XOnlyPublicKey;
use std::sync::RwLock;

// in-memory database for persistence 
// refrenced from https://github.com/ArkLabsHQ/ark-rs/blob/7c793c4f3226dc4d5ce7637f3087adf56727799a/e2e-tests/tests/common.rs#L217
#[derive(Default)]
pub struct InMemoryDb {
    boarding_outputs: RwLock<Vec<(SecretKey, BoardingOutput)>>,
}

impl InMemoryDb {
    pub fn new() -> Self {
        Self {
            boarding_outputs: RwLock::new(Vec::new()),
        }
    }
}

impl Persistence for InMemoryDb {
    fn save_boarding_output(
        &self,
        sk: SecretKey,
        boarding_output: BoardingOutput,
    ) -> Result<(), ArkError> {
        self.boarding_outputs
            .write()
            .unwrap()
            .push((sk, boarding_output));

        Ok(())
    }

    fn load_boarding_outputs(&self) -> Result<Vec<BoardingOutput>, ArkError> {
        Ok(self
            .boarding_outputs
            .read()
            .unwrap()
            .clone()
            .into_iter()
            .map(|(_, b)| b)
            .collect())
    }

    fn sk_for_pk(&self, pk: &XOnlyPublicKey) -> Result<SecretKey, ArkError> {
        let maybe_sk = self
            .boarding_outputs
            .read()
            .unwrap()
            .iter()
            .find_map(|(sk, b)| if b.owner_pk() == *pk { Some(*sk) } else { None });
        
        match maybe_sk {
            Some(secret_key) => Ok(secret_key),
            None => Err(ArkError::wallet(anyhow::anyhow!("Secret key not found for public key"))),
        }
    }
}

// type alias for our client
type ArkClient = Client<BdkWallet<InMemoryDb>, BdkWallet<InMemoryDb>>;

// Global state for web app
pub struct AppState {
    pub client: Arc<Mutex<Option<ArkClient>>>,
    pub grpc_client: Arc<Mutex<ark_grpc::ArkGrpcService>>,
    pub transactions: Arc<Mutex<Vec<crate::models::wallet::TransactionResponse>>>,
    pub balance: Arc<Mutex<crate::models::wallet::WalletBalance>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            client: Arc::new(Mutex::new(None)),
            grpc_client: Arc::new(Mutex::new(ark_grpc::ArkGrpcService::new())),
            transactions: Arc::new(Mutex::new(vec![
                crate::models::wallet::TransactionResponse {
                    txid: "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
                    amount: 100000,
                    timestamp: chrono::Utc::now().timestamp(),
                    type_name: "Boarding".to_string(),
                    is_settled: Some(true),
                }
            ])),
            balance: Arc::new(Mutex::new(crate::models::wallet::WalletBalance {
                confirmed: 100000, // initially all confirmed
                trusted_pending: 0,
                untrusted_pending: 0,
                immature: 0,
                total: 100000,
            })),
        }
    }
    
    pub async fn initialize(&self) -> Result<()> {
        // initialize the Ark client
        // load keys from env var or a secure store
        let network = match std::env::var("BITCOIN_NETWORK").unwrap_or_else(|_| "testnet".into()).as_str() {
            "mainnet" => Network::Bitcoin,
            "testnet" => Network::Testnet,
            "regtest" => Network::Regtest,
            _ => Network::Testnet,
        };
        
        let esplora_url = std::env::var("ESPLORA_URL")
            .unwrap_or_else(|_| "http://localhost:3002".into());
            
        let ark_server_url = std::env::var("ARK_SERVER_URL")
            .unwrap_or_else(|_| "http://localhost:7070".into());
            
        tracing::info!("Initializing with network: {}, esplora: {}, ark server: {}", 
            network, esplora_url, ark_server_url);
        // connect to Ark server using gRPC becasue Ark server only accepts gRPC and not http
        let mut grpc_client = self.grpc_client.lock().await;
        match grpc_client.connect(&ark_server_url).await {
            Ok(_) => {
                tracing::info!("Successfully connected to Ark server via gRPC");
            },
            Err(e) => {
                tracing::error!("Failed to connect to Ark server via gRPC: {}", e);
                // continue even if connection fails so the app can still run with dummy data
            }
        }
        // Note: We're not initializing the original client for now
        // If you want to initialize it as well, you can add that code here
        Ok(())
    }

    pub async fn recalculate_balance(&self) -> Result<()> {
        let transactions = self.transactions.lock().await;
        let mut balance = self.balance.lock().await;
        
        // reset balance
        *balance = crate::models::wallet::WalletBalance {
            confirmed: 0,
            trusted_pending: 0,
            untrusted_pending: 0,
            immature: 0,
            total: 0,
        };
        
        // First pass: calculate confirmed balance from settled tx
        for tx in transactions.iter() {
            if tx.is_settled == Some(true) {
                if tx.amount > 0 {
                    balance.confirmed += tx.amount as u64;
                } else {
                    // don't subtract if it would result in negative balance
                    let amount = tx.amount.abs() as u64;
                    if balance.confirmed >= amount {
                        balance.confirmed -= amount;
                    }
                }
            }
        }
        
        // Second pass: calculate pending balance from unsettled tx
        let mut available_confirmed = balance.confirmed;
        
        for tx in transactions.iter() {
            if tx.is_settled == Some(false) {
                if tx.amount > 0 {
                    // incoming tx
                    balance.untrusted_pending += tx.amount as u64;
                } else {
                    // outgoing tx
                    let amount = tx.amount.abs() as u64;
                    
                    // check if we have enough confirmed balance
                    if available_confirmed >= amount {
                        // if enough confirmed balance => valid pending tx
                        available_confirmed -= amount;
                        balance.trusted_pending += amount;
                    } else {
                        // not enough confirmed balance => tx is invalid
                        // [TODO!!] In a real implementation, we would reject or cancel this tx
                        // for implementation we'll just log a warning
                        tracing::warn!("Invalid pending transaction: insufficient balance for txid {}", tx.txid);
                    }
                }
            }
        }
        
        balance.total = balance.confirmed + balance.trusted_pending + balance.untrusted_pending + balance.immature;
        
        Ok(())
    }

    pub async fn can_send(&self, amount: u64) -> Result<bool> {
        let balance = self.balance.lock().await;
        Ok(balance.confirmed >= amount)
    }
}

// initialize global state
lazy_static::lazy_static! {
    pub static ref APP_STATE: AppState = AppState::new();
}