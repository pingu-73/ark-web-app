pub mod utxo_manager;
pub mod fee_estimator;
pub mod transaction_builder;

pub use utxo_manager::UtxoManager;
pub use fee_estimator::FeeEstimator;
pub use transaction_builder::TransactionBuilder;

use anyhow::Result;
use bitcoin::{Address, Amount, Txid};
use crate::services::ark_grpc::EsploraBlockchain;

pub struct OnChainPaymentService {
    pub utxo_manager: UtxoManager,
    pub fee_estimator: FeeEstimator,
    pub transaction_builder: TransactionBuilder,
}

impl OnChainPaymentService {
    pub fn new(blockchain: std::sync::Arc<EsploraBlockchain>) -> Self {
        let utxo_manager = UtxoManager::new(blockchain.clone());
        let fee_estimator = FeeEstimator::new(blockchain.clone());
        let transaction_builder = TransactionBuilder::new(blockchain);

        Self {
            utxo_manager,
            fee_estimator,
            transaction_builder,
        }
    }

    pub async fn send_payment(
        &self,
        to_address: Address,
        amount: Amount,
        fee_rate: Option<bitcoin::FeeRate>,
    ) -> Result<Txid> {
        // 1. get available UTXOs
        let utxos = self.utxo_manager.get_spendable_utxos().await?;
        
        // 2. estimate fee if not provided
        let fee_rate = match fee_rate {
            Some(rate) => rate,
            None => self.fee_estimator.estimate_fee_rate().await?,
        };

        // 3. select UTXOs and build tx
        let txid = self.transaction_builder
            .build_and_broadcast(utxos, to_address, amount, fee_rate)
            .await?;

        Ok(txid)
    }

    pub async fn get_balance(&self) -> Result<Amount> {
        self.utxo_manager.get_total_balance().await
    }

    pub async fn estimate_fee(&self, to_address: Address, amount: Amount) -> Result<Amount> {
        let utxos = self.utxo_manager.get_spendable_utxos().await?;
        let fee_rate = self.fee_estimator.estimate_fee_rate().await?;
        
        self.transaction_builder.estimate_fee(utxos, to_address, amount, fee_rate).await
    }
}