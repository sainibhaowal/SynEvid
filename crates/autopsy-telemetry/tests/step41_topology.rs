//! Step 41 gate tests — AST-to-span correlation and the topology diff.
//!
//! Gate: *"Maps `code.function` and `http.route` spans to static `SymbolId`; identifies
//! dead code and undocumented endpoints."*
//!
//! Every fixture goes through `parse_otlp_traces` and `sanitize_traces`, so the tests
//! assert the **composed** pipeline. Hand-built `TraceRecord`s would have tested the
//! correlation against a shape Step 40 might not actually produce.
//!
//! The three claims that could be false and dangerous are the ones with guards on both
//! sides: dead code, undocumented endpoints, and the `Verified` coverage state. Each has
//! a **control** asserting the negative direction, so none of them can pass because the
//! engine said nothing at all.

use autopsy_domain::{CoverageState, SourceLocation, Symbol, SymbolId, Visibility};
use autopsy_telemetry::{
    CorrelationBasis, DeclaredRoute, DeclaredSurface, DeclaredSymbol, DispatchModel,
    EvidenceSubject, IngestOptions, InsufficientReason, TopologyOptions, TopologyReport,
    TraceRecord, canonical_route_template, correlate_topology, parse_otlp_traces, sanitize_traces,
};
use std::num::NonZeroU32;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Two spans: an HTTP server span on `/orders/{id}` naming `getOrder` in
/// `src/orders.ts`, and a child span naming `chargeCard` in `src/pay.ts`.
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
                { "key": "code.function", "value": { "stringValue": "getOrder" } },
                { "key": "code.filepath", "value": { "stringValue": "src/orders.ts" } }
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
                { "key": "code.filepath", "value": { "stringValue": "src/pay.ts" } }
              ],
              "status": { "code": 1 }
            }
          ]
        }
      ]
    }
  ]
}"###;

fn ingest(json: &str) -> TraceRecord {
    let request = parse_otlp_traces("traces.json", json).expect("fixture parses as OTLP/JSON");
    sanitize_traces(&request, &IngestOptions::test_salt()).expect("fixture sanitizes")
}

/// The fixture is deliberately a **clean** export, so a test that expects a degraded
/// state has to show that something else caused it.
fn assert_fixture_is_clean(record: &TraceRecord) {
    assert_eq!(
        record.coverage,
        CoverageState::Verified,
        "the fixture must start clean, or a later coverage assertion proves nothing"
    );
}

fn surface() -> DeclaredSurface {
    DeclaredSurface {
        routes: vec![
            DeclaredRoute::new(Some("GET"), "/orders/{orderId}"),
            DeclaredRoute::new(Some("POST"), "/orders"),
        ],
        symbols: vec![
            DeclaredSymbol::new("orders::getOrder", "src/orders.ts", "getOrder"),
            DeclaredSymbol::new("pay::chargeCard", "src/pay.ts", "chargeCard"),
            // Declared, instrumented, and never observed.
            DeclaredSymbol::new("pay::refundCard", "src/pay.ts", "refundCard"),
        ],
    }
}

/// The caller has looked at everything: enough windows, an unsampled export, and a
/// language where a call names its callee.
fn strict(windows: u32) -> TopologyOptions {
    TopologyOptions::new(NonZeroU32::new(windows).expect("a non-zero window budget"))
        .with_complete_observation(true)
        .with_dispatch_model(DispatchModel::Static)
}

/// The same windows, with every conservative default left in place.
fn permissive_defaults(windows: u32) -> TopologyOptions {
    TopologyOptions::new(NonZeroU32::new(windows).expect("a non-zero window budget"))
}

fn replace_once(source: &str, from: &str, to: &str) -> String {
    let edited = source.replace(from, to);
    assert_ne!(edited, source, "fixture edit was a no-op: {from:?}");
    edited
}

