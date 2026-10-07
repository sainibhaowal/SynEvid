use autopsy_adapter_api::{LanguageAdapter, ParseContext};
use autopsy_adapter_typescript::TypeScriptAdapter;
use autopsy_diff::{DiffEngine, DiffInput, FileDelta};
use autopsy_domain::{Contract, EdgeKind, FileUnit, SnapshotId, Visibility};
use autopsy_graph::DependencyGraph;
use autopsy_tests::TestSandbox;
use std::collections::BTreeMap;

#[test]
fn test_e2e_dependency_multigraph_cycles_and_reachability() {
    let sandbox = TestSandbox::new("e2e_graph_reachability");
    sandbox.write_autopsy_toml(&toml::to_string(&sandbox.default_config()).unwrap());

    // File A: imports B, defines ServiceA which calls ServiceB
    sandbox.write_file(
        "src/serviceA.ts",
        r#"
import { ServiceB } from "./serviceB";

export class ServiceA {
    private b: ServiceB;
    constructor() {
        this.b = new ServiceB();
    }
    public execute(): void {
        this.b.process();
    }
}
"#,
    );

    // File B: imports A (cyclic dependency), defines ServiceB
    sandbox.write_file(
        "src/serviceB.ts",
        r#"
import { ServiceA } from "./serviceA";

export class ServiceB {
    public process(): void {
        console.log("Processing");
    }
    public callback(a: ServiceA): void {
        a.execute();
    }
}
"#,
    );

    let config = sandbox.default_config();
    let snapshot = autopsy_repo::scan_repository(&sandbox.root, &config, "0.0.1", None)
        .expect("Scan must succeed");

    let adapter = TypeScriptAdapter::new();
    let ctx = ParseContext::new(&sandbox.root);

    let mut graph = DependencyGraph::new();

    for (rel_path, unit) in &snapshot.files {
        let abs_path = sandbox.root.join(rel_path);
        let content = std::fs::read_to_string(&abs_path).unwrap();
        let parsed = adapter.parse(&ctx, unit, &content).unwrap();
        let symbols = adapter.extract_symbols(&parsed).unwrap();

        let mut sym_index = BTreeMap::new();
        for s in &symbols {
            sym_index.insert(s.qualified_name.clone(), s.stable_id.clone());
        }
        let edges = adapter.extract_edges(&parsed, &sym_index).unwrap();

        for s in symbols {
            graph.add_node(s);
        }
        for e in edges {
            graph.add_edge(e);
        }
    }

    assert!(graph.node_count() >= 2);
    assert!(graph.edge_count() >= 2);

    // Verify symbol lookup
    let sym_a = graph
        .all_symbols_sorted()
        .into_iter()
        .find(|s| s.qualified_name.contains("ServiceA"));
    assert!(sym_a.is_some(), "Must index ServiceA");

    // Verify multigraph edge extraction with provenance
    let all_edges = graph.all_edges_sorted();
    assert!(all_edges.iter().any(|e| e.kind == EdgeKind::Imports));
    for edge in &all_edges {
        assert!(
            !edge.provenance.is_empty(),
            "Edge must retain extraction provenance"
        );
    }

    // Verify bounded forward reachability
    let a_id = sym_a.unwrap().stable_id.clone();
    let reach = graph.bounded_forward_reachability(&a_id, 2);
    assert!(reach.contains_key(&a_id));

    // Verify deterministic graph digest
    let digest1 = graph.compute_digest();
    let digest2 = graph.compute_digest();
    assert_eq!(digest1, digest2);
    assert_eq!(digest1.len(), 64);
}

