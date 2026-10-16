//! Step 42 gate tests — historical git co-change mining.
//!
//! Gate: *"Mines 6-month commit history `R31`; calculates Support, Confidence, Lift; tags
//! edges with non-causality warning."*
//!
//! The fixtures are real repositories built byte-by-byte. They are **not** created by
//! invoking `git`: the zero-sub-process invariant holds under test as well as in
//! production, and the suite stays hermetic. The byte-level encoders are reused from
//! `autopsy-temporal`'s test toolkit by path, so there is exactly one loose-object encoder
//! in this workspace.
//!
//! Every metric assertion is against a hand-computed value. A test that only checked
//! `support <= 1_000_000` would pass for an engine that returned a constant, and the whole
//! risk in this step is a number that is *plausible* and wrong.

#[path = "../../autopsy-temporal/tests/common/mod.rs"]
mod common;

use autopsy_domain::{CoverageState, EdgeKind};
use autopsy_mining::{
    CoChangeOptions, EdgeClaim, ExclusionReason, HistoryOptions, HistoryWindow, MiningError, mine,
    mine_repository,
};
use autopsy_temporal::git::{GitObjectStore, ObjectId};
use common::{commit_payload, tree_payload, write_head, write_loose, write_ref};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A scratch `.git` unique to this call.
///
/// The shared builder keys its directory on the process id, and every test in a binary
/// shares one process — so two tests asking for the same fixture name would delete each
/// other's objects mid-run. A counter makes each fixture its own directory, which is also
/// what lets a fixture be built by several tests in parallel.
static NEXT_SCRATCH: AtomicUsize = AtomicUsize::new(0);

fn scratch(name: &str) -> PathBuf {
    let unique = NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed);
    common::scratch(&format!("{name}-{unique}"))
}

// ---------------------------------------------------------------------------
// A repository, built by hand
// ---------------------------------------------------------------------------

/// A linear history of flat trees. Flat is enough: co-change is about paths, and a nested
/// tree adds an encoder, not a property.
struct Repo {
    dir: PathBuf,
    head: ObjectId,
    commits: Vec<ObjectId>,
}

impl Repo {
    fn start(name: &str) -> Self {
        let dir = scratch(name);
        write_head(&dir, "refs/heads/main");
        Self {
            dir,
            head: ObjectId([0; 20]),
            commits: Vec::new(),
        }
    }

    fn store(&self) -> GitObjectStore {
        GitObjectStore::open(&self.dir).expect("the fixture is a git directory")
    }

    /// Writes `files` as one commit's complete flat tree, on top of the current head.
    fn commit(&mut self, files: &[(&str, &str)], ts: i64) -> ObjectId {
        let parents: Vec<ObjectId> = if self.commits.is_empty() {
            Vec::new()
        } else {
            vec![self.head]
        };
        self.commit_with(&parents, files, ts)
    }

    fn commit_with(&mut self, parents: &[ObjectId], files: &[(&str, &str)], ts: i64) -> ObjectId {
        let entries: Vec<(u32, String, ObjectId)> = files
            .iter()
            .map(|(name, content)| {
                let blob = write_loose(&self.dir, "blob", content.as_bytes());
                (0o100644, (*name).to_string(), blob)
            })
            .collect();
        let borrowed: Vec<(u32, &str, ObjectId)> = entries
            .iter()
            .map(|(mode, name, oid)| (*mode, name.as_str(), *oid))
            .collect();
        let tree = write_loose(&self.dir, "tree", &tree_payload(&borrowed));
        self.commit_tree(tree, parents, ts)
    }

    fn commit_tree(&mut self, tree: ObjectId, parents: &[ObjectId], ts: i64) -> ObjectId {
        let payload = commit_payload(tree, parents, "fixture", ts);
        let oid = write_loose(&self.dir, "commit", &payload);
        if parents.is_empty() || parents.contains(&self.head) {
            self.head = oid;
        }
        self.commits.push(oid);
        oid
    }

    /// A commit whose committer line carries no timestamp.
    fn commit_without_timestamp(&mut self) -> ObjectId {
        let blob = write_loose(&self.dir, "blob", b"x");
        let tree = write_loose(&self.dir, "tree", &tree_payload(&[(0o100644, "x", blob)]));
        let parent = self.head;
        let payload = format!(
            "tree {tree}\nparent {parent}\nauthor T <t@t.t>\ncommitter T <t@t.t>\n\nno clock\n"
        );
        let oid = write_loose(&self.dir, "commit", payload.as_bytes());
        self.head = oid;
        oid
    }

    fn publish(&mut self) {
        write_ref(&self.dir, "refs/heads/main", self.head);
    }
}

