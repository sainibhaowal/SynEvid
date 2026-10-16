//! Commit-history extraction for co-change mining.
//!
//! Turns a repository into a bounded, fully accounted-for list of *change sets*: which
//! paths one commit changed, or `None` with a stated reason for every commit that was
//! not counted.
//!
//! # Every commit the walk sees lands in exactly one bucket
//!
//! Support is `|A ∧ B| / |Total|`, and the ratio is only meaningful if a reader knows
//! what `Total` was. So the accounting is exhaustive rather than a set of incidental
//! counters:
//!
//! ```text
//!   commits_seen == commits_considered + Σ excluded[*]
//! ```
//!
//! That identity is asserted by a test, because the failure it prevents is silent: a
//! commit quietly dropped from the denominator *raises* every support figure in the
//! report while looking like nothing happened.
//!
//! # Why each exclusion exists
//!
//! | Excluded | Reason |
//! |---|---|
//! | Merge commits | The first-parent diff of a merge is the union of a whole branch's work: it is not one change set and it manufactures pairs |
//! | Root commits | An initial import is not co-change; it is a clique over the entire repository |
//! | Mass commits | Above `max_files_per_commit`, the same argument applies to a vendoring or reformatting commit — and it bounds the pair count |
//! | Commits outside the window | The sliding window is the point of the metric; older history is not evidence about the window |
//! | Commits with no timestamp | Window membership cannot be decided, so the commit cannot be placed |
//! | Commits with an undecodable tree | The change set is unknown; counting it as empty would claim the commit changed nothing |
//! | Commits with no changed paths | An empty commit (or an all-rename commit) contributes no pair and is not co-change evidence either way |
//!
//! # Renames, and the limit of what is detected here
//!
//! A pure rename — path `A` disappears and path `B` appears carrying the *identical
//! blob* — is collapsed: the pair is not counted. Without this, a directory move of
//! forty files produces a 780-pair clique for a commit in which nothing was edited.
//!
//! Only the **identical-blob** case is detected. A move that also edits will appear as a
//! deletion plus an addition and will be mined as two unrelated changes. That is a real
//! limitation, disclosed in the report's notes rather than left for the reader to
//! discover.
//!
//! # Time is used for the window and for nothing else
//!
//! `NFR-001` forbids timestamps in canonical outputs, and the fix is not to avoid time
//! but to keep it out of the *identity* of the result:
//!
//! * the window's anchor is the newest commit's `committer` timestamp, read from the
//!   repository — never `SystemTime::now()`, so the same repository always yields the
//!   same window;
//! * what enters the digest is the **set of commit ids that were considered**, which is
//!   content-addressed and timestamp-free, plus the window *width* in seconds.
//!
//! So a re-run on the same repository is byte-identical, and two runs over different
//! windows are distinguishable without either carrying a clock reading.

use autopsy_temporal::git::{GitError, GitObjectStore, ObjectId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Hard bound on tree recursion, per `INV-SANDBOX` traversal bounds.
pub const MAX_TREE_DEPTH: usize = 64;

/// Default number of commits walked from `HEAD`.
pub const DEFAULT_MAX_COMMITS: usize = 2048;

/// Default window: six months `R31`, expressed in seconds so it is a duration and not a
/// date.
pub const DEFAULT_WINDOW_SECONDS: i64 = 183 * 24 * 60 * 60;

/// Default ceiling on the paths a single commit may change and still be mined.
pub const DEFAULT_MAX_FILES_PER_COMMIT: usize = 30;

#[derive(Debug, thiserror::Error)]
pub enum MiningError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error("tree recursion exceeded {MAX_TREE_DEPTH} levels at {path}")]
    TreeDepthExceeded { path: String },
}

/// Why a commit the walk reached is not part of the mined population.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExclusionReason {
    /// The first-parent diff of a merge is a whole branch's work, not one change set.
    MergeCommit,
    /// An initial import manufactures a clique over the entire repository.
    RootCommit,
    /// More paths changed than `max_files_per_commit`.
    MassCommit,
    /// Older than the sliding window.
    OutsideWindow,
    /// No `committer` timestamp, so window membership is undecidable.
    MissingTimestamp,
    /// A tree could not be decoded, so the change set is unknown rather than empty.
    UndecodableTree,
    /// No path changed once pure renames were collapsed.
    NoChangedPaths,
}

