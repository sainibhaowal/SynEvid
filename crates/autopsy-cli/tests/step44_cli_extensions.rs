//! Step 44 gate tests: the CLI surface for every Phase 4.2 engine.
//!
//! The handlers return what they *would* print, so an exit code and the exact output can be
//! asserted without a terminal — which is the only way an exit-code contract stays true as
//! the engines evolve.
//!
//! Two fixtures are worth naming up front:
//!
//! * **This repository.** The history commands need a git repository with real history, and
//!   the repository under test is the one repository guaranteed to exist here. Values are
//!   asserted as *invariants* (a digest that is stable across runs, a population that is
//!   accounted for) rather than as constants, because the history grows and a constant would
//!   be a test that must be edited on every commit.
//! * **Nothing.** An empty tree, a missing salt, a document that is not JSON — the error
//!   paths are where an exit-code contract is usually wrong.

use autopsy_cli::extensions;
use autopsy_domain::CoverageState;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Exit codes, restated rather than imported: a test that reads the constant it is checking
/// cannot catch a constant that changed.
const PASS: i32 = 0;
const POLICY_FAIL: i32 = 2;
const ANALYSIS_ERROR: i32 = 3;
const UNSUPPORTED: i32 = 4;

static NEXT_SCRATCH: AtomicUsize = AtomicUsize::new(0);

fn scratch(name: &str) -> PathBuf {
    let id = NEXT_SCRATCH.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "autopsy-cli-step44-{}-{id}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

fn write(root: &Path, relative: &str, content: &str) -> PathBuf {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent directory");
    }
    fs::write(&path, content).expect("fixture file");
    path
}

/// The cli fixture type: a `Cli` with only the global options set, so handlers can be called
/// directly. `repo_root` is what the history commands read.
fn cli_for(repo_root: &Path) -> Result<autopsy_cli::Cli, String> {
    use clap::Parser;
    autopsy_cli::Cli::try_parse_from([
        "autopsy",
        "--repo-root",
        &repo_root.display().to_string(),
        "version",
    ])
    .map_err(|error| error.to_string())
}

/// The repository under test, when there is one.
///
/// A source archive has no `.git`, and a test that insists on history would fail for a reason
/// that has nothing to do with the code. The skip is explicit and says so.
fn repository_under_test() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    if root.join(".git").exists() {
        Some(root)
    } else {
        eprintln!(
            "skipping: {} has no `.git`, so there is no history to read",
            root.display()
        );
        None
    }
}

// ---------------------------------------------------------------------------
// History: temporal and cochange
// ---------------------------------------------------------------------------

#[test]
fn gate_the_history_engines_work_against_a_real_repository() {
    let Some(root) = repository_under_test() else {
        return;
    };
    let cli = cli_for(&root).expect("the fixture arguments parse");

    let journal = extensions::temporal(&cli, None, 10_000, false).expect("the journal builds");
    assert_eq!(journal.exit_code, PASS, "{}", journal.text);
    assert!(journal.text.contains("TreeExtractor"), "{}", journal.text);
    assert!(
        journal
            .text
            .contains("Symbol-level extraction is not wired"),
        "the extractor's granularity is disclosed, not implied: {}",
        journal.text
    );
    let value: serde_json::Value = serde_json::from_str(&journal.json).expect("valid JSON");
    assert!(
        value["stats"]["total"].as_u64().unwrap_or(0) > 0,
        "the repository under test has history to replay: {}",
        journal.text
    );
    assert!(
        !value["journal"].is_null(),
        "the engine's own canonical projection is embedded verbatim: {}",
        &journal.json[..journal.json.len().min(400)]
    );
    assert!(
        value["notes"]
            .as_array()
            .is_some_and(|notes| notes.iter().any(|note| note
                .as_str()
                .is_some_and(|text| text.contains("not which symbols")))),
        "the CLI states the extractor's granularity in the machine-readable output too"
    );

    let mined = extensions::cochange(&root, 183 * 86_400, 500, 30, 2, false).expect("mines");
    assert_ne!(mined.exit_code, ANALYSIS_ERROR, "{}", mined.text);
    let report: serde_json::Value = serde_json::from_str(&mined.json).expect("valid JSON");
    assert!(
        report["window"]["commits_considered"].as_u64().unwrap_or(0) > 0,
        "the repository under test has history: {}",
        mined.text
    );
    assert!(
        mined.text.contains("not a dependency graph"),
        "the non-causality boundary reaches the human output: {}",
        mined.text
    );
}

