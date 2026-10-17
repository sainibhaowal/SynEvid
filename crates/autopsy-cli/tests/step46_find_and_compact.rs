//! Tier-1 agent-experience gate tests: `autopsy find` (T1.1) and the compact
//! projection (T1.5).
//!
//! Both surfaces are pure functions over a symbol population / a report value, which
//! is why the matching and slicing contracts here are asserted on fixtures rather than
//! on the live repository: a fixture cannot drift the way history does, and an agent
//! contract that drifted would be worse than one asserted exactly.
//!
//! Exit codes are restated rather than imported, for the same reason the other gate
//! tests restate them — a test that reads the constant it is checking cannot catch a
//! constant that changed.

use autopsy_cli::extensions::{self, CompactArgs, Outcome};
use autopsy_cli::{EXIT_ANALYSIS_ERROR, EXIT_PASS, find_symbols, map_projection, run_cli};
use autopsy_domain::{CoverageState, Edge, EdgeKind, SourceLocation, Symbol, SymbolId, Visibility};

// ---------------------------------------------------------------------------
// fixtures
// ---------------------------------------------------------------------------

/// A synthetic symbol. Ids are chosen so ordering (not alphabetical position in the
/// fixture list) decides what `find` returns first.
fn symbol(id: &str, qname: &str, kind: &str, path: &str) -> Symbol {
    Symbol {
        stable_id: SymbolId::new(id),
        language_id: "typescript".to_string(),
        kind: kind.to_string(),
        qualified_name: qname.to_string(),
        range: Some(SourceLocation {
            path: path.to_string(),
            start_line: 1,
            end_line: 2,
            start_col: Some(0),
            end_col: Some(1),
            symbol_id: None,
        }),
        normalized_signature: Some(format!("function {qname}()")),
        visibility: Visibility::Public,
    }
}

fn fixture() -> Vec<Symbol> {
    vec![
        symbol(
            "sym_c",
            "app.checkout.ChargeService",
            "class",
            "src/checkout/charge.ts",
        ),
        symbol(
            "sym_a",
            "app.users.UserService",
            "class",
            "src/users/service.ts",
        ),
        symbol(
            "sym_b",
            "app.users.findUser",
            "function",
            "src/users/find.ts",
        ),
        // Private and test-kind so kind and visibility filters have something to refuse.
        symbol(
            "sym_d",
            "app.users.UserSecret",
            "variable",
            "src/users/secret.ts",
        ),
    ]
}

// ---------------------------------------------------------------------------
// find: the agent-facing resolution contract (T1.1)
// ---------------------------------------------------------------------------

#[test]
fn find_matches_qualified_name_case_insensitively() {
    let report =
        find_symbols(&fixture(), "userservice", None, false, 50, 0).expect("match on qname");
    assert_eq!(report.total, 1);
    assert_eq!(report.symbols[0].stable_id, "sym_a");
    // The report carries the query it was asked, so a caller can relate hit to ask.
    assert_eq!(report.pattern, "userservice");
}

#[test]
fn find_matches_declaring_path() {
    let report =
        find_symbols(&fixture(), "src/users/find", None, false, 50, 0).expect("match on path");
    assert_eq!(report.total, 1);
    assert_eq!(report.symbols[0].qualified_name, "app.users.findUser");
}

#[test]
fn find_matches_stable_id_unless_name_only() {
    let with_id = find_symbols(&fixture(), "sym_b", None, false, 50, 0)
        .expect("stable ids are matchable by default");
    assert_eq!(with_id.total, 1);

    let name_only = find_symbols(&fixture(), "sym_b", None, true, 50, 0)
        .expect("name-only is a narrowing, not an error");
    assert_eq!(
        name_only.total, 0,
        "--name-only means the caller narrowed what counts as a match: a stable id the \
         caller did not type must not widen what it asked for"
    );
}

#[test]
fn find_respects_kind_filter() {
    let report = find_symbols(&fixture(), "user", Some("class"), false, 50, 0)
        .expect("kind filter on the user family");
    assert_eq!(
        report.total, 1,
        "of the `user` family only UserService is a class: findUser is a function and \
         UserSecret a variable, and the kind filter must refuse both"
    );
    assert_eq!(report.symbols[0].kind, "class");

    let none = find_symbols(&fixture(), "user", Some("interface"), false, 50, 0)
        .expect("a kind with no hits is an answer, not an error");
    assert_eq!(
        none.total, 0,
        "no kind match is an honest empty answer with the total it searched"
    );
}

