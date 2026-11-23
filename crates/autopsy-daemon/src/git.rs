//! Git engine for native git operations

use std::path::Path;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, info, warn};

use autopsy_git_engine::{
    GitReader, GitStatusScanner, GitDiffEngine, GitBlameEngine,
    GitMutationManager, BranchRequest, CommitRequest, CheckoutRequest,
    MergeRequest, RevertRequest, StageRequest, TagRequest, BisectRequest,
    TerminalExecRequest,
};

/// Git engine wrapper
pub struct GitEngine {
    // Stateless - uses pure Rust git engine
}

impl GitEngine {
    pub fn new() -> Self {
        Self {}
    }

    /// Get repository status
    pub fn status(&self, repo_root: &Path) -> Result<GitStatusResult, String> {
        let scanner = GitStatusScanner::new(repo_root)
            .map_err(|e| format!("Failed to create status scanner: {}", e))?;
        
        let status = scanner.scan()
            .map_err(|e| format!("Failed to scan status: {}", e))?;

        Ok(GitStatusResult {
            branch: status.branch,
            head_oid: status.head_oid,
            staged: status.staged.into_iter().map(Into::into).collect(),
            unstaged: status.unstaged.into_iter().map(Into::into).collect(),
            untracked: status.untracked.into_iter().map(Into::into).collect(),
            deleted: status.deleted.into_iter().map(Into::into).collect(),
            ignored: status.ignored,
            is_clean: status.is_clean,
            total_uncommitted: status.total_uncommitted,
        })
    }

    /// Get commit galaxy (history DAG)
    pub fn galaxy(&self, repo_root: &Path, limit: Option<usize>) -> Result<GitGalaxyResult, String> {
        let reader = GitReader::new(repo_root)
            .map_err(|e| format!("Failed to create git reader: {}", e))?;
        
        let galaxy = reader.galaxy(limit.unwrap_or(50))
            .map_err(|e| format!("Failed to read galaxy: {}", e))?;

        Ok(GitGalaxyResult {
            repo_id: "".to_string(), // Will be filled by caller
            head_oid: galaxy.head_oid,
            active_branch: galaxy.active_branch,
            total_commits: galaxy.total_commits,
            commits: galaxy.commits.into_iter().map(Into::into).collect(),
            branches: galaxy.branches.into_iter().map(Into::into).collect(),
            tags: galaxy.tags.into_iter().map(Into::into).collect(),
        })
    }

    /// Get time travel frame
    pub fn time_travel(&self, repo_root: &Path, oid: &str, dim_3d: bool) -> Result<TimeTravelResult, String> {
        let reader = GitReader::new(repo_root)
            .map_err(|e| format!("Failed to create git reader: {}", e))?;
        
        let frame = reader.time_travel(oid, dim_3d)
            .map_err(|e| format!("Failed to time travel: {}", e))?;

        Ok(TimeTravelResult {
            oid: frame.oid,
            short_oid: frame.short_oid,
            summary: frame.summary,
            timestamp: frame.timestamp,
            dimensions: if dim_3d { "3D" } else { "2D" }.to_string(),
            node_count: frame.node_count,
            edge_count: frame.edge_count,
            nodes: frame.nodes.into_iter().map(Into::into).collect(),
            edges: frame.edges.into_iter().map(Into::into).collect(),
            added_files: frame.added_files,
            modified_files: frame.modified_files,
            deleted_files: frame.deleted_files,
            coverage_state: frame.coverage_state,
            result_digest: frame.result_digest,
        })
    }

    /// Get semantic diff between commits
    pub fn diff(&self, repo_root: &Path, base: &str, head: &str) -> Result<SemanticDiffResult, String> {
        let reader = GitReader::new(repo_root)
            .map_err(|e| format!("Failed to create git reader: {}", e))?;
        let diff_engine = GitDiffEngine::new(reader);
        
        let diff = diff_engine.diff(base, head)
            .map_err(|e| format!("Failed to compute diff: {}", e))?;

        Ok(SemanticDiffResult {
            base_oid: diff.base_oid,
            head_oid: diff.head_oid,
            added_files: diff.added_files,
            modified_files: diff.modified_files,
            deleted_files: diff.deleted_files,
            total_changed_files: diff.total_changed_files,
            blast_radius_score: diff.blast_radius_score,
            verdict: diff.verdict,
            drifted_contracts: diff.drifted_contracts,
            result_digest: diff.result_digest,
        })
    }

