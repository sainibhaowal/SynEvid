//! Step 40 acceptance: OpenTelemetry 1.44.0 ingestion and redaction.
//!
//! Gate: *"Sanitizes OTLP trace spans [R24]; strips PII, auth tokens, and raw
//! query strings; hashes span IDs with BLAKE3."*
//!
//! The fixtures are mutated by replacing exact substrings and every replacement is
//! guarded with `assert_ne!`. Two steps in this phase lost real coverage to
//! `.replace()` edits that silently did nothing, which turns the assertion that
//! follows into a tautology.

use autopsy_domain::CoverageState;
use autopsy_telemetry::{
    IngestError, IngestOptions, OccupancySubject, RedactionKind, SanitizedValue, parse_otlp_traces,
    sanitize_traces,
};

/// The token in the fixture's query string. Every test that asserts the output is
/// clean greps for literally this, so it cannot be a value nobody remembers.
const QUERY_SECRET: &str = "SECRET-TOKEN-VALUE";
/// A JWT in the fixture's `Authorization` header.
const HEADER_SECRET: &str = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.dBjftJeZ4CVP";
/// A client IP literal.
const IP_SECRET: &str = "203.0.113.9";
/// An account identifier.
const USER_SECRET: &str = "user-9981";

const TRACE: &str = r###"{
  "resourceSpans": [
    {
      "resource": {
        "attributes": [
          { "key": "service.name", "value": { "stringValue": "checkout" } }
        ]
      },
      "scopeSpans": [
        {
          "scope": { "name": "opentelemetry-instrumentation-http", "version": "0.27.0" },
          "spans": [
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
                { "key": "url.full", "value": { "stringValue": "https://api.example.com/orders/7?token=SECRET-TOKEN-VALUE" } },
                { "key": "http.request.header.authorization", "value": { "stringValue": "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.dBjftJeZ4CVP" } },
                { "key": "code.function", "value": { "stringValue": "getOrder" } },
                { "key": "code.filepath", "value": { "stringValue": "src/orders.ts" } },
                { "key": "client.address", "value": { "stringValue": "203.0.113.9" } }
              ],
              "status": { "code": 1 }
            },
            {
              "traceId": "5B8EFFF798038103D269B633813FC60C",
              "spanId": "EEE19B7EC3C1B175",
              "parentSpanId": "EEE19B7EC3C1B174",
              "name": "chargeCard",
              "kind": 1,
              "startTimeUnixNano": "1544712660200000000",
              "endTimeUnixNano": "1544712660500000000",
              "attributes": [
                { "key": "code.function", "value": { "stringValue": "chargeCard" } },
                { "key": "code.filepath", "value": { "stringValue": "src/pay.ts" } },
                { "key": "enduser.id", "value": { "stringValue": "user-9981" } }
              ],
              "status": { "code": 2 }
            }
          ]
        }
      ]
    }
  ]
}"###;

fn ingest(json: &str) -> autopsy_telemetry::TraceRecord {
    let request = parse_otlp_traces("traces.json", json).expect("fixture parses as OTLP/JSON");
    sanitize_traces(&request, &IngestOptions::test_salt()).expect("fixture sanitizes")
}

/// The serialized output, which is what a downstream consumer actually sees.
fn serialized(record: &autopsy_telemetry::TraceRecord) -> String {
    serde_json::to_string(record).expect("the record serializes")
}

fn attribute<'a>(
    record: &'a autopsy_telemetry::TraceRecord,
    span_index: usize,
    key: &str,
) -> Option<&'a SanitizedValue> {
    record
        .spans
        .get(span_index)?
        .attributes
        .iter()
        .find(|entry| entry.key == key)
        .map(|entry| &entry.value)
}

fn replace_once(source: &str, from: &str, to: &str) -> String {
    let edited = source.replace(from, to);
    assert_ne!(edited, source, "fixture edit was a no-op: {from:?}");
    edited
}

// ---------------------------------------------------------------------------
// The happy path
// ---------------------------------------------------------------------------

