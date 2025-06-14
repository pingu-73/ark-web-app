#![allow(dead_code)]
use axum::{
    extract::{Json, Path, State},
    response::IntoResponse,
    http::StatusCode,
};
use std::sync::Arc;
use ark_core::ArkAddress;
use bitcoin::Amount;
use crate::services::multi_wallet::{ MultiWalletManager, WalletInfo };
use crate::services::faucet::FaucetService;

#[derive(Clone)]
pub struct ApiState {
    pub wallet_manager: Arc<MultiWalletManager>,
    pub faucet_service: Arc<FaucetService>,
}

pub async fn create_wallet(
    State(state): State<ApiState>,
    Json(request): Json<CreateWalletRequest>,
) -> impl IntoResponse {
    match state.wallet_manager.create_wallet(request.name).await {
        Ok(wallet_info) => (StatusCode::CREATED, Json(wallet_info)).into_response(),
        Err(e) => {
            tracing::error!("Error creating wallet: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn list_wallets(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    match state.wallet_manager.list_wallets().await {
        Ok(wallets) => (StatusCode::OK, Json(wallets)).into_response(),
        Err(e) => {
            tracing::error!("Error listing wallets: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn get_wallet_info(
    State(state): State<ApiState>,
    Path(wallet_id): Path<String>,
) -> impl IntoResponse {
    match state.wallet_manager.get_wallet(&wallet_id).await {
        Ok(wallet) => {
            let addresses = state.wallet_manager.get_wallet_addresses(&wallet_id).await
                .unwrap_or_default();
            
            let info = WalletInfo {
                wallet_id: wallet.wallet_id.clone(),
                name: wallet.name.clone(),
                addresses,
                created_at: wallet.created_at,
            };
            
            (StatusCode::OK, Json(info)).into_response()
        },
        Err(e) => {
            tracing::error!("Error getting wallet: {}", e);
            (StatusCode::NOT_FOUND, Json(serde_json::json!({
                "error": "Wallet not found"
            }))).into_response()
        }
    }
}

pub async fn get_wallet_balance(
    State(state): State<ApiState>,
    Path(wallet_id): Path<String>,
) -> impl IntoResponse {
    match state.wallet_manager.get_wallet(&wallet_id).await {
        Ok(wallet) => {
            let onchain_balance = match crate::services::wallet::get_onchain_balance().await {
                Ok(balance) => balance,
                Err(_) => 0,
            };
            
            let offchain_balance = match wallet.offchain_service.get_balance().await {
                Ok((confirmed, pending)) => {
                    serde_json::json!({
                        "confirmed": confirmed.to_sat(),
                        "pending": pending.to_sat(),
                        "total": (confirmed + pending).to_sat(),
                    })
                },
                Err(_) => serde_json::json!({
                    "confirmed": 0,
                    "pending": 0,
                    "total": 0,
                }),
            };
            
            (StatusCode::OK, Json(serde_json::json!({
                "wallet_id": wallet_id,
                "onchain": {
                    "total": onchain_balance,
                    "confirmed": onchain_balance,
                    "pending": 0,
                },
                "offchain": offchain_balance,
            }))).into_response()
        },
        Err(_) => {
            (StatusCode::NOT_FOUND, Json(serde_json::json!({
                "error": "Wallet not found"
            }))).into_response()
        }
    }
}

pub async fn send_vtxo(
    State(state): State<ApiState>,
    Path(wallet_id): Path<String>,
    Json(request): Json<SendVtxoRequest>,
) -> impl IntoResponse {
    match state.wallet_manager.get_wallet(&wallet_id).await {
        Ok(wallet) => {
            let ark_address = match ArkAddress::decode(&request.address) {
                Ok(addr) => addr,
                Err(e) => {
                    return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                        "error": format!("Invalid Ark address: {}", e)
                    }))).into_response()
                }
            };
            
            let amount = Amount::from_sat(request.amount);
            let address_str = request.address.clone();

            match wallet.offchain_service.send_vtxo(ark_address, amount).await {
                Ok(txid) => (StatusCode::OK, Json(serde_json::json!({
                    "txid": txid,
                    "amount": request.amount,
                    "address": address_str,
                }))).into_response(),
                Err(e) => {
                    tracing::error!("Error sending VTXO: {}", e);
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                        "error": e.to_string()
                    }))).into_response()
                }
            }
        },
        Err(_) => {
            (StatusCode::NOT_FOUND, Json(serde_json::json!({
                "error": "Wallet not found"
            }))).into_response()
        }
    }
}

