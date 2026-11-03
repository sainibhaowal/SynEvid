//! Guarded In-UI Git Mutation Manager with Atomic Undo Safety Stack.
//!
//! Complies with FR-P3-005: Every mutating git operation snapshots the previous state
//! into `refs/autopsy/undo/<blake3>` for zero-loss recovery (Cmd+Z).

use crate::index::{
    IndexEntry, clean_worktree_bytes, index_digest, index_path, is_safe_index_path, read_index,
    write_index,
};
use crate::model::*;
use crate::semantic;
use crate::status::GitStatusScanner;
use autopsy_diff::{DiffEngine, DiffInput};
use autopsy_temporal::git::{GitError, GitObjectStore, ObjectId, compute_object_id, flatten_tree};
use flate2::Compression;
use flate2::write::ZlibEncoder;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

enum GitTreeNode {
    Blob(u32, ObjectId),
    Tree(BTreeMap<String, GitTreeNode>),
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct UndoCheckpoint {
    target_oid: String,
    head_ref: Option<String>,
    #[serde(default)]
    branch_path: Option<String>,
    #[serde(default)]
    branch_oid: Option<String>,
}

#[derive(Clone)]
struct BisectEvaluation {
    is_bad: bool,
    unknown: bool,
    drifted_symbols: Vec<String>,
}

pub struct GitMutationManager {
    pub repo_root: PathBuf,
}

impl GitMutationManager {
    pub fn new(repo_root: impl AsRef<Path>) -> Self {
        Self {
            repo_root: repo_root.as_ref().to_path_buf(),
        }
    }

    fn git_dir(&self) -> Result<PathBuf, GitError> {
        let store = GitObjectStore::discover(&self.repo_root)?;
        Ok(store.git_dir().to_path_buf())
    }

    fn validate_ref_name(name: &str, kind: &str) -> Result<(), GitError> {
        let valid = !name.is_empty()
            && !name.contains('\\')
            && !name.starts_with('/')
            && !name.contains('\0')
            && name
                .split('/')
                .all(|segment| !segment.is_empty() && segment != "." && segment != "..");
        if valid {
            Ok(())
        } else {
            Err(GitError::Malformed(format!(
                "unsafe {kind} ref name: {name}"
            )))
        }
    }

    fn head_index_entries(
        &self,
        store: &mut GitObjectStore,
    ) -> Result<BTreeMap<String, IndexEntry>, GitError> {
        let Some(head_oid) = store.resolve_ref("HEAD")? else {
            return Ok(BTreeMap::new());
        };
        let commit = store.commit(head_oid)?;
        let tree = flatten_tree(store, commit.tree)?;
        Ok(tree
            .into_iter()
            .map(|(path, (mode, oid))| {
                let size = store.blob(oid).map(|bytes| bytes.len() as u32).unwrap_or(0);
                (
                    path.clone(),
                    IndexEntry {
                        mode,
                        oid,
                        path,
                        size,
                    },
                )
            })
            .collect())
    }

    fn ensure_clean_worktree(&self) -> Result<(), GitError> {
        let status = GitStatusScanner::new(&self.repo_root).scan_status()?;
        if !status.is_clean {
            return Err(GitError::Malformed(
                "ERR-GIT-204: working tree has uncommitted changes; commit, stage, or undo them before checkout".to_string(),
            ));
        }
        Ok(())
    }

    fn semantic_preflight(
        &self,
        store: &GitObjectStore,
        before_tree: ObjectId,
        after_tree: ObjectId,
    ) -> Result<(String, String, String), GitError> {
        let before = semantic::analyze_tree(
            &self.repo_root,
            store,
            before_tree,
            &format!("git:preflight:{}", before_tree.to_hex()),
        )?;
        let after = semantic::analyze_tree(
            &self.repo_root,
            store,
            after_tree,
            &format!("git:preflight:{}", after_tree.to_hex()),
        )?;
        let diff = DiffEngine::compute_diff(DiffInput {
            before_snapshot_id: before.snapshot_id.clone(),
            after_snapshot_id: after.snapshot_id.clone(),
            before_files: &before.files,
            after_files: &after.files,
            before_graph: &before.graph,
            after_graph: &after.graph,
            before_contracts: &before.contracts,
            after_contracts: &after.contracts,
        });
        let verdict = if before.coverage_state != autopsy_domain::CoverageState::Verified
            || after.coverage_state != autopsy_domain::CoverageState::Verified
        {
            "UNKNOWN"
        } else if diff.summary.breaking_contract_changes > 0 {
            "WARN"
        } else {
            "PASS"
        };
        Ok((
            verdict.to_string(),
            format!(
                "{} broken contracts; {} impacted symbols",
                diff.summary.breaking_contract_changes,
                diff.frontier.impacted_boundary.len() + diff.frontier.direct_symbols.len()
            ),
            diff.diff_digest,
        ))
    }

    fn checkout_tree(
        &self,
        git_dir: &Path,
        store: &mut GitObjectStore,
        target_tree: ObjectId,
    ) -> Result<(), GitError> {
        let current_entries = store
            .resolve_ref("HEAD")?
            .and_then(|head| store.commit(head).ok())
            .and_then(|commit| flatten_tree(store, commit.tree).ok())
            .unwrap_or_default();
        let target_entries = flatten_tree(store, target_tree)?;

        for path in current_entries.keys() {
            if !target_entries.contains_key(path) {
                let full_path = self.repo_root.join(path);
                if full_path.exists() {
                    fs::remove_file(&full_path).map_err(|e| GitError::Io {
                        path: full_path,
                        source: e,
                    })?;
                }
            }
        }

        for (path, (mode, oid)) in &target_entries {
            if !is_safe_index_path(path) {
                return Err(GitError::Malformed(format!(
                    "unsafe historical tree path: {path}"
                )));
            }
            let full_path = self.repo_root.join(path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent).map_err(|e| GitError::Io {
                    path: parent.to_path_buf(),
                    source: e,
                })?;
            }
            let bytes = store.blob(*oid)?;
            fs::write(&full_path, &bytes).map_err(|e| GitError::Io {
                path: full_path.clone(),
                source: e,
            })?;
            let _ = mode;
        }

        let index_entries: BTreeMap<String, IndexEntry> = target_entries
            .into_iter()
            .map(|(path, (mode, oid))| {
                let size = store.blob(oid).map(|bytes| bytes.len() as u32).unwrap_or(0);
                (
                    path.clone(),
                    IndexEntry {
                        mode,
                        oid,
                        path,
                        size,
                    },
                )
            })
            .collect();
        write_index(git_dir, &index_entries)
    }

    fn update_head(
        &self,
        git_dir: &Path,
        target_oid: ObjectId,
        branch_ref: Option<&str>,
    ) -> Result<(), GitError> {
        let head_path = git_dir.join("HEAD");
        let target_path = if let Some(branch_ref) = branch_ref {
            fs::write(&head_path, format!("ref: {branch_ref}\n")).map_err(|e| GitError::Io {
                path: head_path,
                source: e,
            })?;
            git_dir.join(branch_ref)
        } else {
            head_path
        };
        fs::write(&target_path, format!("{}\n", target_oid.to_hex())).map_err(|e| GitError::Io {
            path: target_path,
            source: e,
        })
    }