#[test]
fn test_e2e_semantic_diff_unambiguous_and_ambiguous_renames() {
    let mut before_files = BTreeMap::new();
    let mut after_files = BTreeMap::new();

    // 1. Unambiguous rename: src/old_math.ts -> src/calculator.ts (same hash)
    let math_hash = blake3::hash(b"export const add = (a: number, b: number) => a + b;")
        .to_hex()
        .to_string();
    before_files.insert(
        "src/old_math.ts".to_string(),
        FileUnit {
            path: "src/old_math.ts".to_string(),
            language: "typescript".to_string(),
            content_hash: math_hash.clone(),
            is_generated: false,
        },
    );
    after_files.insert(
        "src/calculator.ts".to_string(),
        FileUnit {
            path: "src/calculator.ts".to_string(),
            language: "typescript".to_string(),
            content_hash: math_hash,
            is_generated: false,
        },
    );

    // 2. Ambiguous rename (FR-007): Two deleted files share same hash, one added file matches
    let dup_hash = blake3::hash(b"export const DEFAULT_CONFIG = {};")
        .to_hex()
        .to_string();
    before_files.insert(
        "src/dup_a.ts".to_string(),
        FileUnit {
            path: "src/dup_a.ts".to_string(),
            language: "typescript".to_string(),
            content_hash: dup_hash.clone(),
            is_generated: false,
        },
    );
    before_files.insert(
        "src/dup_b.ts".to_string(),
        FileUnit {
            path: "src/dup_b.ts".to_string(),
            language: "typescript".to_string(),
            content_hash: dup_hash.clone(),
            is_generated: false,
        },
    );
    after_files.insert(
        "src/dup_new.ts".to_string(),
        FileUnit {
            path: "src/dup_new.ts".to_string(),
            language: "typescript".to_string(),
            content_hash: dup_hash,
            is_generated: false,
        },
    );

    let g_empty = DependencyGraph::new();
    let diff = DiffEngine::compute_diff(DiffInput {
        before_snapshot_id: SnapshotId::new("snap_before"),
        after_snapshot_id: SnapshotId::new("snap_after"),
        before_files: &before_files,
        after_files: &after_files,
        before_graph: &g_empty,
        after_graph: &g_empty,
        before_contracts: &[],
        after_contracts: &[],
    });

    // Verify unambiguous rename detected
    let renamed = diff
        .file_deltas
        .iter()
        .find(|d| matches!(d, FileDelta::Renamed { .. }));
    assert!(renamed.is_some(), "Must detect unambiguous file rename");
    if let Some(FileDelta::Renamed {
        old_path, new_path, ..
    }) = renamed
    {
        assert_eq!(old_path, "src/old_math.ts");
        assert_eq!(new_path, "src/calculator.ts");
    }

    // Verify ambiguous renames remained Added and Deleted
    let added_count = diff
        .file_deltas
        .iter()
        .filter(|d| matches!(d, FileDelta::Added { path, .. } if path == "src/dup_new.ts"))
        .count();
    let deleted_count = diff
        .file_deltas
        .iter()
        .filter(|d| matches!(d, FileDelta::Deleted { path, .. } if path.starts_with("src/dup_")))
        .count();

    assert_eq!(added_count, 1, "Ambiguous added file must stay Added");
    assert_eq!(
        deleted_count, 2,
        "Ambiguous deleted files must stay Deleted"
    );
}

