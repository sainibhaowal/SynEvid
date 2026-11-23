//! autopsy-protocol: Shared Command Protocol for SynEvid
//!
//! This crate defines the canonical command protocol that ALL clients
//! (CLI, TUI, MCP, Web, Electron) must use. It includes:
//! - CommandRequest / CommandResponse
//! - Event streaming protocol
//! - Authentication and lease management
//! - JSON Schema generation for validation

use std::collections::BTreeMap;
use std::time::Duration;

use blake3;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

/// Unique command identifier
pub type CommandId = String;

/// Unique repository identifier
pub type RepositoryId = String;

/// Unique snapshot identifier
pub type SnapshotId = String;

/// Unique session identifier
pub type SessionId = String;

/// Unique workflow identifier
pub type WorkflowId = String;

/// Unique event sequence number
pub type EventSeq = i64;

/// Unique lease identifier
pub type LeaseId = String;

/// Client identifier
pub type ClientId = String;

/// Client type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ClientType {
    Cli,
    Tui,
    Mcp,
    Ui,
    Electron,
    Unknown,
}

/// Output format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Json,
    Text,
    Sarif,
    Markdown,
    Html,
    Bundle,
}

/// Cancellation token for long-running operations
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CancellationToken {
    pub token: String,
    #[schemars(with = "String")]
    pub expires_at: Option<DateTime<Utc>>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            token: Uuid::new_v4().to_string(),
            expires_at: None,
        }
    }

    pub fn with_timeout(duration: Duration) -> Self {
        Self {
            token: Uuid::new_v4().to_string(),
            expires_at: Some(Utc::now() + chrono::Duration::from_std(duration).unwrap_or_default()),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.expires_at.map(|exp| Utc::now() > exp).unwrap_or(false)
    }
}

/// Canonical command request - all clients send this
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommandRequest {
    /// Unique command identifier
    pub command_id: CommandId,

    /// Repository identity
    pub repository_id: RepositoryId,

    /// Snapshot pin (for reproducible analysis)
    pub snapshot_pin: Option<SnapshotId>,

    /// Session identity (for forensic sessions)
    pub session_id: Option<SessionId>,

    /// Workflow identity (for multi-step operations)
    pub workflow_id: Option<WorkflowId>,

    /// Typed command arguments (command-specific)
    pub args: BTreeMap<String, serde_json::Value>,

    /// Cancellation token
    pub cancellation_token: Option<CancellationToken>,

    /// Desired output format
    pub output_format: Option<OutputFormat>,

    /// Event receipt request
    pub event_receipt: bool,

    /// Client metadata
    pub client_id: ClientId,
    pub client_type: ClientType,
}

/// Canonical command response - all clients receive this
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommandResponse {
    /// Echo of request command_id
    pub command_id: CommandId,

    /// Operation identifier (BLAKE3 over canonical request)
    pub operation_id: String,

    /// Repository identity
    pub repository_id: RepositoryId,

    /// Canonical repository path
    pub canonical_path: String,

    /// Snapshot identity
    pub snapshot_id: Option<SnapshotId>,

    /// Session identity
    pub session_id: Option<SessionId>,

    /// Workflow identity
    pub workflow_id: Option<WorkflowId>,

    /// Event sequence number
    pub event_seq: Option<EventSeq>,

    /// Verdict
    pub verdict: Verdict,

    /// Coverage state
    pub coverage_state: CoverageState,

    /// Warnings
    #[serde(default)]
    pub warnings: Vec<String>,

    /// Latency in milliseconds
    pub latency_ms: Option<f64>,

    /// BLAKE3 digest of response payload
    pub blake3_digest: String,

    /// Evidence receipt
    pub evidence_receipt: EvidenceReceipt,

    /// Command-specific payload
    #[serde(flatten)]
    pub payload: serde_json::Value,
}

/// Canonical verdict states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum Verdict {
    Pass,
    Fail,
    Warn,
    Partial,
    Unknown,
    Error,
    Unavailable,
}

impl Verdict {
    pub fn is_success(&self) -> bool {
        matches!(self, Verdict::Pass)
    }
}

/// Canonical coverage states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CoverageState {
    Verified,
    Partial,
    Unknown,
    Unavailable,
    Error,
}

impl CoverageState {
    pub fn allows_pass(&self) -> bool {
        matches!(self, CoverageState::Verified)
    }
}