pub async fn faucet_request(
    State(state): State<ApiState>,
    Json(request): Json<FaucetRequest>,
) -> impl IntoResponse {
    let address = match request.address_type.as_str() {
        "boarding" => request.address.clone(),
        "onchain" => request.address.clone(),
        _ => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": "Invalid address type. Use 'boarding' or 'onchain'"
            }))).into_response()
        }
    };
    
    match state.faucet_service.send_to_address(&address).await {
        Ok(txid) => {
            (StatusCode::OK, Json(serde_json::json!({
                "txid": txid,
                "amount": 100000,
                "address": address,
                "message": "Funds sent successfully. Mining block..."
            }))).into_response()
        },
        Err(e) => {
            tracing::error!("Faucet error: {}", e);
            (StatusCode::TOO_MANY_REQUESTS, Json(serde_json::json!({
                "error": e.to_string()
            }))).into_response()
        }
    }
}

pub async fn get_faucet_info(
    State(state): State<ApiState>,
) -> impl IntoResponse {
    let info = state.faucet_service.get_info();
    (StatusCode::OK, Json(info)).into_response()
}

pub async fn get_onchain_balance(
    State(state): State<ApiState>,
    Path(wallet_id): Path<String>,
) -> impl IntoResponse {
    match state.wallet_manager.get_wallet(&wallet_id).await {
        Ok(wallet) => {
            match wallet.get_onchain_balance().await {
                Ok(balance) => (StatusCode::OK, Json(serde_json::json!({
                    "wallet_id": wallet_id,
                    "balance": balance,
                }))).into_response(),
                Err(e) => {
                    tracing::error!("Error getting on-chain balance: {}", e);
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                        "error": e.to_string()
                    }))).into_response()
                }
            }
        },
        Err(_) => {
            (StatusCode::NOT_FOUND, Json(serde_json::json!({
                "error": "Wallet not found"
            }))).into_response()
        }
    }
}

pub async fn send_onchain(
    State(state): State<ApiState>,
    Path(wallet_id): Path<String>,
    Json(request): Json<SendOnchainRequest>,
) -> impl IntoResponse {
    match state.wallet_manager.get_wallet(&wallet_id).await {
        Ok(wallet) => {
            let priority = request.priority.unwrap_or_else(|| "normal".to_string());
            
            match wallet.send_onchain_payment(
                request.address.clone(),
                request.amount,
                priority
            ).await {
                Ok(txid) => (StatusCode::OK, Json(serde_json::json!({
                    "txid": txid,
                    "amount": request.amount,
                    "address": request.address,
                }))).into_response(),
                Err(e) => {
                    tracing::error!("Error sending on-chain payment: {}", e);
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                        "error": e.to_string()
                    }))).into_response()
                }
            }
        },
        Err(_) => {
            (StatusCode::NOT_FOUND, Json(serde_json::json!({
                "error": "Wallet not found"
            }))).into_response()
        }
    }
}

pub async fn get_fee_estimates(
    State(state): State<ApiState>,
    Path(wallet_id): Path<String>,
) -> impl IntoResponse {
    match state.wallet_manager.get_wallet(&wallet_id).await {
        Ok(wallet) => {
            match wallet.get_fee_estimates().await {
                Ok(estimates) => (StatusCode::OK, Json(estimates)).into_response(),
                Err(e) => {
                    tracing::error!("Error getting fee estimates: {}", e);
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                        "error": e.to_string()
                    }))).into_response()
                }
            }
        },
        Err(_) => {
            (StatusCode::NOT_FOUND, Json(serde_json::json!({
                "error": "Wallet not found"
            }))).into_response()
        }
    }
}

pub async fn estimate_transaction_fee(
    State(state): State<ApiState>,
    Path(wallet_id): Path<String>,
    Json(request): Json<EstimateFeeRequest>,
) -> impl IntoResponse {
    match state.wallet_manager.get_wallet(&wallet_id).await {
        Ok(wallet) => {
            match wallet.estimate_onchain_fee(request.address, request.amount).await {
                Ok(fee_info) => (StatusCode::OK, Json(fee_info)).into_response(),
                Err(e) => {
                    tracing::error!("Error estimating fee: {}", e);
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                        "error": e.to_string()
                    }))).into_response()
                }
            }
        },
        Err(_) => {
            (StatusCode::NOT_FOUND, Json(serde_json::json!({
                "error": "Wallet not found"
            }))).into_response()
        }
    }
}

