//! Runtime (lease management) handlers

use axum::{
    extract::{Json, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::{AppState, error::AppError};

#[derive(Debug, Deserialize)]
pub struct RuntimeConnectRequest {
    pub client_id: Option<String>,
    pub client_type: Option<String>,
    pub repository_id: Option<String>,
    pub session_id: Option<String>,
    pub workflow_id: Option<String>,
    pub last_event_seq: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct RuntimeConnectResponse {
    pub lease_id: String,
    pub repository_id: Option<String>,
    pub session_id: Option<String>,
    pub workflow_id: Option<String>,
}

pub async fn connect(
    State(state): State<crate::AppState>,
    Json(request): Json<RuntimeConnectRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let client_id = request.client_id.unwrap_or_else(|| "anonymous".to_string());
    let client_type = request.client_type.unwrap_or_else(|| "unknown".to_string());
    
    let result = state.runtime_manager.connect(
        client_id,
        request.client_type,
        request.repository_id,
        request.session_id,
        request.workflow_id,
        request.last_event_seq,
    ).await?;
    
    Ok(axum::Json(json!({
        "lease_id": result.lease_id,
        "repository_id": request.repository_id,
        "session_id": request.session_id,
        "workflow_id": request.workflow_id
    })))
}

#[derive(Debug, Deserialize)]
pub struct HeartbeatRequest {
    pub lease_id: String,
    #[serde(default)]
    pub last_event_seq: Option<i64>,
    pub session_id: Option<String>,
    pub workflow_id: Option<String>,
}

pub async fn heartbeat(
    State(state): State<crate::AppState>,
    Json(request): Json<HeartbeatRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let result = state.runtime_manager.heartbeat(
        &request.lease_id,
        request.last_event_seq,
        request.session_id,
        request.workflow_id,
    ).await?;
    
    Ok(axum::Json(json!({
        "status": "ok",
        "lease_id": result.lease_id
    })))
}

#[derive(Debug, Deserialize)]
pub struct DisconnectRequest {
    pub lease_id: Option<String>,
    pub client_id: Option<String>,
}

pub async fn disconnect(
    State(state): State<crate::AppState>,
    Json(request): Json<DisconnectRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let lease_id = request.lease_id
        .or_else(|| {
            // Try to find lease by client_id
            None
        });
    
    let result = state.runtime_manager.disconnect(
        lease_id.as_deref(),
        request.client_id.as_deref(),
    ).await?;
    
    Ok(axum::Json(json!({
        "status": "disconnected"
    })))
}

pub async fn status(
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let active_repo_id = state.repository_manager.get_active_repo_id().await;
    let snapshot_id = if let Some(repo_id) = &active_repo_id {
        state.repository_manager.get_repository(repo_id).await?
            .and_then(|r| r.latest_snap_id.clone())
    } else {
        None
    };
    
    let active_session_id = state.repository_manager.get_active_session().await;
    let clients = state.runtime_manager.list_clients().await;
    
    Ok(axum::Json(json!({
        "protocol_version": "v1",
        "active_repo_id": active_repo_id,
        "session_id": active_session_id,
        "workflow_id": null,
        "snapshot_id": snapshot_id,
        "event_seq": 0,
        "lease_id": null,
        "connected_clients": clients,
        "replay": { "ordered": true, "idempotent": true }
    })))
}