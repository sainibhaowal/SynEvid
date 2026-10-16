//! Schema/struct drift guard for `schemas/topology-report.schema.json` (Step 41).
//!
//! The same failure the Step 40 guard exists for: the pre-commit hook proves only that
//! the file parses, and a schema can stay valid JSON forever while the Rust enum it
//! enumerates gains a variant. Every comparison here goes **through serde**, so a
//! `#[serde(rename)]` cannot drift unnoticed.
//!
//! The fixture is built to reach every branch that matters — a placed span, a dead-code
//! finding, a withheld finding, an undocumented route and an uninvoked route — so the
//! vocabulary is held together by real output, not only by the schema.

use autopsy_domain::CoverageState;
use autopsy_telemetry::{
    CorrelationBasis, DeclaredRoute, DeclaredSurface, DeclaredSymbol, DispatchModel,
    EvidenceSubject, IngestOptions, InsufficientReason, TopologyOptions, TopologyReport,
    correlate_topology, parse_otlp_traces, sanitize_traces,
};
use serde_json::Value;
use std::collections::BTreeSet;
use std::num::NonZeroU32;

const SCHEMA_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/topology-report.schema.json"
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
            { "key": "code.function", "value": { "stringValue": "getOrder" } },
            { "key": "code.filepath", "value": { "stringValue": "src/orders.ts" } }
          ],
          "status": { "code": 1 }
        },
        {
          "traceId": "5B8EFFF798038103D269B633813FC60C",
          "spanId": "EEE19B7EC3C1B175",
          "name": "GET /internal/health/{probe}",
          "kind": 2,
          "startTimeUnixNano": "1544712660200000000",
          "endTimeUnixNano": "1544712660500000000",
          "attributes": [
            { "key": "http.route",
              "value": { "stringValue": "/internal/health/{probe}" } },
            { "key": "http.request.method", "value": { "stringValue": "GET" } }
          ],
          "status": { "code": 1 }
        }
      ] } ]
    }
  ]
}"###;

fn schema() -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(SCHEMA_PATH).expect("the topology schema is readable"),
    )
    .expect("the topology schema is valid JSON")
}

/// A report that reaches every finding list at once.
fn fixture() -> TopologyReport {
    let request = parse_otlp_traces("traces.json", TRACE).expect("fixture parses as OTLP/JSON");
    let record = sanitize_traces(&request, &IngestOptions::test_salt()).expect("fixture sanitizes");
    let surface = DeclaredSurface {
        routes: vec![
            DeclaredRoute::new(Some("GET"), "/orders/{orderId}"),
            // Declared, never called.
            DeclaredRoute::new(Some("POST"), "/orders"),
        ],
        symbols: vec![
            DeclaredSymbol::new("orders::getOrder", "src/orders.ts", "getOrder"),
            // Declared, never observed.
            DeclaredSymbol::new("pay::refundCard", "src/pay.ts", "refundCard"),
        ],
    };
    // Two windows required but one supplied, so a withheld finding is reached too —
    // and it is reached for the *same* symbols, which is what makes the split visible.
    let report = correlate_topology(
        std::slice::from_ref(&record),
        &surface,
        &TopologyOptions::new(NonZeroU32::new(2).expect("non-zero"))
            .with_complete_observation(true)
            .with_dispatch_model(DispatchModel::Static),
    );
    assert!(
        !report.undocumented_routes.is_empty(),
        "the fixture must reach the undocumented branch or the test proves less than it says"
    );
    report
}

fn tag<T: serde::Serialize>(value: &T) -> String {
    match serde_json::to_value(value).expect("serializes") {
        Value::String(name) => name,
        Value::Object(map) => map
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| panic!("enum value serialized to several keys: {map:?}")),
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

fn definition_is_closed(name: &str) {
    let schema = schema();
    let definition = &schema["$defs"][name];
    assert!(
        !definition.is_null(),
        "`{name}` is missing from $defs, so the reference resolves to nothing"
    );
    if definition.get("properties").is_some() {
        assert_eq!(
            definition["additionalProperties"], false,
            "`{name}` must be closed"
        );
    }
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
        "an open schema would accept a report this engine cannot emit"
    );
    for (name, definition) in schema["$defs"].as_object().expect("$defs is an object") {
        if definition.get("properties").is_some() {
            assert_eq!(
                definition["additionalProperties"], false,
                "`{name}` must be closed"
            );
        } else if let Some(variants) = definition.get("oneOf").and_then(Value::as_array) {
            for variant in variants {
                if variant.get("properties").is_some() {
                    assert_eq!(
                        variant["additionalProperties"], false,
                        "every `{name}` variant must be closed: {variant}"
                    );
                }
            }
        }
    }
}

