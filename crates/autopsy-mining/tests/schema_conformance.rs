//! Schema/struct drift guard for `schemas/cochange-report.schema.json` (Step 42).
//!
//! Same failure the Step 40 and Step 41 guards exist for: the pre-commit hook proves only
//! that the file parses, and a schema can stay valid JSON forever while the Rust enum it
//! enumerates gains a variant. Every comparison goes **through serde**, so a
//! `#[serde(rename)]` cannot drift unnoticed.
//!
//! The fixture is built as a `HistoryWindow` and mined by the pure engine, so this guard
//! needs no repository — and the repository path is a thin reader on top of exactly that
//! function.

use autopsy_domain::{CoverageState, EdgeKind};
use autopsy_mining::{
    ChangeSet, CoChangeOptions, CoChangeReport, EdgeClaim, ExclusionReason, HistoryWindow, mine,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const SCHEMA_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/cochange-report.schema.json"
);

fn schema() -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(SCHEMA_PATH).expect("the co-change schema is readable"),
    )
    .expect("the co-change schema is valid JSON")
}

fn change_set(oid: &str, paths: &[&str]) -> ChangeSet {
    ChangeSet {
        oid: oid.to_string(),
        paths: paths.iter().map(|path| (*path).to_string()).collect(),
        renamed: 0,
    }
}

