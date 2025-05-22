#![allow(unused_imports, unused_variables)]
use anyhow::{anyhow, Context, Result};
use ark_client::error::ErrorContext;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
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
use bitcoin::hashes::Hash;

use rand::{rng, Rng};

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

    pub async fn test_esplora_connectivity(&self) -> Result<(), anyhow::Error> {
        tracing::info!("Testing Esplora connectivity...");
        
        // get the blockchain tip hash
        match self.client.get_tip_hash().await {
            Ok(hash) => {
                tracing::info!("Esplora server is accessible, tip hash: {}", hash);
                
                // get the current height as additional verification
                match self.client.get_height().await {
                    Ok(height) => {
                        tracing::info!("Current blockchain height: {}", height);
                        Ok(())
                    },
                    Err(e) => {
                        tracing::error!("Failed to get blockchain height: {}", e);
                        Err(anyhow::anyhow!("Failed to get blockchain height: {}", e))
                    }
                }
            },
            Err(e) => {
                tracing::error!("Esplora server is not accessible: {}", e);
                Err(anyhow::anyhow!("Esplora server is not accessible: {}", e))
            }
        }
    }
}

impl Blockchain for EsploraBlockchain {
    async fn find_outpoints(&self, address: &Address) -> Result<Vec<ExplorerUtxo>, ark_client::Error> {
        let script_pubkey = address.script_pubkey();
        
        tracing::debug!("Finding outpoints for address: {}", address);
        
        // [Debug!!]: get the tip hash to verify connectivity
        match self.client.get_tip_hash().await {
            Ok(hash) => {
                tracing::debug!("Esplora server is accessible, tip hash: {}", hash);
            },
            Err(e) => {
                tracing::warn!("Esplora server connectivity check failed: {}", e);
                // return an empty list instead of failing
                return Ok(Vec::new());
            }
        }
        
        // get address stats (lighter call)
        match self.client.get_address_stats(address).await {
            Ok(stats) => {
                // log stats using the actual fields available in AddressStats
                tracing::debug!("Address stats for {}: chain_stats: {:?}, mempool_stats: {:?}", address, stats.chain_stats, stats.mempool_stats);
                
                // check if there are any tx
                if stats.chain_stats.tx_count == 0 && stats.mempool_stats.tx_count == 0 {
                    tracing::debug!("No transactions for address {}", address);
                    return Ok(Vec::new());
                }
            },
            Err(e) => {
                tracing::warn!("Failed to get address stats: {}", e);
                // Continue anyway, as we'll try to get tx directly
            }
        }
        
        // get tx
        match self.client.scripthash_txs(&script_pubkey, None).await {
            Ok(txs) => {
                tracing::debug!("Successfully fetched {} transactions for address {}", txs.len(), address);
                
                let mut utxos = Vec::new();
                for tx in txs {
                    for (vout, output) in tx.vout.iter().enumerate() {
                        if output.scriptpubkey == script_pubkey {
                            let outpoint = bitcoin::OutPoint {
                                txid: tx.txid,
                                vout: vout as u32,
                            };
                            
                            // check if output is spent
                            let is_spent = match self.client.get_output_status(&tx.txid, vout as u64).await {
                                Ok(Some(status)) => status.spent,
                                Ok(None) => false,
                                Err(e) => {
                                    tracing::warn!("Error checking output status: {}, assuming unspent", e);
                                    false
                                }
                            };
                            
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
            },
            Err(e) => {
                if e.to_string().contains("expected value") {
                    tracing::warn!("Got 'expected value' error for address {}, this might be a new address with no transactions", address);
                    return Ok(Vec::new());
                }
                
                // Handle 404 errors (no transactions for this address)
                if e.to_string().contains("404") {
                    tracing::debug!("No transactions found for address {} (404)", address);
                    return Ok(Vec::new());
                }
                
                // For other errors, log and return empty list
                tracing::error!("Error fetching transactions for address {}: {}", address, e);
                Ok(Vec::new())
            }
        }
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
    // boarding_outputs: Mutex<Vec<BoardingOutput>>,
    boarding_outputs: RwLock<Vec<BoardingOutput>>,
    secret_keys: Mutex<std::collections::HashMap<String, SecretKey>>,
}

impl ArkWallet {
    pub fn new(keypair: Keypair, network: Network) -> Self {
        let secp = Secp256k1::new();
        Self {
            keypair,
            secp,
            network,
            // boarding_outputs: Mutex::new(Vec::new()),
            boarding_outputs: RwLock::new(Vec::new()),
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
        
        tokio::task::block_in_place(|| { // to run async func in sync context
            // get a handle to current runtime
            let rt = tokio::runtime::Handle::current();
            
            // run async lock operations on runtime
            rt.block_on(async {
                let mut secret_keys = self.secret_keys.lock().await;
                secret_keys.insert(owner_pk.to_string(), sk);
                
                // let mut boarding_outputs = self.boarding_outputs.lock().await;
                let mut boarding_outputs = self.boarding_outputs.write().await;
                boarding_outputs.push(boarding_output.clone());
            });
        });
        
        tracing::info!("Created boarding output with address: {}", boarding_output.address());
        Ok(boarding_output)
    }


    // [FIX!!] b'coz cannot block the current thread from within a runtime when participating in rounds.
    // fn get_boarding_outputs(&self) -> Result<Vec<BoardingOutput>, ark_client::Error> {
    //     let boarding_outputs = self.boarding_outputs.blocking_lock();
    //     Ok(boarding_outputs.clone())
    // }
    fn get_boarding_outputs(&self) -> Result<Vec<BoardingOutput>, ark_client::Error> {
        match self.boarding_outputs.try_read() {
            Ok(guard) => Ok(guard.clone()),
            Err(_) => Err(ark_client::Error::wallet(anyhow!("Failed to acquire read lock for boarding outputs")))
        }
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
                match self.init_ark_client_with_retry(server_url).await {
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
        
        let esplora_url = std::env::var("ESPLORA_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
        
        tracing::info!("Using network: {}, esplora: {}, ark server: {}", network, esplora_url, server_url);
        
        // create or load keypair
        let keypair = self.load_or_create_keypair()?;
        
        // initialize blockchain and wallet impls
        let blockchain = Arc::new(EsploraBlockchain::new(&esplora_url)?);
        match blockchain.test_esplora_connectivity().await {
            Ok(_) => tracing::info!("Esplora connectivity test passed"),
            Err(e) => tracing::warn!("Esplora connectivity test failed: {}", e),
        }
        let wallet = Arc::new(ArkWallet::new(keypair.clone(), network));
        

        let offline_client = OfflineClient::new(
            "ark-web-app".to_string(),
            keypair,
            blockchain,
            wallet,
            server_url.to_string(),
        );
        
        tracing::debug!(
            "Attempting to connect with: network={:?}, keypair_pubkey={}, server_url={}",
            network,
            keypair.public_key(),
            server_url
        );
        // connect to Ark server and get server info
        tracing::info!("Connecting to Ark server...");
        match offline_client.connect().await {
            Ok(client) => {
                tracing::info!("Successfully connected to Ark server");
                let server_info = client.server_info.clone();
                tracing::info!(
                    "Server info: network={:?}, pk={}, exit_delay={}",
                    server_info.network,
                    server_info.pk,
                    server_info.unilateral_exit_delay
                );
                let mut ark_client = self.ark_client.lock().await;
                *ark_client = Some(client);
                Ok(())
            },
            Err(e) => {
                tracing::error!("Failed to connect to Ark server: {} (type: {})", e, std::any::type_name_of_val(&e));
                Err(anyhow::anyhow!("Failed to connect to Ark server: {}", e))
            }
        }
    }

    async fn init_ark_client_with_retry(&mut self, server_url: &str) -> Result<()> {
        let max_retries = 3;
        let mut retries = 0;
        
        while retries < max_retries {
            tracing::info!("Attempt {} to initialize Ark client", retries + 1);
            
            // create a new gRPC client for each attempt
            let mut grpc_client = ArkGrpcClient::new(server_url.to_string());
            
            // try to connect the gRPC client first
            match grpc_client.connect().await {
                Ok(_) => {
                    tracing::info!("gRPC connection successful");
                    
                    // try to get server info directly
                    match grpc_client.get_info().await {
                        Ok(info) => {
                            tracing::info!("Successfully got server info: {:?}", info);
                            
                            // try the full client initialization
                            let network = Network::Regtest;
                            let esplora_url = std::env::var("ESPLORA_URL")
                                .unwrap_or_else(|_| "http://localhost:3000".to_string());
                            
                            let keypair = self.load_or_create_keypair()?;
                            let blockchain = Arc::new(EsploraBlockchain::new(&esplora_url)?);
                            match blockchain.test_esplora_connectivity().await {
                                Ok(_) => tracing::info!("Esplora connectivity test passed"),
                                Err(e) => tracing::warn!("Esplora connectivity test failed: {}", e),
                            }
                            let wallet = Arc::new(ArkWallet::new(keypair.clone(), network));
                            
                            let offline_client = OfflineClient::new(
                                "ark-web-app".to_string(),
                                keypair,
                                blockchain,
                                wallet,
                                server_url.to_string(),
                            );
                            
                            match offline_client.connect().await {
                                Ok(client) => {
                                    tracing::info!("Successfully initialized Ark client");
                                    let mut ark_client = self.ark_client.lock().await;
                                    *ark_client = Some(client);
                                    return Ok(());
                                },
                                Err(e) => {
                                    tracing::error!("Failed to initialize Ark client: {}", e);
                                    // Continue to retry
                                }
                            }
                        },
                        Err(e) => {
                            tracing::error!("Failed to get server info: {}", e);
                            // Continue to retry
                        }
                    }
                },
                Err(e) => {
                    tracing::error!("Failed to connect gRPC client: {}", e);
                    // Continue to retry
                }
            }
            
            retries += 1;
            if retries < max_retries {
                let delay = std::time::Duration::from_secs(2 * retries as u64);
                tracing::info!("Retrying in {} seconds...", delay.as_secs());
                tokio::time::sleep(delay).await;
            }
        }
        
        Err(anyhow::anyhow!("Failed to initialize Ark client after {} attempts", max_retries))
    }

    async fn check_server_status(&self, server_url: &str) -> Result<()> {
        let mut grpc_client = ArkGrpcClient::new(server_url.to_string());
        
        match grpc_client.connect().await {
            Ok(_) => {
                tracing::info!("gRPC connection successful");
                
                match grpc_client.get_info().await {
                    Ok(info) => {
                        tracing::info!("Server info: {:?}", info);
                        Ok(())
                    },
                    Err(e) => {
                        tracing::error!("Failed to get server info: {}", e);
                        Err(anyhow::anyhow!("Failed to get server info: {}", e))
                    }
                }
            },
            Err(e) => {
                tracing::error!("Failed to connect gRPC client: {}", e);
                Err(anyhow::anyhow!("Failed to connect gRPC client: {}", e))
            }
        }
    }
    
    fn load_or_create_keypair(&self) -> Result<Keypair> {
        // use the key manager from APP_STATE
        let (keypair, _) = crate::services::APP_STATE.key_manager.load_or_create_wallet()?;
        
        tracing::info!("Loaded keypair with public key: {}", keypair.public_key());
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
            Ok(())
        }
        else {
            tracing::warn!("Cannot update app state: Ark client not initialized");
            Err(anyhow::anyhow!("Ark client not initialized"))
        }
    }
    
    pub async fn get_address(&self) -> Result<String> {
        let client_opt = self.get_ark_client().await?;
        
        if let Some(client) = client_opt.as_ref() {
            match client.get_offchain_address() {
                Ok((address, _)) => {
                    tracing::info!("Got real offchain address: {}", address);
                    return Ok(address.to_string());
                },
                Err(e) => {
                    tracing::warn!("Failed to get offchain address: {}", e);
                    // Fall through to fallback
                }
            }
        }
        
        // fallback if client unavailable
        tracing::warn!("Using fallback dummy address");
        Ok("ark1dummy123456789".to_string())
    }
    
    pub async fn get_boarding_address(&self) -> Result<String> {
        // get a clone of the mutex guard
        let ark_client_mutex = self.ark_client.clone();
        
        // spawn a blocking task that will acquire the lock and use the client
        let address_result = tokio::task::spawn_blocking(move || {
            // inside blocking task, we can safely use blocking_lock()
            let guard = ark_client_mutex.blocking_lock();
            
            if let Some(client) = guard.as_ref() {
                // call sync method
                match client.get_boarding_address() {
                    Ok(address) => Ok(address.to_string()),
                    Err(e) => Err(anyhow::anyhow!("Failed to get boarding address: {}", e))
                }
            } else {
                // fallback if client unavailable
                Ok("bcrt1dummy123456789".to_string())
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("Join error: {}", e))??;
        
        Ok(address_result)
    }
    
    pub async fn send_vtxo(&self, address_str: String, amount: u64) -> Result<String> {
        let client_opt = self.get_ark_client().await?;
        
        if let Some(client) = client_opt.as_ref() {
            tracing::info!("Parsing address: {}", address_str);
            
            let address = match ArkAddress::decode(&address_str) {
                Ok(addr) => { 
                    tracing::info!("Successfully parsed Ark address: {}", addr);
                    addr
                },
                Err(e) => {
                    tracing::error!("Failed to parse address '{}': {}", address_str, e);

                    return Err(anyhow::anyhow!("parsing failed: {}", e));
                }
            };
            
            let amount = Amount::from_sat(amount);
            
            tracing::info!("Sending {} sats to {}", amount.to_sat(), address_str);
            
            match client.send_vtxo(address, amount).await {
                Ok(psbt) => {
                    match psbt.extract_tx() {
                        Ok(tx) => {
                            let txid = tx.compute_txid();
                            tracing::info!("Successfully sent VTXO with txid: {}", txid);
                            
                            // update app state after sending
                            if let Err(e) = self.update_app_state().await {
                                tracing::warn!("Failed to update app state after sending: {}", e);
                            }
                            
                            Ok(txid.to_string())
                        },
                        Err(e) => {
                            tracing::error!("Failed to extract transaction from PSBT: {}", e);
                            Err(anyhow::anyhow!("Failed to extract transaction: {}", e))
                        }
                    }
                },
                Err(e) => {
                    tracing::error!("Failed to send VTXO: {}", e);
                    Err(anyhow::anyhow!("Failed to send vtxo: {}", e))
                }
            }
        } 
        else {
            tracing::warn!("Ark client not available, using fallback");
            
            // fallback if client unavailable
            let txid = format!("tx_{}_{}", chrono::Utc::now().timestamp(), rand::random::<u32>());
            Ok(txid)
        }
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
        tracing::info!("ArkGrpcService: Starting to fetch transaction history");
        
        let timeout_duration = std::time::Duration::from_secs(5);
        
        let client_opt = self.get_ark_client().await?;
        tracing::info!("ArkGrpcService: Acquired Ark client lock");
        
        if let Some(client) = client_opt.as_ref() {
            // update app state with a timeout
            let update_future = self.update_app_state();
            match tokio::time::timeout(timeout_duration, update_future).await {
                Ok(update_result) => {
                    match update_result {
                        Ok(_) => tracing::info!("ArkGrpcService: Successfully updated app state"),
                        Err(e) => tracing::warn!("ArkGrpcService: Failed to update app state: {}", e),
                    }
                },
                Err(_) => {
                    tracing::warn!("ArkGrpcService: Timeout while updating app state");
                }
            }
            
            // get tx from app state
            let transactions = crate::services::APP_STATE.transactions.lock().await;
            tracing::info!("ArkGrpcService: Retrieved {} transactions from app state", transactions.len());
            
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
        
        // fallback if client unavailable
        tracing::info!("ArkGrpcService: Using fallback transaction data");
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

    pub async fn send_on_chain(&self, bitcoin_address: bitcoin::Address, amount: u64) -> Result<Txid> {
        let client_opt = self.get_ark_client().await?;
        
        if let Some(client) = client_opt.as_ref() {
            tracing::info!("Sending {} sats on-chain to {}", amount, bitcoin_address);
            
            // convert amount to Bitcoin Amount
            let amount = Amount::from_sat(amount);
            
            // send on-chain
            match client.send_on_chain(bitcoin_address, amount).await {
                Ok(txid) => {
                    tracing::info!("Successfully sent on-chain with txid: {}", txid);
                    
                    // update app state after sending
                    if let Err(e) = self.update_app_state().await {
                        tracing::warn!("Failed to update app state after sending: {}", e);
                    }
                    
                    Ok(txid)
                },
                Err(e) => {
                    tracing::error!("Failed to send on-chain: {}", e);
                    Err(anyhow::anyhow!("Failed to send on-chain: {}", e))
                }
            }
        } else {
            tracing::warn!("Ark client not available, using fallback");
            // fallback if client unavailable (generate a random txid)
            let mut rng = rand::rng();
            let random_bytes: [u8; 32] = rng.random();
            let txid = Txid::from_slice(&random_bytes)
                .map_err(|e| anyhow::anyhow!("Failed to create random txid: {}", e))?;
            
            Ok(txid)
        }
    }
}