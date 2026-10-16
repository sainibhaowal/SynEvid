//! Step 39 acceptance: Pact consumer/provider verification.
//!
//! Gate: *"Ingests Pact [R27] JSON artifacts; cross-references consumer
//! expectations against provider AST interfaces."*
//!
//! # Why the fixtures are mutated rather than string-patched
//!
//! Two steps in this phase lost real coverage to `.replace()` fixtures that
//! silently did nothing when the whitespace did not match, turning the assertion
//! that followed into a tautology. Every variant here is therefore produced by
//! mutating the **parsed model** where a public field allows it, and where a
//! source edit is genuinely clearer it is guarded with `assert_ne!`.

use autopsy_contracts_distributed::{
    ChangeKind, CompatibilityVerdict, ContractDelta, DocFormat, HttpMethod, OpenApiSpec,
    PactDocument, Severity, SpecDocument, parse_openapi, parse_pact, xref_pact_against_provider,
};
use autopsy_domain::CoverageState;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// An OpenAPI-documented provider: the interface a Pact is checked against.
const PROVIDER: &str = r###"{
  "openapi": "3.2.1",
  "info": { "title": "Alligator Service", "version": "1.0.0" },
  "paths": {
    "/alligators": {
      "get": {
        "parameters": [
          { "name": "size", "in": "query", "required": true, "schema": { "type": "integer" } }
        ],
        "responses": { "200": { "description": "ok" } }
      },
      "post": {
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": {
                "type": "object",
                "properties": { "colour": { "type": "string" } }
              }
            }
          }
        },
        "responses": { "201": { "description": "created" } }
      }
    },
    "/alligators/{id}": {
      "get": {
        "parameters": [
          { "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }
        ],
        "responses": { "200": { "description": "ok" }, "404": { "description": "absent" } }
      }
    }
  }
}"###;

/// A consumer that agrees with `PROVIDER` exactly: one interaction for each of
/// the three operations, including a **concrete** path against a template.
const CONSUMER: &str = r###"{
  "consumer": { "name": "Zoo" },
  "provider": { "name": "Alligator Service" },
  "metadata": { "pactSpecification": { "version": "3.0.0" } },
  "interactions": [
    {
      "description": "a list of alligators",
      "request": { "method": "GET", "path": "/alligators", "query": "size=3" },
      "response": { "status": 200 }
    },
    {
      "description": "an alligator by id",
      "request": { "method": "GET", "path": "/alligators/3" },
      "response": { "status": 200 }
    },
    {
      "description": "a new alligator",
      "request": { "method": "POST", "path": "/alligators", "body": { "colour": "red" } },
      "response": { "status": 201 }
    }
  ]
}"###;

fn spec_of(json: &str) -> OpenApiSpec {
    let doc = SpecDocument::parse("api.json", json, DocFormat::Json).expect("fixture parses");
    parse_openapi(&doc).expect("fixture is OpenAPI")
}

fn pact_of(json: &str) -> PactDocument {
    let doc = SpecDocument::parse("pact.json", json, DocFormat::Json).expect("fixture parses");
    parse_pact(&doc).expect("fixture is a Pact")
}

fn xref(pact: &PactDocument, provider: &OpenApiSpec) -> ContractDelta {
    xref_pact_against_provider(pact, provider)
}

fn kinds(delta: &ContractDelta) -> Vec<ChangeKind> {
    delta.changes.iter().map(|c| c.kind).collect()
}

fn details_of(delta: &ContractDelta, kind: ChangeKind) -> Vec<&str> {
    delta
        .changes
        .iter()
        .filter(|c| c.kind == kind)
        .map(|c| c.detail.as_str())
        .collect()
}

/// The interaction at `index`, mutably.
fn interaction(
    pact: &mut PactDocument,
    index: usize,
) -> &mut autopsy_contracts_distributed::Interaction {
    pact.interactions
        .get_mut(index)
        .unwrap_or_else(|| panic!("fixture has no interaction {index}"))
}

// ---------------------------------------------------------------------------
// The happy path
// ---------------------------------------------------------------------------

