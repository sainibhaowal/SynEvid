//! Schema/struct drift guard for `schemas/telemetry-fusion.schema.json`.
//!
//! The pre-commit `schema-validation` hook only checks that `schemas/*.json`
//! parses. That is not the failure mode that hurts: a schema can stay valid JSON
//! forever while the Rust enum it enumerates quietly gains a variant, and every
//! downstream consumer then rejects a record the engine considers valid.
//!
//! These tests enumerate the Rust side exhaustively and compare against the
//! schema on disk **through serde**, not through a hand-written string, so a
//! `#[serde(rename)]` cannot drift unnoticed.

use autopsy_domain::CoverageState;
use autopsy_telemetry::{
    IngestOptions, OccupancySubject, RedactionKind, TraceRecord, parse_otlp_traces, sanitize_traces,
};
use serde_json::Value;
use std::collections::BTreeSet;

const SCHEMA_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/telemetry-fusion.schema.json"
);

const TRACE: &str = r###"{
  "resourceSpans": [
    {
      "resource": { "attributes": [
        { "key": "service.name", "value": { "stringValue": "checkout" } } ] },
      "scopeSpans": [ { "spans": [
        {
          "traceId": "5B8EFFF798038103D269B633813FC60C",
          "spanId": "EEE19B7EC3C1B174",
          "name": "GET /orders/{id}",
          "kind": 2,
          "startTimeUnixNano": "1544712660000000000",
          "endTimeUnixNano": "1544712661000000000",
          "attributes": [
            { "key": "http.route", "value": { "stringValue": "/orders/{id}" } },
            { "key": "http.request.method", "value": { "stringValue": "GET" } },
            { "key": "http.response.status_code", "value": { "intValue": "200" } },
            { "key": "url.full",
              "value": { "stringValue": "https://api.example.com/orders/7?token=abc" } },
            { "key": "nested", "value": { "kvlistValue": { "values": [
              { "key": "a", "value": { "stringValue": "b" } } ] } } },
            { "key": "listy", "value": { "arrayValue": { "values": [
              { "boolValue": true }, { "doubleValue": 1.5 } ] } } },
            { "key": "flag", "value": { "boolValue": false } },
            { "key": "opaque", "value": { "bytesValue": "aGk=" } }
          ],
          "events": [ { "name": "exception", "timeUnixNano": "1",
            "attributes": [ { "key": "exception.type", "value": { "stringValue": "X" } } ] } ],
          "links": [ { "traceId": "11111111111111111111111111111111",
            "spanId": "2222222222222222",
            "attributes": [] } ],
          "status": { "code": 2, "message": "prose that could carry anything" }
        }
      ] } ]
    }
  ]
}"###;

fn schema() -> Value {
    let text = std::fs::read_to_string(SCHEMA_PATH)
        .unwrap_or_else(|e| panic!("cannot read {SCHEMA_PATH}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{SCHEMA_PATH} is not valid JSON: {e}"))
}

fn record() -> TraceRecord {
    let request = parse_otlp_traces("traces.json", TRACE).expect("fixture parses");
    sanitize_traces(&request, &IngestOptions::test_salt()).expect("fixture sanitizes")
}

/// The serde tag of a value: for an externally tagged enum, its single key.
fn tag<T: serde::Serialize>(value: &T) -> String {
    match serde_json::to_value(value).expect("serializes") {
        Value::String(name) => name,
        Value::Object(map) => map
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| panic!("enum value serialized to more than one key: {map:?}")),
        other => panic!("enum value serialized to {other}"),
    }
}

fn schema_strings(schema: &Value) -> BTreeSet<String> {
    schema
        .get("enum")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected an `enum` array, found {schema}"))
        .iter()
        .map(|value| {
            value
                .as_str()
                .unwrap_or_else(|| panic!("enum member is not a string: {value}"))
                .to_string()
        })
        .collect()
}

fn object_keys(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("expected an object, found {value}"))
        .keys()
        .cloned()
        .collect()
}

fn declared_keys(schema: &Value) -> BTreeSet<String> {
    object_keys(&schema["properties"])
}

// ---------------------------------------------------------------------------
// Shape
// ---------------------------------------------------------------------------

#[test]
fn the_schema_is_closed_and_draft_2020_12() {
    let schema = schema();
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["type"], "object");
    assert_eq!(
        schema["additionalProperties"], false,
        "an open schema would accept a record this engine cannot emit"
    );
    assert!(
        schema["$defs"].is_object(),
        "the nested types live in $defs"
    );
    for definition in schema["$defs"]
        .as_object()
        .expect("$defs is an object")
        .values()
    {
        if definition["type"] == "object" && definition.get("properties").is_some() {
            assert_eq!(
                definition["additionalProperties"], false,
                "every object definition must be closed: {definition}"
            );
        }
    }
}

