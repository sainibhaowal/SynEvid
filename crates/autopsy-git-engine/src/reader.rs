//! Pure-Rust Git DAG reader and Galaxy topology builder.
//!
//! Complies with INV-SANDBOX-003 (Zero sub-process execution) and FR-026 (Read-only analyzer).

use crate::model::*;
use autopsy_temporal::git::{GitError, GitObjectStore, ObjectId, flatten_tree};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

pub struct GitGalaxyReader {
    pub repo_root: PathBuf,
}

impl GitGalaxyReader {
    pub fn new(repo_root: impl AsRef<Path>) -> Self {
        Self {
            repo_root: repo_root.as_ref().to_path_buf(),
        }
    }

    /// Read branches, tags, and commit DAG from `init` to `HEAD`.
    pub fn read_galaxy(&self, max_commits: usize) -> Result<GitGalaxy, GitError> {
        let mut store = GitObjectStore::discover(&self.repo_root)?;
        let git_dir = store.git_dir().to_path_buf();

        // A large packed history is dominated by independent object
        // decompression. Warm only bounded packs so the normal small-history
        // path remains cheap and repositories with large binary asset packs
        // are not scanned just to render a commit galaxy.
        if max_commits >= 1_000 {
            let preload_bound = max_commits.saturating_mul(4);
            if store.packed_object_count() > 0 {
                store.preload_packed_objects(preload_bound);
            } else {
                store.preload_loose_objects(preload_bound);
            }
        }

        // 1. Identify active branch & HEAD commit
        let (active_branch, head_oid) = Self::read_head_info(&git_dir, &mut store)?;

        // 2. Discover local branches
        let branches = Self::discover_branches(&git_dir, &mut store, &active_branch)?;

        // 3. Discover tags
        let tags = Self::discover_tags(&git_dir, &mut store)?;

        // Map commit OID hex -> attached branch names & tag names
        let mut commit_branches: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for b in &branches {
            commit_branches
                .entry(b.target_oid.clone())
                .or_default()
                .push(b.name.clone());
        }

        let mut commit_tags: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for t in &tags {
            commit_tags
                .entry(t.target_oid.clone())
                .or_default()
                .push(t.name.clone());
        }

        // 4. Walk the commit DAG starting from HEAD and all branch pointers
        let mut start_oids = Vec::new();
        if let Some(ref h) = head_oid {
            if let Ok(oid) = ObjectId::from_hex(h) {
                start_oids.push(oid);
            }
        }
        for b in &branches {
            if let Ok(oid) = ObjectId::from_hex(&b.target_oid) {
                if !start_oids.contains(&oid) {
                    start_oids.push(oid);
                }
            }
        }

        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::new();
        for oid in start_oids {
            queue.push_back(oid);
            visited.insert(oid);
        }

        let mut raw_commits = Vec::new();
        while let Some(oid) = queue.pop_front() {
            if raw_commits.len() >= max_commits {
                break;
            }
            if let Ok(c) = store.commit(oid) {
                for parent in &c.parents {
                    if visited.insert(*parent) {
                        queue.push_back(*parent);
                    }
                }
                raw_commits.push((oid, c));
            }
        }

        // 5. Build topological DAG nodes with branch lanes and forensic impact.
        // Cache decoded commits and trees so long linear histories are read once.
        // The common single-parent case is already emitted newest-first by the
        // history walk; avoid rebuilding a 5,000-entry ordered set for it.
        let linear_history = raw_commits.windows(2).all(|pair| {
            pair[0].1.parents.len() <= 1 && pair[0].1.parents.first() == Some(&pair[1].0)
        });
        let commit_cache: BTreeMap<ObjectId, autopsy_temporal::git::Commit> = raw_commits
            .iter()
            .map(|(oid, commit)| (*oid, commit.clone()))
            .collect();
        let ordered_commits = if linear_history {
            raw_commits
        } else {
            let mut child_counts: BTreeMap<ObjectId, usize> =
                commit_cache.keys().map(|oid| (*oid, 0)).collect();
            for commit in commit_cache.values() {
                for parent in &commit.parents {
                    if let Some(count) = child_counts.get_mut(parent) {
                        *count += 1;
                    }
                }
            }
            let mut ready: BTreeSet<ObjectId> = child_counts
                .iter()
                .filter_map(|(oid, children)| (*children == 0).then_some(*oid))
                .collect();
            let mut topological_oids = Vec::with_capacity(commit_cache.len());
            while let Some(oid) = ready.iter().next().copied() {
                ready.remove(&oid);
                topological_oids.push(oid);
                if let Some(commit) = commit_cache.get(&oid) {
                    for parent in &commit.parents {
                        if let Some(count) = child_counts.get_mut(parent) {
                            *count -= 1;
                            if *count == 0 {
                                ready.insert(*parent);
                            }
                        }
                    }
                }
            }
            topological_oids
                .into_iter()
                .filter_map(|oid| commit_cache.get(&oid).cloned().map(|commit| (oid, commit)))
                .collect()
        };
        let mut tree_cache = BTreeMap::new();
        let mut commits = Vec::with_capacity(ordered_commits.len());
        let head_hex = head_oid.clone().unwrap_or_default();

        for (idx, (oid, commit)) in ordered_commits.iter().enumerate() {
            let oid_hex = oid.to_hex();
            let short_oid = oid_hex[..7.min(oid_hex.len())].to_string();
            let is_head = oid_hex == head_hex;

            let parents_hex: Vec<String> = commit.parents.iter().map(|p| p.to_hex()).collect();
            let lane = if idx == 0 || commit.parents.len() <= 1 {
                0
            } else {
                1 + (idx % 3)
            };

            // Lightweight per-commit tree differ to extract modified symbols & blast radius
            let parent = commit
                .parents
                .first()
                .and_then(|parent_oid| commit_cache.get(parent_oid));
            let (sym_mod, sym_add, sym_del, blast_score, verdict) =
                Self::evaluate_commit_forensics(&store, commit, parent, &mut tree_cache);

            commits.push(CommitGalaxyNode {
                oid: oid_hex.clone(),
                short_oid,
                summary: commit.summary().to_string(),
                author_name: commit.author.clone(),
                author_email: "".to_string(),
                timestamp: commit.committer_timestamp().unwrap_or(0),
                parents: parents_hex,
                lane,
                is_head,
                branches: commit_branches.remove(&oid_hex).unwrap_or_default(),
                tags: commit_tags.remove(&oid_hex).unwrap_or_default(),
                symbols_modified: sym_mod,
                symbols_added: sym_add,
                symbols_deleted: sym_del,
                blast_radius_score: blast_score,
                verdict,
            });
        }

        let repo_id = blake3::hash(self.repo_root.to_string_lossy().as_bytes())
            .to_hex()
            .to_string();
        let total_commits = commits.len();

        Ok(GitGalaxy {
            repo_id,
            head_oid,
            active_branch,
            commits,
            branches,
            tags,
            total_commits,
        })
    }

