//! Steps 35–36 acceptance: conservative test selection and advisory ranking.
//!
//! Step 35 gate: *"0% false negatives on regression fixture; force
//! `RunAllTests` on dynamic dispatch / Unknown coverage."*
//!
//! Step 36 gate: *"Ranking is advisory-only and cannot block merges."*

mod common;

use autopsy_domain::CoverageState;
use autopsy_test_impact::{
    ChangeSetSummary, CoverageMatrix, NoReachability, PredictiveRanking, SelectionError,
    SelectionFallback, merge_matrices, parse_lcov, select_affected_tests,
};
use common::*;

fn clean_inventory() -> autopsy_test_impact::TestInventory {
    autopsy_test_impact::discover_inventory(&clean_repo())
}

// ===========================================================================
// Step 35 — the 0% false negative gate
// ===========================================================================

#[test]
fn step35_gate_no_test_that_can_catch_the_regression_is_omitted() {
    let inv = clean_inventory();
    let cov = merge_matrices(vec![
        parse_lcov("src/main.test.ts", LCOV_MAIN_SUITE).unwrap(),
        parse_lcov("src/mid.test.ts", LCOV_MID_SUITE).unwrap(),
    ]);

    let plan = select_affected_tests(&change_add(), &inv, &cov, &symbol_graph()).unwrap();

    // The regression is in `add()` in `src/util.ts`. The only suite that can
    // catch it is two call hops away (util <- mid <- main). Omitting it is
    // exactly the false negative this gate forbids.
    assert!(
        plan.selected_suites.contains("src/main.test.ts"),
        "transitively-dependent suite was omitted: {:?}",
        plan.selected_suites
    );
    assert!(plan.selected_suites.contains("src/mid.test.ts"));

    // ...while the genuinely unrelated suite is still excluded. Narrowing and
    // safety are asserted together: either alone is trivially satisfiable.
    assert!(
        !plan.selected_suites.contains("src/other.test.ts"),
        "unrelated suite was not excluded: {:?}",
        plan.selected_suites
    );

    assert_eq!(plan.fallback, SelectionFallback::None);
    assert_eq!(plan.coverage, CoverageState::Verified);
    assert_eq!(plan.selected_suites.len(), 2);
    assert_eq!(plan.total_suites, 3);
}

#[test]
fn step35_file_and_symbol_granularity_agree_on_the_affected_set() {
    let inv = clean_inventory();
    let cov = merge_matrices(vec![
        parse_lcov("src/main.test.ts", LCOV_MAIN_SUITE).unwrap(),
    ]);

    let by_symbol = select_affected_tests(&change_add(), &inv, &cov, &symbol_graph()).unwrap();
    let by_file = select_affected_tests(&change_add(), &inv, &cov, &file_graph()).unwrap();

    assert_eq!(
        by_symbol.affected_sources, by_file.affected_sources,
        "a change must not hide depending on which layer reported it"
    );
    assert_eq!(by_symbol.selected_suites, by_file.selected_suites);
}

#[test]
fn step35_selected_tests_are_concrete_case_ids_not_just_files() {
    let inv = clean_inventory();
    let cov = merge_matrices(vec![
        parse_lcov("src/main.test.ts", LCOV_MAIN_SUITE).unwrap(),
    ]);
    let plan = select_affected_tests(&change_add(), &inv, &cov, &symbol_graph()).unwrap();

    assert!(
        plan.selected_tests.contains("src/main.test.ts::test::runs"),
        "{:?}",
        plan.selected_tests
    );
    assert!(
        plan.selected_tests
            .contains("src/main.test.ts::describe::main")
    );
    assert!(
        !plan
            .selected_tests
            .iter()
            .any(|t| t.starts_with("src/other")),
        "unrelated cases leaked in: {:?}",
        plan.selected_tests
    );
}

// ---------------------------------------------------------------------------
// Forced full runs
// ---------------------------------------------------------------------------

