#![allow(unused_features, dead_code)]
use anyhow::{anyhow, Result};
use bip39::{Language, Mnemonic};
use bitcoin::bip32::{DerivationPath, Xpriv};
use bitcoin::key::{Keypair, Secp256k1};
use bitcoin::secp256k1::SecretKey;
use bitcoin::Network;
use std::fs;
use std::path::Path;
use std::str::FromStr;

// manages wallet keys using BIP39 mnemonics
pub struct KeyManager {
    storage_path: String,
    network: Network,
}

impl KeyManager {
    pub fn new(storage_path: &str, network: Network) -> Self {
        Self {
            storage_path: storage_path.to_string(),
            network,
        }
    }

    // generate a new wallet with a random mnemonic
    pub fn generate_new_wallet(&self) -> Result<(Keypair, String)> {
        // generate a new mnemonic with 24 words
        let mut rng = bip39::rand::thread_rng();
        let mnemonic = Mnemonic::generate_in_with(&mut rng, Language::English, 24)
            .map_err(|e| anyhow!("Failed to generate mnemonic: {}", e))?;
        
        let phrase = mnemonic.to_string();

        // derive keypair from mnemonic
        let keypair = self.keypair_from_mnemonic(&phrase)?;

        // [TODO!!: Encrypt this file] save mnemonic to file
        let mnemonic_path = Path::new(&self.storage_path).join("mnemonic.txt");
        if let Some(parent) = mnemonic_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&mnemonic_path, &phrase)?;

        tracing::info!("Generated new wallet with mnemonic");
        Ok((keypair, phrase))
    }

    
    // returns: (keypair, mnemonic phrase)
    pub fn load_or_create_wallet(&self) -> Result<(Keypair, String)> {
        let mnemonic_path = Path::new(&self.storage_path).join("mnemonic.txt");

        if mnemonic_path.exists() {
            // load existing mnemonic
            let phrase = fs::read_to_string(&mnemonic_path)?;
            let keypair = self.keypair_from_mnemonic(&phrase)?;
            tracing::info!("Loaded existing wallet from mnemonic");
            Ok((keypair, phrase))
        } else {
            // generate new wallet
            self.generate_new_wallet()
        }
    }


    // returns: Bitcoin keypair
    fn keypair_from_mnemonic(&self, phrase: &str) -> Result<Keypair> {
        // parse the mnemonic phrase
        let mnemonic = Mnemonic::parse_in(Language::English, phrase)
            .map_err(|e| anyhow!("Invalid mnemonic: {}", e))?;

        // generate seed from mnemonic (using empty passphrase)
        let seed = mnemonic.to_seed("");

        // derive master key using BIP32
        let secp = Secp256k1::new();
        let master_key = Xpriv::new_master(self.network, &seed)
            .map_err(|e| anyhow!("Failed to derive master key: {}", e))?;

        // derive account key (m/84'/0'/0'/0/0 for BIP84 SegWit)
        let path = DerivationPath::from_str("m/84'/0'/0'/0/0")
            .map_err(|e| anyhow!("Invalid derivation path: {}", e))?;
        let child_key = master_key
            .derive_priv(&secp, &path)
            .map_err(|e| anyhow!("Failed to derive child key: {}", e))?;

        // convert to keypair
        let secret_key = SecretKey::from_slice(&child_key.private_key.secret_bytes())
            .map_err(|e| anyhow!("Invalid secret key: {}", e))?;
        let keypair = Keypair::from_secret_key(&secp, &secret_key);

        Ok(keypair)
    }

    
    // returns: Bitcoin keypair
    pub fn import_wallet(&self, phrase: &str) -> Result<Keypair> {
        // validate and derive keypair from mnemonic
        let keypair = self.keypair_from_mnemonic(phrase)?;

        // [TODO!!: Encrypt it] save mnemonic to file
        let mnemonic_path = Path::new(&self.storage_path).join("mnemonic.txt");
        if let Some(parent) = mnemonic_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&mnemonic_path, phrase)?;

        tracing::info!("Imported wallet from mnemonic");
        Ok(keypair)
    }

    
    // retuns: BIP39 mnemonic phrase
    pub fn get_mnemonic(&self) -> Result<String> {
        let mnemonic_path = Path::new(&self.storage_path).join("mnemonic.txt");
        if !mnemonic_path.exists() {
            return Err(anyhow!("No wallet found"));
        }

        let phrase = fs::read_to_string(&mnemonic_path)?;
        Ok(phrase)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_generate_new_wallet() {
        let temp_dir = tempdir().unwrap();
        let key_manager = KeyManager::new(
            temp_dir.path().to_str().unwrap(),
            Network::Regtest,
        );

        let (keypair, phrase) = key_manager.generate_new_wallet().unwrap();

        // Verify the mnemonic was saved
        let mnemonic_path = temp_dir.path().join("mnemonic.txt");
        assert!(mnemonic_path.exists());

        // Verify the saved mnemonic matches the returned phrase
        let saved_phrase = fs::read_to_string(mnemonic_path).unwrap();
        assert_eq!(phrase, saved_phrase);

        // Verify we can derive a keypair from the phrase
        let derived_keypair = key_manager.keypair_from_mnemonic(&phrase).unwrap();
        assert_eq!(
            keypair.public_key().to_string(),
            derived_keypair.public_key().to_string()
        );
    }

    #[test]
    fn test_load_or_create_wallet() {
        let temp_dir = tempdir().unwrap();
        let key_manager = KeyManager::new(
            temp_dir.path().to_str().unwrap(),
            Network::Regtest,
        );

        // First call should create a new wallet
        let (keypair1, _) = key_manager.load_or_create_wallet().unwrap();

        // Second call should load the existing wallet
        let (keypair2, _) = key_manager.load_or_create_wallet().unwrap();

        // Verify both keypairs are the same
        assert_eq!(
            keypair1.public_key().to_string(),
            keypair2.public_key().to_string()
        );
    }

    #[test]
    fn test_import_wallet() {
        let temp_dir = tempdir().unwrap();
        let key_manager = KeyManager::new(
            temp_dir.path().to_str().unwrap(),
            Network::Regtest,
        );

        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

        let keypair = key_manager.import_wallet(phrase).unwrap();

        // verify the mnemonic was saved
        let mnemonic_path = temp_dir.path().join("mnemonic.txt");
        assert!(mnemonic_path.exists());

        // verify saved mnemonic matches the input phrase
        let saved_phrase = fs::read_to_string(mnemonic_path).unwrap();
        assert_eq!(phrase, saved_phrase);

        // verify we get the same keypair when loading
        let (loaded_keypair, _) = key_manager.load_or_create_wallet().unwrap();
        assert_eq!(
            keypair.public_key().to_string(),
            loaded_keypair.public_key().to_string()
        );
    }
}