#[test]
fn gate_a_clean_export_ingests_to_verified_coverage() {
    let record = ingest(TRACE);
    assert_eq!(record.span_count, 2);
    assert_eq!(
        record.services,
        ["checkout".to_string()].into_iter().collect()
    );
    assert_eq!(
        record.coverage,
        CoverageState::Verified,
        "{:#?}",
        record.notes
    );

    let server = &record.spans[0];
    assert_eq!(server.route.as_deref(), Some("/orders/{id}"));
    assert_eq!(server.http_method.as_deref(), Some("GET"));
    assert_eq!(server.http_status_code, Some(200));
    assert_eq!(server.code_function.as_deref(), Some("getOrder"));
    assert_eq!(server.service.as_deref(), Some("checkout"));
    assert_eq!(server.duration_nanos, 1_000_000_000);
    assert_eq!(
        server.occupancy_key().subject,
        OccupancySubject::Route("/orders/{id}".to_string())
    );

    let internal = &record.spans[1];
    assert_eq!(
        internal.occupancy_key().subject,
        OccupancySubject::Function("chargeCard".to_string())
    );
    assert!(
        internal.parent_span_id_hash.is_some(),
        "the parent link survives as a pseudonym"
    );
    assert_eq!(record.occupancy_keys().len(), 2);
}

// ---------------------------------------------------------------------------
// The gate: auth tokens, query strings, PII
// ---------------------------------------------------------------------------

#[test]
fn gate_an_authorization_header_never_reaches_the_output() {
    let record = ingest(TRACE);
    let output = serialized(&record);
    assert!(
        !output.contains(HEADER_SECRET),
        "the bearer token reached the output"
    );
    assert!(
        !output.contains("Bearer"),
        "even the scheme prefix was kept"
    );
    assert_eq!(
        attribute(&record, 0, "http.request.header.authorization"),
        Some(&SanitizedValue::Redacted(
            RedactionKind::AuthorizationHeader
        ))
    );
    assert_eq!(
        record
            .redaction_totals
            .get(&RedactionKind::AuthorizationHeader),
        Some(&1)
    );
}

#[test]
fn gate_a_raw_query_string_is_stripped() {
    let record = ingest(TRACE);
    let output = serialized(&record);
    assert!(
        !output.contains(QUERY_SECRET),
        "the query token reached the output"
    );
    assert_eq!(
        attribute(&record, 0, "url.full"),
        Some(&SanitizedValue::Text(
            "https://api.example.com/orders/7".to_string()
        )),
        "the scheme, host and path survive; only the query is removed"
    );
    assert_eq!(
        record.redaction_totals.get(&RedactionKind::QueryString),
        Some(&1)
    );
}

#[test]
fn gate_pii_is_stripped() {
    let record = ingest(TRACE);
    let output = serialized(&record);
    assert!(
        !output.contains(IP_SECRET),
        "the client IP reached the output"
    );
    assert!(
        !output.contains(USER_SECRET),
        "the end-user id reached the output"
    );
    assert_eq!(
        attribute(&record, 0, "client.address"),
        Some(&SanitizedValue::Redacted(RedactionKind::NetworkAddress))
    );
    assert_eq!(
        attribute(&record, 1, "enduser.id"),
        Some(&SanitizedValue::Redacted(RedactionKind::UserIdentifier))
    );
}

#[test]
fn gate_span_ids_are_hashed_and_the_raw_ids_are_absent() {
    let record = ingest(TRACE);
    let output = serialized(&record);
    for span in &record.spans {
        assert_eq!(span.span_id_hash.len(), 64);
        assert!(
            span.span_id_hash
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "the pseudonym is lowercase hex"
        );
        assert_eq!(span.trace_id_hash.len(), 64);
    }
    // The raw identifiers were uppercase in the fixture and are gone from the
    // output in every spelling.
    assert!(!output.contains("EEE19B7EC3C1B174"));
    assert!(!output.contains("eee19b7ec3c1b174"));
    assert!(!output.contains("5B8EFFF798038103D269B633813FC60C"));
    assert!(!output.contains("5b8efff798038103d269b633813fc60c"));
}