    /// Stage specific files for pre-commit verification.
    pub fn stage(&self, paths: &[String]) -> Result<GitMutationResult, GitError> {
        let git_dir = self.git_dir()?;
        let mut store = GitObjectStore::discover(&self.repo_root)?;
        let mut entries = read_index(&git_dir)?;
        if entries.is_empty() {
            entries = self.head_index_entries(&mut store)?;
        }

        for path in paths {
            if !is_safe_index_path(path) {
                return Err(GitError::Malformed(format!(
                    "unsafe repository-relative stage path: {path}"
                )));
            }
            let full_path = self.repo_root.join(path);
            if full_path.is_file() {
                let bytes = fs::read(&full_path).map_err(|e| GitError::Io {
                    path: full_path.clone(),
                    source: e,
                })?;
                let clean_bytes = clean_worktree_bytes(path, &bytes);
                let mode = entries
                    .get(path)
                    .map(|entry| entry.mode)
                    .unwrap_or(0o100644);
                // A real index entry must point at an object that exists in the
                // object database. Computing the OID alone would make status
                // appear staged while a later commit could not read the blob.
                let oid = self.write_loose_object(&git_dir, "blob", &clean_bytes)?;
                entries.insert(
                    path.clone(),
                    IndexEntry {
                        mode,
                        oid,
                        path: path.clone(),
                        size: bytes.len() as u32,
                    },
                );
            } else if !full_path.exists() {
                // A staged path that disappeared represents a staged deletion.
                entries.remove(path);
            } else {
                return Err(GitError::Malformed(format!(
                    "cannot stage non-file path: {path}"
                )));
            }
        }
        write_index(&git_dir, &entries)?;

        Ok(GitMutationResult {
            success: true,
            status: "success".to_string(),
            action: "stage".to_string(),
            new_commit_oid: None,
            undo_checkpoint_id: None,
            message: format!("Staged {} file(s) for verification", paths.len()),
            error: None,
        })
    }

    /// Stage selected unified hunks into the real Git index without staging
    /// unrelated working-tree lines.
    pub fn stage_hunks(
        &self,
        hunks_by_path: &BTreeMap<String, Vec<StageHunk>>,
    ) -> Result<GitMutationResult, GitError> {
        let git_dir = self.git_dir()?;
        let mut store = GitObjectStore::discover(&self.repo_root)?;
        let mut entries = read_index(&git_dir)?;
        if entries.is_empty() {
            entries = self.head_index_entries(&mut store)?;
        }

        for (path, hunks) in hunks_by_path {
            if !is_safe_index_path(path) {
                return Err(GitError::Malformed(format!(
                    "unsafe repository-relative hunk path: {path}"
                )));
            }
            let base = entries
                .get(path)
                .and_then(|entry| store.blob(entry.oid).ok())
                .unwrap_or_default();
            let base_text = String::from_utf8(base).map_err(|_| {
                GitError::Malformed(format!("cannot hunk-stage binary file: {path}"))
            })?;
            let staged_text = Self::apply_stage_hunks(&base_text, hunks)?;
            let bytes = staged_text.as_bytes();
            let mode = entries
                .get(path)
                .map(|entry| entry.mode)
                .unwrap_or(0o100644);
            let oid = self.write_loose_object(&git_dir, "blob", bytes)?;
            entries.insert(
                path.clone(),
                IndexEntry {
                    mode,
                    oid,
                    path: path.clone(),
                    size: bytes.len() as u32,
                },
            );
        }
        write_index(&git_dir, &entries)?;
        Ok(GitMutationResult {
            success: true,
            status: "success".to_string(),
            action: "stage_hunks".to_string(),
            new_commit_oid: None,
            undo_checkpoint_id: None,
            message: format!("Staged hunks for {} file(s)", hunks_by_path.len()),
            error: None,
        })
    }

    fn apply_stage_hunks(base_text: &str, hunks: &[StageHunk]) -> Result<String, GitError> {
        let base_lines = base_text
            .lines()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let trailing_newline = base_text.ends_with('\n');
        let mut ordered = hunks.to_vec();
        ordered.sort_by_key(|hunk| (hunk.old_start, hunk.new_start));
        let mut cursor = 0usize;
        let mut output = Vec::new();

        for hunk in &ordered {
            let start = hunk.old_start.saturating_sub(1);
            if start < cursor || start > base_lines.len() {
                return Err(GitError::Malformed(
                    "ERR-GIT-209: hunk does not match the staged base".to_string(),
                ));
            }
            output.extend(base_lines[cursor..start].iter().cloned());
            cursor = start;
            let mut old_seen = 0usize;
            let mut new_seen = 0usize;
            for line in &hunk.lines {
                if line.is_empty() {
                    return Err(GitError::Malformed(
                        "ERR-GIT-209: invalid empty hunk line".to_string(),
                    ));
                }
                let (marker, content) = line.split_at(1);
                match marker {
                    " " => {
                        if base_lines.get(cursor).map(String::as_str) != Some(content) {
                            return Err(GitError::Malformed(
                                "ERR-GIT-209: hunk context does not match staged base".to_string(),
                            ));
                        }
                        output.push(content.to_string());
                        cursor += 1;
                        old_seen += 1;
                        new_seen += 1;
                    }
                    "-" => {
                        if base_lines.get(cursor).map(String::as_str) != Some(content) {
                            return Err(GitError::Malformed(
                                "ERR-GIT-209: hunk deletion does not match staged base".to_string(),
                            ));
                        }
                        cursor += 1;
                        old_seen += 1;
                    }
                    "+" => {
                        output.push(content.to_string());
                        new_seen += 1;
                    }
                    _ => {
                        return Err(GitError::Malformed(
                            "ERR-GIT-209: hunk lines must begin with space, plus, or minus"
                                .to_string(),
                        ));
                    }
                }
            }
            if old_seen != hunk.old_lines || new_seen != hunk.new_lines {
                return Err(GitError::Malformed(
                    "ERR-GIT-209: hunk line counts do not match header".to_string(),
                ));
            }
        }
        output.extend(base_lines[cursor..].iter().cloned());
        let mut result = output.join("\n");
        if trailing_newline {
            result.push('\n');
        }
        Ok(result)
    }

    /// Unstage specific files from pre-commit verification.
    pub fn unstage(&self, paths: &[String]) -> Result<GitMutationResult, GitError> {
        let git_dir = self.git_dir()?;
        let mut store = GitObjectStore::discover(&self.repo_root)?;
        let mut entries = read_index(&git_dir)?;
        let head_entries = self.head_index_entries(&mut store)?;
        for path in paths {
            if !is_safe_index_path(path) {
                return Err(GitError::Malformed(format!(
                    "unsafe repository-relative unstage path: {path}"
                )));
            }
            if let Some(entry) = head_entries.get(path) {
                entries.insert(path.clone(), entry.clone());
            } else {
                entries.remove(path);
            }
        }
        write_index(&git_dir, &entries)?;

        Ok(GitMutationResult {
            success: true,
            status: "success".to_string(),
            action: "unstage".to_string(),
            new_commit_oid: None,
            undo_checkpoint_id: None,
            message: format!("Unstaged {} file(s)", paths.len()),
            error: None,
        })
    }

    /// Unstage all files.
    pub fn unstage_all(&self) -> Result<GitMutationResult, GitError> {
        let git_dir = self.git_dir()?;
        let mut store = GitObjectStore::discover(&self.repo_root)?;
        let entries = self.head_index_entries(&mut store)?;
        write_index(&git_dir, &entries)?;
        Ok(GitMutationResult {
            success: true,
            status: "success".to_string(),
            action: "unstage_all".to_string(),
            new_commit_oid: None,
            undo_checkpoint_id: None,
            message: "Unstaged all files".to_string(),
            error: None,
        })
    }

