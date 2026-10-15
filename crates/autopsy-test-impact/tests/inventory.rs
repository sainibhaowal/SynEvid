//! Step 33 acceptance: test inventory discovery and manifest mapping.
//!
//! Gate: *"Discovers Vitest, Jest, and Mocha test files; maps test suites to
//! declared source imports."*

mod common;

use autopsy_domain::CoverageState;
use autopsy_test_impact::{
    TestCaseKind, TestFramework, discover_inventory, inventory::is_test_file,
};
use common::*;

// ---------------------------------------------------------------------------
// Gate: all three frameworks are discovered
// ---------------------------------------------------------------------------

#[test]
fn step33_discovers_vitest_jest_and_mocha() {
    let inv = discover_inventory(&files(&[
        ("src/main.test.ts", MAIN_TEST_TS),
        ("src/mid.test.ts", MID_TEST_TS),
        ("test/legacy.spec.js", LEGACY_SPEC_JS),
    ]));

    assert_eq!(
        inv.len(),
        3,
        "expected three suites, got {:?}",
        inv.suites.keys()
    );

    assert_eq!(
        inv.suites["src/main.test.ts"].frameworks,
        vec![TestFramework::Vitest]
    );
    assert_eq!(
        inv.suites["src/mid.test.ts"].frameworks,
        vec![TestFramework::Jest]
    );
    assert_eq!(
        inv.suites["test/legacy.spec.js"].frameworks,
        vec![TestFramework::Mocha]
    );
}

// ---------------------------------------------------------------------------
// Gate: suites map to their declared source imports
// ---------------------------------------------------------------------------

#[test]
fn step33_maps_suites_to_declared_source_imports() {
    let inv = discover_inventory(&clean_repo());

    assert_eq!(
        inv.suites["src/main.test.ts"].mapped_sources,
        vec!["src/main.ts".to_string()]
    );
    assert_eq!(
        inv.suites["src/mid.test.ts"].mapped_sources,
        vec!["src/mid.ts".to_string()]
    );
    assert_eq!(
        inv.suites["src/other.test.ts"].mapped_sources,
        vec!["src/other.ts".to_string()]
    );

    // The framework import must NOT appear as a mapped source.
    for suite in inv.suites.values() {
        assert!(
            !suite.mapped_sources.iter().any(|s| s == "vitest"),
            "bare specifier leaked into mapped_sources: {:?}",
            suite.mapped_sources
        );
    }
}

#[test]
fn step33_crosses_directory_boundaries_when_resolving() {
    let inv = discover_inventory(&files(&[
        ("test/legacy.spec.js", LEGACY_SPEC_JS),
        ("src/other.ts", OTHER_TS),
    ]));
    // `../src/other` from `test/` must land on `src/other.ts` via extension probing.
    assert_eq!(
        inv.suites["test/legacy.spec.js"].mapped_sources,
        vec!["src/other.ts".to_string()]
    );
    assert_eq!(
        inv.suites["test/legacy.spec.js"].coverage,
        CoverageState::Verified
    );
}

#[test]
fn step33_resolution_requires_the_target_to_exist() {
    // The identical import, with `src/other.ts` absent from the file set:
    // the resolver must refuse to invent a target.
    let inv = discover_inventory(&files(&[("test/legacy.spec.js", LEGACY_SPEC_JS)]));
    let suite = &inv.suites["test/legacy.spec.js"];
    assert!(
        suite.mapped_sources.is_empty(),
        "{:?}",
        suite.mapped_sources
    );
    assert_eq!(
        suite.unresolved_specifiers,
        vec!["../src/other".to_string()]
    );
    assert_eq!(suite.coverage, CoverageState::Partial);
}

#[test]
fn step33_resolves_extensionless_and_index_imports() {
    let inv = discover_inventory(&files(&[
        (
            "src/a.test.ts",
            "import { x } from './lib';\nimport { y } from './pkg';\nimport { describe, it } from 'vitest';\nit('a', () => {});\n",
        ),
        ("src/lib.ts", "export const x = 1;\n"),
        ("src/pkg/index.ts", "export const y = 2;\n"),
    ]));
    assert_eq!(
        inv.suites["src/a.test.ts"].mapped_sources,
        vec!["src/lib.ts".to_string(), "src/pkg/index.ts".to_string()]
    );
}

