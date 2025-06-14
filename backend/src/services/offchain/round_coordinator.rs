use anyhow::{Result, anyhow};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use crate::services::ark_grpc::EsploraBlockchain;
use crate::services::ark_grpc::ArkWallet;
use crate::services::Client;
use bip39::rand::{SeedableRng, rngs::StdRng};

pub struct RoundCoordinator {
    grpc_client: Arc<crate::services::ark_grpc::ArkGrpcService>,
}

impl RoundCoordinator {
    pub fn new(grpc_client: Arc<crate::services::ark_grpc::ArkGrpcService>) -> Self {
        Self { grpc_client }
    }

    /// Participate in round
    pub async fn participate(&self) -> Result<Option<String>> {
        tracing::info!("Checking round participation requirements");
        
        let client = {
            let client_opt = self.grpc_client.get_ark_client();
            client_opt.as_ref().map(|c| Arc::clone(c))
        };

        if let Some(client) = client {
            // check if we have anything to board
            let has_inputs = self.check_for_inputs(&client).await?;
            if !has_inputs {
                tracing::debug!("No inputs available for round participation");
                return Ok(None);
            }

            // `bitcoin::secp256k1::rand::thread_rng();` future doesn't impl `Send`
            let mut rng = StdRng::from_entropy(); // switch to StdRng::seed_from_u64(42)
            match client.board(&mut rng).await {
                Ok(_) => {
                    tracing::info!("Successfully participated in round");
                    // [TODO!!] actual round txid would need to be extracted from events
                    Ok(Some("round_completed".to_string()))
                },
                Err(e) => {
                    if e.to_string().contains("No boarding outputs") {
                        tracing::debug!("No participation needed");
                        Ok(None)
                    } else {
                        Err(anyhow!("Round participation failed: {}", e))
                    }
                }
            }
        } else {
            Err(anyhow!("Ark client not available"))
        }
    }

    // inputs that need round participation
    async fn check_for_inputs(&self, client: &Client<EsploraBlockchain, ArkWallet>) -> Result<bool> {
        // VTXOs near expiry
        if let Ok(vtxos) = client.spendable_vtxos().await {
            let now = chrono::Utc::now().timestamp() as u64;
            let renewal_threshold = 7200; // 2 hours
            
            for (outpoints, _) in &vtxos {
                for outpoint in outpoints {
                    let time_to_expiry = outpoint.expire_at.saturating_sub(now as i64);
                    if time_to_expiry <= renewal_threshold as i64 {
                        tracing::info!("Found VTXO near expiry, round participation needed");
                        return Ok(true);
                    }
                }
            }
        }

        Ok(true) // let the client.board() method do its own checks
    }

    async fn check_available_inputs(&self) -> Result<bool> {
        let has_boarding = self.check_boarding_outputs().await?;
        let has_vtxos = self.check_vtxos_for_renewal().await?;

        Ok(has_boarding || has_vtxos)
    }

    async fn check_boarding_outputs(&self) -> Result<bool> {
        // [TODO!!] check for confirmed boarding transactions
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

    /// monitor round events and handle signing
    pub async fn monitor_rounds(&self) -> Result<()> {
        tracing::info!("Starting round monitoring");
        
        // [TODO!!] implement continuous monitoring of round events
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        
        loop {
            interval.tick().await;
            
            if let Err(e) = self.check_and_participate().await {
                tracing::error!("Error during round monitoring: {}", e);
            }
        }
    }


    async fn check_and_participate(&self) -> Result<()> {
        let needs_participation = self.check_available_inputs().await?;
        
        if needs_participation {
            tracing::info!("Conditions met for round participation");
            if let Err(e) = self.participate().await {
                tracing::error!("Failed to participate in round: {}", e);
            }
        }
        
        Ok(())
    }

    pub async fn get_round_status(&self) -> Result<RoundStatus> {
        // [TODO!!] query the current round status from the server
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