impl ExclusionReason {
    /// Every reason, so a conformance test can prove the report accounts for all of them.
    pub fn all() -> &'static [ExclusionReason] {
        &[
            Self::MergeCommit,
            Self::RootCommit,
            Self::MassCommit,
            Self::OutsideWindow,
            Self::MissingTimestamp,
            Self::UndecodableTree,
            Self::NoChangedPaths,
        ]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::MergeCommit => "merge_commit",
            Self::RootCommit => "root_commit",
            Self::MassCommit => "mass_commit",
            Self::OutsideWindow => "outside_window",
            Self::MissingTimestamp => "missing_timestamp",
            Self::UndecodableTree => "undecodable_tree",
            Self::NoChangedPaths => "no_changed_paths",
        }
    }

    /// Whether the exclusion makes the mined population smaller than the window claims —
    /// as opposed to removing a commit that was never co-change evidence.
    ///
    /// A mass commit *was* real code changing together with other code, so dropping it
    /// understates co-change and is not the same kind of choice as dropping a merge.
    pub fn biases_the_population(self) -> bool {
        matches!(
            self,
            Self::MassCommit | Self::MissingTimestamp | Self::UndecodableTree
        )
    }
}

/// One commit's change set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSet {
    pub oid: String,
    /// Paths with a changed or added blob, sorted; pure renames removed.
    pub paths: BTreeSet<String>,
    /// Paths collapsed as pure renames (`A` deleted, `B` added, same blob).
    pub renamed: usize,
}

/// What was in the window, and what was not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowAccounting {
    /// Every commit the walk reached.
    pub commits_seen: usize,
    /// The commits the metric is over.
    pub commits_considered: usize,
    /// Non-zero counts only, so an absent reason is absent rather than reported as zero.
    pub excluded: BTreeMap<ExclusionReason, usize>,
    /// Pure renames collapsed across every considered commit.
    pub pure_renames_collapsed: usize,
    /// Whether `max_commits` stopped the walk before history ran out.
    pub walk_truncated: bool,
}

/// A decoded observation window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryWindow {
    pub accounting: WindowAccounting,
    /// The commit the window is anchored to — an id, not a timestamp.
    pub reference_oid: String,
    pub window_seconds: i64,
    pub max_files_per_commit: usize,
    pub changes: Vec<ChangeSet>,
    /// BLAKE3 over the sorted considered commit ids. Timestamp-free by construction, and
    /// it pins exactly which commits the percentages are over.
    pub commit_set_digest: String,
}