#[test]
fn every_serialized_field_is_declared_and_every_required_field_is_emitted() {
    let schema = schema();
    let emitted = serde_json::to_value(record()).expect("the record serializes");

    let declared = declared_keys(&schema);
    let actual = object_keys(&emitted);
    let undeclared: Vec<&String> = actual.difference(&declared).collect();
    assert!(
        undeclared.is_empty(),
        "the record emits fields the schema does not declare: {undeclared:?}"
    );
    let required: BTreeSet<String> = schema["required"]
        .as_array()
        .expect("`required` is an array")
        .iter()
        .map(|value| value.as_str().expect("a string").to_string())
        .collect();
    let missing: Vec<&String> = required.difference(&actual).collect();
    assert!(
        missing.is_empty(),
        "the schema requires fields the record does not emit: {missing:?}"
    );

    // The same for a span, which is the record's only nested object with a
    // declared field set.
    let span_schema = &schema["$defs"]["spanRecord"];
    let span = emitted["spans"]
        .as_array()
        .expect("spans is an array")
        .first()
        .expect("the fixture has a span");
    let span_declared = declared_keys(span_schema);
    let span_emitted = object_keys(span);
    let undeclared: Vec<&String> = span_emitted
        .iter()
        .filter(|key| !span_declared.contains(*key))
        .collect();
    assert!(undeclared.is_empty(), "span emits {undeclared:?}");
    let missing: Vec<&str> = span_schema["required"]
        .as_array()
        .expect("`required` is an array")
        .iter()
        .map(|value| value.as_str().expect("a string"))
        .filter(|key| !span_emitted.contains(*key))
        .collect();
    assert!(
        missing.is_empty(),
        "the schema requires span fields the record does not emit: {missing:?}"
    );
}

