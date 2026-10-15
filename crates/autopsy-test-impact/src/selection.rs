//! Conservative test selection (Step 35) and advisory ranking (Step 36).
//!
//! # Safety invariant
//! The promotion gate is **0% false negatives**: no test that could detect a
//! regression may be omitted. Selection is therefore conservative by
//! construction:
//!
//! * every changed symbol with `CoverageState::Unknown` semantics (dynamic
//!   dispatch, `eval`, reflection) forces `RunAllTests`;
//! * a suite whose mapping is not `Verified` **cannot be ruled out**, so it is
//!   included rather than excluded — this is the key asymmetry that makes the
//!   0%-false-negative gate hold;
//! * a change set with no positively-mapped suite forces `RunAllTests`, because
//!   "no test covers this" and "we failed to map this" are indistinguishable
//!   from inside the engine and the honest reading of that ambiguity is to run
//!   everything.
//!
//! # Step 36 is advisory only
//! [`PredictiveRanking`] never influences [`SelectionPlan`]. `select_affected_tests`
//! takes no ranking input, so there is no code path by which a probabilistic
//! score could gate a merge. The types enforce the rule rather than relying on
//! a comment.

use crate::inventory::{TestInventory, TestSuite};
use crate::lcov::CoverageMatrix;
use autopsy_domain::CoverageState;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SelectionError {
    #[error("change set exceeds the configured bound of {0} seeds")]
    TooManySeeds(usize),
}

/// Bound on change-set seeds, guarding traversal cost (`INV-SANDBOX-004`).
pub const MAX_SEEDS: usize = 100_000;

/// Reverse-reachability oracle over the call graph.
///
/// Implemented by `autopsy-impact` / `autopsy-graph` in production; the
/// selection logic depends only on this trait, which keeps the 0%-false-negative
/// argument testable in isolation.
pub trait Reachability {
    /// All nodes that can reach any seed, including the seeds themselves.
    fn reverse_reachable(&self, seeds: &BTreeSet<String>) -> BTreeSet<String>;
}

/// An oracle that reports nothing reachable. Used when no graph is available.
///
/// Selecting nothing from an empty closure is only safe because
/// [`select_affected_tests`] treats a mapping-less suite as un-rulable-out.
pub struct NoReachability;

impl Reachability for NoReachability {
    fn reverse_reachable(&self, seeds: &BTreeSet<String>) -> BTreeSet<String> {
        seeds.clone()
    }
}

/// Oracle over an explicit set of `target -> callers` edges.
#[derive(Debug, Clone, Default)]
pub struct EdgeMapReachability {
    /// callee -> callers
    callers: BTreeMap<String, BTreeSet<String>>,
}

impl EdgeMapReachability {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that `caller` calls `callee`.
    pub fn add_edge(&mut self, caller: impl Into<String>, callee: impl Into<String>) {
        self.callers
            .entry(callee.into())
            .or_default()
            .insert(caller.into());
    }
}

impl Reachability for EdgeMapReachability {
    fn reverse_reachable(&self, seeds: &BTreeSet<String>) -> BTreeSet<String> {
        let mut seen: BTreeSet<String> = seeds.clone();
        let mut stack: Vec<String> = seeds.iter().cloned().collect();
        while let Some(node) = stack.pop() {
            if let Some(callers) = self.callers.get(&node) {
                for caller in callers {
                    if seen.insert(caller.clone()) {
                        stack.push(caller.clone());
                    }
                }
            }
        }
        seen
    }
}

/// The change being evaluated for test impact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSetSummary {
    /// Changed source file paths, sorted.
    pub changed_files: BTreeSet<String>,
    /// Changed symbol identifiers, sorted.
    pub changed_symbols: BTreeSet<String>,
    /// Symbols whose semantics the analyzer could not resolve.
    pub unknown_symbols: BTreeSet<String>,
    /// Aggregate coverage of the change set itself.
    pub coverage: CoverageState,
}

impl Default for ChangeSetSummary {
    /// A change set that was not explicitly analyzed is `Unknown`, not
    /// `Verified`. The default rounds toward forcing a full run, which is the
    /// only safe direction: an unanalyzed change set must never narrow the
    /// selection.
    fn default() -> Self {
        Self {
            changed_files: BTreeSet::new(),
            changed_symbols: BTreeSet::new(),
            unknown_symbols: BTreeSet::new(),
            coverage: CoverageState::Unknown,
        }
    }
}