#[test]
fn find_order_is_stable_id_then_qname_and_pagination_is_honest() {
    // `user` hits three fixtures (a class, a function, a variable); ids sym_a, sym_b,
    // sym_d encode the expected stable-id order regardless of fixture list position.
    let report = find_symbols(&fixture(), "user", None, false, 2, 0).expect("first page");
    assert_eq!(report.total, 3, "total is the population, not the page");
    assert_eq!(report.limit, 2);
    let ids_first: Vec<&str> = report
        .symbols
        .iter()
        .map(|s| s.stable_id.as_str())
        .collect();
    assert_eq!(ids_first, ["sym_a", "sym_b"]);

    let second = find_symbols(&fixture(), "user", None, false, 2, 2).expect("second page");
    let ids_second: Vec<&str> = second
        .symbols
        .iter()
        .map(|s| s.stable_id.as_str())
        .collect();
    assert_eq!(ids_second, ["sym_d"]);
    assert_eq!(
        second.offset, 2,
        "the report must name the page it is showing"
    );
}

#[test]
fn find_is_deterministic_across_repeated_calls() {
    let first = find_symbols(&fixture(), "user", None, false, 50, 0).expect("first run");
    let second = find_symbols(&fixture(), "user", None, false, 50, 0).expect("second run");
    assert_eq!(
        serde_json::to_string(&first).expect("first serialize"),
        serde_json::to_string(&second).expect("second serialize"),
        "same population + same query must produce byte-identical output"
    );
}

#[test]
fn find_refuses_an_empty_pattern() {
    let err = find_symbols(&fixture(), "   ", None, false, 50, 0)
        .expect_err("a blank pattern is a caller bug, not match-everything");
    assert!(
        err.to_string().contains("pattern is empty"),
        "the refusal must name the cause: {err}"
    );

    // And through the real CLI surface: exit 3 (analysis error), never a silent empty list.
    let code = run_cli(&[
        "autopsy".to_string(),
        "--repo-root".to_string(),
        std::env::temp_dir().display().to_string(),
        "find".to_string(),
        " ".to_string(),
    ]);
    assert_eq!(code, EXIT_ANALYSIS_ERROR);
}

#[test]
fn find_limit_is_capped_at_the_browse_ceiling() {
    // 1001 would be a dump; the ceiling keeps `find` a lookup. The cap is asserted
    // structurally: the resolved limit in the report, not a panic or a truncation.
    let report = find_symbols(&fixture(), "user", None, false, 5000, 0).expect("oversized ask");
    assert_eq!(
        report.limit, 1000,
        "the ceiling applies no matter how large the caller's page request is"
    );
}

// ---------------------------------------------------------------------------
// compact projection: the token-budget contract (T1.5)
// ---------------------------------------------------------------------------

fn sample_report(total: u32) -> serde_json::Value {
    serde_json::json!({
        "snapshot_id": "snap-1",
        "coverage": "verified",
        "population": total,
        "facts": (0..total).map(|i| serde_json::json!({"i": i})).collect::<Vec<_>>(),
        "notes": ["one", "two"],
    })
}

#[test]
fn compact_slices_large_arrays_and_names_the_residual() {
    let out = extensions::compact_projection(&sample_report(45), 20);
    let facts = out.get("facts").and_then(|v| v.as_array()).expect("sliced");
    assert_eq!(facts.len(), 20);
    assert_eq!(
        facts[0].get("i").and_then(|v| v.as_u64()),
        Some(0),
        "the slice keeps the engine's own canonical order, from the front"
    );
    assert_eq!(
        out.get("facts_omitted").and_then(|v| v.as_u64()),
        Some(25),
        "the residual must be stated, never silent"
    );
    assert_eq!(
        out.get("compact").and_then(|v| v.as_bool()),
        Some(true),
        "a consumer must be able to tell a compacted view from a full one"
    );
}

#[test]
fn compact_keeps_scalars_and_small_arrays_exact() {
    let out = extensions::compact_projection(&sample_report(10), 20);
    assert_eq!(
        out.get("population").and_then(|v| v.as_u64()),
        Some(10),
        "counts stay exact: a compact view that rounded its own totals would be a report lying about itself"
    );
    assert_eq!(
        out.get("coverage").and_then(|v| v.as_str()),
        Some("verified"),
        "coverage must survive compaction unchanged: unknown before compaction cannot read verified after"
    );
    assert_eq!(
        out.get("notes").and_then(|v| v.as_array()).map(|a| a.len()),
        Some(2),
        "an array at or under the top-k passes through untouched"
    );
    assert!(
        out.get("notes_omitted").is_none(),
        "no residual key for an array that was not sliced"
    );
}

#[test]
fn compact_wraps_a_bare_top_level_array() {
    let bare = serde_json::json!([1, 2, 3, 4, 5]);
    let out = extensions::compact_projection(&bare, 2);
    assert_eq!(
        out.get("items")
            .and_then(|v| v.as_array())
            .map(|a| a.as_slice()),
        Some(&[serde_json::json!(1), serde_json::json!(2)][..]),
    );
    assert_eq!(out.get("items_omitted").and_then(|v| v.as_u64()), Some(3));
}

