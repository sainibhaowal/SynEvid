//! Core finding model for evidence-backed analytics
//!
//! This module defines the core finding model for evidence-backed analytics,
//! replacing boolean dead-code fields with rich finding states that include
//! evidence basis, coverage, reasoning, and residual unknowns.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::report_types::{PerformanceResources, ErrorsRecovery};

/// Canonical finding state for evidence-backed analytics
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingState {
    /// Evidence was complete for the stated scope; finding is verified
    Verified,
    /// Evidence was complete; no violation found within verified scope
    Pass,
    /// A verified violation exists within the analyzed scope
    Fail,
    /// A possible issue exists but evidence is not conclusive
    Warn,
    /// Some relevant input or behavior was not analyzable
    Partial,
    /// The engine cannot prove the claim (insufficient evidence)
    Unknown,
    /// The operation could not be completed
    Error,
    /// The required service or artifact was not reachable
    Unavailable,
}

impl FindingState {
    /// Returns true if this state represents a successful outcome (Pass)
    pub fn is_success(&self) -> bool {
        matches!(self, FindingState::Pass)
    }

    /// Returns true if this state is a non-success state
    pub fn is_non_success(&self) -> bool {
        !self.is_success()
    }

    /// Returns true if this state indicates a finding that requires attention
    pub fn is_finding(&self) -> bool {
        matches!(
            self,
            FindingState::Fail | FindingState::Warn | FindingState::Partial | FindingState::Unknown
        )
    }

    /// Returns the canonical string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            FindingState::Verified => "verified",
            FindingState::Pass => "pass",
            FindingState::Fail => "fail",
            FindingState::Warn => "warn",
            FindingState::Partial => "partial",
            FindingState::Unknown => "unknown",
            FindingState::Error => "error",
            FindingState::Unavailable => "unavailable",
        }
    }
}

/// Coverage state for a specific finding
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingCoverageState {
    /// All relevant inputs/behaviors were analyzed
    Verified,
    /// Some relevant inputs/behaviors were not analyzable
    Partial,
    /// Coverage could not be determined
    Unknown,
    /// Coverage determination failed
    Error,
}

impl FindingCoverageState {
    /// Returns true if coverage allows a Pass verdict
    pub fn allows_pass(&self) -> bool {
        matches!(self, FindingCoverageState::Verified)
    }

    /// Returns true if coverage is insufficient for definitive verdict
    pub fn is_insufficient(&self) -> bool {
        matches!(
            self,
            FindingCoverageState::Partial
                | FindingCoverageState::Unknown
                | FindingCoverageState::Error
        )
    }
}

/// Evidence basis for a finding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingEvidence {
    /// The type of evidence this finding is based on
    pub evidence_basis: String,
    /// Coverage state for this specific finding
    pub coverage_state: FindingCoverageState,
    /// Human-readable reason for the finding state
    pub reason: String,
    /// Residual unknowns that prevent a definitive verdict
    pub residual_unknowns: Vec<String>,
    /// Source locations (file, line, column) relevant to this finding
    pub source_locations: Vec<SourceLocation>,
    /// Related symbols/edges in the graph
    pub related_symbols: Vec<String>,
    /// The analysis method that produced this finding
    pub analysis_method: String,
    /// Confidence level (0.0 - 1.0) - optional
    pub confidence: Option<f64>,
}

/// Source location in source code
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceLocation {
    pub file_path: PathBuf,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: Option<u32>,
    pub end_column: Option<u32>,
}

/// False-positive control categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FalsePositiveCategory {
    /// Entrypoints (main, test, generated, etc.)
    Entrypoint,
    /// Exported/public API
    Exported,
    /// Reflection/dynamic dispatch
    Reflection,
    /// Generated code
    Generated,
    /// Test-only code
    TestOnly,
    /// Plugin/contract registration
    PluginContract,
    /// Reflection/dynamic imports
    DynamicImport,
    /// Runtime code generation
    RuntimeGeneration,
}

/// False-positive control for a specific symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FalsePositiveControl {
    pub category: FalsePositiveCategory,
    pub description: String,
    pub applies_to: Vec<String>, // Symbol IDs this control applies to
}

