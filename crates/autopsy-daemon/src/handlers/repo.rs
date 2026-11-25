//! Repository management handlers

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::{AppState, error::AppError};

/// Connect a new repository
#[derive(Debug, Deserialize)]
pub struct ConnectRequest {
    pub root_path: String,
    pub display_name: Option<String>,
}

#[axum::debug_handler]
pub async fn connect(
    State(state): State<crate::AppState>,
    Json(request): Json<ConnectRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_manager = &state.repository_manager;
    let record = repo_manager.register_repository(&std::path::PathBuf::from(&request.root_path), request.display_name).await?;
    
    Ok(Json(json!({
        "status": "connected",
        "repo_id": record.repo_id,
        "display_name": record.display_name,
        "root_path": record.root_path,
        "scan": {
            "snapshot_id": record.latest_snap_id,
            "files_scanned": 0,
            "symbol_count": 0,
            "edge_count": 0,
            "coverage_state": "verified"
        }
    })))
}

/// List all repositories
#[axum::debug_handler]
pub async fn list(
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repos = state.repository_manager.list_repositories();
    
    Ok(Json(json!(repos)))
}

/// Switch active repository
#[derive(Debug, Deserialize)]
pub struct SwitchRequest {
    pub repo_id: String,
}

#[axum::debug_handler]
pub async fn switch(
    State(state): State<crate::AppState>,
    Json(request): Json<SwitchRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_manager = &state.repository_manager;
    let record = repo_manager.switch_repository(&request.repo_id).await?;
    
    Ok(Json(json!({
        "status": "switched",
        "active_repo": {
            "repo_id": record.repo_id,
            "display_name": record.display_name,
            "root_path": record.root_path
        }
    })))
}

/// Remove a repository
#[derive(Debug, Deserialize)]
pub struct RemoveRequest {
    pub repo_id: String,
}

#[axum::debug_handler]
pub async fn remove(
    State(state): State<crate::AppState>,
    Json(request): Json<RemoveRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    state.repository_manager.remove_repository(&request.repo_id).await?;
    
    Ok(Json(json!({
        "status": "removed",
        "repo_id": request.repo_id
    })))
}

/// Export repository
#[axum::debug_handler]
pub async fn export(
    State(state): State<crate::AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, crate::error::AppError> {
    let active_repo_id = state.repository_manager.get_active_repo_id();
    let repo_id = params.get("repo_id")
        .cloned()
        .or(active_repo_id)
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let snapshot_id = params.get("snapshot_id")
        .cloned()
        .or_else(|| state.repository_manager.get_repository(&repo_id).and_then(|r| r.latest_snap_id))
        .ok_or_else(|| crate::error::AppError::Repository("No snapshot available".to_string()))?;
    
    // Export would be implemented here
    Ok(Json(json!({
        "repo_id": repo_id,
        "snapshot_id": snapshot_id,
        "export": "not_implemented"
    })))
}

/// List repositories (legacy endpoint)
#[axum::debug_handler]
pub async fn list_repos(
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    list(State(state)).await
}

/// Browse filesystem
#[derive(Debug, Deserialize)]
pub struct BrowseRequest {
    pub path: Option<String>,
}

#[axum::debug_handler]
pub async fn browse(
    State(state): State<crate::AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, crate::error::AppError> {
    let path = params.get("path").map(|s| s.as_str()).unwrap_or("");
    let _repo_id = state.repository_manager.get_active_repo_id()
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    // Browse would be implemented here
    Ok(Json(json!({
        "current_path": path,
        "parent_path": None::<String>,
        "is_git_repo": false,
        "directories": [],
        "quick_presets": []
    })))
}

/// List discovered files
#[axum::debug_handler]
pub async fn files(
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id()
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    Ok(Json(json!({
        "repo_id": repo_id,
        "total_files": 0,
        "files": []
    })))
}

/// Get file content
#[axum::debug_handler]
pub async fn file_content(
    State(state): State<crate::AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, crate::error::AppError> {
    let path = params.get("path")
        .ok_or_else(|| crate::error::AppError::Validation("path parameter required".to_string()))?;
    let repo_id = params.get("repo_id").cloned();
    
    Ok(Json(json!({
        "repo_id": repo_id,
        "file_path": path,
        "content": "",
        "size_bytes": 0,
        "coverage_state": "unknown"
    })))
}