//! Impact analysis handlers

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::{AppState, error::AppError};

/// Impact query parameters
#[derive(Debug, Deserialize)]
pub struct ImpactQuery {
    pub seed: String,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub max_depth: Option<usize>,
    #[serde(default)]
    pub budget: Option<usize>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub edge_kinds: Option<String>,
    #[serde(default)]
    pub top_k: Option<usize>,
    #[serde(default)]
    pub detail: Option<String>,
}

/// Legacy impact query
#[derive(Debug, Deserialize)]
pub struct LegacyImpactQuery {
    pub seed: String,
    pub direction: Option<String>,
    pub max_depth: Option<usize>,
    pub budget: Option<usize>,
    pub profile: Option<String>,
    pub edge_kinds: Option<String>,
    pub top_k: Option<usize>,
    pub detail: Option<String>,
}

/// Get impact analysis (legacy endpoint)
pub async fn get(
    Query(params): Query<LegacyImpactQuery>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let seed = params.seed;
    if seed.is_empty() {
        return Err(crate::error::AppError::Validation("seed parameter required".to_string()));
    }
    
    compute_impact(
        seed,
        params.direction,
        params.max_depth,
        params.budget,
        params.profile,
        params.edge_kinds,
        params.top_k,
        params.detail,
        state,
    ).await
}

/// V1 impact analysis endpoint
pub async fn get_v1(
    Query(params): Query<ImpactQuery>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    if params.seed.is_empty() {
        return Err(crate::error::AppError::Validation("seed parameter required".to_string()));
    }
    
    compute_impact(
        params.seed,
        params.direction,
        params.max_depth,
        params.budget,
        params.profile,
        params.edge_kinds,
        params.top_k,
        params.detail,
        state,
    ).await
}

/// Core impact computation
async fn compute_impact(
    seed: String,
    direction: Option<String>,
    max_depth: Option<usize>,
    budget: Option<usize>,
    profile: Option<String>,
    edge_kinds: Option<String>,
    top_k: Option<usize>,
    detail: Option<String>,
    state: crate::AppState,
) -> Result<Json<Value>, crate::error::AppError> {
    // Get active repository
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?;
    
    let snapshot_id = repo.latest_snap_id
        .ok_or_else(|| crate::error::AppError::Repository("No snapshot available".to_string()))?;
    
    // Get storage
    let storage = state.repository_manager.get_storage(&v1::SnapshotId::new(snapshot_id)).await?;
    
    // Get graph
    let graph = storage.get_graph(&v1::SnapshotId::new(snapshot_id)).await?;
    
    // Parse parameters
    let direction = match params.direction.as_deref() {
        Some("backward") => autopsy_impact::ImpactDirection::Backward,
        Some("bidirectional") => autopsy_impact::ImpactDirection::Bidirectional,
        _ => autopsy_impact::ImpactDirection::Forward,
    };
    
    let profile = match params.profile.as_deref() {
        Some("callers") => autopsy_impact::callers_profile(64, 10_000),
        _ => autopsy_impact::ImpactProfile::default(),
    };
    
    let max_depth = params.max_depth.unwrap_or(10);
    let budget = params.budget.unwrap_or(1000);
    
    // Build edge kinds filter
    let edge_kinds = if let Some(kinds) = params.edge_kinds {
        kinds.split(',')
            .filter_map(|k| match k.trim() {
                "contains" => Some(autopsy_domain::EdgeKind::Contains),
                "imports" => Some(autopsy_domain::EdgeKind::Imports),
                "references" => Some(autopsy_domain::EdgeKind::References),
                "calls" => Some(autopsy_domain::EdgeKind::Calls),
                "implements" => Some(autopsy_domain::EdgeKind::Implements),
                "inherits" => Some(autopsy_domain::EdgeKind::Inherits),
                "reads-schema" => Some(autopsy_domain::EdgeKind::ReadsSchema),
                "writes-schema" => Some(autopsy_domain::EdgeKind::WritesSchema),
                "test-covers" => Some(autopsy_domain::EdgeKind::TestCovers),
                _ => None,
            })
            .collect()
    } else {
        std::collections::BTreeSet::new()
    };
    
    let query = autopsy_impact::ImpactQuery {
        seed_symbols: vec![autopsy_domain::SymbolId::new(&seed)],
        direction,
        profile: autopsy_impact::ImpactProfile {
            name: "api".to_string(),
            max_depth,
            budget,
            edge_kinds,
            traverse_file_nodes: true,
        },
    };
    
    let engine = autopsy_impact::ImpactEngine::new();
    let report = engine.compute_impact(&graph, &query)
        .map_err(|e| crate::error::AppError::Analysis(e.to_string()))?;
    
    let detail = params.detail.as_deref().unwrap_or("full");
    
    if detail == "summary" {
        Ok(axum::Json(json!({
            "seed": seed,
            "likely_affected": report.likely_affected.iter().take(params.top_k.unwrap_or(5)).map(|s| s.as_str()).collect::<Vec<_>>(),
            "possibly_affected": report.possibly_affected.iter().take(params.top_k.unwrap_or(5)).map(|s| s.as_str()).collect::<Vec<_>>(),
            "total_impacted": report.impacted_entities.len(),
            "truncated": report.truncated,
            "impact_digest": report.impact_digest,
            "coverage_state": "verified"
        })))
    } else {
        Ok(axum::Json(json!({
            "seed": seed,
            "likely_affected": report.likely_affected.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            "possibly_affected": report.possibly_affected.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            "paths": report.paths.iter().map(|p| p.iter().map(|s| s.as_str()).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "truncated": report.truncated,
            "impact_digest": report.impact_digest,
            "query": {
                "direction": format!("{:?}", query.direction),
                "profile": query.profile,
                "budget": query.profile.budget
            },
            "confidence_tiers": {
                "likely_count": report.likely_affected.len(),
                "possible_count": report.possibly_affected.len()
            },
            "coverage_state": "verified"
        })))
    }
}