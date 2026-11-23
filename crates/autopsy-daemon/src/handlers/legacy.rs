//! Legacy compatibility endpoints

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use crate::{AppState, error::AppError};

/// Legacy status endpoint
pub async fn status(
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, (axum::http::StatusCode, axum::Json<Value>)> {
    let active_repo_id = state.repository_manager.get_active_repo_id().await;
    let snapshot_id = if let Some(repo_id) = &active_repo_id {
        state.repository_manager.get_repository(repo_id)
            .await
            .and_then(|r| r.latest_snap_id.clone())
    } else {
        None
    };
    
    let event_seq = state.event_store.latest_seq();
    
    Ok(Json(json!({
        "status": "online",
        "active_repo_id": active_repo_id,
        "snapshot_id": snapshot_id,
        "last_event_seq": event_seq,
        "storage_health": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    })))
}

/// Legacy verify endpoint
pub async fn verify(
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, (axum::http::StatusCode, axum::Json<Value>)> {
    verify::get_v1(State(state)).await
}

/// Legacy impact endpoint
pub async fn impact(
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, (axum::http::StatusCode, axum::Json<Value>)> {
    let seed = params.get("seed").cloned().unwrap_or_default();
    if seed.is_empty() {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({
                "error": "seed parameter required",
                "error_code": "ERR-IMPACT-400"
            }))
        ));
    }
    
    impact::get(axum::extract::Query(impact::ImpactQuery { seed }), crate::extractors::AppStateExtractor(state)).await
}

/// Legacy trajectory endpoint
pub async fn trajectory(
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, (axum::http::StatusCode, axum::Json<Value>)> {
    let runs = state.repository_manager.get_trajectory_runs().await?;
    
    Ok(axum::Json(json!({
        "runs": runs
    })))
}

/// Legacy contracts endpoint
pub async fn contracts(
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, (axum::http::StatusCode, axum::Json<Value>)> {
    Ok(axum::Json(serde_json::json!({
        "contracts": [],
        "breaking_changes": [],
        "coverage_state": "unknown"
    })))
}

/// Legacy graph endpoint
pub async fn graph(
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, (axum::http::StatusCode, axum::Json<Value>)> {
    let active_repo_id = state.repository_manager.get_active_repo_id().await;
    
    // Early return if no active repository
    let Some(repo_id) = active_repo_id else {
        return Ok(axum::Json(json!({
            "nodes": [],
            "edges": [],
            "coverage_state": "Unknown",
            "error_code": "ERR-GRAPH-501",
            "error": "No active repository"
        })));
    }
    
    let Some(repo) = state.repository_manager.get_repository(&repo_id).await else {
        return Ok(axum::Json(json!({
            "nodes": [],
            "edges": [],
            "coverage_state": "Unknown",
            "error_code": "ERR-GRAPH-501",
            "error": "Repository not found"
        })));
    }
    
    let Some(snapshot_id) = repo.latest_snap_id else {
        return Ok(axum::Json(json!({
            "nodes": [],
            "edges": [],
            "coverage_state": "Unknown",
            "error_code": "ERR-GRAPH-501",
            "error": "No snapshot available"
        })));
    }
    
    let storage = match state.repository_manager.get_storage(&v1::SnapshotId::new(snapshot_id)).await {
        Ok(s) => s,
        Err(e) => {
            return Ok(axum::Json(json!({
                "nodes": [],
                "edges": [],
                "coverage_state": "Unknown",
                "error_code": "ERR-GRAPH-501",
                "error": format!("Failed to open storage: {}", e)
            })));
        }
    };
    
    let graph = match storage.get_graph(&v1::SnapshotId::new(snapshot_id)).await {
        Ok(g) => g,
        Err(e) => {
            return Ok(axum::Json(json!({
                "nodes": [],
                "edges": [],
                "coverage_state": "Unknown",
                "error_code": "ERR-GRAPH-501",
                "error": format!("Failed to get graph: {}", e)
            })));
        }
    };
    
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    
    for node in graph.all_symbols_sorted().into_iter().take(100) {
        nodes.push(json!({
            "id": node.stable_id.as_str(),
            "label": node.qualified_name.clone(),
            "kind": node.kind.clone(),
            "full_name": node.stable_id.as_str()
        }));
    }
    
    for edge in graph.all_edges_sorted().into_iter().take(200) {
        edges.push(json!({
            "source": edge.source.as_str(),
            "target": edge.target.as_str(),
            "kind": format!("{:?}", edge.kind)
        }));
    }
    
    Ok(axum::Json(json!({
        "nodes": nodes,
        "edges": edges,
        "total_nodes": graph.node_count(),
        "total_edges": graph.edge_count(),
        "coverage_state": format!("{:?}", graph.coverage_state)
    })))
}