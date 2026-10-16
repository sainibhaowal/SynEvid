//! Step 43 gate tests: Backstage `catalog-info.yaml` ingestion, dependency manifests, and
//! organisational blast radius (`R28``R29``R37`).
//!
//! Every fixture is a real directory tree written to disk, because the bounds this step has
//! to honour — symlinks, file size, depth, pruned directories — live in the filesystem
//! layer and are invisible to a test that hands the parser a string.

use autopsy_catalog::{DiscoveryOptions, EntityKind, MANIFEST_NAMES, OwnershipIndex, scan};
use autopsy_domain::CoverageState;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The shared fixture root keys on the process id; the counter keeps two tests in one
/// binary from sharing a directory. (Step 42 lost twelve tests to exactly that.)
static NEXT_SCRATCH: AtomicUsize = AtomicUsize::new(0);

fn scratch(name: &str) -> PathBuf {
    let id = NEXT_SCRATCH.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "autopsy-catalog-{}-{id}-{name}",
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

const ROOT_CATALOG: &str = r#"
apiVersion: backstage.io/v1alpha1
kind: Domain
metadata:
  name: payments
spec:
  owner: group:default/team-payments
---
apiVersion: backstage.io/v1alpha1
kind: System
metadata:
  name: checkout
spec:
  domain: domain:default/payments
  owner: group:default/team-payments
---
apiVersion: backstage.io/v1alpha1
kind: API
metadata:
  name: checkout
spec:
  type: openapi
  owner: group:default/team-payments
  system: system:default/checkout
---
apiVersion: backstage.io/v1alpha1
kind: Component
metadata:
  name: refunds-worker
spec:
  type: worker
  owner: group:default/team-payments
  system: system:default/checkout
  dependsOn:
    - component:default/checkout-api
"#;

const SERVICE_CATALOG: &str = r#"
apiVersion: backstage.io/v1alpha1
kind: Component
metadata:
  name: checkout-api
  description: Takes payments
  annotations:
    github.com/project-slug: acme/checkout-api
spec:
  type: service
  lifecycle: production
  owner: group:default/team-payments
  system: system:default/checkout
  providesApis:
    - api:default/checkout
"#;

const PACKAGE_JSON: &str = r#"{
  "name": "checkout-web",
  "dependencies": { "react": "^18.2.0" },
  "devDependencies": { "vitest": "^1.0.0" }
}"#;

/// A repository with a root catalog, a service catalog, and three manifests.
fn repository(name: &str) -> PathBuf {
    let root = scratch(name);
    write(&root, "catalog-info.yaml", ROOT_CATALOG);
    write(
        &root,
        "services/checkout/catalog-info.yaml",
        SERVICE_CATALOG,
    );
    write(&root, "web/package.json", PACKAGE_JSON);
    write(
        &root,
        "workers/requirements.txt",
        "requests==2.31.0\nflask>=3.0\n",
    );
    write(
        &root,
        "tools/go.mod",
        "module github.com/acme/tools\n\ngo 1.22\n\nrequire github.com/foo/bar v1.2.3\n",
    );
    root
}

// ---------------------------------------------------------------------------
// The gate
// ---------------------------------------------------------------------------

#[test]
fn gate_a_repository_yields_entities_ownership_and_manifests_in_one_report() {
    let root = repository("gate-scan");
    let report = scan(&root, &DiscoveryOptions::new()).expect("scans");

    assert_eq!(report.files_scanned, 5, "two catalogs and three manifests");
    assert_eq!(
        report.entities.len(),
        5,
        "a domain, a system, an API and two components"
    );
    assert_eq!(report.entities_of_kind(EntityKind::Component).len(), 2);
    assert_eq!(report.manifest_names_looked_for.len(), MANIFEST_NAMES.len());
    assert_eq!(report.ecosystems_read.len(), 3, "npm, pip and go");
    assert_eq!(
        report.dependencies.len(),
        5,
        "react and vitest, requests and flask, github.com/foo/bar"
    );
    assert_eq!(report.teams(), ["group:default/team-payments"]);

    let radius = report
        .blast_radius("component:default/checkout-api")
        .expect("the component is in the catalog");
    assert!(radius.found);
    assert_eq!(
        radius.co_owned_components,
        ["component:default/refunds-worker"]
    );
    assert_eq!(radius.domain.as_deref(), Some("domain:default/payments"));
    assert_eq!(
        radius.dependents,
        ["component:default/refunds-worker"],
        "the worker declares a dependency on the component"
    );

    assert_eq!(
        report.coverage,
        CoverageState::Verified,
        "nothing in the tree was skipped and every document was read in full: {:?}",
        report.notes
    );
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("a requirement is not the version")),
        "the lockfile gap has to be stated, not left to the reader: {:?}",
        report.notes
    );
    assert_eq!(report.digest.len(), 64);
    assert_eq!(
        report.ownership.dangling.len(),
        0,
        "{:?}",
        report.ownership.dangling
    );
}

