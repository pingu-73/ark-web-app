use anyhow::{Result, anyhow};
use std::sync::Arc;
use bitcoin::Amount;
use chrono::Utc;

pub struct ExitManager {
    grpc_client: Arc<crate::services::ark_grpc::ArkGrpcService>,
}

impl ExitManager {
    pub fn new(grpc_client: Arc<crate::services::ark_grpc::ArkGrpcService>) -> Self {
        Self { grpc_client }
    }

    /// Perform unilateral exit for a VTXO
    pub async fn exit_vtxo(&self, vtxo_id: String) -> Result<String> {
        tracing::info!("Starting unilateral exit for VTXO: {}", vtxo_id);

        // Validate VTXO exists and is eligible for exit
        self.validate_exit_eligibility(&vtxo_id).await?;

        // Use the existing unilateral_exit method from ArkGrpcService
        match self.grpc_client.unilateral_exit(vtxo_id.clone()).await {
            Ok(tx) => {
                tracing::info!("Successfully initiated unilateral exit with txid: {}", tx.txid);
                
                // Update app state after exit
                if let Err(e) = self.grpc_client.update_app_state().await {
                    tracing::warn!("Failed to update app state after exit: {}", e);
                }
                
                Ok(tx.txid)
            },
            Err(e) => {
                tracing::error!("Failed to perform unilateral exit: {}", e);
                Err(anyhow!("Failed to perform unilateral exit: {}", e))
            }
        }
    }

    /// Check if unilateral exit is needed (server unresponsive)
    pub async fn check_exit_conditions(&self) -> Result<Vec<ExitRecommendation>> {
        let mut recommendations = Vec::new();

        // Check server responsiveness
        let server_responsive = self.check_server_responsiveness().await?;
        if !server_responsive {
            recommendations.push(ExitRecommendation {
                vtxo_id: "all".to_string(),
                reason: ExitReason::ServerUnresponsive,
                urgency: ExitUrgency::High,
                estimated_cost: Amount::from_sat(10000), // Estimated exit cost
            });
        }

        // Check VTXO expiry
        let expiry_recommendations = self.check_vtxo_expiry().await?;
        recommendations.extend(expiry_recommendations);

        // Check for stuck transactions
        let stuck_recommendations = self.check_stuck_transactions().await?;
        recommendations.extend(stuck_recommendations);

        Ok(recommendations)
    }

    /// Validate that a VTXO is eligible for unilateral exit
    async fn validate_exit_eligibility(&self, vtxo_id: &str) -> Result<()> {
        let client = {
            let client_opt = self.grpc_client.get_ark_client();
            client_opt.as_ref().map(|c| Arc::clone(c))
        };

        if let Some(client) = client {
            match client.spendable_vtxos().await {
                Ok(vtxos) => {
                    // Find the VTXO
                    let vtxo_found = vtxos.iter().any(|(outpoints, vtxo)| {
                        vtxo.address().to_string() == vtxo_id ||
                        outpoints.iter().any(|o| o.outpoint.to_string() == vtxo_id)
                    });

                    if !vtxo_found {
                        return Err(anyhow!("VTXO not found or not spendable: {}", vtxo_id));
                    }

                    // Check if VTXO is confirmed (can't exit pending VTXOs unilaterally)
                    let is_confirmed = vtxos.iter().any(|(outpoints, vtxo)| {
                        (vtxo.address().to_string() == vtxo_id ||
                         outpoints.iter().any(|o| o.outpoint.to_string() == vtxo_id)) &&
                        outpoints.iter().all(|o| !o.is_pending)
                    });

                    if !is_confirmed {
                        return Err(anyhow!("Cannot exit pending VTXO unilaterally: {}", vtxo_id));
                    }

                    Ok(())
                },
                Err(e) => Err(anyhow!("Failed to validate VTXO eligibility: {}", e))
            }
        } else {
            Err(anyhow!("Ark client not available"))
        }
    }

    /// Check if the Ark server is responsive
    async fn check_server_responsiveness(&self) -> Result<bool> {
        // Try to get server info with a timeout
        let timeout_duration = std::time::Duration::from_secs(10);
        
        match tokio::time::timeout(timeout_duration, self.test_server_connection()).await {
            Ok(Ok(_)) => {
                tracing::debug!("Server is responsive");
                Ok(true)
            },
            Ok(Err(e)) => {
                tracing::warn!("Server connection failed: {}", e);
                Ok(false)
            },
            Err(_) => {
                tracing::warn!("Server connection timed out");
                Ok(false)
            }
        }
    }

    /// Test server connection
    async fn test_server_connection(&self) -> Result<()> {
        // Fixed: Access the grpc_client field correctly
        if self.grpc_client.is_connected() {
            // Try to get address as a simple connectivity test
            match self.grpc_client.get_address().await {
                Ok(_) => Ok(()),
                Err(e) => Err(anyhow!("Server test failed: {}", e))
            }
        } else {
            Err(anyhow!("Not connected to server"))
        }
    }