/// Evidence receipt
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EvidenceReceipt {
    pub algorithm: String,
    pub digest: String,
    pub source: String,
    pub test_reference: String,
    #[schemars(with = "String")]
    pub timestamp: DateTime<Utc>,
    pub engine_version: String,
    #[serde(default)]
    pub adapter_versions: BTreeMap<String, String>,
}

/// Event streaming protocol
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EventRecord {
    pub seq_id: EventSeq,
    pub event_type: String,
    #[schemars(with = "String")]
    pub timestamp: DateTime<Utc>,
    pub repository_id: Option<RepositoryId>,
    pub session_id: Option<SessionId>,
    pub workflow_id: Option<WorkflowId>,
    pub source_client_id: ClientId,
    pub source_client_type: ClientType,
    pub source_lease_id: Option<LeaseId>,
    pub payload: serde_json::Value,
    pub evidence_digest: String,
}

/// Event subscription request
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EventSubscribeRequest {
    pub client_id: ClientId,
    pub client_type: ClientType,
    pub lease_id: Option<LeaseId>,
    pub since_seq: Option<EventSeq>,
    pub event_types: Option<Vec<String>>,
}

/// Runtime lease management
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeLease {
    pub lease_id: LeaseId,
    pub client_id: ClientId,
    pub client_type: ClientType,
    pub repository_id: Option<RepositoryId>,
    pub session_id: Option<SessionId>,
    pub workflow_id: Option<WorkflowId>,
    #[schemars(with = "String")]
    pub created_at: DateTime<Utc>,
    #[schemars(with = "String")]
    pub last_heartbeat: DateTime<Utc>,
    pub last_event_seq: EventSeq,
}

/// Runtime connect request
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeConnectRequest {
    pub client_id: ClientId,
    pub client_type: ClientType,
    pub repository_id: Option<RepositoryId>,
    pub session_id: Option<SessionId>,
    pub workflow_id: Option<WorkflowId>,
    pub last_event_seq: Option<EventSeq>,
}

/// Runtime heartbeat request
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeHeartbeatRequest {
    pub lease_id: LeaseId,
    pub last_event_seq: Option<EventSeq>,
    pub session_id: Option<SessionId>,
    pub workflow_id: Option<WorkflowId>,
}

/// Runtime disconnect request
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeDisconnectRequest {
    pub lease_id: LeaseId,
    pub client_id: ClientId,
}

/// Command definitions - each command has a unique ID and typed args
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommandDefinition {
    pub command_id: CommandId,
    pub name: String,
    pub description: String,
    pub args_schema: serde_json::Value, // JSON Schema for args
    pub output_schema: serde_json::Value, // JSON Schema for output
    pub requires_daemon: bool,
    pub supports_offline: bool,
    pub supports_snapshot_pin: bool,
    pub supports_session: bool,
}

