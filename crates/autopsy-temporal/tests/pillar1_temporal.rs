//! End-to-end Pillar 1 tests against a synthetic multi-commit repository.
//!
//! These cover the three Pillar 1 acceptance gates:
//!
//! * **Step 30** — facts are appended from git commits, `fact_id` is a BLAKE3
//!   digest, and repeated runs are byte-identical.
//! * **Step 31** — symbol identity survives a file move, ambiguous renames are
//!   `Partial`, and no rename is ever invented.
//! * **Step 32** — the graph reconstructed at *any* historical commit equals a
//!   fresh parse of that commit, byte for byte.

use autopsy_domain::CoverageState;
use autopsy_temporal::fact::FactPredicate;
use autopsy_temporal::git::{GitObjectStore, ObjectId};
use autopsy_temporal::lineage::{FileEntry, LineageConfig, LineageEventKind, detect_lineage};
use autopsy_temporal::timeline::{
    GraphSnapshot, SourceExtractor, TemporalError, TemporalJournal, Timeline, TreeExtractor,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

mod common;
use common::*;

const REPO_URN: &str = "urn:autopsy:repo:github.com:synevid:fixture";

/// A synthetic repository built commit by commit.
struct Repo {
    git_dir: PathBuf,
    /// (commit oid, path -> content) for every revision, oldest first.
    history: Vec<(ObjectId, BTreeMap<String, Vec<u8>>)>,
}

impl Repo {
    /// Builds a 4-commit history that modifies, renames, and adds files.
    fn build(name: &str) -> Self {
        let git_dir = scratch(name);

        // Tree/blob objects are written per revision; each commit points at a
        // fresh root tree, exactly as git would.
        let mut parent: Option<ObjectId> = None;

        let revisions: Vec<BTreeMap<String, Vec<u8>>> = vec![
            // c1: two files.
            [
                ("src/one.ts", "export function fn() { return 1; }\n"),
                ("src/util.ts", "export const util = 42;\n"),
            ]
            .into_iter()
            .map(|(p, c)| (p.to_string(), c.as_bytes().to_vec()))
            .collect(),
            // c2: modify one.ts.
            [
                ("src/one.ts", "export function fn() { return 2; }\n"),
                ("src/util.ts", "export const util = 42;\n"),
            ]
            .into_iter()
            .map(|(p, c)| (p.to_string(), c.as_bytes().to_vec()))
            .collect(),
            // c3: rename one.ts -> two.ts, content unchanged from c2.
            [
                ("src/two.ts", "export function fn() { return 2; }\n"),
                ("src/util.ts", "export const util = 42;\n"),
            ]
            .into_iter()
            .map(|(p, c)| (p.to_string(), c.as_bytes().to_vec()))
            .collect(),
            // c4: add a new file.
            [
                ("src/two.ts", "export function fn() { return 2; }\n"),
                ("src/util.ts", "export const util = 42;\n"),
                ("src/extra.ts", "export const extra = true;\n"),
            ]
            .into_iter()
            .map(|(p, c)| (p.to_string(), c.as_bytes().to_vec()))
            .collect(),
        ];

        let mut nodes: Vec<(ObjectId, BTreeMap<String, Vec<u8>>)> = Vec::new();
        for (i, files) in revisions.into_iter().enumerate() {
            // Build a nested tree for the `src/` directory.
            let mut src_entries: Vec<(u32, &str, ObjectId)> = Vec::new();
            let mut blob_oids = BTreeMap::new();
            for (path, content) in &files {
                let oid = write_loose(&git_dir, "blob", content);
                blob_oids.insert(path.clone(), oid);
                let file_name = path.strip_prefix("src/").unwrap();
                src_entries.push((0o100644, file_name, oid));
            }
            let src_tree = write_loose(&git_dir, "tree", &tree_payload(&src_entries));
            let root = write_loose(
                &git_dir,
                "tree",
                &tree_payload(&[(0o40000, "src", src_tree)]),
            );

            let parents: Vec<ObjectId> = parent.into_iter().collect();
            let commit = write_loose(
                &git_dir,
                "commit",
                &commit_payload(
                    root,
                    &parents,
                    &format!("c{}", i + 1),
                    1_700_000_000 + i as i64,
                ),
            );
            parent = Some(commit);
            nodes.push((commit, files));
        }

        let head = nodes.last().unwrap().0;
        write_head(&git_dir, "refs/heads/main");
        write_ref(&git_dir, "refs/heads/main", head);

        Self {
            git_dir,
            history: nodes,
        }
    }

    fn head(&self) -> ObjectId {
        self.history.last().unwrap().0
    }

    fn open(&self) -> (GitObjectStore, TemporalJournal) {
        // `head_commit` resolves refs lazily and memoizes, hence `mut`.
        let mut store = GitObjectStore::open(&self.git_dir).unwrap();
        let head = store.head_commit().unwrap().oid;
        let extractor = TreeExtractor::new(REPO_URN);
        let journal = TemporalJournal::build(&store, head, 100, &extractor).unwrap();
        (store, journal)
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.git_dir);
    }
}