#[test]
fn gate_the_repositorys_own_component_is_resolved_from_evidence_not_from_a_name() {
    let root = repository("own-component");
    let report = scan(&root, &DiscoveryOptions::new()).expect("scans");

    // Two independent facts, either of which a caller can use, and neither of which is an
    // inference from a directory name.
    assert_eq!(
        report
            .ownership
            .components_declared_at("services/checkout", &report.entities),
        ["component:default/checkout-api"]
    );
    assert_eq!(
        report
            .ownership
            .components_for_github_project("acme/checkout-api", &report.entities),
        ["component:default/checkout-api"]
    );
    assert!(
        report
            .ownership
            .components_declared_at("services/refunds", &report.entities)
            .is_empty(),
        "a directory with no catalog-info.yaml declares nothing"
    );
}

#[test]
fn gate_an_unknown_component_is_unknown_rather_than_zero_impact() {
    let root = repository("unknown-component");
    let report = scan(&root, &DiscoveryOptions::new()).expect("scans");
    let radius = report
        .blast_radius("component:default/nope")
        .expect("an answer");
    assert!(!radius.found);
    assert_eq!(radius.coverage, CoverageState::Unknown);
    assert!(radius.touched_entities().is_empty());
}

// ---------------------------------------------------------------------------
// Sandboxing bounds
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn gate_a_symlink_is_not_followed() {
    let root = repository("symlink");
    let outside = scratch("symlink-outside");
    write(
        &outside,
        "package.json",
        r#"{"name":"outside","dependencies":{"evil":"1"}}"#,
    );
    std::os::unix::fs::symlink(
        outside.join("package.json"),
        root.join("linked-package.json"),
    )
    .expect("symlink");
    std::os::unix::fs::symlink(&outside, root.join("vendor-link")).expect("directory symlink");

    let report = scan(&root, &DiscoveryOptions::new()).expect("scans");
    assert_eq!(
        report.discovery.skipped_symlinks, 2,
        "the file symlink and the directory symlink"
    );
    assert!(
        !report
            .dependencies
            .iter()
            .any(|dependency| dependency.package == "evil"),
        "a path inside the tree must not reach a file outside it"
    );
    assert_eq!(report.files_scanned, 5);
    assert_eq!(
        report.coverage,
        CoverageState::Partial,
        "{:?}",
        report.notes
    );
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("symbolic link")),
        "{:?}",
        report.notes
    );
}

#[test]
fn gate_an_oversize_file_is_recorded_rather_than_read() {
    let root = repository("oversize");
    let big = format!(
        "{{\"name\": \"big\", \"dependencies\": {{\"x\": \"{0}\"}}}}",
        "1".repeat(4096)
    );
    write(&root, "huge/package.json", &big);

    let report = scan(&root, &DiscoveryOptions::new().with_max_file_bytes(512)).expect("scans");
    assert!(
        report
            .discovery
            .skipped_oversize_files
            .contains(&"huge/package.json".to_string()),
        "`INV-SANDBOX-004`: an oversize file is named, not dropped: {:?}",
        report.discovery.skipped_oversize_files
    );
    for skipped in &report.discovery.skipped_oversize_files {
        let size = fs::metadata(root.join(skipped))
            .expect("the file exists")
            .len();
        assert!(
            size > 512,
            "{skipped} was skipped at {size} bytes, which is within the bound"
        );
        assert!(
            !report
                .manifests
                .iter()
                .any(|manifest| manifest.path == *skipped),
            "{skipped} was both skipped and read"
        );
    }
    // Every candidate file is either read or named as skipped. Dropping one silently would
    // make the report look complete while a manifest was missing from it.
    assert_eq!(
        report.files_scanned + report.discovery.skipped_oversize_files.len(),
        6,
        "five readable files plus the oversize one"
    );
    assert_eq!(report.coverage, CoverageState::Partial);
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("exceeded the size bound")),
        "{:?}",
        report.notes
    );
}

#[test]
fn gate_the_depth_bound_is_recorded_and_bounds_the_walk() {
    let root = repository("depth");
    write(&root, "a/b/c/d/package.json", PACKAGE_JSON);
    let report = scan(&root, &DiscoveryOptions::new().with_max_depth(1)).expect("scans");
    assert!(
        !report
            .manifests
            .iter()
            .any(|manifest| manifest.path == "a/b/c/d/package.json"),
        "a file past the depth bound is not read"
    );
    assert!(!report.discovery.depth_truncated.is_empty());
    assert_eq!(report.coverage, CoverageState::Partial);
}

