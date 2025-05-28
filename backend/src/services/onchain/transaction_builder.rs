use anyhow::{Result, anyhow};
use bitcoin::{
    Address, Amount, FeeRate, Transaction, TxIn, TxOut, Txid, OutPoint,
    absolute::LockTime, transaction::Version, Witness, ScriptBuf, AddressType
};
use bitcoin::sighash::{Prevouts, SighashCache, TapSighashType};
use ark_client::Blockchain;
use std::sync::Arc;
use std::str::FromStr;
use crate::services::ark_grpc::EsploraBlockchain;
use super::utxo_manager::SpendableUtxo;

pub struct TransactionBuilder {
    blockchain: Arc<EsploraBlockchain>,
}

impl TransactionBuilder {
    pub fn new(blockchain: Arc<EsploraBlockchain>) -> Self {
        Self { blockchain }
    }

    pub async fn build_and_broadcast(
        &self,
        available_utxos: Vec<SpendableUtxo>,
        to_address: Address,
        amount: Amount,
        fee_rate: FeeRate,
    ) -> Result<Txid> {
        // build the tx
        let (tx, _change_amount) = self.build_transaction(
            available_utxos,
            to_address,
            amount,
            fee_rate,
        ).await?;

        // broadcast the tx
        self.blockchain.broadcast(&tx).await
            .map_err(|e| anyhow!("Failed to broadcast transaction: {}", e))?;

        let txid = tx.compute_txid();
        tracing::info!("Successfully broadcast transaction: {}", txid);

        Ok(txid)
    }

    pub async fn estimate_fee(
        &self,
        available_utxos: Vec<SpendableUtxo>,
        to_address: Address,
        amount: Amount,
        fee_rate: FeeRate,
    ) -> Result<Amount> {
        let (_, fee, _) = self.calculate_transaction_details (
            available_utxos,
            to_address,
            amount,
            fee_rate,
        ).await?;

        Ok(fee)
    }

    async fn build_transaction(
        &self,
        available_utxos: Vec<SpendableUtxo>,
        to_address: Address,
        amount: Amount,
        fee_rate: FeeRate,
    ) -> Result<(Transaction, Amount)> {
        let (selected_utxos, fee, change_amount) = self.calculate_transaction_details(
            available_utxos,
            to_address.clone(),
            amount,
            fee_rate,
        ).await?;

        let (keypair, _) = crate::services::APP_STATE.key_manager.load_or_create_wallet()?;
        let change_address = self.get_change_address(&keypair)?;

        // build ip
        let inputs: Vec<TxIn> = selected_utxos
            .iter()
            .map(|utxo| TxIn {
                previous_output: utxo.outpoint,
                script_sig: ScriptBuf::new(),
                sequence: bitcoin::Sequence::ENABLE_RBF_NO_LOCKTIME,
                witness: Witness::new(),
            })
            .collect();

        // build op
        let mut outputs = vec![TxOut {
            value: amount,
            script_pubkey: to_address.script_pubkey(),
        }];

        // add change op if needed
        if change_amount > Amount::ZERO {
            outputs.push(TxOut {
                value: change_amount,
                script_pubkey: change_address.script_pubkey(),
            });
        }

        // create unsigned tx
        let mut tx = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: inputs,
            output: outputs,
        };

        self.sign_transaction(&mut tx, &selected_utxos, &keypair).await?;