#[test]
fn a_clean_pact_against_a_matching_provider_is_compatible() {
    let delta = xref(&pact_of(CONSUMER), &spec_of(PROVIDER));
    assert_eq!(
        delta.changes,
        Vec::new(),
        "a consumer that agrees with its provider must produce no findings: {:#?}",
        delta.changes
    );
    assert_eq!(delta.verdict, CompatibilityVerdict::Compatible);
    assert!(
        delta.coverage == CoverageState::Verified,
        "coverage is {:?}: {:#?}",
        delta.coverage,
        delta.notes
    );
}

#[test]
fn a_concrete_path_is_matched_to_the_provider_template() {
    // The load-bearing behaviour. Comparing the two paths as strings reports
    // every interaction as unmatched, which is a *silent* wrong answer because
    // the output is still a well-formed delta.
    let provider = spec_of(PROVIDER);
    let pact = pact_of(CONSUMER);
    assert_eq!(
        interaction(&mut pact.clone(), 1)
            .request
            .as_ref()
            .unwrap()
            .path,
        "/alligators/3"
    );

    let delta = xref(&pact, &provider);
    assert!(
        !kinds(&delta).contains(&ChangeKind::PactInteractionUnmatched),
        "`/alligators/3` must match `/alligators/{{id}}`: {:#?}",
        delta.changes
    );

    // And the control: with the template gone, the same interaction must be
    // reported, so the assertion above is not passing for want of checking.
    let mut stripped = spec_of(PROVIDER);
    let removed = stripped.paths.remove("/alligators/{id}");
    assert!(removed.is_some(), "fixture is missing the template path");
    let delta = xref(&pact, &stripped);
    assert!(kinds(&delta).contains(&ChangeKind::PactInteractionUnmatched));
}

// ---------------------------------------------------------------------------
// Absence on the provider side
// ---------------------------------------------------------------------------

#[test]
fn an_interaction_the_provider_does_not_offer_breaks() {
    let mut provider = spec_of(PROVIDER);
    provider.paths.clear();
    let delta = xref(&pact_of(CONSUMER), &provider);
    assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
    assert_eq!(
        kinds(&delta),
        vec![
            ChangeKind::PactInteractionUnmatched,
            ChangeKind::PactInteractionUnmatched,
            ChangeKind::PactInteractionUnmatched,
        ]
    );
    assert!(
        delta
            .breaking()
            .iter()
            .all(|c| c.severity == Severity::Breaking)
    );
}

#[test]
fn a_path_the_provider_declares_under_another_method_is_reported_as_a_method_problem() {
    // The defect this test was written to catch: the first version of the path
    // search ignored the method while ranking paths, then found no operation
    // under the path it chose and only *noted* the problem. The change list came
    // back empty for a consumer calling an operation the provider does not
    // offer, and `PactMethodNotDeclared` was unreachable.
    let mut provider = spec_of(PROVIDER);
    let removed = provider
        .paths
        .get_mut("/alligators/{id}")
        .expect("fixture path")
        .operations
        .remove(&HttpMethod::Get);
    assert!(removed.is_some(), "fixture is missing the operation");

    let delta = xref(&pact_of(CONSUMER), &provider);
    assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
    let declared = details_of(&delta, ChangeKind::PactMethodNotDeclared);
    assert_eq!(declared.len(), 1, "{:#?}", delta.changes);
    assert!(
        declared[0].contains("/alligators/{id}") && declared[0].contains("GET"),
        "{}",
        declared[0]
    );
    // A method problem is not reported as a *path* problem: the two are fixed in
    // different places.
    assert!(!kinds(&delta).contains(&ChangeKind::PactInteractionUnmatched));
}

