//! Verify handlers

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::{AppState, error::AppError};

/// Verify query parameters
#[derive(Debug, Deserialize)]
pub struct VerifyQuery {
    #[serde(default)]
    pub strict: bool,
    #[serde(default)]
    pub invariants_file: Option<String>,
    #[serde(default)]
    pub baseline_id: Option<String>,
    #[serde(default)]
    pub snapshot_id: Option<String>,
}

pub async fn get_v1(
    Query(params): Query<VerifyQuery>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?;
    
    let snapshot_id = params.snapshot_id
        .or_else(|| repo.latest_snap_id.clone())
        .ok_or_else(|| crate::error::AppError::Repository("No snapshot available".to_string()))?;
    
    let storage = state.repository_manager.get_storage(&v1::SnapshotId::new(snapshot_id)).await?;
    let graph = storage.get_graph(&v1::SnapshotId::new(snapshot_id)).await?;
    
    // Load invariants config
    let invariants_path = params.invariants_file
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".autopsy/invariants.yml"));
    
    let invariants_config = if invariants_path.exists() {
        autopsy_invariants::InvariantsConfig::load_from_file(&invariants_path)
            .map_err(|e| crate::error::AppError::Config(e.to_string()))?
    } else {
        autopsy_invariants::InvariantsConfig {
            version: "0.0.1".to_string(),
            invariants: vec![],
        }
    };
    
    let eval_ctx = autopsy_invariants::EvaluationContext {
        snapshot_id: v1::SnapshotId::new(snapshot_id.clone()),
        analyzer_version: env!("CARGO_PKG_VERSION"),
        graph: &graph,
        baseline_graph: None,
        diff: None,
        contracts_before: &std::collections::BTreeMap::new(),
        contracts_after: &std::collections::BTreeMap::new(),
        coverage_state: graph.coverage_state,
    };
    
    let evaluator = autopsy_invariants::InvariantEvaluator::new();
    let pairs = evaluator.evaluate(&invariants_config, &eval_ctx)
        .map_err(|e| crate::error::AppError::Analysis(e.to_string()))?;
    
    let mut findings = Vec::new();
    let mut fail_count = 0;
    let mut unknown_count = 0;
    
    for (f, _) in pairs {
        let status = match f.status {
            autopsy_domain::FindingStatus::Pass => "pass",
            autopsy_domain::FindingStatus::Fail => { fail_count += 1; "fail" },
            autopsy_domain::FindingStatus::Unknown => { unknown_count += 1; "unknown" },
        };
        
        findings.push(json!({
            "rule_id": f.rule_id,
            "status": status,
            "severity": format!("{:?}", f.severity).to_lowercase(),
            "message": f.message,
            "entities": f.entities
        }));
    }
    
    let verdict = if fail_count > 0 { "FAIL" } else if unknown_count > 0 { "UNKNOWN" } else { "PASS" };
    
    Ok(axum::Json(json!({
        "snapshot_id": snapshot_id,
        "findings": findings,
        "verdict": verdict,
        "coverage_state": format!("{:?}", graph.coverage_state).to_lowercase()
    })))
}

pub async fn post(
    State(state): State<crate::AppState>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<Value>, crate::error::AppError> {
    // Handle verify with custom invariants file
    let invariants_file = request.get("invariants_file")
        .and_then(|v| v.as_str())
        .map(PathBuf::from);
    
    let strict = request.get("strict").and_then(|v| v.as_bool()).unwrap_or(false);
    let baseline_id = request.get("baseline_id").and_then(|v| v.as_str()).map(String::from);
    
    // Similar to get_v1 but with custom invariants
    get_v1(axum::extract::Query(VerifyQuery {
        strict,
        invariants_file: invariants_file.map(|p| p.to_string_lossy().to_string()),
        baseline_id,
        snapshot_id: None,
    }), State(state)).await
}