#[test]
fn gate_a_pruned_directory_is_not_walked_and_is_recorded() {
    let root = repository("pruned");
    write(&root, "node_modules/dep/package.json", PACKAGE_JSON);
    write(&root, "target/debug/build/catalog-info.yaml", ROOT_CATALOG);
    let report = scan(&root, &DiscoveryOptions::new()).expect("scans");
    assert_eq!(
        report.files_scanned, 5,
        "nothing under a pruned directory is read"
    );
    assert_eq!(
        report.discovery.pruned_directories.get("node_modules"),
        Some(&1)
    );
    assert_eq!(report.discovery.pruned_directories.get("target"), Some(&1));
    assert!(
        report.discovery.is_complete(),
        "pruning `node_modules` and `target` is policy, not truncation: those never hold a \
         hand-written catalog, and the prune is still recorded in `pruned_directories`"
    );
    assert_eq!(report.coverage, CoverageState::Verified);
}

#[test]
fn gate_a_missing_root_is_an_error_and_an_empty_root_is_unknown() {
    let missing = scan(
        Path::new("/nonexistent/autopsy-catalog-root"),
        &DiscoveryOptions::new(),
    )
    .expect_err("an absent catalog is an error");
    assert!(
        missing.to_string().contains("could not be read"),
        "{missing}"
    );

    let empty = scratch("empty");
    let report = scan(&empty, &DiscoveryOptions::new()).expect("scans");
    assert_eq!(report.coverage, CoverageState::Unknown);
    assert_eq!(report.files_scanned, 0);
    assert!(report.entities.is_empty());
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("no catalog or dependency manifest was found")),
        "{:?}",
        report.notes
    );
}

#[test]
fn gate_a_parent_directory_component_in_a_path_is_refused() {
    let error = autopsy_catalog::discover::safe_relative(Path::new("../escape/catalog-info.yaml"))
        .expect_err("`..` is not a relative path inside the tree");
    assert!(
        error.to_string().contains("escapes the catalog root"),
        "{error}"
    );
}

// ---------------------------------------------------------------------------
// Partial failure
// ---------------------------------------------------------------------------

#[test]
fn gate_one_malformed_manifest_does_not_remove_the_rest_of_the_report() {
    let root = repository("malformed");
    write(&root, "broken/package.json", "{ not json");
    let report = scan(&root, &DiscoveryOptions::new()).expect("scans");

    assert_eq!(report.files_scanned, 6);
    assert_eq!(
        report.manifests.len(),
        3,
        "the other three manifests were still read"
    );
    assert_eq!(report.entities.len(), 5, "the entities are untouched");
    assert_eq!(
        report.refusals.len(),
        1,
        "and the refusal is counted, not swallowed: {:?}",
        report.refusals
    );
    assert!(
        report
            .refusals
            .keys()
            .next()
            .expect("one")
            .contains("unparsable manifest"),
        "{:?}",
        report.refusals
    );
    assert_eq!(report.coverage, CoverageState::Partial);
}

#[test]
fn gate_an_unmodelled_entity_kind_is_counted_and_named() {
    let root = repository("group-kind");
    write(
        &root,
        "org/catalog-info.yaml",
        "apiVersion: backstage.io/v1alpha1\nkind: Group\nmetadata:\n  name: team-payments\nspec:\n  parent: group:default/engineering\n",
    );
    let report = scan(&root, &DiscoveryOptions::new()).expect("scans");
    assert_eq!(report.unmodelled_kinds.get("Group"), Some(&1));
    assert!(
        !report
            .entities
            .iter()
            .any(|entity| entity.reference == "group:default/team-payments"),
        "a Group is not one of the five modelled kinds"
    );
    assert_eq!(report.coverage, CoverageState::Partial);
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("a parent team")),
        "the reason a Group matters is stated: {:?}",
        report.notes
    );
}

