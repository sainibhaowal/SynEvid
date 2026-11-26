//! Graph handlers

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::{AppState, error::AppError};

#[derive(Debug, Deserialize)]
pub struct UniverseQuery {
    #[serde(default)]
    pub dim: Option<String>, // "2d" or "3d"
}

pub async fn universe(
    Query(params): Query<UniverseQuery>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?;
    
    let snapshot_id = repo.latest_snap_id
        .ok_or_else(|| crate::error::AppError::Repository("No snapshot available".to_string()))?;
    
    let storage = state.repository_manager.get_storage(&v1::SnapshotId::new(snapshot_id)).await?;
    let graph = storage.get_graph(&v1::SnapshotId::new(snapshot_id)).await?;
    
    let is_3d = params.dim.as_deref() != Some("2d");
    let layout = autopsy_graph_layout::DeterministicForceLayout;
    let seed = blake3::hash(format!("{}_universe", repo_id).as_bytes()).into();
    let coords = layout.compute_layout(&graph, &seed, is_3d)
        .map_err(|e| crate::error::AppError::Analysis(e.to_string()))?;
    
    let is_3d_str = if is_3d { "3D" } else { "2D" };
    
    let nodes: Vec<Value> = coords.into_iter().map(|(id, coord)| {
        let node = graph.get_node(&id);
        json!({
            "id": id.as_str(),
            "x": coord.x,
            "y": coord.y,
            "z": coord.z,
            "cluster_id": coord.cluster_id,
            "label": node.map(|n| n.qualified_name.clone()).unwrap_or_default(),
            "kind": node.map(|n| n.kind.clone()).unwrap_or_default(),
            "file_path": node.and_then(|n| n.range.as_ref().map(|r| r.path.clone())),
            "start_line": node.and_then(|n| n.range.as_ref().map(|r| r.start_line as i64)),
            "end_line": node.and_then(|n| n.range.as_ref().map(|r| r.end_line as i64)),
            "status": node.map(|n| format!("{:?}", n.coverage_state).to_lowercase()).unwrap_or_else(|| "unknown".to_string()),
            "caller_count": node.map(|n| n.caller_count as i64).unwrap_or(0),
            "callee_count": node.map(|n| n.callee_count as i64).unwrap_or(0),
        })
    }).collect();
    
    let edges: Vec<Value> = graph.all_edges_sorted().into_iter().take(10000).map(|edge| {
        let source_node = graph.get_node(&edge.source);
        let target_node = graph.get_node(&edge.target);
        json!({
            "source": edge.source.as_str(),
            "target": edge.target.as_str(),
            "kind": format!("{:?}", edge.kind),
            "source_file": edge.location.as_ref().and_then(|l| Some(l.path.clone())),
            "source_line": edge.location.as_ref().and_then(|l| Some(l.start_line as i64)),
            "confidence": "high"
        })
    }).collect();
    
    Ok(axum::Json(json!({
        "repo_id": repo_id,
        "snapshot_id": snapshot_id,
        "dimensions": is_3d_str,
        "node_count": nodes.len(),
        "edge_count": edges.len(),
        "total_node_count": graph.node_count(),
        "total_edge_count": graph.edge_count(),
        "is_truncated": graph.edge_count() > 10000,
        "nodes": nodes,
        "edges": edges,
        "coverage_state": format!("{:?}", graph.coverage_state).to_lowercase()
    })))
}

#[derive(Debug, Deserialize)]
pub struct EgoQuery {
    pub id: String,
}

pub async fn ego(
    Query(params): Query<crate::handlers::graph::EgoQuery>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?;
    
    let snapshot_id = repo.latest_snap_id
        .ok_or_else(|| crate::error::AppError::Repository("No snapshot available".to_string()))?;
    
    let storage = state.repository_manager.get_storage(&v1::SnapshotId::new(snapshot_id)).await?;
    let symbol_id = v1::SymbolId::new(params.id);
    let ego = storage.query_ego_network(&v1::SnapshotId::new(snapshot_id), &symbol_id).await
        .map_err(|e| crate::error::AppError::Analysis(e.to_string()))?;
    
    if ego.target_node.is_none() {
        return Ok(axum::Json(serde_json::json!({
            "target_node": null,
            "incoming_callers": [],
            "outgoing_callees": [],
            "coverage_state": "unknown",
            "error_code": "ERR-GRAPH-404",
            "error": format!("No analyzed symbol named '{}' was found", params.id)
        })));
    }
    
    Ok(axum::Json(json!({
        "target_node": ego.target_node,
        "incoming_callers": ego.incoming_callers,
        "outgoing_callees": ego.outgoing_callees,
        "coverage_state": ego.coverage_state.unwrap_or_else(|| "unknown".to_string())
    })))
}

#[derive(Debug, Deserialize)]
pub struct EgoIdQuery {
    pub id: String,
}

pub async fn ego_id(
    Path(id): Path<String>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?;
    
    let snapshot_id = repo.latest_snap_id
        .ok_or_else(|| crate::error::AppError::Repository("No snapshot available".to_string()))?;
    
    let storage = state.repository_manager.get_storage(&v1::SnapshotId::new(snapshot_id)).await?;
    let symbol_id = v1::SymbolId::new(id);
    let ego = storage.query_ego_network(&v1::SnapshotId::new(snapshot_id), &symbol_id).await
        .map_err(|e| crate::error::AppError::Analysis(e.to_string()))?;
    
    if ego.target_node.is_none() {
        return Ok(axum::Json(serde_json::json!({
            "target_node": null,
            "incoming_callers": [],
            "outgoing_callees": [],
            "coverage_state": "unknown",
            "error_code": "ERR-GRAPH-404",
            "error": format!("No analyzed symbol named '{}' was found", id)
        })));
    }
    
    Ok(axum::Json(json!({
        "target_node": ego.target_node,
        "incoming_callers": ego.incoming_callers,
        "outgoing_callees": ego.outgoing_callees,
        "coverage_state": ego.coverage_state.unwrap_or_else(|| "unknown".to_string())
    })))
}