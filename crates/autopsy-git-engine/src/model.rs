//! Data models and schemas for the Native Git Galaxy, Time-Travel Engine, and Guarded Mutations.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitGalaxyNode {
    pub oid: String,
    pub short_oid: String,
    pub summary: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,
    pub parents: Vec<String>,
    pub lane: usize,
    pub is_head: bool,
    pub branches: Vec<String>,
    pub tags: Vec<String>,
    pub symbols_modified: usize,
    pub symbols_added: usize,
    pub symbols_deleted: usize,
    pub blast_radius_score: usize,
    pub verdict: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BranchRef {
    pub name: String,
    pub target_oid: String,
    pub is_head: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TagRef {
    pub name: String,
    pub target_oid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitGalaxy {
    pub repo_id: String,
    pub head_oid: Option<String>,
    pub active_branch: Option<String>,
    pub commits: Vec<CommitGalaxyNode>,
    pub branches: Vec<BranchRef>,
    pub tags: Vec<TagRef>,
    pub total_commits: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeTravelNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub cluster_id: u32,
    pub diff_status: String, // "added" | "modified" | "unchanged" | "deleted"
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimeTravelEdge {
    pub source: String,
    pub target: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeTravelVisualFrame {
    pub oid: String,
    pub short_oid: String,
    pub summary: String,
    pub timestamp: i64,
    pub dimensions: String,
    pub node_count: usize,
    pub edge_count: usize,
    pub nodes: Vec<TimeTravelNode>,
    pub edges: Vec<TimeTravelEdge>,
    pub added_files: Vec<String>,
    pub modified_files: Vec<String>,
    pub deleted_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticDiffReport {
    pub base_oid: String,
    pub head_oid: String,
    pub added_files: Vec<String>,
    pub modified_files: Vec<String>,
    pub deleted_files: Vec<String>,
    pub total_changed_files: usize,
    pub blast_radius_score: usize,
    pub verdict: String,
    pub drifted_contracts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitMutationResult {
    pub success: bool,
    pub status: String,
    pub action: String,
    pub new_commit_oid: Option<String>,
    pub undo_checkpoint_id: Option<String>,
    pub message: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageRequest {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitRequest {
    pub message: String,
    pub author_name: Option<String>,
    pub author_email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchRequest {
    pub branch_name: String,
    pub action: String, // "create" | "switch" | "delete"
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileStatusItem {
    pub path: String,
    pub status: String, // "modified" | "untracked" | "staged" | "deleted" | "ignored"
    pub added_lines: usize,
    pub deleted_lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitStatusReport {
    pub branch: String,
    pub head_oid: Option<String>,
    pub staged: Vec<FileStatusItem>,
    pub unstaged: Vec<FileStatusItem>,
    pub untracked: Vec<FileStatusItem>,
    pub deleted: Vec<FileStatusItem>,
    pub ignored: Vec<String>,
    pub is_clean: bool,
    pub total_uncommitted: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: String, // "add" | "del" | "ctx"
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileDiffReport {
    pub path: String,
    pub status: String,
    pub old_content: String,
    pub new_content: String,
    pub diff_lines: Vec<DiffLine>,
    pub total_additions: usize,
    pub total_deletions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagRequest {
    pub tag_name: String,
    pub target_oid: Option<String>,
    pub message: Option<String>,
    pub action: Option<String>, // "create" | "delete"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeRequest {
    pub source_branch: String,
    pub strategy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutRequest {
    pub target: String, // commit OID or branch name
    pub detach: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevertRequest {
    pub commit_oid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BisectRequest {
    pub start_oid: String, // usually good
    pub end_oid: String,   // usually bad/HEAD
    pub rule_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BisectReport {
    pub breaking_commit_oid: Option<String>,
    pub breaking_summary: String,
    pub breaking_author: String,
    pub breaking_timestamp: i64,
    pub verdict: String,
    pub drifted_symbols: Vec<String>,
    pub commits_evaluated: usize,
    pub total_scanned: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalExecRequest {
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalExecResponse {
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}