    /// Check VTXOs approaching expiry
    async fn check_vtxo_expiry(&self) -> Result<Vec<ExitRecommendation>> {
        let mut recommendations = Vec::new();
        let now = Utc::now().timestamp() as u64;
        let critical_threshold = 1800; // 30 minutes
        let warning_threshold = 3600;  // 1 hour

        let client = {
            let client_opt = self.grpc_client.get_ark_client();
            client_opt.as_ref().map(|c| Arc::clone(c))
        };

        if let Some(client) = client {
            match client.spendable_vtxos().await {
                Ok(vtxos) => {
                    for (outpoints, _vtxo) in vtxos {
                        for outpoint in outpoints {
                            let time_to_expiry = outpoint.expire_at.saturating_sub(now.try_into().unwrap());
                            
                            if time_to_expiry <= critical_threshold {
                                recommendations.push(ExitRecommendation {
                                    vtxo_id: outpoint.outpoint.to_string(),
                                    reason: ExitReason::NearExpiry(time_to_expiry.try_into().unwrap()),
                                    urgency: ExitUrgency::Critical,
                                    estimated_cost: self.estimate_exit_cost(outpoint.amount).await?,
                                });
                            } else if time_to_expiry <= warning_threshold {
                                recommendations.push(ExitRecommendation {
                                    vtxo_id: outpoint.outpoint.to_string(),
                                    reason: ExitReason::NearExpiry(time_to_expiry.try_into().unwrap()),
                                    urgency: ExitUrgency::Medium,
                                    estimated_cost: self.estimate_exit_cost(outpoint.amount).await?,
                                });
                            }
                        }
                    }
                },
                Err(e) => {
                    tracing::error!("Failed to check VTXO expiry: {}", e);
                }
            }
        }

        Ok(recommendations)
    }

    /// Check for stuck transactions that might need unilateral exit
    async fn check_stuck_transactions(&self) -> Result<Vec<ExitRecommendation>> {
        let mut recommendations = Vec::new();
        
        // Check app state for pending transactions that have been stuck too long
        let transactions = crate::services::APP_STATE.transactions.lock().await;
        let now = Utc::now().timestamp();
        let stuck_threshold = 3600; // 1 hour

        for tx in transactions.iter() {
            if tx.is_settled == Some(false) && 
               (now - tx.timestamp) > stuck_threshold &&
               tx.type_name == "Redeem" {
                
                recommendations.push(ExitRecommendation {
                    vtxo_id: tx.txid.clone(),
                    reason: ExitReason::StuckTransaction,
                    urgency: ExitUrgency::Medium,
                    estimated_cost: Amount::from_sat(5000),
                });
            }
        }

        Ok(recommendations)
    }

    /// Estimate the cost of unilateral exit
    async fn estimate_exit_cost(&self, vtxo_amount: Amount) -> Result<Amount> {
        // Simplified cost estimation
        // In reality, this would depend on current fee rates and transaction size
        let base_cost = Amount::from_sat(2000); // Base transaction cost
        let percentage_cost = Amount::from_sat(vtxo_amount.to_sat() / 1000); // 0.1% of amount
        
        Ok(base_cost + percentage_cost)
    }

    /// Perform emergency exit for all VTXOs
    pub async fn emergency_exit_all(&self) -> Result<Vec<String>> {
        tracing::warn!("Performing emergency exit for all VTXOs");
        
        let mut exit_txids = Vec::new();
        
        let client = {
            let client_opt = self.grpc_client.get_ark_client();
            client_opt.as_ref().map(|c| Arc::clone(c))
        };

        if let Some(client) = client {
            match client.spendable_vtxos().await {
                Ok(vtxos) => {
                    for (outpoints, _vtxo) in vtxos {
                        for outpoint in outpoints {
                            if !outpoint.is_pending {
                                match self.exit_vtxo(outpoint.outpoint.to_string()).await {
                                    Ok(txid) => {
                                        exit_txids.push(txid);
                                        tracing::info!("Emergency exit successful for {}", outpoint.outpoint);
                                    },
                                    Err(e) => {
                                        tracing::error!("Emergency exit failed for {}: {}", outpoint.outpoint, e);
                                    }
                                }
                            }
                        }
                    }
                },
                Err(e) => {
                    return Err(anyhow!("Failed to get VTXOs for emergency exit: {}", e));
                }
            }
        }

        Ok(exit_txids)
    }

    /// Get exit recommendations for user
    pub async fn get_exit_recommendations(&self) -> Result<Vec<ExitRecommendation>> {
        self.check_exit_conditions().await
    }
}

impl Clone for ExitManager {
    fn clone(&self) -> Self {
        Self {
            grpc_client: self.grpc_client.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExitRecommendation {
    pub vtxo_id: String,
    pub reason: ExitReason,
    pub urgency: ExitUrgency,
    pub estimated_cost: Amount,
}

#[derive(Debug, Clone)]
pub enum ExitReason {
    ServerUnresponsive,
    NearExpiry(u64), // seconds until expiry
    StuckTransaction,
    UserRequested,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExitUrgency {
    Low,
    Medium,
    High,
    Critical,
}