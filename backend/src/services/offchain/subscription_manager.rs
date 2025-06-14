use anyhow::{Result, anyhow};
use futures::Stream;
use std::sync::Arc;
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

pub struct SubscriptionManager {
    grpc_client: Arc<crate::services::ark_grpc::ArkGrpcService>,
}

impl SubscriptionManager {
    pub fn new(grpc_client: Arc<crate::services::ark_grpc::ArkGrpcService>) -> Self {
        Self { grpc_client }
    }

    pub async fn subscribe_to_scripts(&self) -> Result<Pin<Box<dyn Stream<Item = ScriptUpdate> + Send>>> {
        // [TODO!!]
        tracing::info!("Starting script subscription");
        
        let (tx, rx) = mpsc::channel(100);
        let grpc_client = self.grpc_client.clone();
        
        // spawn bg task to simulate script updates
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                
                // check for updates
                if let Ok(updates) = Self::check_script_updates(&grpc_client).await {
                    for update in updates {
                        if tx.send(update).await.is_err() {
                            break; // receiver dropped
                        }
                    }
                }
            }
        });
        
        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    pub async fn list_vtxos(&self, address: &str) -> Result<Vec<VtxoInfo>> {
        tracing::debug!("Listing VTXOs for address: {}", address);
        
        let client = {
            let client_opt = self.grpc_client.get_ark_client();
            client_opt.as_ref().map(|c| Arc::clone(c))
        };

        if let Some(client) = client {
            match client.spendable_vtxos().await {
                Ok(vtxos) => {
                    let mut vtxo_infos = Vec::new();
                    
                    for (outpoints, vtxo) in vtxos {
                        // filter by address
                        if address.is_empty() || vtxo.address().to_string() == address {
                            for outpoint in outpoints {
                                vtxo_infos.push(VtxoInfo {
                                    outpoint: outpoint.outpoint.to_string(),
                                    amount: outpoint.amount.to_sat(),
                                    status: if outpoint.is_pending { "pending".to_string() } else { "confirmed".to_string() },
                                    expiry: outpoint.expire_at.try_into().unwrap(),
                                    address: vtxo.address().to_string(),
                                });
                            }
                        }
                    }
                    
                    tracing::debug!("Found {} VTXOs", vtxo_infos.len());
                    Ok(vtxo_infos)
                },
                Err(e) => {
                    tracing::error!("Failed to list VTXOs: {}", e);
                    Err(anyhow!("Failed to list VTXOs: {}", e))
                }
            }
        } else {
            Err(anyhow!("Ark client not available"))
        }
    }

    pub async fn get_vtxo_updates(&self, address: &str) -> Result<Vec<VtxoUpdate>> {
        let vtxos = self.list_vtxos(address).await?;
        let mut updates = Vec::new();
        
        for vtxo in vtxos {
            updates.push(VtxoUpdate {
                vtxo_info: vtxo,
                update_type: VtxoUpdateType::Status,
                timestamp: chrono::Utc::now().timestamp() as u64,
            });
        }
        
        Ok(updates)
    }

    pub async fn monitor_vtxo_changes(&self) -> Result<Pin<Box<dyn Stream<Item = VtxoUpdate> + Send>>> {
        let (tx, rx) = mpsc::channel(100);
        let grpc_client = self.grpc_client.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            let mut last_state = std::collections::HashMap::new();
            
            loop {
                interval.tick().await;
                
                // current VTXO state
                if let Ok(current_vtxos) = Self::get_current_vtxo_state(&grpc_client).await {
                    // cmp with last state and send updates
                    for (key, vtxo) in &current_vtxos {
                        if let Some(last_vtxo) = last_state.get(key) {
                            if vtxo != last_vtxo {
                                let update = VtxoUpdate {
                                    vtxo_info: vtxo.clone(),
                                    update_type: VtxoUpdateType::Status,
                                    timestamp: chrono::Utc::now().timestamp() as u64,
                                };
                                
                                if tx.send(update).await.is_err() {
                                    break;
                                }
                            }
                        } else {
                            // new VTXO
                            let update = VtxoUpdate {
                                vtxo_info: vtxo.clone(),
                                update_type: VtxoUpdateType::Created,
                                timestamp: chrono::Utc::now().timestamp() as u64,
                            };
                            
                            if tx.send(update).await.is_err() {
                                break;
                            }
                        }
                    }
                    
                    // check for removed VTXOs
                    for (key, vtxo) in &last_state {
                        if !current_vtxos.contains_key(key) {
                            let update = VtxoUpdate {
                                vtxo_info: vtxo.clone(),
                                update_type: VtxoUpdateType::Spent,
                                timestamp: chrono::Utc::now().timestamp() as u64,
                            };
                            
                            if tx.send(update).await.is_err() {
                                break;
                            }
                        }
                    }
                    
                    last_state = current_vtxos;
                }
            }
        });
        
        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    async fn check_script_updates(grpc_client: &Arc<crate::services::ark_grpc::ArkGrpcService>) -> Result<Vec<ScriptUpdate>> {
        // [TODO!!!]
        Ok(Vec::new())
    }

    async fn get_current_vtxo_state(grpc_client: &Arc<crate::services::ark_grpc::ArkGrpcService>) -> Result<std::collections::HashMap<String, VtxoInfo>> {
        let mut state = std::collections::HashMap::new();
        
        let client = {
            let client_opt = grpc_client.get_ark_client();
            client_opt.as_ref().map(|c| Arc::clone(c))
        };

        if let Some(client) = client {
            if let Ok(vtxos) = client.spendable_vtxos().await {
                for (outpoints, vtxo) in vtxos {
                    for outpoint in outpoints {
                        let key = outpoint.outpoint.to_string();
                        let vtxo_info = VtxoInfo {
                            outpoint: outpoint.outpoint.to_string(),
                            amount: outpoint.amount.to_sat(),
                            status: if outpoint.is_pending { "pending".to_string() } else { "confirmed".to_string() },
                            expiry: outpoint.expire_at.try_into().unwrap(),
                            address: vtxo.address().to_string(),
                        };
                        
                        state.insert(key, vtxo_info);
                    }
                }
            }
        }
        
        Ok(state)
    }

    pub async fn subscribe_to_balance_changes(&self) -> Result<Pin<Box<dyn Stream<Item = BalanceUpdate> + Send>>> {
        let (tx, rx) = mpsc::channel(100);
        let grpc_client = self.grpc_client.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
            let mut last_balance = (bitcoin::Amount::ZERO, bitcoin::Amount::ZERO);
            
            loop {
                interval.tick().await;
                
                // current balance
                if let Ok((confirmed, pending)) = Self::get_current_balance(&grpc_client).await {
                    if (confirmed, pending) != last_balance {
                        let update = BalanceUpdate {
                            confirmed_sats: confirmed.to_sat(),
                            pending_sats: pending.to_sat(),
                            total_sats: (confirmed + pending).to_sat(),
                            timestamp: chrono::Utc::now().timestamp() as u64,
                        };
                        
                        if tx.send(update).await.is_err() {
                            break;
                        }
                        
                        last_balance = (confirmed, pending);
                    }
                }
            }
        });
        
        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    async fn get_current_balance(grpc_client: &Arc<crate::services::ark_grpc::ArkGrpcService>) -> Result<(bitcoin::Amount, bitcoin::Amount)> {
        let client = {
            let client_opt = grpc_client.get_ark_client();
            client_opt.as_ref().map(|c| Arc::clone(c))
        };

        if let Some(client) = client {
            match client.offchain_balance().await {
                Ok(balance) => Ok((balance.confirmed(), balance.pending())),
                Err(_) => Ok((bitcoin::Amount::ZERO, bitcoin::Amount::ZERO))
            }
        } else {
            Ok((bitcoin::Amount::ZERO, bitcoin::Amount::ZERO))
        }
    }
}

impl Clone for SubscriptionManager {
    fn clone(&self) -> Self {
        Self {
            grpc_client: self.grpc_client.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScriptUpdate {
    pub script: String,
    pub status: String,
    pub amount: u64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VtxoInfo {
    pub outpoint: String,
    pub amount: u64,
    pub status: String,
    pub expiry: u64,
    pub address: String,
}

#[derive(Debug, Clone)]
pub struct VtxoUpdate {
    pub vtxo_info: VtxoInfo,
    pub update_type: VtxoUpdateType,
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub enum VtxoUpdateType {
    Created,
    Status,
    Spent,
    Expired,
}

#[derive(Debug, Clone)]
pub struct BalanceUpdate {
    pub confirmed_sats: u64,
    pub pending_sats: u64,
    pub total_sats: u64,
    pub timestamp: u64,
}