fn route_shapes(report: &TopologyReport) -> Vec<String> {
    report
        .undocumented_routes
        .iter()
        .map(|route| route.template.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// The gate: spans map to static symbols and routes
// ---------------------------------------------------------------------------

#[test]
fn gate_a_span_naming_a_function_maps_to_the_declared_symbol() {
    let record = ingest(TRACE);
    assert_fixture_is_clean(&record);
    let report = correlate_topology(&[record], &surface(), &strict(1));

    let charge = report
        .correlations
        .iter()
        .find(|correlation| {
            correlation
                .symbol
                .as_ref()
                .is_some_and(|id| id.as_str() == "pay::chargeCard")
        })
        .expect("the `chargeCard` span maps to the declared symbol");
    assert_eq!(charge.basis, CorrelationBasis::CodeFunction);
    assert_eq!(
        charge.route, None,
        "a non-HTTP span must not acquire a route from anywhere"
    );
    assert_eq!(report.coverage, CoverageState::Verified);
}

#[test]
fn gate_a_span_naming_an_http_route_maps_to_the_declared_route() {
    let record = ingest(TRACE);
    assert_fixture_is_clean(&record);
    let report = correlate_topology(&[record], &surface(), &strict(1));

    let orders = report
        .correlations
        .iter()
        .find(|correlation| correlation.basis == CorrelationBasis::RouteTemplate)
        .expect("the `/orders/{id}` span maps to a declared route");
    assert_eq!(
        orders.route.as_ref().map(|route| route.template.as_str()),
        Some("/orders/{orderId}"),
        "the contract's spelling is reported, not the observation's"
    );
    assert_eq!(
        orders.symbol.as_ref().map(SymbolId::as_str),
        Some("orders::getOrder"),
        "a span that names both is placed by both facts, not just the reported basis"
    );
}

#[test]
fn gate_the_same_route_in_two_placeholder_spellings_is_one_route() {
    let record = ingest(TRACE);
    assert_fixture_is_clean(&record);
    let report = correlate_topology(&[record], &surface(), &strict(1));
    // The observation says `/orders/{id}`, the contract says `/orders/{orderId}`.
    assert!(
        route_shapes(&report).is_empty(),
        "a renamed path parameter must not read as an undocumented endpoint: {:?}",
        route_shapes(&report)
    );
    assert_eq!(report.coverage, CoverageState::Verified);
}

#[test]
fn gate_dead_code_is_a_symbol_with_no_observed_invocation() {
    let report = correlate_topology(&[ingest(TRACE)], &surface(), &strict(1));

    let dead: Vec<&str> = report
        .dead_code
        .iter()
        .map(|finding| finding.symbol.as_str())
        .collect();
    assert_eq!(dead, ["pay::refundCard"]);
    assert_eq!(
        report.dead_code[0].observed_invocations, 0,
        "the claim is carried as a count, not inferred from its absence"
    );
    assert_eq!(
        report.dead_code[0].coverage,
        CoverageState::Verified,
        "with an unsampled export, enough windows and static dispatch, the finding stands"
    );
}

#[test]
fn gate_an_undocumented_endpoint_is_an_observed_route_no_contract_declares() {
    // The observation's route template is not in the contract, so it is undocumented.
    let edited = replace_once(
        TRACE,
        r###""key": "http.route", "value": { "stringValue": "/orders/{id}" }"###,
        r###""key": "http.route", "value": { "stringValue": "/internal/health/{probe}" }"###,
    );
    let report = correlate_topology(&[ingest(&edited)], &surface(), &strict(1));

    assert_eq!(route_shapes(&report), ["/internal/health/{}"]);
    let finding = &report.undocumented_routes[0];
    assert_eq!(finding.method.as_deref(), Some("GET"));
    assert_eq!(
        finding.invocations, 1,
        "the reader needs to know how much traffic hit the undeclared route"
    );
    assert_eq!(
        finding.services.iter().cloned().collect::<Vec<String>>(),
        ["checkout"],
        "and which service serves it"
    );
    assert_eq!(
        finding.coverage,
        CoverageState::Partial,
        "the engine knows the route exists; it does not know whether its contract covers this service"
    );
}

#[test]
fn gate_a_declared_route_is_never_reported_as_undocumented() {
    // The control for the test above: same fixture, unedited, empty finding list.
    let report = correlate_topology(&[ingest(TRACE)], &surface(), &strict(1));
    assert!(
        report.undocumented_routes.is_empty(),
        "a declared route would make every undocumented finding suspect"
    );
}

// ---------------------------------------------------------------------------
// The honest-coverage gates
// ---------------------------------------------------------------------------

#[test]
fn gate_no_observation_windows_means_no_findings_and_unknown_coverage() {
    let report = correlate_topology(&[], &surface(), &strict(1));

    assert_eq!(report.observation.windows_observed, 0);
    assert_eq!(report.coverage, CoverageState::Unknown);
    assert!(
        report.undocumented_routes.is_empty(),
        "a route is not undocumented if nobody looked"
    );
    assert!(
        report.dead_code.is_empty(),
        "a function is not dead if nobody looked"
    );
    assert!(
        report.insufficient_evidence.is_empty(),
        "and it is not weak evidence either — there is no evidence"
    );
    assert_eq!(report.coverage, CoverageState::Unknown);
}

#[test]
fn gate_fewer_windows_than_required_yields_insufficient_evidence_not_dead_code() {
    let report = correlate_topology(&[ingest(TRACE)], &surface(), &strict(2));

    assert!(
        report.dead_code.is_empty(),
        "one window against a budget of two cannot support a dead-code claim"
    );
    assert!(
        report.uninvoked_routes.is_empty(),
        "and it cannot support an uninvoked-route claim either"
    );
    // Both the uncalled symbol and the uncalled route are held back, each named.
    let subjects: Vec<EvidenceSubject> = report
        .insufficient_evidence
        .iter()
        .map(|entry| entry.subject.clone())
        .collect();
    assert_eq!(
        subjects,
        [
            // Sorted by the derived order of `EvidenceSubject`, which is the order its
            // variants are declared in: symbols, then routes.
            EvidenceSubject::Symbol(SymbolId::new("pay::refundCard")),
            EvidenceSubject::Route {
                method: Some("POST".to_string()),
                template: "/orders".to_string(),
            },
        ],
        "an unreturned finding must still name what it is about"
    );
    for entry in &report.insufficient_evidence {
        assert_eq!(entry.reason, InsufficientReason::TooFewWindows);
        assert_eq!(entry.windows_observed, 1);
        assert_eq!(entry.required_windows, 2);
    }
}

#[test]
fn gate_a_maybe_sampled_export_cannot_support_a_dead_code_claim() {
    // The conservative default: the caller has not asserted that the export is
    // complete, so absence of a span is exactly what a sampled export looks like for a
    // function that ran perfectly well.
    let report = correlate_topology(&[ingest(TRACE)], &surface(), &permissive_defaults(1));

    assert!(report.dead_code.is_empty());
    assert_eq!(
        report.insufficient_evidence[0].reason,
        InsufficientReason::ObservationMayBeSampled
    );
    assert!(
        !report.observation.complete_observation,
        "the report states the assumption it made"
    );
}

#[test]
fn gate_dynamic_dispatch_is_named_as_the_reason_the_finding_is_only_partial() {
    let report = correlate_topology(
        &[ingest(TRACE)],
        &surface(),
        &TopologyOptions::new(NonZeroU32::new(1).unwrap())
            .with_complete_observation(true)
            .with_dispatch_model(DispatchModel::Dynamic),
    );

    assert_eq!(report.dead_code.len(), 1, "the finding is still made");
    assert_eq!(
        report.dead_code[0].coverage,
        CoverageState::Partial,
        "a dynamically dispatched call can reach a function without naming it"
    );
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("dynamically dispatched")),
        "the reason has to reach the reader, not just the coverage field: {:?}",
        report.notes
    );
}