    /// Tag management (create or delete tags).
    pub fn tag(&self, req: &TagRequest) -> Result<GitMutationResult, GitError> {
        let git_dir = self.git_dir()?;
        let tags_dir = git_dir.join("refs").join("tags");
        fs::create_dir_all(&tags_dir).map_err(|e| GitError::Io {
            path: tags_dir.clone(),
            source: e,
        })?;

        let clean_name = req.tag_name.trim().trim_start_matches("refs/tags/");
        Self::validate_ref_name(clean_name, "tag")?;
        let tag_file = tags_dir.join(clean_name);
        let tag_ref_name = format!("refs/tags/{clean_name}");
        let previous_tag_oid = fs::read_to_string(&tag_file)
            .ok()
            .and_then(|value| ObjectId::from_hex(value.trim()).ok());

        if req.action.as_deref() == Some("delete") {
            let undo_id = if let Some(head_oid) =
                GitObjectStore::discover(&self.repo_root)?.resolve_ref("HEAD")?
            {
                Some(self.create_undo_checkpoint_with_branch(
                    &git_dir,
                    head_oid,
                    Some((&tag_ref_name, previous_tag_oid)),
                )?)
            } else {
                None
            };
            if tag_file.exists() {
                let _ = fs::remove_file(&tag_file);
            }
            return Ok(GitMutationResult {
                success: true,
                status: "success".to_string(),
                action: "tag_delete".to_string(),
                new_commit_oid: None,
                undo_checkpoint_id: undo_id,
                message: format!("Deleted tag '{clean_name}'"),
                error: None,
            });
        }

        let mut store = GitObjectStore::discover(&self.repo_root)?;
        let undo_id = if let Some(head_oid) = store.resolve_ref("HEAD")? {
            Some(self.create_undo_checkpoint_with_branch(
                &git_dir,
                head_oid,
                Some((&tag_ref_name, previous_tag_oid)),
            )?)
        } else {
            None
        };
        let target_oid_hex = if let Some(ref o) = req.target_oid {
            o.clone()
        } else if let Some(head_oid) = store.resolve_ref("HEAD")? {
            head_oid.to_hex()
        } else {
            return Ok(GitMutationResult {
                success: false,
                status: "refused".to_string(),
                action: "tag_create".to_string(),
                new_commit_oid: None,
                undo_checkpoint_id: None,
                message: "Cannot create tag on empty repository with no commits".to_string(),
                error: Some("ERR-GIT-TAG-001: No target commit for tag".to_string()),
            });
        };
        let target_oid = ObjectId::from_hex(&target_oid_hex).map_err(|_| {
            GitError::Malformed(format!(
                "ERR-GIT-TAG-002: invalid target object '{target_oid_hex}'"
            ))
        })?;
        if !matches!(
            store.object(target_oid)?,
            autopsy_temporal::git::GitObject::Commit(_)
        ) {
            return Err(GitError::Malformed(format!(
                "ERR-GIT-TAG-003: target '{target_oid_hex}' is not a commit"
            )));
        }

        fs::write(&tag_file, format!("{target_oid_hex}\n")).map_err(|e| GitError::Io {
            path: tag_file,
            source: e,
        })?;

        Ok(GitMutationResult {
            success: true,
            status: "success".to_string(),
            action: "tag_create".to_string(),
            new_commit_oid: Some(target_oid_hex.clone()),
            undo_checkpoint_id: undo_id,
            message: format!(
                "Created tag '{clean_name}' pointing to {}",
                &target_oid_hex[..7.min(target_oid_hex.len())]
            ),
            error: None,
        })
    }

    /// Guarded merge of source branch into active branch with undo checkpoint.
    pub fn merge(&self, req: &MergeRequest) -> Result<GitMutationResult, GitError> {
        let git_dir = self.git_dir()?;
        let mut store = GitObjectStore::discover(&self.repo_root)?;

        let head_oid = match store.resolve_ref("HEAD")? {
            Some(o) => o,
            None => {
                return Ok(GitMutationResult {
                    success: false,
                    status: "refused".to_string(),
                    action: "merge".to_string(),
                    new_commit_oid: None,
                    undo_checkpoint_id: None,
                    message: "Cannot merge into repository without commits".to_string(),
                    error: Some("ERR-GIT-MERGE-001: HEAD commit not found".to_string()),
                });
            }
        };

        let clean_source = req.source_branch.trim().trim_start_matches("refs/heads/");
        let source_ref_path = git_dir.join("refs").join("heads").join(clean_source);
        if !source_ref_path.exists() {
            return Ok(GitMutationResult {
                success: false,
                status: "refused".to_string(),
                action: "merge".to_string(),
                new_commit_oid: None,
                undo_checkpoint_id: None,
                message: format!("Source branch '{clean_source}' does not exist"),
                error: Some("ERR-GIT-MERGE-002: Source branch missing".to_string()),
            });
        }

        let source_oid_hex = fs::read_to_string(&source_ref_path)
            .unwrap_or_default()
            .trim()
            .to_string();
        let source_oid = ObjectId::from_hex(&source_oid_hex)
            .map_err(|e| GitError::Malformed(format!("Invalid branch commit OID: {e}")))?;

        if head_oid == source_oid {
            return Ok(GitMutationResult {
                success: true,
                status: "success".to_string(),
                action: "merge".to_string(),
                new_commit_oid: Some(head_oid.to_hex()),
                undo_checkpoint_id: None,
                message: "Already up to date (commits are identical)".to_string(),
                error: None,
            });
        }

        // Snapshot state for 1-click Undo
        let undo_id = self.create_undo_checkpoint(&git_dir, head_oid)?;

        // Create merge commit with 2 parents: HEAD and source_oid
        self.ensure_clean_worktree()?;
        let head_commit = store.commit(head_oid)?;
        let source_commit = store.commit(source_oid)?;
        let head_tree = flatten_tree(&store, head_commit.tree)?;
        let source_tree = flatten_tree(&store, source_commit.tree)?;
        let mut merged_tree = head_tree.clone();
        for (path, source_entry) in source_tree {
            if let Some(head_entry) = merged_tree.get(&path)
                && head_entry.1 != source_entry.1
            {
                return Err(GitError::Malformed(format!(
                    "ERR-GIT-MERGE-003: merge conflict at {path}"
                )));
            }
            merged_tree.insert(path, source_entry);
        }
        let tree_oid = self.build_git_tree(&git_dir, &merged_tree)?;
        let (merge_verdict, merge_summary, merge_digest) =
            self.semantic_preflight(&store, head_commit.tree, tree_oid)?;

        let snap_digest = blake3::hash(
            format!(
                "{}_{}_{}",
                tree_oid.to_hex(),
                head_oid.to_hex(),
                source_oid.to_hex()
            )
            .as_bytes(),
        )
        .to_hex()[..16]
            .to_string();

        let now = head_commit.committer_timestamp().unwrap_or(0);
        let full_message = format!(
            "Merge branch '{}'\n\nSynevid-Snapshot: blake3:{}\nSynevid-Diff: blake3:{}\nSynevid-Verdict: {} ({})\n",
            clean_source, snap_digest, merge_digest, merge_verdict, merge_summary
        );

        let commit_content = format!(
            "tree {}\nparent {}\nparent {}\nauthor SynEvid Engineer <engineer@synevid.local> {} +0000\ncommitter SynEvid Engineer <engineer@synevid.local> {} +0000\n\n{}",
            tree_oid.to_hex(),
            head_oid.to_hex(),
            source_oid.to_hex(),
            now,
            now,
            full_message
        );

        let new_commit_oid =
            self.write_loose_object(&git_dir, "commit", commit_content.as_bytes())?;

        // Update active branch ref
        let head_content = fs::read_to_string(git_dir.join("HEAD")).unwrap_or_default();
        let target_ref_path = if let Some(branch_ref) = head_content.trim().strip_prefix("ref: ") {
            git_dir.join(branch_ref.trim())
        } else {
            git_dir.join("HEAD")
        };

        self.checkout_tree(&git_dir, &mut store, tree_oid)?;
        fs::write(&target_ref_path, format!("{}\n", new_commit_oid.to_hex())).map_err(|e| {
            GitError::Io {
                path: target_ref_path,
                source: e,
            }
        })?;

        Ok(GitMutationResult {
            success: true,
            status: "success".to_string(),
            action: "merge".to_string(),
            new_commit_oid: Some(new_commit_oid.to_hex()),
            undo_checkpoint_id: Some(undo_id),
            message: format!("Merged branch '{clean_source}' into active branch"),
            error: None,
        })
    }