/// The fixture the metric assertions are computed against.
///
/// ```text
///   R  ts=1000  {a="1", b="1"}        root — excluded
///   1  ts=2000  {a="2", b="2"}        a, b
///   2  ts=3000  {a="3", b="2"}        a
///   3  ts=4000  {a="4", b="3"}        a, b
///   4  ts=5000  {a="4", b="3", c="1"} c
///   5  ts=6000  {a="5", b="3", c="2"} a, c
///   6  ts=7000  {a="6", b="3", c="3"} a, c
/// ```
///
/// So over the six considered commits: `a` in 5, `b` in 2, `c` in 3, `(a, b)` in 2,
/// `(a, c)` in 2, `(b, c)` in 0.
fn metric_repo() -> Repo {
    let mut repo = Repo::start("mining-metrics");
    repo.commit(&[("a.txt", "1"), ("b.txt", "1")], 1_000);
    repo.commit(&[("a.txt", "2"), ("b.txt", "2")], 2_000);
    repo.commit(&[("a.txt", "3"), ("b.txt", "2")], 3_000);
    repo.commit(&[("a.txt", "4"), ("b.txt", "3")], 4_000);
    repo.commit(&[("a.txt", "4"), ("b.txt", "3"), ("c.txt", "1")], 5_000);
    repo.commit(&[("a.txt", "5"), ("b.txt", "3"), ("c.txt", "2")], 6_000);
    repo.commit(&[("a.txt", "6"), ("b.txt", "3"), ("c.txt", "3")], 7_000);
    repo.publish();
    repo
}

fn edge_between<'a>(
    report: &'a autopsy_mining::CoChangeReport,
    left: &str,
    right: &str,
) -> &'a autopsy_mining::CoChangeEdge {
    report
        .edges
        .iter()
        .find(|edge| (edge.a.as_str(), edge.b.as_str()) == (left, right))
        .unwrap_or_else(|| {
            panic!(
                "no edge {left}..{right} in {:?}",
                report
                    .edges
                    .iter()
                    .map(|edge| (&edge.a, &edge.b))
                    .collect::<Vec<_>>()
            )
        })
}

// ---------------------------------------------------------------------------
// The gate
// ---------------------------------------------------------------------------

#[test]
fn gate_support_confidence_and_lift_are_the_hand_computed_values() {
    let repo = metric_repo();
    let mut store = repo.store();
    let report = mine_repository(&mut store, &HistoryOptions::new(), &CoChangeOptions::new())
        .expect("the fixture mines");

    assert_eq!(report.window.commits_considered, 6);
    assert_eq!(report.paths_with_changes, 3);

    // Support(A, B) = |A ∧ B| / |Total| = 2/6
    // Confidence(A → B) = |A ∧ B| / |A|  = 2/5
    // Confidence(B → A) = |A ∧ B| / |B|  = 2/2
    // Lift = P(A ∧ B) / (P(A)·P(B)) = (2/6) / ((5/6)·(2/6)) = 1.2
    let ab = edge_between(&report, "a.txt", "b.txt");
    assert_eq!(ab.commits_together, 2);
    assert_eq!(ab.commits_with_a, 5);
    assert_eq!(ab.commits_with_b, 2);
    assert_eq!(ab.total_commits, 6);
    assert_eq!(ab.support_ppm, 333_333);
    assert_eq!(ab.confidence_a_to_b_ppm, 400_000);
    assert_eq!(ab.confidence_b_to_a_ppm, 1_000_000);
    assert_eq!(ab.lift_ppm, 1_200_000, "2/6 / (10/36) = 12/10");

    // Lift = (2/6) / ((5/6)·(3/6)) = 0.8
    let ac = edge_between(&report, "a.txt", "c.txt");
    assert_eq!(ac.commits_together, 2);
    assert_eq!(ac.commits_with_b, 3);
    assert_eq!(ac.support_ppm, 333_333);
    assert_eq!(ac.confidence_a_to_b_ppm, 400_000);
    assert_eq!(ac.confidence_b_to_a_ppm, 666_666);
    assert_eq!(ac.lift_ppm, 800_000);
}

#[test]
fn gate_a_pair_below_the_floor_is_counted_but_not_reported() {
    let repo = metric_repo();
    let mut store = repo.store();
    // `(b, c)` never co-changed, so raising the floor below 2 must still not produce it;
    // and lowering it to 1 must produce it only if it existed.
    let report = mine_repository(
        &mut store,
        &HistoryOptions::new(),
        &CoChangeOptions::new().with_min_pair_commits(1),
    )
    .expect("mines");
    assert!(
        !report
            .edges
            .iter()
            .any(|edge| (edge.a.as_str(), edge.b.as_str()) == ("b.txt", "c.txt")),
        "a pair that never co-changed cannot appear at any floor"
    );
    assert_eq!(
        report.pairs_below_threshold, 0,
        "every observed pair met a floor of one"
    );

    let strict = mine_repository(
        &mut store,
        &HistoryOptions::new(),
        &CoChangeOptions::new().with_min_pair_commits(3),
    )
    .expect("mines");
    assert!(strict.edges.is_empty(), "no pair reached three commits");
    assert_eq!(strict.pairs_below_threshold, 2);
}

