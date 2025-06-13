#![allow(unused_imports, unused_variables, unused_assignments, dead_code, unused_features)]
use axum::{
    extract::{Json, Path},
    response::IntoResponse,
    http::StatusCode,
};
use crate::services::transactions;
use crate::services::offchain::ArkOffChainService;

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
    match crate::services::wallet::participate_in_round().await {
        Ok(Some(txid)) => {
            (StatusCode::OK, Json(serde_json::json!({
                "success": true,
                "round_txid": txid,
                "message": "Successfully participated in round"
            }))).into_response()
        },
        Ok(None) => {
            (StatusCode::OK, Json(serde_json::json!({
                "success": true,
                "message": "No round participation needed at this time"
            }))).into_response()
        },
        Err(e) => {
            tracing::error!("Error participating in round: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn unilateral_exit(Json(request): Json<crate::models::wallet::ExitRequest>) -> impl IntoResponse {
    match crate::services::wallet::unilateral_exit(request.vtxo_txid).await {
        Ok(txid) => {
            (StatusCode::OK, Json(serde_json::json!({
                "success": true,
                "exit_txid": txid,
                "message": "Successfully initiated unilateral exit"
            }))).into_response()
        },
        Err(e) => {
            tracing::error!("Error performing unilateral exit: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}