#[test]
fn gate_a_history_report_is_byte_identical_across_one_hundred_runs() {
    // The engines assert their own determinism; this asserts that the *CLI* does not add a
    // source of variation — an iteration order, a rendering, a clock.
    let Some(root) = repository_under_test() else {
        return;
    };
    let first = extensions::cochange(&root, 183 * 86_400, 200, 30, 2, false).expect("mines");
    for run in 0..100 {
        let again = extensions::cochange(&root, 183 * 86_400, 200, 30, 2, false).expect("mines");
        assert_eq!(again.json, first.json, "run {run} differs");
        assert_eq!(again.exit_code, first.exit_code, "run {run} differs");
    }
}

#[test]
fn gate_every_mined_edge_is_tagged_as_temporal_co_occurrence() {
    let Some(root) = repository_under_test() else {
        return;
    };
    let mined = extensions::cochange(&root, 183 * 86_400, 200, 30, 1, false).expect("mines");
    let report: serde_json::Value = serde_json::from_str(&mined.json).expect("valid JSON");
    let edges = report["edges"].as_array().expect("an array");
    for edge in edges {
        assert_eq!(edge["claim"], "temporal_co_occurrence_only");
        assert_eq!(edge["kind"], "statistical_co_change");
        assert_ne!(edge["kind"], "calls");
    }
    if edges.is_empty() {
        eprintln!("note: this repository's window has no edge above the floor at this bound");
    }
}

#[test]
fn gate_a_repository_with_no_commits_is_unsupported_rather_than_an_error() {
    // `3` would say the analysis failed. Nothing failed: there is no history, which is a
    // different statement and a different exit code.
    let root = scratch("no-head");
    fs::create_dir_all(root.join(".git/objects")).expect("objects");
    fs::create_dir_all(root.join(".git/refs")).expect("refs");
    let cli = cli_for(&root).expect("parses");

    let journal = extensions::temporal(&cli, None, 50, false).expect("a value, not a failure");
    assert_eq!(journal.exit_code, UNSUPPORTED);
    assert!(journal.text.contains("no commits"), "{}", journal.text);

    let mined = extensions::cochange(&root, 86_400, 50, 30, 2, false).expect("a value");
    assert_eq!(mined.exit_code, UNSUPPORTED);
    assert!(mined.text.contains("no commits"), "{}", mined.text);
}

#[test]
fn gate_the_journal_coverage_comes_from_the_facts_that_did_not_resolve() {
    // Not a blanket `verified`: a journal carrying unresolved facts must fail `--strict`,
    // otherwise the flag would pass over precisely what the engine flagged.
    let Some(root) = repository_under_test() else {
        return;
    };
    let cli = cli_for(&root).expect("parses");
    let lenient = extensions::temporal(&cli, None, 10_000, false).expect("builds");
    let strict = extensions::temporal(&cli, None, 10_000, true).expect("builds");
    let report: serde_json::Value = serde_json::from_str(&lenient.json).expect("valid JSON");
    let unresolved = report["stats"]["partial"].as_u64().unwrap_or(0)
        + report["stats"]["unknown"].as_u64().unwrap_or(0);
    let expected_coverage = if report["stats"]["unknown"].as_u64().unwrap_or(0) > 0 {
        "unknown"
    } else if unresolved > 0 {
        "partial"
    } else {
        "verified"
    };
    assert_eq!(report["coverage"], expected_coverage, "{}", lenient.text);
    let expected_exit = if unresolved > 0 { UNSUPPORTED } else { PASS };
    assert_eq!(strict.exit_code, expected_exit, "{}", strict.text);
    assert_eq!(
        lenient.json, strict.json,
        "strictness changes the exit code, not the report"
    );
}