#[test]
fn gate_every_edge_is_tagged_as_temporal_co_occurrence_and_not_a_dependency() {
    let repo = metric_repo();
    let mut store = repo.store();
    let report = mine_repository(&mut store, &HistoryOptions::new(), &CoChangeOptions::new())
        .expect("mines");

    assert!(!report.edges.is_empty(), "the fixture must produce edges");
    for edge in &report.edges {
        assert_eq!(edge.claim, EdgeClaim::TemporalCoOccurrenceOnly);
        assert_eq!(edge.kind, EdgeKind::StatisticalCoChange);
        // The type-level distinction the design asks for.
        assert_ne!(edge.kind, EdgeKind::Calls);
    }
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("not a dependency")),
        "the boundary is stated in prose as well: {:?}",
        report.notes
    );

    // And it survives being sliced out of the report: one edge on its own still carries it.
    let one_edge = serde_json::to_value(&report.edges[0]).expect("serializes");
    assert_eq!(one_edge["claim"], "temporal_co_occurrence_only");
    assert_eq!(one_edge["kind"], "statistical_co_change");
}

#[test]
fn gate_the_population_is_fully_accounted_for() {
    let repo = metric_repo();
    let mut store = repo.store();
    let report = mine_repository(&mut store, &HistoryOptions::new(), &CoChangeOptions::new())
        .expect("mines");

    // `commits_seen == commits_considered + Σ excluded[*]`. A commit dropped from the
    // denominator *raises* every support figure in the report, and looks like nothing
    // happened — so the identity is asserted, not assumed.
    let excluded: usize = report.window.excluded.values().sum();
    assert_eq!(
        report.window.commits_seen,
        report.window.commits_considered + excluded
    );
    assert_eq!(report.window.commits_seen, 7, "six plus the root import");
    assert_eq!(
        report.window.excluded.get(&ExclusionReason::RootCommit),
        Some(&1),
        "an initial import is a clique over the whole repository, not co-change"
    );
}

#[test]
fn gate_an_empty_repository_is_unknown_never_a_report_of_zero_couplings() {
    let dir = scratch("mining-empty");
    write_head(&dir, "refs/heads/main");
    let mut store = GitObjectStore::open(&dir).expect("opens");

    let report = mine_repository(&mut store, &HistoryOptions::new(), &CoChangeOptions::new())
        .expect("mines");
    assert_eq!(report.coverage, CoverageState::Unknown);
    assert!(report.edges.is_empty());
    assert!(
        report.notes.iter().any(|note| note.contains("no commits")),
        "{:?}",
        report.notes
    );
}

// ---------------------------------------------------------------------------
// The window, and what falls outside it
// ---------------------------------------------------------------------------

#[test]
fn gate_commits_outside_the_window_are_excluded_and_counted() {
    let repo = metric_repo();
    let mut store = repo.store();
    // The anchor is the newest commit's timestamp (7000), not a clock reading, so a
    // 2000-second window reaches back to 5000.
    let report = mine_repository(
        &mut store,
        &HistoryOptions::new().with_window_seconds(2_000),
        &CoChangeOptions::new(),
    )
    .expect("mines");

    assert_eq!(
        report.window.excluded.get(&ExclusionReason::OutsideWindow),
        Some(&3),
        "commits at 2000, 3000 and 4000"
    );
    assert_eq!(report.window.commits_considered, 3);
    let ac = edge_between(&report, "a.txt", "c.txt");
    assert_eq!(ac.total_commits, 3);
    assert_eq!(ac.support_ppm, 666_666, "2/3");
    assert_eq!(
        ac.confidence_a_to_b_ppm, 1_000_000,
        "both `a` commits touched `c`"
    );
}

#[test]
fn an_explicit_reference_timestamp_anchors_the_window_without_a_clock() {
    let repo = metric_repo();
    let mut store = repo.store();
    let report = mine_repository(
        &mut store,
        &HistoryOptions::new()
            .with_window_seconds(2_000)
            .with_reference_timestamp(4_000),
        &CoChangeOptions::new(),
    )
    .expect("mines");
    // Anchored at 4000 with a 2000-second window: the window is the closed interval
    // [2000, 4000], so the commits at 5000, 6000 and 7000 are *after* the anchor and are
    // excluded. Without the upper bound they would count, and the window would silently
    // grow to the present while claiming to be historical.
    assert_eq!(
        report.window.commits_considered, 3,
        "ts 2000, 3000 and 4000"
    );
    assert_eq!(
        report.window.excluded.get(&ExclusionReason::OutsideWindow),
        Some(&3),
        "ts 5000, 6000 and 7000"
    );
}