#[test]
fn gate_the_same_identifier_in_two_spellings_is_one_span() {
    // OTLP says lowercase hex; exporters differ on case. Hashing the raw text
    // would give one span two pseudonyms and silently split the topology.
    let lower = replace_once(
        TRACE,
        "\"spanId\": \"EEE19B7EC3C1B174\"",
        "\"spanId\": \"eee19b7ec3c1b174\"",
    );
    let upper = ingest(TRACE);
    let lowercase = ingest(&lower);
    let hash_of = |record: &autopsy_telemetry::TraceRecord| {
        record
            .spans
            .iter()
            .find(|span| span.route.is_some())
            .map(|span| span.span_id_hash.clone())
            .expect("the routed span is present")
    };
    assert_eq!(hash_of(&upper), hash_of(&lowercase));
    assert_eq!(upper.digest, lowercase.digest);
}

#[test]
fn an_identifier_that_is_not_hex_is_an_error_not_a_degradation() {
    // Degrading would leave Step 41 correlating on an identifier it cannot
    // normalise, while reporting nothing wrong — a silent split in the topology.
    let bad = replace_once(
        TRACE,
        "\"spanId\": \"EEE19B7EC3C1B174\"",
        "\"spanId\": \"not-a-span-id!!\"",
    );
    let request = parse_otlp_traces("traces.json", &bad).expect("the reader is lossless");
    assert!(matches!(
        sanitize_traces(&request, &IngestOptions::test_salt()),
        Err(IngestError::BadIdentifier { .. })
    ));

    let short = replace_once(
        TRACE,
        "\"traceId\": \"5B8EFFF798038103D269B633813FC60C\"",
        "\"traceId\": \"5B8EFFF7\"",
    );
    let request = parse_otlp_traces("traces.json", &short).expect("the reader is lossless");
    assert!(matches!(
        sanitize_traces(&request, &IngestOptions::test_salt()),
        Err(IngestError::BadIdentifier {
            kind: "trace id",
            length: 32,
            ..
        })
    ));
}

// ---------------------------------------------------------------------------
// The property that makes redaction real
// ---------------------------------------------------------------------------

#[test]
fn span_identity_does_not_depend_on_the_secret_it_carried() {
    // This is the assertion the whole ordering rule exists for. If the digest
    // moved when the token changed, the digest would be a fingerprint of the
    // token: two runs carrying the same credential would correlate, and a guessed
    // credential could be confirmed by recomputing the digest.
    let a = ingest(TRACE);
    let b = ingest(&replace_once(
        TRACE,
        QUERY_SECRET,
        "A-COMPLETELY-DIFFERENT-TOKEN",
    ));

    // Guard first: the fixtures must genuinely contain a query to strip, or the
    // equality asserted below would hold because nothing was there to remove.
    for record in [&a, &b] {
        assert_eq!(
            record.redaction_totals.get(&RedactionKind::QueryString),
            Some(&1)
        );
    }
    // Control: the digest *is* sensitive to attribute content that survives
    // redaction, so the equality below is about the redaction and not about the
    // digest ignoring attributes altogether.
    let changed = ingest(&replace_once(
        TRACE,
        r###""key": "code.filepath", "value": { "stringValue": "src/orders.ts" }"###,
        r###""key": "code.filepath", "value": { "stringValue": "src/elsewhere.ts" }"###,
    ));
    assert_ne!(
        a.digest, changed.digest,
        "a kept attribute must move the digest"
    );

    // And the property the ordering rule exists for: the record digest does not
    // move with the secret, because the secret was never in it.
    assert_eq!(
        a.digest, b.digest,
        "the secret reached the digest, making it a fingerprint of the credential"
    );

    for (left, right) in a.spans.iter().zip(&b.spans) {
        assert_eq!(
            left.digest, right.digest,
            "the digest moved with the secret"
        );
        assert_eq!(left.span_id_hash, right.span_id_hash);
        assert_eq!(left.occupancy_key(), right.occupancy_key());
    }
    assert_eq!(a.occupancy_keys(), b.occupancy_keys());
}