#[test]
fn step35_gate_unknown_symbol_semantics_forces_run_all_tests() {
    let inv = clean_inventory();
    let cov = merge_matrices(vec![
        parse_lcov("src/main.test.ts", LCOV_MAIN_SUITE).unwrap(),
    ]);

    let mut cs = change_add();
    cs.unknown_symbols
        .insert("src/util.ts::add.#dynamic-dispatch".to_string());

    let plan = select_affected_tests(&cs, &inv, &cov, &symbol_graph()).unwrap();

    assert!(plan.fallback.is_full_run(), "{:?}", plan.fallback);
    assert_eq!(plan.selected_suites.len(), inv.len(), "everything must run");
    assert_eq!(plan.coverage, CoverageState::Unknown);
    assert!(
        plan.reasons
            .iter()
            .any(|r| r.code == "unknown_changed_symbol_semantics"),
        "{:?}",
        plan.reasons
    );
    // The offending symbol is disclosed, not summarized away.
    let detail = plan
        .reasons
        .iter()
        .find(|r| r.code == "unknown_changed_symbol_semantics")
        .unwrap()
        .detail
        .clone();
    assert!(detail.contains("dynamic-dispatch"), "{detail}");
}

#[test]
fn step35_gate_unknown_change_set_coverage_forces_run_all_tests() {
    let inv = clean_inventory();
    let mut cs = ChangeSetSummary::from_verified_files(vec!["src/util.ts".to_string()]);
    cs.coverage = CoverageState::Unknown;

    let plan =
        select_affected_tests(&cs, &inv, &CoverageMatrix::default(), &symbol_graph()).unwrap();

    assert!(plan.fallback.is_full_run(), "{:?}", plan.fallback);
    assert_eq!(plan.selected_suites.len(), inv.len());
    assert!(
        plan.reasons
            .iter()
            .any(|r| r.code == "change_set_coverage_unknown"),
        "{:?}",
        plan.reasons
    );
}

#[test]
fn step35_run_all_plan_still_reports_every_case() {
    let inv = clean_inventory();
    let mut cs = change_add();
    cs.unknown_symbols.insert("x".to_string());

    let plan =
        select_affected_tests(&cs, &inv, &CoverageMatrix::default(), &symbol_graph()).unwrap();
    assert_eq!(plan.selected_tests.len(), inv.test_case_count());
    assert_eq!(plan.suite_narrowing_ratio(), 1.0);
}

// ---------------------------------------------------------------------------
// The un-rulable-out asymmetry
// ---------------------------------------------------------------------------

#[test]
fn step35_gate_a_suite_that_is_not_verified_cannot_be_excluded() {
    let inv = autopsy_test_impact::discover_inventory(&repo_with_unresolvable_import());
    let cov = merge_matrices(vec![
        parse_lcov("src/main.test.ts", LCOV_MAIN_SUITE).unwrap(),
        parse_lcov("src/mid.test.ts", LCOV_MID_SUITE).unwrap(),
    ]);

    let plan = select_affected_tests(&change_add(), &inv, &cov, &symbol_graph()).unwrap();

    // `test/broken.spec.js` declares an import we could not resolve, so we do
    // not know what it covers. It must run.
    assert_eq!(
        plan.unrulable_out_suites,
        ["test/broken.spec.js".to_string()].into_iter().collect()
    );
    assert!(
        plan.selected_suites.contains("test/broken.spec.js"),
        "an unmapped suite was silently excluded — that is a false negative"
    );
    // The Verified, unrelated suite is still excluded: safety does not degrade
    // into "run everything".
    assert!(!plan.selected_suites.contains("src/other.test.ts"));
    assert_eq!(plan.fallback, SelectionFallback::None);
    assert_eq!(plan.coverage, CoverageState::Partial);
    assert!(
        plan.reasons
            .iter()
            .any(|r| r.code == "unmapped_suite_cannot_be_excluded"),
        "{:?}",
        plan.reasons
    );
}

#[test]
fn step35_an_undecidable_framework_also_cannot_be_excluded() {
    let inv = autopsy_test_impact::discover_inventory(&repo_with_ambient_suite());
    let plan = select_affected_tests(
        &change_add(),
        &inv,
        &CoverageMatrix::default(),
        &symbol_graph(),
    )
    .unwrap();

    assert!(
        plan.selected_suites.contains("src/ambient.test.ts"),
        "a suite whose framework is undecidable was excluded"
    );
}