impl ChangeSetSummary {
    /// Covers the common case of a fully-analyzable change.
    pub fn verified() -> Self {
        Self {
            coverage: CoverageState::Verified,
            ..Default::default()
        }
    }

    /// Derives a change set from a file list, asserting full analysis.
    pub fn from_verified_files(paths: impl IntoIterator<Item = String>) -> Self {
        Self {
            changed_files: paths.into_iter().collect(),
            coverage: CoverageState::Verified,
            ..Default::default()
        }
    }
}

/// What the engine could not narrow, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SelectionFallback {
    /// Selection was possible.
    None,
    /// Only within-package scoping could be justified.
    RunPackageTests {
        packages: Vec<String>,
        reason: String,
    },
    /// Everything must run.
    RunAllTests { reason: String },
}

impl SelectionFallback {
    /// True when the engine could not produce a narrow selection.
    pub fn is_full_run(&self) -> bool {
        matches!(self, Self::RunAllTests { .. })
    }
}

/// A single justification attached to the plan.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SelectionReason {
    /// Stable machine-readable code.
    pub code: String,
    /// Human-readable detail.
    pub detail: String,
}

/// The deterministic, evidence-backed test selection plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionPlan {
    /// Test case ids to run, sorted.
    pub selected_tests: BTreeSet<String>,
    /// Suites to run, sorted.
    pub selected_suites: BTreeSet<String>,
    /// Source files judged affected, sorted.
    pub affected_sources: BTreeSet<String>,
    /// Suites that could not be ruled out, sorted.
    pub unrulable_out_suites: BTreeSet<String>,
    pub fallback: SelectionFallback,
    pub coverage: CoverageState,
    pub reasons: Vec<SelectionReason>,
    /// Total suites in the inventory, for the "we narrowed X of Y" report.
    pub total_suites: usize,
    /// Total test cases in the inventory.
    pub total_tests: usize,
    pub digest: String,
}

impl SelectionPlan {
    /// Fraction of suites selected, in `0.0..=1.0`. `1.0` when the inventory is
    /// empty or a full run was forced.
    pub fn suite_narrowing_ratio(&self) -> f64 {
        if self.total_suites == 0 || self.fallback.is_full_run() {
            return 1.0;
        }
        self.selected_suites.len() as f64 / self.total_suites as f64
    }
}

