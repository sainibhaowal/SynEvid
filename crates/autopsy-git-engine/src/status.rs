//! Pure-Rust Working Tree Git Status Scanner and Line-Level Diff Engine.
//!
//! Complies with INV-SANDBOX-003 (Zero sub-process execution) and FR-026 (Read-only analyzer).

use crate::index::{clean_worktree_bytes, index_path, read_index};
use crate::model::*;
use autopsy_temporal::git::{GitError, GitObjectStore, ObjectId, flatten_tree};
use sha1::{Digest, Sha1};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub struct GitStatusScanner {
    pub repo_root: PathBuf,
}

impl GitStatusScanner {
    pub fn new(repo_root: impl AsRef<Path>) -> Self {
        Self {
            repo_root: repo_root.as_ref().to_path_buf(),
        }
    }

    /// Read comprehensive working tree status (staged, modified, untracked, deleted, ignored).
    pub fn scan_status(&self) -> Result<GitStatusReport, GitError> {
        let mut store = GitObjectStore::discover(&self.repo_root)?;
        let git_dir = store.git_dir().to_path_buf();

        // 1. Resolve active branch & HEAD commit
        let (branch_name, head_oid) = self.resolve_head_and_branch(&git_dir, &mut store)?;

        // 2. Read the real Git index. The index, not an Autopsy sidecar,
        // defines staged state.
        let has_index = index_path(&git_dir).exists();
        let index_entries = read_index(&git_dir)?;

        // 3. Flatten HEAD tree into map of path -> object_id
        let mut head_files: BTreeMap<String, ObjectId> = BTreeMap::new();
        if let Some(ref h) = head_oid {
            if let Ok(oid) = ObjectId::from_hex(h) {
                if let Ok(commit) = store.commit(oid) {
                    if let Ok(entries) = flatten_tree(&store, commit.tree) {
                        for (path, (_mode, id)) in entries {
                            head_files.insert(path, id);
                        }
                    }
                }
            }
        }

        // 4. Parse ignore patterns
        let ignore_rules = self.load_ignore_patterns();

        // 5. Walk working directory
        let mut staged = Vec::new();
        let mut unstaged = Vec::new();
        let mut untracked = Vec::new();
        let mut ignored = Vec::new();
        let mut seen_disk_files = BTreeSet::new();

        self.walk_dir(
            &self.repo_root,
            "",
            &ignore_rules,
            &mut seen_disk_files,
            &mut ignored,
        );

        for rel_path in &seen_disk_files {
            let full_path = self.repo_root.join(rel_path);
            let file_bytes = fs::read(&full_path).unwrap_or_default();
            let clean_bytes = clean_worktree_bytes(rel_path, &file_bytes);
            let local_blob_oid = Self::compute_git_blob_id(&clean_bytes);
            let index_entry = index_entries.get(rel_path);
            let head_blob_oid = head_files.get(rel_path).copied();
            let staged_changed = index_entry
                .map(|entry| Some(entry.oid) != head_blob_oid)
                .unwrap_or_else(|| head_blob_oid.is_some() && has_index);
            let worktree_changed = index_entry
                .map(|entry| local_blob_oid != entry.oid)
                .unwrap_or_else(|| {
                    head_blob_oid
                        .map(|head_oid| local_blob_oid != head_oid)
                        .unwrap_or(true)
                });

            if staged_changed {
                let (added, deleted) = if let Some(head_oid) = head_blob_oid {
                    self.count_line_diffs(&mut store, head_oid, &file_bytes)
                } else {
                    (String::from_utf8_lossy(&file_bytes).lines().count(), 0)
                };
                staged.push(FileStatusItem {
                    path: rel_path.clone(),
                    status: "staged".to_string(),
                    added_lines: added,
                    deleted_lines: deleted,
                });
            }
            if worktree_changed {
                let (added, deleted) = if let Some(entry) = index_entry {
                    self.count_line_diffs(&mut store, entry.oid, &file_bytes)
                } else {
                    (String::from_utf8_lossy(&file_bytes).lines().count(), 0)
                };
                if index_entry.is_none() && head_blob_oid.is_none() {
                    untracked.push(FileStatusItem {
                        path: rel_path.clone(),
                        status: "untracked".to_string(),
                        added_lines: added,
                        deleted_lines: deleted,
                    });
                } else {
                    unstaged.push(FileStatusItem {
                        path: rel_path.clone(),
                        status: "modified".to_string(),
                        added_lines: added,
                        deleted_lines: deleted,
                    });
                }
            }
        }

        // 6. Check for deleted files in the working tree and/or index.
        let mut deleted = Vec::new();
        let mut all_tracked = BTreeSet::new();
        all_tracked.extend(head_files.keys().cloned());
        all_tracked.extend(index_entries.keys().cloned());
        for head_path in all_tracked {
            let head_oid = head_files.get(&head_path).copied();
            let index_oid = index_entries.get(&head_path).map(|entry| entry.oid);
            if !seen_disk_files.contains(&head_path) {
                if has_index && index_oid != head_oid {
                    staged.push(FileStatusItem {
                        path: head_path.clone(),
                        status: "staged".to_string(),
                        added_lines: 0,
                        deleted_lines: 1,
                    });
                }
                if index_oid.is_some() || (!has_index && head_oid.is_some()) {
                    deleted.push(FileStatusItem {
                        path: head_path,
                        status: "deleted".to_string(),
                        added_lines: 0,
                        deleted_lines: 1,
                    });
                }
            }
        }

        let total_uncommitted = staged.len() + unstaged.len() + untracked.len() + deleted.len();
        let is_clean = total_uncommitted == 0;

        Ok(GitStatusReport {
            branch: branch_name,
            head_oid,
            staged,
            unstaged,
            untracked,
            deleted,
            ignored,
            is_clean,
            total_uncommitted,
        })
    }