#[test]
fn compact_top_k_resolves_default_and_is_clamped() {
    // Default when unnamed: 20.
    let out = extensions::compact_projection(&sample_report(30), extensions::resolve_top_k(None));
    assert_eq!(
        out.get("facts").and_then(|v| v.as_array()).map(|a| a.len()),
        Some(20)
    );
    // Clamp: 0 and 10_000 both resolve inside the browse ceiling rather than disabling
    // or exploding the slice.
    assert_eq!(extensions::resolve_top_k(Some(0)), 1);
    assert_eq!(extensions::resolve_top_k(Some(10_000)), 1000);
    assert_eq!(extensions::resolve_top_k(Some(7)), 7);
}

#[test]
fn apply_compact_is_a_noop_when_not_requested() {
    let outcome = Outcome {
        exit_code: 4,
        json: serde_json::to_string_pretty(&sample_report(45)).expect("report"),
        text: "text summary".to_string(),
    };
    let out = extensions::apply_compact(
        outcome,
        &CompactArgs {
            compact: false,
            top_k: None,
        },
    );
    assert!(
        out.json.contains("\"i\": 44"),
        "unrequested compaction must leave the full report"
    );
    assert!(
        !out.json.contains("compact"),
        "no compaction marker on a full report"
    );
    assert_eq!(
        out.exit_code, 4,
        "compaction must never alter the exit contract"
    );
    assert_eq!(out.text, "text summary");
}

#[test]
fn apply_compact_preserves_exit_code_and_text_and_slices_json() {
    let outcome = Outcome {
        exit_code: 2,
        json: serde_json::to_string_pretty(&sample_report(45)).expect("report"),
        text: "text summary".to_string(),
    };
    let out = extensions::apply_compact(
        outcome,
        &CompactArgs {
            compact: true,
            top_k: Some(10),
        },
    );
    let value: serde_json::Value = serde_json::from_str(&out.json).expect("compact json");
    assert_eq!(
        value
            .get("facts")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(10)
    );
    assert_eq!(
        value.get("facts_omitted").and_then(|v| v.as_u64()),
        Some(35)
    );
    // Exit code 2 is the engine's policy verdict; a presentation slice cannot flip it.
    assert_eq!(out.exit_code, 2);
    assert_eq!(
        out.text, "text summary",
        "the prose keeps speaking in full counts"
    );
}

#[test]
fn apply_compact_hands_back_a_non_json_report_unchanged() {
    let outcome = Outcome {
        exit_code: 0,
        json: "not json at all".to_string(),
        text: String::new(),
    };
    let out = extensions::apply_compact(
        outcome,
        &CompactArgs {
            compact: true,
            top_k: None,
        },
    );
    assert_eq!(
        out.json, "not json at all",
        "compaction must never stand between an agent and evidence it already produced"
    );
    assert_eq!(out.exit_code, 0);
}

#[test]
fn compact_projection_is_deterministic() {
    let one = extensions::compact_projection(&sample_report(45), 20);
    let two = extensions::compact_projection(&sample_report(45), 20);
    assert_eq!(
        serde_json::to_string(&one).expect("one"),
        serde_json::to_string(&two).expect("two"),
        "same report + same top-k must be byte-identical"
    );
}

// ---------------------------------------------------------------------------
// map: the bounded whole-of-graph projection (T1.3)
// ---------------------------------------------------------------------------

/// An edge fixture; kind is deterministic by construction.
fn edge(source: &str, target: &str) -> Edge {
    Edge {
        source: SymbolId::new(source),
        target: SymbolId::new(target),
        kind: EdgeKind::Calls,
        coverage: CoverageState::Verified,
        location: None,
        provenance: "fixture".to_string(),
    }
}

fn graph_fixture() -> (Vec<Symbol>, Vec<Edge>) {
    let symbols = vec![
        symbol(
            "sym_hub",
            "hub.dispatch.Fanout",
            "class",
            "src/hub/fanout.ts",
        ),
        symbol(
            "sym_left",
            "hub.left.Process",
            "function",
            "src/hub/left.ts",
        ),
        symbol(
            "sym_right",
            "hub.right.Process",
            "function",
            "src/hub/right.ts",
        ),
        symbol(
            "sym_user",
            "users.findUser",
            "function",
            "src/users/find.ts",
        ),
    ];
    let edges = vec![
        edge("sym_hub", "sym_left"),
        edge("sym_hub", "sym_right"),
        edge("sym_left", "sym_right"),
        edge("sym_user", "sym_hub"),
    ];
    (symbols, edges)
}