/// Get all command definitions
pub fn all_commands() -> Vec<CommandDefinition> {
    vec![
        // Repository management
        CommandDefinition {
            command_id: "attach".to_string(),
            name: "attach".to_string(),
            description: "Register and activate a repository through the daemon".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Repository path" },
                    "display_name": { "type": "string", "description": "Optional display name" }
                },
                "required": ["path"]
            }),
            output_schema: json!({
                "type": "object",
                "properties": {
                    "status": { "type": "string" },
                    "repo_id": { "type": "string" },
                    "display_name": { "type": "string" },
                    "root_path": { "type": "string" }
                }
            }),
            requires_daemon: true,
            supports_offline: false,
            supports_snapshot_pin: false,
            supports_session: false,
        },
        CommandDefinition {
            command_id: "status".to_string(),
            name: "status".to_string(),
            description: "Read daemon health, active repository, snapshot, cursor, and storage state".to_string(),
            args_schema: json!({ "type": "object", "properties": {} }),
            output_schema: json!({
                "type": "object",
                "properties": {
                    "daemon_id": { "type": "string" },
                    "active_repo_id": { "type": "string" },
                    "snapshot_id": { "type": "string" },
                    "last_event_seq": { "type": "integer" },
                    "storage_health": { "type": "string" }
                }
            }),
            requires_daemon: true,
            supports_offline: false,
            supports_snapshot_pin: false,
            supports_session: false,
        },

        // Core analysis
        CommandDefinition {
            command_id: "baseline".to_string(),
            name: "baseline".to_string(),
            description: "Scan repository, compute deterministic snapshot, build dependency graph".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "revision": { "type": "string" },
                    "incremental": { "type": "boolean" },
                    "baseline_id": { "type": "string" }
                }
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: true,
            supports_snapshot_pin: false,
            supports_session: false,
        },
        CommandDefinition {
            command_id: "impact".to_string(),
            name: "impact".to_string(),
            description: "Compute bounded transitive impact analysis from seed symbols".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "symbol": { "type": "array", "items": { "type": "string" }, "minItems": 1 },
                    "direction": { "type": "string", "enum": ["forward", "backward", "bidirectional"] },
                    "max_depth": { "type": "integer", "minimum": 1 },
                    "budget": { "type": "integer", "minimum": 1 },
                    "profile": { "type": "string", "enum": ["full", "callers"] },
                    "edge_kinds": { "type": "string" },
                    "top_k": { "type": "integer", "minimum": 1 },
                    "detail": { "type": "string", "enum": ["summary", "full"] },
                    "snapshot_id": { "type": "string" }
                },
                "required": ["symbol"]
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: true,
            supports_snapshot_pin: true,
            supports_session: true,
        },
        CommandDefinition {
            command_id: "verify".to_string(),
            name: "verify".to_string(),
            description: "Evaluate architectural invariants against repository snapshot or diff".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "strict": { "type": "boolean" },
                    "invariants_file": { "type": "string" },
                    "baseline_id": { "type": "string" }
                }
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: true,
            supports_snapshot_pin: true,
            supports_session: true,
        },
        CommandDefinition {
            command_id: "diff".to_string(),
            name: "diff".to_string(),
            description: "Compare two snapshots or baseline against worktree".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "before": { "type": "string" },
                    "after": { "type": "string" }
                }
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: true,
            supports_snapshot_pin: true,
            supports_session: false,
        },

        // Graph browsing
        CommandDefinition {
            command_id: "symbols".to_string(),
            name: "symbols".to_string(),
            description: "List symbols in stable-id order with cursor pagination".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "minimum": 1, "maximum": 1000 },
                    "offset": { "type": "integer", "minimum": 0 },
                    "kind": { "type": "string" },
                    "snapshot_id": { "type": "string" }
                }
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: true,
            supports_snapshot_pin: true,
            supports_session: false,
        },
        CommandDefinition {
            command_id: "find".to_string(),
            name: "find".to_string(),
            description: "Search symbols by stable id / qualified name / path, case-insensitive".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "minLength": 1 },
                    "kind": { "type": "string" },
                    "name_only": { "type": "boolean" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 1000 },
                    "offset": { "type": "integer", "minimum": 0 },
                    "snapshot_id": { "type": "string" }
                },
                "required": ["pattern"]
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: true,
            supports_snapshot_pin: true,
            supports_session: false,
        },
        CommandDefinition {
            command_id: "edges".to_string(),
            name: "edges".to_string(),
            description: "List 1-hop edges for a symbol with cursor pagination".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string" },
                    "direction": { "type": "string", "enum": ["forward", "backward", "bidirectional"] },
                    "kind": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 1000 },
                    "offset": { "type": "integer", "minimum": 0 },
                    "snapshot_id": { "type": "string" }
                },
                "required": ["source"]
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: true,
            supports_snapshot_pin: true,
            supports_session: false,
        },
        CommandDefinition {
            command_id: "map".to_string(),
            name: "map".to_string(),
            description: "Bounded whole-graph projection: kind counts, package counts, top-N by degree".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "top": { "type": "integer", "minimum": 1, "maximum": 500 },
                    "under": { "type": "string" },
                    "snapshot_id": { "type": "string" }
                }
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: true,
            supports_snapshot_pin: true,
            supports_session: false,
        },

        // Forensic / Sessions
        CommandDefinition {
            command_id: "session".to_string(),
            name: "session".to_string(),
            description: "Repair-loop session lifecycle: start, record, close, report, show, list".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["start", "record", "close", "report", "show", "list"] },
                    "session_id": { "type": "string" },
                    "invoked": { "type": "array", "items": { "type": "string" } },
                    "exit_code": { "type": "integer" },
                    "snapshot_id": { "type": "string" },
                    "note": { "type": "string" }
                },
                "required": ["action"]
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: false,
            supports_snapshot_pin: true,
            supports_session: true,
        },

        // Git
        CommandDefinition {
            command_id: "git".to_string(),
            name: "git".to_string(),
            description: "Native pure-Rust Git engine: status, galaxy, diff, stage, commit, branch, tag, merge, bisect, undo".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "enum": ["status", "galaxy", "diff", "stage", "commit", "branch", "tag", "merge", "bisect", "undo", "checkout", "revert"] },
                    "base": { "type": "string" },
                    "head": { "type": "string" },
                    "message": { "type": "string" },
                    "target": { "type": "string" },
                    "good": { "type": "string" },
                    "bad": { "type": "string" },
                    "paths": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["command"]
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: true,
            supports_snapshot_pin: false,
            supports_session: false,
        },

        // Analytics
        CommandDefinition {
            command_id: "analytics".to_string(),
            name: "analytics".to_string(),
            description: "Generate dead-code, Git risk, and AI audit reports".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "format": { "type": "string", "enum": ["json", "markdown", "html", "sarif", "bundle"] }
                }
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: false,
            supports_snapshot_pin: true,
            supports_session: false,
        },

        // Telemetry
        CommandDefinition {
            command_id: "telemetry".to_string(),
            name: "telemetry".to_string(),
            description: "Ingest OTLP trace export and correlate with declared surface".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "traces": { "type": "string" },
                    "salt": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                    "surface": { "type": "string" },
                    "windows": { "type": "integer", "minimum": 1 },
                    "complete_observation": { "type": "boolean" },
                    "dispatch_static": { "type": "boolean" }
                },
                "required": ["traces", "salt"]
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: false,
            supports_snapshot_pin: true,
            supports_session: false,
        },

        // Catalog
        CommandDefinition {
            command_id: "catalog".to_string(),
            name: "catalog".to_string(),
            description: "Ingest Backstage catalog entities and dependency manifests".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "root": { "type": "string" },
                    "max_file_bytes": { "type": "integer" }
                }
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: true,
            supports_snapshot_pin: true,
            supports_session: false,
        },
        CommandDefinition {
            command_id: "blast-radius".to_string(),
            name: "blast-radius".to_string(),
            description: "Organizational blast radius of one catalog component".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "reference": { "type": "string" },
                    "root": { "type": "string" },
                    "max_file_bytes": { "type": "integer" }
                },
                "required": ["reference"]
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: true,
            supports_snapshot_pin: true,
            supports_session: false,
        },

        // Temporal / Lineage / Co-change
        CommandDefinition {
            command_id: "temporal".to_string(),
            name: "temporal".to_string(),
            description: "Replay git history into revision journal or reconstruct at one revision".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "at": { "type": "string" },
                    "max_commits": { "type": "integer" },
                    "strict": { "type": "boolean" },
                    "compact": { "type": "boolean" },
                    "top_k": { "type": "integer" }
                }
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: true,
            supports_snapshot_pin: true,
            supports_session: false,
        },
        CommandDefinition {
            command_id: "lineage".to_string(),
            name: "lineage".to_string(),
            description: "Trace file identity across a revision pair: renames, moves, modifications".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string" },
                    "to": { "type": "string" },
                    "threshold": { "type": "number" },
                    "margin": { "type": "number" },
                    "max_files": { "type": "integer" },
                    "max_file_bytes": { "type": "integer" },
                    "strict": { "type": "boolean" },
                    "compact": { "type": "boolean" },
                    "top_k": { "type": "integer" }
                }
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: true,
            supports_snapshot_pin: true,
            supports_session: false,
        },
        CommandDefinition {
            command_id: "cochange".to_string(),
            name: "cochange".to_string(),
            description: "Mine git history for paths that change together".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "window_seconds": { "type": "integer" },
                    "max_commits": { "type": "integer" },
                    "max_files_per_commit": { "type": "integer" },
                    "min_pair_commits": { "type": "integer" },
                    "strict": { "type": "boolean" },
                    "compact": { "type": "boolean" },
                    "top_k": { "type": "integer" }
                }
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: true,
            supports_snapshot_pin: true,
            supports_session: false,
        },
        CommandDefinition {
            command_id: "contract-dist".to_string(),
            name: "contract-dist".to_string(),
            description: "Diff two contract documents across repositories".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string" },
                    "to": { "type": "string" },
                    "protocol": { "type": "string", "enum": ["openapi", "asyncapi", "asyncapi-flows", "pact"] },
                    "strict": { "type": "boolean" },
                    "compact": { "type": "boolean" },
                    "top_k": { "type": "integer" }
                },
                "required": ["from", "to"]
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: true,
            supports_snapshot_pin: true,
            supports_session: false,
        },
        CommandDefinition {
            command_id: "test-select".to_string(),
            name: "test-select".to_string(),
            description: "Select tests that must run for a set of changed files".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "changed": { "type": "array", "items": { "type": "string" } },
                    "lcov": { "type": "array", "items": { "type": "string" } },
                    "fail_if_all_tests_required": { "type": "boolean" },
                    "strict": { "type": "boolean" }
                },
                "required": ["changed"]
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: true,
            supports_snapshot_pin: true,
            supports_session: false,
        },

        // Presentation
        CommandDefinition {
            command_id: "ui".to_string(),
            name: "ui".to_string(),
            description: "Launch embedded web dashboard".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "port": { "type": "integer" },
                    "host": { "type": "string" },
                    "open": { "type": "boolean" }
                }
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: false,
            supports_snapshot_pin: false,
            supports_session: false,
        },
        CommandDefinition {
            command_id: "tui".to_string(),
            name: "tui".to_string(),
            description: "Launch interactive terminal dashboard".to_string(),
            args_schema: json!({ "type": "object", "properties": {} }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: true,
            supports_snapshot_pin: false,
            supports_session: false,
        },
        CommandDefinition {
            command_id: "watch".to_string(),
            name: "watch".to_string(),
            description: "Real-time live watch mode for automatic incremental verification".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "debounce": { "type": "integer", "minimum": 1 }
                }
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: false,
            supports_snapshot_pin: false,
            supports_session: false,
        },

        // Utility
        CommandDefinition {
            command_id: "doctor".to_string(),
            name: "doctor".to_string(),
            description: "Verify repository configuration, storage integrity, and analyzer health".to_string(),
            args_schema: json!({ "type": "object", "properties": {} }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: true,
            supports_snapshot_pin: false,
            supports_session: false,
        },
        CommandDefinition {
            command_id: "init".to_string(),
            name: "init".to_string(),
            description: "Initialize autopsy.toml and .autopsy/invariants.yml".to_string(),
            args_schema: json!({
                "type": "object",
                "properties": {
                    "force": { "type": "boolean" }
                }
            }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: true,
            supports_offline: true,
            supports_snapshot_pin: false,
            supports_session: false,
        },
        CommandDefinition {
            command_id: "tools".to_string(),
            name: "tools".to_string(),
            description: "Emit machine-readable tool specification".to_string(),
            args_schema: json!({ "type": "object", "properties": {} }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: false,
            supports_offline: true,
            supports_snapshot_pin: false,
            supports_session: false,
        },
        CommandDefinition {
            command_id: "version".to_string(),
            name: "version".to_string(),
            description: "Display engine version and adapter capabilities".to_string(),
            args_schema: json!({ "type": "object", "properties": {} }),
            output_schema: json!({ "type": "object" }),
            requires_daemon: false,
            supports_offline: true,
            supports_snapshot_pin: false,
            supports_session: false,
        },
    ]
}

/// Generate JSON Schema for the protocol
pub fn generate_schema() -> serde_json::Value {
    let schema = schemars::schema_for!(CommandRequest);
    serde_json::to_value(schema).unwrap_or(serde_json::json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_request_serialization() {
        let request = CommandRequest {
            command_id: "test-123".to_string(),
            repository_id: "repo-456".to_string(),
            snapshot_pin: Some("snap-789".to_string()),
            session_id: None,
            workflow_id: None,
            args: BTreeMap::new(),
            cancellation_token: None,
            output_format: Some(OutputFormat::Json),
            event_receipt: true,
            client_id: "cli:default".to_string(),
            client_type: ClientType::Cli,
        };

        let json = serde_json::to_string(&request).unwrap();
        let parsed: CommandRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.command_id, "test-123");
    }

    #[test]
    fn test_all_commands_defined() {
        let commands = all_commands();
        assert!(commands.len() >= 30, "Should have at least 30 commands");
        
        // Check for required commands
        let names: Vec<_> = commands.iter().map(|c| c.name.clone()).collect();
        assert!(names.contains(&"impact".to_string()));
        assert!(names.contains(&"verify".to_string()));
        assert!(names.contains(&"baseline".to_string()));
        assert!(names.contains(&"diff".to_string()));
        assert!(names.contains(&"session".to_string()));
        assert!(names.contains(&"git".to_string()));
    }

    #[test]
    fn test_verdict_serialization() {
        let v = Verdict::Pass;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"PASS\"");
        
        let parsed: Verdict = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Verdict::Pass);
    }

    #[test]
    fn test_coverage_state() {
        assert!(CoverageState::Verified.allows_pass());
        assert!(!CoverageState::Partial.allows_pass());
        assert!(!CoverageState::Unknown.allows_pass());
        assert!(!CoverageState::Unavailable.allows_pass());
        assert!(!CoverageState::Error.allows_pass());
    }
}