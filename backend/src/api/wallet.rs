#![allow(unused_imports, unused_variables, unused_assignments, dead_code, unused_features)]
use axum::{
    extract::Json,
    response::IntoResponse,
    http::StatusCode,
};
use crate::models::wallet::{SendRequest, SendOnchainRequest, EstimateFeeDetailedRequest};
use crate::services::wallet;
use crate::services::offchain::ArkOffChainService;
use ark_core::ArkAddress;

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

pub async fn get_fee_estimates_detailed() -> impl IntoResponse {
    match wallet::get_detailed_fee_estimates().await {
        Ok(estimates) => (StatusCode::OK, Json(estimates)).into_response(),
        Err(e) => {
            tracing::error!("Error getting fee estimates: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn estimate_transaction_fees(
    Json(request): Json<EstimateFeeDetailedRequest>
) -> impl IntoResponse {
    match wallet::estimate_onchain_fee_detailed(request.address, request.amount).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => {
            tracing::error!("Error estimating transaction fees: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn send_onchain_with_priority(
    Json(request): Json<SendOnchainRequest>
) -> impl IntoResponse {
    let priority = request.priority.unwrap_or_else(|| "normal".to_string());
    
    match wallet::send_onchain_payment_with_fee_priority(
        request.address,
        request.amount,
        priority.into()
    ).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => {
            tracing::error!("Error sending payment: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}


pub async fn send_vtxo(Json(request): Json<SendRequest>) -> impl IntoResponse {
    // Clone the values we need before moving request
    let address = request.address.clone();
    let amount = request.amount;
    
    match crate::services::wallet::send_vtxo(request.address, request.amount).await {
        Ok(txid) => (StatusCode::OK, Json(serde_json::json!({
            "txid": txid,
            "amount": amount,
            "address": address
        }))).into_response(),
        Err(e) => {
            tracing::error!("Error sending VTXO: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn get_offchain_balance() -> impl IntoResponse {
    match crate::services::wallet::get_offchain_balance_detailed().await {
        Ok((confirmed, pending, expired)) => {
            (StatusCode::OK, Json(serde_json::json!({
                "confirmed": confirmed.to_sat(),
                "pending": pending.to_sat(),
                "expired": expired.to_sat(),
                "total": (confirmed + pending).to_sat()
            }))).into_response()
        },
        Err(e) => {
            tracing::error!("Error getting offchain balance: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn get_vtxo_list() -> impl IntoResponse {
    match crate::services::wallet::get_vtxo_list().await {
        Ok(vtxo_list) => {
            (StatusCode::OK, Json(serde_json::json!({
                "vtxos": vtxo_list,
                "count": vtxo_list.len()
            }))).into_response()
        },
        Err(e) => {
            tracing::error!("Error getting VTXO list: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn estimate_vtxo_fee(Json(request): Json<serde_json::Value>) -> impl IntoResponse {
    if let Some(amount_sats) = request.get("amount").and_then(|v| v.as_u64()) {
        // Simple fee estimation for VTXOs (much lower than on-chain)
        let base_fee = 100u64; // 100 sats base fee
        let amount_fee = amount_sats / 10000; // 0.01% of amount
        let total_fee = base_fee + amount_fee;
        
        (StatusCode::OK, Json(serde_json::json!({
            "amount": amount_sats,
            "estimated_fee": total_fee,
            "total": amount_sats + total_fee
        }))).into_response()
    } else {
        (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "Missing or invalid 'amount' field"
        }))).into_response()
    }
}

pub async fn get_service_health() -> impl IntoResponse {
    // Get connection status
    let is_connected = {
        let grpc_client = crate::services::APP_STATE.grpc_client.lock().await;
        grpc_client.is_connected()
    };
    
    // Get VTXO count and balance
    let (vtxo_count, balance) = match crate::services::wallet::get_vtxo_list().await {
        Ok(vtxos) => {
            let count = vtxos.len();
            // Calculate balance from the JSON values
            let total_confirmed = vtxos.iter()
                .filter_map(|v| v.get("amount").and_then(|a| a.as_u64()))
                .sum::<u64>();
            (count, (total_confirmed, 0u64)) // pending is 0 for now
        },
        Err(_) => (0, (0, 0))
    };
    
    (StatusCode::OK, Json(serde_json::json!({
        "status": if is_connected { "Healthy" } else { "Disconnected" },
        "grpc_connected": is_connected,
        "vtxo_count": vtxo_count,
        "balance_confirmed": balance.0,
        "balance_pending": balance.1,
        "round_active": false, // Placeholder
        "exit_recommendations": 0, // Placeholder
        "is_healthy": is_connected
    }))).into_response()
}

pub async fn get_exit_recommendations() -> impl IntoResponse {
    // [Dummy Impl] For now, return empty recommendations
    // In a real implementation, this would check for VTXOs near expiry, etc.
    (StatusCode::OK, Json(serde_json::json!({
        "recommendations": [],
        "count": 0,
        "message": "Exit recommendations feature coming soon"
    }))).into_response()
}