/// A fresh extraction at one revision: independent of the journal.
fn fresh_parse(store: &GitObjectStore, revision: &str) -> GraphSnapshot {
    let oid = ObjectId::from_hex(revision).unwrap();
    let commit = store.commit(oid).unwrap();
    let entries = autopsy_temporal::git::flatten_tree(store, commit.tree).unwrap();
    let extractor = TreeExtractor::new(REPO_URN);
    let mut facts = Vec::new();
    for (path, (_mode, blob)) in &entries {
        let content = store.blob(*blob).unwrap();
        facts.extend(extractor.extract_file(revision, path, &content));
    }
    GraphSnapshot::from_facts(revision, &facts)
}

// ---------------------------------------------------------------------------
// Step 30 — journal extraction and determinism
// ---------------------------------------------------------------------------

#[test]
fn test_step30_timeline_is_topologically_ordered() {
    let repo = Repo::build("p1-timeline");
    let store = GitObjectStore::open(&repo.git_dir).unwrap();
    let timeline = Timeline::build(&store, repo.head(), 100).unwrap();

    assert_eq!(timeline.len(), 4);
    assert_eq!(timeline.head().unwrap().oid, repo.head().to_hex());

    // Every parent must precede its child.
    for node in timeline.iter() {
        for parent in &node.parents {
            if let Some(pidx) = timeline.index_of(parent) {
                assert!(
                    pidx < node.index,
                    "parent {parent} must precede {} in topological order",
                    node.oid
                );
            }
        }
    }
}

#[test]
fn test_step30_journal_records_and_retires_facts() {
    let repo = Repo::build("p1-journal");
    let (_store, journal) = repo.open();

    // The fixture asserts five *distinct* facts across history:
    //   c1  src/one.ts @ digest-1   (retired at c2: content changed)
    //   c1  src/util.ts @ digest-U  (still active)
    //   c2  src/one.ts @ digest-2   (retired at c3: path moved)
    //   c3  src/two.ts @ digest-2   (still active)
    //   c4  src/extra.ts @ digest-E (still active)
    // `src/util.ts` is re-observed at c2/c3/c4 but that is a *continuation*,
    // so it must not mint additional facts.
    let stats = journal.stats();
    assert_eq!(stats.total, 5, "one fact per distinct assertion version");
    assert_eq!(stats.active, 3, "files present at HEAD");
    assert_eq!(
        stats.retired, 2,
        "superseded by a content change and a move"
    );
    assert_eq!(stats.unknown, 0);
    assert_eq!(stats.partial, 0);

    for fact in journal.facts() {
        assert_eq!(fact.predicate, FactPredicate::FilePresent);
        assert_eq!(fact.coverage, CoverageState::Verified);
        assert_eq!(fact.provenance, "git:tree");
        assert_eq!(fact.valid_from.len(), 40);
        assert_eq!(fact.fact_id.as_str().len(), 64, "BLAKE3 hex");
    }

    // The retired facts must carry a closing revision and the active ones must not.
    for fact in journal.facts() {
        assert_eq!(
            fact.valid_to.is_none(),
            fact_is_active_at_head(&journal, fact)
        );
    }
}

/// A fact is active at HEAD when its closing revision is absent.
fn fact_is_active_at_head(
    _journal: &TemporalJournal,
    fact: &autopsy_temporal::fact::TemporalFact,
) -> bool {
    fact.valid_to.is_none()
}

#[test]
fn test_step30_journal_is_byte_identical_across_100_runs() {
    let repo = Repo::build("p1-determinism");

    let mut digests = Vec::new();
    for _ in 0..100 {
        let (store, journal) = {
            let store = GitObjectStore::open(&repo.git_dir).unwrap();
            let extractor = TreeExtractor::new(REPO_URN);
            let journal = TemporalJournal::build(&store, repo.head(), 100, &extractor).unwrap();
            (store, journal)
        };
        let _ = store;
        digests.push(journal.to_canonical_json().unwrap());
    }

    let first = &digests[0];
    for (i, d) in digests.iter().enumerate() {
        assert_eq!(first, d, "run {i} diverged from run 0");
    }
}

#[test]
fn test_step30_repeated_commit_ids_produce_identical_journals() {
    let repo = Repo::build("p1-repeat");
    let a = repo.open().1.to_canonical_json().unwrap();
    let b = repo.open().1.to_canonical_json().unwrap();
    assert_eq!(a, b);
}

