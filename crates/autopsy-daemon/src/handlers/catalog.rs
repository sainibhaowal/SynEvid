//! Catalog handlers

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::{AppState, error::AppError};

#[derive(Debug, Deserialize)]
pub struct CatalogScanRequest {
    pub root: Option<String>,
    #[serde(default)]
    pub max_file_bytes: Option<u64>,
    #[serde(default)]
    pub strict: bool,
    #[serde(default)]
    pub compact: bool,
    #[serde(default)]
    pub top_k: Option<usize>,
}

pub async fn scan(
    State(state): State<crate::AppState>,
    Json(request): Json<CatalogScanRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?;
    
    let root = request.root
        .map(PathBuf::from)
        .unwrap_or(repo.root_path.clone());
    
    let scanner = autopsy_catalog::CatalogScanner::new(&root, request.max_file_bytes.unwrap_or(1_048_576));
    let catalog = scanner.scan()
        .map_err(|e| crate::error::AppError::Analysis(e.to_string()))?;
    
    // Persist
    state.repository_manager.persist_catalog(&repo_id, &catalog).await?;
    
    Ok(axum::Json(json!({
        "entities": catalog.entities,
        "manifests": catalog.manifests,
        "dependencies": catalog.dependencies,
        "coverage": catalog.coverage,
        "digest": catalog.digest,
        "files_scanned": catalog.files_scanned,
        "unmodelled_kinds": catalog.unmodelled_kinds,
        "refusals": catalog.refusals,
        "duplicate_entities": catalog.duplicate_entities
    })))
}

#[derive(Debug, Deserialize)]
pub struct BlastRadiusRequest {
    pub reference: String,
    pub root: Option<String>,
    #[serde(default)]
    pub max_file_bytes: Option<u64>,
    #[serde(default)]
    pub strict: bool,
    #[serde(default)]
    pub compact: bool,
    #[serde(default)]
    pub top_k: Option<usize>,
}

pub async fn blast_radius(
    State(state): State<crate::AppState>,
    Json(request): Json<BlastRadiusRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?;
    
    let root = request.root
        .map(PathBuf::from)
        .unwrap_or(repo.root_path.clone());
    
    let engine = autopsy_catalog::BlastRadiusEngine::new(&root, request.max_file_bytes.unwrap_or(1_048_576));
    let blast_radius = engine.compute(&request.reference)
        .map_err(|e| crate::error::AppError::Analysis(e.to_string()))?;
    
    Ok(axum::Json(json!({
        "component": blast_radius.component,
        "found": blast_radius.found,
        "coverage": blast_radius.coverage,
        "owners": blast_radius.owners,
        "owner_is_team": blast_radius.owner_is_team,
        "system": blast_radius.system,
        "domain": blast_radius.domain,
        "co_owned_components": blast_radius.co_owned_components,
        "siblings": blast_radius.siblings,
        "provided_apis": blast_radius.provided_apis,
        "consumed_apis": blast_radius.consumed_apis,
        "dependents": blast_radius.dependents,
        "unresolved": blast_radius.unresolved,
        "notes": blast_radius.notes
    })))
}