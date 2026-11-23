//! Telemetry handlers

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::{AppState, error::AppError};

#[derive(Debug, Deserialize)]
pub struct TelemetryIngestRequest {
    pub traces: String, // OTLP/JSON trace export
    pub salt: String, // 64 hex chars
    pub surface: Option<String>,
    #[serde(default = "default_windows")]
    pub windows: u32,
    #[serde(default)]
    pub complete_observation: bool,
    #[serde(default)]
    pub dispatch_static: bool,
    #[serde(default)]
    pub strict: bool,
    #[serde(default)]
    pub compact: bool,
    #[serde(default)]
    pub top_k: Option<usize>,
}

fn default_windows() -> u32 { 3 }

pub async fn ingest(
    State(state): State<crate::AppState>,
    Json(request): Json<TelemetryIngestRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let repo_id = state.repository_manager.get_active_repo_id().await
        .ok_or_else(|| crate::error::AppError::Repository("No active repository".to_string()))?;
    
    let repo = state.repository_manager.get_repository(&repo_id).await?
        .ok_or_else(|| crate::error::AppError::Repository("Repository not found".to_string()))?;
    
    let snapshot_id = repo.latest_snap_id
        .ok_or_else(|| crate::error::AppError::Repository("No snapshot available".to_string()))?;
    
    let storage = state.repository_manager.get_storage(&v1::SnapshotId::new(snapshot_id)).await?;
    
    // Parse OTLP
    let otlp: Value = serde_json::from_str(&request.traces)
        .map_err(|e| crate::error::AppError::Validation(format!("Invalid OTLP JSON: {}", e)))?;
    
    // Ingest
    let ingest = autopsy_telemetry::OtlpIngest::new(&request.salt);
    let spans = ingest.parse(otlp)
        .map_err(|e| crate::error::AppError::Analysis(e.to_string()))?;
    
    // Correlate if surface provided
    let topology_report = if let Some(surface_json) = request.surface {
        let surface: Value = serde_json::from_str(&surface_json)
            .map_err(|e| crate::error::AppError::Validation(format!("Invalid surface JSON: {}", e)))?;
        
        let correlator = autopsy_telemetry::TopologyCorrelator::new(
            request.windows,
            request.complete_observation,
            request.dispatch_static,
        );
        Some(correlator.correlate(&spans, &surface)
            .map_err(|e| crate::error::AppError::Analysis(e.to_string()))?)
    } else {
        None
    };
    
    // Store in storage
    // storage.insert_telemetry_spans(&repo_id, &snapshot_id, &spans)?;
    // if let Some(report) = &topology_report {
    //     storage.insert_topology_report(&repo_id, &snapshot_id, report)?;
    // }
    
    Ok(axum::Json(json!({
        "spans_ingested": spans.len(),
        "topology_report": topology_report
    })))
}

pub async fn topology(
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    Ok(axum::Json(json!({"topology": "not_implemented"})))
}