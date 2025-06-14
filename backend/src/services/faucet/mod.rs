use anyhow::{Result, anyhow};
use bitcoin::{Amount, Address};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct FaucetService {
    esplora_url: String,
    network: bitcoin::Network,
    rate_limiter: Arc<Mutex<HashMap<String, Instant>>>,
    faucet_amount: Amount,
    cooldown_period: Duration,
}

impl FaucetService {
    pub fn new(esplora_url: String, network: bitcoin::Network) -> Self {
        Self {
            esplora_url,
            network,
            rate_limiter: Arc::new(Mutex::new(HashMap::new())),
            faucet_amount: Amount::from_sat(100_000), // 0.001 BTC
            cooldown_period: Duration::from_secs(3600), // 1 hour
        }
    }

    pub async fn send_to_address(&self, address: &str) -> Result<String> {
        self.check_rate_limit(address).await?;
        
        // For regtest, use bitcoin-cli or nigiri
        if self.network == bitcoin::Network::Regtest {
            self.send_regtest_funds(address).await
        } else {
            Err(anyhow!("Faucet only available on regtest"))
        }
    }

    async fn send_regtest_funds(&self, address: &str) -> Result<String> {
        // try nigiri first
        let output = tokio::process::Command::new("nigiri")
            .args(&["rpc", "sendtoaddress", address, "0.001"])
            .output()
            .await;

        let (txid, needs_mining) = match output {
            Ok(output) if output.status.success() => {
                let txid = String::from_utf8_lossy(&output.stdout).trim().to_string();
                (txid, true)
            },
            _ => {
                // fallback to bitcoin-cli
                let output = tokio::process::Command::new("bitcoin-cli")
                    .args(&["-regtest", "sendtoaddress", address, "0.001"])
                    .output()
                    .await?;

                if !output.status.success() {
                    return Err(anyhow!("Failed to send funds: {}", 
                        String::from_utf8_lossy(&output.stderr)));
                }

                let txid = String::from_utf8_lossy(&output.stdout).trim().to_string();
                (txid, true)
            }
        };

        // mine a block to confirm
        if needs_mining {
            self.mine_blocks(1).await?;
        }

        // update rate limiter
        let mut limiter = self.rate_limiter.lock().await;
        limiter.insert(address.to_string(), Instant::now());

        Ok(txid)
    }

    async fn mine_blocks(&self, count: u32) -> Result<()> {
        // try nigiri first
        let output = tokio::process::Command::new("nigiri")
            .args(&["rpc", "generatetoaddress", &count.to_string(), 
                   "$(nigiri rpc getnewaddress)"])
            .output()
            .await;

        if output.is_err() || !output.unwrap().status.success() {
            // [TODO!!!] fallback to bitcoin-cli
            tokio::process::Command::new("bitcoin-cli")
                .args(&["-regtest", "generatetoaddress", &count.to_string(),
                       "bcrt1qst65t8j4p7gf8zpqp6wfkn2l9mznqkd5jh46u"])
                .output()
                .await?;
        }

        Ok(())
    }

    async fn check_rate_limit(&self, address: &str) -> Result<()> {
        let limiter = self.rate_limiter.lock().await;
        
        if let Some(last_request) = limiter.get(address) {
            if last_request.elapsed() < self.cooldown_period {
                let remaining = self.cooldown_period - last_request.elapsed();
                return Err(anyhow!(
                    "Rate limited. Try again in {} seconds", 
                    remaining.as_secs()
                ));
            }
        }
        
        Ok(())
    }

    pub async fn fund_ark_address(&self, boarding_address: &str) -> Result<String> {
        self.send_to_address(boarding_address).await
    }

    pub fn get_info(&self) -> FaucetInfo {
        FaucetInfo {
            network: self.network.to_string(),
            amount_sats: self.faucet_amount.to_sat(),
            cooldown_seconds: self.cooldown_period.as_secs(),
            available: self.network == bitcoin::Network::Regtest,
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub struct FaucetInfo {
    pub network: String,
    pub amount_sats: u64,
    pub cooldown_seconds: u64,
    pub available: bool,
}