#[test]
fn the_same_repository_yields_the_same_window_across_100_runs() {
    let repo = metric_repo();
    let mut store = repo.store();
    let first = mine_repository(&mut store, &HistoryOptions::new(), &CoChangeOptions::new())
        .expect("mines");
    for run in 1..100 {
        let again = mine_repository(&mut store, &HistoryOptions::new(), &CoChangeOptions::new())
            .expect("mines");
        assert_eq!(again.digest, first.digest, "run {run} differed");
        assert_eq!(again, first, "run {run} differed structurally");
    }
}

#[test]
fn the_digest_carries_no_timestamp_but_does_carry_the_commit_set() {
    let repo = metric_repo();
    let mut store = repo.store();
    let report = mine_repository(&mut store, &HistoryOptions::new(), &CoChangeOptions::new())
        .expect("mines");

    let text = serde_json::to_string(&report).expect("serializes");
    for stamp in ["1000", "2000", "3000", "4000", "5000", "6000", "7000"] {
        assert!(
            !text.contains(&format!("\"ts\":{stamp}")),
            "a wall-clock reading reached the report"
        );
    }
    assert_eq!(report.window.commit_set_digest.len(), 64);
    assert_eq!(
        report.window.reference_oid.len(),
        40,
        "the anchor is a content-addressed id, not a date"
    );

    // The control: a different window is a different report, so the digest is not constant.
    let narrower = mine_repository(
        &mut store,
        &HistoryOptions::new().with_window_seconds(2_000),
        &CoChangeOptions::new(),
    )
    .expect("mines");
    assert_ne!(report.digest, narrower.digest);
    assert_ne!(
        report.window.commit_set_digest,
        narrower.window.commit_set_digest
    );
}

#[test]
fn a_window_with_a_non_positive_duration_is_refused() {
    let repo = metric_repo();
    let mut store = repo.store();
    let error = mine_repository(
        &mut store,
        &HistoryOptions::new().with_window_seconds(0),
        &CoChangeOptions::new(),
    )
    .expect_err("a zero-width window is not a window");
    assert!(
        matches!(error, MiningError::Git(_)),
        "expected a refusal, got {error:?}"
    );
}

// ---------------------------------------------------------------------------
// Exclusions, each with its own reason
// ---------------------------------------------------------------------------

#[test]
fn gate_a_merge_commit_is_excluded_because_its_diff_is_not_one_change_set() {
    let mut repo = Repo::start("mining-merge");
    repo.commit(&[("a.txt", "1")], 1_000);
    let first = repo.commit(&[("a.txt", "2")], 2_000);
    repo.commit(&[("a.txt", "3")], 3_000);
    // A merge whose first-parent diff would be the whole branch's work.
    repo.commit_with(
        &[repo.head, first],
        &[("a.txt", "4"), ("b.txt", "1"), ("c.txt", "1")],
        4_000,
    );
    repo.publish();

    let mut store = repo.store();
    let report = mine_repository(&mut store, &HistoryOptions::new(), &CoChangeOptions::new())
        .expect("mines");
    assert_eq!(
        report.window.excluded.get(&ExclusionReason::MergeCommit),
        Some(&1)
    );
    assert!(
        !report
            .edges
            .iter()
            .any(|edge| (edge.a.as_str(), edge.b.as_str()) == ("b.txt", "c.txt")),
        "the merge would have manufactured that pair"
    );
}

#[test]
fn gate_a_mass_commit_is_excluded_and_degrades_coverage() {
    let mut repo = Repo::start("mining-mass");
    repo.commit(&[("a.txt", "1"), ("b.txt", "1")], 1_000);
    repo.commit(&[("a.txt", "2"), ("b.txt", "2")], 2_000);
    // Five paths at once: a vendoring or reformatting commit.
    repo.commit(
        &[
            ("a.txt", "3"),
            ("b.txt", "3"),
            ("c.txt", "1"),
            ("d.txt", "1"),
            ("e.txt", "1"),
        ],
        3_000,
    );
    repo.publish();

    let mut store = repo.store();
    let report = mine_repository(
        &mut store,
        &HistoryOptions::new().with_max_files_per_commit(4),
        // `a.txt` and `b.txt` change together in the mass commit too, but a five-path
        // commit is not one change set, so it is not evidence — which drops the pair to a
        // single observation, so the floor has to be lowered for it to be reported at all.
        &CoChangeOptions::new().with_min_pair_commits(1),
    )
    .expect("mines");
    assert_eq!(
        report.window.excluded.get(&ExclusionReason::MassCommit),
        Some(&1)
    );
    assert_eq!(
        report.window.excluded.get(&ExclusionReason::RootCommit),
        Some(&1)
    );
    let ab = edge_between(&report, "a.txt", "b.txt");
    assert_eq!(
        ab.commits_together, 1,
        "only the 2000 commit, not the mass commit"
    );
    assert_eq!(
        report.window.commits_considered, 1,
        "the mass commit leaves one minable commit, so every ratio is over one"
    );
    assert_eq!(
        report.coverage,
        CoverageState::Partial,
        "dropping a real edit understates co-change, so the report is not Verified"
    );
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("lower bound rather than the whole population")),
        "{:?}",
        report.notes
    );
}

