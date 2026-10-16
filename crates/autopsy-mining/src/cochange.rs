//! Co-change metrics and the non-causality boundary.
//!
//! Support, Confidence and Lift `R31`, over the change sets [`crate::history`] extracted,
//! with the boundary `R32``R33` made structural rather than a paragraph a reader might
//! skip.
//!
//! # The formulas, and why the counts are carried alongside them
//!
//! ```text
//!   Support(A, B)      = |A ∧ B| / |Total|
//!   Confidence(A → B)  = |A ∧ B| / |A|
//!   Lift(A, B)         = P(A ∧ B) / (P(A) · P(B))
//! ```
//!
//! Every ratio is reported as **parts per million in integer arithmetic**, and every
//! ratio is accompanied by the four counts it came from. Two reasons:
//!
//! 1. **`NFR-001`.** A digest over `f64` inherits float formatting, and a percentage that
//!    moves between platforms because a decimal was rendered differently is a
//!    determinism bug that looks like a data change. Integers have one spelling.
//! 2. **Trust.** A reader who wants to check `Support` can divide two numbers that are
//!    right there, instead of re-deriving the population the engine chose.
//!
//! The ppm rendering rounds half-up on truncation, so it is a *rendering*: the counts are
//! authoritative and the ppm value is for ordering and thresholding.
//!
//! # Confidence is directional and the pair is not
//!
//! Co-change is symmetric — `A` changed with `B` exactly when `B` changed with `A` — but
//! Confidence is not: `|A ∧ B| / |A|` and `|A ∧ B| / |B|` answer different questions
//! ("when A changed, did B come along?" versus the reverse). Both are reported, and the
//! pair is stored with `a < b` so the *identity* of an edge does not depend on which
//! direction the reader happened to ask about.
//!
//! # The interval is about the window, not about causality
//!
//! Each confidence carries a Wilson score interval at 95%. It answers one question: how
//! much of this proportion could be the accident of which commits happened to fall in the
//! window? It says **nothing** about whether `A` depends on `B`. A tight interval around
//! `Confidence(A → B) = 1.0` means "every commit that touched `A` in this window also
//! touched `B`, and that is unlikely to be sampling noise" — and two files changed
//! together by a code generator, or by one person's habit, will show exactly that.
//!
//! # The warning travels with the edge
//!
//! [`EdgeClaim::TemporalCoOccurrenceOnly`] is a **single-variant enum**, and it is a field
//! of every edge. A `bool` could be set wrong; an enum with one value cannot be omitted,
//! misread, or defaulted into existence. A reader who slices one edge out of a report
//! still has it, and a reader who slices one edge out is precisely who the warning is for.
//! `EdgeKind::StatisticalCoChange` travels alongside it, so the edge is distinguishable
//! from `EdgeKind::Calls` at the type level rather than in prose.

use autopsy_domain::{CoverageState, EdgeKind};
use blake3;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

use crate::history::{ExclusionReason, HistoryOptions, HistoryWindow, MiningError, recent_changes};
use autopsy_temporal::git::GitObjectStore;

/// The fixed-point scale every ratio is reported in.
pub const PPM: u128 = 1_000_000;

/// Confidence intervals are reported at 95%, so the two-sided `z` is 1.96, kept as the
/// exact rational `196/100`.
const Z_NUMERATOR: u128 = 196;
const Z_DENOMINATOR: u128 = 100;

/// The coefficient of the half-width once the scaled radicand has been divided by
/// `SQRT_SCALE`: `z · scale / (2 · √scale) = 98`.
const HALF_NUMERATOR: u128 = 98;

/// Extra precision applied to the radicand before the integer square root. Without it the
/// division by `√n` truncates to a division by `1` for every sample smaller than four
/// commits — the case this engine reports most.
const SQRT_SCALE: u128 = 1_000_000;

/// Default floor on how many commits must contain both paths before a pair is reported.
///
/// Two, not one: a single commit that happens to touch two files is a coincidence, and a
/// report of coincidences is worse than a short report.
pub const DEFAULT_MIN_PAIR_COMMITS: u32 = 2;

/// Default ceiling on reported edges, so a large repository yields a bounded report.
pub const DEFAULT_MAX_EDGES: usize = 500;

/// Default ceiling on distinct observed pairs, so a pathological history cannot exhaust
/// memory (`INV-SANDBOX` resource bounds).
pub const DEFAULT_MAX_PAIRS: usize = 500_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoChangeOptions {
    pub min_pair_commits: u32,
    pub max_edges: usize,
    pub max_pairs: usize,
}

