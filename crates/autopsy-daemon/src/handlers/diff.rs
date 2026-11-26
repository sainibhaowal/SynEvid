//! Diff handlers

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::{AppState, error::AppError};

#[derive(Debug, Deserialize)]
pub struct DiffQuery {
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub baseline_id: Option<String>,
}

pub async fn get(
    Query(params): Query<DiffQuery>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?;
    
    let before = params.before
        .or(params.baseline_id)
        .or_else(|| repo.latest_snap_id.clone())
        .ok_or_else(|| crate::error::AppError::Repository("No baseline snapshot".to_string()))?;
    
    let after = params.after;
    
    let storage = state.repository_manager.get_storage(&v1::SnapshotId::new(
        params.after.clone().unwrap_or_else(|| repo.latest_snap_id.clone().unwrap())
    )).await?;
    
    let diff = storage.diff_snapshots(
        &v1::SnapshotId::new(before),
        params.after.as_ref().map(v1::SnapshotId::new).as_ref(),
    ).await
        .map_err(|e| crate::error::AppError::Analysis(e.to_string()))?;
    
    Ok(axum::Json(json!({
        "base_snapshot": params.before,
        "head_snapshot": params.after,
        "added_files": diff.added_files,
        "modified_files": diff.modified_files,
        "deleted_files": diff.deleted_files,
        "total_changed_files": diff.total_changed_files,
        "blast_radius_score": diff.blast_radius_score,
        "verdict": diff.verdict,
        "drifted_contracts": diff.drifted_contracts,
        "result_digest": diff.result_digest
    })))
}

pub async fn post(
    State(state): State<crate::AppState>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<Value>, crate::error::AppError> {
    // Similar to get but with JSON body
    let params = serde_json::from_value::<DiffQuery>(request)
        .map_err(|e| crate::error::AppError::Validation(e.to_string()))?;
    
    get(axum::extract::Query(params), crate::extractors::AppStateExtractor(state)).await
}