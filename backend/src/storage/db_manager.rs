#![allow(unused_features, dead_code)]
use anyhow::{Result, anyhow};
use rusqlite::{Connection, params};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct DbManager {
    conn: Arc<Mutex<Connection>>,
}

impl DbManager {
    pub async fn new(db_path: &str) -> Result<Self> {
        if let Some(parent) = Path::new(db_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;
        
        let manager = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        
        manager.init_schema().await?;
        
        Ok(manager)
    }

    async fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().await;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS wallets (
                wallet_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                last_accessed INTEGER,
                is_active BOOLEAN DEFAULT 1
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS wallet_keys (
                wallet_id TEXT PRIMARY KEY,
                encrypted_seed TEXT NOT NULL,
                public_key TEXT NOT NULL,
                FOREIGN KEY (wallet_id) REFERENCES wallets(wallet_id)
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS wallet_balances (
                wallet_id TEXT PRIMARY KEY,
                onchain_confirmed INTEGER DEFAULT 0,
                onchain_pending INTEGER DEFAULT 0,
                offchain_confirmed INTEGER DEFAULT 0,
                offchain_pending INTEGER DEFAULT 0,
                last_updated INTEGER,
                FOREIGN KEY (wallet_id) REFERENCES wallets(wallet_id)
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS wallet_transactions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                wallet_id TEXT NOT NULL,
                txid TEXT NOT NULL,
                amount INTEGER NOT NULL,
                timestamp INTEGER NOT NULL,
                type_name TEXT NOT NULL,
                is_settled BOOLEAN,
                raw_tx TEXT,
                FOREIGN KEY (wallet_id) REFERENCES wallets(wallet_id)
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS wallet_addresses (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                wallet_id TEXT NOT NULL,
                address TEXT NOT NULL,
                address_type TEXT NOT NULL,
                derivation_index INTEGER,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (wallet_id) REFERENCES wallets(wallet_id)
            )",
            [],
        )?;

        // for backward compat
        conn.execute(
            "CREATE TABLE IF NOT EXISTS transactions (
                txid TEXT PRIMARY KEY,
                amount INTEGER NOT NULL,
                timestamp INTEGER NOT NULL,
                type_name TEXT NOT NULL,
                is_settled BOOLEAN,
                raw_tx TEXT
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS secret_keys (
                public_key TEXT PRIMARY KEY,
                secret_key TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;

        Ok(())
    }

    pub async fn get_conn(&self) -> Result<tokio::sync::MutexGuard<'_, Connection>> {
        Ok(self.conn.lock().await)
    }
    
    pub async fn save_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.get_conn().await?;
        
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)",
            params![key, value],
        )?;
        
        Ok(())
    }
    
    pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.get_conn().await?;
        
        let value = conn.query_row(
            "SELECT value FROM settings WHERE key = ?",
            params![key],
            |row| row.get(0),
        );
        
        match value {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow!("Storage error: {}", e)),
        }
    }
}