#[test]
fn map_counts_kinds_packages_and_edges_in_key_order() {
    let (symbols, edges) = graph_fixture();
    let report = map_projection(&symbols, &edges, "snap-1", 25, None);

    assert_eq!(report.symbols_total, 4);
    assert_eq!(report.edges_total, 4);
    assert_eq!(report.snapshot_id, "snap-1");

    // BTreeMap key order is the serialization order; assert the shape and the counts.
    let kinds: Vec<(&String, &usize)> = report.kind_counts.iter().collect();
    assert_eq!(
        kinds,
        vec![(&"class".to_string(), &1), (&"function".to_string(), &3)]
    );

    let packages: Vec<(&String, &usize)> = report.package_counts.iter().collect();
    assert_eq!(
        packages,
        vec![(&"src/hub".to_string(), &3), (&"src/users".to_string(), &1)],
        "package keys are the first two path segments"
    );
}

#[test]
fn map_ranks_by_degree_descending_with_stable_id_tiebreak() {
    let (symbols, edges) = graph_fixture();
    let report = map_projection(&symbols, &edges, "snap-1", 25, None);

    // sym_hub: out 2 (left, right) + in 1 (user) = 3; sym_left: in 1 (hub) + out 1
    // (right) = 2; sym_right: in 2 (hub, left); sym_user: out 1. The left/right tie is
    // broken by stable id.
    let degrees: Vec<(String, usize)> = report
        .top_symbols
        .iter()
        .map(|n| (n.stable_id.clone(), n.degree))
        .collect();
    assert_eq!(
        degrees,
        vec![
            ("sym_hub".to_string(), 3),
            ("sym_left".to_string(), 2),
            ("sym_right".to_string(), 2),
            ("sym_user".to_string(), 1),
        ]
    );
    let fan_hub = &report.top_symbols[0];
    assert_eq!((fan_hub.fan_in, fan_hub.fan_out), (1, 2));
}

#[test]
fn map_scope_narrows_every_count_and_rank() {
    let (symbols, edges) = graph_fixture();
    let report = map_projection(&symbols, &edges, "snap-1", 25, Some("src/users"));

    assert_eq!(report.symbols_total, 1);
    assert_eq!(
        report.edges_total, 0,
        "an edge both of whose endpoints leave the scope is out of the scope: ranking a \
         scoped map by its edges to the outside would put the excluded glue code first"
    );
    assert_eq!(
        report.under.as_deref(),
        Some("src/users"),
        "a scoped map must state its scope"
    );
    assert_eq!(report.top_symbols.len(), 1);
    assert_eq!(report.top_symbols[0].stable_id, "sym_user");
}

#[test]
fn map_top_is_bounded_and_clamped() {
    let (symbols, edges) = graph_fixture();
    let zero = map_projection(&symbols, &edges, "snap-1", 0, None);
    assert_eq!(
        zero.top_symbols.len(),
        1,
        "top=0 clamps to 1: an empty map is a failed map"
    );
    let huge = map_projection(&symbols, &edges, "snap-1", 10_000, None);
    assert_eq!(
        huge.top_symbols.len(),
        4,
        "top above the population returns every node, capped by the 500 ceiling"
    );
}

#[test]
fn map_is_deterministic_across_repeated_calls() {
    let (symbols, edges) = graph_fixture();
    let one = map_projection(&symbols, &edges, "snap-1", 25, None);
    let two = map_projection(&symbols, &edges, "snap-1", 25, None);
    assert_eq!(
        serde_json::to_string(&one).expect("one"),
        serde_json::to_string(&two).expect("two"),
        "same graph + same window must be byte-identical"
    );
}

// ---------------------------------------------------------------------------
// end-to-end: the flags exist and the commands still pass on the live repository
// ---------------------------------------------------------------------------

#[test]
fn evidence_commands_accept_compact_and_top_k_and_pass() {
    let Some(root) = repository_under_test() else {
        return;
    };
    // One command is enough to prove the flag wiring and the pass-through contract;
    // the projection beneath it is covered exhaustively above.
    let code = run_cli(&[
        "autopsy".to_string(),
        "--repo-root".to_string(),
        root.display().to_string(),
        "--format".to_string(),
        "json".to_string(),
        "catalog".to_string(),
        "--compact".to_string(),
        "--top-k".to_string(),
        "5".to_string(),
    ]);
    assert_eq!(code, EXIT_PASS);
}

/// The repository under test, as in the other gate tests: only meaningful when the
/// checkout is a real git repository.
fn repository_under_test() -> Option<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    if root.join(".git").exists() {
        Some(root)
    } else {
        eprintln!(
            "skipping: {} has no `.git`, so there is no live surface to exercise",
            root.display()
        );
        None
    }
}
