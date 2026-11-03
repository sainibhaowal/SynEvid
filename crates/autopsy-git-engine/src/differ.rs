//! Any-to-Any Commit Structural Diff & Drift Engine.
//!
//! Complies with FR-P3-004: Pure-Rust in-memory tree comparison yielding deterministic
//! symbol and blast-radius diffs between any two historical commits or branches.

use crate::model::*;
use crate::semantic;
use autopsy_diff::{ContractDelta, DiffEngine, DiffInput};
use autopsy_temporal::git::{GitError, GitObjectStore, flatten_tree};
use std::path::{Path, PathBuf};

pub struct GitDiffer {
    pub repo_root: PathBuf,
}

impl GitDiffer {
    pub fn new(repo_root: impl AsRef<Path>) -> Self {
        Self {
            repo_root: repo_root.as_ref().to_path_buf(),
        }
    }

    /// Compute semantic difference between `base_rev` and `head_rev`.
    pub fn diff_commits(
        &self,
        base_hex: &str,
        head_hex: &str,
    ) -> Result<SemanticDiffReport, GitError> {
        let store = GitObjectStore::discover(&self.repo_root)?;
        let mut store_for_refs = GitObjectStore::discover(&self.repo_root)?;
        let git_dir = store.git_dir().to_path_buf();
        let base_oid = crate::reader::resolve_revision(&mut store_for_refs, &git_dir, base_hex)?;
        let head_oid = crate::reader::resolve_revision(&mut store_for_refs, &git_dir, head_hex)?;

        let base_commit = store.commit(base_oid)?;
        let head_commit = store.commit(head_oid)?;

        let tree_base = flatten_tree(&store, base_commit.tree)?;
        let tree_head = flatten_tree(&store, head_commit.tree)?;

        let mut added_files = Vec::new();
        let mut modified_files = Vec::new();
        let mut deleted_files = Vec::new();
        let mut drifted_contracts = Vec::new();

        for (path, (_mode, head_blob)) in &tree_head {
            match tree_base.get(path) {
                None => {
                    added_files.push(path.clone());
                    if path.contains("contract") || path.contains("schema") || path.contains("api")
                    {
                        drifted_contracts.push(format!("Added contract surface: {path}"));
                    }
                }
                Some((_b_mode, base_blob)) => {
                    if base_blob != head_blob {
                        modified_files.push(path.clone());
                        if path.contains("contract")
                            || path.contains("schema")
                            || path.contains("api")
                        {
                            drifted_contracts.push(format!("Modified contract definition: {path}"));
                        }
                    }
                }
            }
        }

        for path in tree_base.keys() {
            if !tree_head.contains_key(path) {
                deleted_files.push(path.clone());
                if path.contains("contract") || path.contains("schema") || path.contains("api") {
                    drifted_contracts.push(format!("Removed contract definition: {path}"));
                }
            }
        }

        added_files.sort();
        modified_files.sort();
        deleted_files.sort();
        drifted_contracts.sort();

        let before = semantic::analyze_tree(
            &self.repo_root,
            &store,
            base_commit.tree,
            &format!("git:{base_oid}"),
        )?;
        let after = semantic::analyze_tree(
            &self.repo_root,
            &store,
            head_commit.tree,
            &format!("git:{head_oid}"),
        )?;
        let semantic_diff = DiffEngine::compute_diff(DiffInput {
            before_snapshot_id: before.snapshot_id.clone(),
            after_snapshot_id: after.snapshot_id.clone(),
            before_files: &before.files,
            after_files: &after.files,
            before_graph: &before.graph,
            after_graph: &after.graph,
            before_contracts: &before.contracts,
            after_contracts: &after.contracts,
        });

        for delta in &semantic_diff.contract_deltas {
            match delta {
                ContractDelta::Modified {
                    owner,
                    is_breaking,
                    breaking_reasons,
                    ..
                } => {
                    if *is_breaking {
                        drifted_contracts.push(format!(
                            "Breaking contract {owner}: {}",
                            breaking_reasons.join(", ")
                        ));
                    }
                }
                ContractDelta::Added { contract } => {
                    drifted_contracts.push(format!("Added contract: {}", contract.owner));
                }
                ContractDelta::Removed { contract } => {
                    drifted_contracts.push(format!("Removed contract: {}", contract.owner));
                }
            }
        }
        drifted_contracts.sort();
        drifted_contracts.dedup();

        let total_changed_files = semantic_diff.summary.files_added
            + semantic_diff.summary.files_modified
            + semantic_diff.summary.files_deleted;
        let blast_radius_score = semantic_diff.frontier.impacted_boundary.len()
            + semantic_diff.frontier.direct_symbols.len();
        let verdict = if before.coverage_state != autopsy_domain::CoverageState::Verified
            || after.coverage_state != autopsy_domain::CoverageState::Verified
        {
            "UNKNOWN".to_string()
        } else if semantic_diff.summary.breaking_contract_changes > 0 {
            "BREAKING".to_string()
        } else if !drifted_contracts.is_empty()
            || !semantic_diff.symbol_deltas.is_empty()
            || !semantic_diff.edge_deltas.is_empty()
        {
            "WARN".to_string()
        } else {
            "PASS".to_string()
        };

        Ok(SemanticDiffReport {
            base_oid: base_oid.to_hex(),
            head_oid: head_oid.to_hex(),
            added_files,
            modified_files,
            deleted_files,
            total_changed_files,
            blast_radius_score,
            verdict,
            drifted_contracts,
            result_digest: semantic_diff.diff_digest.clone(),
            direct_symbols: semantic_diff
                .frontier
                .direct_symbols
                .iter()
                .map(ToString::to_string)
                .collect(),
            impacted_boundary: semantic_diff
                .frontier
                .impacted_boundary
                .iter()
                .map(ToString::to_string)
                .collect(),
            symbol_deltas: semantic_diff.symbol_deltas,
            contract_deltas: semantic_diff.contract_deltas,
            edge_deltas: semantic_diff.edge_deltas,
            coverage_state: format!("{:?}", after.coverage_state),
        })
    }
}
