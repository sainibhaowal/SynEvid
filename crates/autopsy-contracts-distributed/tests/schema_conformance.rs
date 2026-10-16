//! Schema/struct drift guard for `schemas/distributed-contract.schema.json`.
//!
//! The repository's pre-commit `schema-validation` hook only checks that
//! `schemas/*.json` parses. That is not the failure mode that hurts: a schema
//! can stay valid JSON forever while the Rust enum it enumerates quietly gains
//! a variant, and every downstream consumer then rejects a document the engine
//! considers valid.
//!
//! These tests enumerate the Rust side exhaustively via [`ChangeKind::ALL`] and
//! compare against the schema on disk, through `serde` rather than through a
//! hand-written string, so a `#[serde(rename)]` cannot drift unnoticed.

use autopsy_contracts_distributed::openapi::OpenApiSpec;
use autopsy_contracts_distributed::{
    AsyncApiSpec, ChangeKind, CompatibilityVerdict, ContractDelta, DocFormat, Severity,
    SpecDocument, diff_asyncapi, diff_openapi, parse_asyncapi, parse_openapi,
};
use serde_json::Value;
use std::collections::BTreeSet;

const SCHEMA_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/distributed-contract.schema.json"
);

fn schema() -> Value {
    let text = std::fs::read_to_string(SCHEMA_PATH)
        .unwrap_or_else(|e| panic!("cannot read {SCHEMA_PATH}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{SCHEMA_PATH} is not valid JSON: {e}"))
}

fn parse(json: &str) -> OpenApiSpec {
    let doc = SpecDocument::parse("api.json", json, DocFormat::Json).unwrap();
    parse_openapi(&doc).unwrap()
}

fn tag<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .expect("serializes")
        .as_str()
        .expect("variant serializes to a string")
        .to_string()
}

/// Two documents that together exercise all three verdicts and every severity.
fn deltas() -> Vec<(&'static str, ContractDelta)> {
    let base = r#"{
      "openapi": "3.2.1",
      "paths": {
        "/pets": { "get": {
          "parameters": [ { "name": "limit", "in": "query", "required": false,
                            "schema": { "type": "integer" } } ],
          "responses": { "200": { "description": "ok" }, "404": { "description": "x" } }
        } },
        "/pets/{petId}": { "get": { "responses": { "200": { "description": "ok" } } } }
      }
    }"#;

    let compatible_delta = diff_openapi(&parse(base), &parse(base));

    let breaking_delta = {
        let mut to = parse(base);
        let removed = to.paths.remove("/pets/{petId}");
        assert!(removed.is_some(), "fixture is missing the petId path");
        diff_openapi(&parse(base), &to)
    };
    assert!(
        !breaking_delta.breaking().is_empty(),
        "fixture must produce a breaking change"
    );

    let with_all_of = r#"{
      "openapi": "3.2.1",
      "paths": { "/x": { "get": {
        "parameters": [ { "name": "q", "in": "query", "schema": { "allOf": [ { "type": "string" } ] } } ],
        "responses": { "200": { "description": "ok" } }
      } } }
    }"#;
    let unclassified_delta = diff_openapi(&parse(with_all_of), &parse(with_all_of));
    assert!(!unclassified_delta.unclassified().is_empty());

    // An AsyncAPI delta, so the Step 38 kinds are exercised by every structural
    // check above rather than only by the exhaustive enum comparison.
    let async_source = r###"{
      "asyncapi": "3.1.0",
      "channels": {
        "orders": {
          "address": "orders",
          "messages": {
            "M": { "payload": { "type": "object", "properties": { "a": { "type": "string" } } } }
          }
        }
      },
      "operations": {
        "p": { "action": "send", "channel": { "$ref": "#/channels/orders" } }
      }
    }"###;
    let async_to = async_source.replace(
        "\"properties\": { \"a\": { \"type\": \"string\" } }",
        "\"properties\": { \"a\": { \"type\": \"string\" }, \"b\": { \"type\": \"string\" } }",
    );
    assert_ne!(async_to, async_source, "fixture edit was a no-op");
    let asyncapi_delta = diff_asyncapi(&parse_async(async_source), &parse_async(&async_to));

    vec![
        ("compatible", compatible_delta),
        ("breaking", breaking_delta),
        ("unknown", unclassified_delta),
        ("asyncapi", asyncapi_delta),
    ]
}

