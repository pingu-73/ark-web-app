#![allow(unused_imports, unused_variables, dead_code)]
pub mod wallet;
pub mod transactions;
pub mod ark_grpc;
pub mod onchain;
pub mod offchain;
pub mod faucet;
pub mod multi_wallet;

use tokio::sync::OnceCell;
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

use crate::storage::{DbManager, KeyManager};

#[derive(Clone)]
pub struct AppState {
    // pub client: Arc<Mutex<Option<ark_client::Client<ark_grpc::EsploraBlockchain, ark_grpc::ArkWallet>>>>,
    pub grpc_client: Arc<Mutex<ark_grpc::ArkGrpcService>>,
    pub transactions: Arc<Mutex<Vec<crate::models::wallet::TransactionResponse>>>,
    pub balance: Arc<Mutex<crate::models::wallet::WalletBalance>>,
    pub db_manager: Arc<DbManager>,
    pub key_manager: Arc<KeyManager>,
}

impl AppState {
    pub async fn new() -> Result<Self> {
        let network = match std::env::var("BITCOIN_NETWORK").unwrap_or_else(|_| "regtest".to_string()).as_str() {
            "mainnet" => Network::Bitcoin,
            "testnet" => Network::Testnet,
            "signet" => Network::Signet,
            _ => Network::Regtest,
        };
        
        // initialize storage
        let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string());
        let db_path = format!("{}/ark.db", data_dir);
        let db_manager = Arc::new(DbManager::new(&db_path).await?);
        let key_manager = Arc::new(KeyManager::new(&data_dir, network));
        
        Ok(Self {
            // client: Arc::new(Mutex::new(None)),
            grpc_client: Arc::new(Mutex::new(ark_grpc::ArkGrpcService::new())),
            transactions: Arc::new(Mutex::new(Vec::new())),
            balance: Arc::new(Mutex::new(crate::models::wallet::WalletBalance {
                confirmed: 0,
                trusted_pending: 0,
                untrusted_pending: 0,
                immature: 0,
                total: 0,
            })),
            db_manager,
            key_manager,
        })
    }
    
    pub async fn initialize(&self) -> Result<()> {
        // initialize the Ark gRPC client
        let ark_server_url = std::env::var("ARK_SERVER_URL")
            .unwrap_or_else(|_| "http://localhost:7070".into());
            
        tracing::info!("Initializing with ark server: {}", ark_server_url);
        
        // connect to Ark server using gRPC
        let mut grpc_client = self.grpc_client.lock().await;
        match grpc_client.connect(&ark_server_url).await {
            Ok(_) => {
                tracing::info!("Successfully connected to Ark server via gRPC");
                
                // update app state with client info
                match grpc_client.update_app_state().await {
                    Ok(_) => tracing::info!("Successfully updated app state from Ark client"),
                    Err(e) => tracing::warn!("Failed to update app state from Ark client: {}", e),
                }
            },
            Err(e) => {
                tracing::error!("Failed to connect to Ark server via gRPC: {}", e);
                // continue even if connection fails so the app can still run with dummy data
            }
        }
        
        // load tx from db
        self.load_transactions_from_db().await?;
        
        // load balance from db
        self.load_balance_from_db().await?;
        
        Ok(())
    }

    async fn load_transactions_from_db(&self) -> Result<()> {
        // [TODO!!]  currently just use the in-memory tx
        Ok(())
    }

    async fn load_balance_from_db(&self) -> Result<()> {
        // [TODO!!] currently just use the in-memory balance
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
        
        // calculate balance from tx
        for tx in transactions.iter() {
            if tx.is_settled == Some(true) {
                // for settled tx
                if tx.amount > 0 {
                    // incoming tx
                    balance.confirmed += tx.amount as u64;
                } else {
                    // outgoing tx
                    let amount = tx.amount.abs() as u64;
                    if balance.confirmed >= amount {
                        balance.confirmed -= amount;
                    } else {
                        tracing::warn!("Negative balance would result from transaction {}", tx.txid);
                    }
                }
            } else if tx.is_settled == Some(false) {
                // for pending tx
                if tx.amount > 0 {
                    // incoming tx
                    balance.untrusted_pending += tx.amount as u64;
                } else {
                    // outgoing tx
                    let amount = tx.amount.abs() as u64;
                    balance.trusted_pending += amount;
                }
            }
        }
        
        // calculate total
        balance.total = balance.confirmed + balance.untrusted_pending;
        
        // save balance to db
        let balance_json = serde_json::to_string(&*balance)?;
        self.db_manager.save_setting("balance", &balance_json).await?;
        
        tracing::info!(
            "Recalculated balance: confirmed={}, trusted_pending={}, untrusted_pending={}, total={}",
            balance.confirmed, balance.trusted_pending, balance.untrusted_pending, balance.total
        );
        
        Ok(())
    }

    pub async fn can_send(&self, amount: u64) -> Result<bool> {
        let balance = self.balance.lock().await;
        Ok(balance.confirmed >= amount)
    }
}

// initialize global state
// lazy_static::lazy_static! {
//     pub static ref APP_STATE: AppState = AppState::new().expect("Failed to initialize app state");
// }
// Global state using OnceCell
static APP_STATE_CELL: OnceCell<AppState> = OnceCell::const_new();

// Helper function to get APP_STATE
pub async fn get_app_state() -> &'static AppState {
    APP_STATE_CELL.get_or_init(|| async {
        AppState::new().await.expect("Failed to initialize app state")
    }).await
}

// Create a struct that implements Deref to AppState for backward compatibility
pub struct AppStateRef;

impl std::ops::Deref for AppStateRef {
    type Target = AppState;

    fn deref(&self) -> &Self::Target {
        // This is a bit of a hack, but it allows synchronous access
        // We'll need to ensure APP_STATE is initialized before any sync access
        APP_STATE_CELL.get().expect("APP_STATE not initialized - call initialize_app_state() first")
    }
}

// Export APP_STATE for backward compatibility
pub static APP_STATE: AppStateRef = AppStateRef;

// Initialization function to call from main.rs
pub async fn initialize_app_state() -> Result<()> {
    let app_state = get_app_state().await;
    app_state.initialize().await?;
    Ok(())
}