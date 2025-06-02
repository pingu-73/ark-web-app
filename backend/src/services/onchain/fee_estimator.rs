use anyhow::{Result, anyhow};
use bitcoin::FeeRate;
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::services::ark_grpc::EsploraBlockchain;

#[derive(Debug, Deserialize)]
struct MempoolSpaceFees {
    #[serde(rename = "fastestFee")]
    fastest_fee: u64,
    #[serde(rename = "halfHourFee")]
    half_hour_fee: u64,
    #[serde(rename = "hourFee")]
    hour_fee: u64,
    #[serde(rename = "economyFee")]
    economy_fee: u64,
    #[serde(rename = "minimumFee")]
    minimum_fee: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeEstimates {
    pub fastest: u64,      // next block
    pub fast: u64,         // 2-3 blocks
    pub normal: u64,       // 6 blocks
    pub slow: u64,         // 12-24 blocks
    pub minimum: u64,      // min relay fee
    pub timestamp: i64,
}

#[derive(Debug, Clone)]
pub struct CachedFeeEstimates {
    estimates: FeeEstimates,
    last_updated: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FeePriority {
    Fastest, // next block
    Fast,    // 2-3 blocks
    Normal,  // ~6 blocks
    Slow,    // 12+ blocks
}

impl From<String> for FeePriority {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "fastest" | "urgent" => FeePriority::Fastest,
            "fast" | "high" => FeePriority::Fast,
            "normal" | "medium" => FeePriority::Normal,
            "slow" | "low" | "economy" => FeePriority::Slow,
            _ => FeePriority::Normal,
        }
    }
}

pub struct FeeEstimator {
    blockchain: Arc<EsploraBlockchain>,
    http_client: reqwest::Client,
    network: bitcoin::Network,
    cache: Arc<RwLock<Option<CachedFeeEstimates>>>,
    cache_duration: Duration,
}

impl FeeEstimator {
    pub fn new(blockchain: Arc<EsploraBlockchain>) -> Self {
        let network = match std::env::var("BITCOIN_NETWORK").unwrap_or_else(|_| "regtest".to_string()).as_str() {
            "mainnet" => bitcoin::Network::Bitcoin,
            "testnet" => bitcoin::Network::Testnet,
            "signet" => bitcoin::Network::Signet,
            _ => bitcoin::Network::Regtest,
        };

        Self {
            blockchain,
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("Failed to build HTTP client"),
            network,
            cache: Arc::new(RwLock::new(None)),
            cache_duration: Duration::from_secs(300), // 5 minutes
        }
    }

    pub async fn estimate_fee_rate(&self) -> Result<FeeRate> {
        let estimates = self.get_fee_estimates().await?;
        // normal priority as default
        FeeRate::from_sat_per_vb(estimates.normal)
            .ok_or_else(|| anyhow!("Invalid fee rate"))
    }

    pub async fn get_fee_estimates(&self) -> Result<FeeEstimates> {
        // check cache first
        if let Some(cached) = self.get_cached_estimates() {
            return Ok(cached);
        }

        // multiple sources in order of preference
        let estimates = match self.network {
            bitcoin::Network::Bitcoin | bitcoin::Network::Testnet => {
                self.try_multiple_sources().await?
            },
            bitcoin::Network::Regtest => {
                self.get_regtest_estimates().await?
            },
            _ => {
                self.get_default_estimates()
            }
        };

        // cache the estimates
        self.cache_estimates(estimates.clone());
        
        Ok(estimates)
    }

    async fn try_multiple_sources(&self) -> Result<FeeEstimates> {
        // mempool.space first
        if let Ok(estimates) = self.fetch_mempool_space_estimates().await {
            return Ok(estimates);
        }

        // blockstream
        if let Ok(estimates) = self.fetch_blockstream_estimates().await {
            return Ok(estimates);
        }

        // local node
        if let Ok(estimates) = self.fetch_bitcoin_core_estimates().await {
            return Ok(estimates);
        }

        // fallback to defaults
        Ok(self.get_default_estimates())
    }

    async fn fetch_mempool_space_estimates(&self) -> Result<FeeEstimates> {
        let base_url = match self.network {
            bitcoin::Network::Bitcoin => "https://mempool.space",
            bitcoin::Network::Testnet => "https://mempool.space/testnet",
            _ => return Err(anyhow!("Network not supported by mempool.space")),
        };

        let url = format!("{}/api/v1/fees/recommended", base_url);
        let response: MempoolSpaceFees = self.http_client
            .get(&url)
            .send()
            .await?
            .json()
            .await?;

        Ok(FeeEstimates {
            fastest: response.fastest_fee,
            fast: response.half_hour_fee,
            normal: response.hour_fee,
            slow: response.economy_fee,
            minimum: response.minimum_fee,
            timestamp: chrono::Utc::now().timestamp(),
        })
    }