#[test]
fn gate_a_history_that_does_not_fit_the_bound_is_unsupported_and_says_what_to_pass() {
    // The walker refuses to truncate a timeline silently — a truncated timeline is not a
    // timeline — so a bound smaller than the history is a *refusal*, and a refusal has to
    // tell the caller how to get an answer rather than failing with a bare message.
    let Some(root) = repository_under_test() else {
        return;
    };
    let cli = cli_for(&root).expect("parses");
    let outcome = extensions::temporal(&cli, None, 1, false).expect("a value, not a failure");
    assert_eq!(outcome.exit_code, UNSUPPORTED, "{}", outcome.text);
    assert!(
        outcome.text.contains("--max-commits"),
        "the refusal must say what to pass instead: {}",
        outcome.text
    );
}

#[test]
fn gate_a_directory_that_is_not_a_repository_is_an_analysis_error() {
    let root = scratch("not-a-repo");
    let cli = cli_for(&root).expect("parses");
    let error = extensions::temporal(&cli, None, 50, false).expect_err("not a repository");
    assert!(
        error.to_string().contains("is not a git repository"),
        "{error}"
    );
}

// ---------------------------------------------------------------------------
// contract-dist
// ---------------------------------------------------------------------------

const OPENAPI_TWO_PATHS: &str = r#"
openapi: 3.2.1
info:
  title: checkout
  version: 1.0.0
paths:
  /pay:
    get:
      responses:
        '200':
          description: ok
  /refund:
    get:
      responses:
        '200':
          description: ok
"#;

const OPENAPI_ONE_PATH: &str = r#"
openapi: 3.2.1
info:
  title: checkout
  version: 1.1.0
paths:
  /pay:
    get:
      responses:
        '200':
          description: ok
"#;

const OPENAPI_ADDED_PATH: &str = r#"
openapi: 3.2.1
info:
  title: checkout
  version: 1.0.1
paths:
  /pay:
    get:
      responses:
        '200':
          description: ok
  /refund:
    get:
      responses:
        '200':
          description: ok
  /ledger:
    get:
      responses:
        '200':
          description: ok
"#;

#[test]
fn gate_a_breaking_contract_change_exits_two_without_being_asked() {
    // A positive breaking finding is a policy failure at any strictness: exiting `0` would
    // bury it in a pipeline that only checks the status.
    let root = scratch("contract-breaking");
    let from = write(&root, "from.yaml", OPENAPI_TWO_PATHS);
    let to = write(&root, "to.yaml", OPENAPI_ONE_PATH);
    let outcome = extensions::contract_dist(
        &from,
        &to,
        extensions::ContractProtocol::Openapi,
        None,
        false,
    )
    .expect("diffs");
    assert_eq!(outcome.exit_code, POLICY_FAIL, "{}", outcome.text);
    assert!(outcome.text.contains("Breaking"), "{}", outcome.text);
    assert!(
        outcome.text.contains("/refund"),
        "the breaking subject is named: {}",
        outcome.text
    );
}

#[test]
fn gate_a_compatible_change_exits_zero_and_says_only_that_it_found_nothing_breaking() {
    let root = scratch("contract-compatible");
    let from = write(&root, "from.yaml", OPENAPI_TWO_PATHS);
    let to = write(&root, "to.yaml", OPENAPI_ADDED_PATH);
    let outcome = extensions::contract_dist(
        &from,
        &to,
        extensions::ContractProtocol::Openapi,
        None,
        false,
    )
    .expect("diffs");
    assert_eq!(outcome.exit_code, PASS, "{}", outcome.text);
    assert!(outcome.text.contains("Compatible"), "{}", outcome.text);
    // The digest is in the output, so two runs of the same pair are comparable by eye.
    let report: serde_json::Value = serde_json::from_str(&outcome.json).expect("valid JSON");
    assert_eq!(report["protocol"], "openapi");
    assert_eq!(
        report["verdict"], "compatible",
        "with a fully modelled version and no unread construct, adding an endpoint is compatible: {}",
        outcome.text
    );
    assert_eq!(
        report["coverage"], "verified",
        "a `compatible` verdict requires verified coverage — it is the only verdict that \
         claims nothing was missed: {}",
        outcome.text
    );
}