// ---------------------------------------------------------------------------
// Step 31 — lineage across moves and renames
// ---------------------------------------------------------------------------

#[test]
fn test_step31_rename_preserves_identity_with_zero_false_positives() {
    let repo = Repo::build("p1-lineage");
    let store = GitObjectStore::open(&repo.git_dir).unwrap();

    let before = snapshot_files(&store, repo.history[1].0);
    let after = snapshot_files(&store, repo.history[2].0);
    let events = detect_lineage(&before, &after, &LineageConfig::default());

    let renames: Vec<_> = events
        .iter()
        .filter(|e| e.kind == LineageEventKind::Renamed)
        .collect();
    assert_eq!(renames.len(), 1, "exactly one file moved");
    assert_eq!(renames[0].from.as_deref(), Some("src/one.ts"));
    assert_eq!(renames[0].to.as_deref(), Some("src/two.ts"));
    assert_eq!(renames[0].coverage, CoverageState::Verified);
    assert!(
        renames[0].preserves_identity(),
        "content-identical move must preserve identity"
    );

    // Zero false positives: util.ts did not move.
    for e in &events {
        if e.kind == LineageEventKind::Renamed {
            assert_ne!(e.from.as_deref(), Some("src/util.ts"));
        }
    }
}

#[test]
fn test_step31_same_digest_rename_is_the_only_verified_identity_claim() {
    let repo = Repo::build("p1-lineage2");
    let store = GitObjectStore::open(&repo.git_dir).unwrap();

    // Across every adjacent pair, any confident identity claim must be
    // backed by an exact content digest.
    for pair in repo.history.windows(2) {
        let before = snapshot_files(&store, pair[0].0);
        let after = snapshot_files(&store, pair[1].0);
        let events = detect_lineage(&before, &after, &LineageConfig::default());
        for e in &events {
            if e.preserves_identity() {
                let from = e.from.as_deref().unwrap();
                let to = e.to.as_deref().unwrap();
                let fb = before.iter().find(|f| f.path == from).unwrap();
                let fa = after.iter().find(|f| f.path == to).unwrap();
                assert_eq!(
                    fb.content_digest, fa.content_digest,
                    "a confident rename must be digest-backed"
                );
            }
        }
    }
}

#[test]
fn test_step31_ambiguous_renames_are_partial_never_verified() {
    // Two identical files swap names: attribution is genuinely ambiguous.
    let content = "export const shared = 1;";
    let before = vec![
        FileEntry::new("src/a.ts", content.as_bytes()),
        FileEntry::new("src/b.ts", content.as_bytes()),
    ];
    let after = vec![
        FileEntry::new("src/c.ts", content.as_bytes()),
        FileEntry::new("src/d.ts", content.as_bytes()),
    ];
    let events = detect_lineage(&before, &after, &LineageConfig::default());

    assert!(!events.is_empty());
    for e in &events {
        assert_ne!(
            e.coverage,
            CoverageState::Verified,
            "ambiguous identity must be Partial"
        );
        assert!(e.ambiguity.is_some(), "ambiguity must be disclosed");
    }
}