// ---------------------------------------------------------------------------
// Test-case extraction
// ---------------------------------------------------------------------------

#[test]
fn step33_extracts_test_cases_in_file_order() {
    let inv = discover_inventory(&files(&[("src/main.test.ts", MAIN_TEST_TS)]));
    let suite = &inv.suites["src/main.test.ts"];

    let names: Vec<Option<&str>> = suite.test_cases.iter().map(|c| c.name.as_deref()).collect();
    assert_eq!(names, vec![Some("main"), Some("runs")]);

    let kinds: Vec<TestCaseKind> = suite.test_cases.iter().map(|c| c.kind).collect();
    assert_eq!(kinds, vec![TestCaseKind::Describe, TestCaseKind::Test]);

    assert_eq!(
        suite.test_cases[0].case_id(&suite.suite_id),
        "src/main.test.ts::describe::main"
    );
    assert_eq!(
        suite.test_cases[1].case_id(&suite.suite_id),
        "src/main.test.ts::test::runs"
    );
}

#[test]
fn step33_computed_test_name_is_positional_not_invented() {
    let inv = discover_inventory(&files(&[("src/dynamic.test.ts", DYNAMIC_TEST_TS)]));
    let suite = &inv.suites["src/dynamic.test.ts"];

    // The `test(name, ...)` call has no literal, so no name is fabricated.
    let computed: Vec<&autopsy_test_impact::TestCase> = suite
        .test_cases
        .iter()
        .filter(|c| c.name.is_none())
        .collect();
    assert_eq!(
        computed.len(),
        1,
        "expected one computed case: {:?}",
        suite.test_cases
    );
    let id = computed[0].case_id(&suite.suite_id);
    assert!(
        id.starts_with("src/dynamic.test.ts::test::@line"),
        "got {id}"
    );
}

#[test]
fn step33_comment_lines_are_not_declarations() {
    let inv = discover_inventory(&files(&[(
        "src/c.test.ts",
        "import { describe, it } from 'vitest';\n// it('disabled', () => {});\n/* it('also disabled', () => {}); */\n * it('still not real', () => {});\nit('real', () => {});\n",
    )]));
    let suite = &inv.suites["src/c.test.ts"];
    assert_eq!(suite.test_cases.len(), 1, "{:?}", suite.test_cases);
    assert_eq!(suite.test_cases[0].name.as_deref(), Some("real"));
}

// ---------------------------------------------------------------------------
// File recognition must not over-fire
// ---------------------------------------------------------------------------

#[test]
fn step33_test_file_detection_is_segment_based() {
    // True positives.
    for path in [
        "src/a.test.ts",
        "src/a.spec.tsx",
        "src/a.test.js",
        "test/a.ts",
        "tests/a.ts",
        "__tests__/a.tsx",
        "src/__tests__/a.js",
    ] {
        assert!(is_test_file(path), "expected {path} to be a test file");
    }

    // True negatives — the substring trap.
    for path in [
        "src/latest.ts",  // contains "test" but not ".test."
        "src/contest.js", // idem
        "src/testing.ts", // directory-ish name, not a test dir
        "src/.test.ts",   // no stem before the marker
        "src/a.test.txt", // unrecognized extension
        "src/protest.spec.css",
    ] {
        assert!(!is_test_file(path), "expected {path} NOT to be a test file");
    }
}

#[test]
fn step33_source_files_are_never_reported_as_suites() {
    let inv = discover_inventory(&clean_repo());
    for path in ["src/util.ts", "src/mid.ts", "src/main.ts", "src/other.ts"] {
        assert!(
            !inv.suites.contains_key(path),
            "{path} was misclassified as a test suite"
        );
    }
    assert_eq!(inv.len(), 3);
    assert_eq!(inv.test_case_count(), 6);
}

// ---------------------------------------------------------------------------
// Honest coverage
// ---------------------------------------------------------------------------

#[test]
fn step33_unresolvable_import_degrades_to_partial_and_names_the_specifier() {
    let inv = discover_inventory(&repo_with_unresolvable_import());
    let suite = &inv.suites["test/broken.spec.js"];

    assert_eq!(suite.coverage, CoverageState::Partial);
    assert_eq!(
        suite.unresolved_specifiers,
        vec!["./missing-helper".to_string()],
        "the offending specifier must be disclosed, not dropped"
    );
    assert_eq!(inv.coverage, CoverageState::Partial);
}

