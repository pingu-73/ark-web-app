#![allow(unused_imports, unused_variables)]
use anyhow::{anyhow, Context, Result};
use ark_client::error::ErrorContext;
use std::sync::Arc;
use tokio::sync::Mutex;
use once_cell::sync::OnceCell;
use std::path::Path;
use std::fs;
use std::str::FromStr;

use ark_grpc::Client as ArkGrpcClient;
use ark_client::{Client, OfflineClient, Blockchain, ExplorerUtxo, SpendStatus};
use ark_bdk_wallet::Wallet as BdkWallet;
use ark_core::{ArkAddress, ArkTransaction, BoardingOutput};

use bitcoin::key::{Keypair, Secp256k1};
use bitcoin::secp256k1::SecretKey;
use bitcoin::{Address, Amount, Network, Transaction, Txid};
use rand::rng;

// global client instance
static ARK_CLIENT: OnceCell<Arc<Mutex<Option<Client<EsploraBlockchain, ArkWallet>>>>> = OnceCell::new();

// Blockchain impl for Esplora
pub struct EsploraBlockchain {
    client: esplora_client::AsyncClient,
}

impl EsploraBlockchain {
    pub fn new(url: &str) -> Result<Self> {
        let client = esplora_client::Builder::new(url).build_async()?;
        Ok(Self { client })
    }
}

impl Blockchain for EsploraBlockchain {
    async fn find_outpoints(&self, address: &Address) -> Result<Vec<ExplorerUtxo>, ark_client::Error> {
        let script_pubkey = address.script_pubkey();
        
        tracing::debug!("Finding outpoints for address: {}", address);
        
        let txs = match self.client.scripthash_txs(&script_pubkey, None).await {
            Ok(txs) => txs,
            Err(e) => {
                tracing::error!("Error fetching transactions: {}", e);
                return Err(ark_client::Error::wallet(anyhow!("Esplora error: {}", e)));
            }
        };

        let mut utxos = Vec::new();
        for tx in txs {
            for (vout, output) in tx.vout.iter().enumerate() {
                if output.scriptpubkey == script_pubkey {
                    let outpoint = bitcoin::OutPoint {
                        txid: tx.txid,
                        vout: vout as u32,
                    };
                    
                    // ensure if output is spent
                    let status = match self.client.get_output_status(&tx.txid, vout as u64).await {
                        Ok(status) => status,
                        Err(e) => {
                            tracing::error!("Error checking output status: {}", e);
                            return Err(ark_client::Error::wallet(anyhow!("Esplora error: {}", e)));
                        }
                    };
                    
                    let is_spent = status.map(|s| s.spent).unwrap_or(false);
                    
                    utxos.push(ExplorerUtxo {
                        outpoint,
                        amount: bitcoin::Amount::from_sat(output.value),
                        confirmation_blocktime: tx.status.block_time,
                        is_spent,
                    });
                }
            }
        }
        
        tracing::debug!("Found {} outpoints for address {}", utxos.len(), address);
        Ok(utxos)
    }

    async fn find_tx(&self, txid: &Txid) -> Result<Option<Transaction>, ark_client::Error> {
        tracing::debug!("Finding transaction: {}", txid);
        
        match self.client.get_tx(txid).await {
            Ok(Some(tx)) => {
                let tx_bytes = bitcoin::consensus::serialize(&tx);
                match bitcoin::consensus::deserialize(&tx_bytes) {
                    Ok(tx) => Ok(Some(tx)),
                    Err(e) => {
                        tracing::error!("Error deserializing transaction: {}", e);
                        Err(ark_client::Error::wallet(anyhow!("Failed to deserialize transaction: {}", e)))
                    }
                }
            }
            Ok(None) => {Ok(None)}
            Err(esplora_client::Error::TransactionNotFound(_)) => {
                tracing::debug!("Transaction not found: {}", txid);
                Ok(None)
            }
            Err(e) => {
                tracing::error!("Error fetching transaction: {}", e);
                Err(ark_client::Error::wallet(anyhow!("Esplora error: {}", e)))
            }
        }
    }

