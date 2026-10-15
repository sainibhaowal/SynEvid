//! Deterministic timeline and point-in-time graph reconstruction (Steps 30 & 32).
//!
//! # Deterministic commit ordering
//! Commit timestamps are attacker-controlled and vary between machines, so
//! they must not influence output (`NFR-001`). The timeline is therefore a
//! topological order (every parent precedes its children) with object ids as
//! the tie-breaker. Two clones of the same repository always produce the same
//! ordering.
//!
//! # The reconstruction invariant
//! The journal is built by replaying commits oldest-first and, at each
//! revision, re-observing the facts that are still true. Anything not
//! re-observed is closed at that revision. Consequently the set of facts
//! active at revision `C_k` is *by construction* exactly the set extracted
//! from `C_k`, which is what makes point-in-time reconstruction byte-for-byte
//! identical to a fresh parse.

use crate::fact::{FactId, FactPredicate, JournalStats, TemporalFact};
use crate::git::{GitObjectStore, ObjectId, flatten_tree};
use autopsy_domain::CoverageState;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TemporalError {
    #[error("git: {0}")]
    Git(#[from] crate::git::GitError),
    #[error("revision `{0}` is not present in the analyzed timeline")]
    UnknownRevision(String),
    #[error("timeline exceeds the configured bound of {0} commits")]
    TimelineTooLarge(usize),
    #[error("serialization: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Bound on how many commits a single analysis will walk.
///
/// Guards the traversal resource bound required by `INV-SANDBOX-004`.
pub const DEFAULT_MAX_COMMITS: usize = 4096;

/// One commit in the deterministic topological order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitNode {
    /// Full 40-character commit id.
    pub oid: String,
    /// Parent commit ids, in recorded order.
    pub parents: Vec<String>,
    /// Root tree object id.
    pub tree: String,
    /// Position in the topological order; 0 is the oldest analyzed commit.
    pub index: usize,
}

/// A deterministically ordered view of commit history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timeline {
    commits: Vec<CommitNode>,
    by_oid: BTreeMap<String, usize>,
}

impl Timeline {
    /// Builds a topological timeline from `head`, walking at most `max_commits`.
    ///
    /// Ordering is parents-before-children with object-id tie-breaking, so it
    /// is independent of commit dates, machine locale, and insertion order.
    pub fn build(
        store: &GitObjectStore,
        head: ObjectId,
        max_commits: usize,
    ) -> Result<Self, TemporalError> {
        // 1. Collect the reachable commit set (bounded, cycle-safe).
        let mut nodes: BTreeMap<String, (Vec<String>, String)> = BTreeMap::new();
        let mut queue = vec![head];
        let mut seen: BTreeSet<String> = BTreeSet::new();
        while let Some(oid) = queue.pop() {
            let hex = oid.to_hex();
            if !seen.insert(hex.clone()) {
                continue;
            }
            if seen.len() > max_commits {
                return Err(TemporalError::TimelineTooLarge(max_commits));
            }
            let commit = store.commit(oid)?;
            queue.extend(commit.parents.iter().copied());
            nodes.insert(
                hex,
                (
                    commit.parents.iter().map(|p| p.to_hex()).collect(),
                    commit.tree.to_hex(),
                ),
            );
        }

        // 2. Kahn's algorithm restricted to the reachable set. A parent
        //    outside the set (shallow clone) is treated as already satisfied.
        let mut in_degree: BTreeMap<&str, usize> = BTreeMap::new();
        let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (oid, (parents, _)) in &nodes {
            let internal = parents.iter().filter(|p| nodes.contains_key(*p)).count();
            in_degree.insert(oid.as_str(), internal);
            for p in parents.iter().filter(|p| nodes.contains_key(*p)) {
                children.entry(p.as_str()).or_default().push(oid.as_str());
            }
        }

        let mut ready: BTreeSet<&str> = in_degree
            .iter()
            .filter(|(_, d)| **d == 0)
            .map(|(o, _)| *o)
            .collect();
        let mut order: Vec<String> = Vec::with_capacity(nodes.len());
        while let Some(next) = ready.iter().next().copied() {
            ready.remove(next);
            order.push(next.to_string());
            if let Some(kids) = children.get(next) {
                let mut kids: Vec<&str> = kids.clone();
                kids.sort_unstable();
                for kid in kids {
                    if let Some(d) = in_degree.get_mut(kid) {
                        *d -= 1;
                        if *d == 0 {
                            ready.insert(kid);
                        }
                    }
                }
            }
        }

        if order.len() != nodes.len() {
            // A cycle is impossible in a commit DAG; reaching here means the
            // objects are corrupt. Report rather than silently truncate.
            return Err(TemporalError::Git(crate::git::GitError::Malformed(
                format!(
                    "commit graph is cyclic or incomplete: ordered {} of {}",
                    order.len(),
                    nodes.len()
                ),
            )));
        }

        let mut commits = Vec::with_capacity(order.len());
        let mut by_oid = BTreeMap::new();
        for (index, oid) in order.into_iter().enumerate() {
            let (parents, tree) = nodes.get(&oid).expect("collected above").clone();
            by_oid.insert(oid.clone(), index);
            commits.push(CommitNode {
                oid,
                parents,
                tree,
                index,
            });
        }

        Ok(Self { commits, by_oid })
    }