    /// Checkout commit or branch (detach or switch) with undo safety ref.
    pub fn checkout(&self, req: &CheckoutRequest) -> Result<GitMutationResult, GitError> {
        let git_dir = self.git_dir()?;
        let mut store = GitObjectStore::discover(&self.repo_root)?;
        self.ensure_clean_worktree()?;
        let head_oid_opt = store.resolve_ref("HEAD")?;
        let target_str = req.target.trim();
        if ObjectId::from_hex(target_str).is_err() {
            Self::validate_ref_name(target_str, "checkout")?;
        }
        let branch_path = git_dir.join("refs").join("heads").join(target_str);
        let (target_oid, branch_ref, action, message) =
            if branch_path.exists() && req.detach != Some(true) {
                let hex = fs::read_to_string(&branch_path)
                    .map_err(|e| GitError::Io {
                        path: branch_path.clone(),
                        source: e,
                    })?
                    .trim()
                    .to_string();
                (
                    ObjectId::from_hex(&hex)
                        .map_err(|e| GitError::Malformed(format!("Invalid branch OID: {e}")))?,
                    Some(format!("refs/heads/{target_str}")),
                    "checkout_branch",
                    format!("Switched to branch '{target_str}'"),
                )
            } else {
                let target_oid = crate::reader::resolve_revision(&mut store, &git_dir, target_str)?;
                (
                    target_oid,
                    None,
                    "checkout_commit",
                    format!(
                        "Detached HEAD at commit {}",
                        &target_oid.to_hex()[..7.min(target_oid.to_hex().len())]
                    ),
                )
            };

        let target_commit = store.commit(target_oid)?;
        let undo_id = if let Some(head_oid) = head_oid_opt {
            Some(self.create_undo_checkpoint(&git_dir, head_oid)?)
        } else {
            None
        };
        self.checkout_tree(&git_dir, &mut store, target_commit.tree)?;
        self.update_head(&git_dir, target_oid, branch_ref.as_deref())?;

        Ok(GitMutationResult {
            success: true,
            status: "success".to_string(),
            action: action.to_string(),
            new_commit_oid: Some(target_oid.to_hex()),
            undo_checkpoint_id: undo_id,
            message,
            error: None,
        })
    }

    /// Revert historical commit with pre-flight check and undo checkpoint.
    pub fn revert(&self, req: &RevertRequest) -> Result<GitMutationResult, GitError> {
        let git_dir = self.git_dir()?;
        let mut store = GitObjectStore::discover(&self.repo_root)?;
        self.ensure_clean_worktree()?;

        let head_oid = match store.resolve_ref("HEAD")? {
            Some(o) => o,
            None => {
                return Ok(GitMutationResult {
                    success: false,
                    status: "refused".to_string(),
                    action: "revert".to_string(),
                    new_commit_oid: None,
                    undo_checkpoint_id: None,
                    message: "Cannot revert in empty repository".to_string(),
                    error: Some("ERR-GIT-REVERT-001: Empty repository".to_string()),
                });
            }
        };

        let target_oid = ObjectId::from_hex(req.commit_oid.trim())
            .map_err(|e| GitError::Malformed(format!("Invalid target commit OID: {e}")))?;

        let target_commit = store.commit(target_oid)?;
        let head_commit = store.commit(head_oid)?;
        let current_tree = flatten_tree(&store, head_commit.tree)?;
        let target_tree = flatten_tree(&store, target_commit.tree)?;
        let parent_tree = if let Some(parent_oid) = target_commit.parents.first() {
            let parent = store.commit(*parent_oid)?;
            flatten_tree(&store, parent.tree)?
        } else {
            BTreeMap::new()
        };

        // Apply the inverse patch only when the current tree still contains
        // the target side of each change. Divergence is refused explicitly;
        // silently manufacturing a bad revert would violate coverage honesty.
        let mut reverted_tree = current_tree.clone();
        let mut paths = BTreeMap::new();
        paths.extend(parent_tree.keys().cloned().map(|path| (path, ())));
        paths.extend(target_tree.keys().cloned().map(|path| (path, ())));
        for path in paths.keys() {
            let before = parent_tree.get(path);
            let target = target_tree.get(path);
            if before == target {
                continue;
            }
            let current = current_tree.get(path);
            if current == target {
                if let Some((mode, oid)) = before {
                    reverted_tree.insert(path.clone(), (*mode, *oid));
                } else {
                    reverted_tree.remove(path);
                }
            } else if current != before {
                return Ok(GitMutationResult {
                    success: false,
                    status: "conflict".to_string(),
                    action: "revert".to_string(),
                    new_commit_oid: None,
                    undo_checkpoint_id: None,
                    message: format!("Revert conflicts with current changes in '{path}'"),
                    error: Some(
                        "ERR-GIT-208: Revert conflict requires manual resolution".to_string(),
                    ),
                });
            }
        }
        let tree_oid = self.build_git_tree(&git_dir, &reverted_tree)?;
        let (revert_verdict, revert_summary, revert_digest) =
            self.semantic_preflight(&store, head_commit.tree, tree_oid)?;
        let undo_id = self.create_undo_checkpoint(&git_dir, head_oid)?;

        let snap_digest = blake3::hash(
            format!("{}_revert_{}", tree_oid.to_hex(), target_oid.to_hex()).as_bytes(),
        )
        .to_hex()[..16]
            .to_string();

        let now = head_commit.committer_timestamp().unwrap_or(0);
        let full_message = format!(
            "Revert \"{}\"\n\nThis reverts commit {}.\n\nSynevid-Snapshot: blake3:{}\nSynevid-Diff: blake3:{}\nSynevid-Verdict: {} ({})\n",
            target_commit.summary(),
            target_oid.to_hex(),
            snap_digest,
            revert_digest,
            revert_verdict,
            revert_summary
        );

        let commit_content = format!(
            "tree {}\nparent {}\nauthor SynEvid Engineer <engineer@synevid.local> {} +0000\ncommitter SynEvid Engineer <engineer@synevid.local> {} +0000\n\n{}",
            tree_oid.to_hex(),
            head_oid.to_hex(),
            now,
            now,
            full_message
        );

        let new_commit_oid =
            self.write_loose_object(&git_dir, "commit", commit_content.as_bytes())?;

        // Update active branch ref
        let head_content = fs::read_to_string(git_dir.join("HEAD")).unwrap_or_default();
        let target_ref_path = if let Some(branch_ref) = head_content.trim().strip_prefix("ref: ") {
            git_dir.join(branch_ref.trim())
        } else {
            git_dir.join("HEAD")
        };

        self.checkout_tree(&git_dir, &mut store, tree_oid)?;
        fs::write(&target_ref_path, format!("{}\n", new_commit_oid.to_hex())).map_err(|e| {
            GitError::Io {
                path: target_ref_path,
                source: e,
            }
        })?;

        Ok(GitMutationResult {
            success: true,
            status: "success".to_string(),
            action: "revert".to_string(),
            new_commit_oid: Some(new_commit_oid.to_hex()),
            undo_checkpoint_id: Some(undo_id),
            message: format!(
                "Reverted commit {}",
                &target_oid.to_hex()[..7.min(target_oid.to_hex().len())]
            ),
            error: None,
        })
    }