pub async fn get_transaction_history(
    State(state): State<ApiState>,
    Path(wallet_id): Path<String>,
) -> impl IntoResponse {
    match state.wallet_manager.get_wallet(&wallet_id).await {
        Ok(wallet) => {
            match wallet.get_transaction_history().await {
                Ok(history) => (StatusCode::OK, Json(history)).into_response(),
                Err(e) => {
                    tracing::error!("Error getting transaction history: {}", e);
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                        "error": e.to_string()
                    }))).into_response()
                }
            }
        },
        Err(_) => {
            (StatusCode::NOT_FOUND, Json(serde_json::json!({
                "error": "Wallet not found"
            }))).into_response()
        }
    }
}

pub async fn get_offchain_balance(
    State(state): State<ApiState>,
    Path(wallet_id): Path<String>,
) -> impl IntoResponse {
    match state.wallet_manager.get_wallet(&wallet_id).await {
        Ok(wallet) => {
            match wallet.offchain_service.get_balance().await {
                Ok((confirmed, pending)) => {
                    (StatusCode::OK, Json(serde_json::json!({
                        "wallet_id": wallet_id,
                        "confirmed": confirmed.to_sat(),
                        "pending": pending.to_sat(),
                        "total": (confirmed + pending).to_sat(),
                    }))).into_response()
                },
                Err(e) => {
                    tracing::error!("Error getting off-chain balance: {}", e);
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                        "error": e.to_string()
                    }))).into_response()
                }
            }
        },
        Err(_) => {
            (StatusCode::NOT_FOUND, Json(serde_json::json!({
                "error": "Wallet not found"
            }))).into_response()
        }
    }
}

pub async fn get_vtxo_list(
    State(state): State<ApiState>,
    Path(wallet_id): Path<String>,
) -> impl IntoResponse {
    match state.wallet_manager.get_wallet(&wallet_id).await {
        Ok(wallet) => {
            match wallet.offchain_service.get_vtxo_list().await {
                Ok(vtxos) => {
                    
                    let vtxo_data: Vec<_> = vtxos.into_iter().map(|vtxo_state| {
                        let status_str = match vtxo_state.status {
                            crate::services::offchain::VtxoStatus::Pending => "pending",
                            crate::services::offchain::VtxoStatus::Confirmed => "confirmed",
                            crate::services::offchain::VtxoStatus::Spent => "spent",
                            crate::services::offchain::VtxoStatus::Expired => "expired",
                        };
                        
                        serde_json::json!({
                            "address": vtxo_state.vtxo.address().to_string(),
                            "total_amount": vtxo_state.total_amount.to_sat(),
                            "status": status_str,
                            "earliest_expiry": vtxo_state.earliest_expiry,
                            "outpoint_count": vtxo_state.outpoints.len(),
                        })
                    }).collect();
                    
                    (StatusCode::OK, Json(serde_json::json!({
                        "wallet_id": wallet_id,
                        "vtxos": vtxo_data,
                        "count": vtxo_data.len(),
                    }))).into_response()
                },
                Err(e) => {
                    tracing::error!("Error getting VTXO list: {}", e);
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                        "error": e.to_string()
                    }))).into_response()
                }
            }
        },
        Err(_) => {
            (StatusCode::NOT_FOUND, Json(serde_json::json!({
                "error": "Wallet not found"
            }))).into_response()
        }
    }
}

pub async fn participate_in_round(
    State(state): State<ApiState>,
    Path(wallet_id): Path<String>,
) -> impl IntoResponse {
    match state.wallet_manager.get_wallet(&wallet_id).await {
        Ok(wallet) => {
            match wallet.offchain_service.participate_in_round().await {
                Ok(Some(txid)) => {
                    (StatusCode::OK, Json(serde_json::json!({
                        "success": true,
                        "round_txid": txid,
                    }))).into_response()
                },
                Ok(None) => {
                    (StatusCode::OK, Json(serde_json::json!({
                        "success": true,
                        "message": "No round participation needed"
                    }))).into_response()
                },
                Err(e) => {
                    tracing::error!("Error participating in round: {}", e);
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                        "error": e.to_string()
                    }))).into_response()
                }
            }
        },
        Err(_) => {
            (StatusCode::NOT_FOUND, Json(serde_json::json!({
                "error": "Wallet not found"
            }))).into_response()
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateWalletRequest {
    pub name: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct SendVtxoRequest {
    pub address: String,
    pub amount: u64,
}

#[derive(Debug, serde::Deserialize)]
pub struct FaucetRequest {
    pub address: String,
    pub address_type: String, // "boarding" or "onchain"
}

#[derive(Debug, serde::Deserialize)]
pub struct SendOnchainRequest {
    pub address: String,
    pub amount: u64,
    pub priority: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct EstimateFeeRequest {
    pub address: String,
    pub amount: u64,
}