    async fn fetch_blockstream_estimates(&self) -> Result<FeeEstimates> {
        let base_url = match self.network {
            bitcoin::Network::Bitcoin => "https://blockstream.info",
            bitcoin::Network::Testnet => "https://blockstream.info/testnet",
            _ => return Err(anyhow!("Network not supported by blockstream")),
        };

        let url = format!("{}/api/fee-estimates", base_url);
        let response: HashMap<String, f64> = self.http_client
            .get(&url)
            .send()
            .await?
            .json()
            .await?;

        // map block targets to fee tiers
        let fastest = response.get("1").copied().unwrap_or(50.0) as u64;
        let fast = response.get("3").copied().unwrap_or(30.0) as u64;
        let normal = response.get("6").copied().unwrap_or(20.0) as u64;
        let slow = response.get("144").copied().unwrap_or(10.0) as u64;

        Ok(FeeEstimates {
            fastest,
            fast,
            normal,
            slow,
            minimum: 1,
            timestamp: chrono::Utc::now().timestamp(),
        })
    }

    async fn fetch_bitcoin_core_estimates(&self) -> Result<FeeEstimates> {
        tracing::info!("using nigiri to estimate fees");
        
        let targets = vec![1, 3, 6, 144];
        let mut estimates = vec![];
    
        for target in targets {
            tracing::info!("Fetching fee estimate for {} blocks", target);
            
            // change to `bitcoin-cli` if running it in regtest mode instead of `nigiri` and everything accordingly
            let output = tokio::process::Command::new("nigiri")
                .args(&[
                    "rpc",
                    "estimatesmartfee",
                    &target.to_string(),
                ])
                .output()
                .await?;

            tracing::debug!("Command exit status: {}", output.status);
            tracing::debug!("Stdout: {}", String::from_utf8_lossy(&output.stdout));
            tracing::debug!("Stderr: {}", String::from_utf8_lossy(&output.stderr));
    
            if output.status.success() {
                let stdout_str = String::from_utf8_lossy(&output.stdout);
                
                let clean_json = Self::strip_ansi_codes(&stdout_str);
                tracing::info!("Cleaned JSON for {} blocks: {}", target, clean_json);
                
                match serde_json::from_str::<serde_json::Value>(&clean_json) {
                    Ok(response) => {
                        tracing::debug!("Parsed JSON: {:?}", response);
                        
                        if let Some(feerate) = response.get("feerate").and_then(|v| v.as_f64()) {
                            // convert BTC/kvB to sat/vB
                            let sat_per_vb = (feerate * 100_000.0) as u64;
                            estimates.push(sat_per_vb);
                            tracing::info!("Fee estimate for {} blocks: {} BTC/kvB = {} sat/vB", target, feerate, sat_per_vb);
                        } else {
                            tracing::warn!("No 'feerate' field found in response for {} blocks", target);
                        }
                    },
                    Err(e) => {
                        tracing::error!("Failed to parse JSON for {} blocks: {}", target, e);
                        tracing::debug!("Clean JSON was: {}", clean_json);
                    }
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                tracing::warn!("Command failed for {} blocks. Exit code: {}, stderr: {}", target, output.status, stderr);
            }
        }
    
        tracing::info!("Collected {} estimates: {:?}", estimates.len(), estimates);
    
        if estimates.len() >= 4 {
            let fee_estimates = FeeEstimates {
                fastest: estimates[0],
                fast: estimates[1],
                normal: estimates[2],
                slow: estimates[3],
                minimum: 1,
                timestamp: chrono::Utc::now().timestamp(),
            };
            tracing::info!("Successfully created fee estimates: {:?}", fee_estimates);
            Ok(fee_estimates)
        } else {
            Err(anyhow!("Failed to get enough fee estimates from bitcoin core: got {} estimates, need 4", estimates.len()))
        }
    }
    
    // helper function to strip ANSI color codes (alt: no color env var for nigiri)
    fn strip_ansi_codes(input: &str) -> String {
        // simple regex to remove ANSI escape seq
        // pattern: \x1b\[[0-9;]*m
        let mut result = String::new();
        let mut chars = input.chars();
        
        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                // skip escape seq
                if chars.next() == Some('[') {
                    // skip until we find 'm'
                    while let Some(c) = chars.next() {
                        if c == 'm' {
                            break;
                        }
                    }
                }
            } else {
                result.push(ch);
            }
        }
        
