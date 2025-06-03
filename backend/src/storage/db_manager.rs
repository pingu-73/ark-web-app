#![allow(unused_features, dead_code)]
use anyhow::{Result, anyhow};
use rusqlite::{Connection, params};
use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct DbManager {
    conn: Arc<Mutex<Connection>>,
}

impl DbManager {
    pub fn new(db_path: &str) -> Result<Self> {
        // ensure directory exists
        if let Some(parent) = Path::new(db_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        // open SQLite connection
        let conn = Connection::open(db_path)?;
        
        // create instance
        let manager = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        
        // initialize database schema
        manager.init_schema()?;
        
        Ok(manager)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow!("Failed to lock connection: {}", e))?;
        
        // create tables
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

    pub fn get_conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|e| anyhow!("Failed to lock connection: {}", e))
    }
    
    pub fn save_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.get_conn()?;
        
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)",
            params![key, value],
        )?;
        
        Ok(())
    }
    
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.get_conn()?;
        
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