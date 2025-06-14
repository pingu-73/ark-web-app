use anyhow::{Result, anyhow};
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::HashMap;
use uuid::Uuid;
use bitcoin::key::Keypair;
use bitcoin::Network;
use std::str::FromStr;
use ark_client::Blockchain;
use crate::services::onchain::fee_estimator::FeePriority;


#[derive(Clone)]
pub struct WalletInstance {
    pub wallet_id: String,
    pub name: String,
    pub keypair: Keypair,
    pub grpc_client: Arc<crate::services::ark_grpc::ArkGrpcService>,
    pub offchain_service: Arc<crate::services::offchain::ArkOffChainService>,
    pub created_at: i64,
}

impl WalletInstance {
    /// Get on-chain balance for this wallet
    pub async fn get_onchain_balance(&self) -> Result<u64> {
        let address = self.grpc_client.get_onchain_address().await?;
        
        let esplora_url = std::env::var("ESPLORA_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());
        let blockchain = Arc::new(crate::services::ark_grpc::EsploraBlockchain::new(&esplora_url)?);
        
        let bitcoin_address = bitcoin::Address::from_str(&address)?
            .assume_checked();
        
        let utxos = blockchain.find_outpoints(&bitcoin_address).await
            .map_err(|e| anyhow!("Failed to find UTXOs: {}", e))?;
        
        let total: u64 = utxos.iter()
            .filter(|u| !u.is_spent)
            .map(|u| u.amount.to_sat())
            .sum();
        
        Ok(total)
    }

    /// Send on-chain payment
    pub async fn send_onchain_payment(
        &self,
        address: String,
        amount: u64,
        priority: String,
    ) -> Result<String> {
        let bitcoin_address = bitcoin::Address::from_str(&address)?
            .assume_checked();
        
        let esplora_url = std::env::var("ESPLORA_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());
        let blockchain = Arc::new(crate::services::ark_grpc::EsploraBlockchain::new(&esplora_url)?);
        
        let payment_service = crate::services::onchain::OnChainPaymentService::new(blockchain);
        
        let fee_priority = match priority.as_str() {
            "fastest" => FeePriority::Fastest,
            "fast" => FeePriority::Fast,
            "slow" => FeePriority::Slow,
            _ => FeePriority::Normal,
        };
        
        let fee_rate = payment_service.fee_estimator
            .estimate_fee_for_priority(fee_priority)
            .await?;
        
        let amount = bitcoin::Amount::from_sat(amount);
        let txid = payment_service.send_payment(bitcoin_address, amount, Some(fee_rate)).await?;
        
        Ok(txid.to_string())
    }

    /// Get fee estimates
    pub async fn get_fee_estimates(&self) -> Result<crate::services::onchain::fee_estimator::FeeEstimates> {
        let esplora_url = std::env::var("ESPLORA_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());
        let blockchain = Arc::new(crate::services::ark_grpc::EsploraBlockchain::new(&esplora_url)?);
        
        let fee_estimator = crate::services::onchain::fee_estimator::FeeEstimator::new(blockchain);
        fee_estimator.get_fee_estimates().await
    }

    /// Estimate on-chain transaction fee
    pub async fn estimate_onchain_fee(
        &self,
        address: String,
        amount: u64,
    ) -> Result<serde_json::Value> {
        let bitcoin_address = bitcoin::Address::from_str(&address)?
            .assume_checked();

            let esplora_url = std::env::var("ESPLORA_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());
        let blockchain = Arc::new(crate::services::ark_grpc::EsploraBlockchain::new(&esplora_url)?);
        
        let payment_service = crate::services::onchain::OnChainPaymentService::new(blockchain);
        let amount = bitcoin::Amount::from_sat(amount);
        
        let fee = payment_service.estimate_fee(bitcoin_address, amount).await?;
        
        Ok(serde_json::json!({
            "estimated_fee": fee.to_sat(),
            "amount": amount.to_sat(),
            "total": (amount + fee).to_sat(),
        }))
    }

    /// Get transaction history
    pub async fn get_transaction_history(&self) -> Result<Vec<serde_json::Value>> {
        // Get Ark transactions
        let ark_transactions = match self.grpc_client.get_transaction_history().await {
            Ok(txs) => txs,
            Err(e) => {
                tracing::warn!("Failed to get Ark transactions: {}", e);
                vec![]
            }
        };
        
        let mut all_transactions = Vec::new();
        
        for (txid, amount, timestamp, type_name, is_settled) in ark_transactions {
            all_transactions.push(serde_json::json!({
                "txid": txid,
                "amount": amount,
                "timestamp": timestamp,
                "type": type_name,
                "is_settled": is_settled,
            }));
        }
        
        // Sort by timestamp
        all_transactions.sort_by(|a, b| {
            let ts_a = a.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
            let ts_b = b.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
            ts_b.cmp(&ts_a)
        });
        
        Ok(all_transactions)
    }
}

pub struct MultiWalletManager {
    wallets: Arc<RwLock<HashMap<String, WalletInstance>>>,
    db_manager: Arc<crate::storage::DbManager>,
    network: Network,
    ark_server_url: String,
}

impl MultiWalletManager {
    pub fn new(
        db_manager: Arc<crate::storage::DbManager>,
        network: Network,
        ark_server_url: String,
    ) -> Self {
        Self {
            wallets: Arc::new(RwLock::new(HashMap::new())),
            db_manager,
            network,
            ark_server_url,
        }
    }

    pub async fn create_wallet(&self, name: String) -> Result<WalletInfo> {
        let wallet_id = Uuid::new_v4().to_string();
        
        let secp = bitcoin::secp256k1::Secp256k1::new();
        let (secret_key, _) = secp.generate_keypair(&mut bitcoin::secp256k1::rand::thread_rng());
        let keypair = Keypair::from_secret_key(&secp, &secret_key);
        
        let mut grpc_client = crate::services::ark_grpc::ArkGrpcService::new();
        grpc_client.connect(&self.ark_server_url).await?;
        let grpc_client = Arc::new(grpc_client);
        
        let offchain_service = Arc::new(
            crate::services::offchain::ArkOffChainService::new(grpc_client.clone())
        );
        
        self.store_wallet_in_db(&wallet_id, &name, &keypair).await?;
        
        let wallet_instance = WalletInstance {
            wallet_id: wallet_id.clone(),
            name: name.clone(),
            keypair,
            grpc_client,
            offchain_service,
            created_at: chrono::Utc::now().timestamp(),
        };
        
        // add to memory
        self.wallets.write().insert(wallet_id.clone(), wallet_instance);
        
        let addresses = self.get_wallet_addresses(&wallet_id).await?;
        
        Ok(WalletInfo {
            wallet_id,
            name,
            addresses,
            created_at: chrono::Utc::now().timestamp(),
        })
    }

    pub async fn get_wallet(&self, wallet_id: &str) -> Result<Arc<WalletInstance>> {
        {
            let wallets = self.wallets.read();
            if let Some(wallet) = wallets.get(wallet_id) {
                return Ok(Arc::new(WalletInstance {
                    wallet_id: wallet.wallet_id.clone(),
                    name: wallet.name.clone(),
                    keypair: wallet.keypair.clone(),
                    grpc_client: wallet.grpc_client.clone(),
                    offchain_service: wallet.offchain_service.clone(),
                    created_at: wallet.created_at,
                }));
            }
        }
        
        self.load_wallet_from_db(wallet_id).await
    }

    pub async fn list_wallets(&self) -> Result<Vec<WalletInfo>> {
        let wallet_data = {
            let conn = self.db_manager.get_conn().await?;
            
            let mut stmt = conn.prepare(
                "SELECT wallet_id, name, created_at FROM wallets WHERE is_active = 1"
            )?;
            
            let wallets = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?, // wallet_id
                    row.get::<_, String>(1)?, // name  
                    row.get::<_, i64>(2)?,    // created_at
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
            
            wallets
        }; // conn and stmt dropped
        
        // async operations after db is done
        let mut result = Vec::new();
        for (wallet_id, name, created_at) in wallet_data {
            let addresses = self.get_wallet_addresses(&wallet_id).await?;
            
            result.push(WalletInfo {
                wallet_id,
                name,
                addresses,
                created_at,
            });
        }
        
        Ok(result)
    }

    pub async fn get_wallet_addresses(&self, wallet_id: &str) -> Result<WalletAddresses> {
        let wallet = self.get_wallet(wallet_id).await?;
        
        let onchain = wallet.grpc_client.get_onchain_address().await?;
        let offchain = wallet.grpc_client.get_address().await?;
        let boarding = wallet.grpc_client.get_boarding_address().await?;
        
        Ok(WalletAddresses {
            onchain,
            offchain,
            boarding,
        })
    }

    async fn store_wallet_in_db(
        &self,
        wallet_id: &str,
        name: &str,
        keypair: &Keypair,
    ) -> Result<()> {
        let conn = self.db_manager.get_conn().await?;
        
        // store wallet info
        conn.execute(
            "INSERT INTO wallets (wallet_id, name, created_at) VALUES (?, ?, ?)",
            rusqlite::params![
                wallet_id,
                name,
                chrono::Utc::now().timestamp(),
            ],
        )?;
        
        // [TODO!!!] store encrypted seed 
        let seed_hex = hex::encode(keypair.secret_key().secret_bytes());
        let pubkey_hex = hex::encode(keypair.public_key().serialize());
        
        conn.execute(
            "INSERT INTO wallet_keys (wallet_id, encrypted_seed, public_key) VALUES (?, ?, ?)",
            rusqlite::params![wallet_id, seed_hex, pubkey_hex],
        )?;
        
        Ok(())
    }

    async fn load_wallet_from_db(&self, wallet_id: &str) -> Result<Arc<WalletInstance>> {
        let conn = self.db_manager.get_conn().await?;
        
        // wallet info
        let (name, created_at): (String, i64) = conn.query_row(
            "SELECT name, created_at FROM wallets WHERE wallet_id = ?",
            [wallet_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        
        // keypair
        let seed_hex: String = conn.query_row(
            "SELECT encrypted_seed FROM wallet_keys WHERE wallet_id = ?",
            [wallet_id],
            |row| row.get(0),
        )?;
        
        let secret_key = bitcoin::secp256k1::SecretKey::from_slice(
            &hex::decode(seed_hex)?
        )?;
        let secp = bitcoin::secp256k1::Secp256k1::new();
        let keypair = Keypair::from_secret_key(&secp, &secret_key);
        
        // create services
        let mut grpc_client = crate::services::ark_grpc::ArkGrpcService::new();
        grpc_client.connect(&self.ark_server_url).await?;
        let grpc_client = Arc::new(grpc_client);
        
        let offchain_service = Arc::new(
            crate::services::offchain::ArkOffChainService::new(grpc_client.clone())
        );
        
        let wallet_instance = WalletInstance {
            wallet_id: wallet_id.to_string(),
            name,
            keypair,
            grpc_client,
            offchain_service,
            created_at,
        };
        
        // cache it
        self.wallets.write().insert(wallet_id.to_string(), wallet_instance.clone());
        
        Ok(Arc::new(wallet_instance))
    }
}



#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WalletInfo {
    pub wallet_id: String,
    pub name: String,
    pub addresses: WalletAddresses,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct WalletAddresses {
    pub onchain: String,
    pub offchain: String,
    pub boarding: String,
}