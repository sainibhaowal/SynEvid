//! Forensic handlers

use axum::{
    extract::{Query, Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::{AppState, error::AppError};

#[derive(Debug, Deserialize)]
pub struct StartSessionRequest {
    pub repo_id: Option<String>,
    pub baseline_snap_id: Option<String>,
    pub actor_kind: Option<String>,
    pub actor_name: Option<String>,
    pub session_id: Option<String>,
}

pub async fn start_session(
    State(state): State<crate::AppState>,
    Json(request): Json<StartSessionRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = request.repo_id
        .or_else(|| state.repository_manager.get_active_repo_id().await)
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let session_id = request.session_id.unwrap_or_else(|| {
        use blake3;
        format!("{}-{}", repo_id, uuid::Uuid::new_v4().to_string())
    });
    
    let baseline_snap_id = request.baseline_snap_id;
    let actor_kind = request.actor_kind.unwrap_or_else(|| "AI_AGENT".to_string());
    let actor_name = request.actor_name.unwrap_or_else(|| "Unknown".to_string());
    
    // Store session
    state.repository_manager.create_forensic_session(
        &session_id,
        &repo_id,
        &actor_kind,
        &actor_name,
        baseline_snap_id.as_deref(),
    ).await?;
    
    Ok(axum::Json(json!({
        "session_id": session_id,
        "status": "ACTIVE"
    })))
}

#[derive(Debug, Deserialize)]
pub struct RecordEventRequest {
    pub session_id: Option<String>,
    pub repo_root: Option<String>,
    pub job_id: Option<String>,
    pub workflow_id: Option<String>,
    pub event_type: String,
    pub target_symbol: Option<String>,
    pub target_file: Option<String>,
    pub before_snap_id: Option<String>,
    pub after_snap_id: Option<String>,
    pub verdict: Option<String>,
    pub summary: String,
    pub evidence: serde_json::Value,
    pub tokens_spent: Option<i64>,
    pub tokens_saved: Option<i64>,
}

pub async fn record_event(
    State(state): State<crate::AppState>,
    Json(request): Json<RecordEventRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let session_id = request.session_id
        .ok_or_else(|| crate::error::AppError::Validation("session_id required".to_string()))?;
    
    let event_seq = state.repository_manager.record_forensic_event(
        &session_id,
        request.repo_root.as_deref(),
        request.job_id.as_deref(),
        request.workflow_id.as_deref(),
        &request.event_type,
        request.target_symbol.as_deref(),
        request.target_file.as_deref(),
        request.before_snap_id.as_deref(),
        request.after_snap_id.as_deref(),
        request.verdict.as_deref(),
        &request.summary,
        request.evidence,
        request.tokens_spent,
        request.tokens_saved,
    ).await?;
    
    Ok(axum::Json(json!({
        "event_seq": event_seq,
        "blake3_receipt": format!("blake3:{}", blake3::hash(serde_json::to_string(&json!({
            "session_id": session_id,
            "event_type": request.event_type,
            "summary": request.summary
        })).unwrap().as_bytes()).to_hex())
    })))
}

#[derive(Debug, Deserialize)]
pub struct RevertRequest {
    pub session_id: String,
    pub event_seq: i64,
}

pub async fn revert(
    State(state): State<crate::AppState>,
    Json(request): Json<RevertRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let result = state.repository_manager.revert_forensic_step(
        &request.session_id,
        request.event_seq,
    ).await?;
    
    Ok(axum::Json(json!({
        "reverted_event_seq": request.event_seq,
        "revert_event_seq": result.revert_event_seq,
        "status": "reverted"
    })))
}

pub async fn sessions(
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let sessions = state.repository_manager.list_forensic_sessions(None).await?;
    
    Ok(axum::Json(json!({
        "sessions": sessions
    })))
}

#[derive(Debug, Deserialize)]
pub struct TimelineQuery {
    pub session_id: Option<String>,
}

pub async fn timeline(
    Query(params): Query<TimelineQuery>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let session_id = params.session_id
        .ok_or_else(|| crate::error::AppError::Validation("session_id required".to_string()))?;
    
    let timeline = state.repository_manager.get_forensic_timeline(&params.session_id.unwrap()).await?;
    
    Ok(axum::Json(json!({
        "session": timeline.session,
        "workflows": timeline.workflows,
        "events": timeline.events,
        "ledger": timeline.ledger
    })))
}

#[derive(Debug, Deserialize)]
pub struct EventQuery {
    pub session_id: Option<String>,
}

pub async fn event(
    Query(params): Query<EventQuery>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    // Implementation
    Ok(axum::Json(json!({})))
}

#[derive(Debug, Deserialize)]
pub struct ExportRequest {
    pub session_id: String,
}

pub async fn export(
    Query(params): Query<ExportRequest>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    // Implementation
    Ok(axum::Json(json!({})))
}

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    pub bundle: serde_json::Value,
}

pub async fn import(
    State(state): State<crate::AppState>,
    Json(request): Json<ImportRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    // Implementation
    Ok(axum::Json(json!({})))
}