#[test]
fn gate_an_empty_declared_route_set_makes_no_route_undocumented() {
    // An empty contract and an absent contract are indistinguishable, so reporting
    // every observed route as a violation of a document nobody read would be a claim
    // the engine cannot support.
    let mut surface = surface();
    surface.routes.clear();
    let report = correlate_topology(&[ingest(TRACE)], &surface, &strict(1));

    assert!(
        report.undocumented_routes.is_empty(),
        "no contract was loaded, so no route can be called undocumented"
    );
    assert_eq!(
        report.coverage,
        CoverageState::Unknown,
        "and the reader has to be told the contract was missing"
    );
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("empty contract")),
        "the reason is stated: {:?}",
        report.notes
    );
}

#[test]
fn an_observation_with_no_declared_routes_still_maps_by_function() {
    // The control for the test above: clearing the routes must not have broken symbol
    // correlation, or the assertion would hold for the wrong reason.
    let mut surface = surface();
    surface.routes.clear();
    let report = correlate_topology(&[ingest(TRACE)], &surface, &strict(1));

    assert!(
        report.correlations.iter().any(|correlation| correlation
            .symbol
            .as_ref()
            .is_some_and(|id| id.as_str() == "pay::chargeCard")),
        "the symbol side is independent of the route side"
    );
}