    /// Get blame for a file
    pub fn blame(&self, repo_root: &Path, path: &str, oid: Option<&str>) -> Result<BlameResult, String> {
        let reader = GitReader::new(repo_root)
            .map_err(|e| format!("Failed to create git reader: {}", e))?;
        let blame_engine = GitBlameEngine::new(reader);
        
        let blame = blame_engine.blame(path, oid)
            .map_err(|e| format!("Failed to compute blame: {}", e))?;

        Ok(BlameResult {
            path: path.to_string(),
            lines: blame.lines.into_iter().map(Into::into).collect(),
            coverage_state: blame.coverage_state,
            result_digest: blame.result_digest,
        })
    }

    /// Stage files
    pub fn stage(&self, repo_root: &Path, paths: Vec<String>) -> Result<GitMutationResult, String> {
        let mut manager = GitMutationManager::new(repo_root)
            .map_err(|e| format!("Failed to create mutation manager: {}", e))?;
        
        let result = manager.stage(paths)
            .map_err(|e| format!("Failed to stage: {}", e))?;

        Ok(GitMutationResult {
            success: true,
            status: "staged".to_string(),
            action: "stage".to_string(),
            new_commit_oid: None,
            undo_checkpoint_id: None,
            message: format!("Staged {} files", result.staged_count),
        })
    }

    /// Commit staged changes
    pub fn commit(&self, repo_root: &Path, message: &str, author_name: Option<&str>, author_email: Option<&str>) -> Result<GitMutationResult, String> {
        let mut manager = GitMutationManager::new(repo_root)
            .map_err(|e| format!("Failed to create mutation manager: {}", e))?;
        
        let result = manager.commit(message, author_name, author_email)
            .map_err(|e| format!("Failed to commit: {}", e))?;

        Ok(GitMutationResult {
            success: true,
            status: "committed".to_string(),
            action: "commit".to_string(),
            new_commit_oid: Some(result.commit_oid),
            undo_checkpoint_id: Some(result.undo_checkpoint_id),
            message: result.message,
        })
    }

    /// Create branch
    pub fn branch(&self, repo_root: &Path, name: &str, action: &str) -> Result<GitMutationResult, String> {
        let mut manager = GitMutationManager::new(repo_root)
            .map_err(|e| format!("Failed to create mutation manager: {}", e))?;
        
        let branch_request = BranchRequest {
            name: name.to_string(),
            action: action.to_string(),
        };
        
        let result = manager.branch(branch_request)
            .map_err(|e| format!("Failed to branch: {}", e))?;

        Ok(GitMutationResult {
            success: true,
            status: format!("branch_{}", action),
            action: format!("branch_{}", action),
            new_commit_oid: None,
            undo_checkpoint_id: None,
            message: result.message,
        })
    }

    /// Undo last mutation
    pub fn undo(&self, repo_root: &Path) -> Result<GitMutationResult, String> {
        let mut manager = GitMutationManager::new(repo_root)
            .map_err(|e| format!("Failed to create mutation manager: {}", e))?;
        
        let result = manager.undo()
            .map_err(|e| format!("Failed to undo: {}", e))?;

        Ok(GitMutationResult {
            success: true,
            status: "undone".to_string(),
            action: "undo".to_string(),
            new_commit_oid: Some(result.restored_oid),
            undo_checkpoint_id: None,
            message: result.message,
        })
    }

    /// Tag
    pub fn tag(&self, repo_root: &Path, tag_name: &str, target_oid: Option<&str>, message: Option<&str>, action: &str) -> Result<GitMutationResult, String> {
        let mut manager = GitMutationManager::new(repo_root)
            .map_err(|e| format!("Failed to create mutation manager: {}", e))?;
        
        let tag_request = autopsy_git_engine::TagRequest {
            tag_name: tag_name.to_string(),
            target_oid: target_oid.map(|s| s.to_string()),
            message: message.map(|s| s.to_string()),
            action: action.to_string(),
        };
        
        let result = manager.tag(tag_request)
            .map_err(|e| format!("Failed to tag: {}", e))?;

        Ok(GitMutationResult {
            success: true,
            status: format!("tag_{}", action),
            action: format!("tag_{}", action),
            new_commit_oid: None,
            undo_checkpoint_id: None,
            message: result.message,
        })
    }

    /// Merge
    pub fn merge(&self, repo_root: &Path, source_branch: &str, strategy: Option<&str>) -> Result<GitMutationResult, String> {
        let mut manager = GitMutationManager::new(repo_root)
            .map_err(|e| format!("Failed to create mutation manager: {}", e))?;
        
        let merge_request = MergeRequest {
            source_branch: source_branch.to_string(),
            strategy: strategy.map(|s| s.to_string()),
        };
        
        let result = manager.merge(merge_request)
            .map_err(|e| format!("Failed to merge: {}", e))?;

        Ok(GitMutationResult {
            success: true,
            status: "merged".to_string(),
            action: "merge".to_string(),
            new_commit_oid: Some(result.merge_commit_oid),
            undo_checkpoint_id: Some(result.undo_checkpoint_id),
            message: result.message,
        })
    }