/// Computes the affected test set.
///
/// `AffectedTests = union over changed symbols of ReverseReachability(s), intersected with the inventory`.
pub fn select_affected_tests(
    change_set: &ChangeSetSummary,
    inventory: &TestInventory,
    coverage: &CoverageMatrix,
    reachability: &dyn Reachability,
) -> Result<SelectionPlan, SelectionError> {
    let mut reasons: Vec<SelectionReason> = Vec::new();

    // 1. Guard the traversal bound.
    let seed_count = change_set.changed_symbols.len() + change_set.changed_files.len();
    if seed_count > MAX_SEEDS {
        return Err(SelectionError::TooManySeeds(seed_count));
    }

    // 2. Dynamic/unresolvable semantics cannot be reasoned about laterally.
    if !change_set.unknown_symbols.is_empty() {
        let mut sample: Vec<&str> = change_set
            .unknown_symbols
            .iter()
            .map(String::as_str)
            .collect();
        sample.truncate(5);
        let reason = format!(
            "{} changed symbol(s) have unresolved semantics (dynamic dispatch, eval, or reflection): {}",
            change_set.unknown_symbols.len(),
            sample.join(", ")
        );
        return Ok(full_run_plan(
            change_set,
            inventory,
            "unknown_changed_symbol_semantics",
            &reason,
            CoverageState::Unknown,
        ));
    }

    if change_set.coverage == CoverageState::Unknown {
        return Ok(full_run_plan(
            change_set,
            inventory,
            "change_set_coverage_unknown",
            "the change set itself could not be fully analyzed",
            CoverageState::Unknown,
        ));
    }

    // 3. Reverse reachability, intersected with the inventory's known files.
    let seeds: BTreeSet<String> = change_set
        .changed_symbols
        .iter()
        .chain(change_set.changed_files.iter())
        .cloned()
        .collect();
    let reachable = reachability.reverse_reachable(&seeds);

    // A reachable entity is an affected source when it names a known file, or
    // when a known file is a prefix of it (`path:symbol` / `path::sym` forms).
    let affected_sources: BTreeSet<String> = reachable
        .iter()
        .filter_map(|entity| match_source_of(entity, &inventory.known_files))
        .collect();

    // 4. Positive evidence: static imports and observed coverage.
    let mut selected_suites: BTreeSet<String> = BTreeSet::new();
    let mut static_matches = 0usize;
    let mut dynamic_matches = 0usize;

    for suite in inventory.suites.values() {
        if suite
            .mapped_sources
            .iter()
            .any(|src| affected_sources.contains(src))
        {
            selected_suites.insert(suite.suite_id.clone());
            static_matches += 1;
        }
    }
    for path in &affected_sources {
        for suite_id in coverage.suites_touching(path) {
            if selected_suites.insert(suite_id.to_string()) {
                dynamic_matches += 1;
            }
        }
    }

    if static_matches > 0 {
        reasons.push(SelectionReason {
            code: "static_import_mapping".to_string(),
            detail: format!("{static_matches} suite(s) declare imports from an affected source"),
        });
    }
    if dynamic_matches > 0 {
        reasons.push(SelectionReason {
            code: "observed_coverage".to_string(),
            detail: format!("{dynamic_matches} suite(s) executed affected sources"),
        });
    }

    // 5. The conservative step: any suite we could not map is un-rulable-out.
    let mut unrulable: BTreeSet<String> = BTreeSet::new();
    for suite in inventory.suites.values() {
        if suite.coverage != CoverageState::Verified && !selected_suites.contains(&suite.suite_id) {
            unrulable.insert(suite.suite_id.clone());
            selected_suites.insert(suite.suite_id.clone());
        }
    }
    if !unrulable.is_empty() {
        reasons.push(SelectionReason {
            code: "unmapped_suite_cannot_be_excluded".to_string(),
            detail: format!(
                "{} suite(s) have non-Verified mappings and cannot be ruled out",
                unrulable.len()
            ),
        });
    }

    // 6. Decide the honest fallback.
    let fallback = if inventory.is_empty() {
        SelectionFallback::None
    } else if selected_suites.is_empty() {
        SelectionFallback::RunAllTests {
            reason: "no test suite could be positively mapped to the change set; \
                     'no test covers this' cannot be distinguished from a mapping failure"
                .to_string(),
        }
    } else if coverage.is_empty() && static_matches > 0 {
        SelectionFallback::RunPackageTests {
            packages: packages_of(&affected_sources),
            reason: "no observed coverage was ingested; selection rests on static imports alone"
                .to_string(),
        }
    } else {
        SelectionFallback::None
    };

    // 7. Expand the final selection: on a forced full run, everything is in.
    let mut final_suites = selected_suites;
    if fallback.is_full_run() {
        for suite in inventory.suites.keys() {
            final_suites.insert(suite.clone());
        }
    }

    let mut selected_tests: BTreeSet<String> = BTreeSet::new();
    for suite_id in &final_suites {
        if let Some(suite) = inventory.suites.get(suite_id) {
            push_cases(suite, &mut selected_tests);
        }
    }
    // A suite with coverage evidence but no parsed cases still runs; represent
    // it as a synthetic suite-level case so the plan is never empty for a
    // selected suite.
    for suite_id in &final_suites {
        if let Some(suite) = inventory.suites.get(suite_id)
            && suite.test_cases.is_empty()
        {
            selected_tests.insert(format!("{suite_id}::@suite"));
        }
    }

    // An empty matrix carries `coverage == Unknown`, so "no data" and
    // "degraded data" collapse into the same honest answer: Partial.
    let coverage_state = if fallback.is_full_run() {
        CoverageState::Unknown
    } else if !unrulable.is_empty() || coverage.coverage != CoverageState::Verified {
        CoverageState::Partial
    } else {
        CoverageState::Verified
    };

    reasons.push(plan_summary_reason(
        final_suites.len(),
        inventory.len(),
        selected_tests.len(),
        inventory.test_case_count(),
    ));
    reasons.sort();

    let mut plan = SelectionPlan {
        selected_tests,
        selected_suites: final_suites,
        affected_sources,
        unrulable_out_suites: unrulable,
        fallback,
        coverage: coverage_state,
        reasons,
        total_suites: inventory.len(),
        total_tests: inventory.test_case_count(),
        digest: String::new(),
    };
    plan.digest = compute_plan_digest(&plan);
    Ok(plan)
}

