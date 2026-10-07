//! autopsy-domain: Core deterministic domain models and identifiers.
//!
//! Hard boundary: No HashMap iteration in canonical serialization paths;
//! all maps in canonical structures use BTreeMap to ensure deterministic ordering.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// Unique identifier for a repository snapshot, typically derived from a blake3 digest.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SnapshotId(pub String);

impl SnapshotId {
    pub fn new(hash: impl Into<String>) -> Self {
        Self(hash.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SnapshotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Stable symbol identity independent of line shifts.
/// Format: `namespace::module::parent#kind:name:signature_hash`
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SymbolId(pub String);

impl SymbolId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SymbolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Typed edge categories in the program dependency multigraph.
/// No stringly-typed edges allowed in the core model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Contains,
    Imports,
    References,
    Calls,
    Implements,
    Inherits,
    ReadsSchema,
    WritesSchema,
    TestCovers,
}

impl fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Contains => write!(f, "contains"),
            Self::Imports => write!(f, "imports"),
            Self::References => write!(f, "references"),
            Self::Calls => write!(f, "calls"),
            Self::Implements => write!(f, "implements"),
            Self::Inherits => write!(f, "inherits"),
            Self::ReadsSchema => write!(f, "reads_schema"),
            Self::WritesSchema => write!(f, "writes_schema"),
            Self::TestCovers => write!(f, "test_covers"),
        }
    }
}

/// Coverage status for semantic constructs.
/// Crucial rule: Unknown or unsupported semantics must never collapse into Pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    Verified,
    Partial,
    Unsupported,
    Unknown,
}

/// Visibility of a symbol or contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Protected,
    Internal,
    Private,
}

/// Precise source location.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceLocation {
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub start_col: Option<u32>,
    pub end_col: Option<u32>,
    pub symbol_id: Option<SymbolId>,
}

/// Single analyzed file unit in a snapshot.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FileUnit {
    pub path: String,
    pub language: String,
    pub content_hash: String,
    pub is_generated: bool,
}

/// Immutable repository snapshot metadata and file manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoSnapshot {
    pub snapshot_id: SnapshotId,
    pub revision: String,
    pub config_hash: String,
    pub file_set_digest: String,
    /// Canonical map of relative file path -> FileUnit (sorted by path)
    pub files: BTreeMap<String, FileUnit>,
}

/// Normalized callable or interface contract.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Contract {
    pub owner: SymbolId,
    pub visibility: Visibility,
    pub inputs: Vec<String>,
    pub output: Option<String>,
    pub effects: Vec<String>,
}

/// Normalized symbol representing a declaration with a stable identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Symbol {
    pub stable_id: SymbolId,
    pub language_id: String,
    pub kind: String,
    pub qualified_name: String,
    pub range: Option<SourceLocation>,
    pub normalized_signature: Option<String>,
    pub visibility: Visibility,
}

/// Typed directed edge in the dependency/reference multigraph with explicit provenance.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Edge {
    pub source: SymbolId,
    pub target: SymbolId,
    pub kind: EdgeKind,
    pub coverage: CoverageState,
    pub location: Option<SourceLocation>,
    pub provenance: String,
}

/// Status of an invariant finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingStatus {
    Pass,
    Fail,
    Unknown,
}

/// Severity level for an invariant or rule failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// Individual rule verification finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub rule_id: String,
    pub status: FindingStatus,
    pub severity: Severity,
    pub message: String,
    pub entities: Vec<String>,
    pub coverage: CoverageState,
}

/// Verifiable evidence receipt connecting finding to exact locations and graph paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub finding_id: String,
    pub rule_id: String,
    pub locations: Vec<SourceLocation>,
    pub paths: Vec<Vec<String>>,
    pub deltas: Vec<String>,
    pub evidence_digest: String,
}

/// Change summary between two repository snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSet {
    pub before_snapshot_id: SnapshotId,
    pub after_snapshot_id: SnapshotId,
    pub file_deltas: Vec<String>,
    pub symbol_deltas: Vec<String>,
    pub contract_deltas: Vec<String>,
    pub edge_deltas: Vec<String>,
}

/// Analysis run metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisRun {
    pub run_id: String,
    pub command: String,
    pub analyzer_version: String,
    pub duration_ms: u64,
    pub result_digest: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_id_display() {
        let sid = SnapshotId::new("blake3_test_hash");
        assert_eq!(sid.to_string(), "blake3_test_hash");
        assert_eq!(sid.as_str(), "blake3_test_hash");
    }

    #[test]
    fn test_edge_kind_display() {
        assert_eq!(EdgeKind::Calls.to_string(), "calls");
        assert_eq!(EdgeKind::Inherits.to_string(), "inherits");
        assert_eq!(EdgeKind::Contains.to_string(), "contains");
    }

    #[test]
    fn test_coverage_state_serde_roundtrip() {
        let state = CoverageState::Unknown;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "\"unknown\"");
        let deserialized: CoverageState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, state);
    }

    #[test]
    fn test_canonical_ordering_btreemap() {
        let mut files = BTreeMap::new();
        files.insert(
            "src/z.rs".to_string(),
            FileUnit {
                path: "src/z.rs".to_string(),
                language: "rust".to_string(),
                content_hash: "hash_z".to_string(),
                is_generated: false,
            },
        );
        files.insert(
            "src/a.rs".to_string(),
            FileUnit {
                path: "src/a.rs".to_string(),
                language: "rust".to_string(),
                content_hash: "hash_a".to_string(),
                is_generated: false,
            },
        );

        let keys: Vec<&String> = files.keys().collect();
        assert_eq!(keys, vec!["src/a.rs", "src/z.rs"]);
    }
}
