//! Schema/struct drift guard for `schemas/catalog-report.schema.json` (Step 43).
//!
//! The existing pre-commit hook proves the file is valid JSON and nothing more, and a schema
//! can stay valid JSON forever while the Rust enum it enumerates gains a variant. Every
//! comparison below goes **through serde**, so a `#[serde(rename)]` cannot drift unnoticed.
//!
//! The fixture is a real directory tree, because half of what this schema has to describe —
//! the discovery accounting — does not exist off the filesystem.

use autopsy_catalog::{
    DependencyScope, DependencySource, DiscoveryOptions, Ecosystem, EntityKind, MANIFEST_NAMES,
    PRUNED_DIRECTORIES, scan,
};
use autopsy_domain::CoverageState;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

const SCHEMA_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../schemas/catalog-report.schema.json"
);

static NEXT_SCRATCH: AtomicUsize = AtomicUsize::new(0);

fn scratch(name: &str) -> PathBuf {
    let id = NEXT_SCRATCH.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "autopsy-catalog-schema-{}-{id}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

fn write(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent directory");
    }
    fs::write(&path, content).expect("fixture file");
}

fn schema() -> Value {
    serde_json::from_str(&std::fs::read_to_string(SCHEMA_PATH).expect("the schema is readable"))
        .expect("the schema is valid JSON")
}

/// A tree that reaches every list at once: two entity kinds, an owner group, a manifest, an
/// unmodelled kind, and a pruned directory.
fn fixture_report() -> autopsy_catalog::CatalogReport {
    let root = scratch("conformance");
    write(
        &root,
        "catalog-info.yaml",
        "apiVersion: backstage.io/v1alpha1\nkind: Component\nmetadata:\n  name: checkout-api\n  annotations:\n    github.com/project-slug: acme/checkout-api\nspec:\n  type: service\n  owner: group:default/team-payments\n  system: system:default/checkout\n---\napiVersion: backstage.io/v1alpha1\nkind: System\nmetadata:\n  name: checkout\nspec:\n  owner: group:default/team-payments\n",
    );
    write(
        &root,
        "org/catalog-info.yaml",
        "apiVersion: backstage.io/v1alpha1\nkind: Group\nmetadata:\n  name: team-payments\nspec:\n  parent: group:default/engineering\n",
    );
    write(
        &root,
        "web/package.json",
        r#"{"name":"checkout-web","dependencies":{"react":"^18.2.0"}}"#,
    );
    write(&root, "node_modules/dep/package.json", r#"{"name":"dep"}"#);

    let report = scan(&root, &DiscoveryOptions::new()).expect("scans");
    assert!(
        !report.entities.is_empty() && !report.manifests.is_empty(),
        "the fixture must reach the entity and manifest lists or the tests below assert nothing"
    );
    assert!(
        !report.unmodelled_kinds.is_empty() && !report.discovery.pruned_directories.is_empty(),
        "the fixture must reach the unmodelled-kind and prune branches"
    );
    report
}

fn tag<T: serde::Serialize>(value: &T) -> String {
    match serde_json::to_value(value).expect("serializes") {
        Value::String(name) => name,
        other => panic!("expected a string tag, found {other}"),
    }
}

fn enum_names(schema: &Value) -> BTreeSet<String> {
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

fn keys(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("expected an object, found {value}"))
        .keys()
        .cloned()
        .collect()
}

/// Every property name anywhere in the schema document, at any depth.
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
fn the_schema_is_closed_and_draft_2020_12_at_every_level() {
    let schema = schema();
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["additionalProperties"], false);
    for (name, definition) in schema["$defs"].as_object().expect("$defs is an object") {
        if definition.get("properties").is_some() {
            assert_eq!(
                definition["additionalProperties"], false,
                "`{name}` must be closed: an open subschema accepts a report this engine cannot emit"
            );
        }
    }
}

