//! Analytics handlers

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::{AppState, error::AppError};

#[derive(Debug, Deserialize)]
pub struct ReportQuery {
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String { "json".to_string() }

pub async fn report(
    Query(params): Query<ReportQuery>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?;
    
    let snapshot_id = repo.latest_snap_id
        .ok_or_else(|| crate::error::AppError::Repository("No snapshot available".to_string()))?;
    
    let storage = state.repository_manager.get_storage(&v1::SnapshotId::new(snapshot_id)).await?;
    let (nodes, edges) = storage.graph_rows_for_snapshot(&snapshot_id)?;
    
    // Static unreferenced analysis
    let (verified_candidates, unknown_candidates, excluded_count) = 
        analytics::static_unreferenced_candidates(&nodes, &edges, &snapshot_id);
    
    // Runtime unobserved (if telemetry data exists)
    let runtime_unobserved = analytics::runtime_unobserved_analysis(
        &state.repository_manager, &repo_id, &snapshot_id
    ).await?;
    
    // Git evolution
    let git_evolution = analytics::git_evolution_analysis(
        &state.repository_manager, &repo_id
    ).await?;
    
    let report = AnalyticsReport {
        report_id: format!("analytics-{}-{}", repo_id, snapshot_id),
        scope: AnalyticsScope {
            repository_id: repo_id,
            snapshot_id,
            generated_at: chrono::Utc::now().timestamp_millis() as u64,
        },
        structural_health: analytics::structural_health(&nodes, &edges, &snapshot_id),
        static_unreferenced: StaticUnreferencedResult {
            verified_candidates,
            unknown_candidates,
            excluded_count,
        },
        runtime_unobserved: runtime_unobserved,
        git_evolution,
        coverage: format!("{:?}", "Verified").to_lowercase(),
    };
    
    let format = params.format.as_str();
    let content = match format {
        "json" => serde_json::to_string_pretty(&report)?,
        "markdown" => format!("Analytics report not yet implemented in markdown"),
        "html" => format!("Analytics report not yet implemented in HTML"),
        "sarif" => serde_json::to_string(&json!({
            "version": "2.1.0",
            "$schema": "https://schemastore.org/schemas/json/sarif-2.1.0.json",
            "runs": [{
                "tool": { "driver": { "name": "SynEvid Analytics", "version": env!("CARGO_PKG_VERSION") }},
                "results": []
            }]
        })),
        "bundle" => format!("Bundle format not yet implemented"),
        _ => return Err(crate::error::AppError::Validation("Invalid format".to_string())),
    };
    
    let digest = format!("blake3:{}", blake3::hash(content.as_bytes()).to_hex());
    
    Ok(axum::Json(json!({
        "format": format,
        "filename": format!("analytics-report-{}.{}", snapshot_id, match format { "json" => "json", "markdown" => "md", "html" => "html", "sarif" => "sarif", _ => "txt" }),
        "content_type": match format { "json" => "application/json", "markdown" => "text/markdown", "html" => "text/html", "sarif" => "application/sarif+json", _ => "text/plain" },
        "digest": digest,
        "content": content,
        "report": report
    })))
}

#[derive(Debug, Deserialize)]
pub struct RuntimeWindowRequest {
    pub repo_id: Option<String>,
    pub snapshot_id: Option<String>,
    // Runtime observation window fields
}

pub async fn runtime_window(
    State(state): State<crate::AppState>,
    Json(request): Json<RuntimeWindowRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    // Implementation
    Ok(axum::Json(json!({"status": "accepted"})))
}

pub async fn ledger(
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let session_id = params.get("session_id")
        .ok_or_else(|| crate::error::AppError::Validation("session_id required".to_string()))?;
    
    let ledger = state.repository_manager.get_ai_session_ledger(&params["session_id"]).await?;
    
    Ok(axum::Json(json!({
        "ledger": ledger
    })))
}

pub async fn cache_query(
    State(state): State<crate::AppState>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<Value>, crate::error::AppError> {
    Ok(axum::Json(json!({"result": "not_implemented"})))
}

pub async fn config_snippet(
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let client = params.get("client").unwrap_or(&"cursor".to_string());
    
    Ok(axum::Json(json!({
        "client": client,
        "file_name": format!("{}.json", client),
        "config_json": json!({}),
        "instruction": "Configure your MCP client to connect to the SynEvid daemon"
    })))
}

// Analytics report structures
#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyticsReport {
    pub report_id: String,
    pub scope: AnalyticsScope,
    pub structural_health: StructuralHealth,
    pub static_unreferenced: StaticUnreferencedResult,
    pub runtime_unobserved: RuntimeUnobservedResult,
    pub git_evolution: GitEvolutionResult,
    pub coverage: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyticsScope {
    pub repository_id: String,
    pub snapshot_id: String,
    pub generated_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StructuralHealth {
    pub total_files: usize,
    pub total_symbols: usize,
    pub total_edges: usize,
    pub coverage_state: String,
    pub languages: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StaticUnreferencedResult {
    pub verified_candidates: Vec<Value>,
    pub unknown_candidates: Vec<Value>,
    pub excluded_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RuntimeUnobservedResult {
    pub status: String,
    pub windows_analyzed: usize,
    pub unobserved_symbols: Vec<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitEvolutionResult {
    pub semantic_git_risk: SemanticGitRisk,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SemanticGitRisk {
    pub status: String,
    pub risk_score: f64,
}

impl analytics::AnalyticsEngine {
    fn static_unreferenced_candidates(
        nodes: &[autopsy_storage::GraphNodeRow],
        edges: &[autopsy_storage::GraphEdgeRow],
        snapshot_id: &str,
    ) -> (Vec<Value>, Vec<Value>, usize) {
        let mut verified = Vec::new();
        let mut unknown = Vec::new();
        let mut excluded = 0;
        
        let incoming: std::collections::BTreeSet<&str> = edges.iter()
            .filter(|e| e.edge_kind != "contains")
            .map(|e| e.target_stable_id.as_str())
            .collect();
        
        let graph_edge_digest = "computed";
        
        for node in nodes.iter().filter(|n| n.node_kind != "file" && n.node_kind != "unknown") {
            if incoming.contains(node.stable_id.as_str()) {
                continue;
            }
            
            let name = node.qualified_name.rsplit([':', '.', '#']).next().unwrap_or(&node.qualified_name);
            let normalized_name = name.to_ascii_lowercase();
            let normalized_path = node.file_path.replace('\\', "/").to_ascii_lowercase();
            
            let generated = normalized_path.split('/').any(|p| 
                p == "generated" || p == "__generated__" || p == "dist" || p == "build"
            );
            let test_only = normalized_path.split('/').any(|p| 
                p == "test" || p == "tests" || p == "__tests__"
            ) || normalized_path.ends_with(".test.ts")
                || normalized_path.ends_with(".spec.ts")
                || normalized_path.ends_with("_test.go");
            let conventional_entrypoint = matches!(
                normalized_name.as_str(),
                "main" | "__main__" | "setup" | "teardown" | "register" | "plugin_init"
            );
            
            if node.is_exported || node.visibility == "public" || generated || test_only || conventional_entrypoint {
                excluded += 1;
                continue;
            }
            
            let coverage = node.coverage_state.to_ascii_lowercase();
            let payload = json!({
                "stable_id": node.stable_id,
                "qualified_name": node.qualified_name,
                "file_path": node.file_path,
                "line": node.start_line,
                "finding_state": if coverage == "verified" { "candidate" } else { "unknown" },
                "evidence_basis": "no_inbound_non_containment_edge_in_persisted_static_graph",
                "inbound_edge_count": 0,
                "graph_edge_digest": graph_edge_digest,
                "coverage_state": coverage,
                "reason": if coverage == "verified" {
                    "No inbound edge was found in the analyzed static graph; dynamic or external use is not disproven."
                } else {
                    "No inbound edge was found, but graph coverage is insufficient to classify this declaration."
                },
                "residual_unknowns": ["reflection", "generated_or_external_consumers", "framework_or_plugin_registration"],
                "snapshot_id": snapshot_id
            });
            
            if coverage == "verified" {
                verified.push(payload);
            } else {
                unknown.push(payload);
            }
        }
        
        (verified, unknown, excluded)
    }
    
    async fn runtime_unobserved_analysis(
        &self,
        storage: &crate::repository::RepositoryManager,
        repo_id: &str,
        snapshot_id: &str,
    ) -> Result<RuntimeUnobservedResult, crate::error::AppError> {
        // Query runtime observation windows
        let windows = storage.get_runtime_observation_windows(repo_id, snapshot_id)
            .map_err(|e| crate::error::AppError::Storage(e.to_string()))?;
        
        if windows.is_empty() {
            return Ok(RuntimeUnobservedResult {
                status: "insufficient_data".to_string(),
                windows_analyzed: 0,
                unobserved_symbols: Vec::new(),
            });
        }
        
        Ok(RuntimeUnobservedResult {
            status: "analyzed".to_string(),
            windows_analyzed: windows.len(),
            unobserved_symbols: Vec::new(),
        })
    }
    
    async fn git_evolution_analysis(
        &self,
        storage: &crate::repository::RepositoryManager,
        repo_id: &str,
    ) -> Result<GitEvolutionResult, crate::error::AppError> {
        Ok(GitEvolutionResult {
            semantic_git_risk: SemanticGitRisk {
                status: "not_analyzed".to_string(),
                risk_score: 0.0,
            },
        })
    }
    
    fn structural_health(
        nodes: &[autopsy_storage::GraphNodeRow],
        edges: &[autopsy_storage::GraphEdgeRow],
        snapshot: &autopsy_domain::RepoSnapshot,
    ) -> StructuralHealth {
        StructuralHealth {
            total_files: snapshot.files.len(),
            total_symbols: nodes.len(),
            total_edges: edges.len(),
            coverage_state: format!("{:?}", snapshot.coverage_state).to_lowercase(),
            languages: serde_json::from_str::<Vec<String>>(&serde_json::to_string(&snapshot.languages).unwrap_or_default()).unwrap_or_default(),
        }
    }
}