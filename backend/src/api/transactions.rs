#![allow(unused_imports, unused_variables, unused_assignments, dead_code, unused_features)]
use axum::{
    extract::{Json, Path},
    response::IntoResponse,
    http::StatusCode,
};
use crate::services::transactions;

pub async fn get_history() -> impl IntoResponse {
    tracing::info!("API: Received request for transaction history");

    match transactions::get_transaction_history().await {
        Ok(history) => {
            tracing::info!("API: Successfully retrieved {} transactions", history.len());
            (StatusCode::OK, Json(history)).into_response()
        },
        Err(e) => {
            tracing::error!("Error getting transaction history: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn get_transaction(Path(txid): Path<String>) -> impl IntoResponse {
    match transactions::get_transaction(txid).await {
        Ok(tx) => (StatusCode::OK, Json(tx)).into_response(),
        Err(e) => {
            tracing::error!("Error getting transaction: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn participate_in_round() -> impl IntoResponse {
    tracing::info!("API: Received request for round participation");
    
    match tokio::time::timeout(
        std::time::Duration::from_secs(30),
        transactions::participate_in_round()
    ).await {
        Ok(result) => match result {
            Ok(Some(txid)) => {
                tracing::info!("API: Successfully participated in round: {}", txid);
                (StatusCode::OK, Json(serde_json::json!({ "txid": txid }))).into_response()
            },
            Ok(None) => {
                tracing::info!("API: No outputs to include in round");
                (StatusCode::OK, Json(serde_json::json!({ 
                    "message": "No outputs to include in round. Make sure you have funded your boarding address."
                }))).into_response()
            },
            Err(e) => {
                tracing::error!("API: Error participating in round: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ 
                    "error": e.to_string() 
                }))).into_response()
            }
        },
        Err(_) => {
            tracing::error!("API: Timeout while participating in round");
            (StatusCode::REQUEST_TIMEOUT, Json(serde_json::json!({ 
                "error": "Operation timed out. This could be due to network issues or a deadlock."
            }))).into_response()
        }
    }
}

pub async fn unilateral_exit(Json(request): Json<crate::models::wallet::ExitRequest>) -> impl IntoResponse {
    match transactions::unilateral_exit(request.vtxo_txid).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => {
            tracing::error!("Error performing unilateral exit: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}