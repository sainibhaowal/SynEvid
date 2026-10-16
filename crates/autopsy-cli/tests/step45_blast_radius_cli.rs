//! Step 45 gate tests: the organisational blast-radius command.
//!
//! The fixture is a small Backstage catalog that the Web Inspector and the MCP tool both
//! consume, written with **bare names** in the relationship fields (`owner: team-x`), because
//! that is how real catalogs are written and it is the spelling the engine used to get wrong:
//! every bare name was read as a component, so `owner: team-x` became
//! `component:default/team-x` — a well-formed reference that resolves to nothing. Nothing
//! failed; the joins across entities silently did not happen.
//!
//! The fixture also carries the three outcomes the traversal must distinguish: a reference
//! that resolves, a reference to an unmodelled kind (`group`, counted and noted), and a
//! reference that dangles (degrading coverage and never counted as an edge).

use autopsy_cli::extensions;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Exit codes, restated rather than imported: a test that reads the constant it is checking
/// cannot catch a constant that changed.
const PASS: i32 = 0;
const UNSUPPORTED: i32 = 4;

static NEXT_SCRATCH: AtomicUsize = AtomicUsize::new(0);

fn scratch(name: &str) -> PathBuf {
    let id = NEXT_SCRATCH.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "autopsy-cli-step45-{}-{id}-{name}",
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

fn entity(kind: &str, name: &str, spec: &str) -> String {
    format!(
        "apiVersion: backstage.io/v1alpha1\nkind: {kind}\nmetadata:\n  name: {name}\nspec:\n{spec}"
    )
}

/// Eight entities: two groups' worth of ownership, one system inside one domain, three
/// components (the subject, a sibling, and a consumer), and two APIs.
fn catalog_fixture(root: &Path) {
    write(
        root,
        "groups/team-x/catalog-info.yaml",
        &entity(
            "Group",
            "team-x",
            "  type: team\n  profile:\n    displayName: Team X\n",
        ),
    );
    write(
        root,
        "domains/shop/catalog-info.yaml",
        &entity("Domain", "shop", "  owner: team-x\n"),
    );
    write(
        root,
        "systems/checkout-system/catalog-info.yaml",
        &entity(
            "System",
            "checkout-system",
            "  owner: team-x\n  domain: shop\n",
        ),
    );
    write(
        root,
        "components/checkout/catalog-info.yaml",
        &entity(
            "Component",
            "checkout",
            "  type: service\n  lifecycle: production\n  owner: team-x\n  \
             system: checkout-system\n  providesApis:\n    - checkout-api\n  \
             consumesApis:\n    - payments-api\n",
        ),
    );
    write(
        root,
        "components/cart/catalog-info.yaml",
        &entity(
            "Component",
            "cart",
            "  type: service\n  lifecycle: production\n  owner: team-x\n  \
             system: checkout-system\n",
        ),
    );
    write(
        root,
        "components/web/catalog-info.yaml",
        &entity(
            "Component",
            "web",
            "  type: website\n  lifecycle: production\n  owner: team-x\n  \
             system: checkout-system\n  dependsOn:\n    - component:default/checkout\n",
        ),
    );
    write(
        root,
        "apis/checkout-api/catalog-info.yaml",
        &entity(
            "API",
            "checkout-api",
            "  type: openapi\n  lifecycle: production\n  owner: team-x\n",
        ),
    );
    // Owned by a group that is not in the catalog: an owner reference that dangles. It is
    // not on the subject's own radius, and it must not be silently dropped either.
    write(
        root,
        "apis/payments-api/catalog-info.yaml",
        &entity(
            "API",
            "payments-api",
            "  type: openapi\n  lifecycle: production\n  owner: team-payments\n",
        ),
    );
}

fn radius_of(root: &Path, reference: &str, strict: bool) -> extensions::Outcome {
    let cli = cli_for(root).expect("parses");
    extensions::blast_radius(&cli, reference, None, None, strict).expect("a value")
}

fn radius_json(outcome: &extensions::Outcome) -> serde_json::Value {
    let document: serde_json::Value =
        serde_json::from_str(&outcome.json).expect("the CLI emits canonical JSON");
    document["blast_radius"].clone()
}

#[test]
fn gate_a_verified_radius_joins_the_owner_system_apis_and_dependents() {
    let root = scratch("verified");
    catalog_fixture(&root);
    let outcome = radius_of(&root, "component:default/checkout", false);
    let radius = radius_json(&outcome);

    // The join that used to be lost: a bare `owner: team-x` is a **group**, and it is the
    // same group reference the team's other components name, so co-ownership is a fact.
    assert_eq!(
        radius["owners"],
        serde_json::json!(["group:default/team-x"])
    );
    assert_eq!(radius["owner_is_team"], serde_json::json!(true));
    assert_eq!(
        radius["co_owned_components"],
        serde_json::json!(["component:default/cart", "component:default/web"])
    );
    // A bare `system: checkout-system` is a **system**, and it is the same reference the
    // System entity carries — so siblings and the domain are reachable at all.
    assert_eq!(
        radius["system"],
        serde_json::json!("system:default/checkout-system")
    );
    assert_eq!(radius["domain"], serde_json::json!("domain:default/shop"));
    assert_eq!(
        radius["siblings"],
        serde_json::json!(["component:default/cart", "component:default/web"])
    );
    // An entry of `providesApis` is an **API**.
    assert_eq!(
        radius["provided_apis"],
        serde_json::json!(["api:default/checkout-api"])
    );
    assert_eq!(
        radius["consumed_apis"],
        serde_json::json!(["api:default/payments-api"])
    );
    assert_eq!(
        radius["dependents"],
        serde_json::json!(["component:default/web"])
    );
    assert_eq!(radius["unresolved"], serde_json::json!([]));
    assert_eq!(radius["coverage"], serde_json::json!("verified"));
    assert_eq!(outcome.exit_code, PASS, "{}", outcome.text);
    assert!(outcome.text.contains("blast radius"), "{}", outcome.text);
}

#[test]
fn gate_the_output_always_has_both_halves_so_a_radius_can_be_checked() {
    // The radius is a *view* of the entities the traversal walked. Emitting it alone would
    // leave every answer unverifiable, so the catalog it came from is a sibling field — on
    // every path, including the one where nothing was found.
    let root = scratch("both-halves");
    catalog_fixture(&root);
    for reference in ["component:default/checkout", "component:default/ghost"] {
        let document: serde_json::Value =
            serde_json::from_str(&radius_of(&root, reference, false).json).expect("valid JSON");
        let object = document.as_object().expect("an object");
        assert_eq!(
            object.keys().collect::<Vec<_>>(),
            vec!["blast_radius", "catalog"],
            "for {reference}"
        );
        assert!(
            document["catalog"]["entities"].is_array(),
            "the entities the traversal walked are in the output for {reference}"
        );
    }
}

#[test]
fn gate_an_absent_component_is_unknown_and_exits_four() {
    let root = scratch("absent");
    catalog_fixture(&root);
    let outcome = radius_of(&root, "component:default/ghost", false);
    let radius = radius_json(&outcome);
    assert_eq!(radius["found"], serde_json::json!(false));
    assert_eq!(
        radius["coverage"],
        serde_json::json!("unknown"),
        "an absent component is not a component with no owners"
    );
    assert_eq!(outcome.exit_code, UNSUPPORTED);
    assert!(
        outcome.text.contains("not in the catalog"),
        "the text must say what is actually known: {}",
        outcome.text
    );
    assert_eq!(
        radius_of(&root, "component:default/ghost", true).exit_code,
        UNSUPPORTED,
        "strictness cannot make an absent component actionable"
    );
}

#[test]
fn gate_a_modelled_kind_that_is_not_a_component_is_unsupported_not_empty() {
    // A System has no organisational blast radius in this traversal. Reporting an empty
    // radius would read as "this system touches nothing", which is a different and wrong
    // statement.
    let root = scratch("not-a-component");
    catalog_fixture(&root);
    let outcome = radius_of(&root, "system:default/checkout-system", false);
    let radius = radius_json(&outcome);
    assert_eq!(radius["found"], serde_json::json!(true));
    assert_eq!(radius["coverage"], serde_json::json!("unsupported"));
    assert_eq!(outcome.exit_code, UNSUPPORTED);
    assert!(
        outcome
            .text
            .contains("blast radius is defined for components"),
        "{}",
        outcome.text
    );
}

#[test]
fn gate_a_reference_that_is_not_a_reference_is_refused_not_guessed() {
    let root = scratch("bad-ref");
    catalog_fixture(&root);
    let cli = cli_for(&root).expect("parses");
    let error = extensions::blast_radius(&cli, "a/b/c", None, None, false)
        .expect_err("three segments is not a reference");
    let message = error.to_string();
    assert!(message.contains("a/b/c"), "{message}");
    assert!(
        message.contains("reference"),
        "the refusal must name what was wrong: {message}"
    );
}

#[test]
fn gate_a_bare_name_is_read_as_a_component_at_the_entry_point() {
    // The implied kind is a property of the field: at the CLI's entry point the field is
    // "which component", so a bare name is a component. Everywhere else it is not.
    let root = scratch("bare-entry");
    catalog_fixture(&root);
    let radius = radius_json(&radius_of(&root, "checkout", false));
    assert_eq!(
        radius["component"],
        serde_json::json!("component:default/checkout")
    );
    assert_eq!(radius["found"], serde_json::json!(true));
}

#[test]
fn gate_the_radius_output_is_byte_identical_across_ten_runs() {
    // Every list is sorted and deduplicated and the digest is over canonical JSON, so two
    // runs of one scan are comparable by eye and by hash (`NFR-001`, `FR-028`).
    let root = scratch("determinism");
    catalog_fixture(&root);
    let first = radius_of(&root, "component:default/checkout", false);
    for run in 0..10 {
        let again = radius_of(&root, "component:default/checkout", false);
        assert_eq!(first.json, again.json, "run {run} differed");
        assert_eq!(first.exit_code, again.exit_code, "run {run} differed");
    }
}

#[test]
fn gate_the_schema_declares_exactly_what_the_engine_emits() {
    // A schema that lags a struct is worse than no schema: it validates a shape the engine
    // no longer emits. The check is set equality, not containment, in both directions.
    let root = scratch("schema");
    catalog_fixture(&root);
    let radius = radius_json(&radius_of(&root, "component:default/checkout", false));
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/blast-radius.schema.json");
    let schema: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("schema")).expect("valid JSON");

    let mut emitted: Vec<String> = radius
        .as_object()
        .expect("object")
        .keys()
        .cloned()
        .collect();
    emitted.sort();
    let mut declared: Vec<String> = schema["$defs"]["blastRadius"]["properties"]
        .as_object()
        .expect("properties")
        .keys()
        .cloned()
        .collect();
    declared.sort();
    assert_eq!(declared, emitted, "schema properties vs emitted fields");

    let mut required: Vec<String> = schema["$defs"]["blastRadius"]["required"]
        .as_array()
        .expect("required")
        .iter()
        .map(|value| value.as_str().expect("string").to_string())
        .collect();
    required.sort();
    assert_eq!(
        required, emitted,
        "every field is always emitted — `null` is a statement, absence would not be"
    );
    assert_eq!(
        schema["additionalProperties"],
        serde_json::json!(false),
        "the output document is closed"
    );
    assert_eq!(
        schema["$defs"]["blastRadius"]["additionalProperties"],
        serde_json::json!(false),
        "the radius must not accept undeclared fields"
    );
    assert_eq!(
        schema["required"],
        serde_json::json!(["catalog", "blast_radius"]),
        "both halves are always present"
    );
}

#[test]
fn gate_the_commands_radius_matches_the_libraries() {
    // The CLI is a caller, not a second implementation: the traversal through the command
    // must equal the traversal through the library on the same tree.
    let root = scratch("library-parity");
    catalog_fixture(&root);
    let report = autopsy_catalog::scan(&root, &autopsy_catalog::DiscoveryOptions::new())
        .expect("scan succeeds");
    let expected = report
        .ownership
        .blast_radius(&report.entities, "component:default/checkout")
        .expect("computes");
    let outcome = radius_of(&root, "component:default/checkout", false);
    let actual: autopsy_catalog::OrgBlastRadius =
        serde_json::from_value(radius_json(&outcome)).expect("round-trips");
    assert_eq!(actual, expected);
}
