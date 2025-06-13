use anyhow::{Result, anyhow};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

pub struct RoundCoordinator {
    grpc_client: Arc<crate::services::ark_grpc::ArkGrpcService>,
}

impl RoundCoordinator {
    pub fn new(grpc_client: Arc<crate::services::ark_grpc::ArkGrpcService>) -> Self {
        Self { grpc_client }
    }

    /// Participate in round for batch swaps
    pub async fn participate(&self) -> Result<Option<String>> {
        tracing::info!("Starting round participation");

        // Check if we have any VTXOs or boarding outputs to include
        let has_inputs = self.check_available_inputs().await?;
        if !has_inputs {
            tracing::info!("No inputs available for round participation");
            return Ok(None);
        }

        // Participate in round with timeout
        let participation_timeout = Duration::from_secs(30);
        match timeout(participation_timeout, self.grpc_client.participate_in_round()).await {
            Ok(result) => match result {
                Ok(Some(round_txid)) => {
                    tracing::info!("Successfully participated in round: {}", round_txid);
                    
                    // Update app state after successful round participation
                    if let Err(e) = self.grpc_client.update_app_state().await {
                        tracing::warn!("Failed to update app state after round: {}", e);
                    }
                    
                    Ok(Some(round_txid))
                },
                Ok(None) => {
                    tracing::info!("No round participation needed");
                    Ok(None)
                },
                Err(e) => {
                    tracing::error!("Round participation failed: {}", e);
                    Err(anyhow!("Round participation failed: {}", e))
                }
            },
            Err(_) => {
                tracing::error!("Round participation timed out");
                Err(anyhow!("Round participation timed out"))
            }
        }
    }

    /// Check if we have boarding outputs or VTXOs to include in round
    async fn check_available_inputs(&self) -> Result<bool> {
        // Check for boarding outputs
        let has_boarding = self.check_boarding_outputs().await?;
        
        // Check for VTXOs that need renewal
        let has_vtxos = self.check_vtxos_for_renewal().await?;

        Ok(has_boarding || has_vtxos)
    }

    /// Check for available boarding outputs
    async fn check_boarding_outputs(&self) -> Result<bool> {
        // This would check for confirmed boarding transactions
        // For now, we'll use the existing check_deposits logic
        match self.grpc_client.check_deposits().await {
            Ok(has_deposits) => {
                if has_deposits {
                    tracing::info!("Found boarding outputs ready for round");
                }
                Ok(has_deposits)
            },
            Err(e) => {
                tracing::warn!("Failed to check boarding outputs: {}", e);
                Ok(false)
            }
        }
    }

    /// Check for VTXOs that need renewal
    async fn check_vtxos_for_renewal(&self) -> Result<bool> {
        let client = {
            let client_opt = self.grpc_client.get_ark_client();
            client_opt.as_ref().map(|c| Arc::clone(c))
        };

        if let Some(client) = client {
            match client.spendable_vtxos().await {
                Ok(vtxos) => {
                    let now = chrono::Utc::now().timestamp() as u64;
                    let renewal_threshold = 7200; // 2 hours before expiry

                    for (outpoints, _) in &vtxos {
                        for outpoint in outpoints {
                            let time_to_expiry = outpoint.expire_at.saturating_sub(now.try_into().unwrap());
                            if time_to_expiry <= renewal_threshold {
                                tracing::info!("Found VTXO needing renewal");
                                return Ok(true);
                            }
                        }
                    }
                    Ok(false)
                },
                Err(e) => {
                    tracing::warn!("Failed to check VTXOs: {}", e);
                    Ok(false)
                }
            }
        } else {
            Ok(false)
        }
    }

    /// Monitor round events and handle signing
    pub async fn monitor_rounds(&self) -> Result<()> {
        tracing::info!("Starting round monitoring");
        
        // This would implement continuous monitoring of round events
        // For now, we'll do a simple periodic check
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        
        loop {
            interval.tick().await;
            
            if let Err(e) = self.check_and_participate().await {
                tracing::error!("Error during round monitoring: {}", e);
            }
        }
    }

    /// Check conditions and participate if needed
    async fn check_and_participate(&self) -> Result<()> {
        // Check if we need to participate in a round
        let needs_participation = self.check_available_inputs().await?;
        
        if needs_participation {
            tracing::info!("Conditions met for round participation");
            if let Err(e) = self.participate().await {
                tracing::error!("Failed to participate in round: {}", e);
            }
        }
        
        Ok(())
    }

    /// Get round status information
    pub async fn get_round_status(&self) -> Result<RoundStatus> {
        // This would query the current round status from the server
        // For now, return a basic status
        Ok(RoundStatus {
            current_round_id: None,
            next_round_time: None,
            participants_count: 0,
            is_active: false,
        })
    }
}

impl Clone for RoundCoordinator {
    fn clone(&self) -> Self {
        Self {
            grpc_client: self.grpc_client.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RoundStatus {
    pub current_round_id: Option<String>,
    pub next_round_time: Option<u64>,
    pub participants_count: u32,
    pub is_active: bool,
}