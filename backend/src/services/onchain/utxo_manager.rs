use anyhow::{Result, anyhow};
use bitcoin::{Address, Amount, OutPoint};
use ark_client::{Blockchain, ExplorerUtxo};
use std::sync::Arc;
use std::str::FromStr;
use crate::services::ark_grpc::EsploraBlockchain;

#[derive(Debug, Clone)]
pub struct SpendableUtxo {
    pub outpoint: OutPoint,
    pub amount: Amount,
    pub address: Address,
    pub confirmation_time: Option<u64>,
}

impl From<(ExplorerUtxo, Address)> for SpendableUtxo {
    fn from((utxo, address): (ExplorerUtxo, Address)) -> Self {
        Self {
            outpoint: utxo.outpoint,
            amount: utxo.amount,
            address,
            confirmation_time: utxo.confirmation_blocktime,
        }
    }
}

pub struct UtxoManager {
    blockchain: Arc<EsploraBlockchain>,
}

impl UtxoManager {
    pub fn new(blockchain: Arc<EsploraBlockchain>) -> Self {
        Self { blockchain }
    }

    pub async fn get_spendable_utxos(&self) -> Result<Vec<SpendableUtxo>> {
        let address_str = crate::services::wallet::get_onchain_address().await?;
        let address = bitcoin::Address::from_str(&address_str)?
            .assume_checked();

        tracing::info!("Looking for UTXOs at regular Bitcoin address: {}", address);

        // find UTXOs for this address
        let explorer_utxos = self.blockchain.find_outpoints(&address).await
            .map_err(|e| anyhow!("Failed to find outpoints: {}", e))?;

        // filter for unspent UTXOs and convert to SpendableUtxo
        let spendable_utxos: Vec<SpendableUtxo> = explorer_utxos
            .into_iter()
            .filter(|utxo| !utxo.is_spent)
            .map(|utxo| SpendableUtxo::from((utxo, address.clone())))
            .collect();

        tracing::info!("Found {} spendable UTXOs totaling {} sats", 
            spendable_utxos.len(),
            spendable_utxos.iter().map(|u| u.amount.to_sat()).sum::<u64>()
        );
        
        Ok(spendable_utxos)
    }

    pub async fn get_total_balance(&self) -> Result<Amount> {
        let utxos = self.get_spendable_utxos().await?;
        let total = utxos.iter().map(|utxo| utxo.amount).sum();
        Ok(total)
    }

    pub fn select_utxos(&self, utxos: Vec<SpendableUtxo>, target_amount: Amount) -> Result<Vec<SpendableUtxo>> {
        // largest first selection
        let mut sorted_utxos = utxos;
        sorted_utxos.sort_by(|a, b| b.amount.cmp(&a.amount));

        let mut selected = Vec::new();
        let mut total_selected = Amount::ZERO;

        for utxo in sorted_utxos {
            selected.push(utxo.clone());
            total_selected += utxo.amount;

            if total_selected >= target_amount {
                break;
            }
        }

        if total_selected < target_amount {
            return Err(anyhow!(
                "Insufficient funds: need {}, have {}",
                target_amount,
                total_selected
            ));
        }

        tracing::info!(
            "Selected {} UTXOs totaling {} for target {}",
            selected.len(),
            total_selected,
            target_amount
        );

        Ok(selected)
    }
}