#[test]
fn a_commit_changed_by_more_paths_than_the_bound_is_excluded_at_the_bound() {
    let mut repo = Repo::start("mining-bound");
    repo.commit(&[("a.txt", "1")], 1_000);
    repo.commit(&[("a.txt", "2"), ("b.txt", "1")], 2_000);
    repo.publish();

    let mut store = repo.store();
    // Exactly at the bound: mined. An off-by-one the other way would drop ordinary commits.
    let at_bound = mine_repository(
        &mut store,
        &HistoryOptions::new().with_max_files_per_commit(2),
        &CoChangeOptions::new(),
    )
    .expect("mines");
    assert_eq!(
        at_bound.window.excluded.get(&ExclusionReason::MassCommit),
        None,
        "a commit changing exactly the bound is not over it: {:?}",
        at_bound.window.excluded
    );
    assert_eq!(at_bound.window.commits_considered, 1);

    // One over the bound: excluded.
    let over_bound = mine_repository(
        &mut store,
        &HistoryOptions::new().with_max_files_per_commit(1),
        &CoChangeOptions::new(),
    )
    .expect("mines");
    assert_eq!(
        over_bound.window.excluded.get(&ExclusionReason::MassCommit),
        Some(&1)
    );
    assert_eq!(over_bound.window.commits_considered, 0);
    assert!(over_bound.edges.is_empty());
    assert_eq!(
        over_bound.coverage,
        CoverageState::Unknown,
        "bounding every commit out leaves no population at all"
    );
}

#[test]
fn a_commit_with_no_timestamp_cannot_be_placed_in_the_window_and_is_counted() {
    let mut repo = Repo::start("mining-nots");
    repo.commit(&[("a.txt", "1")], 1_000);
    repo.commit(&[("a.txt", "2")], 2_000);
    repo.commit_without_timestamp();
    repo.publish();

    let mut store = repo.store();
    let report = mine_repository(&mut store, &HistoryOptions::new(), &CoChangeOptions::new())
        .expect("mines");
    assert_eq!(
        report
            .window
            .excluded
            .get(&ExclusionReason::MissingTimestamp),
        Some(&1)
    );
    assert_eq!(
        report.coverage,
        CoverageState::Partial,
        "a commit that cannot be placed leaves the population uncertain"
    );
}

#[test]
fn a_commit_with_an_undecodable_tree_is_not_counted_as_having_changed_nothing() {
    let mut repo = Repo::start("mining-bad-tree");
    repo.commit(&[("a.txt", "1")], 1_000);
    repo.commit(&[("a.txt", "2")], 2_000);
    // A commit whose tree object is not in the store.
    let missing = ObjectId([0xab; 20]);
    repo.commit_tree(missing, &[repo.head], 3_000);
    repo.publish();

    let mut store = repo.store();
    let report = mine_repository(&mut store, &HistoryOptions::new(), &CoChangeOptions::new())
        .expect("mines");
    assert_eq!(
        report
            .window
            .excluded
            .get(&ExclusionReason::UndecodableTree),
        Some(&1)
    );
    assert!(report.notes.iter().any(|note| note.contains("lower bound")));
}

#[test]
fn a_pure_rename_is_collapsed_so_a_directory_move_is_not_a_clique() {
    let mut repo = Repo::start("mining-rename");
    repo.commit(&[("a.txt", "content")], 1_000);
    // `a.txt` disappears and `moved.txt` appears with the identical blob: a move.
    repo.commit(&[("moved.txt", "content")], 2_000);
    repo.commit(&[("moved.txt", "changed")], 3_000);
    repo.publish();

    let mut store = repo.store();
    let report = mine_repository(&mut store, &HistoryOptions::new(), &CoChangeOptions::new())
        .expect("mines");
    assert_eq!(report.window.pure_renames_collapsed, 1);
    assert_eq!(
        report.window.excluded.get(&ExclusionReason::NoChangedPaths),
        Some(&1),
        "the rename commit changed no content"
    );
    assert!(
        !report.notes.iter().any(|note| note.contains("duplicate")),
        "collapsing a rename is a correction, not a loss"
    );
    assert!(
        report.notes.iter().any(|note| note.contains("pure rename")),
        "and the count is disclosed: {:?}",
        report.notes
    );
}

