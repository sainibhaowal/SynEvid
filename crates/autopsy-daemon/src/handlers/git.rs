//! Git handlers

use axum::{
    extract::{Query, Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::{AppState, error::AppError};

#[derive(Debug, Deserialize)]
pub struct GalaxyQuery {
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize { 50 }

pub async fn galaxy(
    Query(params): Query<GalaxyQuery>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo_root = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?
        .root_path.clone();
    
    let engine = autopsy_git_engine::GitReader::new(&repo_root)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    let galaxy = engine.galaxy(params.limit)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    Ok(axum::Json(json!({
        "repo_id": repo_id,
        "head_oid": galaxy.head_oid,
        "active_branch": galaxy.active_branch,
        "total_commits": galaxy.total_commits,
        "commits": galaxy.commits,
        "branches": galaxy.branches,
        "tags": galaxy.tags
    })))
}

#[derive(Debug, Deserialize)]
pub struct TimeTravelQuery {
    pub dim: Option<String>,
}

pub async fn time_travel(
    Path(oid): Path<String>,
    Query(params): Query<TimeTravelQuery>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo_root = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?
        .root_path.clone();
    
    let reader = autopsy_git_engine::GitReader::new(&repo_root)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    let dim_3d = params.dim.as_deref() != Some("2d");
    let frame = reader.time_travel(&oid, dim_3d)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    Ok(axum::Json(json!({
        "oid": frame.oid,
        "short_oid": frame.short_oid,
        "summary": frame.summary,
        "timestamp": frame.timestamp,
        "dimensions": if frame.dimensions == "3D" { "3D" } else { "2D" },
        "node_count": frame.node_count,
        "edge_count": frame.edge_count,
        "nodes": frame.nodes,
        "edges": frame.edges,
        "added_files": frame.added_files,
        "modified_files": frame.modified_files,
        "deleted_files": frame.deleted_files,
        "coverage_state": frame.coverage_state,
        "result_digest": frame.result_digest
    })))
}

#[derive(Debug, Deserialize)]
pub struct BlameQuery {
    pub oid: Option<String>,
}

pub async fn blame(
    Query(params): Query<BlameQuery>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let path = params.path.ok_or_else(|| crate::error::AppError::Validation("path parameter required".to_string()))?;
    let oid = params.oid.unwrap_or_else(|| "HEAD".to_string());
    
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo_root = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?
        .root_path.clone();
    
    let reader = autopsy_git_engine::GitReader::new(&repo_root)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    let blame_engine = autopsy_git_engine::GitBlameEngine::new(reader);
    
    let blame = blame_engine.blame(&path, Some(&oid))
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    Ok(axum::Json(json!({
        "path": blame.path,
        "lines": blame.lines,
        "coverage_state": blame.coverage_state,
        "result_digest": blame.result_digest
    })))
}

#[derive(Debug, Deserialize)]
pub struct DiffQuery {
    pub base: String,
    pub head: String,
}

pub async fn diff(
    Query(params): Query<DiffQuery>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo_root = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?
        .root_path.clone();
    
    let reader = autopsy_git_engine::GitReader::new(&repo_root)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    let diff_engine = autopsy_git_engine::GitDiffEngine::new(reader);
    
    let diff = diff_engine.diff(&params.base, &params.head)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    Ok(axum::Json(json!({
        "base_oid": diff.base_oid,
        "head_oid": diff.head_oid,
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

#[derive(Debug, Deserialize)]
pub struct StageRequest {
    pub paths: Vec<String>,
}

pub async fn stage(
    State(state): State<crate::AppState>,
    Json(request): Json<StageRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo_root = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?
        .root_path.clone();
    
    let mut manager = autopsy_git_engine::GitMutationManager::new(&repo_root)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    let result = manager.stage(request.paths)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    Ok(axum::Json(json!({
        "success": true,
        "status": "staged",
        "action": "stage",
        "message": format!("Staged {} files", result.staged_count)
    })))
}

#[derive(Debug, Deserialize)]
pub struct CommitRequest {
    pub message: String,
    pub tag: Option<String>,
    pub author: Option<String>,
    pub email: Option<String>,
}

pub async fn commit(
    State(state): State<crate::AppState>,
    Json(request): Json<CommitRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo_root = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?
        .root_path.clone();
    
    let mut manager = autopsy_git_engine::GitMutationManager::new(&repo_root)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    let result = manager.commit(&request.message, request.author.as_deref(), request.email.as_deref())
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    Ok(axum::Json(json!({
        "success": true,
        "status": "committed",
        "action": "commit",
        "new_commit_oid": result.commit_oid,
        "undo_checkpoint_id": result.undo_checkpoint_id,
        "message": result.message
    })))
}

#[derive(Debug, Deserialize)]
pub struct BranchRequest {
    pub name: Option<String>,
    pub action: String, // create, switch, delete
}

pub async fn branch(
    State(state): State<crate::AppState>,
    Json(request): Json<BranchRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo_root = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?
        .root_path.clone();
    
    let mut manager = autopsy_git_engine::GitMutationManager::new(&repo_root)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    let branch_request = autopsy_git_engine::BranchRequest {
        name: request.name,
        action: request.action,
    };
    
    let result = manager.branch(branch_request)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    Ok(axum::Json(json!({
        "success": true,
        "status": format!("branch_{}", request.action),
        "action": format!("branch_{}", request.action),
        "message": result.message
    })))
}

pub async fn undo(
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo_root = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?
        .root_path.clone();
    
    let mut manager = autopsy_git_engine::GitMutationManager::new(&repo_root)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    let result = manager.undo()
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    Ok(axum::Json(json!({
        "success": true,
        "status": "undone",
        "action": "undo",
        "new_commit_oid": result.restored_oid,
        "message": result.message
    })))
}

#[derive(Debug, Deserialize)]
pub struct TagRequest {
    pub tag_name: String,
    pub target_oid: Option<String>,
    pub message: Option<String>,
    pub action: Option<String>, // create, delete
}

pub async fn tag(
    State(state): State<crate::AppState>,
    Json(request): Json<TagRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo_root = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?
        .root_path.clone();
    
    let mut manager = autopsy_git_engine::GitMutationManager::new(&repo_root)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    let tag_request = autopsy_git_engine::TagRequest {
        tag_name: request.tag_name,
        target_oid: request.target_oid,
        message: request.message,
        action: request.action.unwrap_or_else(|| "create".to_string()),
    };
    
    let result = manager.tag(tag_request)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    Ok(axum::Json(json!({
        "success": true,
        "status": format!("tag_{}", request.action.unwrap_or("create")),
        "action": format!("tag_{}", request.action.unwrap_or("create")),
        "message": result.message
    })))
}

#[derive(Debug, Deserialize)]
pub struct MergeRequest {
    pub source_branch: String,
    pub strategy: Option<String>,
}

pub async fn merge(
    State(state): State<crate::AppState>,
    Json(request): Json<MergeRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo_root = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?
        .root_path.clone();
    
    let mut manager = autopsy_git_engine::GitMutationManager::new(&repo_root)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    let merge_request = autopsy_git_engine::MergeRequest {
        source_branch: request.source_branch,
        strategy: request.strategy,
    };
    
    let result = manager.merge(merge_request)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    Ok(axum::Json(json!({
        "success": true,
        "status": "merged",
        "action": "merge",
        "new_commit_oid": result.merge_commit_oid,
        "undo_checkpoint_id": result.undo_checkpoint_id,
        "message": result.message
    })))
}

#[derive(Debug, Deserialize)]
pub struct CheckoutRequest {
    pub target: String,
    pub detach: Option<bool>,
}

pub async fn checkout(
    State(state): State<crate::AppState>,
    Json(request): Json<CheckoutRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo_root = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?
        .root_path.clone();
    
    let mut manager = autopsy_git_engine::GitMutationManager::new(&repo_root)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    let checkout_request = autopsy_git_engine::CheckoutRequest {
        target: request.target,
        detach: request.detach.unwrap_or(false),
    };
    
    let result = manager.checkout(checkout_request)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    Ok(axum::Json(json!({
        "success": true,
        "status": "checked_out",
        "action": "checkout",
        "new_commit_oid": result.new_head_oid,
        "undo_checkpoint_id": result.undo_checkpoint_id,
        "message": result.message
    })))
}

#[derive(Debug, Deserialize)]
pub struct RevertRequest {
    pub commit_oid: String,
}

pub async fn revert(
    State(state): State<crate::AppState>,
    Json(request): Json<RevertRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo_root = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?
        .root_path.clone();
    
    let mut manager = autopsy_git_engine::GitMutationManager::new(&repo_root)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    let revert_request = autopsy_git_engine::RevertRequest {
        commit_oid: request.commit_oid,
    };
    
    let result = manager.revert(revert_request)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    Ok(axum::Json(json!({
        "success": true,
        "status": "reverted",
        "action": "revert",
        "new_commit_oid": result.revert_commit_oid,
        "undo_checkpoint_id": result.undo_checkpoint_id,
        "message": result.message
    })))
}

#[derive(Debug, Deserialize)]
pub struct BisectRequest {
    pub good: String,
    pub bad: String,
    pub rule_id: Option<String>,
}

pub async fn bisect(
    State(state): State<crate::AppState>,
    Json(request): Json<BisectRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo_root = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?
        .root_path.clone();
    
    let reader = autopsy_git_engine::GitReader::new(&repo_root)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    let bisect_request = autopsy_git_engine::BisectRequest {
        start_oid: request.good,
        end_oid: request.bad,
        rule_id: request.rule_id,
    };
    
    let result = reader.bisect(bisect_request)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    Ok(axum::Json(json!({
        "breaking_commit_oid": result.breaking_commit_oid,
        "breaking_summary": result.breaking_summary,
        "breaking_author": result.breaking_author,
        "breaking_timestamp": result.breaking_timestamp,
        "verdict": result.verdict,
        "drifted_symbols": result.drifted_symbols,
        "commits_evaluated": result.commits_evaluated,
        "total_scanned": result.total_scanned
    })))
}

pub async fn status(
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo_root = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?
        .root_path.clone();
    
    let scanner = autopsy_git_engine::GitStatusScanner::new(&repo_root)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    let status = scanner.scan_status()
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    Ok(axum::Json(json!({
        "branch": status.branch,
        "head_oid": status.head_oid,
        "staged": status.staged,
        "unstaged": status.unstaged,
        "untracked": status.untracked,
        "deleted": status.deleted,
        "ignored": status.ignored,
        "is_clean": status.is_clean,
        "total_uncommitted": status.total_uncommitted
    })))
}

#[derive(Debug, Deserialize)]
pub struct UnstageRequest {
    pub paths: Option<Vec<String>>,
}

pub async fn unstage(
    State(state): State<crate::AppState>,
    Json(request): Json<UnstageRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo_root = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?
        .root_path.clone();
    
    let mut manager = autopsy_git_engine::GitMutationManager::new(&repo_root)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    let result = manager.unstage(request.paths)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    Ok(axum::Json(json!({
        "success": true,
        "status": "unstaged",
        "action": "unstage",
        "message": format!("Unstaged {} files", result.unstaged_count)
    })))
}

#[derive(Debug, Deserialize)]
pub struct TerminalExecRequest {
    pub command: String,
}

pub async fn terminal_exec(
    State(state): State<crate::AppState>,
    Json(request): Json<TerminalExecRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo_root = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?
        .root_path.clone();
    
    let result = autopsy_git_engine::execute_terminal(&repo_root, &request.command)
        .map_err(|e| crate::error::AppError::Git(e.to_string()))?;
    
    Ok(axum::Json(json!({
        "command": result.command,
        "stdout": result.stdout,
        "stderr": result.stderr,
        "exit_code": result.exit_code
    })))
}