    /// Autopsy Bisect Assistant (FR-P3-007): Scans commit range to pinpoint breaking commit.
    pub fn bisect(&self, req: &BisectRequest) -> Result<BisectReport, GitError> {
        let mut store = GitObjectStore::discover(&self.repo_root)?;
        let git_dir = store.git_dir().to_path_buf();
        let start_oid = crate::reader::resolve_revision(&mut store, &git_dir, &req.start_oid)?;
        let end_oid = crate::reader::resolve_revision(&mut store, &git_dir, &req.end_oid)?;

        // Build the first-parent range and require the requested good commit to
        // be an ancestor of the requested end commit.
        let mut newest_first = Vec::new();
        let mut cursor = end_oid;
        let mut visited = std::collections::BTreeSet::new();
        while visited.insert(cursor) {
            let commit = store.commit(cursor)?;
            let reached_start = cursor == start_oid;
            let parent = commit.parents.first().copied();
            newest_first.push((cursor, commit));
            if reached_start {
                break;
            }
            cursor = parent.ok_or_else(|| {
                GitError::Malformed("bisect start is not an ancestor of end".to_string())
            })?;
        }
        if newest_first.last().map(|(oid, _)| *oid) != Some(start_oid) {
            return Err(GitError::Malformed(
                "bisect start is not an ancestor of end".to_string(),
            ));
        }
        newest_first.reverse();
        let total_scanned = newest_first.len();
        if total_scanned <= 1 {
            return Ok(BisectReport {
                breaking_commit_oid: None,
                breaking_summary: "No invariant transition in an empty commit range".to_string(),
                breaking_author: String::new(),
                breaking_timestamp: 0,
                verdict: "PASS".to_string(),
                drifted_symbols: Vec::new(),
                commits_evaluated: 0,
                total_scanned,
            });
        }

        let rule_id = req.rule_id.as_deref().unwrap_or("contract_transition");
        let mut evaluations: BTreeMap<usize, BisectEvaluation> = BTreeMap::new();
        let mut evaluate = |index: usize| -> Result<BisectEvaluation, GitError> {
            if let std::collections::btree_map::Entry::Vacant(entry) = evaluations.entry(index) {
                let (oid, commit) = &newest_first[index];
                let parent = newest_first[index - 1].0;
                let result = self.evaluate_bisect_commit(&store, parent, *oid, commit, rule_id)?;
                entry.insert(result);
            }
            Ok(evaluations
                .get(&index)
                .expect("bisect evaluation inserted")
                .clone())
        };

        let final_index = total_scanned - 1;
        let final_evaluation = evaluate(final_index)?;
        if !final_evaluation.is_bad {
            let verdict = if final_evaluation.unknown {
                "UNKNOWN"
            } else {
                "PASS"
            };
            return Ok(BisectReport {
                breaking_commit_oid: None,
                breaking_summary: "No invariant transition detected in commit range".to_string(),
                breaking_author: String::new(),
                breaking_timestamp: 0,
                verdict: verdict.to_string(),
                drifted_symbols: final_evaluation.drifted_symbols,
                commits_evaluated: evaluations.len(),
                total_scanned,
            });
        }

        // Binary search the first semantic failure. The predicate is the
        // invariant/contract result, never a commit-message marker.
        let mut low = 1usize;
        let mut high = final_index;
        while low < high {
            let midpoint = low + (high - low) / 2;
            if evaluate(midpoint)?.is_bad {
                high = midpoint;
            } else {
                low = midpoint + 1;
            }
        }
        let breaking = evaluate(low)?;
        let (_, commit) = &newest_first[low];
        Ok(BisectReport {
            breaking_commit_oid: Some(commit.oid.to_hex()),
            breaking_summary: commit.summary().to_string(),
            breaking_author: commit.author.clone(),
            breaking_timestamp: commit.committer_timestamp().unwrap_or(0),
            verdict: if breaking.is_bad {
                "FAIL".to_string()
            } else {
                "UNKNOWN".to_string()
            },
            drifted_symbols: breaking.drifted_symbols,
            commits_evaluated: evaluations.len(),
            total_scanned,
        })
    }

    fn evaluate_bisect_commit(
        &self,
        store: &GitObjectStore,
        parent_oid: ObjectId,
        commit_oid: ObjectId,
        commit: &autopsy_temporal::git::Commit,
        rule_id: &str,
    ) -> Result<BisectEvaluation, GitError> {
        let parent = store.commit(parent_oid)?;
        let before = semantic::analyze_tree(
            &self.repo_root,
            store,
            parent.tree,
            &format!("git:{}", parent_oid.to_hex()),
        )?;
        let after = semantic::analyze_tree(
            &self.repo_root,
            store,
            commit.tree,
            &format!("git:{}", commit_oid.to_hex()),
        )?;
        let diff = DiffEngine::compute_diff(DiffInput {
            before_snapshot_id: before.snapshot_id.clone(),
            after_snapshot_id: after.snapshot_id.clone(),
            before_files: &before.files,
            after_files: &after.files,
            before_graph: &before.graph,
            after_graph: &after.graph,
            before_contracts: &before.contracts,
            after_contracts: &after.contracts,
        });
        let unknown = before.coverage_state != autopsy_domain::CoverageState::Verified
            || after.coverage_state != autopsy_domain::CoverageState::Verified;
        let contract_failure = diff.summary.breaking_contract_changes > 0;
        let coverage_failure = unknown && rule_id == "coverage";
        let is_bad = contract_failure || coverage_failure;
        let mut drifted_symbols = diff
            .frontier
            .direct_symbols
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        drifted_symbols.extend(
            diff.frontier
                .impacted_boundary
                .iter()
                .map(ToString::to_string),
        );
        drifted_symbols.sort();
        drifted_symbols.dedup();
        Ok(BisectEvaluation {
            is_bad,
            unknown,
            drifted_symbols,
        })
    }

    /// Convenience wrapper for `CommitRequest`.
    pub fn commit_request(&self, req: &CommitRequest) -> Result<GitMutationResult, GitError> {
        self.commit(
            &req.message,
            req.author_name.as_deref(),
            req.author_email.as_deref(),
        )
    }

    /// Convenience wrapper for `BranchRequest`.
    pub fn branch_request(&self, req: &BranchRequest) -> Result<GitMutationResult, GitError> {
        self.branch(&req.branch_name, &req.action)
    }

