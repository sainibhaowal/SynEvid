//! Deterministic line-level semantic blame over native Git objects.

use crate::index::is_safe_index_path;
use crate::model::{BlameLine, BlameReport};
use autopsy_temporal::git::{GitError, GitObjectStore, ObjectId, flatten_tree};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

pub struct GitBlameEngine {
    pub repo_root: PathBuf,
}

impl GitBlameEngine {
    pub fn new(repo_root: impl AsRef<Path>) -> Self {
        Self {
            repo_root: repo_root.as_ref().to_path_buf(),
        }
    }

    pub fn blame(&self, path: &str, revision: &str) -> Result<BlameReport, GitError> {
        if !is_safe_index_path(path) {
            return Err(GitError::Malformed(format!("unsafe blame path: {path}")));
        }
        let mut store = GitObjectStore::discover(&self.repo_root)?;
        let git_dir = store.git_dir().to_path_buf();
        let end_oid = crate::reader::resolve_revision(&mut store, &git_dir, revision)?;

        let mut newest_first = Vec::new();
        let mut cursor = end_oid;
        let mut visited = std::collections::BTreeSet::new();
        loop {
            if !visited.insert(cursor) {
                break;
            }
            let commit = store.commit(cursor)?;
            let next = commit.parents.first().copied();
            newest_first.push(commit);
            let Some(parent) = next else {
                break;
            };
            cursor = parent;
        }
        newest_first.reverse();

        let mut previous_lines: Vec<String> = Vec::new();
        let mut attributions: Vec<Attribution> = Vec::new();
        let mut coverage_state = "Verified".to_string();

        for (sequence_index, commit) in newest_first.iter().enumerate() {
            let sequence = sequence_index + 1;
            let current_bytes = Self::blob_for_path(&store, commit.tree, path)?;
            let current_text = match current_bytes {
                Some(bytes) => match String::from_utf8(bytes) {
                    Ok(text) => text,
                    Err(_) => {
                        coverage_state = "Unknown".to_string();
                        String::new()
                    }
                },
                None => String::new(),
            };
            let current_lines = split_lines(&current_text);
            let changed_files = Self::changed_file_count(&store, commit)?;
            let attribution = Attribution {
                commit_oid: commit.oid.to_hex(),
                author: display_author(&commit.author),
                timestamp: commit.committer_timestamp().unwrap_or(0),
                sequence,
                blast_radius_score: changed_files,
            };

            let matches = lcs_matches(&previous_lines, &current_lines);
            let mut next_attributions = vec![attribution; current_lines.len()];
            for (old_index, new_index) in matches {
                if let Some(previous) = attributions.get(old_index) {
                    next_attributions[new_index] = previous.clone();
                }
            }
            previous_lines = current_lines;
            attributions = next_attributions;
        }

        let lines = previous_lines
            .into_iter()
            .zip(attributions)
            .enumerate()
            .map(|(index, (text, attribution))| BlameLine {
                line: index + 1,
                text,
                commit_oid: attribution.commit_oid,
                author: attribution.author,
                timestamp: attribution.timestamp,
                sequence: attribution.sequence,
                blast_radius_score: attribution.blast_radius_score,
            })
            .collect::<Vec<_>>();
        let digest_input = serde_json::to_vec(&(path, &lines, &coverage_state))
            .map_err(|error| GitError::Malformed(format!("blame encoding failed: {error}")))?;

        Ok(BlameReport {
            path: path.to_string(),
            lines,
            coverage_state,
            result_digest: blake3::hash(&digest_input).to_hex().to_string(),
        })
    }

    fn blob_for_path(
        store: &GitObjectStore,
        tree: ObjectId,
        path: &str,
    ) -> Result<Option<Vec<u8>>, GitError> {
        let entries = flatten_tree(store, tree)?;
        let Some((_mode, oid)) = entries.get(path) else {
            return Ok(None);
        };
        store.blob(*oid).map(Some)
    }

    fn changed_file_count(
        store: &GitObjectStore,
        commit: &autopsy_temporal::git::Commit,
    ) -> Result<usize, GitError> {
        let current = flatten_tree(store, commit.tree)?;
        let previous = if let Some(parent) = commit.parents.first() {
            let parent_commit = store.commit(*parent)?;
            flatten_tree(store, parent_commit.tree)?
        } else {
            BTreeMap::new()
        };
        let mut paths = std::collections::BTreeSet::new();
        paths.extend(current.keys().cloned());
        paths.extend(previous.keys().cloned());
        Ok(paths
            .into_iter()
            .filter(|path| {
                current.get(path).map(|entry| entry.1) != previous.get(path).map(|entry| entry.1)
            })
            .count())
    }
}

#[derive(Clone)]
struct Attribution {
    commit_oid: String,
    author: String,
    timestamp: i64,
    sequence: usize,
    blast_radius_score: usize,
}

fn display_author(raw: &str) -> String {
    raw.split_once(" <")
        .map(|(name, _)| name.to_string())
        .unwrap_or_else(|| raw.to_string())
}

fn split_lines(text: &str) -> Vec<String> {
    text.lines().map(ToString::to_string).collect()
}

fn lcs_matches(old: &[String], new: &[String]) -> Vec<(usize, usize)> {
    // Keep blame bounded for large generated files. The deterministic fallback
    // still preserves equal line occurrences in source order.
    if old.len().saturating_mul(new.len()) > 2_000_000 {
        return greedy_matches(old, new);
    }
    let mut dp = vec![vec![0usize; new.len() + 1]; old.len() + 1];
    for old_index in (0..old.len()).rev() {
        for new_index in (0..new.len()).rev() {
            dp[old_index][new_index] = if old[old_index] == new[new_index] {
                dp[old_index + 1][new_index + 1] + 1
            } else {
                dp[old_index + 1][new_index].max(dp[old_index][new_index + 1])
            };
        }
    }
    let mut matches = Vec::new();
    let (mut old_index, mut new_index) = (0usize, 0usize);
    while old_index < old.len() && new_index < new.len() {
        if old[old_index] == new[new_index] {
            matches.push((old_index, new_index));
            old_index += 1;
            new_index += 1;
        } else if dp[old_index + 1][new_index] >= dp[old_index][new_index + 1] {
            old_index += 1;
        } else {
            new_index += 1;
        }
    }
    matches
}

fn greedy_matches(old: &[String], new: &[String]) -> Vec<(usize, usize)> {
    let mut positions: BTreeMap<&str, VecDeque<usize>> = BTreeMap::new();
    for (index, line) in old.iter().enumerate() {
        positions.entry(line).or_default().push_back(index);
    }
    let mut matches = Vec::new();
    for (new_index, line) in new.iter().enumerate() {
        if let Some(old_index) = positions
            .get_mut(line.as_str())
            .and_then(VecDeque::pop_front)
        {
            matches.push((old_index, new_index));
        }
    }
    matches.sort_by_key(|(old_index, _)| *old_index);
    matches
}