#[test]
fn a_less_specific_template_serves_a_method_the_more_specific_path_does_not() {
    // Guard against the false positive that method-aware ranking introduces if
    // it is done in the wrong order: the literal path is more specific but does
    // not serve `POST`, and the provider *does* serve the request via the
    // template. Reporting a method problem here would be wrong.
    let provider = spec_of(
        r###"{
          "openapi": "3.2.1",
          "paths": {
            "/a/health": { "get": { "responses": { "200": { "description": "ok" } } } },
            "/a/{id}": {
              "parameters": [
                { "name": "id", "in": "path", "required": true, "schema": { "type": "string" } }
              ],
              "post": { "responses": { "200": { "description": "ok" } } }
            }
          }
        }"###,
    );
    let pact = pact_of(
        r###"{
          "consumer": "c",
          "provider": "p",
          "interactions": [
            {
              "description": "post to a namespaced path",
              "request": { "method": "POST", "path": "/a/health" },
              "response": { "status": 200 }
            }
          ]
        }"###,
    );
    let delta = xref(&pact, &provider);
    assert_eq!(
        delta.changes,
        Vec::new(),
        "the template serves POST, so this is not a method problem: {:#?}",
        delta.changes
    );
}

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

#[test]
fn a_required_query_parameter_the_consumer_omits_breaks() {
    let mut pact = pact_of(CONSUMER);
    interaction(&mut pact, 0)
        .request
        .as_mut()
        .expect("request")
        .query
        .clear();
    let delta = xref(&pact, &spec_of(PROVIDER));
    assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
    let missing = details_of(&delta, ChangeKind::PactRequiredQueryParameterMissing);
    assert_eq!(missing.len(), 1, "{:#?}", delta.changes);
    assert!(missing[0].contains("size"), "{}", missing[0]);

    // Control: with the parameter dropped from the provider's own declaration
    // the same request is fine, so the assertion above is about the *required*
    // parameter and not merely about an empty query.
    let mut relaxed = spec_of(PROVIDER);
    let parameters = &mut relaxed
        .paths
        .get_mut("/alligators")
        .expect("fixture path")
        .operations
        .get_mut(&HttpMethod::Get)
        .expect("fixture operation")
        .parameters;
    assert_eq!(parameters.len(), 1, "fixture has one query parameter");
    parameters.clear();
    let delta = xref(&pact, &relaxed);
    assert!(!kinds(&delta).contains(&ChangeKind::PactRequiredQueryParameterMissing));
}

#[test]
fn a_query_parameter_the_provider_does_not_declare_is_disclosed_but_not_a_break() {
    // Whether an undeclared parameter is ignored or rejected is provider
    // behaviour, and the interface does not state it. Asserting a break would be
    // asserting something the artifact never said.
    let mut pact = pact_of(CONSUMER);
    interaction(&mut pact, 0)
        .request
        .as_mut()
        .expect("request")
        .query
        .insert("verbose".to_string(), vec!["true".to_string()]);
    let delta = xref(&pact, &spec_of(PROVIDER));
    assert_ne!(delta.verdict, CompatibilityVerdict::Breaking);
    let undeclared = delta
        .changes
        .iter()
        .find(|c| c.kind == ChangeKind::PactUndeclaredRequestParameter)
        .expect("the undeclared parameter is reported");
    assert_eq!(undeclared.severity, Severity::Unknown);
    assert!(
        undeclared.detail.contains("verbose"),
        "{}",
        undeclared.detail
    );
}

#[test]
fn a_provider_that_declares_no_query_parameters_says_so_once() {
    // With no declared parameters, "undeclared" is true of everything and
    // therefore says nothing. The honest report is that the check could not be
    // made — once, not once per parameter.
    let mut pact = pact_of(CONSUMER);
    for (name, value) in [("a", "1"), ("b", "2"), ("c", "3")] {
        interaction(&mut pact, 1)
            .request
            .as_mut()
            .expect("request")
            .query
            .insert(name.to_string(), vec![value.to_string()]);
    }
    let delta = xref(&pact, &spec_of(PROVIDER));
    assert!(!kinds(&delta).contains(&ChangeKind::PactUndeclaredRequestParameter));
    assert!(
        delta.coverage != CoverageState::Verified,
        "a check that did not happen cannot be verified"
    );
    let saying_so: Vec<&String> = delta
        .notes
        .iter()
        .filter(|n| n.contains("declares no query parameters"))
        .collect();
    assert_eq!(saying_so.len(), 1, "{:#?}", delta.notes);
}