    /// Pre-flight verify and create commit with Synevid trailers & undo safety checkpoint.
    pub fn commit(
        &self,
        message: &str,
        author_name: Option<&str>,
        author_email: Option<&str>,
    ) -> Result<GitMutationResult, GitError> {
        let git_dir = self.git_dir()?;
        let mut store = GitObjectStore::discover(&self.repo_root)?;

        // 1. Get current HEAD commit OID if present (supports initial root commit)
        let head_oid_opt = store.resolve_ref("HEAD")?;

        // 2. Snapshot current state into refs/autopsy/undo/<id> if HEAD exists
        let undo_id = if let Some(head_oid) = head_oid_opt {
            self.create_undo_checkpoint(&git_dir, head_oid)?
        } else {
            "initial_root".to_string()
        };

        // 3. Resolve active branch ref path (e.g. refs/heads/main)
        let head_content = fs::read_to_string(git_dir.join("HEAD")).unwrap_or_default();
        let target_ref_path = if let Some(branch_ref) = head_content.trim().strip_prefix("ref: ") {
            git_dir.join(branch_ref.trim())
        } else {
            git_dir.join("HEAD")
        };

        // 4. Construct the commit tree from the real Git index. An absent
        // index retains the historical empty-root behavior for compatibility;
        // once staging is used, the index is the only source of commit files.
        let has_index = index_path(&git_dir).exists();
        let index_entries = read_index(&git_dir)?;
        let tree_oid = if has_index {
            let file_map: BTreeMap<String, (u32, ObjectId)> = index_entries
                .values()
                .map(|entry| (entry.path.clone(), (entry.mode, entry.oid)))
                .collect();
            self.build_git_tree(&git_dir, &file_map)?
        } else if let Some(head_oid) = head_oid_opt {
            let parent_commit = store.commit(head_oid)?;
            parent_commit.tree
        } else {
            self.write_loose_object(&git_dir, "tree", b"")?
        };

        // 5. Run the same semantic diff used by the public range-diff API
        // before writing the commit. The result is recorded in trailers so a
        // commit never claims PASS merely because the mutation succeeded.
        let (preflight_verdict, preflight_summary, preflight_digest) =
            if let Some(head_oid) = head_oid_opt {
                let parent_commit = store.commit(head_oid)?;
                match (
                    semantic::analyze_tree(
                        &self.repo_root,
                        &store,
                        parent_commit.tree,
                        &format!("git:{}", head_oid.to_hex()),
                    ),
                    semantic::analyze_tree(
                        &self.repo_root,
                        &store,
                        tree_oid,
                        &format!("git:staged:{}", tree_oid.to_hex()),
                    ),
                ) {
                    (Ok(before), Ok(after)) => {
                        let diff = DiffEngine::compute_diff(DiffInput {
                            before_snapshot_id: before.snapshot_id.clone(),
                            after_snapshot_id: after.snapshot_id.clone(),
                            before_files: &before.files,
                            after_files: &after.files,
                            before_graph: &before.graph,
                            after_graph: &after.graph,
                            before_contracts: &before.contracts,
                            after_contracts: &after.contracts,
                        });
                        let verdict = if before.coverage_state
                            != autopsy_domain::CoverageState::Verified
                            || after.coverage_state != autopsy_domain::CoverageState::Verified
                        {
                            "UNKNOWN"
                        } else if diff.summary.breaking_contract_changes > 0 {
                            "WARN"
                        } else {
                            "PASS"
                        };
                        (
                            verdict.to_string(),
                            format!(
                                "{} broken contracts; {} impacted symbols",
                                diff.summary.breaking_contract_changes,
                                diff.frontier.impacted_boundary.len()
                                    + diff.frontier.direct_symbols.len()
                            ),
                            diff.diff_digest,
                        )
                    }
                    _ => (
                        "UNKNOWN".to_string(),
                        "semantic preflight could not verify the staged tree".to_string(),
                        blake3::hash(tree_oid.to_hex().as_bytes())
                            .to_hex()
                            .to_string(),
                    ),
                }
            } else {
                (
                    "PASS".to_string(),
                    "initial repository commit".to_string(),
                    blake3::hash(tree_oid.to_hex().as_bytes())
                        .to_hex()
                        .to_string(),
                )
            };

        // 6. Build Synevid verified commit message with deterministic trailers.
        let snap_digest = blake3::hash(
            format!(
                "{}_{}",
                tree_oid.to_hex(),
                head_oid_opt.map(|o| o.to_hex()).unwrap_or_default()
            )
            .as_bytes(),
        )
        .to_hex()[..16]
            .to_string();

        let author = author_name.unwrap_or("SynEvid Engineer");
        let email = author_email.unwrap_or("engineer@synevid.local");
        let now = head_oid_opt
            .and_then(|oid| store.commit(oid).ok())
            .and_then(|commit| commit.committer_timestamp())
            .unwrap_or(0);

        let full_message = format!(
            "{}\n\nSynevid-Snapshot: blake3:{}\nSynevid-Diff: blake3:{}\nSynevid-Verdict: {} ({})\n",
            message.trim(),
            snap_digest,
            preflight_digest,
            preflight_verdict,
            preflight_summary,
        );

        let parent_line = if let Some(head_oid) = head_oid_opt {
            format!("parent {}\n", head_oid.to_hex())
        } else {
            String::new()
        };

        let commit_content = format!(
            "tree {}\n{}author {} <{}> {} +0000\ncommitter {} <{}> {} +0000\n\n{}",
            tree_oid.to_hex(),
            parent_line,
            author,
            email,
            now,
            author,
            email,
            now,
            full_message
        );

        // 7. Write loose git object to .git/objects/xx/yyyy...
        let new_oid = self.write_loose_object(&git_dir, "commit", commit_content.as_bytes())?;

        // 8. Atomically advance branch ref or HEAD
        if let Some(parent_dir) = target_ref_path.parent() {
            let _ = fs::create_dir_all(parent_dir);
        }
        fs::write(&target_ref_path, format!("{}\n", new_oid.to_hex())).map_err(|e| {
            GitError::Io {
                path: target_ref_path,
                source: e,
            }
        })?;

        if has_index {
            write_index(&git_dir, &index_entries)?;
        }
        let _ = fs::remove_file(self.repo_root.join(".autopsy").join("staged_files.json"));

        Ok(GitMutationResult {
            success: true,
            status: "success".to_string(),
            action: "commit".to_string(),
            new_commit_oid: Some(new_oid.to_hex()),
            undo_checkpoint_id: Some(undo_id),
            message: format!("Created verified commit {}", &new_oid.to_hex()[..7]),
            error: None,
        })
    }