#[test]
fn test_e2e_contract_breaking_and_frontier_impact() {
    let mut before_g = DependencyGraph::new();
    let mut after_g = DependencyGraph::new();

    let sym_api_id = autopsy_domain::SymbolId::new("src/api.ts#fn:fetchData");
    let sym_client_id = autopsy_domain::SymbolId::new("src/client.ts#class:Client::load");

    let api_sym_before = autopsy_domain::Symbol {
        stable_id: sym_api_id.clone(),
        language_id: "typescript".to_string(),
        kind: "function".to_string(),
        qualified_name: "src/api.ts::fetchData".to_string(),
        range: None,
        normalized_signature: Some("fn(url: string) -> Response".to_string()),
        visibility: Visibility::Public,
    };
    let mut api_sym_after = api_sym_before.clone();
    api_sym_after.visibility = Visibility::Private; // Breaking visibility change

    let client_sym = autopsy_domain::Symbol {
        stable_id: sym_client_id.clone(),
        language_id: "typescript".to_string(),
        kind: "method".to_string(),
        qualified_name: "src/client.ts::Client::load".to_string(),
        range: None,
        normalized_signature: Some("fn() -> void".to_string()),
        visibility: Visibility::Public,
    };

    before_g.add_node(api_sym_before);
    before_g.add_node(client_sym.clone());
    before_g.add_edge(autopsy_domain::Edge {
        source: sym_client_id.clone(),
        target: sym_api_id.clone(),
        kind: EdgeKind::Calls,
        coverage: autopsy_domain::CoverageState::Verified,
        location: None,
        provenance: "call_expression".to_string(),
    });

    after_g.add_node(api_sym_after);
    after_g.add_node(client_sym);

    let before_contracts = vec![Contract {
        owner: sym_api_id.clone(),
        visibility: Visibility::Public,
        inputs: vec!["url: string".to_string()],
        output: Some("Response".to_string()),
        effects: vec![],
    }];

    let after_contracts = vec![Contract {
        owner: sym_api_id.clone(),
        visibility: Visibility::Private,
        inputs: vec!["url: string".to_string()],
        output: Some("Response".to_string()),
        effects: vec![],
    }];

    let diff = DiffEngine::compute_diff(DiffInput {
        before_snapshot_id: SnapshotId::new("snap_1"),
        after_snapshot_id: SnapshotId::new("snap_2"),
        before_files: &BTreeMap::new(),
        after_files: &BTreeMap::new(),
        before_graph: &before_g,
        after_graph: &after_g,
        before_contracts: &before_contracts,
        after_contracts: &after_contracts,
    });

    // 1. Verify contract breaking change
    assert_eq!(diff.contract_deltas.len(), 1);
    match &diff.contract_deltas[0] {
        autopsy_diff::ContractDelta::Modified {
            is_breaking,
            breaking_reasons,
            ..
        } => {
            assert!(*is_breaking, "Visibility reduction must be marked breaking");
            assert!(
                breaking_reasons
                    .iter()
                    .any(|r| r.contains("Visibility reduced"))
            );
        }
        _ => panic!("Expected modified contract delta"),
    }

    // 2. Verify changed frontier includes direct symbol and 1-hop dependent boundary
    assert!(diff.frontier.direct_symbols.contains(&sym_api_id));
    assert!(
        diff.frontier.impacted_boundary.contains(&sym_client_id),
        "Client caller must be in impacted boundary"
    );
}

#[test]
fn test_e2e_golden_diff_fixture_exact_json_match() {
    let before_snapshot_id = SnapshotId::new("golden_snap_v1");
    let after_snapshot_id = SnapshotId::new("golden_snap_v2");

    let mut before_files = BTreeMap::new();
    let mut after_files = BTreeMap::new();

    before_files.insert(
        "src/index.ts".to_string(),
        FileUnit {
            path: "src/index.ts".to_string(),
            language: "typescript".to_string(),
            content_hash: "hash_index_v1".to_string(),
            is_generated: false,
        },
    );
    after_files.insert(
        "src/index.ts".to_string(),
        FileUnit {
            path: "src/index.ts".to_string(),
            language: "typescript".to_string(),
            content_hash: "hash_index_v2".to_string(),
            is_generated: false,
        },
    );

    let mut before_g = DependencyGraph::new();
    let mut after_g = DependencyGraph::new();

    let sym_id = autopsy_domain::SymbolId::new("src/index.ts#fn:main");
    let sym_v1 = autopsy_domain::Symbol {
        stable_id: sym_id.clone(),
        language_id: "typescript".to_string(),
        kind: "function".to_string(),
        qualified_name: "src/index.ts::main".to_string(),
        range: None,
        normalized_signature: Some("fn() -> void".to_string()),
        visibility: Visibility::Public,
    };
    let mut sym_v2 = sym_v1.clone();
    sym_v2.normalized_signature = Some("fn(args: string[]) -> void".to_string());

    before_g.add_node(sym_v1);
    after_g.add_node(sym_v2);

    let diff = DiffEngine::compute_diff(DiffInput {
        before_snapshot_id,
        after_snapshot_id,
        before_files: &before_files,
        after_files: &after_files,
        before_graph: &before_g,
        after_graph: &after_g,
        before_contracts: &[],
        after_contracts: &[],
    });

    // Serialize to canonical JSON
    let actual_json = serde_json::to_string_pretty(&diff).expect("Must serialize diff");

    // Frozen golden reference shape assertion
    let parsed: serde_json::Value = serde_json::from_str(&actual_json).unwrap();
    assert_eq!(parsed["before_snapshot_id"], "golden_snap_v1");
    assert_eq!(parsed["after_snapshot_id"], "golden_snap_v2");
    assert_eq!(parsed["summary"]["files_modified"], 1);
    assert_eq!(parsed["summary"]["symbols_modified"], 1);
    assert!(!diff.diff_digest.is_empty());

    // Round-trip deserialization assertion
    let roundtripped: autopsy_diff::SemanticDiff = serde_json::from_str(&actual_json).unwrap();
    assert_eq!(roundtripped, diff);
}