#[test]
fn gate_a_declared_route_nothing_called_is_reported_as_uninvoked() {
    let report = correlate_topology(&[ingest(TRACE)], &surface(), &strict(1));

    // The fixture calls `GET /orders/{id}` and never calls `POST /orders`.
    let uninvoked: Vec<(&str, Option<&str>)> = report
        .uninvoked_routes
        .iter()
        .map(|route| (route.template.as_str(), route.method.as_deref()))
        .collect();
    assert_eq!(uninvoked, [("/orders", Some("POST"))]);
    assert_eq!(report.uninvoked_routes[0].observed_invocations, 0);
    assert_eq!(
        report.uninvoked_routes[0].coverage,
        CoverageState::Verified,
        "the same evidence discipline as dead code: enough windows, unsampled export"
    );
}

#[test]
fn gate_a_route_that_was_called_is_not_reported_as_uninvoked() {
    // The control: otherwise every route would be reported, and the finding would be
    // worthless.
    let report = correlate_topology(&[ingest(TRACE)], &surface(), &strict(1));
    assert!(
        !report
            .uninvoked_routes
            .iter()
            .any(|route| route.template == "/orders/{}"),
        "the called route must not appear: {:?}",
        report.uninvoked_routes
    );
}

#[test]
fn gate_a_maybe_sampled_export_cannot_support_an_uninvoked_route_claim() {
    let report = correlate_topology(&[ingest(TRACE)], &surface(), &permissive_defaults(1));

    assert!(report.uninvoked_routes.is_empty());
    assert!(
        report
            .insufficient_evidence
            .iter()
            .any(|entry| entry.subject
                == (EvidenceSubject::Route {
                    method: Some("POST".to_string()),
                    template: "/orders".to_string(),
                })
                && entry.reason == InsufficientReason::ObservationMayBeSampled),
        "{:?}",
        report.insufficient_evidence
    );
}

// ---------------------------------------------------------------------------
// Ambiguity is reported, never resolved by a coin flip
// ---------------------------------------------------------------------------