    /// Manage local branches (create, switch, delete) with conflict guard.
    pub fn branch(&self, branch_name: &str, action: &str) -> Result<GitMutationResult, GitError> {
        let git_dir = self.git_dir()?;
        let mut store = GitObjectStore::discover(&self.repo_root)?;
        let head_oid = store
            .resolve_ref("HEAD")?
            .ok_or_else(|| GitError::Malformed("HEAD does not resolve".into()))?;

        let clean_name = branch_name.trim_start_matches("refs/heads/");
        Self::validate_ref_name(clean_name, "branch")?;
        let branch_path = git_dir.join("refs").join("heads").join(clean_name);

        match action {
            "create" => {
                if branch_path.exists() {
                    return Ok(GitMutationResult {
                        success: false,
                        status: "refused".to_string(),
                        action: "branch_create".to_string(),
                        new_commit_oid: None,
                        undo_checkpoint_id: None,
                        message: format!("Branch '{clean_name}' already exists"),
                        error: Some("ERR-GIT-203: Branch already exists".to_string()),
                    });
                }
                let branch_ref_name = format!("refs/heads/{clean_name}");
                let undo_id = self.create_undo_checkpoint_with_branch(
                    &git_dir,
                    head_oid,
                    Some((&branch_ref_name, None)),
                )?;
                if let Some(p) = branch_path.parent() {
                    let _ = fs::create_dir_all(p);
                }
                fs::write(&branch_path, format!("{}\n", head_oid.to_hex())).map_err(|e| {
                    GitError::Io {
                        path: branch_path,
                        source: e,
                    }
                })?;
                Ok(GitMutationResult {
                    success: true,
                    status: "success".to_string(),
                    action: "branch_create".to_string(),
                    new_commit_oid: Some(head_oid.to_hex()),
                    undo_checkpoint_id: Some(undo_id),
                    message: format!(
                        "Created branch '{clean_name}' at {}",
                        &head_oid.to_hex()[..7]
                    ),
                    error: None,
                })
            }
            "switch" => {
                if !branch_path.exists() {
                    return Ok(GitMutationResult {
                        success: false,
                        status: "refused".to_string(),
                        action: "branch_switch".to_string(),
                        new_commit_oid: None,
                        undo_checkpoint_id: None,
                        message: "Branch does not exist".to_string(),
                        error: Some(format!("ERR-GIT-204: Branch '{clean_name}' not found")),
                    });
                }
                self.ensure_clean_worktree()?;
                let target_hex = fs::read_to_string(&branch_path)
                    .map_err(|e| GitError::Io {
                        path: branch_path.clone(),
                        source: e,
                    })?
                    .trim()
                    .to_string();
                let target_oid = ObjectId::from_hex(&target_hex)
                    .map_err(|e| GitError::Malformed(format!("Invalid branch OID: {e}")))?;
                let target_commit = store.commit(target_oid)?;
                let branch_ref_name = format!("refs/heads/{clean_name}");
                let previous_branch_oid = fs::read_to_string(&branch_path)
                    .ok()
                    .and_then(|value| ObjectId::from_hex(value.trim()).ok());
                let undo_id = self.create_undo_checkpoint_with_branch(
                    &git_dir,
                    head_oid,
                    Some((&branch_ref_name, previous_branch_oid)),
                )?;
                self.checkout_tree(&git_dir, &mut store, target_commit.tree)?;
                self.update_head(
                    &git_dir,
                    target_oid,
                    Some(&format!("refs/heads/{clean_name}")),
                )?;
                Ok(GitMutationResult {
                    success: true,
                    status: "success".to_string(),
                    action: "branch_switch".to_string(),
                    new_commit_oid: Some(target_oid.to_hex()),
                    undo_checkpoint_id: Some(undo_id),
                    message: format!("Switched to branch '{clean_name}'"),
                    error: None,
                })
            }
            "delete" => {
                let head_content = fs::read_to_string(git_dir.join("HEAD")).unwrap_or_default();
                if head_content.trim() == format!("ref: refs/heads/{clean_name}") {
                    return Ok(GitMutationResult {
                        success: false,
                        status: "refused".to_string(),
                        action: "branch_delete".to_string(),
                        new_commit_oid: None,
                        undo_checkpoint_id: None,
                        message: "Cannot delete the active branch".to_string(),
                        error: Some("ERR-GIT-207: Active branch deletion refused".to_string()),
                    });
                }
                let undo_id = self.create_undo_checkpoint(&git_dir, head_oid)?;
                if branch_path.exists() {
                    let _ = fs::remove_file(&branch_path);
                }
                Ok(GitMutationResult {
                    success: true,
                    status: "success".to_string(),
                    action: "branch_delete".to_string(),
                    new_commit_oid: None,
                    undo_checkpoint_id: Some(undo_id),
                    message: format!("Deleted branch '{clean_name}'"),
                    error: None,
                })
            }
            _ => Ok(GitMutationResult {
                success: false,
                status: "refused".to_string(),
                action: action.to_string(),
                new_commit_oid: None,
                undo_checkpoint_id: None,
                message: format!("Unsupported action '{action}'"),
                error: Some("ERR-GIT-205: Invalid branch action".to_string()),
            }),
        }
    }

    /// Instantaneous rollback using refs/autopsy/undo/<id> safety stack (Cmd+Z).
    pub fn undo(&self) -> Result<GitMutationResult, GitError> {
        let git_dir = self.git_dir()?;
        let undo_dir = git_dir.join("refs").join("autopsy").join("undo");

        if !undo_dir.is_dir() {
            return Ok(GitMutationResult {
                success: false,
                status: "refused".to_string(),
                action: "undo".to_string(),
                new_commit_oid: None,
                undo_checkpoint_id: None,
                message: "No undo checkpoints available".to_string(),
                error: Some("ERR-GIT-206: Undo stack is empty".to_string()),
            });
        }

        let mut checkpoints: Vec<PathBuf> = fs::read_dir(&undo_dir)
            .map_err(|e| GitError::Io {
                path: undo_dir.clone(),
                source: e,
            })?
            .filter_map(|e| e.ok().map(|ent| ent.path()))
            .filter(|p| p.is_file() && p.extension().is_none_or(|ext| ext != "index"))
            .collect();

        checkpoints.sort();

        let latest = match checkpoints.pop() {
            Some(p) => p,
            None => {
                return Ok(GitMutationResult {
                    success: false,
                    status: "refused".to_string(),
                    action: "undo".to_string(),
                    new_commit_oid: None,
                    undo_checkpoint_id: None,
                    message: "No undo checkpoints available".to_string(),
                    error: Some("ERR-GIT-206: Undo stack is empty".to_string()),
                });
            }
        };

        let checkpoint_text = fs::read_to_string(&latest).unwrap_or_default();
        let checkpoint =
            serde_json::from_str::<UndoCheckpoint>(&checkpoint_text).unwrap_or_else(|_| {
                UndoCheckpoint {
                    target_oid: checkpoint_text.trim().to_string(),
                    head_ref: None,
                    branch_path: None,
                    branch_oid: None,
                }
            });
        let target_oid = ObjectId::from_hex(&checkpoint.target_oid)
            .map_err(|e| GitError::Malformed(format!("Invalid undo checkpoint: {e}")))?;
        let mut store = GitObjectStore::discover(&self.repo_root)?;
        let target_commit = store.commit(target_oid)?;
        self.checkout_tree(&git_dir, &mut store, target_commit.tree)?;
        self.update_head(&git_dir, target_oid, checkpoint.head_ref.as_deref())?;
        if let Some(branch_path) = checkpoint.branch_path.as_deref() {
            let branch_file = git_dir.join(branch_path);
            if let Some(branch_oid) = checkpoint.branch_oid {
                if let Some(parent) = branch_file.parent() {
                    fs::create_dir_all(parent).map_err(|e| GitError::Io {
                        path: parent.to_path_buf(),
                        source: e,
                    })?;
                }
                fs::write(&branch_file, format!("{branch_oid}\n")).map_err(|e| GitError::Io {
                    path: branch_file.clone(),
                    source: e,
                })?;
            } else if branch_file.exists() {
                fs::remove_file(&branch_file).map_err(|e| GitError::Io {
                    path: branch_file,
                    source: e,
                })?;
            }
        }

        let index_snapshot = latest.with_extension("index");
        let index_file = index_path(&git_dir);
        if index_snapshot.exists() {
            let bytes = fs::read(&index_snapshot).map_err(|e| GitError::Io {
                path: index_snapshot.clone(),
                source: e,
            })?;
            fs::write(&index_file, bytes).map_err(|e| GitError::Io {
                path: index_file,
                source: e,
            })?;
        } else if index_file.exists() {
            fs::remove_file(&index_file).map_err(|e| GitError::Io {
                path: index_file,
                source: e,
            })?;
        }

        let _ = fs::remove_file(latest);
        let _ = fs::remove_file(index_snapshot);

        Ok(GitMutationResult {
            success: true,
            status: "success".to_string(),
            action: "undo".to_string(),
            new_commit_oid: Some(target_oid.to_hex()),
            undo_checkpoint_id: None,
            message: format!(
                "Rolled back to checkpoint {}",
                &target_oid.to_hex()[..7.min(target_oid.to_hex().len())]
            ),
            error: None,
        })
    }

