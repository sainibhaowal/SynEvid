//! Telemetry engine for OTLP ingestion and topology correlation

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, info, warn};

use autopsy_telemetry::{OtlpIngest, TopologyCorrelator, TelemetryTopologyReport};
use autopsy_storage::StorageEngine;

/// Telemetry engine
pub struct TelemetryEngine {
    // Stateless - uses storage
}

impl TelemetryEngine {
    pub fn new() -> Self {
        Self {}
    }

    /// Ingest OTLP trace export and correlate with declared surface
    pub fn ingest_and_correlate(
        &self,
        storage: &mut StorageEngine,
        repo_id: &str,
        snapshot_id: &str,
        otlp_json: &str,
        salt_hex: &str,
        surface_json: Option<&str>,
        windows: u32,
        complete_observation: bool,
        dispatch_static: bool,
    ) -> Result<TelemetryIngestResult, String> {
        // Parse OTLP
        let otlp: Value = serde_json::from_str(otlp_json)
            .map_err(|e| format!("Invalid OTLP JSON: {}", e))?;

        // Ingest
        let ingest = OtlpIngest::new(salt_hex);
        let spans = ingest.parse(otlp)
            .map_err(|e| format!("Failed to parse OTLP: {}", e))?;

        // If surface provided, correlate
        let topology_report = if let Some(surface_json) = surface_json {
            let surface: Value = serde_json::from_str(surface_json)
                .map_err(|e| format!("Invalid surface JSON: {}", e))?;
            
            let correlator = TopologyCorrelator::new(
                windows,
                complete_observation,
                dispatch_static,
            );
            Some(correlator.correlate(&spans, &surface)
                .map_err(|e| format!("Failed to correlate: {}", e))?)
        } else {
            None
        };

        // Store in storage
        // storage.insert_telemetry_spans(repo_id, snapshot_id, &spans)?;
        // if let Some(report) = &topology_report {
        //     storage.insert_topology_report(repo_id, snapshot_id, report)?;
        // }

        Ok(TelemetryIngestResult {
            spans_ingested: spans.len(),
            topology_report: topology_report.map(|r| r.into()),
        })
    }

    /// Get telemetry topology report
    pub fn get_topology_report(
        &self,
        storage: &StorageEngine,
        repo_id: &str,
        snapshot_id: &str,
    ) -> Result<Option<TelemetryTopologyReportInfo>, String> {
        // storage.get_topology_report(repo_id, snapshot_id)
        Ok(None)
    }
}

// Result types

#[derive(Debug, Serialize, Deserialize)]
pub struct TelemetryIngestResult {
    pub spans_ingested: usize,
    pub topology_report: Option<TelemetryTopologyReportInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TelemetryTopologyReportInfo {
    pub coverage: String,
    pub actionable: bool,
    pub observation: ObservationInfo,
    pub report: Value,
    pub notes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ObservationInfo {
    pub windows_observed: u32,
    pub required_windows: u32,
    pub span_count: usize,
    pub complete_observation: bool,
    pub dispatch: String,
}