#[test]
fn gate_two_symbols_sharing_a_name_are_ambiguous_not_matched() {
    let mut surface = surface();
    // The instrumentation recorded no `code.filepath` for the charge span, so the two
    // `chargeCard` symbols cannot be told apart.
    let edited = replace_once(
        TRACE,
        r###"{ "key": "code.filepath", "value": { "stringValue": "src/pay.ts" } }"###,
        r###"{ "key": "service.instance.id", "value": { "stringValue": "src/pay.ts" } }"###,
    );
    surface.symbols.push(DeclaredSymbol::new(
        "pay2::chargeCard",
        "src/other.ts",
        "chargeCard",
    ));
    let report = correlate_topology(&[ingest(&edited)], &surface, &strict(1));

    let ambiguous = report
        .correlations
        .iter()
        .find(|correlation| correlation.basis == CorrelationBasis::AmbiguousSymbol)
        .expect("the ambiguous span is classified as ambiguous");
    assert_eq!(
        ambiguous.symbol, None,
        "choosing one would be a coin flip reported as a match"
    );
    assert_eq!(report.coverage, CoverageState::Partial);
    // And the ambiguity must not have marked either candidate alive.
    let dead: Vec<&str> = report
        .dead_code
        .iter()
        .map(|finding| finding.symbol.as_str())
        .collect();
    assert_eq!(
        dead,
        ["pay2::chargeCard", "pay::chargeCard", "pay::refundCard"],
        "an unresolved span cannot count as evidence that a symbol ran"
    );
}

#[test]
fn a_symbol_with_no_recorded_file_is_matched_only_when_the_name_is_unique() {
    let mut surface = surface();
    surface.symbols.push(DeclaredSymbol::new(
        "orders2::getOrder",
        "src/other.ts",
        "getOrder",
    ));
    // Two files declare `getOrder`, but the span records which file it was in, so the
    // pair is still unique.
    let report = correlate_topology(&[ingest(TRACE)], &surface, &strict(1));
    assert!(
        !report
            .correlations
            .iter()
            .any(|correlation| correlation.basis == CorrelationBasis::AmbiguousSymbol),
        "a recorded file separates two same-named symbols"
    );
    assert!(report.correlations.iter().any(|correlation| {
        correlation
            .symbol
            .as_ref()
            .is_some_and(|id| id.as_str() == "orders::getOrder")
    }));
}

#[test]
fn a_concrete_path_matching_two_declared_shapes_is_reported_rather_than_chosen() {
    // No `http.route`, so the observed path is matched against declared templates — and
    // two equally specific templates fit.
    let edited = replace_once(
        TRACE,
        r###"{ "key": "http.route", "value": { "stringValue": "/orders/{id}" } }"###,
        r###"{ "key": "url.path", "value": { "stringValue": "/orders/7" } }"###,
    );
    let mut surface = surface();
    surface.routes = vec![
        DeclaredRoute::new(Some("GET"), "/orders/{id}"),
        DeclaredRoute::new(Some("GET"), "/{collection}/7"),
    ];
    let report = correlate_topology(&[ingest(&edited)], &surface, &strict(1));

    assert!(
        report
            .correlations
            .iter()
            .any(|correlation| correlation.basis == CorrelationBasis::ObservedPathSubsumption),
        "the path was used, and the basis says so"
    );
    assert_eq!(
        report.coverage,
        CoverageState::Partial,
        "an inferred placement is not a declared fact"
    );
}

#[test]
fn a_method_the_contract_does_not_serve_is_not_a_documented_route() {
    // `DELETE /orders/{id}` is observed; the contract declares GET on that shape.
    let edited = replace_once(
        TRACE,
        r###""key": "http.request.method", "value": { "stringValue": "GET" }"###,
        r###""key": "http.request.method", "value": { "stringValue": "DELETE" }"###,
    );
    let report = correlate_topology(&[ingest(&edited)], &surface(), &strict(1));

    assert_eq!(
        route_shapes(&report),
        ["/orders/{}"],
        "a route the contract serves by another method is an undeclared endpoint, not a match"
    );
    assert_eq!(
        report.undocumented_routes[0].method.as_deref(),
        Some("DELETE")
    );
}

#[test]
fn a_route_declared_without_a_method_matches_any_observation_but_says_so() {
    let mut surface = surface();
    surface.routes = vec![DeclaredRoute::new(None, "/orders/{id}")];
    let report = correlate_topology(&[ingest(TRACE)], &surface, &strict(1));

    assert!(
        report.undocumented_routes.is_empty(),
        "a method-agnostic declaration does cover the observation"
    );
    assert_eq!(
        report.coverage,
        CoverageState::Partial,
        "but the observed method was not checked, and the report must not imply it was"
    );
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("declared without a method")),
        "the reason is stated: {:?}",
        report.notes
    );
}