impl CoChangeOptions {
    pub fn new() -> Self {
        Self {
            min_pair_commits: DEFAULT_MIN_PAIR_COMMITS,
            max_edges: DEFAULT_MAX_EDGES,
            max_pairs: DEFAULT_MAX_PAIRS,
        }
    }

    pub fn with_min_pair_commits(mut self, min: u32) -> Self {
        self.min_pair_commits = min;
        self
    }

    pub fn with_max_edges(mut self, max: usize) -> Self {
        self.max_edges = max;
        self
    }

    pub fn with_max_pairs(mut self, max: usize) -> Self {
        self.max_pairs = max;
        self
    }
}

impl Default for CoChangeOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// What a co-change edge is evidence of.
///
/// One variant, on purpose. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeClaim {
    /// The two paths changed in the same commits within the window. That is the whole
    /// claim. It is *not* a dependency, a call, a data flow, or a cause.
    TemporalCoOccurrenceOnly,
}

impl EdgeClaim {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TemporalCoOccurrenceOnly => "temporal_co_occurrence_only",
        }
    }

    /// The sentence a reader of the JSON needs, in case they read a field and nothing
    /// else.
    pub const WARNING: &'static str =
        "temporal co-occurrence within the window, not a dependency, call, data flow, or cause";

    pub fn warning(self) -> &'static str {
        Self::WARNING
    }
}

/// One mined pair.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CoChangeEdge {
    /// Lexicographically first, so the edge's identity does not depend on which direction
    /// was asked about.
    pub a: String,
    pub b: String,
    /// Always `statistical_co_change`. Carried per edge so the graph vocabulary cannot
    /// mistake this for a call edge, and the distinction survives being sliced out.
    pub kind: EdgeKind,
    pub claim: EdgeClaim,
    pub commits_together: u32,
    pub commits_with_a: u32,
    pub commits_with_b: u32,
    pub total_commits: u32,
    pub support_ppm: u64,
    /// `P(B | A)`: when `A` changed, how often `B` did too.
    pub confidence_a_to_b_ppm: u64,
    pub confidence_a_to_b_lower_ppm: u64,
    pub confidence_a_to_b_upper_ppm: u64,
    /// `P(A | B)`: the same question asked the other way round.
    pub confidence_b_to_a_ppm: u64,
    pub confidence_b_to_a_lower_ppm: u64,
    pub confidence_b_to_a_upper_ppm: u64,
    pub lift_ppm: u64,
    pub coverage: CoverageState,
}

impl CoChangeEdge {
    /// Confidence in the direction the reader asked about. `None` when the two paths are
    /// the same, which cannot happen for a mined pair.
    pub fn confidence_towards(&self, from: &str) -> Option<u64> {
        if from == self.a {
            Some(self.confidence_a_to_b_ppm)
        } else if from == self.b {
            Some(self.confidence_b_to_a_ppm)
        } else {
            None
        }
    }