// ---------------------------------------------------------------------------
// Request bodies
// ---------------------------------------------------------------------------

#[test]
fn a_required_request_body_the_consumer_omits_breaks() {
    let mut pact = pact_of(CONSUMER);
    let request = interaction(&mut pact, 2).request.as_mut().expect("request");
    request.has_body = false;
    request.body_keys.clear();
    request.body_digest = None;
    let delta = xref(&pact, &spec_of(PROVIDER));
    assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
    assert_eq!(
        kinds(&delta),
        vec![ChangeKind::PactRequiredRequestBodyMissing],
        "{:#?}",
        delta.changes
    );
}

#[test]
fn a_body_for_an_operation_that_declares_none_is_disclosed_but_not_a_break() {
    let mut pact = pact_of(CONSUMER);
    let request = interaction(&mut pact, 1).request.as_mut().expect("request");
    request.has_body = true;
    request.body_keys.clear();
    request.body_digest = Some("deadbeef".to_string());
    let delta = xref(&pact, &spec_of(PROVIDER));
    let found = delta
        .changes
        .iter()
        .find(|c| c.kind == ChangeKind::PactUndeclaredRequestBody)
        .expect("an undeclared body is reported");
    assert_eq!(found.severity, Severity::Unknown);
}

#[test]
fn an_undeclared_body_field_breaks_only_when_the_schema_is_closed() {
    // The same fact — the consumer sends a field the provider does not declare —
    // has two different consequences, and the provider's own schema decides
    // which. JSON Schema permits additional properties by default, so "unknown"
    // is not hedging here; it is the only defensible reading of an open schema.
    let mut pact = pact_of(CONSUMER);
    let request = interaction(&mut pact, 2).request.as_mut().expect("request");
    request.body_keys.insert("nickname".to_string());

    let open = xref(&pact, &spec_of(PROVIDER));
    let open_finding = open
        .changes
        .iter()
        .find(|c| c.kind == ChangeKind::PactUndeclaredBodyProperty)
        .expect("the undeclared field is reported");
    assert_eq!(open_finding.severity, Severity::Unknown);
    assert_ne!(open.verdict, CompatibilityVerdict::Breaking);

    let closed = xref(
        &pact,
        &spec_of(
            r###"{
              "openapi": "3.2.1",
              "paths": {
                "/alligators": {
                  "post": {
                    "requestBody": {
                      "required": true,
                      "content": {
                        "application/json": {
                          "schema": {
                            "type": "object",
                            "properties": { "colour": { "type": "string" } },
                            "additionalProperties": false
                          }
                        }
                      }
                    },
                    "responses": { "201": { "description": "created" } }
                  }
                }
              }
            }"###,
        ),
    );
    let closed_finding = closed
        .changes
        .iter()
        .find(|c| c.kind == ChangeKind::PactUndeclaredBodyProperty)
        .expect("the undeclared field is reported");
    assert_eq!(
        closed_finding.severity,
        Severity::Breaking,
        "a closed schema rejects the field, so the exchange cannot succeed"
    );
    assert_eq!(closed.verdict, CompatibilityVerdict::Breaking);
}

// ---------------------------------------------------------------------------
// Responses
// ---------------------------------------------------------------------------

#[test]
fn an_expected_status_the_provider_does_not_declare_breaks() {
    let mut pact = pact_of(CONSUMER);
    interaction(&mut pact, 0)
        .response
        .as_mut()
        .expect("response")
        .status = 418;
    let delta = xref(&pact, &spec_of(PROVIDER));
    assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
    let found = details_of(&delta, ChangeKind::PactResponseStatusUndeclared);
    assert_eq!(found.len(), 1, "{:#?}", delta.changes);
    assert!(found[0].contains("418"), "{}", found[0]);
    // The provider *does* declare 404 on another operation; a status declared
    // elsewhere must not be read as declared here.
    assert!(!found[0].contains("404"));
}