#[test]
fn a_copy_restores_the_unpaired_surplus_rather_than_dropping_it() {
    let mut repo = Repo::start("mining-copy");
    repo.commit(&[("a.txt", "content")], 1_000);
    // One path removed, three added, all the same blob: one move and two copies. `zip`
    // would silently drop the surplus, so the count has to be checked, not the mechanism.
    repo.commit(
        &[
            ("m1.txt", "content"),
            ("m2.txt", "content"),
            ("m3.txt", "content"),
        ],
        2_000,
    );
    repo.publish();

    let mut store = repo.store();
    let report = mine_repository(&mut store, &HistoryOptions::new(), &CoChangeOptions::new())
        .expect("mines");
    assert_eq!(
        report.window.pure_renames_collapsed, 1,
        "one path was moved; the other two are new files"
    );
    let change = &report.window.commits_considered;
    assert_eq!(
        *change, 1,
        "the commit is a change set, being two additions"
    );
}

#[test]
fn a_submodule_pointer_is_not_a_path_in_this_repository() {
    let dir = scratch("mining-gitlink");
    write_head(&dir, "refs/heads/main");
    let blob = write_loose(&dir, "blob", b"content");
    // A gitlink entry, as a submodule appears in a tree, alongside a real blob.
    let tree = write_loose(
        &dir,
        "tree",
        &tree_payload(&[
            (0o100644, "a.txt", blob),
            (0o160000, "sub", ObjectId([0xcd; 20])),
        ]),
    );
    let commit = write_loose(&dir, "commit", &commit_payload(tree, &[], "root", 1_000));
    write_ref(&dir, "refs/heads/main", commit);

    let mut store = GitObjectStore::open(&dir).expect("opens");
    let window = autopsy_mining::recent_changes(&mut store, &HistoryOptions::new()).expect("reads");
    // The root commit is excluded, so the tree is reached only through accounting; the
    // property under test is that the walk did not fail on the gitlink.
    assert_eq!(window.accounting.commits_seen, 1);
    assert_eq!(
        window.accounting.excluded.get(&ExclusionReason::RootCommit),
        Some(&1)
    );
}

// ---------------------------------------------------------------------------
// Honest coverage
// ---------------------------------------------------------------------------

#[test]
fn a_population_of_one_commit_cannot_support_a_ratio() {
    let mut repo = Repo::start("mining-single");
    repo.commit(&[("a.txt", "1")], 1_000);
    repo.commit(&[("a.txt", "2"), ("b.txt", "1")], 2_000);
    repo.publish();

    let mut store = repo.store();
    let report = mine_repository(&mut store, &HistoryOptions::new(), &CoChangeOptions::new())
        .expect("mines");
    assert_eq!(report.window.commits_considered, 1);
    assert_eq!(
        report.coverage,
        CoverageState::Partial,
        "one commit makes every ratio trivially 1 and meaningless"
    );
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("too small to support a measurement"))
    );
}

#[test]
fn an_edge_resting_on_two_commits_is_reported_with_partial_coverage_while_the_report_is_verified() {
    // The distinction matters: the *measurement* covered the population completely, and
    // this *number* rests on two commits. Both statements are true and they are not the
    // same statement, so they get the same field at different scopes.
    let repo = metric_repo();
    let mut store = repo.store();
    let report = mine_repository(&mut store, &HistoryOptions::new(), &CoChangeOptions::new())
        .expect("mines");

    assert_eq!(report.coverage, CoverageState::Verified);
    for edge in &report.edges {
        assert_eq!(edge.commits_together, 2);
        assert_eq!(
            edge.coverage,
            CoverageState::Partial,
            "lift is dominated by small counts `R32`"
        );
    }
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("fewer than three commits")),
        "{:?}",
        report.notes
    );
}

#[test]
fn a_reached_pair_bound_is_disclosed_as_a_lower_bound() {
    let repo = metric_repo();
    let mut store = repo.store();
    let report = mine_repository(
        &mut store,
        &HistoryOptions::new(),
        &CoChangeOptions::new().with_max_pairs(1),
    )
    .expect("mines");
    assert!(report.pairs_truncated);
    assert_eq!(
        report.coverage,
        CoverageState::Partial,
        "a truncated pair set is a lower bound and must not read as complete"
    );
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("reached its bound")),
        "{:?}",
        report.notes
    );
}

