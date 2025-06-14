use anyhow::{Result, anyhow};
use ark_core::ArkAddress;
use bitcoin::Amount;
use std::sync::Arc;

pub struct ArkTransactionBuilder {
    grpc_client: Arc<crate::services::ark_grpc::ArkGrpcService>,
}

impl ArkTransactionBuilder {
    pub fn new(grpc_client: Arc<crate::services::ark_grpc::ArkGrpcService>) -> Self {
        Self { grpc_client }
    }

    pub async fn build_vtxo_transfer(
        &self,
        to_address: ArkAddress,
        amount: Amount,
    ) -> Result<String> {
        tracing::info!("Building VTXO transfer: {} sats to {}", amount.to_sat(), to_address);

        if to_address.to_string().is_empty() {
            return Err(anyhow!("Invalid Ark address"));
        }

        if amount == Amount::ZERO {
            return Err(anyhow!("Amount must be greater than zero"));
        }

        match self.grpc_client.send_vtxo(to_address.to_string(), amount.to_sat()).await {
            Ok(txid) => {
                tracing::info!("Successfully built VTXO transfer with txid: {}", txid);
                Ok(txid)
            },
            Err(e) => {
                tracing::error!("Failed to build VTXO transfer: {}", e);
                Err(anyhow!("Failed to build VTXO transfer: {}", e))
            }
        }
    }

    pub async fn build_redeem_transaction(
        &self,
        outputs: Vec<(ArkAddress, Amount)>,
    ) -> Result<String> {
        if outputs.is_empty() {
            return Err(anyhow!("No outputs specified"));
        }

        if outputs.len() > 1 {
            return Err(anyhow!("Multiple outputs not yet supported"));
        }

        let (address, amount) = &outputs[0];
        self.build_vtxo_transfer(address.clone(), *amount).await
    }

    pub async fn estimate_vtxo_fee(&self, amount: Amount) -> Result<Amount> {
        // [TODO!!!]
        let base_fee = Amount::from_sat(100); // Base fee in sats
        let amount_fee = Amount::from_sat(amount.to_sat() / 10000); // 0.01% of amount
        
        Ok(base_fee + amount_fee)
    }

    pub fn validate_transaction_params(
        &self,
        address: &ArkAddress,
        amount: Amount,
    ) -> Result<()> {
        if address.to_string().is_empty() {
            return Err(anyhow!("Invalid Ark address"));
        }

        if amount == Amount::ZERO {
            return Err(anyhow!("Amount must be greater than zero"));
        }

        // dust limit for VTXOs
        let min_amount = Amount::from_sat(546);
        if amount < min_amount {
            return Err(anyhow!("Amount {} is below minimum {}", amount, min_amount));
        }

        Ok(())
    }

    pub async fn prepare_transaction(
        &self,
        to_address: ArkAddress,
        amount: Amount,
    ) -> Result<TransactionPreparation> {
        self.validate_transaction_params(&to_address, amount)?;

        let estimated_fee = self.estimate_vtxo_fee(amount).await?;
        let total_needed = amount + estimated_fee;

        Ok(TransactionPreparation {
            to_address,
            amount,
            estimated_fee,
            total_needed,
        })
    }
}

impl Clone for ArkTransactionBuilder {
    fn clone(&self) -> Self {
        Self {
            grpc_client: self.grpc_client.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransactionPreparation {
    pub to_address: ArkAddress,
    pub amount: Amount,
    pub estimated_fee: Amount,
    pub total_needed: Amount,
}