        result
    }

    async fn get_regtest_estimates(&self) -> Result<FeeEstimates> {
        tracing::info!("Testing API calls on regtest...");

        // for regtest => bitcoin-cli first
        if let Ok(estimates) = self.fetch_bitcoin_core_estimates().await {
            tracing::info!("Successfully fetched from bitcoin-cli: {:?}", estimates);
            return Ok(estimates);
        }

        // otherwise use fixed values
        tracing::warn!("using hardcoded regtest values");
        Ok(FeeEstimates {
            fastest: 10,
            fast: 5,
            normal: 2,
            slow: 1,
            minimum: 1,
            timestamp: chrono::Utc::now().timestamp(),
        })
    }

    fn get_default_estimates(&self) -> FeeEstimates {
        match self.network {
            bitcoin::Network::Bitcoin => FeeEstimates {
                fastest: 50,
                fast: 30,
                normal: 20,
                slow: 10,
                minimum: 1,
                timestamp: chrono::Utc::now().timestamp(),
            },
            _ => FeeEstimates {
                fastest: 10,
                fast: 5,
                normal: 2,
                slow: 1,
                minimum: 1,
                timestamp: chrono::Utc::now().timestamp(),
            },
        }
    }

    fn get_cached_estimates(&self) -> Option<FeeEstimates> {
        let cache = self.cache.read();
        if let Some(cached) = cache.as_ref() {
            if cached.last_updated.elapsed() < self.cache_duration {
                return Some(cached.estimates.clone());
            }
        }
        None
    }

    fn cache_estimates(&self, estimates: FeeEstimates) {
        let mut cache = self.cache.write();
        *cache = Some(CachedFeeEstimates {
            estimates,
            last_updated: Instant::now(),
        });
    }

    pub async fn estimate_fee_for_priority(&self, priority: FeePriority) -> Result<FeeRate> {
        let estimates = self.get_fee_estimates().await?;
        
        let sat_per_vb = match priority {
            FeePriority::Fastest => estimates.fastest,
            FeePriority::Fast => estimates.fast,
            FeePriority::Normal => estimates.normal,
            FeePriority::Slow => estimates.slow,
        };

        tracing::info!(
            "Fee rate for {:?} priority: {} sat/vB",
            priority,
            sat_per_vb
        );

        FeeRate::from_sat_per_vb(sat_per_vb)
            .ok_or_else(|| anyhow!("Invalid fee rate"))
    }

    // legacy method for backward compatibility
    pub fn get_priority_fee_rate(&self, priority: FeePriority) -> FeeRate {
        let base_rate = self.get_fallback_fee_rate();
        self.adjust_fee_for_priority(base_rate, priority)
    }

    fn adjust_fee_for_priority(&self, base_rate: FeeRate, priority: FeePriority) -> FeeRate {
        let base_sat_per_vb = base_rate.to_sat_per_vb_ceil();
        
        let adjusted = match self.network {
            bitcoin::Network::Regtest => {
                match priority {
                    FeePriority::Fastest => base_sat_per_vb * 10,
                    FeePriority::Fast => base_sat_per_vb * 5,
                    FeePriority::Normal => base_sat_per_vb * 2,
                    FeePriority::Slow => base_sat_per_vb,
                }
            },
            _ => {
                match priority {
                    FeePriority::Fastest => (base_sat_per_vb as f64 * 2.0) as u64,
                    FeePriority::Fast => (base_sat_per_vb as f64 * 1.5) as u64,
                    FeePriority::Normal => base_sat_per_vb,
                    FeePriority::Slow => (base_sat_per_vb as f64 * 0.7).max(1.0) as u64,
                }
            }
        };

        FeeRate::from_sat_per_vb(adjusted).expect("Valid fee rate")
    }

    fn get_fallback_fee_rate(&self) -> FeeRate {
        match self.network {
            bitcoin::Network::Bitcoin => FeeRate::from_sat_per_vb(10).expect("Valid fee rate"),
            bitcoin::Network::Testnet => FeeRate::from_sat_per_vb(2).expect("Valid fee rate"),
            _ => FeeRate::from_sat_per_vb(1).expect("Valid fee rate"),
        }
    }
}