    fn read_head_info(
        git_dir: &Path,
        store: &mut GitObjectStore,
    ) -> Result<(Option<String>, Option<String>), GitError> {
        let head_path = git_dir.join("HEAD");
        if !head_path.exists() {
            return Ok((None, None));
        }

        let contents = fs::read_to_string(&head_path).unwrap_or_default();
        let contents = contents.trim();

        if let Some(branch_ref) = contents.strip_prefix("ref: refs/heads/") {
            let branch_name = branch_ref.trim().to_string();
            let head_oid = store.resolve_ref("HEAD")?.map(|o| o.to_hex());
            Ok((Some(branch_name), head_oid))
        } else if let Ok(oid) = ObjectId::from_hex(contents) {
            Ok((None, Some(oid.to_hex())))
        } else {
            let head_oid = store.resolve_ref("HEAD")?.map(|o| o.to_hex());
            Ok((Some("main".to_string()), head_oid))
        }
    }

    fn discover_branches(
        git_dir: &Path,
        _store: &mut GitObjectStore,
        active_branch: &Option<String>,
    ) -> Result<Vec<BranchRef>, GitError> {
        let mut branches = Vec::new();
        let heads_dir = git_dir.join("refs").join("heads");

        if heads_dir.is_dir() {
            Self::scan_ref_dir(&heads_dir, &heads_dir, &mut |name, path| {
                if let Ok(contents) = fs::read_to_string(path) {
                    let hex = contents.trim();
                    let is_head = active_branch.as_deref() == Some(&name);
                    branches.push(BranchRef {
                        name,
                        target_oid: hex.to_string(),
                        is_head,
                    });
                }
            });
        }

        // Also check packed-refs for heads
        let packed = git_dir.join("packed-refs");
        if packed.exists() {
            if let Ok(contents) = fs::read_to_string(&packed) {
                for line in contents.lines() {
                    let line = line.trim();
                    if line.starts_with('#') || line.starts_with('^') || line.is_empty() {
                        continue;
                    }
                    if let Some((oid_hex, name)) = line.split_once(' ') {
                        if let Some(branch_name) = name.strip_prefix("refs/heads/") {
                            if !branches.iter().any(|b| b.name == branch_name) {
                                let is_head = active_branch.as_deref() == Some(branch_name);
                                branches.push(BranchRef {
                                    name: branch_name.to_string(),
                                    target_oid: oid_hex.to_string(),
                                    is_head,
                                });
                            }
                        }
                    }
                }
            }
        }

        branches.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(branches)
    }