    async fn get_output_status(&self, txid: &Txid, vout: u32) -> Result<SpendStatus, ark_client::Error> {
        tracing::debug!("Getting output status for {}:{}", txid, vout);
        
        let status = match self.client.get_output_status(txid, vout as u64).await {
            Ok(status) => status,
            Err(e) => {
                tracing::error!("Error getting output status: {}", e);
                return Err(ark_client::Error::wallet(anyhow!("Esplora error: {}", e)));
            }
        };
        
        Ok(SpendStatus {
            spend_txid: status.and_then(|s| s.txid),
        })
    }

    async fn broadcast(&self, tx: &Transaction) -> Result<(), ark_client::Error> {
        tracing::info!("Broadcasting transaction: {}", tx.compute_txid());
        
        let _tx_bytes = bitcoin::consensus::serialize(tx);
        match self.client.broadcast(&tx).await {
            Ok(_) => {
                tracing::info!("Successfully broadcast transaction: {}", tx.compute_txid());
                Ok(())
            }
            Err(e) => {
                tracing::error!("Error broadcasting transaction: {}", e);
                Err(ark_client::Error::wallet(anyhow!("Failed to broadcast transaction: {}", e)))
            }
        }
    }
}

// wallet impl
pub struct ArkWallet {
    keypair: Keypair,
    secp: Secp256k1<bitcoin::secp256k1::All>,
    network: Network,
    boarding_outputs: Mutex<Vec<BoardingOutput>>,
    secret_keys: Mutex<std::collections::HashMap<String, SecretKey>>,
}