#[test]
fn every_sanitized_value_variant_is_declared() {
    // A variant the schema does not know is a record a consumer rejects. The
    // fixture is built to reach every variant, so this is an exhaustive check of
    // the enum's serialization rather than of one sample.
    let schema = schema();
    let declared: BTreeSet<String> = schema["$defs"]["sanitizedValue"]["oneOf"]
        .as_array()
        .expect("oneOf is an array")
        .iter()
        .flat_map(|variant| {
            variant
                .get("required")
                .map(|required| {
                    required
                        .as_array()
                        .expect("required is an array")
                        .iter()
                        .map(|name| name.as_str().expect("a string").to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| vec!["const".to_string()])
        })
        .collect();
    let expected: BTreeSet<String> = [
        "text",
        "free_text",
        "number",
        "flag",
        "redacted",
        "list",
        "map",
        "unmodelled",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(declared, expected);

    // And the fixture actually reaches six of the eight, so the check above is not
    // the only thing holding the vocabulary together.
    let emitted = serde_json::to_string(&record()).expect("serializes");
    for variant in ["text", "redacted", "map", "list", "flag", "number"] {
        assert!(
            emitted.contains(&format!("\"{variant}\"")),
            "the fixture never produced a `{variant}` value: {emitted}"
        );
    }
}

// ---------------------------------------------------------------------------
// Exhaustive enum coverage — the reason this test file exists
// ---------------------------------------------------------------------------

#[test]
fn the_schema_enumerates_every_redaction_kind_the_engine_can_emit() {
    let declared = schema_strings(&schema()["$defs"]["redactionKind"]);
    let actual: BTreeSet<String> = RedactionKind::ALL.iter().map(tag).collect();
    let missing: Vec<&String> = actual.difference(&declared).collect();
    let extra: Vec<&String> = declared.difference(&actual).collect();
    assert!(
        missing.is_empty(),
        "the schema is missing redaction kinds the engine emits: {missing:?}"
    );
    assert!(
        extra.is_empty(),
        "the schema declares redaction kinds the engine cannot emit: {extra:?}"
    );
    assert_eq!(RedactionKind::ALL.len(), 18, "ALL drifted from the enum");
}

#[test]
fn the_schema_enumerates_every_coverage_state_and_span_kind() {
    let declared = schema_strings(&schema()["$defs"]["coverageState"]);
    let actual: BTreeSet<String> = [
        CoverageState::Verified,
        CoverageState::Partial,
        CoverageState::Unsupported,
        CoverageState::Unknown,
    ]
    .iter()
    .map(tag)
    .collect();
    assert_eq!(declared, actual);

    let declared = schema_strings(&schema()["$defs"]["spanKind"]);
    let actual: BTreeSet<String> = [
        autopsy_telemetry::SpanKind::Unspecified,
        autopsy_telemetry::SpanKind::Internal,
        autopsy_telemetry::SpanKind::Server,
        autopsy_telemetry::SpanKind::Client,
        autopsy_telemetry::SpanKind::Producer,
        autopsy_telemetry::SpanKind::Consumer,
    ]
    .iter()
    .map(tag)
    .collect();
    assert_eq!(declared, actual);

    let declared = schema_strings(&schema()["$defs"]["statusCode"]);
    let actual: BTreeSet<String> = [
        autopsy_telemetry::StatusCode::Unset,
        autopsy_telemetry::StatusCode::Ok,
        autopsy_telemetry::StatusCode::Error,
    ]
    .iter()
    .map(tag)
    .collect();
    assert_eq!(declared, actual);
}

#[test]
fn the_schema_enumerates_every_occupancy_subject() {
    // The occupancy key is Step 41's input, so a subject the schema does not
    // declare would be a correlation the consumer cannot read.
    let declared: BTreeSet<String> = schema()["$defs"]["occupancySubject"]["oneOf"]
        .as_array()
        .expect("oneOf is an array")
        .iter()
        .map(|variant| {
            // A unit variant serializes as a bare string, so it is spelled
            // `{"const": "..."}` rather than through `required`.
            if let Some(required) = variant.get("required").and_then(Value::as_array) {
                required
                    .first()
                    .and_then(Value::as_str)
                    .expect("a required member name")
                    .to_string()
            } else {
                variant
                    .get("const")
                    .and_then(Value::as_str)
                    .expect("a unit variant spells its value with `const`")
                    .to_string()
            }
        })
        .collect();
    let actual: BTreeSet<String> = [
        OccupancySubject::Route("/x".to_string()),
        OccupancySubject::Function("f".to_string()),
        OccupancySubject::Rpc("s/m".to_string()),
        OccupancySubject::Uncorrelated,
    ]
    .iter()
    .map(tag)
    .collect();
    assert_eq!(declared, actual);
}

// ---------------------------------------------------------------------------
// Invariants the schema states, asserted on real output
// ---------------------------------------------------------------------------

#[test]
fn notes_are_sorted_and_unique() {
    let record = record();
    assert!(!record.notes.is_empty(), "the record explains itself");
    let mut sorted = record.notes.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(record.notes, sorted, "notes must be canonical");
    for span in &record.spans {
        let mut sorted = span.notes.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(span.notes, sorted, "a span's notes must be canonical");
    }
}

#[test]
fn digests_are_lowercase_blake3_hex() {
    let record = record();
    let check = |ctx: &str, digest: &str| {
        assert_eq!(digest.len(), 64, "{ctx}");
        assert!(
            digest
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "{ctx}: {digest}"
        );
    };
    check("record", &record.digest);
    for span in &record.spans {
        check("span", &span.digest);
        check("span id pseudonym", &span.span_id_hash);
        check("trace id pseudonym", &span.trace_id_hash);
        if let Some(parent) = &span.parent_span_id_hash {
            check("parent pseudonym", parent);
        }
        for link in &span.links {
            check("link trace pseudonym", &link.trace_id_hash);
        }
    }
}

#[test]
fn a_redaction_count_is_always_at_least_one() {
    // A zero would mean an entry was created without a removal, which would read
    // as "something was redacted here" when nothing was.
    let record = record();
    assert!(!record.redaction_totals.is_empty(), "the fixture redacts");
    for (kind, count) in &record.redaction_totals {
        assert!(*count >= 1, "{kind:?} has a count of {count}");
    }
    for span in &record.spans {
        for (kind, count) in &span.redactions {
            assert!(*count >= 1, "{kind:?} has a count of {count}");
        }
    }
    // The totals are the sum of the per-span counts, not an independent tally.
    let mut recomputed: std::collections::BTreeMap<RedactionKind, usize> =
        std::collections::BTreeMap::new();
    for span in &record.spans {
        for (kind, count) in &span.redactions {
            *recomputed.entry(*kind).or_insert(0) += count;
        }
    }
    assert_eq!(record.redaction_totals, recomputed);
}

#[test]
fn no_secret_and_no_raw_identifier_appears_anywhere_in_the_record() {
    // The schema cannot state this, so it is asserted on real output: a field
    // added later that carries a raw value would fail here rather than in
    // production.
    let emitted = serde_json::to_string(&record()).expect("serializes");
    for secret in [
        "token=abc",
        "Bearer ",
        "5b8efff798038103d269b633813fc60c",
        "EEE19B7EC3C1B174",
        "eee19b7ec3c1b174",
        "2222222222222222",
    ] {
        assert!(
            !emitted.contains(secret),
            "`{secret}` reached the output: {emitted}"
        );
    }
}

#[test]
fn a_free_text_or_unmodelled_value_always_costs_coverage() {
    // The schema declares `free_text` and `unmodelled`; this asserts the engine's
    // half of the bargain — that either one is a gap, never a silent success.
    let record = record();
    // Scanned over the whole span, not just its `attributes`: the fixture carries
    // its prose in `status.message`, which is exactly the field the OpenTelemetry
    // specification warns "MAY contain sensitive information", and a scan that only
    // looked at attributes would have reported no gap while one was present.
    let has_gap_text = record.spans.iter().any(|span| {
        serde_json::to_string(span)
            .map(|text| text.contains("\"free_text\"") || text.contains("\"unmodelled\""))
            .unwrap_or(false)
    });
    assert!(
        has_gap_text,
        "the fixture is meant to contain prose and an unmodelled node"
    );
    assert_ne!(
        record.coverage,
        CoverageState::Verified,
        "a record containing unclassified values cannot claim verified coverage"
    );
}