    /// Compute line-by-line diff between HEAD blob and working copy.
    pub fn diff_file(&self, rel_path: &str) -> Result<FileDiffReport, GitError> {
        let mut store = GitObjectStore::discover(&self.repo_root)?;
        let git_dir = store.git_dir().to_path_buf();
        let (_branch, head_oid) = self.resolve_head_and_branch(&git_dir, &mut store)?;

        let mut old_content = String::new();
        if let Some(ref h) = head_oid {
            if let Ok(oid) = ObjectId::from_hex(h) {
                if let Ok(commit) = store.commit(oid) {
                    if let Ok(entries) = flatten_tree(&store, commit.tree) {
                        for (path, (_mode, blob_id)) in entries {
                            if path == rel_path {
                                if let Ok(blob) = store.blob(blob_id) {
                                    old_content = String::from_utf8_lossy(&blob).to_string();
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }

        let full_path = self.repo_root.join(rel_path);
        let new_content = if full_path.exists() {
            fs::read_to_string(&full_path).unwrap_or_default()
        } else {
            String::new()
        };

        let status = if old_content.is_empty() && !new_content.is_empty() {
            "untracked".to_string()
        } else if !old_content.is_empty() && new_content.is_empty() {
            "deleted".to_string()
        } else {
            "modified".to_string()
        };

        let (diff_lines, total_additions, total_deletions) =
            Self::compute_line_diff_hunks(&old_content, &new_content);

        Ok(FileDiffReport {
            path: rel_path.to_string(),
            status,
            old_content,
            new_content,
            diff_lines,
            total_additions,
            total_deletions,
        })
    }

    fn resolve_head_and_branch(
        &self,
        git_dir: &Path,
        store: &mut GitObjectStore,
    ) -> Result<(String, Option<String>), GitError> {
        let head_path = git_dir.join("HEAD");
        let head_content = fs::read_to_string(&head_path).unwrap_or_default();
        let branch = if let Some(ref_path) = head_content.trim().strip_prefix("ref: refs/heads/") {
            ref_path.to_string()
        } else if head_content.trim().starts_with("ref: ") {
            head_content.trim().to_string()
        } else {
            "HEAD (detached)".to_string()
        };

        let head_oid = store.resolve_ref("HEAD")?.map(|o| o.to_hex());
        Ok((branch, head_oid))
    }

    fn load_ignore_patterns(&self) -> Vec<String> {
        let mut patterns = vec![
            ".git".to_string(),
            "target".to_string(),
            "node_modules".to_string(),
            ".next".to_string(),
            ".autopsy".to_string(),
            "dist".to_string(),
            ".DS_Store".to_string(),
        ];

        let gitignore = self.repo_root.join(".gitignore");
        if let Ok(content) = fs::read_to_string(gitignore) {
            for line in content.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    let cleaned = trimmed.trim_start_matches('/').trim_end_matches('/');
                    if !cleaned.is_empty() {
                        patterns.push(cleaned.to_string());
                    }
                }
            }
        }

        patterns
    }

    fn is_ignored(rel_path: &str, patterns: &[String]) -> bool {
        let normalized = rel_path.replace('\\', "/");
        for p in patterns {
            if normalized == *p
                || normalized.starts_with(&format!("{}/", p))
                || normalized.contains(&format!("/{}", p))
            {
                return true;
            }
        }
        false
    }

    fn walk_dir(
        &self,
        current_dir: &Path,
        prefix: &str,
        patterns: &[String],
        seen_files: &mut BTreeSet<String>,
        ignored_files: &mut Vec<String>,
    ) {
        if let Ok(entries) = fs::read_dir(current_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let file_name = entry.file_name().to_string_lossy().to_string();
                let rel_path = if prefix.is_empty() {
                    file_name.clone()
                } else {
                    format!("{}/{}", prefix, file_name)
                };

                if Self::is_ignored(&rel_path, patterns) {
                    if path.is_file() {
                        ignored_files.push(rel_path);
                    }
                    continue;
                }

                if path.is_dir() {
                    self.walk_dir(&path, &rel_path, patterns, seen_files, ignored_files);
                } else if path.is_file() {
                    seen_files.insert(rel_path);
                }
            }
        }
    }

    fn compute_git_blob_id(bytes: &[u8]) -> ObjectId {
        let header = format!("blob {}\0", bytes.len());
        let mut hasher = Sha1::new();
        hasher.update(header.as_bytes());
        hasher.update(bytes);
        let result = hasher.finalize();
        let mut id = [0u8; 20];
        id.copy_from_slice(&result);
        ObjectId(id)
    }

    fn count_line_diffs(
        &self,
        store: &mut GitObjectStore,
        head_blob_oid: ObjectId,
        working_bytes: &[u8],
    ) -> (usize, usize) {
        let old_content = store
            .blob(head_blob_oid)
            .map(|b| String::from_utf8_lossy(&b).to_string())
            .unwrap_or_default();
        let new_content = String::from_utf8_lossy(working_bytes).to_string();

        let (_lines, add, del) = Self::compute_line_diff_hunks(&old_content, &new_content);
        (add, del)
    }

    fn compute_line_diff_hunks(
        old_content: &str,
        new_content: &str,
    ) -> (Vec<DiffLine>, usize, usize) {
        let old_lines: Vec<&str> = old_content.lines().collect();
        let new_lines: Vec<&str> = new_content.lines().collect();

        let mut diff_lines = Vec::new();
        let mut additions = 0;
        let mut deletions = 0;

        let max_lines = old_lines.len().max(new_lines.len());
        for i in 0..max_lines {
            match (old_lines.get(i), new_lines.get(i)) {
                (Some(o), Some(n)) => {
                    if o == n {
                        diff_lines.push(DiffLine {
                            kind: "ctx".to_string(),
                            old_line: Some(i + 1),
                            new_line: Some(i + 1),
                            text: (*o).to_string(),
                        });
                    } else {
                        diff_lines.push(DiffLine {
                            kind: "del".to_string(),
                            old_line: Some(i + 1),
                            new_line: None,
                            text: (*o).to_string(),
                        });
                        deletions += 1;
                        diff_lines.push(DiffLine {
                            kind: "add".to_string(),
                            old_line: None,
                            new_line: Some(i + 1),
                            text: (*n).to_string(),
                        });
                        additions += 1;
                    }
                }
                (Some(o), None) => {
                    diff_lines.push(DiffLine {
                        kind: "del".to_string(),
                        old_line: Some(i + 1),
                        new_line: None,
                        text: (*o).to_string(),
                    });
                    deletions += 1;
                }
                (None, Some(n)) => {
                    diff_lines.push(DiffLine {
                        kind: "add".to_string(),
                        old_line: None,
                        new_line: Some(i + 1),
                        text: (*n).to_string(),
                    });
                    additions += 1;
                }
                (None, None) => {}
            }
        }

        (diff_lines, additions, deletions)
    }
}
