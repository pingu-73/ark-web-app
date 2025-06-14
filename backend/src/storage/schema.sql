CREATE TABLE IF NOT EXISTS wallets (
    wallet_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_accessed INTEGER,
    is_active BOOLEAN DEFAULT 1
);

CREATE TABLE IF NOT EXISTS wallet_keys (
    wallet_id TEXT PRIMARY KEY,
    encrypted_seed TEXT NOT NULL,
    public_key TEXT NOT NULL,
    FOREIGN KEY (wallet_id) REFERENCES wallets(wallet_id)
);

CREATE TABLE IF NOT EXISTS wallet_balances (
    wallet_id TEXT PRIMARY KEY,
    onchain_confirmed INTEGER DEFAULT 0,
    onchain_pending INTEGER DEFAULT 0,
    offchain_confirmed INTEGER DEFAULT 0,
    offchain_pending INTEGER DEFAULT 0,
    last_updated INTEGER,
    FOREIGN KEY (wallet_id) REFERENCES wallets(wallet_id)
);

CREATE TABLE IF NOT EXISTS wallet_transactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet_id TEXT NOT NULL,
    txid TEXT NOT NULL,
    amount INTEGER NOT NULL,
    timestamp INTEGER NOT NULL,
    type_name TEXT NOT NULL,
    is_settled BOOLEAN,
    raw_tx TEXT,
    FOREIGN KEY (wallet_id) REFERENCES wallets(wallet_id)
);

CREATE TABLE IF NOT EXISTS wallet_addresses (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet_id TEXT NOT NULL,
    address TEXT NOT NULL,
    address_type TEXT NOT NULL, -- 'onchain', 'offchain', 'boarding'
    derivation_index INTEGER,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (wallet_id) REFERENCES wallets(wallet_id)
);