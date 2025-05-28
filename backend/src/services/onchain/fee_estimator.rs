use anyhow::{Result, anyhow};
use bitcoin::FeeRate;
use std::sync::Arc;
use crate::services::ark_grpc::EsploraBlockchain;

pub struct FeeEstimator {
    blockchain: Arc<EsploraBlockchain>,
}

impl FeeEstimator {
    pub fn new(blockchain: Arc<EsploraBlockchain>) -> Self {
        Self { blockchain }
    }

    pub async fn estimate_fee_rate(&self) -> Result<FeeRate> {
        // fee estimates from  blockchain
        match self.get_network_fee_rate().await {
            Ok(fee_rate) => {
                tracing::info!("Using network fee rate: {} sat/vB", fee_rate.to_sat_per_vb_ceil());
                Ok(fee_rate)
            },
            Err(e) => {
                tracing::warn!("Failed to get network fee rate: {}, using fallback", e);
                Ok(self.get_fallback_fee_rate())
            }
        }
    }

    async fn get_network_fee_rate(&self) -> Result<FeeRate> {
        // [TODO!!!]
        let network = std::env::var("BITCOIN_NETWORK").unwrap_or_else(|_| "regtest".to_string());
        
        match network.as_str() {
            "regtest" => {
                Ok(FeeRate::from_sat_per_vb(1 as u64).expect("Valid fee rate"))
            },
            "testnet" => {
                Ok(FeeRate::from_sat_per_vb(2 as u64).expect("Valid fee rate"))
            },
            _ => {
                Ok(FeeRate::from_sat_per_vb(10 as u64).expect("Valid fee rate"))
            }
        }
    }

    fn get_fallback_fee_rate(&self) -> FeeRate {
        let network = std::env::var("BITCOIN_NETWORK").unwrap_or_else(|_| "regtest".to_string());
        
        match network.as_str() {
            "regtest" => FeeRate::from_sat_per_vb(1 as u64).expect("Valid fee rate"),
            "testnet" => FeeRate::from_sat_per_vb(2 as u64).expect("Valid fee rate"),
            _ => FeeRate::from_sat_per_vb(20 as u64).expect("Valid fee rate"),
        }
    }

    pub fn get_priority_fee_rate(&self, priority: FeePriority) -> FeeRate {
        let base_rate = self.get_fallback_fee_rate();
        let base_sat_per_vb = base_rate.to_sat_per_vb_ceil();
        
        match priority {
            FeePriority::Low => {
                let low_rate = (base_sat_per_vb as f64 * 0.5).max(1.0) as u64;
                FeeRate::from_sat_per_vb(low_rate).expect("Valid fee rate")
            },
            FeePriority::Medium => base_rate,
            FeePriority::High => {
                let high_rate = base_sat_per_vb * 2;
                FeeRate::from_sat_per_vb(high_rate).expect("Valid fee rate")
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum FeePriority {
    Low,
    Medium,
    High,
}