// ---------------------------------------------------------------------------
// Honest fallbacks
// ---------------------------------------------------------------------------

#[test]
fn step35_missing_coverage_data_falls_back_to_package_scope() {
    let inv = clean_inventory();
    let plan = select_affected_tests(
        &change_add(),
        &inv,
        &CoverageMatrix::default(),
        &symbol_graph(),
    )
    .unwrap();

    assert_eq!(
        plan.fallback,
        SelectionFallback::RunPackageTests {
            packages: vec!["src".to_string()],
            reason: "no observed coverage was ingested; selection rests on static imports alone"
                .to_string(),
        }
    );
    assert!(
        plan.reasons
            .iter()
            .any(|r| r.code == "static_import_mapping"),
        "{:?}",
        plan.reasons
    );
    // Package-scoped, not a full run: the static mapping still narrowed.
    assert!(!plan.fallback.is_full_run());
    assert_eq!(plan.selected_suites.len(), 2);
}

#[test]
fn step35_no_positive_mapping_forces_run_all_tests() {
    // A change to a file that no suite maps to. "Nothing covers this" and "we
    // failed to map this" are indistinguishable from in here, so the honest
    // reading is to run everything.
    let inv = autopsy_test_impact::discover_inventory(&files(&[
        ("src/util.ts", UTIL_TS),
        ("src/other.ts", OTHER_TS),
        ("src/other.test.ts", OTHER_TEST_TS),
    ]));

    let plan = select_affected_tests(
        &change_add(),
        &inv,
        &CoverageMatrix::default(),
        &NoReachability,
    )
    .unwrap();

    assert!(plan.fallback.is_full_run(), "{:?}", plan.fallback);
    assert!(!plan.selected_suites.is_empty());
    assert_eq!(plan.coverage, CoverageState::Unknown);
    let SelectionFallback::RunAllTests { reason } = &plan.fallback else {
        unreachable!()
    };
    assert!(reason.contains("mapping failure"), "{reason}");
}

#[test]
fn step35_empty_inventory_selects_nothing_and_does_not_pretend_to_be_verified() {
    let inv = autopsy_test_impact::discover_inventory(&[]);
    let plan = select_affected_tests(
        &change_add(),
        &inv,
        &CoverageMatrix::default(),
        &symbol_graph(),
    )
    .unwrap();

    assert_eq!(plan.fallback, SelectionFallback::None, "nothing to run");
    assert!(plan.selected_suites.is_empty());
    assert!(plan.selected_tests.is_empty());
    assert_eq!(plan.total_suites, 0);
    assert_eq!(
        plan.coverage,
        CoverageState::Partial,
        "an empty inventory is not a verified selection"
    );
}

#[test]
fn step35_suite_without_parsed_cases_is_still_addressable() {
    // A suite whose cases we could not parse must still appear in the plan by
    // something other than silence.
    let inv = autopsy_test_impact::discover_inventory(&files(&[
        ("src/util.ts", UTIL_TS),
        ("src/main.ts", MAIN_TS),
        ("test/opaque.test.ts", EMPTY_TEST_TS),
    ]));
    let mut cs = ChangeSetSummary::from_verified_files(vec!["src/util.ts".to_string()]);
    cs.unknown_symbols.insert("src/util.ts::add".to_string());

    let plan =
        select_affected_tests(&cs, &inv, &CoverageMatrix::default(), &symbol_graph()).unwrap();
    assert!(
        plan.selected_tests.contains("test/opaque.test.ts::@suite"),
        "{:?}",
        plan.selected_tests
    );
}

// ---------------------------------------------------------------------------
// Resource bounds and determinism
// ---------------------------------------------------------------------------

#[test]
fn step35_seed_count_is_bounded() {
    let inv = clean_inventory();
    let mut cs = ChangeSetSummary::verified();
    for i in 0..=autopsy_test_impact::MAX_SEEDS {
        cs.changed_symbols.insert(format!("src/f{i}.ts::f{i}"));
    }
    let err =
        select_affected_tests(&cs, &inv, &CoverageMatrix::default(), &NoReachability).unwrap_err();
    assert_eq!(
        err,
        SelectionError::TooManySeeds(autopsy_test_impact::MAX_SEEDS + 1)
    );
}

