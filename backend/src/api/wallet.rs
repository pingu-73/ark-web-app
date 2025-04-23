#![allow(unused_imports, unused_variables)]
use axum::{
    extract::Json,
    response::IntoResponse,
    http::StatusCode,
};
use crate::models::wallet::SendRequest;
use crate::services::wallet;

pub async fn get_info() -> impl IntoResponse {
    match wallet::get_wallet_info().await {
        Ok(info) => (StatusCode::OK, Json(info)).into_response(),
        Err(e) => {
            tracing::error!("Error getting wallet info: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn get_balance() -> impl IntoResponse {
    match wallet::get_balance().await {
        Ok(balance) => (StatusCode::OK, Json(balance)).into_response(),
        Err(e) => {
            tracing::error!("Error getting balance: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn get_address() -> impl IntoResponse {
    match wallet::get_offchain_address().await {
        Ok(address) => (StatusCode::OK, Json(address)).into_response(),
        Err(e) => {
            tracing::error!("Error getting address: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn get_boarding_address() -> impl IntoResponse {
    match wallet::get_boarding_address().await {
        Ok(address) => (StatusCode::OK, Json(address)).into_response(),
        Err(e) => {
            tracing::error!("Error getting boarding address: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn send_vtxo(Json(request): Json<SendRequest>) -> impl IntoResponse {
    match wallet::send_vtxo(request.address, request.amount).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => {
            tracing::error!("Error sending VTXO: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn get_available_balance() -> impl IntoResponse {
    match wallet::get_available_balance().await {
        Ok(balance) => (StatusCode::OK, Json(serde_json::json!({
            "available": balance
        }))).into_response(),
        Err(e) => {
            tracing::error!("Error getting available balance: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn check_deposits() -> impl IntoResponse {
    match wallet::check_deposits().await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => {
            tracing::error!("Error checking deposits: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn receive_vtxo(Json(request): Json<crate::models::wallet::ReceiveRequest>) -> impl IntoResponse {
    match wallet::receive_vtxo(request.from_address, request.amount).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => {
            tracing::error!("Error receiving VTXO: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}