/// The mandatory summary reason.
///
/// Emitted by **both** the narrowed and the forced-full-run paths. A forced
/// full run needs this most: it is exactly the case where a reader must be able
/// to see that nothing was narrowed, and "we narrowed 0 of 40" is only visible
/// through this entry.
fn plan_summary_reason(
    selected_suites: usize,
    total_suites: usize,
    selected_tests: usize,
    total_tests: usize,
) -> SelectionReason {
    SelectionReason {
        code: "plan_summary".to_string(),
        detail: format!(
            "selected {selected_suites} of {total_suites} suite(s) and {selected_tests} of {total_tests} case(s)"
        ),
    }
}

/// Builds a plan that forces every test to run.
fn full_run_plan(
    _change_set: &ChangeSetSummary,
    inventory: &TestInventory,
    code: &str,
    detail: &str,
    coverage: CoverageState,
) -> SelectionPlan {
    let mut selected_tests = BTreeSet::new();
    let mut selected_suites = BTreeSet::new();
    for (id, suite) in &inventory.suites {
        selected_suites.insert(id.clone());
        push_cases(suite, &mut selected_tests);
        if suite.test_cases.is_empty() {
            selected_tests.insert(format!("{id}::@suite"));
        }
    }
    let mut reasons = vec![
        SelectionReason {
            code: code.to_string(),
            detail: detail.to_string(),
        },
        plan_summary_reason(
            selected_suites.len(),
            inventory.len(),
            selected_tests.len(),
            inventory.test_case_count(),
        ),
    ];
    reasons.sort();
    let mut plan = SelectionPlan {
        selected_tests,
        selected_suites,
        affected_sources: BTreeSet::new(),
        unrulable_out_suites: BTreeSet::new(),
        fallback: SelectionFallback::RunAllTests {
            reason: detail.to_string(),
        },
        coverage,
        reasons,
        total_suites: inventory.len(),
        total_tests: inventory.test_case_count(),
        digest: String::new(),
    };
    plan.digest = compute_plan_digest(&plan);
    plan
}

/// Adds a suite's cases to the selection.
fn push_cases(suite: &TestSuite, out: &mut BTreeSet<String>) {
    for case in &suite.test_cases {
        out.insert(case.case_id(&suite.suite_id));
    }
}

/// Resolves a graph entity id to a known source file path.
///
/// Handles bare paths, `path:symbol`, and `path::symbol` forms.
fn match_source_of(entity: &str, known_files: &BTreeSet<String>) -> Option<String> {
    if known_files.contains(entity) {
        return Some(entity.to_string());
    }
    for separator in [":", "::", "#"] {
        if let Some((path, _symbol)) = entity.split_once(separator)
            && known_files.contains(path)
        {
            return Some(path.to_string());
        }
    }
    None
}

/// Top-level directory of each affected source, sorted and deduplicated.
fn packages_of(sources: &BTreeSet<String>) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for src in sources {
        let pkg = src
            .split_once('/')
            .map(|(dir, _)| dir.to_string())
            .unwrap_or_else(|| ".".to_string());
        out.insert(pkg);
    }
    out.into_iter().collect()
}

/// Canonical plan digest per `03_4.2_STANDARDS_SCHEMAS_VERSIONING.md` §3.2.
fn compute_plan_digest(plan: &SelectionPlan) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"affected\n");
    for s in &plan.affected_sources {
        hasher.update(s.as_bytes());
        hasher.update(b"\n");
    }
    hasher.update(b"tests\n");
    for t in &plan.selected_tests {
        hasher.update(t.as_bytes());
        hasher.update(b"\n");
    }
    hasher.update(b"fallback\n");
    let fallback_tag = match &plan.fallback {
        SelectionFallback::None => "none",
        SelectionFallback::RunPackageTests { .. } => "run_package_tests",
        SelectionFallback::RunAllTests { .. } => "run_all_tests",
    };
    hasher.update(fallback_tag.as_bytes());
    hasher.update(b"\n");
    hasher.update(plan.coverage_state_tag().as_bytes());
    hasher.finalize().to_hex().to_string()
}