#[test]
fn step35_plan_is_reproducible_across_100_runs() {
    let inv = clean_inventory();
    let cov = merge_matrices(vec![
        parse_lcov("src/main.test.ts", LCOV_MAIN_SUITE).unwrap(),
        parse_lcov("src/mid.test.ts", LCOV_MID_SUITE).unwrap(),
    ]);

    let baseline = select_affected_tests(&change_add(), &inv, &cov, &symbol_graph()).unwrap();
    for run in 0..100 {
        let plan = select_affected_tests(&change_add(), &inv, &cov, &symbol_graph()).unwrap();
        assert_eq!(plan, baseline, "run {run} diverged");
        assert_eq!(plan.digest, baseline.digest, "run {run} digest diverged");
    }
    assert_eq!(baseline.digest.len(), 64, "BLAKE3 hex digest");
}

#[test]
fn step35_plan_does_not_depend_on_change_set_insertion_order() {
    let inv = clean_inventory();
    let cov = merge_matrices(vec![
        parse_lcov("src/main.test.ts", LCOV_MAIN_SUITE).unwrap(),
    ]);

    let mut forward = ChangeSetSummary::verified();
    for f in ["src/util.ts", "src/mid.ts"] {
        forward.changed_files.insert(f.to_string());
    }
    forward
        .changed_symbols
        .insert("src/util.ts::add".to_string());

    let mut reversed = ChangeSetSummary::verified();
    for f in ["src/mid.ts", "src/util.ts"] {
        reversed.changed_files.insert(f.to_string());
    }
    reversed
        .changed_symbols
        .insert("src/util.ts::add".to_string());

    let a = select_affected_tests(&forward, &inv, &cov, &symbol_graph()).unwrap();
    let b = select_affected_tests(&reversed, &inv, &cov, &symbol_graph()).unwrap();
    assert_eq!(a.digest, b.digest);
}

#[test]
fn step35_reasons_are_sorted_and_carry_the_summary() {
    let inv = clean_inventory();
    let plan = select_affected_tests(
        &change_add(),
        &inv,
        &CoverageMatrix::default(),
        &symbol_graph(),
    )
    .unwrap();

    let mut sorted = plan.reasons.clone();
    sorted.sort();
    assert_eq!(plan.reasons, sorted, "reasons must be canonically ordered");
    assert!(plan.reasons.iter().any(|r| r.code == "plan_summary"));
}

#[test]
fn step35_narrowing_ratio_reflects_the_fallback() {
    let inv = clean_inventory();
    let cov = merge_matrices(vec![
        parse_lcov("src/main.test.ts", LCOV_MAIN_SUITE).unwrap(),
    ]);

    let narrow = select_affected_tests(&change_add(), &inv, &cov, &symbol_graph()).unwrap();
    assert!((narrow.suite_narrowing_ratio() - 2.0 / 3.0).abs() < 1e-12);

    let mut cs = change_add();
    cs.unknown_symbols.insert("x".to_string());
    let full = select_affected_tests(&cs, &inv, &cov, &symbol_graph()).unwrap();
    assert_eq!(full.suite_narrowing_ratio(), 1.0);
}

// ===========================================================================
// Step 36 — advisory ranking, structurally unable to gate a merge
// ===========================================================================

#[test]
fn step36_gate_ranking_is_always_marked_advisory() {
    let inv = clean_inventory();
    let cov = merge_matrices(vec![
        parse_lcov("src/main.test.ts", LCOV_MAIN_SUITE).unwrap(),
    ]);
    let plan = select_affected_tests(&change_add(), &inv, &cov, &symbol_graph()).unwrap();

    let ranking = PredictiveRanking::from_plan(&plan, &inv, &cov);
    assert!(ranking.advisory_only);
    assert!(!ranking.entries.is_empty());
}