#[test]
fn one_route_shape_declared_twice_is_noted_because_the_operation_is_ambiguous() {
    let mut surface = surface();
    surface.routes = vec![
        DeclaredRoute::new(Some("GET"), "/orders/{orderId}"),
        DeclaredRoute::new(Some("GET"), "/orders/{reference}"),
    ];
    let report = correlate_topology(&[ingest(TRACE)], &surface, &strict(1));

    assert_eq!(report.coverage, CoverageState::Partial);
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("declared more than once")),
        "{:?}",
        report.notes
    );
}

#[test]
fn a_span_that_carries_nothing_correlatable_costs_coverage_and_is_counted() {
    let edited = replace_once(
        TRACE,
        r###"{ "key": "code.function", "value": { "stringValue": "chargeCard" } }"###,
        r###"{ "key": "app.internal.step", "value": { "stringValue": "settle" } }"###,
    );
    let report = correlate_topology(&[ingest(&edited)], &surface(), &strict(1));

    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("could not be placed")),
        "an unplaceable span is the false-negative source for the dead-code pass: {:?}",
        report.notes
    );
    assert_eq!(report.coverage, CoverageState::Partial);
}

#[test]
fn an_rpc_span_is_disclosed_as_uncorrelated_rather_than_silently_ignored() {
    let edited = replace_once(
        TRACE,
        r###"{ "key": "code.function", "value": { "stringValue": "chargeCard" } }"###,
        r###"{ "key": "rpc.method", "value": { "stringValue": "Charge" } }"###,
    );
    let edited = replace_once(
        &edited,
        r###"{ "key": "code.filepath", "value": { "stringValue": "src/pay.ts" } }"###,
        r###"{ "key": "rpc.service", "value": { "stringValue": "billing.Billing" } }"###,
    );
    let report = correlate_topology(&[ingest(&edited)], &surface(), &strict(1));

    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("no static surface is modelled for RPC handlers")),
        "the gap is stated, not left as an unexplained absence: {:?}",
        report.notes
    );
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn the_report_is_reproducible_across_100_runs() {
    let first = correlate_topology(&[ingest(TRACE)], &surface(), &strict(1));
    for run in 1..100 {
        let again = correlate_topology(&[ingest(TRACE)], &surface(), &strict(1));
        assert_eq!(again.digest, first.digest, "run {run} differed");
        assert_eq!(again, first, "run {run} differed structurally");
    }
}

#[test]
fn the_order_the_windows_are_handed_over_does_not_change_the_report() {
    let other = {
        let edited = replace_once(
            TRACE,
            r###""spanId": "EEE19B7EC3C1B175""###,
            r###""spanId": "EEE19B7EC3C1B176""###,
        );
        ingest(&edited)
    };
    let forwards = correlate_topology(&[ingest(TRACE), other.clone()], &surface(), &strict(2));
    let backwards = correlate_topology(&[other, ingest(TRACE)], &surface(), &strict(2));

    assert_eq!(forwards.digest, backwards.digest);
    assert_eq!(forwards.observation.windows_observed, 2);
    assert_eq!(
        forwards.observation.span_count,
        backwards.observation.span_count
    );
}

#[test]
fn the_digest_moves_when_the_static_surface_moves() {
    // The control for the test above: equality that holds for everything is not a
    // property, and a digest that ignores its input would pass every determinism test.
    let baseline = correlate_topology(&[ingest(TRACE)], &surface(), &strict(1));

    let mut changed = surface();
    changed
        .routes
        .push(DeclaredRoute::new(Some("GET"), "/internal/health"));
    let route_moved = correlate_topology(&[ingest(TRACE)], &changed, &strict(1));
    assert_ne!(
        baseline.digest, route_moved.digest,
        "the route set reaches the digest"
    );

    let mut changed = surface();
    changed.symbols.push(DeclaredSymbol::new(
        "pay::voidCard",
        "src/pay.ts",
        "voidCard",
    ));
    let symbol_moved = correlate_topology(&[ingest(TRACE)], &changed, &strict(1));
    assert_ne!(
        baseline.digest, symbol_moved.digest,
        "the symbol set reaches the digest"
    );
}

