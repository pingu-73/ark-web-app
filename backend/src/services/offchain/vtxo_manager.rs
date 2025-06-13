use anyhow::{Result, anyhow};
use ark_core::Vtxo;
use ark_core::server::VtxoOutPoint;
use bitcoin::Amount;
use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::RwLock;
use chrono::Utc;

#[derive(Debug, Clone)]
pub struct VtxoState {
    pub vtxo: Vtxo,
    pub outpoints: Vec<VtxoOutPoint>,
    pub total_amount: Amount,
    pub status: VtxoStatus,
    pub earliest_expiry: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VtxoStatus {
    Pending,      // Preconfirmed
    Confirmed,    // Bitcoin-finalized
    Spent,        // Used in transaction
    Expired,      // Past expiry time
}

pub struct VtxoManager {
    grpc_client: Arc<crate::services::ark_grpc::ArkGrpcService>,
    vtxo_cache: Arc<RwLock<HashMap<String, VtxoState>>>,
}

impl VtxoManager {
    pub fn new(grpc_client: Arc<crate::services::ark_grpc::ArkGrpcService>) -> Self {
        Self {
            grpc_client,
            vtxo_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get all spendable VTXOs
    pub async fn get_spendable_vtxos(&self) -> Result<Vec<VtxoState>> {
        let client = {
            let client_opt = self.grpc_client.get_ark_client();
            client_opt.as_ref().map(|c| Arc::clone(c))
        };

        if let Some(client) = client {
            match client.spendable_vtxos().await {
                Ok(vtxos) => {
                    let mut vtxo_states = Vec::new();
                    let now = Utc::now().timestamp() as u64;

                    for (outpoints, vtxo) in vtxos {
                        if outpoints.is_empty() {
                            continue;
                        }

                        let total_amount: Amount = outpoints.iter().map(|o| o.amount).sum();
                        let earliest_expiry = outpoints.iter().map(|o| o.expire_at).min().unwrap_or(0).try_into().unwrap();
                        
                        let status = if earliest_expiry <= now {
                            VtxoStatus::Expired
                        } else if outpoints.iter().any(|o| o.is_pending) {
                            VtxoStatus::Pending
                        } else {
                            VtxoStatus::Confirmed
                        };

                        let vtxo_state = VtxoState {
                            vtxo: vtxo.clone(),
                            outpoints,
                            total_amount,
                            status,
                            earliest_expiry,
                        };

                        vtxo_states.push(vtxo_state);
                    }

                    // Update cache
                    {
                        let mut cache = self.vtxo_cache.write();
                        cache.clear();
                        for state in &vtxo_states {
                            cache.insert(state.vtxo.address().to_string(), state.clone());
                        }
                    }

                    tracing::info!("Found {} spendable VTXOs", vtxo_states.len());
                    Ok(vtxo_states)
                },
                Err(e) => {
                    tracing::error!("Failed to get spendable VTXOs: {}", e);
                    Err(anyhow!("Failed to get spendable VTXOs: {}", e))
                }
            }
        } else {
            Err(anyhow!("Ark client not available"))
        }
    }

    /// Check VTXO expiry and trigger renewal if needed
    pub async fn check_expiry_and_renew(&self) -> Result<()> {
        let vtxos = self.get_spendable_vtxos().await?;
        let now = Utc::now().timestamp() as u64;
        let warning_threshold = 3600; // 1 hour before expiry

        let mut needs_renewal = false;

        for vtxo in &vtxos {
            if vtxo.status == VtxoStatus::Expired {
                tracing::error!("VTXO {} has expired!", vtxo.vtxo.address());
                continue;
            }

            let time_to_expiry = vtxo.earliest_expiry.saturating_sub(now);
            if time_to_expiry <= warning_threshold {
                tracing::warn!(
                    "VTXO {} expires in {} seconds, needs renewal",
                    vtxo.vtxo.address(),
                    time_to_expiry
                );
                needs_renewal = true;
            }
        }

        if needs_renewal {
            tracing::info!("Triggering round participation for VTXO renewal");
            match self.grpc_client.participate_in_round().await {
                Ok(Some(round_txid)) => {
                    tracing::info!("Successfully participated in round: {}", round_txid);
                },
                Ok(None) => {
                    tracing::info!("No round participation needed at this time");
                },
                Err(e) => {
                    tracing::error!("Failed to participate in round: {}", e);
                    return Err(anyhow!("Failed to renew VTXOs: {}", e));
                }
            }
        }

        Ok(())
    }

    /// Get VTXO by address
    pub fn get_vtxo_by_address(&self, address: &str) -> Option<VtxoState> {
        let cache = self.vtxo_cache.read();
        cache.get(address).cloned()
    }

    /// Get total balance breakdown
    pub fn get_balance_breakdown(&self) -> (Amount, Amount, Amount) {
        let cache = self.vtxo_cache.read();
        let mut confirmed = Amount::ZERO;
        let mut pending = Amount::ZERO;
        let mut expired = Amount::ZERO;

        for vtxo in cache.values() {
            match vtxo.status {
                VtxoStatus::Confirmed => confirmed += vtxo.total_amount,
                VtxoStatus::Pending => pending += vtxo.total_amount,
                VtxoStatus::Expired => expired += vtxo.total_amount,
                VtxoStatus::Spent => {}, // Don't count spent VTXOs
            }
        }

        (confirmed, pending, expired)
    }

    /// Mark VTXO as spent
    pub fn mark_vtxo_spent(&self, address: &str) {
        let mut cache = self.vtxo_cache.write();
        if let Some(vtxo) = cache.get_mut(address) {
            vtxo.status = VtxoStatus::Spent;
            tracing::info!("Marked VTXO {} as spent", address);
        }
    }
}

impl Clone for VtxoManager {
    fn clone(&self) -> Self {
        Self {
            grpc_client: self.grpc_client.clone(),
            vtxo_cache: self.vtxo_cache.clone(),
        }
    }
}