#[test]
fn step33_ambient_globals_leave_the_framework_undecided() {
    let inv = discover_inventory(&repo_with_ambient_suite());
    let suite = &inv.suites["src/ambient.test.ts"];

    assert!(
        suite.frameworks.is_empty(),
        "no framework import exists, so none may be claimed"
    );
    assert_eq!(
        suite.coverage,
        CoverageState::Partial,
        "declarations without a framework are Partial, never Verified"
    );
    // The relative import still resolves, so the suite is still mappable.
    assert_eq!(suite.mapped_sources, vec!["src/main.ts".to_string()]);
}

#[test]
fn step33_unrecognized_test_file_is_unknown_not_verified() {
    let inv = discover_inventory(&files(&[("src/empty.test.ts", EMPTY_TEST_TS)]));
    assert_eq!(
        inv.suites["src/empty.test.ts"].coverage,
        CoverageState::Unknown
    );
}

#[test]
fn step33_aggregate_coverage_takes_the_worst_case() {
    // One Unknown suite must poison the aggregate: coverage is never upgraded.
    let mut f = clean_repo();
    f.push(autopsy_test_impact::TestDiscovery {
        path: "src/empty.test.ts".to_string(),
        content: EMPTY_TEST_TS.to_string(),
    });
    let inv = discover_inventory(&f);
    assert_eq!(inv.coverage, CoverageState::Unknown);
}

#[test]
fn step33_dynamic_imports_are_not_guessed_at() {
    let inv = discover_inventory(&files(&[(
        "src/dyn.test.ts",
        "import { describe, it } from 'vitest';\nconst mod = await import(`./${name}`);\nconst req = require('./legacy');\ndescribe('d', () => { it('i', () => {}); });\n",
    )]));
    let suite = &inv.suites["src/dyn.test.ts"];
    // Neither `import(...)` nor `require(...)` is treated as a declared import,
    // so neither is silently bound to a wrong file.
    assert!(
        suite.mapped_sources.is_empty(),
        "{:?}",
        suite.mapped_sources
    );
    assert!(suite.unresolved_specifiers.is_empty());
}

// ---------------------------------------------------------------------------
// Lookups used by Step 35
// ---------------------------------------------------------------------------

#[test]
fn step33_suites_importing_reports_reverse_mapping() {
    let inv = discover_inventory(&clean_repo());
    let importers: Vec<&str> = inv
        .suites_importing("src/main.ts")
        .iter()
        .map(|s| s.suite_id.as_str())
        .collect();
    assert_eq!(importers, vec!["src/main.test.ts"]);
    assert!(inv.suites_importing("src/nonexistent.ts").is_empty());
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn step33_inventory_digest_is_deterministic_and_input_order_independent() {
    let a = discover_inventory(&clean_repo());

    // Same files, shuffled input order, plus a duplicate source file.
    let mut shuffled = clean_repo();
    shuffled.reverse();
    let b = discover_inventory(&shuffled);

    assert_eq!(
        a.digest(),
        b.digest(),
        "digest must not depend on input order"
    );
    assert_eq!(a, b);
}

#[test]
fn step33_digest_changes_when_a_mapping_changes() {
    let a = discover_inventory(&clean_repo());
    let mut f = clean_repo();
    f.iter_mut()
        .find(|x| x.path == "src/main.test.ts")
        .unwrap()
        .content = "import { describe, it } from 'vitest';\nimport { compute } from './mid';\ndescribe('m', () => { it('i', () => {}); });\n".to_string();
    let b = discover_inventory(&f);
    assert_ne!(a.digest(), b.digest());
}

#[test]
fn step33_known_files_contains_every_file_not_only_tests() {
    let inv = discover_inventory(&clean_repo());
    assert!(inv.known_files.contains("src/util.ts"));
    assert!(inv.known_files.contains("src/main.test.ts"));
    assert_eq!(inv.known_files.len(), 7);
}

#[test]
fn step33_empty_input_is_a_valid_empty_inventory() {
    let inv = discover_inventory(&[]);
    assert!(inv.is_empty());
    assert_eq!(inv.test_case_count(), 0);
    assert_eq!(inv.coverage, CoverageState::Verified);
    // Deterministic and stable across calls with no content.
    assert_eq!(inv.digest(), discover_inventory(&[]).digest());
}
