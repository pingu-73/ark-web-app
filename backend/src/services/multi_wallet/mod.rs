use crate::services::onchain::fee_estimator::FeePriority;
use anyhow::{anyhow, Result};
use ark_client::Blockchain;
use bitcoin::key::Keypair;
use bitcoin::Network;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct WalletInstance {
    pub wallet_id: String,
    pub name: String,
    pub keypair: Keypair,
    pub grpc_client: Arc<crate::services::ark_grpc::ArkGrpcService>,
    pub offchain_service: Arc<crate::services::offchain::ArkOffChainService>,
    pub created_at: i64,
    pub network: Network,
    pub server_info: Option<ark_core::server::Info>,
}

impl WalletInstance {
    pub fn get_onchain_address(&self) -> Result<String> {
        let pubkey = self.keypair.public_key();
        let pubkey_bytes = pubkey.serialize();
        let wpkh = bitcoin::key::CompressedPublicKey::from_slice(&pubkey_bytes)
            .map_err(|e| anyhow!("Failed to create WPKH: {}", e))?;
        let address = bitcoin::Address::p2wpkh(&wpkh, self.network);
        Ok(address.to_string())
    }

    pub fn get_boarding_address(&self) -> Result<String> {
        let server_info = self
            .server_info
            .as_ref()
            .ok_or_else(|| anyhow!("Server info not available"))?;

        let secp = bitcoin::secp256k1::Secp256k1::new();
        let (server_pk, _) = server_info.pk.x_only_public_key();
        let (owner_pk, _) = self.keypair.x_only_public_key();

        let boarding_output = ark_core::BoardingOutput::new(
            &secp,
            server_pk,
            owner_pk,
            server_info.unilateral_exit_delay,
            self.network,
        )?;

        Ok(boarding_output.address().to_string())
    }

    pub fn get_ark_address(&self) -> Result<String> {
        let server_info = self
            .server_info
            .as_ref()
            .ok_or_else(|| anyhow!("Server info not available"))?;

        let secp = bitcoin::secp256k1::Secp256k1::new();
        let (server_pk, _) = server_info.pk.x_only_public_key();
        let (owner_pk, _) = self.keypair.x_only_public_key();

        let vtxo = ark_core::Vtxo::new_default(
            &secp,
            server_pk,
            owner_pk,
            server_info.unilateral_exit_delay,
            self.network,
        )
        .map_err(|e| anyhow!("Failed to create VTXO: {}", e))?;

        Ok(vtxo.to_ark_address().to_string())
    }

    pub fn get_mnemonic(&self) -> Result<String> {
        // [TODO!!!]
        todo!("Store mnemonic during wallet creation")
    }

    pub fn get_private_key_hex(&self) -> String {
        hex::encode(self.keypair.secret_key().secret_bytes())
    }

    pub fn get_private_key_wif(&self, network: bitcoin::Network) -> String {
        let private_key = bitcoin::PrivateKey::new(self.keypair.secret_key(), network);
        private_key.to_wif()
    }

