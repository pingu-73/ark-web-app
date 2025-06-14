mod api;
mod services;
mod models;
mod storage;

use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use dotenv::dotenv;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    // load env vars
    dotenv().ok();
    
    // initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string());
    std::fs::create_dir_all(&data_dir).expect("Failed to create data directory");

    // initialize global APP_STATE first
    match services::initialize_app_state().await {
        Ok(_) => tracing::info!("APP_STATE initialized successfully"),
        Err(e) => tracing::error!("Failed to initialize APP_STATE: {}", e),
    }
    
    // initialize db
    let db_path = format!("{}/arkive.db", data_dir);
    let db_manager = Arc::new(storage::DbManager::new(&db_path).await.unwrap());

    // initialize multi-wallet manager
    let network = match std::env::var("BITCOIN_NETWORK").unwrap_or_else(|_| "regtest".to_string()).as_str() {
        "mainnet" => bitcoin::Network::Bitcoin,
        "testnet" => bitcoin::Network::Testnet,
        "signet" => bitcoin::Network::Signet,
        _ => bitcoin::Network::Regtest,
    };
    
    let ark_server_url = std::env::var("ARK_SERVER_URL")
        .unwrap_or_else(|_| "http://localhost:7070".to_string());
    
    let wallet_manager = Arc::new(
        services::multi_wallet::MultiWalletManager::new(
            db_manager.clone(),
            network,
            ark_server_url,
        )
    );
    
    let esplora_url = std::env::var("ESPLORA_URL")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());
    
    let faucet_service = Arc::new(
        services::faucet::FaucetService::new(esplora_url, network)
    );
    
    // create API state
    let api_state = api::multi_wallet::ApiState {
        wallet_manager: wallet_manager.clone(),
        faucet_service: faucet_service.clone(),
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

        let app = Router::new()
        // Multi-wallet management
        .route("/api/wallets", post(api::multi_wallet::create_wallet))
        .route("/api/wallets", get(api::multi_wallet::list_wallets))
        .route("/api/wallets/:wallet_id", get(api::multi_wallet::get_wallet_info))
        
        // Balance endpoints
        .route("/api/wallets/:wallet_id/balance", get(api::multi_wallet::get_wallet_balance))
        .route("/api/wallets/:wallet_id/balance/onchain", get(api::multi_wallet::get_onchain_balance))
        .route("/api/wallets/:wallet_id/balance/offchain", get(api::multi_wallet::get_offchain_balance))
        
        // Off-chain operations
        .route("/api/wallets/:wallet_id/send-vtxo", post(api::multi_wallet::send_vtxo))
        .route("/api/wallets/:wallet_id/vtxos", get(api::multi_wallet::get_vtxo_list))
        .route("/api/wallets/:wallet_id/participate-round", post(api::multi_wallet::participate_in_round))
        
        // On-chain operations
        .route("/api/wallets/:wallet_id/send-onchain", post(api::multi_wallet::send_onchain))
        .route("/api/wallets/:wallet_id/fee-estimates", get(api::multi_wallet::get_fee_estimates))
        .route("/api/wallets/:wallet_id/estimate-fee", post(api::multi_wallet::estimate_transaction_fee))
        
        // Transaction history
        .route("/api/wallets/:wallet_id/transactions", get(api::multi_wallet::get_transaction_history))
        
        // Faucet
        .route("/api/faucet", post(api::multi_wallet::faucet_request))
        .route("/api/faucet/info", get(api::multi_wallet::get_faucet_info))
        
        .with_state(api_state)
        .layer(TraceLayer::new_for_http())
        .layer(cors);

    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .expect("PORT must be a number");
    
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await.unwrap();
    
    tracing::info!("Multi-wallet ARKive server listening on {}", addr);
    
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
    tracing::info!("Shutting down gracefully...");
}