#[test]
fn test_e2e_diff_100_runs_determinism() {
    let before_snapshot_id = SnapshotId::new("det_snap_a");
    let after_snapshot_id = SnapshotId::new("det_snap_b");

    let mut before_files = BTreeMap::new();
    let mut after_files = BTreeMap::new();

    for i in 0..10 {
        let p1 = format!("src/module_{}.ts", i);
        let p2 = format!("src/module_{}.ts", i);
        before_files.insert(
            p1.clone(),
            FileUnit {
                path: p1,
                language: "typescript".to_string(),
                content_hash: format!("hash_before_{}", i),
                is_generated: false,
            },
        );
        after_files.insert(
            p2.clone(),
            FileUnit {
                path: p2,
                language: "typescript".to_string(),
                content_hash: format!("hash_after_{}", i),
                is_generated: false,
            },
        );
    }

    let mut before_g = DependencyGraph::new();
    let mut after_g = DependencyGraph::new();

    for i in 0..10 {
        let sid = autopsy_domain::SymbolId::new(format!("sym::{}", i));
        let sym = autopsy_domain::Symbol {
            stable_id: sid,
            language_id: "typescript".to_string(),
            kind: "function".to_string(),
            qualified_name: format!("mod::fn_{}", i),
            range: None,
            normalized_signature: Some("fn() -> void".to_string()),
            visibility: Visibility::Public,
        };
        before_g.add_node(sym.clone());
        after_g.add_node(sym);
    }

    let first_diff = DiffEngine::compute_diff(DiffInput {
        before_snapshot_id: before_snapshot_id.clone(),
        after_snapshot_id: after_snapshot_id.clone(),
        before_files: &before_files,
        after_files: &after_files,
        before_graph: &before_g,
        after_graph: &after_g,
        before_contracts: &[],
        after_contracts: &[],
    });

    let golden_digest = first_diff.diff_digest.clone();
    let golden_json = serde_json::to_string(&first_diff).unwrap();

    for run in 1..=100 {
        let diff = DiffEngine::compute_diff(DiffInput {
            before_snapshot_id: before_snapshot_id.clone(),
            after_snapshot_id: after_snapshot_id.clone(),
            before_files: &before_files,
            after_files: &after_files,
            before_graph: &before_g,
            after_graph: &after_g,
            before_contracts: &[],
            after_contracts: &[],
        });

        assert_eq!(
            diff.diff_digest, golden_digest,
            "Run #{} violated determinism: diff_digest differs!",
            run
        );

        let json = serde_json::to_string(&diff).unwrap();
        assert_eq!(
            json, golden_json,
            "Run #{} violated determinism: JSON output differs!",
            run
        );
    }
}
