//! Step 34 acceptance: observed coverage matrix ingestion (LCOV).
//!
//! The gate for this step is not "parses LCOV" but "distinguishes *zero hits*
//! from *no record*". A source file absent from every tracefile is not
//! evidence that it is untested — it is evidence that nothing was recorded,
//! and Step 35 must read that as `Unknown` and fall back rather than narrow.

mod common;

use autopsy_domain::CoverageState;
use autopsy_test_impact::{LcovError, merge_matrices, parse_lcov};
use common::*;

#[test]
fn step34_parses_a_well_formed_tracefile() {
    let m = parse_lcov("src/main.test.ts", LCOV_MAIN_SUITE).unwrap();

    assert_eq!(m.coverage, CoverageState::Verified);
    let suite = &m.suites["src/main.test.ts"];
    assert_eq!(suite.hits.len(), 3);
    assert_eq!(
        suite.instrumented_files,
        ["src/main.ts".to_string(), "src/mid.ts".to_string()]
            .into_iter()
            .collect()
    );
    assert!(suite.executed_in("src/main.ts"));
    assert!(suite.executed_in("src/mid.ts"));
    assert!(!suite.executed_in("src/util.ts"));
}

#[test]
fn step34_ignores_records_it_does_not_need() {
    // FN / FNDA / BRDA / LF / LH / TN are all present and all skipped. Function
    // and branch records add nothing to line-level selection, and skipping
    // them cannot cause a false negative because Step 35 unions static imports
    // with observed coverage.
    let content = "TN:unit\nSF:src/mid.ts\nFN:4,compute\nFNDA:1,compute\nBRDA:4,0,0,1\nDA:4,1\nLF:1\nLH:1\nend_of_record\n";
    let m = parse_lcov("s", content).unwrap();
    assert_eq!(m.coverage, CoverageState::Verified);
    assert_eq!(m.suites["s"].hits.len(), 1);
}

#[test]
fn step34_zero_hits_is_a_record_not_an_absence() {
    let m = parse_lcov("src/mid.test.ts", LCOV_ZERO_HITS).unwrap();

    // Verified: we did read the data, and the data says "never executed".
    assert_eq!(m.coverage, CoverageState::Verified);
    let suite = &m.suites["src/mid.test.ts"];
    assert_eq!(suite.hits.len(), 2, "the DA records must be retained");
    assert!(suite.instrumented_files.contains("src/mid.ts"));
    assert!(
        !suite.executed_in("src/mid.ts"),
        "zero hits is not execution"
    );
    assert!(suite.executed_files().is_empty());
}

#[test]
fn step34_instrumented_without_line_data_is_partial_not_verified() {
    let m = parse_lcov("src/mid.test.ts", LCOV_NO_LINE_DATA).unwrap();
    assert_eq!(
        m.coverage,
        CoverageState::Partial,
        "an SF with no DA tells us nothing about execution"
    );
    assert_eq!(m.notes.len(), 1);
    assert!(m.notes[0].contains("no DA lines"), "{}", m.notes[0]);
}

#[test]
fn step34_empty_tracefile_is_unknown_with_a_note() {
    let m = parse_lcov("src/mid.test.ts", "").unwrap();
    assert_eq!(m.coverage, CoverageState::Unknown);
    assert!(m.notes[0].contains("no SF records"), "{}", m.notes[0]);
    // The suite is still registered, so a caller can tell "ingested but empty"
    // apart from "never ingested".
    assert!(m.suites.contains_key("src/mid.test.ts"));
}

// ---------------------------------------------------------------------------
// Malformed input is an error, never a silent skip
// ---------------------------------------------------------------------------

#[test]
fn step34_da_without_sf_is_an_error() {
    let err = parse_lcov("s", "DA:1,1\nend_of_record\n").unwrap_err();
    assert_eq!(err, LcovError::RecordWithoutSource);
}

#[test]
fn step34_empty_sf_path_is_an_error() {
    let err = parse_lcov("s", "SF:\n").unwrap_err();
    assert!(
        matches!(err, LcovError::Malformed { line: 1, .. }),
        "{err:?}"
    );
}

#[test]
fn step34_non_numeric_da_is_an_error() {
    let err = parse_lcov("s", "SF:a.ts\nDA:notanumber,1\n").unwrap_err();
    assert!(matches!(err, LcovError::BadDaRecord { line: 2 }), "{err:?}");
}