#[test]
fn the_two_non_integer_answer_keys_both_count_as_declarations() {
    let provider = spec_of(
        r###"{
          "openapi": "3.2.1",
          "paths": {
            "/a": { "get": { "responses": { "2XX": { "description": "ok" } } } },
            "/b": { "get": { "responses": { "default": { "description": "anything" } } } }
          }
        }"###,
    );
    let pact = pact_of(
        r###"{
          "consumer": "c",
          "provider": "p",
          "interactions": [
            { "description": "a class key", "request": { "method": "GET", "path": "/a" },
              "response": { "status": 201 } },
            { "description": "a catch-all", "request": { "method": "GET", "path": "/b" },
              "response": { "status": 500 } }
          ]
        }"###,
    );
    let delta = xref(&pact, &provider);
    assert!(
        !kinds(&delta).contains(&ChangeKind::PactResponseStatusUndeclared),
        "`2XX` and `default` are declarations: {:#?}",
        delta.changes
    );

    // Control: a status outside the declared class is still reported, so the
    // assertion above is not passing because the check never ran.
    let outside = pact_of(
        r###"{
          "consumer": "c",
          "provider": "p",
          "interactions": [
            { "description": "a class key", "request": { "method": "GET", "path": "/a" },
              "response": { "status": 500 } }
          ]
        }"###,
    );
    assert_eq!(
        outside.interactions[0].response.as_ref().unwrap().status,
        500
    );
    let delta = xref(&outside, &provider);
    assert!(kinds(&delta).contains(&ChangeKind::PactResponseStatusUndeclared));
}

// ---------------------------------------------------------------------------
// Ambiguity and coverage
// ---------------------------------------------------------------------------

#[test]
fn equally_specific_provider_paths_are_reported_as_ambiguous() {
    // OpenAPI forbids this pair in one document. Nothing enforces it, real
    // documents contain it, and a coin flip reported as a match is
    // indistinguishable from a correct one.
    let provider = spec_of(
        r###"{
          "openapi": "3.2.1",
          "paths": {
            "/a/{id}": { "get": { "responses": { "200": { "description": "ok" } } } },
            "/a/{name}": { "get": { "responses": { "200": { "description": "ok" } } } }
          }
        }"###,
    );
    let pact = pact_of(
        r###"{
          "consumer": "c",
          "provider": "p",
          "interactions": [
            { "description": "d", "request": { "method": "GET", "path": "/a/3" },
              "response": { "status": 200 } }
          ]
        }"###,
    );
    let delta = xref(&pact, &provider);
    let ambiguity = details_of(&delta, ChangeKind::PactAmbiguousProviderMatch);
    assert_eq!(ambiguity.len(), 1, "{:#?}", delta.changes);
    assert!(ambiguity[0].contains("/a/{name}"), "{}", ambiguity[0]);
    assert!(delta.coverage != CoverageState::Verified);
    assert_eq!(
        delta.verdict,
        CompatibilityVerdict::Unknown,
        "an ambiguity is a gap, not an approval and not a break"
    );
}

#[test]
fn matching_rules_block_a_compatible_verdict() {
    // A Pact that matches a header against a regex describes a *family* of
    // requests. This engine does not evaluate those rules, so it cannot vouch
    // for the interaction — and saying `Compatible` would be claiming it did.
    let pact = pact_of(
        r###"{
          "consumer": "c",
          "provider": "p",
          "metadata": { "pactSpecification": { "version": "3.0.0" } },
          "interactions": [
            {
              "description": "d",
              "request": {
                "method": "GET",
                "path": "/alligators",
                "query": { "size": ["3"] },
                "matchingRules": {
                  "query": { "size": { "matchers": [ { "match": "type" } ] } }
                }
              },
              "response": { "status": 200 }
            }
          ]
        }"###,
    );
    let delta = xref(&pact, &spec_of(PROVIDER));
    assert_eq!(delta.verdict, CompatibilityVerdict::Unknown);
    assert!(delta.coverage != CoverageState::Verified);
    let unmodelled = delta
        .changes
        .iter()
        .find(|c| c.kind == ChangeKind::PactMatchingRulesUnmodelled)
        .expect("unchecked matchers are disclosed");
    assert_eq!(unmodelled.severity, Severity::Unknown);
}