    /// Number of commits in the timeline.
    pub fn len(&self) -> usize {
        self.commits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commits.is_empty()
    }

    /// Position of a revision, or `None` when it is outside the timeline.
    pub fn index_of(&self, oid: &str) -> Option<usize> {
        self.by_oid.get(oid).copied()
    }

    /// Commit at a topological position.
    pub fn at(&self, index: usize) -> Option<&CommitNode> {
        self.commits.get(index)
    }

    /// Oldest-first iteration.
    pub fn iter(&self) -> impl Iterator<Item = &CommitNode> {
        self.commits.iter()
    }

    /// The most recent commit in the timeline.
    pub fn head(&self) -> Option<&CommitNode> {
        self.commits.last()
    }
}

/// Produces the temporal facts observable at a single revision.
///
/// Implementors must be pure functions of their inputs: no clock, no
/// environment, no randomness. Any construct they cannot interpret must be
/// reported with `CoverageState::Unknown` or `Partial`, never silently
/// dropped (`FR-014`, `FR-030`).
pub trait SourceExtractor {
    /// Extracts facts for one file at one revision.
    fn extract_file(&self, revision: &str, path: &str, content: &[u8]) -> Vec<TemporalFact>;
}

/// Built-in extractor recording file presence and content identity.
///
/// This is the language-agnostic layer of Pillar 1: it establishes *which
/// files existed with which bytes* at every revision. Symbol-level facts
/// (declarations, calls, imports) are contributed by adapters implementing
/// [`SourceExtractor`], so this crate stays free of language-specific parsing.
#[derive(Debug, Clone, Default)]
pub struct TreeExtractor {
    /// Repository URN prefix for canonical subject ids.
    pub repo_urn: String,
}

impl TreeExtractor {
    pub fn new(repo_urn: impl Into<String>) -> Self {
        Self {
            repo_urn: repo_urn.into(),
        }
    }
}

impl SourceExtractor for TreeExtractor {
    fn extract_file(&self, revision: &str, path: &str, content: &[u8]) -> Vec<TemporalFact> {
        let subject = format!("{}#{path}", self.repo_urn);
        let digest = blake3::hash(content).to_hex().to_string();
        vec![TemporalFact::new(
            subject,
            FactPredicate::FilePresent,
            format!("blake3:{digest}"),
            revision,
            digest,
            CoverageState::Verified,
            "git:tree",
        )]
    }
}

/// A reconstructed, content-addressed view of the semantic graph at one revision.
///
/// Both field order and element ordering are canonical, so two snapshots are
/// equal exactly when their canonical JSON is byte-identical.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphSnapshot {
    pub revision: String,
    /// Sorted entity identifiers present at this revision.
    pub nodes: BTreeSet<String>,
    /// Sorted `(subject, predicate, object)` triples.
    pub edges: BTreeSet<EdgeTriple>,
    /// BLAKE3 over the canonical JSON of `nodes` and `edges`.
    pub digest: String,
}

/// A canonical `(subject, predicate, object)` assertion.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EdgeTriple {
    pub subject: String,
    pub predicate: FactPredicate,
    pub object: String,
}

impl GraphSnapshot {
    /// Builds a snapshot from an active fact set.
    pub fn from_facts(revision: impl Into<String>, facts: &[TemporalFact]) -> Self {
        let mut nodes = BTreeSet::new();
        let mut edges = BTreeSet::new();
        for fact in facts {
            nodes.insert(fact.subject.clone());
            if fact.object.starts_with("urn:") || fact.object.contains('#') {
                nodes.insert(fact.object.clone());
            }
            edges.insert(EdgeTriple {
                subject: fact.subject.clone(),
                predicate: fact.predicate,
                object: fact.object.clone(),
            });
        }
        let digest = compute_snapshot_digest(&nodes, &edges);
        Self {
            revision: revision.into(),
            nodes,
            edges,
            digest,
        }
    }
}