impl ArkWallet {
    pub fn new(keypair: Keypair, network: Network) -> Self {
        let secp = Secp256k1::new();
        Self {
            keypair,
            secp,
            network,
            boarding_outputs: Mutex::new(Vec::new()),
            secret_keys: Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl ark_client::wallet::BoardingWallet for ArkWallet {
    fn new_boarding_output(
        &self,
        server_pk: bitcoin::XOnlyPublicKey,
        exit_delay: bitcoin::Sequence,
        network: Network,
    ) -> Result<BoardingOutput, ark_client::Error> {
        tracing::info!("Creating new boarding output");
        
        let sk = self.keypair.secret_key();
        let (owner_pk, _) = self.keypair.x_only_public_key();
        
        let boarding_output = match BoardingOutput::new(&self.secp, server_pk, owner_pk, exit_delay, network) {
            Ok(bo) => bo,
            Err(e) => {
                tracing::error!("Error creating boarding output: {}", e);
                return Err(ark_client::Error::wallet(anyhow!("Failed to create boarding output: {}", e)));
            }
        };
        
        // store secret key for this public key
        let mut secret_keys = self.secret_keys.blocking_lock();
        secret_keys.insert(owner_pk.to_string(), sk);
        
        // store boarding output
        let mut boarding_outputs = self.boarding_outputs.blocking_lock();
        boarding_outputs.push(boarding_output.clone());
        
        tracing::info!("Created boarding output with address: {}", boarding_output.address());
        Ok(boarding_output)
    }

    fn get_boarding_outputs(&self) -> Result<Vec<BoardingOutput>, ark_client::Error> {
        let boarding_outputs = self.boarding_outputs.blocking_lock();
        Ok(boarding_outputs.clone())
    }

    fn sign_for_pk(&self, pk: &bitcoin::XOnlyPublicKey, msg: &bitcoin::secp256k1::Message) -> Result<bitcoin::secp256k1::schnorr::Signature, ark_client::Error> {
        let secret_keys = self.secret_keys.blocking_lock();
        
        if let Some(sk) = secret_keys.get(&pk.to_string()) {
            let keypair = Keypair::from_secret_key(&self.secp, sk);
            let sig = self.secp.sign_schnorr_no_aux_rand(msg, &keypair);
            Ok(sig)
        } else {
            tracing::error!("No secret key found for public key: {}", pk);
            Err(ark_client::Error::wallet(anyhow!("No secret key found for public key: {}", pk)))
        }
    }
}

impl ark_client::wallet::OnchainWallet for ArkWallet {
    fn get_onchain_address(&self) -> Result<Address, ark_client::Error> {
        let pubkey = self.keypair.public_key();
        let pubkey_bytes = pubkey.serialize();
        let wpkh = bitcoin::key::CompressedPublicKey::from_slice(&pubkey_bytes)
            .map_err(|e| ark_client::Error::wallet(anyhow!("Failed to create WPKH: {}", e)))?;
        let address = bitcoin::Address::p2wpkh(&wpkh, self.network);
        
        tracing::info!("Generated onchain address: {}", address);
        Ok(address)
    }

    async fn sync(&self) -> Result<(), ark_client::Error> {
        // [TODO!!] implement a full sync
        tracing::info!("Syncing wallet (placeholder)");
        Ok(())
    }

    fn balance(&self) -> Result<ark_client::wallet::Balance, ark_client::Error> {
        // [TODO!!]
        // [Demo:] returning a placeholder balance
        Ok(ark_client::wallet::Balance {
            confirmed: Amount::from_sat(0),
            trusted_pending: Amount::from_sat(0),
            untrusted_pending: Amount::from_sat(0),
            immature: Amount::from_sat(0),
        })
    }

    fn prepare_send_to_address(
        &self,
        address: Address,
        amount: Amount,
        fee_rate: bitcoin::FeeRate,
    ) -> Result<bitcoin::Psbt, ark_client::Error> {
        // [TODO!!]
        tracing::error!("prepare_send_to_address not fully implemented");
        Err(ark_client::Error::wallet(anyhow!("Not implemented")))
    }

    fn sign(&self, psbt: &mut bitcoin::Psbt) -> Result<bool, ark_client::Error> {
        // [TODO!!]
        tracing::error!("sign not fully implemented");
        Err(ark_client::Error::wallet(anyhow!("Not implemented")))
    }
}

pub struct ArkGrpcService {
    grpc_client: Option<ArkGrpcClient>,
    ark_client: Arc<Mutex<Option<Client<EsploraBlockchain, ArkWallet>>>>,
}

impl ArkGrpcService {
    pub fn new() -> Self {
        Self { 
            grpc_client: None,
            ark_client: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn connect(&mut self, server_url: &str) -> Result<()> {
        tracing::info!("ArkGrpcService::connect: Connecting to {}", server_url);
        
        // new gRPC client with the server URL
        let mut grpc_client = ArkGrpcClient::new(server_url.to_string());
        
        // connect to server
        match grpc_client.connect().await {
            Ok(_) => {
                tracing::info!("ArkGrpcService::connect: Successfully connected to {} via gRPC", server_url);
                self.grpc_client = Some(grpc_client);
                
                // Now initialize the Ark client
                match self.init_ark_client(server_url).await {
                    Ok(_) => {
                        tracing::info!("Successfully initialized Ark client");
                    },
                    Err(e) => {
                        tracing::error!("Failed to initialize Ark client: {}", e);
                        // continue even if Ark client initialization fails
                    }
                }
                
                Ok(())
            },
            Err(e) => {
                tracing::error!("ArkGrpcService::connect: Failed to connect to {}: {}", server_url, e);
                Err(anyhow::anyhow!("Failed to connect to Ark server: {}", e))
            }
        }
    }
    
    pub fn is_connected(&self) -> bool {
        let connected = self.grpc_client.is_some();
        tracing::info!("ArkGrpcService::is_connected: {}", connected);
        connected
    }
    
    async fn init_ark_client(&mut self, server_url: &str) -> Result<()> {
        // load env vars
        let network = match std::env::var("BITCOIN_NETWORK").unwrap_or_else(|_| "regtest".to_string()).as_str() {
            "mainnet" => Network::Bitcoin,
            "testnet" => Network::Testnet,
            "signet" => Network::Signet,
            _ => Network::Regtest,
        };
        
        let esplora_url = std::env::var("ESPLORA_URL").unwrap_or_else(|_| "http://localhost:3002".to_string());
        
        tracing::info!("Using network: {}, esplora: {}, ark server: {}", network, esplora_url, server_url);
        
        // create or load keypair
        let keypair = self.load_or_create_keypair()?;
        
        // initialize blockchain and wallet impls
        let blockchain = Arc::new(EsploraBlockchain::new(&esplora_url)?);
        let wallet = Arc::new(ArkWallet::new(keypair.clone(), network));
        

        let offline_client = OfflineClient::new(
            "ark-web-app".to_string(),
            keypair,
            blockchain,
            wallet,
            server_url.to_string(),
        );
        
        // connect to Ark server and get server info
        tracing::info!("Connecting to Ark server...");
        match offline_client.connect().await {
            Ok(client) => {
                tracing::info!("Successfully connected to Ark server");
                let mut ark_client = self.ark_client.lock().await;
                *ark_client = Some(client);
                Ok(())
            },
            Err(e) => {
                tracing::error!("Failed to connect to Ark server: {}", e);
                Err(anyhow::anyhow!("Failed to connect to Ark server: {}", e))
            }
        }
    }
    
    fn load_or_create_keypair(&self) -> Result<Keypair> {
        let key_path = Path::new("./data/key.hex");
        let key_dir = key_path.parent().unwrap();
        
        if !key_dir.exists() {
            fs::create_dir_all(key_dir)?;
        }
        
        let secp = Secp256k1::new();
        
        if key_path.exists() {
            // load existing key
            tracing::info!("Loading existing keypair");
            let key_hex = fs::read_to_string(key_path)?;
            let secret_key = SecretKey::from_str(key_hex.trim())?;
            let keypair = Keypair::from_secret_key(&secp, &secret_key);
            return Ok(keypair);
        }
        
        // generate new key
        tracing::info!("Generating new keypair");
        let mut rng = bitcoin::secp256k1::rand::thread_rng();
        let keypair = Keypair::new(&secp, &mut rng);
        
        // save key [TODO!! improve]
        fs::write(key_path, keypair.secret_key().display_secret().to_string())?;
        
        Ok(keypair)
    }

    pub async fn get_ark_client<'a>(&'a self) -> Result<tokio::sync::MutexGuard<'a, Option<Client<EsploraBlockchain, ArkWallet>>>> {
        Ok(self.ark_client.lock().await)
    }

    // update app state with client info
    pub async fn update_app_state(&self) -> Result<()> {
        let client_opt = self.get_ark_client().await?;
        
        if let Some(client) = client_opt.as_ref() {
            // update app state with client info
            let mut balance = crate::services::APP_STATE.balance.lock().await;
            
            // get on-chain balance
            if let Ok(offchain_balance) = client.offchain_balance().await {
                balance.confirmed = offchain_balance.confirmed().to_sat();
                balance.trusted_pending = offchain_balance.pending().to_sat();
                balance.untrusted_pending = 0; // [TODO!!] functions not exposed
                balance.immature = 0; // [TODO!!] functions not exposed
                balance.total = offchain_balance.total().to_sat();
            }
            
            // get off-chain balance
            if let Ok(offchain_balance) = client.offchain_balance().await {
                // add off-chain balance to the total
                balance.confirmed += offchain_balance.confirmed().to_sat();
                balance.trusted_pending += offchain_balance.pending().to_sat();
                balance.total += offchain_balance.total().to_sat();
            }
            
            // update tx history
            if let Ok(history) = client.transaction_history().await {
                let mut transactions = crate::services::APP_STATE.transactions.lock().await;
                transactions.clear(); // clear existing tx
                
                for tx in history {
                    let tx_response = match tx {
                        ArkTransaction::Boarding { txid, amount, confirmed_at } => {
                            crate::models::wallet::TransactionResponse {
                                txid: txid.to_string(),
                                amount: amount.to_sat() as i64,
                                timestamp: confirmed_at.unwrap_or(chrono::Utc::now().timestamp()),
                                type_name: "Boarding".to_string(),
                                is_settled: Some(confirmed_at.is_some()),
                            }
                        },
                        ArkTransaction::Round { txid, amount, created_at } => {
                            crate::models::wallet::TransactionResponse {
                                txid: txid.to_string(),
                                amount: amount.to_sat() as i64,
                                timestamp: created_at,
                                type_name: "Round".to_string(),
                                is_settled: Some(true),
                            }
                        },
                        ArkTransaction::Redeem { txid, amount, is_settled, created_at } => {
                            crate::models::wallet::TransactionResponse {
                                txid: txid.to_string(),
                                amount: amount.to_sat() as i64,
                                timestamp: created_at,
                                type_name: "Redeem".to_string(),
                                is_settled: Some(is_settled),
                            }
                        },
                    };
                    
                    transactions.push(tx_response);
                }
            }
        }
        
        Ok(())
    }

    // get balance from Ark client
    pub async fn get_balance(&self) -> Result<(u64, u64, u64)> {
        let client_opt = self.get_ark_client().await?;

        if let Some(client) = client_opt.as_ref() {
            // get off-chain balance
            if let Ok(offchain_balance) = client.offchain_balance().await {
                return Ok((
                    offchain_balance.confirmed().to_sat(),
                    offchain_balance.pending().to_sat(),
                    offchain_balance.total().to_sat()
                ));
            }
        }
        
        // fallback if client unavailable
        Ok((100000, 50000, 150000))
    }
    
    pub async fn get_address(&self) -> Result<String> {
        let client_opt = self.get_ark_client().await?;
        
        if let Some(client) = client_opt.as_ref() {
            let (address, _) = client
                .get_offchain_address()
                .map_err(|e| anyhow!("Failed to get offchain address: {}", e))?;
            return Ok(address.to_string());
        }
        
        // fallback if client unavailable
        Ok("ark1dummy123456789".to_string())
    }
    
    pub async fn get_boarding_address(&self) -> Result<String> {
        let client_opt = self.get_ark_client().await?;
        
        if let Some(client) = client_opt.as_ref() {
            let address = client
                .get_boarding_address()
                .map_err(|e| anyhow!("Failed to get boarding address: {}", e))?;
            return Ok(address.to_string());
        }
        
        // fallback if client unavailable
        Ok("bcrt1dummy123456789".to_string())
    }
    
    pub async fn send_vtxo(&self, address_str: String, amount: u64) -> Result<String> {
        let client_opt = self.get_ark_client().await?;
        
        if let Some(client) = client_opt.as_ref() {
            let address = ArkAddress::decode(&address_str)?;
            let amount = Amount::from_sat(amount);
            
            tracing::info!("Sending {} sats to {}", amount.to_sat(), address_str);
            
            let psbt = client
                .send_vtxo(address, amount)
                .await
                .map_err(|e| anyhow!("Failed to send vtxo: {}", e))?;
            let txid = psbt.extract_tx()?.compute_txid();
            
            // Update app state after sending
            self.update_app_state().await?;
            
            return Ok(txid.to_string());
        }
        
        // fallback if client unavailable
        let txid = format!("tx_{}_{}", chrono::Utc::now().timestamp(), rand::random::<u32>());
        Ok(txid)
    }
    

    pub async fn check_deposits(&self) -> Result<bool> {
        let client_opt = self.get_ark_client().await?;
        
        if let Some(client) = client_opt.as_ref() {
            // random no for boarding process
            let mut rng = bitcoin::secp256k1::rand::thread_rng();
            
            // board any pending deposits
            tracing::info!("Checking for deposits to board");
            let result = client.board(&mut rng).await;
            
            match result {
                Ok(_) => {
                    tracing::info!("Successfully boarded deposits");
                    
                    // Update app state after boarding
                    self.update_app_state().await?;
                    
                    return Ok(true);
                },
                Err(e) => {
                    if e.to_string().contains("No boarding outputs") {
                        tracing::info!("No deposits to board");
                        return Ok(false);
                    } else {
                        tracing::error!("Error boarding deposits: {}", e);
                        return Err(anyhow::anyhow!("Error boarding deposits: {}", e));
                    }
                }
            }
        }
        
        // fallback if client unavailable
        let mut transactions = crate::services::APP_STATE.transactions.lock().await;
        transactions.push(crate::models::wallet::TransactionResponse {
            txid: format!("deposit_{}", chrono::Utc::now().timestamp()),
            amount: 100000000, // 1 BTC in satoshis
            timestamp: chrono::Utc::now().timestamp(),
            type_name: "Boarding".to_string(),
            is_settled: Some(true),
        });
        
        // recalculate balance
        drop(transactions);
        crate::services::APP_STATE.recalculate_balance().await?;
        
        Ok(true)
    }
    
    pub async fn participate_in_round(&self) -> Result<Option<String>> {
        let client_opt = self.get_ark_client().await?;
        
        if let Some(client) = client_opt.as_ref() {
            // random no. for round participation
            let mut rng = bitcoin::secp256k1::rand::thread_rng();
            
            // participate in round
            tracing::info!("Participating in a round");
            let result = client.board(&mut rng).await;
            
            match result {
                Ok(_) => {
                    tracing::info!("Successfully participated in round");
                    
                    // update app state after round participation
                    self.update_app_state().await?;
                    
                    // [TODO!! (get the round txid)] 
                    // Dummy impl
                    let placeholder_txid = format!("round_{}", chrono::Utc::now().timestamp());
                    return Ok(Some(placeholder_txid));
                },
                Err(e) => {
                    if e.to_string().contains("No boarding outputs") && e.to_string().contains("No VTXOs") {
                        tracing::info!("No outputs to include in round");
                        return Ok(None);
                    } else {
                        tracing::error!("Error participating in round: {}", e);
                        return Err(anyhow::anyhow!("Error participating in round: {}", e));
                    }
                }
            }
        }
        
        // fallback if client unavailable (simulate round participation)
        let mut transactions = crate::services::APP_STATE.transactions.lock().await;
        
        let pending_txs: Vec<_> = transactions.iter()
            .filter(|tx| tx.is_settled == Some(false))
            .collect();
        
        if pending_txs.is_empty() {
            return Ok(None);
        }
        
        // mark all pending tx as settled
        let mut settled_txids = Vec::new();
        for tx in transactions.iter_mut() {
            if tx.is_settled == Some(false) {
                tx.is_settled = Some(true);
                settled_txids.push(tx.txid.clone());
            }
        }
        
        let round_txid = format!("round_{}_{}", chrono::Utc::now().timestamp(), rand::random::<u32>());
        
        // add round tx to history
        transactions.push(crate::models::wallet::TransactionResponse {
            txid: round_txid.clone(),
            amount: 0, // rounds don't change balance directly
            timestamp: chrono::Utc::now().timestamp(),
            type_name: "Round".to_string(),
            is_settled: Some(true),
        });
        
        drop(transactions);
        
        // recalculate balance for consistency
        crate::services::APP_STATE.recalculate_balance().await?;
        
        // log settled tx
        tracing::info!(
            "Round {} settled {} transactions: {:?}",
            round_txid, settled_txids.len(), settled_txids
        );
        
        Ok(Some(round_txid))
    }
    

    pub async fn get_transaction_history(&self) -> Result<Vec<(String, i64, i64, String, bool)>> {
        let client_opt = self.get_ark_client().await?;
        
        if let Some(client) = client_opt.as_ref() {
            // update app state first
            self.update_app_state().await?;
            
            // get transactions from app state
            let transactions = crate::services::APP_STATE.transactions.lock().await;
            
            let history = transactions.iter().map(|tx| {
                (
                    tx.txid.clone(),
                    tx.amount,
                    tx.timestamp,
                    tx.type_name.clone(),
                    tx.is_settled.unwrap_or(false),
                )
            }).collect::<Vec<_>>();
            
            return Ok(history);
        }
        
        // fallback to dummy data if client unavailable
        Ok(vec![
            (
                "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
                50000,
                chrono::Utc::now().timestamp(),
                "Boarding".to_string(),
                true,
            ),
        ])
    }
    
    // [TODO!!]
    pub async fn unilateral_exit(&self, vtxo_txid: String) -> Result<crate::models::wallet::TransactionResponse> {
        // TODO!! [implment unilateral exit]
        tracing::warn!("Unilateral exit is not fully implemented yet");
        
        // [TODO!!]
        // Dummy Tx
        let exit_txid = format!("exit_{}_{}", chrono::Utc::now().timestamp(), rand::random::<u32>());
        
        let tx = crate::models::wallet::TransactionResponse {
            txid: exit_txid,
            amount: -1000, // [TODO!! (modify to calcualte fee)]
            timestamp: chrono::Utc::now().timestamp(),
            type_name: "Exit".to_string(),
            is_settled: Some(true),
        };
        
        Ok(tx)
    }
}