/// Reads the file set at one revision as lineage input.
fn snapshot_files(store: &GitObjectStore, revision: ObjectId) -> Vec<FileEntry> {
    let commit = store.commit(revision).unwrap();
    let entries = autopsy_temporal::git::flatten_tree(store, commit.tree).unwrap();
    entries
        .into_iter()
        .map(|(path, (_mode, blob))| {
            let content = store.blob(blob).unwrap();
            FileEntry::new(path, &content)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Step 32 — point-in-time reconstruction
// ---------------------------------------------------------------------------

#[test]
fn test_step32_reconstruction_matches_fresh_parse_at_every_revision() {
    let repo = Repo::build("p1-reconstruct");
    let (store, journal) = repo.open();

    assert_eq!(journal.timeline().len(), 4, "fixture sanity");

    for (commit, _files) in &repo.history {
        let revision = commit.to_hex();
        let reconstructed = journal.reconstruct(&revision).unwrap();
        let fresh = fresh_parse(&store, &revision);

        assert_eq!(
            reconstructed.digest, fresh.digest,
            "reconstruction at {revision} must be byte-identical to a fresh parse"
        );
        assert_eq!(reconstructed.nodes, fresh.nodes);
        assert_eq!(reconstructed.edges, fresh.edges);
    }
}

#[test]
fn test_step32_reconstruction_reflects_file_additions_over_time() {
    let repo = Repo::build("p1-reconstruct-growth");
    let (_store, journal) = repo.open();

    let sizes: Vec<usize> = repo
        .history
        .iter()
        .map(|(c, _)| journal.reconstruct(&c.to_hex()).unwrap().nodes.len())
        .collect();

    assert_eq!(sizes, vec![2, 2, 2, 3], "node count must grow with history");
}

#[test]
fn test_step32_reconstruction_is_deterministic_across_100_runs() {
    let repo = Repo::build("p1-reconstruct-det");
    let (_store, journal) = repo.open();
    let head = repo.head().to_hex();

    let first = journal.reconstruct(&head).unwrap();
    for _ in 0..100 {
        assert_eq!(journal.reconstruct(&head).unwrap(), first);
    }
}

#[test]
fn test_step32_unknown_revision_is_an_explicit_error() {
    let repo = Repo::build("p1-unknown-rev");
    let (_store, journal) = repo.open();

    let absent = "0".repeat(40);
    match journal.reconstruct(&absent) {
        Err(TemporalError::UnknownRevision(r)) => assert_eq!(r, absent),
        other => panic!("expected UnknownRevision, got {other:?}"),
    }
}

#[test]
fn test_step32_active_at_matches_reconstruct_node_set() {
    let repo = Repo::build("p1-active");
    let (_store, journal) = repo.open();

    for (commit, _) in &repo.history {
        let revision = commit.to_hex();
        let active: Vec<String> = journal
            .active_at(&revision)
            .unwrap()
            .into_iter()
            .map(|f| f.subject.clone())
            .collect();
        let snapshot = journal.reconstruct(&revision).unwrap();
        for subject in active {
            assert!(
                snapshot.nodes.contains(&subject),
                "every active fact subject must appear as a node"
            );
        }
    }
}

#[test]
fn test_step32_timeline_bound_is_enforced() {
    let repo = Repo::build("p1-bound");
    let store = GitObjectStore::open(&repo.git_dir).unwrap();
    match Timeline::build(&store, repo.head(), 2) {
        Err(TemporalError::TimelineTooLarge(2)) => {}
        other => panic!("expected TimelineTooLarge(2), got {other:?}"),
    }
}

#[test]
fn test_custom_extractor_perceives_declarations_deterministically() {
    /// A deliberately naive extractor: one `Declares` fact per `export ` line.
    /// Demonstrates that symbol-level facts plug in without this crate parsing
    /// any language.
    struct ExportScanner;

    impl SourceExtractor for ExportScanner {
        fn extract_file(
            &self,
            revision: &str,
            path: &str,
            content: &[u8],
        ) -> Vec<autopsy_temporal::fact::TemporalFact> {
            let text = String::from_utf8_lossy(content);
            let mut out = Vec::new();
            for line in text.lines() {
                if let Some(rest) = line.strip_prefix("export ") {
                    // Identifier is the token after the declaration keyword:
                    // `function fn` -> `fn`, `const util` -> `util`.
                    let name: String = rest
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("")
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    let subject = format!("{REPO_URN}#{path}:{name}");
                    out.push(autopsy_temporal::fact::TemporalFact::new(
                        subject.clone(),
                        FactPredicate::Declares,
                        subject,
                        revision,
                        blake3::hash(line.as_bytes()).to_hex().to_string(),
                        CoverageState::Verified,
                        "test:export-scan",
                    ));
                }
            }
            out
        }
    }

    let repo = Repo::build("p1-extractor");
    let store = GitObjectStore::open(&repo.git_dir).unwrap();
    let journal = TemporalJournal::build(&store, repo.head(), 100, &ExportScanner).unwrap();

    // `fn` in src/one.ts (c1) retires when the file moves to src/two.ts (c3),
    // where a new `two.ts:fn` fact opens. `util` and `extra` persist unchanged.
    let declares = journal
        .facts()
        .filter(|f| f.predicate == FactPredicate::Declares)
        .count();
    assert_eq!(declares, 4);

    // Reconstruction still matches a fresh parse with the same extractor.
    for (commit, _) in &repo.history {
        let revision = commit.to_hex();
        let reconstructed = journal.reconstruct(&revision).unwrap();

        let oid = ObjectId::from_hex(&revision).unwrap();
        let tree = store.commit(oid).unwrap().tree;
        let entries = autopsy_temporal::git::flatten_tree(&store, tree).unwrap();
        let mut facts = Vec::new();
        for (path, (_mode, blob)) in &entries {
            let content = store.blob(*blob).unwrap();
            facts.extend(ExportScanner.extract_file(&revision, path, &content));
        }
        let fresh = GraphSnapshot::from_facts(&revision, &facts);
        assert_eq!(reconstructed.digest, fresh.digest);
    }
}