/// Hashes the canonical form of a snapshot body.
fn compute_snapshot_digest(nodes: &BTreeSet<String>, edges: &BTreeSet<EdgeTriple>) -> String {
    let mut hasher = blake3::Hasher::new();
    // Explicit, self-delimiting encoding: never rely on serde formatting here.
    hasher.update(b"nodes\n");
    for n in nodes {
        hasher.update(n.as_bytes());
        hasher.update(b"\n");
    }
    hasher.update(b"edges\n");
    for e in edges {
        hasher.update(e.subject.as_bytes());
        hasher.update(b"\x00");
        hasher.update(e.predicate.as_str().as_bytes());
        hasher.update(b"\x00");
        hasher.update(e.object.as_bytes());
        hasher.update(b"\n");
    }
    hasher.finalize().to_hex().to_string()
}

/// Append-only journal of temporal facts over a deterministic timeline.
#[derive(Debug, Clone)]
pub struct TemporalJournal {
    timeline: Timeline,
    facts: BTreeMap<FactId, TemporalFact>,
    /// Topological index of the commit that opened each fact.
    opened_at: BTreeMap<FactId, usize>,
    /// Topological index of the commit that closed each fact, when closed.
    closed_at: BTreeMap<FactId, usize>,
}

impl TemporalJournal {
    /// Replays history, extracting and ageing facts.
    pub fn build<E: SourceExtractor>(
        store: &GitObjectStore,
        head: ObjectId,
        max_commits: usize,
        extractor: &E,
    ) -> Result<Self, TemporalError> {
        let timeline = Timeline::build(store, head, max_commits)?;

        let mut facts: BTreeMap<FactId, TemporalFact> = BTreeMap::new();
        let mut opened_at: BTreeMap<FactId, usize> = BTreeMap::new();
        let mut closed_at: BTreeMap<FactId, usize> = BTreeMap::new();

        // Currently-open facts keyed by their identity triple.
        let mut open: BTreeMap<(String, FactPredicate, String), FactId> = BTreeMap::new();

        for node in timeline.iter() {
            let revision = &node.oid;
            let tree_oid = ObjectId::from_hex(&node.tree)?;
            let entries = flatten_tree(store, tree_oid)?;

            let mut observed: BTreeMap<(String, FactPredicate, String), FactId> = BTreeMap::new();
            for (path, (_mode, blob_oid)) in &entries {
                let content = store.blob(*blob_oid)?;
                for fact in extractor.extract_file(revision, path, &content) {
                    let key = (fact.subject.clone(), fact.predicate, fact.object.clone());
                    // A re-observation of an already-open assertion is a
                    // *continuation*, not a new fact. `fact_id` embeds
                    // `valid_from`, so minting it again here would fabricate a
                    // fresh fact at every subsequent commit.
                    match open.get(&key) {
                        Some(existing_id) => {
                            observed.insert(key, existing_id.clone());
                        }
                        None => {
                            opened_at.insert(fact.fact_id.clone(), node.index);
                            observed.insert(key, fact.fact_id.clone());
                            facts.insert(fact.fact_id.clone(), fact);
                        }
                    }
                }
            }

            // Close facts that this revision no longer asserts.
            for (key, fact_id) in &open {
                if !observed.contains_key(key)
                    && let Some(fact) = facts.get_mut(fact_id)
                    && fact.valid_to.is_none()
                {
                    fact.valid_to = Some(revision.clone());
                    closed_at.insert(fact_id.clone(), node.index);
                }
            }

            open = observed;
        }

        Ok(Self {
            timeline,
            facts,
            opened_at,
            closed_at,
        })
    }

    /// The deterministic timeline backing this journal.
    pub fn timeline(&self) -> &Timeline {
        &self.timeline
    }

    /// Every fact, ordered by id.
    pub fn facts(&self) -> impl Iterator<Item = &TemporalFact> {
        self.facts.values()
    }

    /// Journal size and coverage breakdown.
    pub fn stats(&self) -> JournalStats {
        let mut stats = JournalStats::default();
        for fact in self.facts.values() {
            stats.total += 1;
            if fact.is_active() {
                stats.active += 1;
            } else {
                stats.retired += 1;
            }
            match fact.coverage {
                CoverageState::Partial => stats.partial += 1,
                CoverageState::Unknown | CoverageState::Unsupported => stats.unknown += 1,
                CoverageState::Verified => {}
            }
        }
        stats
    }