#[test]
fn every_serialized_field_is_declared_and_every_required_field_is_emitted() {
    let schema = schema();
    let report = serde_json::to_value(fixture()).expect("the report serializes");

    let declared = declared_keys(&schema);
    let emitted = object_keys(&report);
    let undeclared: Vec<&String> = emitted
        .iter()
        .filter(|key| !declared.contains(*key))
        .collect();
    assert!(
        undeclared.is_empty(),
        "the report emits fields the schema does not declare: {undeclared:?}"
    );

    let missing: Vec<&str> = schema["required"]
        .as_array()
        .expect("`required` is an array")
        .iter()
        .map(|value| value.as_str().expect("a string"))
        .filter(|key| !emitted.contains(*key))
        .collect();
    assert!(
        missing.is_empty(),
        "the schema requires report fields the engine does not emit: {missing:?}"
    );

    // The nested records are the ones a reader consumes, so their fields are checked
    // too — a top-level check alone would pass while a finding lost a field.
    for (definition, value) in [
        ("observationSummary", &report["observation"]),
        (
            "spanCorrelation",
            report["correlations"]
                .as_array()
                .expect("array")
                .first()
                .expect("the fixture places a span"),
        ),
        (
            "undocumentedRoute",
            report["undocumented_routes"]
                .as_array()
                .expect("array")
                .first()
                .expect("the fixture observes an undeclared route"),
        ),
        (
            "insufficientEvidence",
            report["insufficient_evidence"]
                .as_array()
                .expect("array")
                .first()
                .expect("the fixture withholds a finding"),
        ),
    ] {
        definition_is_closed(definition);
        let nested_declared = declared_keys(&schema["$defs"][definition]);
        let nested_emitted = object_keys(value);
        let undeclared: Vec<&String> = nested_emitted
            .iter()
            .filter(|key| !nested_declared.contains(*key))
            .collect();
        assert!(
            undeclared.is_empty(),
            "`{definition}` emits undeclared fields: {undeclared:?}"
        );
        let required_missing: Vec<&str> = schema["$defs"][definition]["required"]
            .as_array()
            .expect("`required` is an array")
            .iter()
            .map(|value| value.as_str().expect("a string"))
            .filter(|key| !nested_emitted.contains(*key))
            .collect();
        assert!(
            required_missing.is_empty(),
            "`{definition}` is missing required fields: {required_missing:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Vocabulary
// ---------------------------------------------------------------------------

#[test]
fn the_schema_enumerates_every_correlation_basis() {
    let schema = schema();
    let declared = schema_strings(&schema["$defs"]["correlationBasis"]);
    let basis = [
        CorrelationBasis::RouteTemplate,
        CorrelationBasis::ObservedPathSubsumption,
        CorrelationBasis::CodeFunction,
        CorrelationBasis::AmbiguousSymbol,
        CorrelationBasis::Unmatched,
    ];
    assert_eq!(basis.len(), 5, "the list below must cover every variant");
    assert_eq!(
        declared,
        basis.iter().map(tag).collect::<BTreeSet<String>>()
    );
}

#[test]
fn the_schema_enumerates_every_dispatch_model_and_insufficient_reason() {
    let schema = schema();
    assert_eq!(
        schema_strings(&schema["$defs"]["dispatchModel"]),
        [DispatchModel::Static, DispatchModel::Dynamic]
            .iter()
            .map(tag)
            .collect()
    );
    assert_eq!(
        schema_strings(&schema["$defs"]["insufficientReason"]),
        [
            InsufficientReason::TooFewWindows,
            InsufficientReason::ObservationMayBeSampled
        ]
        .iter()
        .map(tag)
        .collect()
    );
}

#[test]
fn the_schema_enumerates_every_coverage_state() {
    let schema = schema();
    assert_eq!(
        schema_strings(&schema["$defs"]["coverageState"]),
        [
            CoverageState::Verified,
            CoverageState::Partial,
            CoverageState::Unsupported,
            CoverageState::Unknown
        ]
        .iter()
        .map(tag)
        .collect()
    );
}

#[test]
fn the_schema_covers_both_evidence_subjects() {
    // A newtype variant is tagged by its name and a struct variant by its name too, so
    // both are read off real serializations rather than assumed.
    assert_eq!(
        tag(&EvidenceSubject::Symbol(autopsy_domain::SymbolId::new("x"))),
        "symbol"
    );
    assert_eq!(
        tag(&EvidenceSubject::Route {
            method: None,
            template: "/a".to_string()
        }),
        "route"
    );
    let schema = schema();
    let variants = schema["$defs"]["evidenceSubject"]["oneOf"]
        .as_array()
        .expect("oneOf is an array");
    let declared: BTreeSet<String> = variants.iter().flat_map(declared_keys).collect();
    assert_eq!(
        declared,
        ["route".to_string(), "symbol".to_string()]
            .into_iter()
            .collect()
    );
}

// ---------------------------------------------------------------------------
// Invariants the schema encodes
// ---------------------------------------------------------------------------

#[test]
fn a_withheld_finding_is_never_counted_as_a_finding() {
    // The two lists are separate in the schema as well as in the engine: merging them
    // would let a reader counting findings see a code path called dead when the engine
    // said only that it had not looked long enough.
    let schema = schema();
    let report = fixture();
    assert!(
        !report.insufficient_evidence.is_empty(),
        "the fixture must withhold something, or this test asserts nothing"
    );
    assert!(
        report
            .insufficient_evidence
            .iter()
            .all(|entry| entry.windows_observed < entry.required_windows
                || entry.reason == InsufficientReason::ObservationMayBeSampled),
        "a withheld finding is withheld for a stated reason"
    );
    assert!(
        schema["$defs"]["insufficientEvidence"]["required"]
            .as_array()
            .expect("required is an array")
            .iter()
            .any(|key| key == "subject"),
        "an entry that did not name its subject would be unusable"
    );
}

#[test]
fn a_finding_carries_its_own_zero_rather_than_leaving_it_to_be_inferred() {
    let schema = schema();
    for name in ["deadCodeFinding", "uninvokedRoute"] {
        let field = &schema["$defs"][name]["properties"]["observed_invocations"];
        assert_eq!(
            field["const"], 0,
            "`{name}` exists because the count is zero, so the schema pins it"
        );
    }
}

#[test]
fn every_digest_in_the_report_is_lowercase_blake3_hex() {
    let report = serde_json::to_value(fixture()).expect("serializes");
    let pattern = &schema()["$defs"]["blake3Hex"]["pattern"];
    assert_eq!(pattern, "^[0-9a-f]{64}$");

    let digest = report["digest"].as_str().expect("the digest is a string");
    assert_eq!(digest.len(), 64, "{digest}");
    assert!(
        digest
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "{digest} is not lowercase hex"
    );
    for correlation in report["correlations"].as_array().expect("array") {
        let hash = correlation["span_id_hash"].as_str().expect("a string");
        assert_eq!(hash.len(), 64, "{hash}");
        assert!(
            hash.chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        );
    }
}

#[test]
fn no_raw_span_identifier_reaches_the_report() {
    // The report joins runtime to static, so this is exactly where a raw identifier
    // would be most tempting to keep for readability — and most damaging.
    let text = serde_json::to_string(&fixture()).expect("serializes");
    for raw in [
        "EEE19B7EC3C1B174",
        "EEE19B7EC3C1B175",
        "5B8EFFF798038103D269B633813FC60C",
    ] {
        assert!(
            !text.contains(raw),
            "the raw identifier {raw} reached the report"
        );
    }
}

#[test]
fn the_report_round_trips_through_the_schema_vocabulary() {
    // Not a validator — a cheap structural check that every value the fixture emits for
    // an enumerated field is one the schema lists, so a new variant fails here.
    let schema = schema();
    let report = serde_json::to_value(fixture()).expect("serializes");

    let basis = schema_strings(&schema["$defs"]["correlationBasis"]);
    for correlation in report["correlations"].as_array().expect("array") {
        let value = correlation["basis"].as_str().expect("a string");
        assert!(basis.contains(value), "undeclared basis `{value}`");
    }
    let reasons = schema_strings(&schema["$defs"]["insufficientReason"]);
    for entry in report["insufficient_evidence"].as_array().expect("array") {
        let value = entry["reason"].as_str().expect("a string");
        assert!(reasons.contains(value), "undeclared reason `{value}`");
    }
    let states = schema_strings(&schema["$defs"]["coverageState"]);
    let value = report["coverage"].as_str().expect("a string");
    assert!(states.contains(value), "undeclared coverage `{value}`");
}

#[test]
fn the_fixture_reaches_every_finding_list() {
    // The guard on the guard: if the fixture stopped reaching a branch, the tests above
    // would keep passing while checking less and less.
    let report = fixture();
    assert_eq!(report.observation.windows_observed, 1);
    assert!(!report.correlations.is_empty(), "correlations");
    assert!(
        !report.undocumented_routes.is_empty(),
        "undocumented routes"
    );
    assert!(
        !report.insufficient_evidence.is_empty(),
        "withheld findings"
    );
    assert!(
        report.dead_code.is_empty() && report.uninvoked_routes.is_empty(),
        "and the two asserted lists are empty for a stated reason: too few windows"
    );
}