        Ok((tx, change_amount))
    }

    async fn calculate_transaction_details(
        &self,
        available_utxos: Vec<SpendableUtxo>,
        to_address: Address,
        amount: Amount,
        fee_rate: FeeRate,
    ) -> Result<(Vec<SpendableUtxo>, Amount, Amount)> {
        // [TODO!!] estimate tx size
        let estimated_size = self.estimate_transaction_size(1, 2);
        let estimated_fee = Amount::from_sat(fee_rate.fee_vb(estimated_size as u64).expect("Fee calculation failed").to_sat());
    
        let total_needed = amount + estimated_fee;
    
        // [TODO!!] select UTXOs
        let utxo_manager = super::UtxoManager::new(self.blockchain.clone());
        let selected_utxos = utxo_manager.select_utxos(available_utxos, total_needed)?;
    
        let total_input: Amount = selected_utxos.iter().map(|utxo| utxo.amount).sum();
    
        // Recalculate with actual number of inputs
        let actual_size = self.estimate_transaction_size(selected_utxos.len(), 2);
        let mut actual_fee = Amount::from_sat(fee_rate.fee_vb(actual_size as u64).expect("Fee calculation failed").to_sat());
    
        // [TODO!!!] ensure mini fee
        let min_fee = Amount::from_sat(160);
        if actual_fee < min_fee {
            tracing::info!("Increasing fee from {} to {} to meet minimum relay fee", actual_fee, min_fee);
            actual_fee = min_fee;
        }
    
        let change_amount = total_input - amount - actual_fee;
    
        // check if change is dust
        let dust_threshold = Amount::from_sat(546);
        let final_change = if change_amount < dust_threshold {
            Amount::ZERO
        } else {
            change_amount
        };
    
        let final_fee = if final_change == Amount::ZERO {
            actual_fee + change_amount
        } else {
            actual_fee
        };
    
        if total_input < amount + final_fee {
            return Err(anyhow!(
                "Insufficient funds after fee calculation: need {}, have {}",
                amount + final_fee,
                total_input
            ));
        }
    
        Ok((selected_utxos, final_fee, final_change))
    }

    fn estimate_transaction_size(&self, num_inputs: usize, num_outputs: usize) -> usize {
        // Simplified transaction size estimation for P2WPKH
        // Base size: 10 bytes (version, locktime, etc.)
        // Input: 41 bytes (outpoint + sequence + script_sig length)
        // Output: 31 bytes (value + script_pubkey for P2WPKH)
        // Witness: ~27 bytes per input (signature + pubkey)
        
        let base_size = 10;
        let input_size = num_inputs * 41;
        let output_size = num_outputs * 31;
        let witness_size = num_inputs * 27;
        
        // for segwit account for witness discount
        let non_witness_size = base_size + input_size + output_size;
        let total_size = non_witness_size + (witness_size / 4); // witness discount
        
        total_size
    }

    async fn sign_transaction(
        &self,
        tx: &mut Transaction,
        selected_utxos: &[SpendableUtxo],
        keypair: &bitcoin::key::Keypair,
    ) -> Result<()> {
        use bitcoin::sighash::{Prevouts, SighashCache};
        use bitcoin::AddressType;
        
        let secp = bitcoin::secp256k1::Secp256k1::new();
        
        // prepare prevouts for sighash calculation
        let prevouts: Vec<TxOut> = selected_utxos
            .iter()
            .map(|utxo| TxOut {
                value: utxo.amount,
                script_pubkey: utxo.address.script_pubkey(),
            })
            .collect();
    
        // sign each ip
        for (input_index, utxo) in selected_utxos.iter().enumerate() {
            let mut sighash_cache = SighashCache::new(&*tx);
            
            match utxo.address.address_type() {
                Some(AddressType::P2wpkh) => {
                    let sighash = sighash_cache
                        .p2wpkh_signature_hash(
                            input_index,
                            &utxo.address.script_pubkey(),
                            utxo.amount,
                            bitcoin::EcdsaSighashType::All,
                        )
                        .map_err(|e| anyhow!("Failed to compute p2wpkh sighash: {}", e))?;
    
                    let message = bitcoin::secp256k1::Message::from_digest_slice(&sighash[..])
                        .map_err(|e| anyhow!("Failed to create message: {}", e))?;
                    
                    let signature = secp.sign_ecdsa(&message, &keypair.secret_key());
                    let mut sig_bytes = signature.serialize_der().to_vec();
                    sig_bytes.push(bitcoin::EcdsaSighashType::All as u8);
    
                    let pubkey_bytes = keypair.public_key().serialize();
                    let witness = vec![sig_bytes, pubkey_bytes.to_vec()];
                    
                    tx.input[input_index].witness = Witness::from_slice(&witness);
                    tracing::debug!("Signed input {} with P2WPKH", input_index);
                },
                Some(AddressType::P2tr) => {
                    return Err(anyhow!("Taproot addresses not supported for regular on-chain payments. Use boarding address only for Ark operations."));
                },
                Some(address_type) => {
                    return Err(anyhow!("Unsupported address type for signing: {:?}", address_type));
                },
                None => {
                    return Err(anyhow!("Unknown address type for input {}", input_index));
                }
            }
        }
    
        tracing::info!("Successfully signed transaction with {} inputs", selected_utxos.len());
        Ok(())
    }
    
    fn get_change_address(&self, keypair: &bitcoin::key::Keypair) -> Result<Address> {
        let network = match std::env::var("BITCOIN_NETWORK").unwrap_or_else(|_| "regtest".to_string()).as_str() {
            "mainnet" => bitcoin::Network::Bitcoin,
            "testnet" => bitcoin::Network::Testnet,
            "signet" => bitcoin::Network::Signet,
            _ => bitcoin::Network::Regtest,
        };
    
        let pubkey = keypair.public_key();
        let pubkey_bytes = pubkey.serialize();
        let wpkh = bitcoin::key::CompressedPublicKey::from_slice(&pubkey_bytes)
            .map_err(|e| anyhow!("Failed to create WPKH: {}", e))?;
        let address = bitcoin::Address::p2wpkh(&wpkh, network);
    
        Ok(address)
    }
}