    /// Get on-chain balance for this wallet
    pub async fn get_onchain_balance(&self) -> Result<u64> {
        let address = self.grpc_client.get_onchain_address().await?;

        let esplora_url =
            std::env::var("ESPLORA_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
        let blockchain = Arc::new(crate::services::ark_grpc::EsploraBlockchain::new(
            &esplora_url,
        )?);

        let bitcoin_address = bitcoin::Address::from_str(&address)?.assume_checked();

        let utxos = blockchain
            .find_outpoints(&bitcoin_address)
            .await
            .map_err(|e| anyhow!("Failed to find UTXOs: {}", e))?;

        let total: u64 = utxos
            .iter()
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
        let bitcoin_address = bitcoin::Address::from_str(&address)?.assume_checked();

        let esplora_url =
            std::env::var("ESPLORA_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
        let blockchain = Arc::new(crate::services::ark_grpc::EsploraBlockchain::new(
            &esplora_url,
        )?);

        let payment_service = crate::services::onchain::OnChainPaymentService::new(blockchain);

        let fee_priority = match priority.as_str() {
            "fastest" => FeePriority::Fastest,
            "fast" => FeePriority::Fast,
            "slow" => FeePriority::Slow,
            _ => FeePriority::Normal,
        };

        let fee_rate = payment_service
            .fee_estimator
            .estimate_fee_for_priority(fee_priority)
            .await?;

        let amount = bitcoin::Amount::from_sat(amount);
        let txid = payment_service
            .send_payment(bitcoin_address, amount, Some(fee_rate))
            .await?;

        Ok(txid.to_string())
    }

    /// Get fee estimates
    pub async fn get_fee_estimates(
        &self,
    ) -> Result<crate::services::onchain::fee_estimator::FeeEstimates> {
        let esplora_url =
            std::env::var("ESPLORA_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
        let blockchain = Arc::new(crate::services::ark_grpc::EsploraBlockchain::new(
            &esplora_url,
        )?);

        let fee_estimator = crate::services::onchain::fee_estimator::FeeEstimator::new(blockchain);
        fee_estimator.get_fee_estimates().await
    }

    /// Estimate on-chain transaction fee
    pub async fn estimate_onchain_fee(
        &self,
        address: String,
        amount: u64,
    ) -> Result<serde_json::Value> {
        let bitcoin_address = bitcoin::Address::from_str(&address)?.assume_checked();

        let esplora_url =
            std::env::var("ESPLORA_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
        let blockchain = Arc::new(crate::services::ark_grpc::EsploraBlockchain::new(
            &esplora_url,
        )?);

        let payment_service = crate::services::onchain::OnChainPaymentService::new(blockchain);
        let amount = bitcoin::Amount::from_sat(amount);

        let fee = payment_service
            .estimate_fee(bitcoin_address, amount)
            .await?;

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

    pub async fn create_wallet(&self, name: String) -> Result<WalletCreationResult> {
        let wallet_id = Uuid::new_v4().to_string();

        let (mnemonic_phrase, secret_key, keypair) = {
            let mut rng = bip39::rand::thread_rng();
            let mnemonic =
                bip39::Mnemonic::generate_in_with(&mut rng, bip39::Language::English, 24)?;
            let mnemonic_phrase = mnemonic.to_string();

            let seed = mnemonic.to_seed("");
            let secp = bitcoin::secp256k1::Secp256k1::new();
            let master_key = bitcoin::bip32::Xpriv::new_master(self.network, &seed)?;
            let path = bitcoin::bip32::DerivationPath::from_str("m/84'/0'/0'/0/0")?;
            let child_key = master_key.derive_priv(&secp, &path)?;
            let secret_key =
                crate::services::SecretKey::from_slice(&child_key.private_key.secret_bytes())?;
            let keypair = Keypair::from_secret_key(&secp, &secret_key);

            (mnemonic_phrase, secret_key, keypair)
        };

        let mut grpc_client = crate::services::ark_grpc::ArkGrpcService::new();
        grpc_client.connect(&self.ark_server_url).await?;
        let grpc_client = Arc::new(grpc_client);

        let server_info = match grpc_client.get_ark_client().as_ref() {
            Some(client) => Some(client.server_info.clone()),
            None => None,
        };

        let offchain_service = Arc::new(crate::services::offchain::ArkOffChainService::new(
            grpc_client.clone(),
        ));

        self.store_wallet_in_db(&wallet_id, &name, &keypair).await?;

        let wallet_instance = WalletInstance {
            wallet_id: wallet_id.clone(),
            name: name.clone(),
            keypair,
            grpc_client,
            offchain_service,
            created_at: chrono::Utc::now().timestamp(),
            network: self.network,
            server_info,
        };

        // add to memory
        self.wallets
            .write()
            .insert(wallet_id.clone(), wallet_instance);

        let addresses = self.get_wallet_addresses(&wallet_id).await?;

        let wallet_info = WalletInfo {
            wallet_id,
            name,
            addresses,
            created_at: chrono::Utc::now().timestamp(),
        };

        Ok(WalletCreationResult {
            wallet_info,
            mnemonic: mnemonic_phrase,
            private_key_hex: hex::encode(secret_key.secret_bytes()),
            private_key_wif: bitcoin::PrivateKey::new(secret_key, self.network).to_wif(),
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
                    network: wallet.network,
                    server_info: wallet.server_info.clone(),
                }));
            }
        }

        self.load_wallet_from_db(wallet_id).await
    }

    pub async fn list_wallets(&self) -> Result<Vec<WalletInfo>> {
        let wallet_data = {
            let conn = self.db_manager.get_conn().await?;

            let mut stmt = conn
                .prepare("SELECT wallet_id, name, created_at FROM wallets WHERE is_active = 1")?;

            let wallets = stmt
                .query_map([], |row| {
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

        let onchain = wallet.get_onchain_address()?;
        let offchain = wallet.get_ark_address()?;
        let boarding = wallet.get_boarding_address()?;

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
            rusqlite::params![wallet_id, name, chrono::Utc::now().timestamp(),],
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

        let secret_key = bitcoin::secp256k1::SecretKey::from_slice(&hex::decode(seed_hex)?)?;
        let secp = bitcoin::secp256k1::Secp256k1::new();
        let keypair = Keypair::from_secret_key(&secp, &secret_key);

        // create services
        let mut grpc_client = crate::services::ark_grpc::ArkGrpcService::new();
        grpc_client.connect(&self.ark_server_url).await?;
        let grpc_client = Arc::new(grpc_client);

        let offchain_service = Arc::new(crate::services::offchain::ArkOffChainService::new(
            grpc_client.clone(),
        ));

        let server_info = match grpc_client.get_ark_client().as_ref() {
            Some(client) => Some(client.server_info.clone()),
            None => None,
        };

        let wallet_instance = WalletInstance {
            wallet_id: wallet_id.to_string(),
            name,
            keypair,
            grpc_client,
            offchain_service,
            created_at,
            network: self.network,
            server_info,
        };

        // cache it
        self.wallets
            .write()
            .insert(wallet_id.to_string(), wallet_instance.clone());

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

#[derive(Debug, serde::Serialize)]
pub struct WalletCreationResult {
    pub wallet_info: WalletInfo,
    pub mnemonic: String,
    pub private_key_hex: String,
    pub private_key_wif: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct WalletAddresses {
    pub onchain: String,
    pub offchain: String,
    pub boarding: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_private_key_methods() {
        // Create a test wallet instance
        let secp = bitcoin::secp256k1::Secp256k1::new();
        let (secret_key, _) = secp.generate_keypair(&mut bitcoin::secp256k1::rand::thread_rng());
        let keypair = bitcoin::key::Keypair::from_secret_key(&secp, &secret_key);

        let wallet = WalletInstance {
            wallet_id: "test".to_string(),
            name: "test".to_string(),
            keypair,
            grpc_client: Arc::new(crate::services::ark_grpc::ArkGrpcService::new()),
            offchain_service: Arc::new(crate::services::offchain::ArkOffChainService::new(
                Arc::new(crate::services::ark_grpc::ArkGrpcService::new()),
            )),
            created_at: 0,
            network: bitcoin::Network::Regtest,
            server_info: None,
        };

        let hex_key = wallet.get_private_key_hex();
        let wif_key = wallet.get_private_key_wif(bitcoin::Network::Regtest);

        assert!(!hex_key.is_empty());
        assert!(!wif_key.is_empty());
        assert!(wif_key.starts_with("c")); // regtest WIF prefix
    }

    #[tokio::test]
    async fn test_full_wallet_creation() {
        let test_data_dir = "./test_data";
        std::env::set_var("BITCOIN_NETWORK", "regtest");
        std::env::set_var("ARK_SERVER_URL", "http://localhost:7070");
        std::env::set_var("ESPLORA_URL", "http://localhost:3000");
        std::env::set_var("DATA_DIR", test_data_dir);

        struct TestCleanup<'a> {
            path: &'a str,
        }

        impl<'a> Drop for TestCleanup<'a> {
            fn drop(&mut self) {
                if std::path::Path::new(self.path).exists() {
                    if let Err(e) = std::fs::remove_dir_all(self.path) {
                        eprintln!(
                            "Warning: Failed to cleanup test directory {}: {}",
                            self.path, e
                        );
                    } else {
                        println!("Cleaned up test directory: {}", self.path);
                    }
                }
            }
        }

        let _cleanup = TestCleanup {
            path: test_data_dir,
        };

        crate::services::initialize_app_state().await
            .expect("Failed to initialize APP_STATE - ensure Ark server is running at http://localhost:7070");

        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_manager = Arc::new(
            crate::storage::DbManager::new(db_path.to_str().unwrap())
                .await
                .unwrap(),
        );

        let manager = MultiWalletManager::new(
            db_manager,
            bitcoin::Network::Regtest,
            "http://localhost:7070".to_string(),
        );

        let result = manager
            .create_wallet("Test Wallet".to_string())
            .await
            .expect("Failed to create wallet - ensure Ark server is running and accessible");

        // wallet creation test
        assert!(!result.mnemonic.is_empty(), "Mnemonic should not be empty");
        assert_eq!(
            result.mnemonic.split_whitespace().count(),
            24,
            "Mnemonic should have 24 words"
        );
        assert!(
            !result.private_key_hex.is_empty(),
            "Private key hex should not be empty"
        );
        assert_eq!(
            result.private_key_hex.len(),
            64,
            "Private key hex should be 64 characters"
        );
        assert!(
            !result.private_key_wif.is_empty(),
            "Private key WIF should not be empty"
        );
        assert!(
            result.private_key_wif.starts_with("c"),
            "WIF should start with regtest prefix (c), got: {}",
            result.private_key_wif
        );

        // wallet info test
        assert_eq!(result.wallet_info.name, "Test Wallet");
        assert!(!result.wallet_info.wallet_id.is_empty());
        assert!(!result.wallet_info.addresses.onchain.is_empty());
        assert!(!result.wallet_info.addresses.offchain.is_empty());
        assert!(!result.wallet_info.addresses.boarding.is_empty());

        println!("Wallet created successfully:");
        println!("  - Wallet ID: {}", result.wallet_info.wallet_id);
        println!(
            "  - Mnemonic: {} (24 words)",
            result.mnemonic.split_whitespace().count()
        );
        println!(
            "  - Private Key Hex: {}...{}",
            &result.private_key_hex[..8],
            &result.private_key_hex[56..]
        );
        println!(
            "  - Private Key WIF: {}...{}",
            &result.private_key_wif[..8],
            &result.private_key_wif[result.private_key_wif.len() - 8..]
        );
        println!(
            "  - On-chain Address: {}",
            result.wallet_info.addresses.onchain
        );
        println!(
            "  - Off-chain Address: {}",
            result.wallet_info.addresses.offchain
        );
        println!(
            "  - Boarding Address: {}",
            result.wallet_info.addresses.boarding
        );

        // wallet retrival test
        let retrieved_wallet = manager
            .get_wallet(&result.wallet_info.wallet_id)
            .await
            .expect("Should be able to retrieve created wallet");

        assert_eq!(retrieved_wallet.wallet_id, result.wallet_info.wallet_id);
        assert_eq!(retrieved_wallet.name, "Test Wallet");

        // private key methods on the retrieved wallet
        let hex_key = retrieved_wallet.get_private_key_hex();
        let wif_key = retrieved_wallet.get_private_key_wif(bitcoin::Network::Regtest);

        assert_eq!(
            hex_key, result.private_key_hex,
            "Retrieved private key hex should match"
        );
        assert_eq!(
            wif_key, result.private_key_wif,
            "Retrieved private key WIF should match"
        );

        println!("Wallet retrieval and key methods work correctly");
        //  _cleanup goes out of scope
    }
}