fn parse_async(json: &str) -> AsyncApiSpec {
    let doc = SpecDocument::parse("a.json", json, DocFormat::Json).unwrap();
    parse_asyncapi(&doc).unwrap()
}

fn keys(value: &Value) -> Vec<String> {
    value.as_object().expect("object").keys().cloned().collect()
}

fn assert_key_contract(value: &Value, schema: &Value, ctx: &str) {
    let actual = keys(value);
    let object = schema.as_object().expect("schema is an object");

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
            "{ctx}: schema requires `{field}` but serialisation omitted it. Keys: {actual:?}"
        );
    }

    let closed = object
        .get("additionalProperties")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if closed {
        let props = object.get("properties").and_then(Value::as_object).unwrap();
        for field in &actual {
            assert!(
                props.contains_key(field),
                "{ctx}: serialisation emitted `{field}` which the schema does not declare"
            );
        }
    }
}

fn schema_strings(schema: &Value) -> BTreeSet<String> {
    schema
        .as_array()
        .expect("enum array")
        .iter()
        .map(|v| v.as_str().expect("enum member is a string").to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Structural contract
// ---------------------------------------------------------------------------

#[test]
fn schema_is_closed_and_draft_2020_12() {
    let s = schema();
    assert_eq!(s["additionalProperties"], Value::Bool(false));
    assert_eq!(s["$schema"], "https://json-schema.org/draft/2020-12/schema");
    assert_eq!(s["properties"]["protocol"]["enum"][0], "openapi");
}

#[test]
fn every_serialized_field_is_declared_and_every_required_field_is_emitted() {
    let s = schema();
    let change_schema = &s["properties"]["changes"]["items"];
    let version_schema = &s["properties"]["version_change"]["oneOf"][1];

    for (ctx, delta) in deltas() {
        let value = serde_json::to_value(&delta).unwrap();
        assert_key_contract(&value, &s, ctx);
        for change in value["changes"].as_array().unwrap() {
            assert_key_contract(change, change_schema, &format!("{ctx}.change"));
        }
        if !value["version_change"].is_null() {
            assert_key_contract(
                &value["version_change"],
                version_schema,
                &format!("{ctx}.version"),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Exhaustive enum coverage — the reason this test file exists
// ---------------------------------------------------------------------------

#[test]
fn the_schema_enumerates_every_change_kind_the_engine_can_emit() {
    let declared =
        schema_strings(&schema()["properties"]["changes"]["items"]["properties"]["kind"]["enum"]);
    let actual: BTreeSet<String> = ChangeKind::ALL.iter().map(tag).collect();

    let missing: Vec<&String> = actual.difference(&declared).collect();
    let extra: Vec<&String> = declared.difference(&actual).collect();
    assert!(
        missing.is_empty(),
        "the schema is missing change kinds the engine emits: {missing:?}. Add them to \
         schemas/distributed-contract.schema.json."
    );
    assert!(
        extra.is_empty(),
        "the schema declares change kinds the engine cannot emit: {extra:?}"
    );
    assert_eq!(ChangeKind::ALL.len(), 53, "ALL drifted from the enum");
}

#[test]
fn the_schema_enumerates_every_severity_the_engine_can_emit() {
    let declared = schema_strings(
        &schema()["properties"]["changes"]["items"]["properties"]["severity"]["enum"],
    );
    let actual: BTreeSet<String> = [Severity::Breaking, Severity::Unknown, Severity::NonBreaking]
        .iter()
        .map(tag)
        .collect();
    assert_eq!(declared, actual);

    // And every kind's default severity is one of them.
    for kind in ChangeKind::ALL {
        let s = tag(&kind.default_severity());
        assert!(
            declared.contains(&s),
            "unclassified severity `{s}` from {kind:?}"
        );
    }
}

#[test]
fn the_schema_enumerates_every_verdict_the_engine_can_emit() {
    let declared = schema_strings(&schema()["properties"]["verdict"]["enum"]);
    let actual: BTreeSet<String> = [
        CompatibilityVerdict::Compatible,
        CompatibilityVerdict::Breaking,
        CompatibilityVerdict::Unknown,
    ]
    .iter()
    .map(tag)
    .collect();
    assert_eq!(declared, actual);

    // Every verdict the fixtures produced must be one the schema allows.
    for (ctx, delta) in deltas() {
        let v = tag(&delta.verdict);
        assert!(declared.contains(&v), "{ctx}: verdict `{v}` not in schema");
    }
}

#[test]
fn the_fixtures_actually_produce_all_three_verdicts() {
    // Guards the other direction: if the fixtures silently stopped producing a
    // verdict, the enum-coverage tests above would still pass while checking
    // less than they claim to.
    let all = deltas();
    assert_eq!(all.len(), 4, "a fixture went missing");
    let produced: BTreeSet<String> = all.iter().map(|(_, d)| tag(&d.verdict)).collect();
    assert_eq!(
        produced,
        ["compatible", "breaking", "unknown"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    );
}

#[test]
fn coverage_values_are_within_the_declared_enum() {
    let declared = schema_strings(&schema()["properties"]["coverage"]["enum"]);
    let change_schema =
        &schema()["properties"]["changes"]["items"]["properties"]["coverage"]["enum"];
    let declared_change = schema_strings(change_schema);

    for (ctx, delta) in deltas() {
        let value = serde_json::to_value(&delta).unwrap();
        let c = value["coverage"].as_str().unwrap().to_string();
        assert!(declared.contains(&c), "{ctx}: coverage `{c}` not in schema");
        for change in value["changes"].as_array().unwrap() {
            let cc = change["coverage"].as_str().unwrap().to_string();
            assert!(declared_change.contains(&cc), "{ctx}: `{cc}` not in schema");
        }
    }
}

// ---------------------------------------------------------------------------
// Field-level contracts
// ---------------------------------------------------------------------------

#[test]
fn digests_are_lowercase_blake3_hex() {
    for (ctx, delta) in deltas() {
        for digest in [&delta.digest, &delta.from_digest, &delta.to_digest] {
            assert_eq!(digest.len(), 64, "{ctx}");
            assert!(
                digest
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "{ctx}: `{digest}` is not lowercase hex"
            );
        }
    }
}

#[test]
fn notes_are_sorted_and_unique() {
    for (ctx, delta) in deltas() {
        let mut sorted = delta.notes.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            delta.notes, sorted,
            "{ctx}: notes are not sorted and unique"
        );
    }
}

#[test]
fn changes_are_sorted_and_unique() {
    // The ordering rule the schema documents is a real precondition for the
    // delta digest being stable, so it is asserted rather than assumed.
    for (ctx, delta) in deltas() {
        let mut sorted = delta.changes.clone();
        sorted.sort_by(|a, b| {
            a.kind
                .cmp(&b.kind)
                .then_with(|| a.subject.cmp(&b.subject))
                .then_with(|| a.detail.cmp(&b.detail))
        });
        assert_eq!(
            delta.changes, sorted,
            "{ctx}: changes are not canonically ordered"
        );
    }
}

#[test]
fn a_compatible_verdict_implies_verified_coverage_and_no_unclassified_change() {
    // This is the schema's central semantic promise. If it ever fails, the
    // format has become an approval channel for unknown state.
    for (ctx, delta) in deltas() {
        if delta.verdict == CompatibilityVerdict::Compatible {
            assert_eq!(
                delta.coverage,
                autopsy_domain::CoverageState::Verified,
                "{ctx}: `compatible` over non-verified coverage"
            );
            assert!(
                delta.unclassified().is_empty(),
                "{ctx}: `compatible` with unclassified changes: {:?}",
                delta.unclassified()
            );
            assert!(
                delta.breaking().is_empty(),
                "{ctx}: `compatible` with breaking changes"
            );
        }
    }
}

#[test]
fn is_approval_admits_only_the_compatible_verdict() {
    // A consumer must not write `verdict != Breaking`, which would silently
    // promote `unknown` to a pass.
    assert!(CompatibilityVerdict::Compatible.is_approval());
    assert!(!CompatibilityVerdict::Breaking.is_approval());
    assert!(!CompatibilityVerdict::Unknown.is_approval());

    for (ctx, delta) in deltas() {
        assert_eq!(
            delta.verdict.is_approval(),
            delta.verdict == CompatibilityVerdict::Compatible,
            "{ctx}"
        );
    }
}