/// Finding with full evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// Unique identifier for this finding
    pub finding_id: String,
    /// The rule/rule-set this finding belongs to
    pub rule_id: String,
    /// The finding state
    pub state: FindingState,
    /// Severity level
    pub severity: Severity,
    /// Human-readable message
    pub message: String,
    /// Entities (symbols) involved
    pub entities: Vec<String>,
    /// Source locations
    pub locations: Vec<SourceLocation>,
    /// Full evidence for this finding
    pub evidence: FindingEvidence,
    /// False-positive controls that apply
    pub false_positive_controls: Vec<FalsePositiveControl>,
    /// Related findings
    pub related_findings: Vec<String>,
    /// Timestamp when finding was created
    pub created_at: u64,
}

/// Severity level for a finding
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// Coverage state for the overall report
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoverageState {
    Verified,
    Partial,
    Unknown,
    Unavailable,
    Error,
}

/// Static unreferenced candidate with evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticUnreferencedCandidate {
    pub stable_id: String,
    pub qualified_name: String,
    pub file_path: String,
    pub line: u32,
    pub finding_state: FindingState,
    pub evidence: FindingEvidence,
    pub false_positive_controls: Vec<FalsePositiveControl>,
}

/// Runtime-unobserved candidate with evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeUnobservedCandidate {
    pub stable_id: String,
    pub qualified_name: String,
    pub file_path: String,
    pub line: u32,
    pub finding_state: FindingState,
    pub evidence: FindingEvidence,
    pub windows_analyzed: u32,
    pub windows_required: u32,
}

/// Git evolution risk finding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitRiskFinding {
    pub commit_oid: String,
    pub commit_short: String,
    pub file_path: String,
    pub finding_state: FindingState,
    pub evidence: FindingEvidence,
    pub risk_score: f64,
    pub semantic_impact: Option<SemanticImpact>,
}

/// Semantic impact analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticImpact {
    pub direct_symbols_affected: usize,
    pub transitive_symbols_affected: usize,
    pub contracts_broken: usize,
    pub api_surface_changed: bool,
    pub blast_radius_score: f64,
}

/// Analytics report with full evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsReport {
    pub report_id: String,
    pub scope: AnalyticsScope,
    pub structural_health: StructuralHealth,
    pub static_unreferenced: Vec<StaticUnreferencedCandidate>,
    pub runtime_unobserved: Vec<RuntimeUnobservedCandidate>,
    pub git_evolution: GitEvolutionResult,
    pub performance_resources: Option<PerformanceResources>,
    pub errors_recovery: Option<ErrorsRecovery>,
    pub coverage_state: CoverageState,
    pub generated_at: u64,
}

/// Analytics report scope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsScope {
    pub repository_id: String,
    pub snapshot_id: String,
    pub generated_at: u64,
}

/// Structural health metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuralHealth {
    pub total_files: usize,
    pub total_symbols: usize,
    pub total_edges: usize,
    pub coverage_state: String,
    pub languages: Vec<String>,
    pub cyclic_dependency_count: usize,
}

/// Static unreferenced result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticUnreferencedResult {
    pub verified_candidates: Vec<StaticUnreferencedCandidate>,
    pub unknown_candidates: Vec<StaticUnreferencedCandidate>,
    pub excluded_count: usize,
}

/// Runtime unobserved result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeUnobservedResult {
    pub status: String,
    pub windows_analyzed: usize,
    pub unobserved_candidates: Vec<RuntimeUnobservedCandidate>,
}

/// Git evolution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitEvolutionResult {
    pub semantic_git_risk: SemanticGitRisk,
    pub velocity: GitVelocity,
    pub change_concentration: ChangeConcentration,
}

/// Semantic Git risk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticGitRisk {
    pub status: String,
    pub risk_score: f64,
    pub verified_findings: Vec<GitRiskFinding>,
    pub unknown_commits: usize,
    pub heuristic_findings: Vec<GitRiskFinding>, // File-count heuristic findings
}

/// Git velocity metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitVelocity {
    pub commits_analyzed: usize,
    pub top_author_concentration_pct: f64,
    pub commits_per_author: BTreeMap<String, usize>,
}

/// Change concentration metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeConcentration {
    pub hot_files: Vec<HotFile>,
    pub commit_frequency: BTreeMap<String, usize>, // file -> commit count
}

/// Hot file (frequently changed)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotFile {
    pub file_path: String,
    pub change_count: usize,
    pub last_modified: u64,
    pub risk_score: f64,
}