    pub fn interval_towards(&self, from: &str) -> Option<(u64, u64)> {
        if from == self.a {
            Some((
                self.confidence_a_to_b_lower_ppm,
                self.confidence_a_to_b_upper_ppm,
            ))
        } else if from == self.b {
            Some((
                self.confidence_b_to_a_lower_ppm,
                self.confidence_b_to_a_upper_ppm,
            ))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoChangeReport {
    pub window: WindowSummary,
    /// Pairs observed at or above the reporting floor.
    pub edges: Vec<CoChangeEdge>,
    /// Pairs observed at least once but below `min_pair_commits`.
    pub pairs_below_threshold: usize,
    /// Whether `max_pairs` was reached, in which case the pair set is a lower bound.
    pub pairs_truncated: bool,
    pub paths_with_changes: usize,
    /// Paths that changed in the window but never in the same commit as another path.
    pub paths_never_coupled: usize,
    pub coverage: CoverageState,
    pub notes: Vec<String>,
    pub digest: String,
}

impl CoChangeReport {
    /// The strongest edges by lift, already in the order the report carries them.
    pub fn strongest(&self, count: usize) -> &[CoChangeEdge] {
        let end = count.min(self.edges.len());
        &self.edges[..end]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowSummary {
    pub commits_seen: usize,
    pub commits_considered: usize,
    /// Non-zero counts only.
    pub excluded: BTreeMap<ExclusionReason, usize>,
    pub pure_renames_collapsed: usize,
    pub walk_truncated: bool,
    pub window_seconds: i64,
    pub max_files_per_commit: usize,
    /// The commit the window is anchored to. An id, not a timestamp.
    pub reference_oid: String,
    /// BLAKE3 over the considered commit ids, so the percentages are pinned to a
    /// timestamp-free set.
    pub commit_set_digest: String,
}

/// `numerator / denominator` in ppm, saturating rather than wrapping.
pub fn ratio_ppm(numerator: u128, denominator: u128) -> u64 {
    if denominator == 0 {
        return 0;
    }
    let scaled = numerator.saturating_mul(PPM) / denominator;
    u64::try_from(scaled).unwrap_or(u64::MAX)
}

/// The Wilson score interval for a proportion, at 95%, in ppm.
///
/// Wilson rather than the normal approximation because the normal approximation is worst
/// exactly where this engine reports most: small proportions and proportions at the
/// boundary. `Confidence = 1.0` over three commits is precisely the case Wald renders as
/// a zero-width interval, which is the opposite of the truth.
///
/// The algebra is done in integers and finished with `isqrt`, so the result is a function
/// of the counts alone. The two square roots truncate, which moves a bound by under one
/// ppm — the counts are carried alongside and are authoritative.
pub fn wilson_interval_ppm(successes: u64, trials: u64) -> (u64, u64) {
    if trials == 0 {
        // Nothing was observed, so the interval is everything.
        return (0, u64::try_from(PPM).unwrap_or(u64::MAX));
    }
    let x = u128::from(successes.min(trials));
    let n = u128::from(trials);

    // `z² = 38416/10000` and `z = 196/100`, kept as exact rationals. Everything below is
    // multiplied through by the denominators, so no float ever enters and the result is a
    // function of the two counts alone.
    let z2 = Z_NUMERATOR * Z_NUMERATOR; // 196²
    let scale = Z_DENOMINATOR * Z_DENOMINATOR; // 100²

    // Centre = (x + z²/2) / (n + z²).
    let centre = ratio_ppm(x * scale + z2 / 2, n * scale + z2);

    // Half-width = z · √(4x(n−x) + z²n) / (2·√n·(n + z²)).
    //
    // `radicand` is that expression scaled by `scale`, and the following extra factor
    // lifts the square root into the fractional range *before* truncating: taking
    // `isqrt(n)` separately would divide by `1` for every n below four, which is the case
    // this engine reports most and the case the normal approximation gets wrong too.
    let radicand = 4 * x * (n - x) * scale + z2 * n;
    let lifted = radicand.saturating_mul(SQRT_SCALE * SQRT_SCALE) / n;
    // `98 = z · scale / (2 · √scale)`: the factor that remains once the radicand's scale
    // and the extra precision have both been divided out.
    let half = ratio_ppm(
        HALF_NUMERATOR.saturating_mul(integer_sqrt(lifted)),
        (n * scale + z2).saturating_mul(SQRT_SCALE),
    );

    // Two values of the Wilson bound are *exactly* known, and the integer square root
    // truncates a fraction of a ppm off both of them: if every trial succeeded the upper
    // bound is 1, and if none did the lower bound is 0. Stated rather than left a ppm short
    // of a certainty, so `confidence = 1.0` is not accompanied by an upper bound of
    // 0.999999.
    let scaled = u64::try_from(PPM).unwrap_or(u64::MAX);
    let upper = if x == n {
        scaled
    } else {
        centre.saturating_add(half).min(scaled)
    };
    let lower = if x == 0 {
        0
    } else {
        centre.saturating_sub(half).min(upper)
    };
    (lower, upper)
}

/// Integer square root by Newton's method. `u128::isqrt` would do, but it is not
/// available on the workspace's MSRV.
fn integer_sqrt(value: u128) -> u128 {
    if value < 2 {
        return value;
    }
    let mut guess = 1u128 << ((128 - value.leading_zeros()) / 2 + 1);
    loop {
        let next = (guess + value / guess) / 2;
        if next >= guess {
            return guess;
        }
        guess = next;
    }
}

fn digest_of<T: Serialize>(value: &T) -> String {
    let mut json = serde_json::to_value(value).unwrap_or(Value::Null);
    if let Value::Object(map) = &mut json {
        map.remove("digest");
        map.remove("notes");
    }
    let text = serde_json::to_string(&json).unwrap_or_default();
    blake3::hash(text.as_bytes()).to_hex().to_string()
}

/// Reads history from a repository and mines it in one call.
pub fn mine_repository(
    store: &mut GitObjectStore,
    history: &HistoryOptions,
    options: &CoChangeOptions,
) -> Result<CoChangeReport, MiningError> {
    let window = recent_changes(store, history)?;
    Ok(mine(&window, options))
}

/// Mines a window. Pure, so the metric behaviour is testable without a repository.
pub fn mine(window: &HistoryWindow, options: &CoChangeOptions) -> CoChangeReport {
    let total_commits = window.accounting.commits_considered;

    if window.is_empty() {
        let mut notes = vec![
            "no commits were considered, so no co-change can be measured and every ratio \
             would be over an empty population"
                .to_string(),
        ];
        if window.accounting.excluded.is_empty() {
            notes
                .push("the repository has no resolvable HEAD, or its history is empty".to_string());
        }
        notes.sort();
        let mut report = CoChangeReport {
            window: summarise(window),
            edges: Vec::new(),
            pairs_below_threshold: 0,
            pairs_truncated: false,
            paths_with_changes: 0,
            paths_never_coupled: 0,
            coverage: CoverageState::Unknown,
            notes,
            digest: String::new(),
        };
        report.digest = digest_of(&report);
        return report;
    }

    // ---- count -------------------------------------------------------------
    let mut commits_with_path: BTreeMap<String, u32> = BTreeMap::new();
    let mut pairs: BTreeMap<(String, String), u32> = BTreeMap::new();
    let mut pairs_truncated = false;
    let mut all_paths: BTreeSet<String> = BTreeSet::new();

    for change in &window.changes {
        let paths: Vec<&String> = change.paths.iter().collect();
        for path in &paths {
            *commits_with_path.entry((*path).clone()).or_insert(0) += 1;
            all_paths.insert((*path).clone());
        }
        for (index, first) in paths.iter().enumerate() {
            for second in &paths[index + 1..] {
                // Sorted pair key, so the edge is the same object however it was reached.
                let key = if first < second {
                    ((*first).clone(), (*second).clone())
                } else {
                    ((*second).clone(), (*first).clone())
                };
                if let Some(count) = pairs.get_mut(&key) {
                    *count += 1;
                } else if pairs.len() >= options.max_pairs {
                    // A lower bound is still evidence; an unqualified lower bound is not.
                    pairs_truncated = true;
                    continue;
                } else {
                    pairs.insert(key, 1);
                }
            }
        }
    }

    let coupled: BTreeSet<&String> = pairs.keys().flat_map(|(a, b)| [a, b]).collect();
    let paths_never_coupled = all_paths
        .iter()
        .filter(|path| !coupled.contains(*path))
        .count();

    // ---- score -------------------------------------------------------------
    let mut coverage = CoverageState::Verified;
    let mut notes: Vec<String> = Vec::new();

    if total_commits < 2 {
        coverage = worse(coverage, CoverageState::Partial);
        notes.push(format!(
            "only {total_commits} commit(s) were considered, so every ratio is trivially 1 \
             or undefined and the population is too small to support a measurement"
        ));
    }
    let mut biased: Vec<&str> = Vec::new();
    for (reason, count) in &window.accounting.excluded {
        if reason.biases_the_population() && *count > 0 {
            biased.push(reason.as_str());
        }
    }
    if !biased.is_empty() {
        coverage = worse(coverage, CoverageState::Partial);
        notes.push(format!(
            "{} commit(s) that changed real code were excluded ({}), so the co-change counts \
             are a lower bound rather than the whole population",
            window
                .accounting
                .excluded
                .iter()
                .filter(|(reason, _)| reason.biases_the_population())
                .map(|(_, count)| *count)
                .sum::<usize>(),
            biased.join(", ")
        ));
    }
    if window.accounting.walk_truncated {
        coverage = worse(coverage, CoverageState::Partial);
        notes.push(
            "the history walk stopped at the configured commit bound, so the window may not \
             have been reached and the population may be incomplete"
                .to_string(),
        );
    }
    if pairs_truncated {
        coverage = worse(coverage, CoverageState::Partial);
        notes.push(format!(
            "the pair set reached its bound of {}, so the mined pairs are a lower bound and a \
             pair with no entry was not necessarily never co-changed",
            options.max_pairs
        ));
    }
    if window.accounting.pure_renames_collapsed > 0 {
        // Not a degradation: collapsing a pure rename is a correction, not a loss. Said out
        // loud so a reader knows the count of `path` changes is not the raw diff size.
        notes.push(format!(
            "{} pure rename(s) were collapsed: a path that disappeared while another appeared \
             carrying the identical content is a move, not two changes. A move that also edits \
             is still mined as a deletion plus an addition",
            window.accounting.pure_renames_collapsed
        ));
    }

    let mut edges: Vec<CoChangeEdge> = Vec::new();
    let mut below_threshold = 0usize;
    for ((a, b), together) in &pairs {
        if *together < options.min_pair_commits {
            below_threshold += 1;
            continue;
        }
        let with_a = commits_with_path.get(a).copied().unwrap_or(0);
        let with_b = commits_with_path.get(b).copied().unwrap_or(0);
        let total = u32::try_from(total_commits).unwrap_or(u32::MAX);
        if with_a == 0 || with_b == 0 {
            // Unreachable: a pair exists only because both paths were counted. Guarded
            // rather than `unwrap`, because dividing by a zero here would be a panic in a
            // read-only analyzer.
            continue;
        }
        let (a_lower, a_upper) = wilson_interval_ppm(u64::from(*together), u64::from(with_a));
        let (b_lower, b_upper) = wilson_interval_ppm(u64::from(*together), u64::from(with_b));
        let edge_coverage = if *together < 3 {
            // Lift is famously unstable at small counts `R32`: two commits that happened to
            // touch both files can produce a spectacular number. Reported, and labelled.
            worse(coverage, CoverageState::Partial)
        } else {
            coverage
        };
        edges.push(CoChangeEdge {
            a: a.clone(),
            b: b.clone(),
            kind: EdgeKind::StatisticalCoChange,
            claim: EdgeClaim::TemporalCoOccurrenceOnly,
            commits_together: *together,
            commits_with_a: with_a,
            commits_with_b: with_b,
            total_commits: total,
            support_ppm: ratio_ppm(u128::from(*together), u128::from(total)),
            confidence_a_to_b_ppm: ratio_ppm(u128::from(*together), u128::from(with_a)),
            confidence_a_to_b_lower_ppm: a_lower,
            confidence_a_to_b_upper_ppm: a_upper,
            confidence_b_to_a_ppm: ratio_ppm(u128::from(*together), u128::from(with_b)),
            confidence_b_to_a_lower_ppm: b_lower,
            confidence_b_to_a_upper_ppm: b_upper,
            lift_ppm: ratio_ppm(
                u128::from(*together) * u128::from(total),
                u128::from(with_a) * u128::from(with_b),
            ),
            coverage: edge_coverage,
        });
    }

    // Strongest first: lift, then how much evidence there is, then the pair itself, so the
    // order never depends on map iteration.
    edges.sort_by(|left, right| {
        right
            .lift_ppm
            .cmp(&left.lift_ppm)
            .then_with(|| right.commits_together.cmp(&left.commits_together))
            .then_with(|| left.a.cmp(&right.a))
            .then_with(|| left.b.cmp(&right.b))
    });
    if edges.len() > options.max_edges {
        coverage = worse(coverage, CoverageState::Partial);
        notes.push(format!(
            "{} pair(s) met the reporting floor but only the {} strongest by lift are listed",
            edges.len(),
            options.max_edges
        ));
        edges.truncate(options.max_edges);
    }

    if edges.iter().any(|edge| edge.commits_together < 3) {
        notes.push(
            "some edges rest on fewer than three commits, where lift is dominated by small \
             counts (Greiler et al. `R32`); the interval beside each confidence shows how \
             little the window constrains it"
                .to_string(),
        );
    }
    notes.push(
        "co-change is temporal co-occurrence within the window, not a dependency: two paths \
         changed in the same commits may share a generator, a reviewer, or a release habit \
         (Ying et al. `R31`, Greiler et al. `R32`, Tao et al. `R33`). The confidence \
         interval covers sampling noise in the window and says nothing about causality"
            .to_string(),
    );
    notes.sort();
    notes.dedup();

    let mut report = CoChangeReport {
        window: summarise(window),
        edges,
        pairs_below_threshold: below_threshold,
        pairs_truncated,
        paths_with_changes: all_paths.len(),
        paths_never_coupled,
        coverage,
        notes,
        digest: String::new(),
    };
    report.digest = digest_of(&report);
    report
}

fn summarise(window: &HistoryWindow) -> WindowSummary {
    WindowSummary {
        commits_seen: window.accounting.commits_seen,
        commits_considered: window.accounting.commits_considered,
        excluded: window
            .accounting
            .excluded
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(reason, count)| (*reason, *count))
            .collect(),
        pure_renames_collapsed: window.accounting.pure_renames_collapsed,
        walk_truncated: window.accounting.walk_truncated,
        window_seconds: window.window_seconds,
        max_files_per_commit: window.max_files_per_commit,
        reference_oid: window.reference_oid.clone(),
        commit_set_digest: window.commit_set_digest.clone(),
    }
}

fn worse(left: CoverageState, right: CoverageState) -> CoverageState {
    use CoverageState::{Partial, Unknown, Unsupported, Verified};
    match (left, right) {
        (Unknown, _) | (_, Unknown) => Unknown,
        (Unsupported, _) | (_, Unsupported) => Unsupported,
        (Partial, _) | (_, Partial) => Partial,
        (Verified, Verified) => Verified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ratio_is_ppm_and_a_zero_denominator_is_zero_not_a_panic() {
        assert_eq!(ratio_ppm(1, 2), 500_000);
        assert_eq!(ratio_ppm(0, 5), 0);
        assert_eq!(ratio_ppm(5, 5), 1_000_000);
        assert_eq!(
            ratio_ppm(1, 0),
            0,
            "a read-only analyzer must not panic here"
        );
    }

    #[test]
    fn the_integer_square_root_is_exact_on_perfect_squares() {
        for value in [0u128, 1, 2, 3, 4, 9, 10, 144, 1_000_000, 999_999] {
            let root = integer_sqrt(value);
            assert!(root * root <= value, "{value}");
            assert!((root + 1) * (root + 1) > value, "{value}");
        }
        // √(2¹²⁸−1) = 2⁶⁴−1: the largest integer whose square still fits.
        assert_eq!(integer_sqrt(u128::MAX), 18_446_744_073_709_551_615);
    }

    #[test]
    fn the_interval_brackets_the_proportion_and_narrows_with_evidence() {
        let (lower, upper) = wilson_interval_ppm(50, 100);
        assert!(lower < 500_000 && 500_000 < upper, "{lower}..{upper}");
        // More evidence, same proportion: a narrower interval. This is the property that
        // makes the number informative rather than decorative.
        let (narrow_low, narrow_high) = wilson_interval_ppm(5_000, 10_000);
        assert!(
            narrow_high - narrow_low < upper - lower,
            "{narrow_low}..{narrow_high} is not narrower than {lower}..{upper}"
        );
    }

    #[test]
    fn a_certain_proportion_still_has_a_width() {
        // The case the normal approximation gets wrong, and the case this engine reports
        // most: every commit that touched `A` also touched `B`. Wald would say the
        // interval is the single point 1.0; Wilson says a small sample cannot support that.
        let (lower, upper) = wilson_interval_ppm(3, 3);
        assert_eq!(upper, 1_000_000);
        assert!(
            lower < 1_000_000,
            "a sample of three cannot prove certainty"
        );
        assert!(
            lower > 200_000,
            "but it is not vacuous either: {lower} is a real bound"
        );
    }

    #[test]
    fn no_trials_means_the_interval_is_everything() {
        assert_eq!(wilson_interval_ppm(0, 0), (0, 1_000_000));
    }

    #[test]
    fn successes_beyond_trials_are_clamped_rather_than_trusted() {
        // Cannot happen from the counters, but a public function must not produce an
        // interval outside [0, 1] because a caller passed a bad pair.
        let (lower, upper) = wilson_interval_ppm(10, 3);
        assert!(lower <= upper && upper <= 1_000_000);
    }

    #[test]
    fn the_claim_has_exactly_one_value_and_carries_its_warning() {
        assert_eq!(
            EdgeClaim::TemporalCoOccurrenceOnly.as_str(),
            "temporal_co_occurrence_only"
        );
        let warning = EdgeClaim::TemporalCoOccurrenceOnly.warning();
        assert!(warning.contains("not a dependency"), "{warning}");
    }

    #[test]
    fn the_edge_kind_is_the_domain_vocabulary_not_a_call() {
        // The distinction the design asks for, asserted at the type level: a co-change
        // edge is not an `EdgeKind::Calls`, and the enum is what makes that structural.
        assert_ne!(EdgeKind::StatisticalCoChange, EdgeKind::Calls);
        assert_eq!(
            serde_json::to_string(&EdgeKind::StatisticalCoChange).unwrap(),
            "\"statistical_co_change\""
        );
    }
}