#[test]
fn the_salt_changes_the_pseudonyms_and_nothing_else() {
    let with_one_salt = ingest(TRACE);
    let request = parse_otlp_traces("traces.json", TRACE).expect("parses");
    let with_another =
        sanitize_traces(&request, &IngestOptions::with_salt([9; 32])).expect("sanitizes");

    for (left, right) in with_one_salt.spans.iter().zip(&with_another.spans) {
        assert_ne!(
            left.span_id_hash, right.span_id_hash,
            "a different key must give a different pseudonym"
        );
        assert_eq!(
            left.route, right.route,
            "the correlation surface is not a pseudonym and must not move"
        );
        assert_eq!(left.occupancy_key(), right.occupancy_key());
    }
    // The record digest covers the pseudonyms, so it moves too — which is why the
    // salt has to travel with the report rather than being inferred.
    assert_ne!(with_one_salt.digest, with_another.digest);

    // And no salt information is recorded: a fingerprint of the salt would let an
    // attacker verify a guessed salt offline.
    let output = serialized(&with_one_salt);
    for byte in [9u8, 0x51] {
        assert!(!output.contains(&format!("{byte:02x}{byte:02x}{byte:02x}")));
    }
}

// ---------------------------------------------------------------------------
// Redaction is recursive, or it is theatre
// ---------------------------------------------------------------------------

#[test]
fn redaction_reaches_an_event_attribute() {
    // `exception.message` is the classic leak: the HTTP instrumentation puts the
    // failing request into it. A sanitizer that walks only the top-level
    // attributes array redacts the easy case and passes this one through.
    let with_event = replace_once(
        TRACE,
        r###""status": { "code": 2 }"###,
        r###""status": { "code": 2 },
              "events": [
                {
                  "name": "exception",
                  "timeUnixNano": "1544712660400000000",
                  "attributes": [
                    { "key": "http.request.header.x-api-key",
                      "value": { "stringValue": "live-api-key-do-not-log" } },
                    { "key": "exception.type", "value": { "stringValue": "ChargeError" } }
                  ]
                }
              ]"###,
    );
    let record = ingest(&with_event);
    let output = serialized(&record);
    assert!(
        !output.contains("live-api-key-do-not-log"),
        "the event's API key reached the output"
    );
    let event = record.spans[1]
        .events
        .first()
        .expect("the event is recorded");
    assert_eq!(event.name, SanitizedValue::Text("exception".to_string()));
    assert_eq!(
        event
            .attributes
            .iter()
            .find(|entry| entry.key == "http.request.header.x-api-key")
            .map(|entry| &entry.value),
        Some(&SanitizedValue::Redacted(RedactionKind::ApiKey))
    );
    assert_eq!(
        record.redaction_totals.get(&RedactionKind::ApiKey),
        Some(&1)
    );
}

#[test]
fn redaction_reaches_a_link_attribute() {
    let with_link = replace_once(
        TRACE,
        r###""status": { "code": 2 }"###,
        r###""status": { "code": 2 },
              "links": [
                {
                  "traceId": "11111111111111111111111111111111",
                  "spanId": "2222222222222222",
                  "attributes": [
                    { "key": "authorization", "value": { "stringValue": "Bearer abc" } }
                  ]
                }
              ]"###,
    );
    let record = ingest(&with_link);
    let link = record.spans[1].links.first().expect("the link is recorded");
    assert_eq!(link.span_id_hash.len(), 64);
    assert_eq!(
        link.attributes
            .iter()
            .find(|entry| entry.key == "authorization")
            .map(|entry| &entry.value),
        Some(&SanitizedValue::Redacted(
            RedactionKind::AuthorizationHeader
        ))
    );
    assert!(!serialized(&record).contains("Bearer"));
}

#[test]
fn redaction_reaches_the_status_message_the_specification_warns_about() {
    let with_message = replace_once(
        TRACE,
        r###""status": { "code": 2 }"###,
        r###""status": { "code": 2, "message": "card 4111 1111 1111 1111 declined" }"###,
    );
    let record = ingest(&with_message);
    let status = record.spans[1]
        .status
        .as_ref()
        .expect("the status is recorded");
    let message = status.message.as_ref().expect("the message is recorded");
    // Kept, and labelled. It is the most useful attribute for understanding the
    // failure; erasing it would make the trace useless while claiming to have
    // sanitized it.
    assert!(matches!(message, SanitizedValue::FreeText(_)));
    assert_eq!(
        record.coverage,
        CoverageState::Partial,
        "unclassified prose blocks a clean verdict"
    );
    assert!(
        record
            .notes
            .iter()
            .any(|note| note.contains("value(s) were removed"))
    );
}