#[test]
fn step36_gate_ranking_cannot_change_the_plan() {
    // The selection function has no ranking parameter, so the only way a score
    // could reach a merge gate is by mutating the plan. Prove that computing a
    // ranking does not: the plan re-derived afterwards is byte-identical.
    let inv = clean_inventory();
    let cov = merge_matrices(vec![
        parse_lcov("src/main.test.ts", LCOV_MAIN_SUITE).unwrap(),
        parse_lcov("src/mid.test.ts", LCOV_MID_SUITE).unwrap(),
    ]);

    let before = select_affected_tests(&change_add(), &inv, &cov, &symbol_graph()).unwrap();
    let ranking = PredictiveRanking::from_plan(&before, &inv, &cov);
    let after = select_affected_tests(&change_add(), &inv, &cov, &symbol_graph()).unwrap();

    assert_eq!(before, after);
    assert_eq!(before.digest, after.digest);
    // A ranking is purely a rendering of evidence already in the plan.
    let ranked: std::collections::BTreeSet<&str> =
        ranking.entries.iter().map(|e| e.test_id.as_str()).collect();
    for entry in &ranked {
        assert!(
            before.selected_tests.contains(*entry),
            "ranking invented a test that the plan did not select: {entry}"
        );
    }
}

#[test]
fn step36_without_coverage_the_ranking_refuses_to_guess() {
    let inv = clean_inventory();
    let cov = CoverageMatrix::default();
    let plan = select_affected_tests(&change_add(), &inv, &cov, &symbol_graph()).unwrap();

    let ranking = PredictiveRanking::from_plan(&plan, &inv, &cov);
    assert!(ranking.entries.is_empty(), "no evidence, no ranking");
    assert_eq!(ranking.coverage, CoverageState::Unknown);
    assert!(ranking.advisory_only);
}

#[test]
fn step36_ordering_is_by_observed_share_then_id() {
    let inv = clean_inventory();
    let cov = merge_matrices(vec![
        parse_lcov("src/main.test.ts", LCOV_MAIN_SUITE).unwrap(),
        parse_lcov("src/mid.test.ts", LCOV_MID_SUITE).unwrap(),
    ]);
    let plan = select_affected_tests(&change_add(), &inv, &cov, &symbol_graph()).unwrap();
    let ranking = PredictiveRanking::from_plan(&plan, &inv, &cov);

    // main.test.ts observed 2 of 3 affected sources; mid.test.ts observed 1.
    let main_score = ranking
        .entries
        .iter()
        .find(|e| e.test_id.starts_with("src/main.test.ts"))
        .unwrap()
        .failure_probability;
    let mid_score = ranking
        .entries
        .iter()
        .find(|e| e.test_id.starts_with("src/mid.test.ts"))
        .unwrap()
        .failure_probability;
    assert!((main_score - 2.0 / 3.0).abs() < 1e-12, "{main_score}");
    assert!((mid_score - 1.0 / 3.0).abs() < 1e-12, "{mid_score}");

    // Descending score, ties broken by id.
    for pair in ranking.entries.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        assert!(
            a.failure_probability > b.failure_probability
                || (a.failure_probability == b.failure_probability && a.test_id <= b.test_id),
            "ordering violated: {a:?} then {b:?}"
        );
    }
}

#[test]
fn step36_rationale_states_the_measurement_not_a_claim() {
    let inv = clean_inventory();
    let cov = merge_matrices(vec![
        parse_lcov("src/main.test.ts", LCOV_MAIN_SUITE).unwrap(),
    ]);
    let plan = select_affected_tests(&change_add(), &inv, &cov, &symbol_graph()).unwrap();
    let ranking = PredictiveRanking::from_plan(&plan, &inv, &cov);

    assert_eq!(ranking.method, "observed_affected_source_ratio");
    for entry in &ranking.entries {
        assert!(
            entry.rationale.contains("observed") && entry.rationale.contains("affected source"),
            "rationale must name the measurement: {}",
            entry.rationale
        );
    }
}

#[test]
fn step36_ranking_is_reproducible() {
    let inv = clean_inventory();
    let cov = merge_matrices(vec![
        parse_lcov("src/main.test.ts", LCOV_MAIN_SUITE).unwrap(),
    ]);
    let plan = select_affected_tests(&change_add(), &inv, &cov, &symbol_graph()).unwrap();

    let baseline = PredictiveRanking::from_plan(&plan, &inv, &cov);
    for _ in 0..100 {
        assert_eq!(PredictiveRanking::from_plan(&plan, &inv, &cov), baseline);
    }
}