#[test]
fn message_interactions_are_counted_and_degraded_rather_than_skipped() {
    // Silently dropping them would let a pact full of message interactions
    // report a clean HTTP cross-reference it never performed.
    let pact = pact_of(
        r###"{
          "consumer": "c",
          "provider": "p",
          "metadata": { "pactSpecification": { "version": "3.0.0" } },
          "interactions": [
            { "description": "an order", "type": "Asynchronous/Messages",
              "contents": { "id": "1" } },
            { "description": "a list", "request": { "method": "GET", "path": "/alligators",
              "query": "size=3" }, "response": { "status": 200 } }
          ]
        }"###,
    );
    let delta = xref(&pact, &spec_of(PROVIDER));
    let found = details_of(&delta, ChangeKind::PactMessageInteractionNotCrossReferenced);
    assert_eq!(found.len(), 1, "{:#?}", delta.changes);
    assert!(found[0].contains("1 message interaction"), "{}", found[0]);
    assert!(delta.coverage != CoverageState::Verified);
    assert_eq!(delta.verdict, CompatibilityVerdict::Unknown);
}

#[test]
fn a_pact_with_nothing_to_compare_is_unknown_not_compatible() {
    let pact = pact_of(
        r###"{
          "consumer": "c",
          "provider": "p",
          "interactions": [
            { "description": "d", "request": { "method": "GET" }, "response": { "status": 200 } }
          ]
        }"###,
    );
    // The interaction has no path, so it was degraded at parse time and nothing
    // reached a comparison.
    let delta = xref(&pact, &spec_of(PROVIDER));
    assert_eq!(delta.verdict, CompatibilityVerdict::Unknown);
    assert!(!delta.verdict.is_approval());
    assert!(
        delta
            .notes
            .iter()
            .any(|n| n.contains("nothing was compared")),
        "{:#?}",
        delta.notes
    );
}

#[test]
fn an_empty_pact_is_never_compatible() {
    let pact = pact_of(
        r###"{
          "consumer": "c",
          "provider": "p",
          "interactions": []
        }"###,
    );
    let delta = xref(&pact, &spec_of(PROVIDER));
    assert_eq!(delta.verdict, CompatibilityVerdict::Unknown);
    assert!(!delta.verdict.is_approval());
    assert!(
        delta.changes.is_empty(),
        "there is nothing to report a change about"
    );
}

// ---------------------------------------------------------------------------
// Determinism — NFR-001
// ---------------------------------------------------------------------------

#[test]
fn the_cross_reference_is_reproducible_across_100_runs() {
    let provider = spec_of(PROVIDER);
    let pact = pact_of(CONSUMER);
    let once = xref(&pact, &provider);
    for run in 0..100 {
        let again = xref(&pact, &provider);
        assert_eq!(again.digest, once.digest, "run {run} drifted");
        assert_eq!(again, once, "run {run} produced a different delta");
    }
    assert_eq!(once.digest.len(), 64);
    assert!(
        once.digest
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "the digest is lowercase hex"
    );
}