    fn create_undo_checkpoint(
        &self,
        git_dir: &Path,
        head_oid: ObjectId,
    ) -> Result<String, GitError> {
        self.create_undo_checkpoint_with_branch(git_dir, head_oid, None)
    }

    fn create_undo_checkpoint_with_branch(
        &self,
        git_dir: &Path,
        head_oid: ObjectId,
        branch_state: Option<(&str, Option<ObjectId>)>,
    ) -> Result<String, GitError> {
        let undo_dir = git_dir.join("refs").join("autopsy").join("undo");
        fs::create_dir_all(&undo_dir).map_err(|e| GitError::Io {
            path: undo_dir.clone(),
            source: e,
        })?;

        let head_content = fs::read_to_string(git_dir.join("HEAD")).unwrap_or_default();
        let head_ref = head_content
            .trim()
            .strip_prefix("ref: ")
            .map(str::to_string);
        let branch_path = branch_state.map(|(path, _)| path.to_string());
        let branch_oid = branch_state
            .and_then(|(_, oid)| oid)
            .map(|oid| oid.to_hex());
        let index_hash = index_digest(git_dir)?;
        let undo_id = blake3::hash(
            format!(
                "undo\0{}\0{}\0{}\0{}\0{}",
                head_oid.to_hex(),
                head_ref.as_deref().unwrap_or("DETACHED"),
                index_hash,
                branch_path.as_deref().unwrap_or(""),
                branch_oid.as_deref().unwrap_or("")
            )
            .as_bytes(),
        )
        .to_hex()
        .chars()
        .take(24)
        .collect::<String>();

        let undo_file = undo_dir.join(&undo_id);
        let checkpoint = serde_json::to_vec(&UndoCheckpoint {
            target_oid: head_oid.to_hex(),
            head_ref,
            branch_path,
            branch_oid,
        })
        .map_err(|e| GitError::Malformed(format!("cannot encode undo checkpoint: {e}")))?;
        fs::write(&undo_file, checkpoint).map_err(|e| GitError::Io {
            path: undo_file,
            source: e,
        })?;
        let index_file = index_path(git_dir);
        let index_snapshot = undo_dir.join(format!("{undo_id}.index"));
        if index_file.exists() {
            fs::copy(&index_file, &index_snapshot).map_err(|e| GitError::Io {
                path: index_snapshot,
                source: e,
            })?;
        } else if index_snapshot.exists() {
            let _ = fs::remove_file(index_snapshot);
        }

        Ok(undo_id)
    }

    pub(crate) fn write_loose_object(
        &self,
        git_dir: &Path,
        kind: &str,
        payload: &[u8],
    ) -> Result<ObjectId, GitError> {
        let oid = compute_object_id(kind, payload);
        let hex = oid.to_hex();
        let (dir_hex, file_hex) = hex.split_at(2);

        let obj_dir = git_dir.join("objects").join(dir_hex);
        fs::create_dir_all(&obj_dir).map_err(|e| GitError::Io {
            path: obj_dir.clone(),
            source: e,
        })?;

        let obj_path = obj_dir.join(file_hex);
        if obj_path.exists() {
            return Ok(oid);
        }

        // Object format: "{kind} {size}\0{payload}" compressed with zlib
        let header = format!("{kind} {}\0", payload.len());
        let mut full_data = Vec::with_capacity(header.len() + payload.len());
        full_data.extend_from_slice(header.as_bytes());
        full_data.extend_from_slice(payload);

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&full_data).map_err(|e| GitError::Io {
            path: obj_path.clone(),
            source: e,
        })?;
        let compressed = encoder.finish().map_err(|e| GitError::Io {
            path: obj_path.clone(),
            source: e,
        })?;

        fs::write(&obj_path, compressed).map_err(|e| GitError::Io {
            path: obj_path,
            source: e,
        })?;

        Ok(oid)
    }

    fn build_git_tree(
        &self,
        git_dir: &Path,
        file_map: &BTreeMap<String, (u32, ObjectId)>,
    ) -> Result<ObjectId, GitError> {
        let mut root = BTreeMap::new();

        for (path, (mode, oid)) in file_map {
            let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            if parts.is_empty() {
                continue;
            }

            let mut current = &mut root;
            for (i, part) in parts.iter().enumerate() {
                if i == parts.len() - 1 {
                    current.insert(part.to_string(), GitTreeNode::Blob(*mode, *oid));
                } else {
                    let entry = current
                        .entry(part.to_string())
                        .or_insert_with(|| GitTreeNode::Tree(BTreeMap::new()));
                    match entry {
                        GitTreeNode::Tree(sub_map) => {
                            current = sub_map;
                        }
                        GitTreeNode::Blob(_, _) => {
                            break;
                        }
                    }
                }
            }
        }

        self.write_tree_node(git_dir, &root)
    }

    fn write_tree_node(
        &self,
        git_dir: &Path,
        node: &BTreeMap<String, GitTreeNode>,
    ) -> Result<ObjectId, GitError> {
        let mut entries = Vec::new();
        for (name, item) in node {
            match item {
                GitTreeNode::Blob(mode, oid) => {
                    entries.push((*mode, name.clone(), *oid));
                }
                GitTreeNode::Tree(sub_map) => {
                    if !sub_map.is_empty() {
                        let sub_tree_oid = self.write_tree_node(git_dir, sub_map)?;
                        entries.push((0o40000, name.clone(), sub_tree_oid));
                    }
                }
            }
        }

        // Git canonical sorting: directories sorted as if they have a trailing '/'
        entries.sort_by(|a, b| {
            let a_key = if a.0 == 0o40000 {
                format!("{}/", a.1)
            } else {
                a.1.clone()
            };
            let b_key = if b.0 == 0o40000 {
                format!("{}/", b.1)
            } else {
                b.1.clone()
            };
            a_key.cmp(&b_key)
        });

        let mut payload = Vec::new();
        for (mode, name, oid) in entries {
            let mode_str = format!("{mode:o}");
            payload.extend_from_slice(mode_str.as_bytes());
            payload.push(b' ');
            payload.extend_from_slice(name.as_bytes());
            payload.push(0);
            payload.extend_from_slice(&oid.0);
        }

        self.write_loose_object(git_dir, "tree", &payload)
    }
}
