//! Schema/struct drift guard for `schemas/test-selection.schema.json`.
//!
//! The repository's pre-commit `schema-validation` hook only checks that
//! `schemas/*.json` is *parseable*. That is not the failure mode that actually
//! hurts: a schema can stay valid JSON forever while the Rust structure it
//! describes quietly gains, loses, or renames a field, and downstream
//! consumers then validate against a contract the engine no longer honours.
//!
//! These tests close that gap by serialising **real** plans and checking them
//! against the schema on disk. They are deliberately targeted rather than a
//! general JSON Schema engine — a partial validator that silently skips the
//! hard keywords would be worse than no validator at all.

mod common;

use autopsy_test_impact::{
    CoverageMatrix, SelectionFallback, merge_matrices, parse_lcov, select_affected_tests,
};
use common::*;
use serde_json::Value;

const SCHEMA_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/test-selection.schema.json"
);

fn schema() -> Value {
    let text = std::fs::read_to_string(SCHEMA_PATH)
        .unwrap_or_else(|e| panic!("cannot read {SCHEMA_PATH}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{SCHEMA_PATH} is not valid JSON: {e}"))
}

fn keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .expect("value is an object")
        .keys()
        .cloned()
        .collect()
}

/// Asserts the object's keys match the schema's contract exactly.
///
/// Checks `required` is a subset of the keys *and*, when the schema declares
/// `additionalProperties: false`, that the keys are a subset of the declared
/// properties. Both directions matter: the first catches a reader that would
/// hit a missing field, the second catches a field the schema forgot to
/// document.
fn assert_key_contract(value: &Value, schema: &Value, ctx: &str) {
    let actual = keys(value);
    let object = schema
        .as_object()
        .unwrap_or_else(|| panic!("{ctx}: schema is not an object"));

    let required: Vec<String> = object
        .get("required")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    for field in &required {
        assert!(
            actual.contains(field),
            "{ctx}: schema requires `{field}` but serialisation omitted it. Actual keys: {actual:?}"
        );
    }

    let closed = object
        .get("additionalProperties")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if closed && let Some(props) = object.get("properties").and_then(Value::as_object) {
        for field in &actual {
            assert!(
                props.contains_key(field),
                "{ctx}: serialisation emitted `{field}` but the schema does not declare it. \
                 Schema keys: {:?}",
                props.keys().collect::<Vec<_>>()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The three fallback shapes are all covered, so the `oneOf` cannot rot
// ---------------------------------------------------------------------------

fn plan_with_fallback_none() -> autopsy_test_impact::SelectionPlan {
    let inv = autopsy_test_impact::discover_inventory(&clean_repo());
    let cov = merge_matrices(vec![
        parse_lcov("src/main.test.ts", LCOV_MAIN_SUITE).unwrap(),
        parse_lcov("src/mid.test.ts", LCOV_MID_SUITE).unwrap(),
    ]);
    select_affected_tests(&change_add(), &inv, &cov, &symbol_graph()).unwrap()
}

fn plan_with_package_fallback() -> autopsy_test_impact::SelectionPlan {
    let inv = autopsy_test_impact::discover_inventory(&clean_repo());
    select_affected_tests(
        &change_add(),
        &inv,
        &CoverageMatrix::default(),
        &symbol_graph(),
    )
    .unwrap()
}

fn plan_with_full_run_fallback() -> autopsy_test_impact::SelectionPlan {
    let inv = autopsy_test_impact::discover_inventory(&clean_repo());
    let mut cs = change_add();
    cs.unknown_symbols
        .insert("src/util.ts::add.#eval".to_string());
    select_affected_tests(&cs, &inv, &CoverageMatrix::default(), &symbol_graph()).unwrap()
}

fn all_plans() -> Vec<(&'static str, autopsy_test_impact::SelectionPlan)> {
    vec![
        ("fallback=none", plan_with_fallback_none()),
        ("fallback=run_package_tests", plan_with_package_fallback()),
        ("fallback=run_all_tests", plan_with_full_run_fallback()),
    ]
}

// ---------------------------------------------------------------------------
// Top-level contract
// ---------------------------------------------------------------------------

#[test]
fn schema_is_parseable_and_close_to_extension() {
    let s = schema();
    assert_eq!(
        s["additionalProperties"],
        Value::Bool(false),
        "the plan schema must be closed, so an undeclared field is a test failure"
    );
    assert_eq!(s["$schema"], "https://json-schema.org/draft/2020-12/schema");
}

#[test]
fn every_serialized_field_is_declared_and_every_required_field_is_emitted() {
    let s = schema();
    for (ctx, plan) in all_plans() {
        let value = serde_json::to_value(&plan).unwrap();
        assert_key_contract(&value, &s, ctx);
    }
}

#[test]
fn the_plans_cover_every_fallback_variant_the_schema_declares() {
    // Guards the other direction from the key contract: if a fourth
    // `SelectionFallback` variant is added, this fails rather than silently
    // shipping an artifact the schema's `oneOf` rejects.
    let declared: Vec<String> = {
        let mut d = s_variants();
        d.sort();
        d
    };
    let mut produced: Vec<String> = Vec::new();
    for (_, plan) in all_plans() {
        produced.push(fallback_kind(&plan.fallback).to_string());
    }
    produced.sort();
    produced.dedup();
    assert_eq!(
        produced, declared,
        "the schema must declare exactly the fallback kinds the engine emits"
    );
}

fn s_variants() -> Vec<String> {
    schema()["properties"]["fallback"]["oneOf"]
        .as_array()
        .expect("fallback is a oneOf")
        .iter()
        .map(|branch| {
            branch["properties"]["kind"]["const"]
                .as_str()
                .expect("each branch declares a `kind` const")
                .to_string()
        })
        .collect()
}

fn fallback_kind(f: &SelectionFallback) -> &'static str {
    match f {
        SelectionFallback::None => "none",
        SelectionFallback::RunPackageTests { .. } => "run_package_tests",
        SelectionFallback::RunAllTests { .. } => "run_all_tests",
    }
}

// ---------------------------------------------------------------------------
// Field-level contracts
// ---------------------------------------------------------------------------

#[test]
fn coverage_values_are_within_the_declared_enum() {
    let allowed: Vec<String> = schema()["properties"]["coverage"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    for (ctx, plan) in all_plans() {
        let value = serde_json::to_value(&plan).unwrap();
        let actual = value["coverage"].as_str().unwrap();
        assert!(
            allowed.contains(&actual.to_string()),
            "{ctx}: coverage `{actual}` is not in the schema enum {allowed:?}"
        );
    }
}

#[test]
fn digest_matches_the_declared_pattern() {
    let s = schema();
    let pattern = s["properties"]["digest"]["pattern"].as_str().unwrap();
    assert_eq!(pattern, "^[0-9a-f]{64}$");
    for (ctx, plan) in all_plans() {
        assert_eq!(plan.digest.len(), 64, "{ctx}");
        assert!(
            plan.digest
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "{ctx}: digest `{}` is not lowercase hex",
            plan.digest
        );
    }
}

#[test]
fn reason_objects_match_their_declared_contract_and_codes() {
    let s = schema();
    let reason_schema = &s["properties"]["reasons"]["items"];
    let documented: Vec<String> = reason_schema["properties"]["code"]["description"]
        .as_str()
        .unwrap()
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect();

    for (ctx, plan) in all_plans() {
        let value = serde_json::to_value(&plan).unwrap();
        for reason in value["reasons"].as_array().unwrap() {
            assert_key_contract(reason, reason_schema, ctx);
            let code = reason["code"].as_str().unwrap();
            assert!(
                documented.contains(&code.to_string()),
                "{ctx}: reason code `{code}` is not in the schema's documented list {documented:?}"
            );
        }
        // The summary is mandatory, so the narrowing ratio is always stated.
        assert!(
            plan.reasons.iter().any(|r| r.code == "plan_summary"),
            "{ctx}: no plan_summary reason"
        );
    }
}

#[test]
fn counter_fields_are_non_negative_integers() {
    let s = schema();
    for field in ["total_suites", "total_tests"] {
        assert_eq!(s["properties"][field]["type"], "integer");
        assert_eq!(s["properties"][field]["minimum"], 0);
    }
    for (ctx, plan) in all_plans() {
        let value = serde_json::to_value(&plan).unwrap();
        for field in ["total_suites", "total_tests"] {
            assert!(value[field].is_u64(), "{ctx}: {field} is not an integer");
        }
        assert!(
            plan.selected_suites.len() <= plan.total_suites,
            "{ctx}: selected more suites than exist"
        );
    }
}

#[test]
fn array_fields_are_deduplicated_and_sorted() {
    // The schema declares `uniqueItems` on every array. Serde emits what the
    // BTreeSet held, so this is really a check that nothing upstream converted
    // a set into a Vec and lost the ordering guarantee.
    for (ctx, plan) in all_plans() {
        let value = serde_json::to_value(&plan).unwrap();
        for field in [
            "selected_tests",
            "selected_suites",
            "affected_sources",
            "unrulable_out_suites",
        ] {
            let array = value[field].as_array().unwrap();
            let items: Vec<&str> = array.iter().map(|v| v.as_str().unwrap()).collect();
            let mut sorted = items.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(items, sorted, "{ctx}: `{field}` is not sorted and unique");
        }
    }
}

#[test]
fn the_unrulable_out_field_is_empty_exactly_when_coverage_is_verified() {
    // This is the schema's semantic promise, so it is asserted rather than
    // left to prose: `verified` means everything unselected was positively
    // ruled out.
    for (ctx, plan) in all_plans() {
        let value = serde_json::to_value(&plan).unwrap();
        let unrulable = value["unrulable_out_suites"].as_array().unwrap().len();
        let coverage = value["coverage"].as_str().unwrap();
        if unrulable > 0 {
            assert_ne!(
                coverage, "verified",
                "{ctx}: a plan with un-rulable-out suites claimed `verified`"
            );
        }
    }
}