/// A window that reaches every list at once: reported edges, a pair below the floor, paths
/// that never coupled, and three kinds of exclusion.
fn fixture() -> CoChangeReport {
    let mut window = HistoryWindow::empty(15_778_800, 30);
    window.reference_oid = "a".repeat(40);
    window.commit_set_digest = "b".repeat(64);
    window.changes = vec![
        change_set("0000000000000000000000000000000000000001", &["a", "b"]),
        change_set("0000000000000000000000000000000000000002", &["a", "b"]),
        change_set("0000000000000000000000000000000000000003", &["a"]),
        change_set("0000000000000000000000000000000000000004", &["solo"]),
        // A pair seen once, so it is below the reporting floor and counted instead.
        change_set("0000000000000000000000000000000000000005", &["c", "d"]),
    ];
    window.accounting.commits_seen = 10;
    window.accounting.commits_considered = 5;
    window.accounting.pure_renames_collapsed = 2;
    window.accounting.excluded = BTreeMap::from([
        (ExclusionReason::RootCommit, 1usize),
        (ExclusionReason::OutsideWindow, 3),
        // Biases the population, so the report is not `Verified`.
        (ExclusionReason::MassCommit, 1),
    ]);
    window.commit_set_digest = "b".repeat(64);

    let report = mine(&window, &CoChangeOptions::new());
    assert!(
        !report.edges.is_empty(),
        "the fixture must reach the edge list or the tests below assert nothing"
    );
    assert!(
        report.pairs_below_threshold > 0,
        "the fixture must reach the below-floor branch"
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

/// Every key anywhere in the schema document, at any depth.
fn all_property_names(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if key == "properties"
                    && let Value::Object(properties) = child
                {
                    out.extend(properties.keys().cloned());
                }
                all_property_names(child, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                all_property_names(item, out);
            }
        }
        _ => {}
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

    for (definition, value) in [
        ("windowSummary", &report["window"]),
        (
            "coChangeEdge",
            report["edges"]
                .as_array()
                .expect("array")
                .first()
                .expect("the fixture mines an edge"),
        ),
    ] {
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
fn the_schema_enumerates_every_exclusion_reason() {
    let declared = schema_strings(&schema()["$defs"]["exclusionReason"]);
    let reasons = ExclusionReason::all();
    assert_eq!(reasons.len(), 7, "the list below must cover every variant");
    assert_eq!(
        declared,
        reasons.iter().map(tag).collect::<BTreeSet<String>>()
    );
}

#[test]
fn the_schema_enumerates_every_coverage_state() {
    assert_eq!(
        schema_strings(&schema()["$defs"]["coverageState"]),
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
fn the_claim_is_pinned_to_its_single_value_on_every_edge() {
    // The design asks for edges "tagged with a non-causality warning". A tag that lives in
    // a document can be skipped; this one cannot, and the schema pins it so a
    // half-populated edge fails here rather than in a reader's head.
    assert_eq!(
        schema()["$defs"]["edgeClaim"]["const"],
        tag(&EdgeClaim::TemporalCoOccurrenceOnly)
    );
    for edge in fixture().edges {
        assert_eq!(edge.claim, EdgeClaim::TemporalCoOccurrenceOnly);
    }
}

#[test]
fn the_kind_is_pinned_to_the_domain_vocabulary_and_is_not_a_call() {
    let schema = schema();
    assert_eq!(
        schema["$defs"]["coChangeEdge"]["properties"]["kind"]["const"],
        tag(&EdgeKind::StatisticalCoChange)
    );
    // The distinction the design asks for, at the type level as well as in the schema.
    assert_ne!(EdgeKind::StatisticalCoChange, EdgeKind::Calls);
    assert_eq!(
        serde_json::to_string(&EdgeKind::Calls).expect("serializes"),
        "\"calls\""
    );
}

// ---------------------------------------------------------------------------
// Invariants the schema encodes
// ---------------------------------------------------------------------------

#[test]
fn every_ratio_is_reported_in_ppm_within_its_range() {
    let schema = schema();
    // The bound lives in `$defs/ppm` and every estimate references it. Checked through the
    // reference rather than by reading a `maximum` that is not there: a test that read the
    // inline key would pass on a schema where the field had quietly lost its bound.
    assert_eq!(
        schema["$defs"]["ppm"]["maximum"], 1_000_000,
        "a proportion cannot exceed certainty"
    );
    for field in [
        "support_ppm",
        "confidence_a_to_b_ppm",
        "confidence_a_to_b_lower_ppm",
        "confidence_a_to_b_upper_ppm",
        "confidence_b_to_a_ppm",
        "confidence_b_to_a_lower_ppm",
        "confidence_b_to_a_upper_ppm",
    ] {
        assert_eq!(
            schema["$defs"]["coChangeEdge"]["properties"][field]["$ref"], "#/$defs/ppm",
            "`{field}` is a proportion and must reference the bounded definition"
        );
    }
    // Lift has no upper bound: a rare pair that always travels together produces a huge
    // number, and clamping it would hide the number that most invites the wrong reading.
    assert!(
        schema["$defs"]["coChangeEdge"]["properties"]["lift_ppm"]
            .get("maximum")
            .is_none(),
        "lift must not be capped"
    );

    for edge in fixture().edges {
        for value in [
            edge.support_ppm,
            edge.confidence_a_to_b_ppm,
            edge.confidence_a_to_b_lower_ppm,
            edge.confidence_a_to_b_upper_ppm,
            edge.confidence_b_to_a_ppm,
            edge.confidence_b_to_a_lower_ppm,
            edge.confidence_b_to_a_upper_ppm,
        ] {
            assert!(value <= 1_000_000, "{edge:?}");
        }
    }
}

#[test]
fn the_counts_beside_each_ratio_are_consistent_with_it() {
    // The counts are what a reader checks the ratio against, so they have to be internally
    // possible: a pair cannot co-change more often than either path changes, and neither
    // can exceed the population.
    let report = fixture();
    for edge in &report.edges {
        assert!(edge.a < edge.b, "the pair is stored in one canonical order");
        assert!(
            edge.commits_together <= edge.commits_with_a.min(edge.commits_with_b),
            "{edge:?}"
        );
        assert!(
            edge.commits_with_a.max(edge.commits_with_b) <= edge.total_commits,
            "{edge:?}"
        );
        assert!(
            edge.confidence_a_to_b_lower_ppm <= edge.confidence_a_to_b_ppm
                && edge.confidence_a_to_b_ppm <= edge.confidence_a_to_b_upper_ppm,
            "the interval must bracket its point estimate: {edge:?}"
        );
        assert!(
            edge.confidence_b_to_a_lower_ppm <= edge.confidence_b_to_a_ppm
                && edge.confidence_b_to_a_ppm <= edge.confidence_b_to_a_upper_ppm,
            "{edge:?}"
        );
    }
}

#[test]
fn the_schema_declares_no_timestamp_field_anywhere() {
    // `NFR-001`: no timestamps in canonical outputs. A window is a *duration* anchored at a
    // commit id, so the property holds by construction — and a field named for a clock
    // reading added later would fail here.
    let mut names: BTreeSet<String> = BTreeSet::new();
    all_property_names(&schema(), &mut names);
    let suspicious: Vec<&String> = names
        .iter()
        .filter(|name| {
            let lowered = name.to_lowercase();
            lowered.contains("timestamp")
                || lowered == "date"
                || lowered == "time"
                || lowered.ends_with("_at")
        })
        .collect();
    assert!(
        suspicious.is_empty(),
        "a schema field names a clock reading: {suspicious:?}"
    );
    assert!(names.contains("window_seconds"), "the window is a duration");
    assert!(names.contains("reference_oid"), "the anchor is a commit id");
}

#[test]
fn the_population_is_accounted_for_in_the_schema_as_well_as_the_engine() {
    // `commits_seen == commits_considered + Σ excluded[*]`. Dropping a commit from the
    // denominator *raises* every support figure while looking like nothing happened, so the
    // fields that make the identity checkable are required, not optional.
    let report = fixture();
    let excluded: usize = report.window.excluded.values().sum();
    assert_eq!(
        report.window.commits_seen,
        report.window.commits_considered + excluded
    );
    let required = schema()["$defs"]["windowSummary"]["required"]
        .as_array()
        .expect("required is an array")
        .iter()
        .map(|value| value.as_str().expect("a string").to_string())
        .collect::<BTreeSet<String>>();
    for field in [
        "commits_seen",
        "commits_considered",
        "excluded",
        "walk_truncated",
    ] {
        assert!(
            required.contains(field),
            "`{field}` is what makes the population auditable"
        );
    }
}

#[test]
fn a_degraded_report_says_why_in_its_own_vocabulary() {
    // The fixture excludes a mass commit, so the population is a lower bound and the
    // coverage field plus the notes have to say so rather than leaving the numbers looking
    // complete.
    let report = fixture();
    assert_eq!(report.coverage, CoverageState::Partial);
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("lower bound rather than the whole population")),
        "{:?}",
        report.notes
    );
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("says nothing about causality")),
        "the interval invites exactly one misreading and it is named: {:?}",
        report.notes
    );
}

#[test]
fn no_raw_commit_identifier_appears_in_a_form_a_reader_would_mistake_for_content() {
    // Commit ids *are* reported — they are content-addressed and pin the population — but
    // the change sets themselves are reduced to paths, and no message, author or payload
    // from a commit reaches the report.
    let text = serde_json::to_string(&fixture()).expect("serializes");
    assert!(
        !text.contains("\"message\""),
        "a commit message reached the report"
    );
    assert!(
        !text.contains("\"author\""),
        "a commit author reached the report"
    );
    assert!(!text.contains("\"diff\""), "a diff reached the report");
    assert!(text.contains(&"a".repeat(40)), "the anchor is reported");
}

#[test]
fn the_report_round_trips_through_the_schema_vocabulary() {
    // Not a validator — a cheap structural check that every value the fixture emits for an
    // enumerated field is one the schema lists, so a new variant fails here.
    let schema = schema();
    let report = serde_json::to_value(fixture()).expect("serializes");

    let states = schema_strings(&schema["$defs"]["coverageState"]);
    let value = report["coverage"].as_str().expect("a string");
    assert!(states.contains(value), "undeclared coverage `{value}`");

    let reasons = schema_strings(&schema["$defs"]["exclusionReason"]);
    for (reason, count) in report["window"]["excluded"].as_object().expect("an object") {
        assert!(reasons.contains(reason), "undeclared reason `{reason}`");
        assert!(count.as_u64().is_some_and(|count| count >= 1));
    }

    assert_eq!(
        report["edges"]
            .as_array()
            .expect("array")
            .first()
            .expect("an edge")["kind"],
        schema["$defs"]["coChangeEdge"]["properties"]["kind"]["const"]
    );
}

#[test]
fn the_fixture_reaches_every_branch_it_claims_to() {
    // The guard on the guard: if the fixture stopped reaching a branch, the tests above
    // would keep passing while checking less and less.
    let report = fixture();
    assert_eq!(report.window.commits_considered, 5);
    assert_eq!(report.window.pure_renames_collapsed, 2);
    assert!(!report.edges.is_empty(), "reported edges");
    assert!(report.pairs_below_threshold > 0, "below-floor pairs");
    assert!(report.paths_never_coupled > 0, "never-coupled paths");
    assert!(report.window.excluded.len() >= 3, "several exclusions");
    assert!(!report.notes.is_empty(), "notes");
    assert_eq!(report.digest.len(), 64, "a digest");
}
