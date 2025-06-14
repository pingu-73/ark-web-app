pub mod vtxo_manager;
pub mod round_coordinator;
pub mod transaction_builder;
pub mod script_manager;
pub mod subscription_manager;
pub mod exit_manager;

pub use vtxo_manager::{VtxoManager, VtxoState, VtxoStatus};
pub use round_coordinator::{RoundCoordinator, RoundStatus};
pub use transaction_builder::{ArkTransactionBuilder, TransactionPreparation};
pub use script_manager::{ScriptManager, ScriptType};
pub use subscription_manager::{
    SubscriptionManager, ScriptUpdate, VtxoInfo, VtxoUpdate, 
    VtxoUpdateType, BalanceUpdate
};
pub use exit_manager::{ExitManager, ExitRecommendation, ExitReason, ExitUrgency};

use anyhow::{Result, anyhow};
use ark_core::ArkAddress;
use bitcoin::Amount;
use std::sync::Arc;

pub struct ArkOffChainService {
    pub vtxo_manager: VtxoManager,
    pub round_coordinator: RoundCoordinator,
    pub transaction_builder: ArkTransactionBuilder,
    pub script_manager: ScriptManager,
    pub subscription_manager: SubscriptionManager,
    pub exit_manager: ExitManager,
    grpc_client: Arc<crate::services::ark_grpc::ArkGrpcService>, // store ref for health checks
}

impl ArkOffChainService {
    pub fn new(grpc_client: Arc<crate::services::ark_grpc::ArkGrpcService>) -> Self {
        let vtxo_manager = VtxoManager::new(grpc_client.clone());
        let round_coordinator = RoundCoordinator::new(grpc_client.clone());
        let transaction_builder = ArkTransactionBuilder::new(grpc_client.clone());
        let script_manager = ScriptManager::new();
        let subscription_manager = SubscriptionManager::new(grpc_client.clone());
        let exit_manager = ExitManager::new(grpc_client.clone());

        Self {
            vtxo_manager,
            round_coordinator,
            transaction_builder,
            script_manager,
            subscription_manager,
            exit_manager,
            grpc_client,
        }
    }

    pub async fn send_vtxo(&self, address: ArkAddress, amount: Amount) -> Result<String> {
        tracing::info!("Sending {} sats to {}", amount.to_sat(), address);

        // validate tx parameters
        self.transaction_builder.validate_transaction_params(&address, amount)?;

        // check if sufficient balance
        let spendable_vtxos = self.vtxo_manager.get_spendable_vtxos().await?;
        let total_balance: Amount = spendable_vtxos.iter()
            .filter(|v| v.status == VtxoStatus::Confirmed)
            .map(|v| v.total_amount)
            .sum();

        if total_balance < amount {
            return Err(anyhow!(
                "Insufficient confirmed balance: have {} sats, need {} sats", 
                total_balance.to_sat(), 
                amount.to_sat()
            ));
        }

        // build and send the tx
        let txid = self.transaction_builder.build_vtxo_transfer(address, amount).await?;
        
        tracing::info!("Successfully sent VTXO transfer with txid: {}", txid);
        Ok(txid)
    }

    pub async fn participate_in_round(&self) -> Result<Option<String>> {
        tracing::info!("Participating in round for batch swap");
        
        // is participation needed?
        let round_status = self.round_coordinator.get_round_status().await?;
        tracing::debug!("Current round status: {:?}", round_status);
        
        // participate in round
        let result = self.round_coordinator.participate().await?;
        
        if let Some(ref txid) = result {
            tracing::info!("Successfully participated in round: {}", txid);
            
            // update VTXO cache after successful round participation
            if let Err(e) = self.vtxo_manager.get_spendable_vtxos().await {
                tracing::warn!("Failed to refresh VTXO cache after round: {}", e);
            }
        }
        
        Ok(result)
    }

    pub async fn unilateral_exit(&self, vtxo_id: String) -> Result<String> {
        tracing::info!("Performing unilateral exit for VTXO: {}", vtxo_id);
        
        // check exit conditions and get recommendations
        let recommendations = self.exit_manager.get_exit_recommendations().await?;
        let critical_exits = recommendations.iter()
            .filter(|r| r.urgency == ExitUrgency::Critical)
            .count();
            
        if critical_exits > 0 {
            tracing::warn!("Found {} critical exit conditions", critical_exits);
        }
        
        // perform the exit
        let txid = self.exit_manager.exit_vtxo(vtxo_id).await?;
        
        tracing::info!("Successfully initiated unilateral exit with txid: {}", txid);
        Ok(txid)
    }

    // Returns (confirmed_balance, pending_balance) in satoshis
    pub async fn get_balance(&self) -> Result<(Amount, Amount)> {
        let vtxos = self.vtxo_manager.get_spendable_vtxos().await?;
        
        let confirmed: Amount = vtxos.iter()
            .filter(|v| v.status == VtxoStatus::Confirmed)
            .map(|v| v.total_amount)
            .sum();
            
        let pending: Amount = vtxos.iter()
            .filter(|v| v.status == VtxoStatus::Pending)
            .map(|v| v.total_amount)
            .sum();

        tracing::debug!("Current balance - Confirmed: {} sats, Pending: {} sats", 
                       confirmed.to_sat(), pending.to_sat());
        
        Ok((confirmed, pending))
    }

    // Returns (confirmed, pending, expired) amounts
    pub async fn get_detailed_balance(&self) -> Result<(Amount, Amount, Amount)> {
        // Refresh VTXO cache first
        let _vtxos = self.vtxo_manager.get_spendable_vtxos().await?;
        
        // get breakdown from cache
        let breakdown = self.vtxo_manager.get_balance_breakdown();
        
        tracing::debug!("Detailed balance - Confirmed: {} sats, Pending: {} sats, Expired: {} sats", 
                       breakdown.0.to_sat(), breakdown.1.to_sat(), breakdown.2.to_sat());
        
        Ok(breakdown)
    }