#[test]
fn gate_an_entity_declared_twice_is_reported_rather_than_resolved_by_iteration_order() {
    let root = repository("duplicate-entity");
    write(
        &root,
        "other/catalog-info.yaml",
        "apiVersion: backstage.io/v1alpha1\nkind: Component\nmetadata:\n  name: checkout-api\nspec:\n  type: service\n  owner: group:default/team-somebody-else\n",
    );
    let report = scan(&root, &DiscoveryOptions::new()).expect("scans");
    assert_eq!(
        report.duplicate_entities.len(),
        1,
        "{:?}",
        report.duplicate_entities
    );
    let duplicate = &report.duplicate_entities[0];
    assert_eq!(duplicate.reference, "component:default/checkout-api");
    assert_eq!(duplicate.kept_source, "other/catalog-info.yaml");
    assert_eq!(
        duplicate.ignored_source,
        "services/checkout/catalog-info.yaml"
    );
    assert_eq!(
        report
            .entities
            .iter()
            .filter(|entity| entity.reference == "component:default/checkout-api")
            .count(),
        1,
        "one reference, one entity: an index keyed by reference cannot hold two"
    );
    assert_eq!(report.coverage, CoverageState::Partial);
    assert!(
        report
            .notes
            .iter()
            .any(|note| note.contains("more than one document")),
        "{:?}",
        report.notes
    );
}

#[test]
fn gate_an_entity_with_a_dangling_reference_degrades_the_report() {
    let root = repository("dangling");
    write(
        &root,
        "unfinished/catalog-info.yaml",
        "apiVersion: backstage.io/v1alpha1\nkind: Component\nmetadata:\n  name: half-built\nspec:\n  owner: group:default/team-payments\n  dependsOn:\n    - component:default/does-not-exist\n",
    );
    let report = scan(&root, &DiscoveryOptions::new()).expect("scans");
    assert_eq!(report.ownership.dangling.len(), 1);
    assert_eq!(
        report.ownership.dangling[0].target,
        "component:default/does-not-exist"
    );
    assert_eq!(report.coverage, CoverageState::Partial);
    let radius = report
        .blast_radius("component:default/half-built")
        .expect("resolves");
    assert!(
        radius.unresolved[0].contains("is not in the catalog"),
        "{:?}",
        radius.unresolved
    );
    assert_eq!(radius.coverage, CoverageState::Partial);
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn gate_one_hundred_scans_of_one_tree_produce_one_digest() {
    let root = repository("determinism");
    let first = scan(&root, &DiscoveryOptions::new()).expect("scans");
    for run in 0..100 {
        let again = scan(&root, &DiscoveryOptions::new()).expect("scans");
        assert_eq!(again.digest, first.digest, "run {run} differs");
        assert_eq!(again, first, "run {run} differs structurally");
    }
}

#[test]
fn gate_the_digest_does_not_depend_on_where_the_tree_lives() {
    // A digest that moved with the absolute path would make one repository hash differently
    // on two machines, which is `NFR-001`'s failure mode and a CI cache that never hits.
    let one = repository("root-a");
    let two = repository("root-b");
    let first = scan(&one, &DiscoveryOptions::new()).expect("scans");
    let second = scan(&two, &DiscoveryOptions::new()).expect("scans");
    assert_eq!(first.digest, second.digest);
    let text = serde_json::to_string(&first).expect("serializes");
    assert!(
        !text.contains(&one.display().to_string()),
        "the absolute root reached the report"
    );
}

#[test]
fn gate_the_digest_moves_when_the_catalog_changes_it() {
    // The control for the two tests above: an equality that holds for everything is not a
    // property.
    let root = repository("digest-moves");
    let before = scan(&root, &DiscoveryOptions::new()).expect("scans");
    write(
        &root,
        "org/catalog-info.yaml",
        "apiVersion: backstage.io/v1alpha1\nkind: Component\nmetadata:\n  name: new-thing\nspec:\n  owner: group:default/team-payments\n",
    );
    let after = scan(&root, &DiscoveryOptions::new()).expect("scans");
    assert_ne!(before.digest, after.digest);
    assert_ne!(before.entities.len(), after.entities.len());
}

#[test]
fn gate_the_digest_ignores_prose_but_not_facts() {
    // `notes` is prose derived from the facts the digest already covers, so editing it must
    // not move the digest — and the facts must.
    let root = repository("digest-prose");
    let mut report = scan(&root, &DiscoveryOptions::new()).expect("scans");
    let before = report.digest.clone();
    report.notes.push("a note nobody asked for".to_string());
    report.recompute_digest();
    assert_eq!(report.digest, before, "prose is excluded from the digest");

    report.entities[0].description = Some("changed".to_string());
    report.recompute_digest();
    assert_ne!(
        report.digest, before,
        "a changed entity is a different catalog"
    );
}

#[test]
fn gate_the_ownership_index_is_a_pure_function_of_the_entities() {
    // The index is derived, and it is reported, so building it twice must give one answer.
    let root = repository("index-pure");
    let report = scan(&root, &DiscoveryOptions::new()).expect("scans");
    assert_eq!(
        OwnershipIndex::new(&report.entities),
        report.ownership,
        "the reported index is the one the entities imply"
    );
}