#[test]
fn a_reached_history_bound_is_disclosed_as_an_incomplete_window() {
    let repo = metric_repo();
    let mut store = repo.store();
    let report = mine_repository(
        &mut store,
        &HistoryOptions::new().with_max_commits(3),
        &CoChangeOptions::new(),
    )
    .expect("mines");
    assert!(report.window.walk_truncated);
    assert_eq!(report.coverage, CoverageState::Partial);
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("commit bound")),
        "{:?}",
        report.notes
    );
}

#[test]
fn a_pair_bound_that_cannot_be_reached_does_not_silently_drop_anything() {
    // The control for the truncation test: with a bound above the pair count, nothing is
    // truncated and the report is not degraded.
    let repo = metric_repo();
    let mut store = repo.store();
    let report = mine_repository(
        &mut store,
        &HistoryOptions::new(),
        &CoChangeOptions::new().with_max_pairs(1_000),
    )
    .expect("mines");
    assert!(!report.pairs_truncated);
    assert_eq!(report.coverage, CoverageState::Verified);
}

#[test]
fn more_edges_than_the_report_bound_are_counted_and_degrade_coverage() {
    let repo = metric_repo();
    let mut store = repo.store();
    let report = mine_repository(
        &mut store,
        &HistoryOptions::new(),
        &CoChangeOptions::new().with_max_edges(1),
    )
    .expect("mines");
    assert_eq!(report.edges.len(), 1);
    assert_eq!(
        report.coverage,
        CoverageState::Partial,
        "a truncated edge list is not the whole answer"
    );
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("strongest by lift")),
        "{:?}",
        report.notes
    );
    // The strongest edge is kept, so the truncation is at least not arbitrary.
    assert_eq!(report.edges[0].lift_ppm, 1_200_000);
}

// ---------------------------------------------------------------------------
// The pure engine, without a repository
// ---------------------------------------------------------------------------

fn synthetic(change_sets: &[&[&str]]) -> HistoryWindow {
    let mut window = HistoryWindow::empty(1_000_000, 30);
    window.accounting.commits_seen = change_sets.len();
    window.accounting.commits_considered = change_sets.len();
    window.reference_oid = "0".repeat(40);
    window.changes = change_sets
        .iter()
        .enumerate()
        .map(|(index, paths)| autopsy_mining::ChangeSet {
            oid: format!("{index:040}"),
            paths: paths.iter().map(|path| (*path).to_string()).collect(),
            renamed: 0,
        })
        .collect();
    window.commit_set_digest = "0".repeat(64);
    window
}

#[test]
fn the_pure_engine_is_a_function_of_its_input() {
    // `mine` takes no repository, so the metric behaviour is testable and reproducible
    // without one — and the repository path is a thin reader on top of it.
    let window = synthetic(&[&["a.txt", "b.txt"], &["a.txt", "b.txt"]]);
    let report = mine(&window, &CoChangeOptions::new());
    let edge = edge_between(&report, "a.txt", "b.txt");
    assert_eq!(edge.commits_together, 2);
    assert_eq!(edge.total_commits, 2);
    assert_eq!(edge.support_ppm, 1_000_000);
    assert_eq!(edge.confidence_a_to_b_ppm, 1_000_000);
    assert_eq!(
        edge.lift_ppm, 1_000_000,
        "everything co-changes, so lift is 1"
    );
    assert_eq!(report.paths_never_coupled, 0);
}

#[test]
fn a_path_that_never_changed_with_another_is_counted_rather_than_dropped() {
    let window = synthetic(&[&["a.txt", "b.txt"], &["solo.txt"]]);
    let report = mine(&window, &CoChangeOptions::new());
    assert_eq!(report.paths_with_changes, 3);
    assert_eq!(report.paths_never_coupled, 1, "solo.txt changed alone");
    // And the control the other way: every path coupled means the count is zero.
    let all = mine(
        &synthetic(&[&["a.txt", "b.txt"], &["a.txt", "b.txt"]]),
        &CoChangeOptions::new(),
    );
    assert_eq!(all.paths_never_coupled, 0);
}

#[test]
fn lift_is_symmetric_and_confidence_is_not() {
    // Reported per direction because they answer different questions, and the pair itself
    // is stored with `a < b` so the edge's identity does not depend on the direction asked.
    let window = synthetic(&[
        &["common.txt", "rare.txt"],
        &["common.txt"],
        &["common.txt"],
        &["common.txt"],
    ]);
    let report = mine(&window, &CoChangeOptions::new().with_min_pair_commits(1));
    let edge = edge_between(&report, "common.txt", "rare.txt");
    assert_eq!(
        edge.confidence_towards("common.txt"),
        Some(250_000),
        "1 of 4"
    );
    assert_eq!(
        edge.confidence_towards("rare.txt"),
        Some(1_000_000),
        "1 of 1"
    );
    assert_eq!(edge.confidence_towards("absent.txt"), None);
    assert_eq!(
        edge.interval_towards("rare.txt"),
        Some((
            edge.confidence_b_to_a_lower_ppm,
            edge.confidence_b_to_a_upper_ppm
        ))
    );
    assert_eq!(edge.interval_towards("absent.txt"), None);
}