/// The same two paths, but one response grows a body. A body change is detected and the
/// engine has no directional evidence for it, so the verdict is `unknown` rather than a
/// guess in either direction.
const OPENAPI_BODY_CHANGED: &str = r#"
openapi: 3.2.1
info:
  title: checkout
  version: 1.0.1
paths:
  /pay:
    get:
      responses:
        '200':
          description: ok
          content:
            application/json:
              schema:
                type: object
                properties:
                  id:
                    type: string
  /refund:
    get:
      responses:
        '200':
          description: ok
"#;

#[test]
fn gate_an_unknown_verdict_is_an_answer_without_strict_and_a_refusal_with_it() {
    // The rule: `unknown` is not actionable for a pipeline, but a report that says so is a
    // legitimate answer for a human. If both were `0`, or both `4`, `--strict` would be a
    // flag with no meaning.
    let root = scratch("contract-unknown");
    let from = write(&root, "from.yaml", OPENAPI_TWO_PATHS);
    let to = write(&root, "to.yaml", OPENAPI_BODY_CHANGED);
    let lenient = extensions::contract_dist(
        &from,
        &to,
        extensions::ContractProtocol::Openapi,
        None,
        false,
    )
    .expect("diffs");
    let strict = extensions::contract_dist(
        &from,
        &to,
        extensions::ContractProtocol::Openapi,
        None,
        true,
    )
    .expect("diffs");

    let report: serde_json::Value = serde_json::from_str(&lenient.json).expect("valid JSON");
    assert_eq!(
        report["verdict"], "unknown",
        "the fixture must reach the unknown branch: {}",
        lenient.text
    );
    assert_eq!(lenient.exit_code, PASS, "{}", lenient.text);
    assert_eq!(strict.exit_code, UNSUPPORTED, "{}", strict.text);
    assert_eq!(
        lenient.json, strict.json,
        "strictness changes the code, not the report"
    );
}

#[test]
fn gate_a_pact_document_that_is_not_a_pact_is_an_error_not_an_empty_diff() {
    // Cross-referencing an OpenAPI document as if it were a Pact must fail loudly: an empty
    // diff would read as "the provider satisfies the contract".
    let root = scratch("contract-pact-mismatch");
    let from = write(&root, "from.yaml", OPENAPI_TWO_PATHS);
    let error = extensions::contract_dist(
        &from,
        &from,
        extensions::ContractProtocol::Pact,
        Some(&from),
        false,
    )
    .expect_err("an OpenAPI document is not a Pact");
    assert!(error.to_string().contains("from.yaml"), "{error}");
}

#[test]
fn gate_a_malformed_contract_document_is_an_analysis_error() {
    let root = scratch("contract-malformed");
    let from = write(&root, "from.yaml", "not: [a, valid,\n");
    let to = write(&root, "to.yaml", OPENAPI_ONE_PATH);
    let error = extensions::contract_dist(
        &from,
        &to,
        extensions::ContractProtocol::Openapi,
        None,
        false,
    )
    .expect_err("malformed YAML is an analysis error");
    assert!(!error.to_string().is_empty());
}

#[test]
fn gate_an_openapi_2_document_is_refused_rather_than_read_as_3_x() {
    // Reading a 2.0 document under 3.x semantics would report every endpoint as removed,
    // which is a false `breaking` verdict — worse than an error.
    let root = scratch("contract-swagger");
    let from = write(
        &root,
        "from.yaml",
        "swagger: '2.0'\ninfo:\n  title: t\n  version: 1\npaths:\n  /a:\n    get:\n      responses:\n        '200':\n          description: ok\n",
    );
    let to = write(&root, "to.yaml", OPENAPI_ONE_PATH);
    let error = extensions::contract_dist(
        &from,
        &to,
        extensions::ContractProtocol::Openapi,
        None,
        false,
    )
    .expect_err("an unsupported version is an error");
    assert!(
        error.to_string().to_lowercase().contains("version"),
        "{error}"
    );
}