    /// Checkout
    pub fn checkout(&self, repo_root: &Path, target: &str, detach: bool) -> Result<GitMutationResult, String> {
        let mut manager = GitMutationManager::new(repo_root)
            .map_err(|e| format!("Failed to create mutation manager: {}", e))?;
        
        let checkout_request = CheckoutRequest {
            target: target.to_string(),
            detach,
        };
        
        let result = manager.checkout(checkout_request)
            .map_err(|e| format!("Failed to checkout: {}", e))?;

        Ok(GitMutationResult {
            success: true,
            status: "checked_out".to_string(),
            action: "checkout".to_string(),
            new_commit_oid: Some(result.new_head_oid),
            undo_checkpoint_id: Some(result.undo_checkpoint_id),
            message: result.message,
        })
    }

    /// Revert commit
    pub fn revert(&self, repo_root: &Path, commit_oid: &str) -> Result<GitMutationResult, String> {
        let mut manager = GitMutationManager::new(repo_root)
            .map_err(|e| format!("Failed to create mutation manager: {}", e))?;
        
        let revert_request = RevertRequest {
            commit_oid: commit_oid.to_string(),
        };
        
        let result = manager.revert(revert_request)
            .map_err(|e| format!("Failed to revert: {}", e))?;

        Ok(GitMutationResult {
            success: true,
            status: "reverted".to_string(),
            action: "revert".to_string(),
            new_commit_oid: Some(result.revert_commit_oid),
            undo_checkpoint_id: Some(result.undo_checkpoint_id),
            message: result.message,
        })
    }

    /// Bisect
    pub fn bisect(&self, repo_root: &Path, good: &str, bad: &str, rule_id: Option<&str>) -> Result<BisectResult, String> {
        let reader = GitReader::new(repo_root)
            .map_err(|e| format!("Failed to create git reader: {}", e))?;
        
        let bisect_request = BisectRequest {
            start_oid: good.to_string(),
            end_oid: bad.to_string(),
            rule_id: rule_id.map(|s| s.to_string()),
        };
        
        let result = reader.bisect(bisect_request)
            .map_err(|e| format!("Failed to bisect: {}", e))?;

        Ok(BisectResult {
            breaking_commit_oid: result.breaking_commit_oid,
            breaking_summary: result.breaking_summary,
            breaking_author: result.breaking_author,
            breaking_timestamp: result.breaking_timestamp,
            verdict: result.verdict,
            drifted_symbols: result.drifted_symbols,
            commits_evaluated: result.commits_evaluated,
            total_scanned: result.total_scanned,
        })
    }

    /// Execute terminal command
    pub fn terminal_exec(&self, repo_root: &Path, command: &str) -> Result<TerminalExecResult, String> {
        let request = TerminalExecRequest {
            command: command.to_string(),
        };
        
        let result = autopsy_git_engine::execute_terminal(repo_root, request)
            .map_err(|e| format!("Terminal exec failed: {}", e))?;

        Ok(TerminalExecResult {
            command: result.command,
            stdout: result.stdout,
            stderr: result.stderr,
            exit_code: result.exit_code,
        })
    }
}

// Result types