    fn discover_tags(git_dir: &Path, _store: &mut GitObjectStore) -> Result<Vec<TagRef>, GitError> {
        let mut tags = Vec::new();
        let tags_dir = git_dir.join("refs").join("tags");

        if tags_dir.is_dir() {
            Self::scan_ref_dir(&tags_dir, &tags_dir, &mut |name, path| {
                if let Ok(contents) = fs::read_to_string(path) {
                    tags.push(TagRef {
                        name,
                        target_oid: contents.trim().to_string(),
                    });
                }
            });
        }

        // Check packed-refs for tags
        let packed = git_dir.join("packed-refs");
        if packed.exists() {
            if let Ok(contents) = fs::read_to_string(&packed) {
                for line in contents.lines() {
                    let line = line.trim();
                    if line.starts_with('#') || line.starts_with('^') || line.is_empty() {
                        continue;
                    }
                    if let Some((oid_hex, name)) = line.split_once(' ') {
                        if let Some(tag_name) = name.strip_prefix("refs/tags/") {
                            if !tags.iter().any(|t| t.name == tag_name) {
                                tags.push(TagRef {
                                    name: tag_name.to_string(),
                                    target_oid: oid_hex.to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }

        tags.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(tags)
    }

    fn scan_ref_dir(base: &Path, current: &Path, cb: &mut dyn FnMut(String, &Path)) {
        if let Ok(entries) = fs::read_dir(current) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    Self::scan_ref_dir(base, &path, cb);
                } else if path.is_file() {
                    if let Ok(rel) = path.strip_prefix(base) {
                        let name = rel.to_string_lossy().replace('\\', "/");
                        cb(name, &path);
                    }
                }
            }
        }
    }

    fn evaluate_commit_forensics(
        store: &GitObjectStore,
        commit: &autopsy_temporal::git::Commit,
        parent: Option<&autopsy_temporal::git::Commit>,
        tree_cache: &mut BTreeMap<ObjectId, BTreeMap<String, (u32, ObjectId)>>,
    ) -> (usize, usize, usize, usize, String) {
        if let Some(parent) = parent
            && parent.tree == commit.tree
        {
            return (0, 0, 0, 0, "PASS".to_string());
        }
        let tree_curr = match Self::cached_tree(store, commit.tree, tree_cache) {
            Ok(t) => t,
            Err(_) => return (0, 0, 0, 0, "PASS".to_string()),
        };

        let tree_prev = parent
            .and_then(|parent| Self::cached_tree(store, parent.tree, tree_cache).ok())
            .unwrap_or_default();

        let mut added = 0;
        let mut modified = 0;
        let mut deleted = 0;

        for (path, (_mode, oid)) in &tree_curr {
            match tree_prev.get(path) {
                None => added += 1,
                Some((_prev_mode, prev_oid)) => {
                    if prev_oid != oid {
                        modified += 1;
                    }
                }
            }
        }

        for path in tree_prev.keys() {
            if !tree_curr.contains_key(path) {
                deleted += 1;
            }
        }

        let blast_score = modified * 2 + added + deleted * 3;
        let verdict = if deleted > 2 || modified > 10 {
            "WARN"
        } else {
            "PASS"
        };

        (modified, added, deleted, blast_score, verdict.to_string())
    }

    fn cached_tree(
        store: &GitObjectStore,
        tree_oid: ObjectId,
        tree_cache: &mut BTreeMap<ObjectId, BTreeMap<String, (u32, ObjectId)>>,
    ) -> Result<BTreeMap<String, (u32, ObjectId)>, GitError> {
        if let Some(tree) = tree_cache.get(&tree_oid) {
            return Ok(tree.clone());
        }
        let tree = flatten_tree(store, tree_oid)?;
        tree_cache.insert(tree_oid, tree.clone());
        Ok(tree)
    }
}

/// Resolve a user-supplied revision string (`HEAD`, branch name, tag name, 40-char OID, or short hex prefix)
/// into an exact `ObjectId`.
pub fn resolve_revision(
    store: &mut GitObjectStore,
    git_dir: &Path,
    rev: &str,
) -> Result<ObjectId, GitError> {
    let rev = rev.trim();
    if rev.is_empty() {
        return Err(GitError::Malformed("empty revision string".to_string()));
    }

    // 1. Exact 40-hex chars
    if rev.len() == 40 && rev.chars().all(|c| c.is_ascii_hexdigit()) {
        if let Ok(oid) = ObjectId::from_hex(rev) {
            return Ok(oid);
        }
    }

    // 2. Exact ref or HEAD
    if let Ok(Some(oid)) = store.resolve_ref(rev) {
        return Ok(oid);
    }

    // 3. Branch ref refs/heads/<rev>
    if let Ok(Some(oid)) = store.resolve_ref(&format!("refs/heads/{rev}")) {
        return Ok(oid);
    }

    // 4. Tag ref refs/tags/<rev>
    if let Ok(Some(oid)) = store.resolve_ref(&format!("refs/tags/{rev}")) {
        return Ok(oid);
    }

    // 5. Short hex prefix (>= 4 chars)
    if rev.len() >= 4 && rev.len() < 40 && rev.chars().all(|c| c.is_ascii_hexdigit()) {
        let (prefix, rest) = rev.split_at(2);
        let obj_dir = git_dir.join("objects").join(prefix);
        if obj_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(obj_dir) {
                let rest_lower = rest.to_ascii_lowercase();
                for entry in entries.flatten() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    if file_name.to_ascii_lowercase().starts_with(&rest_lower)
                        && file_name.len() == 38
                    {
                        let full_hex = format!("{}{}", prefix, file_name);
                        if let Ok(oid) = ObjectId::from_hex(&full_hex) {
                            return Ok(oid);
                        }
                    }
                }
            }
        }
    }

    Err(GitError::ObjectNotFound(format!(
        "revision '{rev}' not found"
    )))
}
