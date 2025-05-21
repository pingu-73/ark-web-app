mod api;
mod services;
mod models;
mod storage;

use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use dotenv::dotenv;

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
    
    // create data directory if it doesn't exist
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string());
    std::fs::create_dir_all(&data_dir).expect("Failed to create data directory");

    // initialize Ark client
    match services::APP_STATE.initialize().await {
        Ok(_) => tracing::info!("Ark client initialized successfully"),
        Err(e) => tracing::error!("Failed to initialize Ark client: {}", e),
    }

    let app_state = services::APP_STATE.clone();
    tokio::spawn(async move {
        loop {
            // Sync every 30 seconds
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                
            let grpc_client = app_state.grpc_client.lock().await;
            if grpc_client.is_connected() {
                match grpc_client.update_app_state().await {
                    Ok(_) => tracing::debug!("Successfully synced app state with Ark client"),
                    Err(e) => tracing::warn!("Failed to sync app state with Ark client: {}", e),
                }
            }
        }
    });

    // CORS layer
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);


    let app = Router::new()
        // wallet routes
        .route("/api/wallet/info", get(api::wallet::get_info))
        .route("/api/wallet/balance", get(api::wallet::get_balance))
        .route("/api/wallet/address", get(api::wallet::get_address))
        .route("/api/wallet/boarding-address", get(api::wallet::get_boarding_address))
        .route("/api/wallet/send", post(api::wallet::send_vtxo))
        .route("/api/wallet/available-balance", get(api::wallet::get_available_balance))
        // .route("/api/wallet/check-deposits", post(api::wallet::check_deposits))
        .route("/api/wallet/receive", post(api::wallet::receive_vtxo))
        .route("/api/wallet/recalculate-balance", post(api::wallet::recalculate_balance))

        // on-chain tx
        .route("/api/wallet/send-onchain", post(api::wallet::send_on_chain))
        
        // tx routes
        .route("/api/transactions", get(api::transactions::get_history))
        .route("/api/transactions/:txid", get(api::transactions::get_transaction))
        
        // round participation
        // .route("/api/round/participate", post(api::transactions::participate_in_round))

        // unilateral exit
        .route("/api/transactions/exit", post(api::transactions::unilateral_exit))
        
        // add middleware
        .layer(TraceLayer::new_for_http())
        .layer(cors);

    // run the server
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .expect("PORT must be a number");
    
    let shutdown_signal = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C signal handler");
        tracing::info!("Shutting down gracefully...");
    };

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown_signal)
        .await
        .unwrap();
}