#[derive(Debug, Serialize, Deserialize)]
pub struct GitStatusResult {
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

#[derive(Debug, Serialize, Deserialize)]
pub struct FileStatusItem {
    pub path: String,
    pub status: String,
    pub added_lines: usize,
    pub deleted_lines: usize,
}

impl From<autopsy_git_engine::FileStatusItem> for FileStatusItem {
    fn from(item: autopsy_git_engine::FileStatusItem) -> Self {
        Self {
            path: item.path,
            status: item.status,
            added_lines: item.added_lines,
            deleted_lines: item.deleted_lines,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitGalaxyResult {
    pub repo_id: String,
    pub head_oid: Option<String>,
    pub active_branch: Option<String>,
    pub total_commits: usize,
    pub commits: Vec<CommitGalaxyNode>,
    pub branches: Vec<BranchRef>,
    pub tags: Vec<TagRef>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommitGalaxyNode {
    pub oid: String,
    pub short_oid: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,
    pub summary: String,
    pub lane: i32,
    pub is_head: bool,
    pub branches: Option<Vec<String>>,
    pub branch_names: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub symbols_modified: usize,
    pub symbols_added: Option<usize>,
    pub symbols_deleted: Option<usize>,
    pub blast_radius_score: f64,
    pub heuristic_verdict: Option<String>,
    pub semantic_verdict: Option<String>,
    pub coverage_state: Option<String>,
    pub evidence_kind: Option<String>,
    pub verdict: Option<String>,
    pub verification_verdict: Option<String>,
}

impl From<autopsy_git_engine::CommitGalaxyNode> for CommitGalaxyNode {
    fn from(node: autopsy_git_engine::CommitGalaxyNode) -> Self {
        Self {
            oid: node.oid,
            short_oid: node.short_oid,
            parents: node.parents,
            author_name: node.author_name,
            author_email: node.author_email,
            timestamp: node.timestamp,
            summary: node.summary,
            lane: node.lane,
            is_head: node.is_head,
            branches: node.branches,
            branch_names: node.branch_names,
            tags: node.tags,
            symbols_modified: node.symbols_modified,
            symbols_added: node.symbols_added,
            symbols_deleted: node.symbols_deleted,
            blast_radius_score: node.blast_radius_score,
            heuristic_verdict: node.heuriistic_verdict,
            semantic_verdict: node.semantic_verdict,
            coverage_state: node.coverage_state,
            evidence_kind: node.evidence_kind,
            verdict: node.verdict,
            verification_verdict: node.verification_verdict,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BranchRef {
    pub name: String,
    pub target_oid: String,
    pub is_active: bool,
}

impl From<autopsy_git_engine::BranchRef> for BranchRef {
    fn from(r: autopsy_git_engine::BranchRef) -> Self {
        Self { name: r.name, target_oid: r.target_oid, is_active: r.is_active }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TagRef {
    pub name: String,
    pub target_oid: String,
}

impl From<autopsy_git_engine::TagRef> for TagRef {
    fn from(r: autopsy_git_engine::TagRef) -> Self {
        Self { name: r.name, target_oid: r.target_oid }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TimeTravelResult {
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
    pub coverage_state: Option<String>,
    pub result_digest: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TimeTravelNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub cluster_id: i32,
    pub diff_status: String,
    pub file_path: Option<String>,
    pub kind: Option<String>,
    pub start_line: Option<i64>,
    pub end_line: Option<i64>,
}

impl From<autopsy_git_engine::TimeTravelNode> for TimeTravelNode {
    fn from(n: autopsy_git_engine::TimeTravelNode) -> Self {
        Self {
            id: n.id,
            x: n.x,
            y: n.y,
            z: n.z,
            cluster_id: n.cluster_id,
            diff_status: n.diff_status,
            file_path: n.file_path,
            kind: n.kind,
            start_line: n.start_line,
            end_line: n.end_line,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TimeTravelEdge {
    pub source: String,
    pub target: String,
    pub kind: String,
}

impl From<autopsy_git_engine::TimeTravelEdge> for TimeTravelEdge {
    fn from(e: autopsy_git_engine::TimeTravelEdge) -> Self {
        Self { source: e.source, target: e.target, kind: e.kind }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SemanticDiffResult {
    pub base_oid: String,
    pub head_oid: String,
    pub added_files: Vec<String>,
    pub modified_files: Vec<String>,
    pub deleted_files: Vec<String>,
    pub total_changed_files: usize,
    pub blast_radius_score: f64,
    pub verdict: String,
    pub drifted_contracts: Vec<String>,
    pub result_digest: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlameResult {
    pub path: String,
    pub lines: Vec<BlameLine>,
    pub coverage_state: String,
    pub result_digest: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlameLine {
    pub line: usize,
    pub text: String,
    pub commit_oid: String,
    pub author: String,
    pub timestamp: i64,
    pub sequence: usize,
    pub blast_radius_score: f64,
}

impl From<autopsy_git_engine::BlameLine> for BlameLine {
    fn from(l: autopsy_git_engine::BlameLine) -> Self {
        Self {
            line: l.line,
            text: l.text,
            commit_oid: l.commit_oid,
            author: l.author,
            timestamp: l.timestamp,
            sequence: l.sequence,
            blast_radius_score: l.blast_radius_score,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitMutationResult {
    pub success: bool,
    pub status: String,
    pub action: String,
    pub new_commit_oid: Option<String>,
    pub undo_checkpoint_id: Option<String>,
    pub message: String,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BisectResult {
    pub breaking_commit_oid: Option<String>,
    pub breaking_summary: String,
    pub breaking_author: String,
    pub breaking_timestamp: i64,
    pub verdict: String,
    pub drifted_symbols: Vec<String>,
    pub commits_evaluated: usize,
    pub total_scanned: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TerminalExecResult {
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}