#[test]
fn the_digest_does_not_depend_on_the_notes() {
    // Notes are prose derived from facts that are already in the digest, so rewording
    // one must not move it — otherwise every report would move whenever a note was
    // clarified.
    let report = correlate_topology(&[ingest(TRACE)], &surface(), &strict(1));
    let mut reworded = report.clone();
    reworded.notes = vec!["a completely different sentence".to_string()];
    assert_eq!(
        serde_json::to_value(&reworded).unwrap()["digest"],
        serde_json::to_value(&report).unwrap()["digest"]
    );

    let digest_of_report = |report: &TopologyReport| {
        let mut value = serde_json::to_value(report).unwrap();
        value.as_object_mut().unwrap().remove("digest");
        value.as_object_mut().unwrap().remove("notes");
        serde_json::to_string(&value).unwrap()
    };
    assert_eq!(digest_of_report(&report), digest_of_report(&reworded));
}

#[test]
fn the_report_round_trips_through_json() {
    let report = correlate_topology(&[ingest(TRACE)], &surface(), &strict(1));
    let text = serde_json::to_string(&report).expect("the report serializes");
    let back: TopologyReport = serde_json::from_str(&text).expect("and deserializes");
    assert_eq!(back, report);
}

// ---------------------------------------------------------------------------
// Vocabulary
// ---------------------------------------------------------------------------

#[test]
fn a_placeholder_is_erased_only_as_a_whole_segment() {
    assert_eq!(canonical_route_template("/orders/{id}"), "/orders/{}");
    assert_eq!(canonical_route_template("/orders/{orderId}"), "/orders/{}");
    // A placeholder sharing a segment with literal text stays literal, which
    // under-matches rather than over-matches: an unmatched observation is visible, a
    // wrong match is not.
    assert_eq!(
        canonical_route_template("/files/{name}.json"),
        "/files/{name}.json"
    );
}

#[test]
fn a_subsumed_path_is_marked_as_an_inference() {
    assert!(CorrelationBasis::ObservedPathSubsumption.is_inferred());
    assert!(!CorrelationBasis::RouteTemplate.is_inferred());
    assert!(!CorrelationBasis::CodeFunction.is_inferred());
    assert!(CorrelationBasis::RouteTemplate.is_placed());
    assert!(!CorrelationBasis::Unmatched.is_placed());
    assert!(
        !CorrelationBasis::AmbiguousSymbol.is_placed(),
        "an ambiguous span is not placed, or the dead-code pass would treat it as evidence"
    );
}

#[test]
fn a_symbol_converts_from_the_domain_type_at_the_boundary() {
    // The one-line bridge from the real symbol table, which is why this crate needs no
    // dependency on a language adapter.
    let symbol = Symbol {
        stable_id: SymbolId::new("orders::getOrder"),
        language_id: "typescript".to_string(),
        kind: "function".to_string(),
        qualified_name: "getOrder".to_string(),
        range: Some(SourceLocation {
            path: "./src/orders.ts".to_string(),
            start_line: 1,
            end_line: 2,
            start_col: Some(0),
            end_col: Some(12),
            symbol_id: None,
        }),
        normalized_signature: None,
        visibility: Visibility::Public,
    };
    let declared = DeclaredSymbol::from(&symbol);
    assert_eq!(declared.id.as_str(), "orders::getOrder");
    assert_eq!(declared.name, "getOrder");
    assert_eq!(
        declared.path.as_deref(),
        Some("src/orders.ts"),
        "a leading `./` is stripped so the symbol table and the instrumentation agree"
    );
}
