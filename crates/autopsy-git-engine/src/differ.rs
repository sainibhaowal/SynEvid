//! Any-to-Any Commit Structural Diff & Drift Engine.
//!
//! Complies with FR-P3-004: Pure-Rust in-memory tree comparison yielding deterministic
//! symbol and blast-radius diffs between any two historical commits or branches.

use crate::model::*;
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
        let mut store = GitObjectStore::discover(&self.repo_root)?;
        let git_dir = store.git_dir().to_path_buf();
        let base_oid = crate::reader::resolve_revision(&mut store, &git_dir, base_hex)?;
        let head_oid = crate::reader::resolve_revision(&mut store, &git_dir, head_hex)?;

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

        let total_changed_files = added_files.len() + modified_files.len() + deleted_files.len();
        let blast_radius_score =
            modified_files.len() * 2 + added_files.len() + deleted_files.len() * 3;

        let verdict = if !drifted_contracts.is_empty() && !deleted_files.is_empty() {
            "BREAKING".to_string()
        } else if modified_files.len() > 15 || !drifted_contracts.is_empty() {
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
        })
    }
}