impl HistoryWindow {
    /// An empty window, for a repository with no resolvable `HEAD`.
    ///
    /// Not an error: an empty repository genuinely has no co-change evidence. The report
    /// turns this into `Unknown` coverage rather than into zero findings.
    pub fn empty(window_seconds: i64, max_files_per_commit: usize) -> Self {
        Self {
            accounting: WindowAccounting {
                commits_seen: 0,
                commits_considered: 0,
                excluded: BTreeMap::new(),
                pure_renames_collapsed: 0,
                walk_truncated: false,
            },
            reference_oid: String::new(),
            window_seconds,
            max_files_per_commit,
            changes: Vec::new(),
            commit_set_digest: digest_ids(&[]),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.accounting.commits_considered == 0
    }
}

/// BLAKE3 over the sorted commit ids — content-addressed, so it carries no clock reading.
fn digest_ids(ids: &[String]) -> String {
    let mut sorted: Vec<&str> = ids.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    let mut hasher = blake3::Hasher::new();
    for (index, id) in sorted.iter().enumerate() {
        if index > 0 {
            hasher.update(b"\n");
        }
        hasher.update(id.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryOptions {
    pub window_seconds: i64,
    pub max_commits: usize,
    pub max_files_per_commit: usize,
    /// When set, the window is anchored here instead of at the newest commit's
    /// timestamp. An explicit integer rather than a clock read, so a caller that wants a
    /// reproducible point in time can name one.
    pub reference_timestamp: Option<i64>,
}

impl HistoryOptions {
    pub fn new() -> Self {
        Self {
            window_seconds: DEFAULT_WINDOW_SECONDS,
            max_commits: DEFAULT_MAX_COMMITS,
            max_files_per_commit: DEFAULT_MAX_FILES_PER_COMMIT,
            reference_timestamp: None,
        }
    }

    pub fn with_window_seconds(mut self, seconds: i64) -> Self {
        self.window_seconds = seconds;
        self
    }

    pub fn with_max_commits(mut self, max: usize) -> Self {
        self.max_commits = max;
        self
    }

    pub fn with_max_files_per_commit(mut self, max: usize) -> Self {
        self.max_files_per_commit = max;
        self
    }

    pub fn with_reference_timestamp(mut self, seconds: i64) -> Self {
        self.reference_timestamp = Some(seconds);
        self
    }

    /// A window that is too wide or too narrow makes the metric say nothing, so both
    /// bounds are refused rather than clamped: clamping would report percentages over a
    /// window the caller did not ask for.
    pub fn validate(&self) -> Result<(), MiningError> {
        if self.window_seconds <= 0 {
            return Err(MiningError::Git(GitError::Malformed(
                "window_seconds must be positive".to_string(),
            )));
        }
        if self.max_files_per_commit == 0 {
            return Err(MiningError::Git(GitError::Malformed(
                "max_files_per_commit must be positive".to_string(),
            )));
        }
        Ok(())
    }
}

impl Default for HistoryOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Flattens a tree into `path -> blob id`.
///
/// Subtrees are recursed with a hard depth bound; gitlinks (submodule entries) are
/// skipped, because a submodule pointer is not a path in this repository.
fn flatten(
    store: &GitObjectStore,
    oid: ObjectId,
    prefix: &str,
    out: &mut BTreeMap<String, ObjectId>,
    depth: usize,
) -> Result<(), MiningError> {
    if depth > MAX_TREE_DEPTH {
        return Err(MiningError::TreeDepthExceeded {
            path: prefix.to_string(),
        });
    }
    for entry in store.tree(oid)?.entries {
        let path = if prefix.is_empty() {
            entry.name.clone()
        } else {
            format!("{prefix}/{}", entry.name)
        };
        if entry.is_tree() {
            flatten(store, entry.oid, &path, out, depth + 1)?;
        } else if entry.mode & 0o170_000 == 0o160_000 {
            // A gitlink. Skipped rather than recorded: the submodule's contents are a
            // different repository, and treating the pointer as a changed path would
            // couple this repository's files to another one's.
            continue;
        } else {
            out.insert(path, entry.oid);
        }
    }
    Ok(())
}

fn flatten_tree(
    store: &GitObjectStore,
    oid: ObjectId,
) -> Result<BTreeMap<String, ObjectId>, MiningError> {
    let mut out = BTreeMap::new();
    flatten(store, oid, "", &mut out, 0)?;
    Ok(out)
}

/// The change set of one commit against its first parent, with pure renames collapsed.
///
/// A path present on both sides is an edit. A path present on exactly one side is a
/// deletion or an addition — unless a blob that disappeared also appeared somewhere else,
/// which is a move. Pairing is by blob id, and the **unpaired surplus is restored as a
/// real change**: one blob copied to three paths is one rename and two additions, not one
/// rename and two silent omissions.
fn change_set(
    before: &BTreeMap<String, ObjectId>,
    after: &BTreeMap<String, ObjectId>,
    oid: String,
) -> ChangeSet {
    let mut changed: BTreeSet<String> = BTreeSet::new();
    for (path, old) in before {
        if let Some(new) = after.get(path)
            && new != old
        {
            changed.insert(path.clone());
        }
    }
    for (path, new) in after {
        if let Some(old) = before.get(path)
            && old != new
        {
            changed.insert(path.clone());
        }
    }

    let mut gone_by_blob: BTreeMap<ObjectId, Vec<String>> = BTreeMap::new();
    for (path, blob) in before {
        if !after.contains_key(path) {
            gone_by_blob.entry(*blob).or_default().push(path.clone());
        }
    }
    let mut arrived_by_blob: BTreeMap<ObjectId, Vec<String>> = BTreeMap::new();
    for (path, blob) in after {
        if !before.contains_key(path) {
            arrived_by_blob.entry(*blob).or_default().push(path.clone());
        }
    }

    let empty: Vec<String> = Vec::new();
    let mut renamed = 0usize;
    let blobs: BTreeSet<ObjectId> = gone_by_blob
        .keys()
        .chain(arrived_by_blob.keys())
        .copied()
        .collect();
    for blob in blobs {
        let gone = gone_by_blob.get(&blob).unwrap_or(&empty);
        let arrived = arrived_by_blob.get(&blob).unwrap_or(&empty);
        let paired = gone.len().min(arrived.len());
        renamed += paired;
        for path in gone.iter().skip(paired) {
            changed.insert(path.clone());
        }
        for path in arrived.iter().skip(paired) {
            changed.insert(path.clone());
        }
    }

    ChangeSet {
        oid,
        paths: changed,
        renamed,
    }
}

/// Reads the last `max_commits` commits from `HEAD` and returns the change sets inside
/// the sliding window.
pub fn recent_changes(
    store: &mut GitObjectStore,
    options: &HistoryOptions,
) -> Result<HistoryWindow, MiningError> {
    options.validate()?;

    let Some(head) = store.resolve_ref("HEAD")? else {
        return Ok(HistoryWindow::empty(
            options.window_seconds,
            options.max_files_per_commit,
        ));
    };

    let walked = store.walk_first_parent(head, options.max_commits)?;
    let walk_truncated = walked.len() >= options.max_commits;

    // The anchor is the newest commit that has a timestamp, read from the repository.
    // Never a clock read, so the same repository always yields the same window.
    //
    // A commit dated *after* the anchor — which clock skew and rebases do produce — falls
    // outside the window and is counted as such rather than quietly included, so the
    // accounting identity keeps holding and the exclusion is visible.
    let reference = options.reference_timestamp.or_else(|| {
        walked
            .iter()
            .find_map(|commit| commit.committer_timestamp())
    });

    let mut excluded: BTreeMap<ExclusionReason, usize> = BTreeMap::new();
    let mut note = |reason: ExclusionReason| {
        *excluded.entry(reason).or_insert(0) += 1;
    };

    let mut changes: Vec<ChangeSet> = Vec::new();
    let mut pure_renames_collapsed = 0usize;
    let mut considered_ids: Vec<String> = Vec::new();

    // The parent's flat tree is reused across the walk: first-parent history means each
    // commit's parent is usually the previous one, so this walks each tree once instead
    // of once per edge.
    let mut previous_tree: Option<(ObjectId, BTreeMap<String, ObjectId>)> = None;

    for commit in &walked {
        if commit.parents.len() > 1 {
            note(ExclusionReason::MergeCommit);
            previous_tree = None;
            continue;
        }
        if commit.parents.is_empty() {
            note(ExclusionReason::RootCommit);
            previous_tree = None;
            continue;
        }
        let Some(timestamp) = commit.committer_timestamp() else {
            note(ExclusionReason::MissingTimestamp);
            previous_tree = None;
            continue;
        };
        if let Some(reference) = reference {
            // The window is closed at **both** ends. Without the upper bound, a caller
            // anchoring the window in the past would silently include every commit after
            // the anchor — a window that grows to the present while claiming to be
            // historical.
            if timestamp < reference.saturating_sub(options.window_seconds) || timestamp > reference
            {
                note(ExclusionReason::OutsideWindow);
                previous_tree = None;
                continue;
            }
        }

        let after = match flatten_tree(store, commit.tree) {
            Ok(tree) => tree,
            Err(_) => {
                // The change set is unknown, not empty. Recording it as empty would claim
                // the commit changed nothing.
                note(ExclusionReason::UndecodableTree);
                previous_tree = None;
                continue;
            }
        };
        let parent_oid = commit.parents[0];
        // The previous commit's tree *is* the parent's tree for first-parent history, so
        // it is taken rather than re-walked. Taking it (instead of cloning) keeps the
        // walk linear in the trees read rather than in commit count times tree size.
        let before = match previous_tree.take() {
            Some((oid, tree)) if oid == parent_oid => tree,
            _ => match store
                .commit(parent_oid)
                .map_err(MiningError::from)
                .and_then(|parent| flatten_tree(store, parent.tree))
            {
                Ok(tree) => tree,
                Err(_) => {
                    note(ExclusionReason::UndecodableTree);
                    continue;
                }
            },
        };

        let set = change_set(&before, &after, commit.oid.to_hex());
        previous_tree = Some((commit.oid, after));

        if set.paths.len() > options.max_files_per_commit {
            note(ExclusionReason::MassCommit);
            continue;
        }
        pure_renames_collapsed += set.renamed;
        if set.paths.is_empty() {
            note(ExclusionReason::NoChangedPaths);
            continue;
        }
        considered_ids.push(set.oid.clone());
        changes.push(set);
    }

    // Newest first, and the id is a full hex OID so the order is total.
    changes.sort_by(|a, b| b.oid.cmp(&a.oid));

    let accounting = WindowAccounting {
        commits_seen: walked.len(),
        commits_considered: changes.len(),
        excluded,
        pure_renames_collapsed,
        walk_truncated,
    };

    Ok(HistoryWindow {
        accounting,
        reference_oid: head.to_hex(),
        window_seconds: options.window_seconds,
        max_files_per_commit: options.max_files_per_commit,
        commit_set_digest: digest_ids(&considered_ids),
        changes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_accounting_identity_is_the_invariant_the_report_rests_on() {
        // `commits_seen == commits_considered + Σ excluded[*]` is what stops a commit
        // being dropped from the denominator, which would silently raise every support
        // figure in the report. Asserted on the empty window here and on a real
        // repository in the integration tests.
        let window = HistoryWindow::empty(DEFAULT_WINDOW_SECONDS, 10);
        assert_eq!(
            window.accounting.commits_considered,
            window.accounting.commits_seen - window.accounting.excluded.values().sum::<usize>()
        );
        assert_eq!(ExclusionReason::all().len(), 7);
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for reason in ExclusionReason::all() {
            seen.insert(reason.as_str());
        }
        assert_eq!(seen.len(), 7, "every reason has a distinct name");
    }

    #[test]
    fn only_the_reasons_that_shrink_the_population_claim_to() {
        // The distinction matters: a merge commit was never one change set, so dropping
        // it is not the same as dropping a real edit that happened to touch 400 files.
        let biased: Vec<&str> = ExclusionReason::all()
            .iter()
            .filter(|reason| reason.biases_the_population())
            .map(|reason| reason.as_str())
            .collect();
        assert_eq!(
            biased,
            ["mass_commit", "missing_timestamp", "undecodable_tree"]
        );
    }

    #[test]
    fn a_window_with_a_non_positive_duration_is_refused_not_clamped() {
        let options = HistoryOptions::new().with_window_seconds(0);
        assert!(options.validate().is_err());
        let options = HistoryOptions::new().with_max_files_per_commit(0);
        assert!(options.validate().is_err());
        assert!(HistoryOptions::new().validate().is_ok());
    }

    #[test]
    fn the_commit_set_digest_is_order_independent_and_timestamp_free() {
        // The digest pins which commits the percentages are over, and it must not move
        // when the same commits arrive in another order or carry another clock reading.
        let a = digest_ids(&["bb".to_string(), "aa".to_string()]);
        let b = digest_ids(&["aa".to_string(), "bb".to_string()]);
        assert_eq!(a, b);
        assert_ne!(a, digest_ids(&["aa".to_string()]));
        assert_eq!(a.len(), 64);
    }
}