    pub async fn handle_expiry_management(&self) -> Result<()> {
        tracing::debug!("Checking VTXO expiry and renewal needs");
        
        // check for expiry and automatically renew if needed
        self.vtxo_manager.check_expiry_and_renew().await?;
        
        // check for any critical exit conditions
        let recommendations = self.exit_manager.get_exit_recommendations().await?;
        let critical_recommendations: Vec<_> = recommendations.iter()
            .filter(|r| r.urgency == ExitUrgency::Critical)
            .collect();
            
        if !critical_recommendations.is_empty() {
            tracing::warn!("Found {} critical exit recommendations", critical_recommendations.len());
            for rec in critical_recommendations {
                tracing::warn!("Critical: VTXO {} - {:?}", rec.vtxo_id, rec.reason);
            }
        }
        
        Ok(())
    }

    // all spendable VTXOs with their current status
    pub async fn get_vtxo_list(&self) -> Result<Vec<VtxoState>> {
        self.vtxo_manager.get_spendable_vtxos().await
    }

    // VTXO info for a specific address
    pub async fn get_vtxo_info(&self, address: &str) -> Result<Vec<VtxoInfo>> {
        self.subscription_manager.list_vtxos(address).await
    }

    // estimate fees for a VTXO tx
    pub async fn estimate_transaction_fee(&self, amount: Amount) -> Result<Amount> {
        self.transaction_builder.estimate_vtxo_fee(amount).await
    }

    // prepare tx for review before sending
    pub async fn prepare_send_transaction(
        &self, 
        to_address: ArkAddress, 
        amount: Amount
    ) -> Result<TransactionPreparation> {
        self.transaction_builder.prepare_transaction(to_address, amount).await
    }

    // current round status info
    pub async fn get_round_status(&self) -> Result<RoundStatus> {
        self.round_coordinator.get_round_status().await
    }

    pub async fn get_exit_recommendations(&self) -> Result<Vec<ExitRecommendation>> {
        self.exit_manager.get_exit_recommendations().await
    }

    pub async fn emergency_exit_all(&self) -> Result<Vec<String>> {
        tracing::warn!("Initiating emergency exit for all VTXOs");
        self.exit_manager.emergency_exit_all().await
    }

    pub async fn start_background_monitoring(&self) -> Result<()> {
        tracing::info!("Starting background monitoring for Ark off-chain service");
        // [TODO!!] background monitoring for VTXO changes & round events

        tracing::info!("todo!");
        Ok(())
    }

    // validate the service is properly init & connected
    pub async fn validate_service_health(&self) -> Result<ServiceHealth> {
        let mut health = ServiceHealth {
            grpc_connected: false,
            vtxo_count: 0,
            balance_confirmed: Amount::ZERO,
            balance_pending: Amount::ZERO,
            round_active: false,
            exit_recommendations: 0,
        };

        // check gRPC connection using stored ref
        health.grpc_connected = self.grpc_client.is_connected();

        // VTXO info
        if let Ok(vtxos) = self.vtxo_manager.get_spendable_vtxos().await {
            health.vtxo_count = vtxos.len();
            health.balance_confirmed = vtxos.iter()
                .filter(|v| v.status == VtxoStatus::Confirmed)
                .map(|v| v.total_amount)
                .sum();
            health.balance_pending = vtxos.iter()
                .filter(|v| v.status == VtxoStatus::Pending)
                .map(|v| v.total_amount)
                .sum();
        }

        // round status
        if let Ok(round_status) = self.round_coordinator.get_round_status().await {
            health.round_active = round_status.is_active;
        }

        // exit recommendations
        if let Ok(recommendations) = self.exit_manager.get_exit_recommendations().await {
            health.exit_recommendations = recommendations.len();
        }

        Ok(health)
    }

    // manual VTXO expiry check
    pub async fn check_vtxo_expiry(&self) -> Result<()> {
        self.vtxo_manager.check_expiry_and_renew().await
    }

    // manual round monitoring check
    pub async fn check_round_participation(&self) -> Result<Option<String>> {
        self.round_coordinator.participate().await
    }
}

impl Clone for ArkOffChainService {
    fn clone(&self) -> Self {
        Self {
            vtxo_manager: self.vtxo_manager.clone(),
            round_coordinator: self.round_coordinator.clone(),
            transaction_builder: ArkTransactionBuilder::new(self.grpc_client.clone()),
            script_manager: ScriptManager::new(),
            subscription_manager: SubscriptionManager::new(self.grpc_client.clone()),
            exit_manager: ExitManager::new(self.grpc_client.clone()),
            grpc_client: self.grpc_client.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServiceHealth {
    pub grpc_connected: bool,
    pub vtxo_count: usize,
    pub balance_confirmed: Amount,
    pub balance_pending: Amount,
    pub round_active: bool,
    pub exit_recommendations: usize,
}

impl ServiceHealth {
    pub fn is_healthy(&self) -> bool {
        self.grpc_connected && self.exit_recommendations == 0
    }

    pub fn status_string(&self) -> String {
        if self.is_healthy() {
            "Healthy".to_string()
        } else if !self.grpc_connected {
            "Disconnected".to_string()
        } else if self.exit_recommendations > 0 {
            format!("Warning: {} exit recommendations", self.exit_recommendations)
        } else {
            "Unknown".to_string()
        }
    }
}