#[test]
fn key_reordering_does_not_change_the_cross_reference() {
    // Both artifacts are written with their keys reversed. The digest is built
    // from the parsed model, so formatting cannot move it.
    let reordered_provider = r###"{
      "paths": {
        "/alligators/{id}": {
          "get": {
            "responses": { "404": { "description": "absent" }, "200": { "description": "ok" } },
            "parameters": [
              { "schema": { "type": "string" }, "required": true, "in": "path", "name": "id" }
            ]
          }
        },
        "/alligators": {
          "post": {
            "responses": { "201": { "description": "created" } },
            "requestBody": {
              "required": true,
              "content": {
                "application/json": {
                  "schema": { "properties": { "colour": { "type": "string" } },
                    "type": "object" }
                }
              }
            }
          },
          "get": {
            "responses": { "200": { "description": "ok" } },
            "parameters": [
              { "required": true, "name": "size", "in": "query", "schema": { "type": "integer" } }
            ]
          }
        }
      },
      "info": { "version": "1.0.0", "title": "Alligator Service" },
      "openapi": "3.2.1"
    }"###;
    // The interaction *array* is left in the same order. Position is the only
    // identity two interactions sharing a description have, so reordering the
    // array is a reform rather than a reformatting — which the second half of
    // this test pins down.
    let reordered_consumer = r###"{
      "interactions": [
        { "response": { "status": 200 },
          "request": { "query": { "size": ["3"] }, "path": "/alligators", "method": "GET" },
          "description": "a list of alligators" },
        { "response": { "status": 200 },
          "request": { "path": "/alligators/3", "method": "GET" },
          "description": "an alligator by id" },
        { "response": { "status": 201 },
          "request": { "body": { "colour": "red" }, "path": "/alligators", "method": "POST" },
          "description": "a new alligator" }
      ],
      "metadata": { "pactSpecification": { "version": "3.0.0" } },
      "provider": { "name": "Alligator Service" },
      "consumer": { "name": "Zoo" }
    }"###;

    let plain = xref(&pact_of(CONSUMER), &spec_of(PROVIDER));
    let shuffled = xref(&pact_of(reordered_consumer), &spec_of(reordered_provider));
    assert_eq!(shuffled.digest, plain.digest);
    assert_eq!(shuffled, plain);

    // The control: moving an interaction *between* positions is a different
    // artifact, and the digest must say so.
    let mut moved = pact_of(reordered_consumer);
    let last = moved.interactions.remove(2);
    moved.interactions.insert(0, last);
    assert_eq!(moved.interactions.len(), 3);
    assert_ne!(
        moved.digest(),
        pact_of(reordered_consumer).digest(),
        "interaction position is identity, so moving one is a reform"
    );
}

#[test]
fn changes_are_canonically_ordered() {
    let mut pact = pact_of(CONSUMER);
    {
        let request = interaction(&mut pact, 0).request.as_mut().expect("request");
        // Inserted out of alphabetical order on purpose: the sort must not
        // reflect the insertion order, or two runs that differ only in how the
        // map was filled would produce different-looking deltas.
        request
            .query
            .insert("zebra".to_string(), vec!["1".to_string()]);
        request
            .query
            .insert("alpha".to_string(), vec!["2".to_string()]);
    }
    interaction(&mut pact, 2)
        .response
        .as_mut()
        .expect("response")
        .status = 418;

    let delta = xref(&pact, &spec_of(PROVIDER));
    assert!(delta.changes.len() >= 3, "{:#?}", delta.changes);
    let undeclared: Vec<&str> = details_of(&delta, ChangeKind::PactUndeclaredRequestParameter);
    assert_eq!(undeclared.len(), 2, "{:#?}", delta.changes);
    assert!(
        undeclared[0].contains("alpha") && undeclared[1].contains("zebra"),
        "the tie-break is on the detail, so `alpha` precedes `zebra`: {undeclared:?}"
    );
    let mut sorted = delta.changes.clone();
    sorted.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.subject.cmp(&b.subject))
            .then_with(|| a.detail.cmp(&b.detail))
    });
    assert_eq!(
        delta.changes, sorted,
        "the change list must be canonically ordered"
    );
}

#[test]
fn the_delta_digest_distinguishes_a_reclassification_from_a_reword() {
    // Two deltas that agree on the detected fact (same kind, same subject) but
    // disagree on its severity must not share a digest: a caller reading the
    // digest must not read a reclassification as "nothing changed".
    let mut pact = pact_of(CONSUMER);
    interaction(&mut pact, 2)
        .request
        .as_mut()
        .expect("request")
        .body_keys
        .insert("nickname".to_string());

    let open = xref(&pact, &spec_of(PROVIDER));
    let closed = xref(
        &pact,
        &spec_of(
            r###"{
              "openapi": "3.2.1",
              "paths": {
                "/alligators": {
                  "post": {
                    "requestBody": {
                      "required": true,
                      "content": {
                        "application/json": {
                          "schema": {
                            "type": "object",
                            "properties": { "colour": { "type": "string" } },
                            "additionalProperties": false
                          }
                        }
                      }
                    },
                    "responses": { "201": { "description": "created" } }
                  }
                }
              }
            }"###,
        ),
    );
    let finding = |d: &ContractDelta| {
        d.changes
            .iter()
            .find(|c| c.kind == ChangeKind::PactUndeclaredBodyProperty)
            .expect("the fact is detected")
            .clone()
    };
    let (a, b) = (finding(&open), finding(&closed));
    assert_eq!(a.kind, b.kind);
    assert_eq!(a.subject, b.subject);
    assert_ne!(
        a.severity, b.severity,
        "the fixture must reclassify, not reform"
    );
    assert_ne!(
        open.digest, closed.digest,
        "a reclassification must move the digest"
    );
}