#[test]
fn every_serialized_field_is_declared_and_every_required_field_is_emitted() {
    let schema = schema();
    let report = serde_json::to_value(fixture_report()).expect("the report serializes");

    for (definition, value, name) in [
        (&schema, &report, "<root>"),
        (
            &schema["$defs"]["discovery"],
            &report["discovery"],
            "discovery",
        ),
        (
            &schema["$defs"]["ownership"],
            &report["ownership"],
            "ownership",
        ),
        (
            &schema["$defs"]["entity"],
            report["entities"]
                .as_array()
                .expect("array")
                .first()
                .expect("an entity"),
            "entity",
        ),
        (
            &schema["$defs"]["manifest"],
            report["manifests"]
                .as_array()
                .expect("array")
                .first()
                .expect("a manifest"),
            "manifest",
        ),
        (
            &schema["$defs"]["dependency"],
            report["manifests"][0]["dependencies"]
                .as_array()
                .expect("array")
                .first()
                .expect("a dependency"),
            "dependency",
        ),
    ] {
        let declared = keys(&definition["properties"]);
        let emitted = keys(value);
        let undeclared: Vec<&String> = emitted
            .iter()
            .filter(|key| !declared.contains(*key))
            .collect();
        assert!(
            undeclared.is_empty(),
            "`{name}` emits fields the schema does not declare: {undeclared:?}"
        );
        let missing: Vec<&str> = definition["required"]
            .as_array()
            .expect("`required` is an array")
            .iter()
            .map(|value| value.as_str().expect("a string"))
            .filter(|key| !emitted.contains(*key))
            .collect();
        assert!(
            missing.is_empty(),
            "`{name}` is missing fields the schema requires: {missing:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Vocabulary
// ---------------------------------------------------------------------------

#[test]
fn the_schema_enumerates_every_entity_kind_the_engine_models() {
    let kinds = EntityKind::all();
    assert_eq!(kinds.len(), 5, "the list below must cover every variant");
    assert_eq!(
        enum_names(&schema()["$defs"]["entityKind"]),
        kinds.iter().map(tag).collect::<BTreeSet<String>>()
    );
}

#[test]
fn the_schema_enumerates_every_ecosystem_scope_and_source() {
    let schema = schema();
    assert_eq!(
        enum_names(&schema["$defs"]["ecosystem"]),
        Ecosystem::all()
            .iter()
            .map(tag)
            .collect::<BTreeSet<String>>()
    );
    assert_eq!(
        enum_names(&schema["$defs"]["dependencyScope"]),
        DependencyScope::all()
            .iter()
            .map(tag)
            .collect::<BTreeSet<String>>()
    );
    assert_eq!(
        enum_names(&schema["$defs"]["dependencySource"]),
        DependencySource::all()
            .iter()
            .map(tag)
            .collect::<BTreeSet<String>>()
    );
}

#[test]
fn the_schema_enumerates_every_coverage_state() {
    assert_eq!(
        enum_names(&schema()["$defs"]["coverageState"]),
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
fn the_pruned_directory_names_are_the_ones_the_walker_actually_prunes() {
    // The schema pins the enum, so a directory the walker prunes but the schema does not
    // list fails here — and the reverse, which is the more dangerous direction: a reader
    // cannot be told a directory was skipped that the walker never skips.
    let expected: BTreeSet<String> = PRUNED_DIRECTORIES
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    assert!(
        expected
            .iter()
            .all(|name| !name.contains('/') && !name.is_empty()),
        "a prune entry must be a directory *name*: {expected:?}"
    );
    assert_eq!(
        enum_names(
            &schema()["$defs"]["discovery"]["properties"]["pruned_directories"]["propertyNames"]
        ),
        expected
    );
}

#[test]
fn the_manifest_names_the_report_lists_are_the_ones_the_walker_looks_for() {
    let report = fixture_report();
    assert_eq!(
        report.manifest_names_looked_for,
        MANIFEST_NAMES
            .iter()
            .map(|name| (*name).to_string())
            .collect::<Vec<String>>()
    );
    // `pyproject.toml` is deliberately absent: a reader can see the scan's scope from the
    // report instead of inferring it from silence.
    assert!(
        !report
            .manifest_names_looked_for
            .iter()
            .any(|name| name == "pyproject.toml"),
        "{:?}",
        report.manifest_names_looked_for
    );
}

#[test]
fn every_report_path_is_relative_and_the_pattern_enforces_it() {
    let schema = schema();
    assert_eq!(
        schema["$defs"]["relativePath"]["pattern"], "^[^/]",
        "a leading `/` on a reported path would make the digest machine-dependent"
    );
    let report = serde_json::to_value(fixture_report()).expect("serializes");
    let mut paths: Vec<String> = Vec::new();
    for entity in report["entities"].as_array().expect("array") {
        paths.push(
            entity["source_path"]
                .as_str()
                .expect("a string")
                .to_string(),
        );
    }
    for manifest in report["manifests"].as_array().expect("array") {
        paths.push(manifest["path"].as_str().expect("a string").to_string());
        for dependency in manifest["dependencies"].as_array().expect("array") {
            paths.push(
                dependency["manifest_path"]
                    .as_str()
                    .expect("a string")
                    .to_string(),
            );
        }
    }
    assert!(!paths.is_empty(), "the fixture reached no paths");
    for path in paths {
        assert!(!path.starts_with('/'), "`{path}` is absolute");
        assert!(
            !path.contains(".."),
            "`{path}` contains a traversal component"
        );
    }
}

#[test]
fn no_property_anywhere_names_a_clock_reading() {
    // `NFR-001`: no timestamps in canonical outputs. A catalog scan has no reason to read a
    // clock at all, and a field named for one added later fails here.
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
                || lowered.contains("duration")
        })
        .collect();
    assert!(
        suspicious.is_empty(),
        "a schema field names a clock reading: {suspicious:?}"
    );
    assert!(names.contains("files_scanned"));
    assert!(names.contains("digest"));
}

// ---------------------------------------------------------------------------
// Invariants the schema encodes
// ---------------------------------------------------------------------------

#[test]
fn the_discovery_accounting_is_required_not_optional() {
    // The truncation facts are what let a reader tell a complete scan from a bounded one,
    // and every one of them is required so an omitted field cannot read as "nothing was
    // skipped".
    let required: BTreeSet<String> = schema()["$defs"]["discovery"]["required"]
        .as_array()
        .expect("required is an array")
        .iter()
        .map(|value| value.as_str().expect("a string").to_string())
        .collect();
    for field in [
        "skipped_symlinks",
        "skipped_oversize_files",
        "skipped_beyond_file_limit",
        "pruned_directories",
        "depth_truncated",
        "truncated",
    ] {
        assert!(
            required.contains(field),
            "`{field}` is what makes the scan auditable"
        );
    }
}

#[test]
fn a_dangling_reference_is_distinguished_from_a_reference_to_an_unmodelled_kind() {
    // The distinction the whole ownership index depends on: every `spec.owner` points at a
    // `Group`, and a schema that merged the two would make a normal catalog look broken.
    let schema = schema();
    assert!(
        schema["$defs"]["danglingReference"]["properties"]["target"]
            .get("description")
            .expect("the field explains itself")
            .as_str()
            .expect("a string")
            .contains("not dangling"),
        "the target field must say which references are *not* dangling"
    );
    assert!(
        schema["$defs"]["ownership"]["properties"]["unmodelled_reference_kinds"].is_object(),
        "the two outcomes need two fields"
    );
}

#[test]
fn the_requirement_field_says_it_is_not_a_version() {
    // The misreading this guards against is concrete: someone reads `^18.2.0` as the
    // installed version, and the next question — which services ship the vulnerable build —
    // is answered wrongly.
    let description = schema()["$defs"]["dependency"]["properties"]["requirement"]["description"]
        .as_str()
        .expect("a description")
        .to_string();
    assert!(
        description.contains("lockfile"),
        "the requirement field must name the lockfile gap: {description}"
    );
    let notes = fixture_report()
        .notes
        .iter()
        .any(|note| note.contains("a requirement is not the version"));
    assert!(notes, "and the report must say the same thing in prose");
}

#[test]
fn the_report_round_trips_through_the_schema_vocabulary() {
    let schema = schema();
    let report = serde_json::to_value(fixture_report()).expect("serializes");

    let states = enum_names(&schema["$defs"]["coverageState"]);
    assert!(states.contains(report["coverage"].as_str().expect("a string")));

    let kinds = enum_names(&schema["$defs"]["entityKind"]);
    for entity in report["entities"].as_array().expect("array") {
        let kind = entity["kind"].as_str().expect("a string");
        assert!(kinds.contains(kind), "undeclared entity kind `{kind}`");
        assert_eq!(
            entity["name"],
            entity["reference"]
                .as_str()
                .expect("a string")
                .rsplit_once('/')
                .expect("a namespace")
                .1,
            "the name in the reference and the name field must agree"
        );
    }

    let ecosystems = enum_names(&schema["$defs"]["ecosystem"]);
    for manifest in report["manifests"].as_array().expect("array") {
        assert!(ecosystems.contains(manifest["ecosystem"].as_str().expect("a string")));
    }

    let scopes = enum_names(&schema["$defs"]["dependencyScope"]);
    for dependency in report["dependencies"].as_array().expect("array") {
        assert!(scopes.contains(dependency["scope"].as_str().expect("a string")));
    }
}

#[test]
fn the_fixture_reaches_every_branch_it_claims_to() {
    // The guard on the guard: if the fixture stopped reaching a branch, the tests above
    // would keep passing while checking less and less.
    let report = fixture_report();
    assert_eq!(
        report.coverage,
        CoverageState::Partial,
        "an unmodelled Group degrades it"
    );
    assert!(report.entities.len() >= 2);
    assert_eq!(report.manifests.len(), 1);
    assert_eq!(report.dependencies.len(), 1);
    assert!(!report.unmodelled_kinds.is_empty());
    assert!(!report.discovery.pruned_directories.is_empty());
    assert!(!report.ownership.by_owner.is_empty());
    assert!(!report.notes.is_empty());
    assert_eq!(report.digest.len(), 64);
}
