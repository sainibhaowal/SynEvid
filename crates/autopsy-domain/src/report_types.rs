//! Extended report types for Phase 6 - Performance, Errors, and Git Risk fixtures

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Performance and resource usage metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceResources {
    pub status: String,
    pub evidence_basis: String,
    pub total_cpu_ms: u64,
    pub peak_memory_mb: u64,
    pub total_disk_bytes: u64,
    pub index_time_ms: u64,
    pub graph_build_ms: u64,
    pub analysis_ms: u64,
    pub export_ms: u64,
    pub by_component: BTreeMap<String, ComponentMetrics>,
}

/// Per-component performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentMetrics {
    pub cpu_ms: u64,
    pub memory_mb: u64,
    pub disk_bytes: u64,
}

/// Error and recovery summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorsRecovery {
    pub status: String,
    pub evidence_basis: String,
    pub total_errors: u64,
    pub errors_by_type: BTreeMap<String, u64>,
    pub recovery_actions: Vec<RecoveryAction>,
    pub unresolved_errors: Vec<UnresolvedError>,
}

/// Recovery action taken
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAction {
    pub error_type: String,
    pub action: String,
    pub timestamp: u64,
    pub success: bool,
}

/// Unresolved error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnresolvedError {
    pub error_type: String,
    pub message: String,
    pub location: Option<SourceLocation>,
    pub first_seen: u64,
    pub count: u64,
}

/// Source location
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceLocation {
    pub file_path: PathBuf,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: Option<u32>,
    pub end_column: Option<u32>,
}

/// Test fixture: Static graph with known entrypoints and dynamic constructs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticGraphFixture {
    pub name: String,
    pub description: String,
    pub nodes: Vec<FixtureNode>,
    pub edges: Vec<FixtureEdge>,
    pub entrypoints: Vec<String>,
    pub dynamic_constructs: Vec<DynamicConstruct>,
}

/// Fixture graph node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureNode {
    pub stable_id: String,
    pub qualified_name: String,
    pub file_path: String,
    pub node_kind: String,
    pub visibility: String,
    pub is_exported: bool,
    pub coverage_state: String,
    pub start_line: u32,
    pub end_line: u32,
    pub start_column: u32,
    pub end_column: u32,
}

/// Fixture graph edge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureEdge {
    pub edge_id: String,
    pub source_stable_id: String,
    pub target_stable_id: String,
    pub edge_kind: String,
    pub confidence: String,
}

/// Dynamic construct in fixture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicConstruct {
    pub construct_type: String, // "reflection", "dynamic_import", "plugin_registration", "generated"
    pub target_symbol: String,
    pub description: String,
}

/// Test fixture: Telemetry windows (complete, incomplete, insufficient)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryFixture {
    pub name: String,
    pub description: String,
    pub windows: Vec<FixtureWindow>,
}

/// Telemetry observation window fixture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureWindow {
    pub window_id: String,
    pub snapshot_id: String,
    pub start_time: u64,
    pub end_time: u64,
    pub required_windows: u32,
    pub observed_windows: u32,
    pub complete_observation: bool,
    pub coverage_state: String,
    pub observed_symbols: Vec<String>,
    pub unobserved_symbols: Vec<String>,
}

/// Test fixture: Git risk scenarios (low file count + high semantic impact, vice versa)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitRiskFixture {
    pub name: String,
    pub description: String,
    pub commits: Vec<FixtureCommit>,
    pub expected_high_impact_low_files: bool,
    pub expected_low_impact_high_files: bool,
}

/// Fixture commit with semantic diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureCommit {
    pub oid: String,
    pub short: String,
    pub summary: String,
    pub author: String,
    pub timestamp: u64,
    pub parents: Vec<String>,
    pub file_changes: Vec<FixtureFileChange>,
    pub semantic_diff: Option<FixtureSemanticDiff>,
}

/// File change in fixture commit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureFileChange {
    pub file_path: String,
    pub change_type: String, // "added", "modified", "deleted", "renamed"
    pub lines_added: usize,
    pub lines_removed: usize,
}

/// Semantic diff in fixture commit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureSemanticDiff {
    pub direct_symbols: Vec<String>,
    pub impacted_boundary: Vec<String>,
    pub symbol_deltas: Vec<FixtureSymbolDelta>,
    pub edge_deltas: Vec<FixtureEdgeDelta>,
    pub contract_deltas: Vec<FixtureContractDelta>,
    pub drifted_contracts: Vec<FixtureDriftedContract>,
}

/// Symbol delta in semantic diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureSymbolDelta {
    pub stable_id: String,
    pub qualified_name: String,
    pub delta_type: String, // "added", "modified", "deleted", "signature_changed"
}

/// Edge delta in semantic diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureEdgeDelta {
    pub source: String,
    pub target: String,
    pub edge_kind: String,
    pub delta_type: String,
}

/// Contract delta in semantic diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureContractDelta {
    pub contract_id: String,
    pub delta_type: String,
}

/// Drifted contract in semantic diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureDriftedContract {
    pub contract_id: String,
    pub drift_type: String,
    pub severity: String,
}