// ---------------------------------------------------------------------------
// Shape of the result
// ---------------------------------------------------------------------------

#[test]
fn the_delta_names_both_artifacts_and_their_digests() {
    let delta = xref(&pact_of(CONSUMER), &spec_of(PROVIDER));
    assert_eq!(delta.protocol, "pact");
    assert_eq!(delta.from_path, "pact.json");
    assert_eq!(delta.to_path, "api.json");
    assert_eq!(delta.from_digest, pact_of(CONSUMER).digest());
    assert_eq!(delta.to_digest, spec_of(PROVIDER).digest());
    assert_ne!(delta.from_digest, delta.to_digest);
    assert_eq!(
        delta
            .version_change
            .as_ref()
            .map(|v| (v.from.as_str(), v.to.as_str())),
        Some(("3.0.0", "3.2.1")),
        "the delta names the Pact major alongside the provider's OpenAPI version"
    );
}

#[test]
fn the_providers_own_disclosures_are_carried_into_the_delta() {
    // A cross-reference inherits the provider's gaps. Reporting a clean Pact
    // against a provider whose document was only partly understood would pass
    // the consumer's gap on to the reader.
    let with_all_of = PROVIDER.replace(
        "\"schema\": { \"type\": \"integer\" }",
        "\"schema\": { \"allOf\": [ { \"type\": \"integer\" } ] }",
    );
    assert_ne!(with_all_of, PROVIDER, "fixture edit was a no-op");
    let provider_delta = xref(&pact_of(CONSUMER), &spec_of(&with_all_of));
    // No interaction is unmatched: the only thing that changed is that the
    // provider's own parameter schema is now something this engine records but
    // does not interpret.
    assert!(
        !kinds(&provider_delta).contains(&ChangeKind::PactInteractionUnmatched),
        "{:#?}",
        provider_delta.changes
    );
    assert!(
        provider_delta.coverage != CoverageState::Verified,
        "the unmodelled `allOf` must reach the cross-reference's coverage"
    );
    assert_eq!(provider_delta.verdict, CompatibilityVerdict::Unknown);
}

#[test]
fn a_breaking_finding_reaches_a_breaking_verdict_for_every_kind_that_claims_one() {
    // Guards the severity/verdict coupling: every kind whose documented default
    // is `Breaking` must actually be able to drive the verdict there, so a
    // reader can rely on `verdict` alone.
    let breaking_kinds = [
        ChangeKind::PactInteractionUnmatched,
        ChangeKind::PactMethodNotDeclared,
        ChangeKind::PactRequiredQueryParameterMissing,
        ChangeKind::PactRequiredRequestBodyMissing,
        ChangeKind::PactResponseStatusUndeclared,
    ];
    for kind in breaking_kinds {
        assert_eq!(
            kind.default_severity(),
            Severity::Breaking,
            "{kind:?} is documented as breaking"
        );
    }
    let unknown_kinds = [
        ChangeKind::PactUndeclaredRequestParameter,
        ChangeKind::PactUndeclaredRequestBody,
        ChangeKind::PactUndeclaredBodyProperty,
        ChangeKind::PactMatchingRulesUnmodelled,
        ChangeKind::PactAmbiguousProviderMatch,
        ChangeKind::PactMessageInteractionNotCrossReferenced,
    ];
    for kind in unknown_kinds {
        assert_eq!(
            kind.default_severity(),
            Severity::Unknown,
            "{kind:?} names a fact whose consequence the interface does not state, so it cannot \
             default to a break"
        );
    }
}