#[test]
fn a_span_name_carrying_a_raw_target_is_stripped() {
    // The OpenTelemetry HTTP instrumentation names a span after the raw target
    // until a route is known, so a name is a common leak, not an edge case.
    let raw_name = replace_once(
        TRACE,
        r###""name": "GET /orders/{id}""###,
        r###""name": "GET /orders/7?token=SECRET-TOKEN-VALUE""###,
    );
    let record = ingest(&raw_name);
    let span = record
        .spans
        .iter()
        .find(|span| span.route.is_some())
        .expect("the routed span is present");
    assert_eq!(span.name, SanitizedValue::Text("GET /orders/7".to_string()));
    // The route template still comes from `http.route`, which is why stripping the
    // name does not cost the correlation.
    assert_eq!(span.route.as_deref(), Some("/orders/{id}"));
    // And the query was actually there to remove.
    assert!(
        span.redactions.contains_key(&RedactionKind::QueryString),
        "the fixture must have had a query to strip: {:?}",
        span.redactions
    );
}

#[test]
fn a_service_name_that_looks_like_an_address_is_not_mistaken_for_one() {
    // Over-redacting a structural key silently destroys the fusion this pillar
    // exists to perform. `checkout@1.0.0` is a version, not an e-mail address.
    let renamed = replace_once(
        TRACE,
        r###""stringValue": "checkout""###,
        r###""stringValue": "checkout@1.0.0""###,
    );
    let record = ingest(&renamed);
    assert_eq!(
        record.coverage,
        CoverageState::Verified,
        "{:#?}",
        record.notes
    );
    assert!(
        record.services.contains("checkout@1.0.0"),
        "the service name survived: {:?}",
        record.services
    );
    assert!(
        !record
            .redaction_totals
            .contains_key(&RedactionKind::EmailAddress)
    );
}

#[test]
fn free_text_in_an_ordinary_attribute_is_also_flagged() {
    let with_message = replace_once(
        TRACE,
        r###""key": "code.function", "value": { "stringValue": "getOrder" }"###,
        r###""key": "db.statement.message", "value": { "stringValue": "select * from users where email = 'dana@example.com'" }"###,
    );
    let record = ingest(&with_message);
    assert_eq!(record.coverage, CoverageState::Partial);
    assert!(
        attribute(&record, 0, "db.statement.message")
            .is_some_and(|value| matches!(value, SanitizedValue::FreeText(_))),
        "prose is kept and labelled, never silently trusted"
    );
}

// ---------------------------------------------------------------------------
// Honest coverage
// ---------------------------------------------------------------------------

#[test]
fn a_span_nothing_can_be_placed_against_degrades_coverage() {
    let uncorrelatable = replace_once(
        TRACE,
        r###""key": "code.function", "value": { "stringValue": "chargeCard" }"###,
        r###""key": "some.other.key", "value": { "stringValue": "chargeCard" }"###,
    );
    let record = ingest(&uncorrelatable);
    let span = &record.spans[1];
    assert!(!span.is_correlated());
    assert_eq!(span.occupancy_key().subject, OccupancySubject::Uncorrelated);
    assert_eq!(span.coverage, CoverageState::Partial);
    assert_eq!(record.coverage, CoverageState::Partial);
    // Still recorded, not dropped: a span that cannot be placed is a gap in the
    // fusion, and dropping it would make the remaining coverage look complete.
    assert_eq!(record.span_count, 2);
}

#[test]
fn dropped_data_degrades_coverage() {
    // The SDK dropped attributes before the engine ever saw them, so a claim of
    // complete observation would be a claim about data that does not exist.
    let dropped = replace_once(
        TRACE,
        r###""status": { "code": 2 }"###,
        r###""status": { "code": 2 }, "droppedAttributesCount": 3"###,
    );
    let record = ingest(&dropped);
    assert_eq!(record.coverage, CoverageState::Partial);
    assert!(
        record.spans[1]
            .notes
            .iter()
            .any(|note| note.contains("dropped")),
        "{:#?}",
        record.spans[1].notes
    );
}