#[test]
fn gate_asyncapi_flow_verification_runs_from_one_document() {
    let root = scratch("contract-flows");
    let spec = write(
        &root,
        "events.yaml",
        r#"asyncapi: 3.0.0
info:
  title: events
  version: 1.0.0
channels:
  refunds:
    address: refunds
    messages:
      refundIssued:
        payload:
          type: object
          properties:
            id:
              type: string
operations:
  publishRefund:
    action: send
    channel:
      $ref: '#/channels/refunds'
    messages:
      - $ref: '#/channels/refunds/messages/refundIssued'
  consumeRefund:
    action: receive
    channel:
      $ref: '#/channels/refunds'
    messages:
      - $ref: '#/channels/refunds/messages/refundIssued'
"#,
    );
    let outcome = extensions::contract_dist(
        &spec,
        &spec,
        extensions::ContractProtocol::AsyncapiFlows,
        None,
        false,
    )
    .expect("verifies");
    let report: serde_json::Value = serde_json::from_str(&outcome.json).expect("valid JSON");
    assert!(
        report["checked"]
            .as_array()
            .is_some_and(|checked| !checked.is_empty()),
        "the producer and consumer of one channel must be checked against each other: {}",
        outcome.text
    );
    assert_ne!(outcome.exit_code, ANALYSIS_ERROR, "{}", outcome.text);
}

// ---------------------------------------------------------------------------
// telemetry
// ---------------------------------------------------------------------------

const SPAN_EXPORT: &str = r#"{
  "resourceSpans": [
    {
      "resource": {
        "attributes": [{ "key": "service.name", "value": { "stringValue": "checkout" } }]
      },
      "scopeSpans": [
        {
          "spans": [
            {
              "traceId": "0102030405060708090a0b0c0d0e0f10",
              "spanId": "0102030405060708",
              "parentSpanId": "0000000000000000",
              "name": "GET /pay",
              "kind": 2,
              "startTimeUnixNano": "1700000000000000000",
              "endTimeUnixNano": "1700000000001000000",
              "attributes": [
                { "key": "http.route", "value": { "stringValue": "/pay" } },
                { "key": "code.function", "value": { "stringValue": "handle_pay" } },
                { "key": "user.email", "value": { "stringValue": "a@example.com" } }
              ]
            }
          ]
        }
      ]
    }
  ]
}"#;

#[test]
fn gate_the_telemetry_commands_refuse_to_run_without_a_salt() {
    let root = scratch("telemetry-no-salt");
    let input = write(&root, "traces.json", SPAN_EXPORT);
    let error = extensions::telemetry(&input, &extensions::TelemetryArgs::default())
        .expect_err("a salt is required");
    assert!(
        error.to_string().contains("no default"),
        "the refusal must say *why* there is no default: {error}"
    );
}

#[test]
fn gate_a_salt_that_is_not_thirty_two_bytes_is_refused() {
    let root = scratch("telemetry-bad-salt");
    let input = write(&root, "traces.json", SPAN_EXPORT);
    for bad in ["abcd", &"ab".repeat(33), &"zz".repeat(32)] {
        let error = extensions::telemetry(
            &input,
            &extensions::TelemetryArgs {
                salt_hex: Some(bad.to_string()),
                ..extensions::TelemetryArgs::default()
            },
        )
        .expect_err("bad salt");
        assert!(
            error.to_string().contains("salt"),
            "the error must name the argument: {error}"
        );
    }
}

#[test]
fn gate_an_ingested_export_redacts_the_email_and_reports_a_digest() {
    let root = scratch("telemetry-ok");
    let input = write(&root, "traces.json", SPAN_EXPORT);
    let outcome = extensions::telemetry(
        &input,
        &extensions::TelemetryArgs {
            salt_hex: Some("5a".repeat(32)),
            ..extensions::TelemetryArgs::default()
        },
    )
    .expect("ingests");
    assert_ne!(outcome.exit_code, ANALYSIS_ERROR, "{}", outcome.text);
    assert!(
        !outcome.json.contains("a@example.com"),
        "a redacted value reached the report"
    );
    let report: serde_json::Value = serde_json::from_str(&outcome.json).expect("valid JSON");
    assert_eq!(report["span_count"], 1);
    assert!(
        report["redaction_totals"]
            .as_object()
            .is_some_and(|totals| !totals.is_empty()),
        "the email detector must have fired: {}",
        outcome.text
    );
    assert!(
        !outcome.json.contains("5a5a5a"),
        "the salt must not be echoed, and must not be recoverable from the report"
    );
    assert!(
        outcome.text.contains("salt is not"),
        "the report says the salt is not recorded: {}",
        outcome.text
    );
}