impl SelectionPlan {
    /// Stable tag for the coverage state, used inside the digest.
    fn coverage_state_tag(&self) -> &'static str {
        match self.coverage {
            CoverageState::Verified => "verified",
            CoverageState::Partial => "partial",
            CoverageState::Unsupported => "unsupported",
            CoverageState::Unknown => "unknown",
        }
    }
}

// ---------------------------------------------------------------------------
// Step 36 — advisory predictive ranking sidecar
// ---------------------------------------------------------------------------

/// One ranked test in the advisory sidecar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankedTest {
    pub test_id: String,
    /// Failure likelihood in `0.0..=1.0`. Advisory only.
    pub failure_probability: f64,
    /// Why this score was assigned, in measurable terms.
    pub rationale: String,
}

/// Advisory failure-likelihood ranking.
///
/// **This type can never gate a merge.** It is produced *after* and
/// independently of [`SelectionPlan`]; `select_affected_tests` has no ranking
/// parameter, so no probabilistic score can influence which tests are required
/// to pass. See `04_4.2_IMPLEMENTATION_SEQUENCE.md` Step 36.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PredictiveRanking {
    /// Always `true`. Present so the artifact is self-describing.
    pub advisory_only: bool,
    /// Fraction of affected sources each test observed. Deterministic and
    /// evidence-derived; no learned model is applied.
    pub entries: Vec<RankedTest>,
    /// Honesty of the ranking. `Unknown` when there is nothing to rank on.
    pub coverage: CoverageState,
    /// Explanation of the scoring function actually used.
    pub method: String,
}

impl PredictiveRanking {
    /// Ranks selected tests by the share of affected sources they observed.
    ///
    /// The score is `observed_affected / total_affected`, a directly
    /// measurable ratio. No statistical model is fitted, so nothing here can
    /// drift between runs or claim predictive power the engine has not
    /// demonstrated.
    pub fn from_plan(
        plan: &SelectionPlan,
        inventory: &TestInventory,
        coverage: &CoverageMatrix,
    ) -> Self {
        if coverage.is_empty() || plan.affected_sources.is_empty() {
            return Self {
                advisory_only: true,
                entries: Vec::new(),
                coverage: CoverageState::Unknown,
                method: "observed_affected_source_ratio".to_string(),
            };
        }

        let total = plan.affected_sources.len() as f64;
        let mut per_suite: BTreeMap<&str, usize> = BTreeMap::new();
        for path in &plan.affected_sources {
            for suite_id in coverage.suites_touching(path) {
                *per_suite.entry(suite_id).or_insert(0) += 1;
            }
        }

        let mut entries = Vec::new();
        for suite_id in &plan.selected_suites {
            let observed = per_suite.get(suite_id.as_str()).copied().unwrap_or(0);
            let probability = observed as f64 / total;
            if let Some(suite) = inventory.suites.get(suite_id) {
                if suite.test_cases.is_empty() {
                    entries.push(RankedTest {
                        test_id: format!("{suite_id}::@suite"),
                        failure_probability: probability,
                        rationale: format!(
                            "suite observed {observed} of {} affected source(s)",
                            plan.affected_sources.len()
                        ),
                    });
                } else {
                    for case in &suite.test_cases {
                        entries.push(RankedTest {
                            test_id: case.case_id(suite_id),
                            failure_probability: probability,
                            rationale: format!(
                                "suite observed {observed} of {} affected source(s)",
                                plan.affected_sources.len()
                            ),
                        });
                    }
                }
            }
        }

        // Highest likelihood first, then id, for a deterministic ordering.
        entries.sort_by(|a, b| {
            b.failure_probability
                .partial_cmp(&a.failure_probability)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.test_id.cmp(&b.test_id))
        });

        Self {
            advisory_only: true,
            entries,
            coverage: CoverageState::Partial,
            method: "observed_affected_source_ratio".to_string(),
        }
    }
}