#[test]
fn an_empty_export_is_unknown_never_verified() {
    let record = ingest(r###"{ "resourceSpans": [] }"###);
    assert_eq!(record.span_count, 0);
    assert_eq!(record.coverage, CoverageState::Unknown);
    assert!(
        record.notes.iter().any(|note| note.contains("no spans")),
        "{:#?}",
        record.notes
    );
    // And the reader says why it is empty rather than reporting a clean ingest.
    let request =
        parse_otlp_traces("traces.json", r###"{ "resourceSpans": [] }"###).expect("parses");
    assert!(!request.notes.is_empty());
}

#[test]
fn an_unknown_anyvalue_shape_is_recorded_rather_than_dropped() {
    // Two variants on one node means the producer is using a newer oneof member
    // this engine does not know. Collapsing it into "malformed" would lose which
    // variants were present.
    let two_variants = replace_once(
        TRACE,
        r###""key": "code.function", "value": { "stringValue": "getOrder" }"###,
        r###""key": "future.attribute", "value": { "stringValue": "a", "intValue": "1" }"###,
    );
    let request = parse_otlp_traces("traces.json", &two_variants).expect("the reader is lossless");
    assert!(!request.unmodelled_pointers.is_empty());
    assert_eq!(request.coverage, CoverageState::Partial);
    assert!(
        request
            .notes
            .iter()
            .any(|note| note.contains("intValue") && note.contains("stringValue")),
        "{:#?}",
        request.notes
    );

    let record = ingest(&two_variants);
    assert!(
        attribute(&record, 0, "future.attribute")
            .is_some_and(|value| matches!(value, SanitizedValue::Unmodelled(_))),
        "the node survives as an explicit gap"
    );
    assert_eq!(record.coverage, CoverageState::Partial);
}

#[test]
fn a_malformed_export_is_an_error_rather_than_a_partial_read() {
    assert!(parse_otlp_traces("traces.json", "not json").is_err());
    assert!(parse_otlp_traces("traces.json", r###"{ "resourceSpans": {} }"###).is_err());
    // An out-of-range span kind is rejected: defaulting it to `Unspecified` would
    // place an unknown span into Step 41's topology.
    let bad_kind = replace_once(TRACE, r###""kind": 2"###, r###""kind": 42"###);
    assert!(parse_otlp_traces("traces.json", &bad_kind).is_err());
}

// ---------------------------------------------------------------------------
// Determinism — NFR-001
// ---------------------------------------------------------------------------

#[test]
fn the_record_is_reproducible_across_100_runs() {
    let once = ingest(TRACE);
    for run in 0..100 {
        let again = ingest(TRACE);
        assert_eq!(again.digest, once.digest, "run {run} drifted");
        assert_eq!(again, once, "run {run} produced a different record");
    }
    assert_eq!(once.digest.len(), 64);
}

#[test]
fn the_export_order_does_not_change_the_record() {
    // A collector is free to export spans in any order, so record order must not
    // be export order: the same observations exported twice must agree.
    let first = ingest(TRACE);
    let mut request = parse_otlp_traces("traces.json", TRACE).expect("parses");
    let scope = &mut request.resource_spans[0].scope_spans[0];
    scope.spans.reverse();
    assert_eq!(
        scope.spans[0].name, "chargeCard",
        "the fixture was reordered"
    );
    let reversed = sanitize_traces(&request, &IngestOptions::test_salt()).expect("sanitizes");
    assert_eq!(reversed.digest, first.digest);
    assert_eq!(reversed.span_count, first.span_count);
}

#[test]
fn an_integer_attribute_reads_the_same_from_either_spelling() {
    // OTLP/JSON writes 64-bit integers as **strings**. A reader that only accepts
    // JSON numbers reads every integer attribute as absent.
    let as_number = replace_once(
        TRACE,
        r###"{ "key": "http.response.status_code", "value": { "intValue": "200" } }"###,
        r###"{ "key": "http.response.status_code", "value": { "intValue": 200 } }"###,
    );
    let from_string = ingest(TRACE);
    let from_number = ingest(&as_number);
    assert_eq!(from_string.spans[0].http_status_code, Some(200));
    assert_eq!(from_number.spans[0].http_status_code, Some(200));
    assert_eq!(from_string.digest, from_number.digest);

    // And a padding-laden spelling canonicalises to the same text.
    let padded = replace_once(
        TRACE,
        r###"{ "intValue": "200" }"###,
        r###"{ "intValue": "00200" }"###,
    );
    assert_eq!(ingest(&padded).digest, from_string.digest);
}

#[test]
fn the_occupancy_key_ignores_time_and_the_digest_does_not() {
    // Two identities, two questions. Dead-code detection asks "was this code path
    // executed in this window", which a timestamp must not change. The record
    // digest asks "is this the same observation", which it must.
    let later = replace_once(
        TRACE,
        r###""startTimeUnixNano": "1544712660000000000""###,
        r###""startTimeUnixNano": "1544799060000000000""###,
    );
    let early = ingest(TRACE);
    let late = ingest(&later);
    assert_eq!(early.occupancy_keys(), late.occupancy_keys());
    assert_eq!(
        early.spans[0].occupancy_key(),
        late.spans[0].occupancy_key()
    );
    assert_ne!(
        early.spans[0].digest, late.spans[0].digest,
        "the observation digest covers when it happened"
    );
}

#[test]
fn a_map_attribute_is_sorted_so_its_order_cannot_move_a_digest() {
    let template = r###"{
      "resourceSpans": [ {
        "resource": { "attributes": [
          { "key": "service.name", "value": { "stringValue": "checkout" } } ] },
        "scopeSpans": [ { "spans": [ {
          "traceId": "11111111111111111111111111111111",
          "spanId": "2222222222222222",
          "name": "op",
          "kind": 1,
          "attributes": [
            { "key": "code.function", "value": { "stringValue": "op" } },
            { "key": "nested", "value": { "kvlistValue": { "values": [
              { "key": "z", "value": { "stringValue": "last" } },
              { "key": "a", "value": { "stringValue": "first" } } ] } } }
          ]
        } ] } ]
      } ]
    }"###;
    let swapped = replace_once(
        template,
        r###"{ "key": "z", "value": { "stringValue": "last" } },
              { "key": "a", "value": { "stringValue": "first" } }"###,
        r###"{ "key": "a", "value": { "stringValue": "first" } },
              { "key": "z", "value": { "stringValue": "last" } }"###,
    );
    assert_eq!(ingest(template).digest, ingest(&swapped).digest);
}

#[test]
fn the_redaction_report_names_reasons_and_never_values() {
    // A per-value record would have to name the value, and a *length* is already a
    // side channel: a 40-character secret and a 16-character one distinguish
    // provider formats.
    let record = ingest(TRACE);
    let output = serialized(&record);
    for kind in record.redaction_totals.keys() {
        assert!(
            output.contains(kind.as_str()),
            "the reason `{}` is named in the output",
            kind.as_str()
        );
    }
    assert_eq!(
        record.redaction_totals.get(&RedactionKind::QueryString),
        Some(&1)
    );
    assert_eq!(
        record.redaction_totals.get(&RedactionKind::NetworkAddress),
        Some(&1)
    );
    assert_eq!(
        record.redaction_totals.get(&RedactionKind::UserIdentifier),
        Some(&1)
    );
    assert_eq!(
        record
            .redaction_totals
            .get(&RedactionKind::AuthorizationHeader),
        Some(&1)
    );
    // Four reasons, and no more: the fixture contains exactly four secrets.
    assert_eq!(
        record.redaction_totals.len(),
        4,
        "{:#?}",
        record.redaction_totals
    );
}

#[test]
fn every_redaction_kind_is_reachable_and_named() {
    assert_eq!(RedactionKind::ALL.len(), 18);
    let named: std::collections::BTreeSet<&str> = RedactionKind::ALL
        .iter()
        .map(|kind| kind.as_str())
        .collect();
    assert_eq!(named.len(), 18, "two kinds share a name");
    // Every kind that a table or shape rule can produce must be one of them.
    let record = ingest(TRACE);
    for kind in record.redaction_totals.keys() {
        assert!(
            RedactionKind::ALL.contains(kind),
            "{kind:?} is not in ALL, so the schema would not enumerate it"
        );
    }
}