#[test]
fn gate_an_empty_export_is_unknown_and_exits_four() {
    // \"No spans\" is not \"no problems\". `FR-014`: the empty case is `unknown`, never `verified`.
    let root = scratch("telemetry-empty");
    let input = write(&root, "traces.json", "{\"resourceSpans\": []}");
    let outcome = extensions::telemetry(
        &input,
        &extensions::TelemetryArgs {
            salt_hex: Some("5a".repeat(32)),
            ..extensions::TelemetryArgs::default()
        },
    )
    .expect("ingests");
    let report: serde_json::Value = serde_json::from_str(&outcome.json).expect("valid JSON");
    assert_eq!(report["coverage"], "unknown");
    assert_eq!(outcome.exit_code, UNSUPPORTED, "{}", outcome.text);
}

#[test]
fn gate_a_topology_report_correlates_an_observed_route_against_a_declared_one() {
    let root = scratch("telemetry-topology");
    let input = write(&root, "traces.json", SPAN_EXPORT);
    let surface = write(
        &root,
        "surface.json",
        r#"{ "routes": [ { "method": "GET", "template": "/pay", "operation_id": "pay" } ], "symbols": [] }"#,
    );
    let outcome = extensions::telemetry(
        &input,
        &extensions::TelemetryArgs {
            salt_hex: Some("5a".repeat(32)),
            surface: Some(surface.clone()),
            ..extensions::TelemetryArgs::default()
        },
    )
    .expect("correlates");
    let report: serde_json::Value = serde_json::from_str(&outcome.json).expect("valid JSON");
    assert_eq!(report["observation"]["windows_observed"], 1);
    assert_eq!(
        report["correlations"].as_array().map(Vec::len),
        Some(1),
        "the span names a route the surface declares: {}",
        outcome.text
    );
    assert!(
        report["undocumented_routes"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "the observed route is declared, so it is not undocumented: {}",
        outcome.text
    );
    // One window against a requirement of three: the CLI's own rule decides the code.
    let expected = match report["coverage"].as_str().expect("a string") {
        "unknown" | "unsupported" => UNSUPPORTED,
        _ => PASS,
    };
    assert_eq!(outcome.exit_code, expected, "{}", outcome.text);
}

#[test]
fn gate_a_surface_that_is_not_json_is_an_analysis_error() {
    let root = scratch("telemetry-bad-surface");
    let input = write(&root, "traces.json", SPAN_EXPORT);
    let surface = write(&root, "surface.json", "{ not json");
    let error = extensions::telemetry(
        &input,
        &extensions::TelemetryArgs {
            salt_hex: Some("5a".repeat(32)),
            surface: Some(surface.clone()),
            ..extensions::TelemetryArgs::default()
        },
    )
    .expect_err("a malformed surface is refused");
    assert!(error.to_string().contains("declared surface"), "{error}");
}

#[test]
fn gate_an_export_that_is_not_otlp_json_is_an_analysis_error() {
    let root = scratch("telemetry-bad-export");
    let input = write(&root, "traces.json", "not json at all");
    let error = extensions::telemetry(
        &input,
        &extensions::TelemetryArgs {
            salt_hex: Some("5a".repeat(32)),
            ..extensions::TelemetryArgs::default()
        },
    )
    .expect_err("malformed input");
    assert!(!error.to_string().is_empty());
}

// ---------------------------------------------------------------------------
// catalog
// ---------------------------------------------------------------------------

#[test]
fn gate_an_empty_tree_is_unknown_and_exits_four() {
    let root = scratch("catalog-empty");
    let cli = cli_for(&root).expect("parses");
    let outcome = extensions::catalog(&cli, None, None, false).expect("scans");
    let report: serde_json::Value = serde_json::from_str(&outcome.json).expect("valid JSON");
    assert_eq!(report["coverage"], "unknown");
    assert_eq!(outcome.exit_code, UNSUPPORTED, "{}", outcome.text);
}

#[test]
fn gate_a_catalog_scan_of_this_workspace_finds_the_manifests_and_the_ownership_gaps() {
    let Some(root) = repository_under_test() else {
        return;
    };
    let cli = cli_for(&root).expect("parses");
    let outcome = extensions::catalog(&cli, None, None, false).expect("scans");
    let report: serde_json::Value = serde_json::from_str(&outcome.json).expect("valid JSON");

    assert!(
        report["files_scanned"].as_u64().unwrap_or(0) > 20,
        "every crate's manifest is in scope: {}",
        outcome.text
    );
    let dependencies = report["dependencies"].as_array().expect("an array");
    assert!(
        dependencies
            .iter()
            .any(|dependency| dependency["package"] == "serde"),
        "a registry dependency"
    );
    assert!(
        dependencies
            .iter()
            .any(|dependency| dependency["source"] == "path"),
        "`autopsy-domain = {{ path = … }}` is a monorepo edge, and the report must say so: {}",
        outcome.text
    );
    assert_eq!(
        report["entities"].as_array().map(Vec::len),
        Some(0),
        "this repository has no catalog-info.yaml — and the report says nothing about ownership \
         rather than inventing an owner"
    );
    assert!(
        report["manifest_names_looked_for"]
            .as_array()
            .is_some_and(|names| names.iter().any(|name| name == "catalog-info.yaml")),
        "the scope is stated"
    );
}

#[test]
fn gate_a_catalog_scan_is_byte_identical_across_runs() {
    let Some(root) = repository_under_test() else {
        return;
    };
    let cli = cli_for(&root).expect("parses");
    let first = extensions::catalog(&cli, None, None, false).expect("scans");
    for run in 0..10 {
        let again = extensions::catalog(&cli, None, None, false).expect("scans");
        assert_eq!(again.json, first.json, "run {run} differs");
    }
}

#[test]
fn gate_a_catalog_root_that_does_not_exist_is_an_analysis_error() {
    let cli = cli_for(Path::new(".")).expect("parses");
    let missing = PathBuf::from("/nonexistent/autopsy-step44-catalog");
    let error = extensions::catalog(&cli, Some(&missing), None, false).expect_err("missing root");
    assert!(error.to_string().contains("catalog scan failed"), "{error}");
}

// ---------------------------------------------------------------------------
// The output protocol itself
// ---------------------------------------------------------------------------

#[test]
fn gate_sarif_is_refused_for_evidence_reports_rather_than_approximated() {
    let root = scratch("sarif");
    let cli = cli_for(&root).expect("parses");
    let outcome = extensions::catalog(&cli, None, None, false).expect("scans");
    let error = outcome
        .print(autopsy_cli::OutputFormat::Sarif)
        .expect_err("SARIF describes findings; these commands report evidence");
    assert!(error.to_string().contains("sarif"), "{error}");
}

#[test]
fn gate_an_unknown_argument_is_an_analysis_error() {
    let code = autopsy_cli::run_cli(&[
        "autopsy".to_string(),
        "catalog".to_string(),
        "--not-a-flag".to_string(),
    ]);
    assert_eq!(code, ANALYSIS_ERROR);
}

#[test]
fn gate_the_exit_code_constants_are_the_documented_ones() {
    use autopsy_cli::{EXIT_ANALYSIS_ERROR, EXIT_PASS, EXIT_POLICY_FAIL, EXIT_UNSUPPORTED};
    assert_eq!(EXIT_PASS, PASS);
    assert_eq!(EXIT_POLICY_FAIL, POLICY_FAIL);
    assert_eq!(EXIT_ANALYSIS_ERROR, ANALYSIS_ERROR);
    assert_eq!(EXIT_UNSUPPORTED, UNSUPPORTED);
    assert_eq!(autopsy_cli::ANALYZER_VERSION, "0.1.0");
}

#[test]
fn gate_no_command_in_this_module_reads_a_clock() {
    // `NFR-001` at the CLI boundary: the engines keep clocks out of their digests, and the
    // layer that calls them must not reintroduce one. The mining window anchors on the
    // newest commit's timestamp *from the repository*, which is why neither `SystemTime` nor
    // a `now()` appears here.
    let source = include_str!("../src/extensions.rs");
    for forbidden in ["SystemTime", "Instant::", "now()", "chrono", "Utc"] {
        assert!(
            !source.contains(forbidden),
            "`{forbidden}` appears in the CLI extension layer"
        );
    }
}

#[test]
fn gate_coverage_state_never_becomes_a_pass_through_the_cli() {
    // The rule from the module docs, asserted through the handlers: an unknown answer is `4`
    // without `--strict`, and `--strict` extends the refusal to `partial`.
    let root = scratch("coverage-rule");
    let cli = cli_for(&root).expect("parses");
    let lenient = extensions::catalog(&cli, None, None, false).expect("scans");
    let strict = extensions::catalog(&cli, None, None, true).expect("scans");
    assert_eq!(lenient.exit_code, UNSUPPORTED);
    assert_eq!(strict.exit_code, UNSUPPORTED);
    let report: serde_json::Value = serde_json::from_str(&strict.json).expect("valid JSON");
    assert_eq!(report["coverage"], "unknown");
    assert_eq!(
        serde_json::to_value(CoverageState::Unknown).expect("serializes"),
        serde_json::json!("unknown"),
        "the CLI's table is keyed on the wire spelling of the coverage state"
    );
}

#[test]
fn gate_the_temporal_schema_declares_exactly_what_the_envelope_carries() {
    // Pillar 1's schema was missing until now: the journal's output shape was pinned only by
    // tests, and a schema that lags a struct validates a shape the engine no longer emits.
    // Set equality in both directions, over **every** fact in a real journal — one sample
    // would miss a field that only some facts carry.
    let Some(root) = repository_under_test() else {
        return;
    };
    let cli = cli_for(&root).expect("parses");
    let outcome = extensions::temporal(&cli, None, 100_000, false).expect("builds");
    let document: serde_json::Value =
        serde_json::from_str(&outcome.json).expect("the envelope is canonical JSON");
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/temporal.schema.json");
    let schema: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("schema")).expect("valid JSON");

    fn keys(value: &serde_json::Value) -> Vec<String> {
        let mut keys: Vec<String> = value.as_object().expect("object").keys().cloned().collect();
        keys.sort();
        keys
    }

    assert_eq!(
        keys(&document),
        keys(&schema["properties"]),
        "envelope fields"
    );
    assert_eq!(
        schema["required"],
        serde_json::json!(["journal", "stats", "coverage", "notes"]),
        "all four fields are always present"
    );
    assert_eq!(
        schema["additionalProperties"],
        serde_json::json!(false),
        "the envelope is closed"
    );

    let mut stats: Vec<String> = schema["$defs"]["journalStats"]["properties"]
        .as_object()
        .expect("journal stats properties")
        .keys()
        .cloned()
        .collect();
    stats.sort();
    assert_eq!(keys(&document["stats"]), stats, "journal stats fields");
    let mut required: Vec<String> = schema["$defs"]["journalStats"]["required"]
        .as_array()
        .expect("required")
        .iter()
        .map(|value| value.as_str().expect("string").to_string())
        .collect();
    required.sort();
    assert_eq!(required, stats, "every counter is always emitted");

    let mut declared: Vec<String> = schema["$defs"]["fact"]["properties"]
        .as_object()
        .expect("fact properties")
        .keys()
        .cloned()
        .collect();
    declared.sort();
    let predicates: Vec<String> = schema["$defs"]["fact"]["properties"]["predicate"]["enum"]
        .as_array()
        .expect("the predicate vocabulary")
        .iter()
        .map(|value| value.as_str().expect("string").to_string())
        .collect();
    assert_eq!(
        predicates.len(),
        autopsy_temporal::FactPredicate::all().len(),
        "the schema's predicate vocabulary must be the engine's, or it rejects the engine's own output"
    );

    let facts = document["journal"].as_array().expect("facts");
    assert!(!facts.is_empty(), "the repository under test has history");
    for fact in facts {
        assert_eq!(
            keys(fact),
            declared,
            "a fact carries exactly the declared fields"
        );
        let predicate = fact["predicate"].as_str().expect("a predicate");
        assert!(
            predicates.iter().any(|allowed| allowed == predicate),
            "`{predicate}` is not in the declared vocabulary"
        );
    }
}