    /// Facts whose interval contains the given revision.
    ///
    /// A fact is active at `k` when `opened_at <= k` and it is either still
    /// open or was closed strictly after `k` (half-open intervals).
    pub fn active_at(&self, revision: &str) -> Result<Vec<&TemporalFact>, TemporalError> {
        let k = self
            .timeline
            .index_of(revision)
            .ok_or_else(|| TemporalError::UnknownRevision(revision.to_string()))?;

        let mut out = Vec::new();
        for (id, fact) in &self.facts {
            let opened = self.opened_at.get(id).copied().unwrap_or(0);
            if opened > k {
                continue;
            }
            match self.closed_at.get(id) {
                Some(closed) if *closed <= k => continue,
                _ => out.push(fact),
            }
        }
        Ok(out)
    }

    /// Reconstructs the semantic graph at an arbitrary historical revision.
    ///
    /// The result is byte-for-byte equal to a fresh extraction at the same
    /// revision, because the replay closes every fact that revision does not
    /// re-observe.
    pub fn reconstruct(&self, revision: &str) -> Result<GraphSnapshot, TemporalError> {
        let active = self.active_at(revision)?;
        Ok(GraphSnapshot::from_facts(
            revision,
            &active.into_iter().cloned().collect::<Vec<_>>(),
        ))
    }

    /// Serializes the journal to canonical JSON (sorted, LF-terminated).
    pub fn to_canonical_json(&self) -> Result<String, TemporalError> {
        let facts: Vec<&TemporalFact> = self.facts().collect();
        let mut json = serde_json::to_string_pretty(&facts)?;
        if !json.ends_with('\n') {
            json.push('\n');
        }
        Ok(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edge_triple_ordering_is_canonical() {
        let mut set = BTreeSet::new();
        set.insert(EdgeTriple {
            subject: "b".into(),
            predicate: FactPredicate::Calls,
            object: "z".into(),
        });
        set.insert(EdgeTriple {
            subject: "a".into(),
            predicate: FactPredicate::Calls,
            object: "y".into(),
        });
        let first = set.iter().next().unwrap();
        assert_eq!(first.subject, "a");
    }

    #[test]
    fn test_snapshot_digest_is_stable_and_content_sensitive() {
        let facts = vec![TemporalFact::new(
            "urn:a#x.ts",
            FactPredicate::FilePresent,
            "blake3:1",
            "c1",
            "e",
            CoverageState::Verified,
            "p",
        )];
        let s1 = GraphSnapshot::from_facts("c1", &facts);
        let s2 = GraphSnapshot::from_facts("c1", &facts);
        assert_eq!(s1.digest, s2.digest);

        let other = vec![TemporalFact::new(
            "urn:a#y.ts",
            FactPredicate::FilePresent,
            "blake3:1",
            "c1",
            "e",
            CoverageState::Verified,
            "p",
        )];
        assert_ne!(s1.digest, GraphSnapshot::from_facts("c1", &other).digest);
    }

    #[test]
    fn test_snapshot_includes_declared_object_nodes_only_for_identities() {
        let facts = vec![
            TemporalFact::new(
                "urn:a#x.ts:fn",
                FactPredicate::Calls,
                "urn:a#y.ts:other",
                "c1",
                "e",
                CoverageState::Verified,
                "p",
            ),
            TemporalFact::new(
                "urn:a#x.ts",
                FactPredicate::FilePresent,
                "blake3:abc",
                "c1",
                "e",
                CoverageState::Verified,
                "p",
            ),
        ];
        let snap = GraphSnapshot::from_facts("c1", &facts);
        assert!(snap.nodes.contains("urn:a#x.ts:fn"));
        assert!(snap.nodes.contains("urn:a#y.ts:other"));
        // A content digest is a value, not an entity, so it is not a node.
        assert!(!snap.nodes.contains("blake3:abc"));
    }

    #[test]
    fn test_tree_extractor_is_a_pure_function() {
        let ex = TreeExtractor::new("urn:autopsy:repo:github.com:o:r");
        let a = ex.extract_file("c1", "src/a.ts", b"content");
        let b = ex.extract_file("c1", "src/a.ts", b"content");
        assert_eq!(a, b);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].predicate, FactPredicate::FilePresent);
    }

    #[test]
    fn test_tree_extractor_changes_with_content() {
        let ex = TreeExtractor::new("urn:a");
        let a = ex.extract_file("c1", "a.ts", b"one");
        let b = ex.extract_file("c1", "a.ts", b"two");
        assert_ne!(a[0].fact_id, b[0].fact_id);
    }

    #[test]
    fn test_journal_stats_default_is_zero() {
        let stats = JournalStats::default();
        assert_eq!(stats.total, 0);
        assert_eq!(stats.active, 0);
    }
}
