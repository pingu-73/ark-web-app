#![allow(unused_imports, unused_variables, unused_assignments, dead_code, unused_features)]
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


pub async fn get_balance() -> impl IntoResponse {
    match crate::services::APP_STATE.recalculate_balance().await {
        Ok(_) => {
            let balance = crate::services::APP_STATE.balance.lock().await.clone();
            (StatusCode::OK, Json(balance)).into_response()
        },
        Err(e) => {
            tracing::error!("Error recalculating balance: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn debug_vtxos() -> impl IntoResponse {
    match wallet::debug_vtxos().await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => {
            tracing::error!("Error debugging VTXOs: {}", e);
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

pub async fn get_onchain_address() -> impl IntoResponse {
    match wallet::get_onchain_address().await {
        Ok(address) => (StatusCode::OK, Json(serde_json::json!({
            "address": address
        }))).into_response(),
        Err(e) => {
            tracing::error!("Error getting onchain address: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn send_onchain_payment(Json(request): Json<SendRequest>) -> impl IntoResponse {
    match wallet::send_onchain_payment(request.address, request.amount).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => {
            tracing::error!("Error sending on-chain payment: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn get_onchain_balance() -> impl IntoResponse {
    match wallet::get_onchain_balance().await {
        Ok(balance) => (StatusCode::OK, Json(serde_json::json!({
            "balance": balance
        }))).into_response(),
        Err(e) => {
            tracing::error!("Error getting on-chain balance: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn estimate_onchain_fee(Json(request): Json<SendRequest>) -> impl IntoResponse {
    match wallet::estimate_onchain_fee(request.address, request.amount).await {
        Ok(fee) => (StatusCode::OK, Json(serde_json::json!({
            "estimated_fee": fee
        }))).into_response(),
        Err(e) => {
            tracing::error!("Error estimating fee: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}