#[test]
fn the_confidence_interval_brackets_the_confidence_and_is_reported_per_direction() {
    let window = synthetic(&[
        &["a.txt", "b.txt"],
        &["a.txt", "b.txt"],
        &["a.txt", "b.txt"],
        &["a.txt", "b.txt"],
        &["a.txt"],
    ]);
    let report = mine(&window, &CoChangeOptions::new());
    let edge = edge_between(&report, "a.txt", "b.txt");
    assert_eq!(edge.confidence_a_to_b_ppm, 800_000, "4 of 5");
    assert!(
        edge.confidence_a_to_b_lower_ppm < edge.confidence_a_to_b_ppm
            && edge.confidence_a_to_b_ppm < edge.confidence_a_to_b_upper_ppm,
        "{} .. {} does not bracket 800000",
        edge.confidence_a_to_b_lower_ppm,
        edge.confidence_a_to_b_upper_ppm
    );
    // `b` only ever changed with `a`, so its interval's upper bound is certainty and its
    // lower bound is a real, non-zero statement about a sample of four.
    assert_eq!(edge.confidence_b_to_a_ppm, 1_000_000);
    assert_eq!(edge.confidence_b_to_a_upper_ppm, 1_000_000);
    assert!(edge.confidence_b_to_a_lower_ppm > 0);
    assert!(edge.confidence_b_to_a_lower_ppm < 1_000_000);
}

#[test]
fn the_interval_is_about_the_window_and_says_nothing_about_causality() {
    let window = synthetic(&[&["a.txt", "b.txt"], &["a.txt", "b.txt"]]);
    let report = mine(&window, &CoChangeOptions::new());
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("says nothing about causality")),
        "an interval invites exactly one misreading, and it is named: {:?}",
        report.notes
    );
}

#[test]
fn an_empty_window_is_unknown_and_reports_no_edges() {
    let report = mine(&HistoryWindow::empty(1_000, 30), &CoChangeOptions::new());
    assert_eq!(report.coverage, CoverageState::Unknown);
    assert!(report.edges.is_empty());
    assert_eq!(report.paths_with_changes, 0);
    assert_eq!(report.digest.len(), 64);
}

#[test]
fn the_report_round_trips_through_json() {
    let repo = metric_repo();
    let mut store = repo.store();
    let report = mine_repository(&mut store, &HistoryOptions::new(), &CoChangeOptions::new())
        .expect("mines");
    let text = serde_json::to_string(&report).expect("serializes");
    let back: autopsy_mining::CoChangeReport = serde_json::from_str(&text).expect("deserializes");
    assert_eq!(back, report);
}

#[test]
fn the_record_order_does_not_change_the_report() {
    // Two windows holding the same change sets in opposite order must produce one report,
    // or the digest would depend on the walk order rather than on the history.
    let forwards = synthetic(&[&["a.txt", "b.txt"], &["a.txt", "b.txt"], &["c.txt"]]);
    let mut backwards = forwards.clone();
    backwards.changes.reverse();
    assert_eq!(
        mine(&forwards, &CoChangeOptions::new()).digest,
        mine(&backwards, &CoChangeOptions::new()).digest
    );
}

#[test]
fn a_path_appearing_twice_in_one_commit_is_impossible_and_would_not_double_count() {
    // `ChangeSet::paths` is a set, so a commit contributes at most one to any pair. The
    // assertion pins the consequence: one commit can never by itself clear a floor of two.
    let window = synthetic(&[&["a.txt", "b.txt", "a.txt"]]);
    let report = mine(&window, &CoChangeOptions::new());
    assert!(report.edges.is_empty(), "one commit is one observation");
    assert_eq!(report.pairs_below_threshold, 1);
}

#[test]
fn the_exclusion_vocabulary_is_complete_and_each_reason_has_a_name() {
    let mut names: BTreeMap<&str, usize> = BTreeMap::new();
    for reason in ExclusionReason::all() {
        *names.entry(reason.as_str()).or_insert(0) += 1;
    }
    assert_eq!(names.len(), ExclusionReason::all().len());
    assert!(names.values().all(|count| *count == 1));
    // Serde's spelling and the accessor's spelling are one string, or a report would say
    // one thing and a log another.
    for reason in ExclusionReason::all() {
        assert_eq!(
            serde_json::to_value(reason).unwrap(),
            serde_json::Value::String(reason.as_str().to_string())
        );
    }
}