#[test]
fn step34_da_missing_the_hit_count_is_an_error() {
    let err = parse_lcov("s", "SF:a.ts\nDA:1\n").unwrap_err();
    assert!(matches!(err, LcovError::BadDaRecord { line: 2 }), "{err:?}");
}

#[test]
fn step34_a_checksum_suffix_on_da_is_accepted() {
    // `DA:<line>,<count>,<checksum>` is the extended form; the checksum is
    // simply not needed.
    let m = parse_lcov("s", "SF:a.ts\nDA:1,7,abcdef\nend_of_record\n").unwrap();
    let hit = m.suites["s"].hits.iter().next().unwrap();
    assert_eq!(hit.line, 1);
    assert_eq!(hit.hits, 7);
}

// ---------------------------------------------------------------------------
// Merging
// ---------------------------------------------------------------------------

#[test]
fn step34_merge_unions_suites_and_takes_the_worst_coverage() {
    let merged = merge_matrices(vec![
        parse_lcov("src/main.test.ts", LCOV_MAIN_SUITE).unwrap(),
        parse_lcov("src/mid.test.ts", LCOV_MID_SUITE).unwrap(),
    ]);

    assert_eq!(merged.suites.len(), 2);
    assert_eq!(merged.coverage, CoverageState::Verified);

    let touching = merged.suites_touching("src/mid.ts");
    assert_eq!(
        touching,
        vec!["src/main.test.ts", "src/mid.test.ts"],
        "both suites observed src/mid.ts"
    );
    assert_eq!(
        merged.suites_touching("src/main.ts"),
        vec!["src/main.test.ts"]
    );
}

#[test]
fn step34_merge_never_upgrades_a_degraded_input() {
    let merged = merge_matrices(vec![
        parse_lcov("good", LCOV_MAIN_SUITE).unwrap(),
        parse_lcov("empty", "").unwrap(),
    ]);
    assert_eq!(
        merged.coverage,
        CoverageState::Unknown,
        "one unknown input must poison the aggregate"
    );
}

#[test]
fn step34_merging_nothing_is_unknown_loudly() {
    let merged = merge_matrices(Vec::new());
    assert_eq!(merged.coverage, CoverageState::Unknown);
    assert!(merged.is_empty());
    assert_eq!(
        merged.notes,
        vec!["no coverage tracefiles were ingested".to_string()]
    );
}

#[test]
fn step34_merge_notes_are_sorted_and_deduplicated() {
    let merged = merge_matrices(vec![
        parse_lcov("a", "").unwrap(),
        parse_lcov("b", "").unwrap(),
    ]);
    let mut sorted = merged.notes.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(merged.notes, sorted);
    assert_eq!(merged.notes.len(), 2, "distinct suites keep distinct notes");
}

#[test]
fn step34_default_matrix_is_empty_and_unknown() {
    let m = autopsy_test_impact::CoverageMatrix::default();
    assert!(m.is_empty());
    assert_eq!(
        m.coverage,
        CoverageState::Unknown,
        "no data is not good data"
    );
    assert!(m.instrumented_files().is_empty());
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn step34_matrix_digest_is_deterministic() {
    let a = merge_matrices(vec![
        parse_lcov("src/main.test.ts", LCOV_MAIN_SUITE).unwrap(),
        parse_lcov("src/mid.test.ts", LCOV_MID_SUITE).unwrap(),
    ]);
    let b = merge_matrices(vec![
        parse_lcov("src/mid.test.ts", LCOV_MID_SUITE).unwrap(),
        parse_lcov("src/main.test.ts", LCOV_MAIN_SUITE).unwrap(),
    ]);
    assert_eq!(
        a.digest(),
        b.digest(),
        "merge order must not reach the digest"
    );
    assert_eq!(
        a.suites.keys().collect::<Vec<_>>(),
        b.suites.keys().collect::<Vec<_>>()
    );
}

#[test]
fn step34_parsing_is_byte_stable_across_repetitions() {
    let first = parse_lcov("s", LCOV_MAIN_SUITE).unwrap();
    for _ in 0..100 {
        assert_eq!(parse_lcov("s", LCOV_MAIN_SUITE).unwrap(), first);
    }
}
