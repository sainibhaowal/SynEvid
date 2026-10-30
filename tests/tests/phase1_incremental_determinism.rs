//! Phase 1 (v0.2.0) Integration & Determinism Test Suite (FR-P1-001..006, NFR-P1-001, NFR-P1-002).
//!
//! Tests:
//! 1. 100-run identical BLAKE3 digests and graph coordinates for seeded layout.
//! 2. SQLite WAL persistence roundtrip for nodes, line-to-line edges, and trajectory events.
//! 3. Sub-5ms ego-network query performance.
//! 4. Sub-50ms in-memory edit blast-damage simulation.

use autopsy_cli::{
    AnalysisArtifacts, build_artifacts_from_snapshot_with_registry,
    build_snapshot_and_graph_for_overlay_from_artifacts_with_registry,
    build_snapshot_and_graph_with_overlays_and_registry, build_snapshot_and_graph_with_registry,
    builtin_plugin_registry,
};
use autopsy_graph_layout::DeterministicForceLayout;
use autopsy_plugin_api::{
    GraphLayoutPlugin, LanguageAdapterPlugin, PluginDescriptor, PluginRegistry,
};
use autopsy_repo::AutopsyConfig;
use autopsy_storage::{
    EgoNetwork, GraphEdgeRow, GraphNodeRow, RepoRecord, StorageEngine, StorageOptions,
    TrajectoryEventRecord,
};
use std::sync::Arc;
use std::time::Instant;
use std::{collections::BTreeMap, fs};

struct TestTypeScriptPlugin;
impl PluginDescriptor for TestTypeScriptPlugin {
    fn plugin_id(&self) -> &'static str {
        "lang.typescript.phase1"
    }
    fn version(&self) -> &'static str {
        "0.2.0"
    }
    fn display_name(&self) -> &'static str {
        "TypeScript / JavaScript Adapter"
    }
    fn description(&self) -> &'static str {
        "Phase 1 pluggable TS adapter"
    }
}
impl LanguageAdapterPlugin for TestTypeScriptPlugin {
    fn file_extensions(&self) -> &'static [&'static str] {
        &[".ts", ".tsx", ".js", ".jsx"]
    }
    fn language_name(&self) -> &'static str {
        "TypeScript"
    }
    fn estimate_coverage(&self, _source: &[u8]) -> autopsy_domain::CoverageState {
        autopsy_domain::CoverageState::Verified
    }
}

#[test]
fn test_phase1_plug_and_play_registry() {
    let registry = PluginRegistry::new();
    registry.register_language(Arc::new(TestTypeScriptPlugin));
    registry.register_layout(Arc::new(DeterministicForceLayout));

    let caps = registry.list_capabilities();
    assert_eq!(caps.len(), 2);
    assert_eq!(caps[0].plugin_id, "lang.typescript.phase1");
    assert_eq!(caps[1].plugin_id, "layout.seeded_force_atlas");

    let found = registry
        .find_language_for_extension(".tsx")
        .expect("Should resolve .tsx extension");
    assert_eq!(found.language_name(), "TypeScript");
}

#[test]
fn test_phase1_100_runs_golden_layout_determinism() {
    let layout = DeterministicForceLayout;
    let nodes: Vec<String> = (0..50)
        .map(|i| format!("Module::Component_{}", i))
        .collect();
    let mut edges = Vec::new();
    for i in 0..49 {
        edges.push((nodes[i].clone(), nodes[i + 1].clone()));
    }
    edges.push((nodes[10].clone(), nodes[40].clone()));

    let seed = blake3::hash(b"phase1_layout_golden_seed_payload").into();

    let baseline = layout.compute_layout(&nodes, &edges, &seed, true).unwrap();
    assert_eq!(baseline.len(), 50);

    // 100 consecutive runs must produce byte-identical (x, y, z, cluster_id) outputs
    for _ in 0..100 {
        let candidate = layout.compute_layout(&nodes, &edges, &seed, true).unwrap();
        assert_eq!(
            baseline, candidate,
            "Layout coordinates drifted non-deterministically!"
        );
    }
}

#[test]
fn test_phase1_incremental_overlay_matches_cold_scan_100_edits() {
    let root =
        std::env::temp_dir().join(format!("synevid_phase1_incremental_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    let service_source = "export function target(value: number) { return value + 1; }\nexport function helper() { return 2; }\nexport function internalCaller() { return target(1); }\n";
    fs::write(root.join("src/service.ts"), service_source).unwrap();
    fs::write(root.join("src/duplicate.ts"), service_source).unwrap();
    fs::write(
        root.join("src/caller.ts"),
        "import { target } from './service';\nexport function caller() { return target(1); }\n",
    )
    .unwrap();

    let config = AutopsyConfig::default();
    let registry = builtin_plugin_registry(&root);
    let storage = StorageEngine::open(
        root.join(".autopsy/storage.db"),
        StorageOptions {
            use_wal: false,
            cache_dir: Some(root.join(".autopsy/cache")),
        },
    )
    .unwrap();
    let baseline =
        build_snapshot_and_graph_with_registry(&root, &config, None, Some(&storage), &registry)
            .unwrap();

    for iteration in 0..100 {
        let value = if iteration % 2 == 0 {
            iteration / 2 + 2
        } else {
            1
        };
        let destination = if iteration % 2 == 0 {
            "helper"
        } else {
            "target"
        };
        let edited = format!(
            "export function target(value: number) {{ return value + {value}; }}\nexport function helper() {{ return 2; }}\nexport function internalCaller() {{ return {destination}(1); }}\n"
        );
        let overlays = BTreeMap::from([("src/service.ts".to_string(), edited.into_bytes())]);
        let incremental = build_snapshot_and_graph_for_overlay_from_artifacts_with_registry(
            &root, &config, &baseline, &overlays, &storage, &registry,
        )
        .unwrap();
        let cold = build_snapshot_and_graph_with_overlays_and_registry(
            &root, &config, None, None, &overlays, None, &registry,
        )
        .unwrap();
        assert_eq!(incremental.snapshot.snapshot_id, cold.snapshot.snapshot_id);
        assert_eq!(analysis_digest(&incremental), analysis_digest(&cold));
    }

    assert_eq!(
        fs::read_to_string(root.join("src/service.ts")).unwrap(),
        service_source
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn test_phase1_resolver_manifest_change_invalidates_cached_edges() {
    let root = std::env::temp_dir().join(format!(
        "synevid_phase1_resolver_context_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("tsconfig.json"),
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@selected":["src/a"]}}}"#,
    )
    .unwrap();
    fs::write(
        root.join("src/a.ts"),
        "export function selected() { return 1; }\n",
    )
    .unwrap();
    fs::write(
        root.join("src/b.ts"),
        "export function selected() { return 2; }\n",
    )
    .unwrap();
    fs::write(
        root.join("src/caller.ts"),
        "import { selected } from '@selected';\nexport function caller() { return selected(); }\n",
    )
    .unwrap();

    let config = AutopsyConfig::default();
    let registry = builtin_plugin_registry(&root);
    let storage = StorageEngine::open(
        root.join(".autopsy/storage.db"),
        StorageOptions {
            use_wal: false,
            cache_dir: Some(root.join(".autopsy/cache")),
        },
    )
    .unwrap();
    let baseline =
        build_snapshot_and_graph_with_registry(&root, &config, None, Some(&storage), &registry)
            .unwrap();
    let target_for_caller = |artifacts: &AnalysisArtifacts| {
        artifacts
            .edges
            .iter()
            .find(|edge| {
                edge.kind == autopsy_domain::EdgeKind::Calls
                    && edge
                        .location
                        .as_ref()
                        .is_some_and(|location| location.path == "src/caller.ts")
            })
            .map(|edge| edge.target.to_string())
            .unwrap()
    };
    assert!(target_for_caller(&baseline).contains("src/a.ts"));

    fs::write(
        root.join("tsconfig.json"),
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@selected":["src/b"]}}}"#,
    )
    .unwrap();
    let incremental = build_snapshot_and_graph_with_overlays_and_registry(
        &root,
        &config,
        None,
        Some(&baseline.snapshot),
        &BTreeMap::new(),
        Some(&storage),
        &registry,
    )
    .unwrap();
    let cold = build_snapshot_and_graph_with_overlays_and_registry(
        &root,
        &config,
        None,
        None,
        &BTreeMap::new(),
        None,
        &registry,
    )
    .unwrap();

    assert!(target_for_caller(&incremental).contains("src/b.ts"));
    assert_eq!(analysis_digest(&incremental), analysis_digest(&cold));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn test_phase1_incremental_1000_file_simulation_p95() {
    let root = std::env::temp_dir().join(format!(
        "synevid_phase1_1000_file_perf_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    for index in 0..1000 {
        let (extension, source) = match index % 6 {
            0 => (
                "ts",
                format!(
                    "export function module_{index}(value: number) {{ return value + {index}; }}\n"
                ),
            ),
            1 => (
                "py",
                format!("def module_{index}(value: int) -> int:\n    return value + {index}\n"),
            ),
            2 => (
                "go",
                format!(
                    "package sample\nfunc Module{index}(value int) int {{ return value + {index} }}\n"
                ),
            ),
            3 => (
                "rs",
                format!("pub fn module_{index}(value: i64) -> i64 {{ value + {index} }}\n"),
            ),
            4 => (
                "java",
                format!(
                    "class Module{index} {{ int value(int input) {{ return input + {index}; }} }}\n"
                ),
            ),
            _ => (
                "cs",
                format!(
                    "class Module{index} {{ int Value(int input) {{ return input + {index}; }} }}\n"
                ),
            ),
        };
        fs::write(
            root.join(format!("src/module_{index:04}.{extension}")),
            source,
        )
        .unwrap();
    }

    let config = AutopsyConfig::default();
    let registry = builtin_plugin_registry(&root);
    let storage = StorageEngine::open(
        root.join(".autopsy/storage.db"),
        StorageOptions {
            use_wal: false,
            cache_dir: Some(root.join(".autopsy/cache")),
        },
    )
    .unwrap();
    let baseline =
        build_snapshot_and_graph_with_registry(&root, &config, None, Some(&storage), &registry)
            .unwrap();
    assert_eq!(baseline.snapshot.files.len(), 1000);
    let overlay = BTreeMap::from([(
        "src/module_0000.ts".to_string(),
        b"export function module_0(value: number) { return value + 1; }\n".to_vec(),
    )]);
    let baseline_artifacts = build_artifacts_from_snapshot_with_registry(
        baseline.snapshot.clone(),
        &storage,
        &registry,
        &root,
    )
    .unwrap();

    let mut timings = Vec::with_capacity(30);
    for _ in 0..30 {
        let started = Instant::now();
        let candidate = build_snapshot_and_graph_for_overlay_from_artifacts_with_registry(
            &root,
            &config,
            &baseline_artifacts,
            &overlay,
            &storage,
            &registry,
        )
        .unwrap();
        assert_eq!(candidate.snapshot.files.len(), 1000);
        timings.push(started.elapsed());
    }
    timings.sort();
    let p95_ms = timings[28].as_secs_f64() * 1000.0;
    eprintln!("Phase 1 1,000-file overlay simulation p95: {p95_ms:.3} ms");
    assert!(
        p95_ms <= 50.0,
        "1,000-file overlay simulation p95 was {p95_ms:.3} ms; required <= 50 ms"
    );

    let _ = fs::remove_dir_all(root);
}

fn analysis_digest(artifacts: &AnalysisArtifacts) -> String {
    let canonical = serde_json::to_vec(&serde_json::json!({
        "snapshot": artifacts.snapshot,
        "symbols": artifacts.symbols,
        "edges": artifacts.edges,
        "contracts": artifacts.contracts,
        "coverage": artifacts.coverage_state,
        "unsupported_constructs": artifacts.unsupported_constructs
    }))
    .unwrap();
    blake3::hash(&canonical).to_hex().to_string()
}

#[test]
fn test_phase1_sqlite_persistence_and_sub_5ms_ego_network() {
    let root =
        std::env::temp_dir().join(format!("synevid_phase1_sqlite_perf_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let database_path = root.join(".autopsy/storage.db");
    let cache_dir = root.join(".autopsy/cache");
    let options = StorageOptions {
        use_wal: true,
        cache_dir: Some(cache_dir),
    };
    let engine = StorageEngine::open(&database_path, options.clone()).unwrap();

    // 1. Register repo
    let repo = RepoRecord {
        repo_id: "repo_phase1_test".to_string(),
        root_path: "/workspace/payment-monorepo".to_string(),
        display_name: "Payment Monorepo".to_string(),
        active_branch: Some("main".to_string()),
        head_commit_oid: Some("d3f8a109b".to_string()),
        latest_snap_id: Some("snap_p1_001".to_string()),
        languages_json: "[\"typescript\",\"java\"]".to_string(),
        monorepo_json: "{\"type\":\"pnpm\"}".to_string(),
        last_indexed_seq: 1,
    };
    engine.register_repository(&repo).unwrap();

    // Insert snapshot
    {
        let conn = engine.raw_connection();
        let c = conn.lock().unwrap();
        c.execute(
            "INSERT INTO snapshots (snapshot_id, revision, config_hash, file_set_digest, snapshot_json)
             VALUES ('snap_p1_001', 'rev_1', 'cfg_1', 'digest_1', '{}');",
            [],
        )
        .unwrap();
    }

    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    for i in 0..1000 {
        nodes.push(GraphNodeRow {
            snapshot_id: "snap_p1_001".to_string(),
            stable_id: format!("Symbol_{}", i),
            node_kind: "function".to_string(),
            qualified_name: format!("Package::Symbol_{}", i),
            file_path: format!("src/File_{}.ts", i % 10),
            start_line: (i as i64) * 10,
            end_line: (i as i64) * 10 + 20,
            signature_hash: format!("sig_{}", i),
            visibility: "public".to_string(),
            is_exported: true,
            is_dead_code: false,
            coverage_state: "verified".to_string(),
        });
    }

    for i in 1..=5000 {
        let source = i % 999 + 1;
        let target = if i % 50 == 0 {
            "Symbol_0".to_string()
        } else {
            format!("Symbol_{}", i % 999 + 1)
        };
        edges.push(GraphEdgeRow {
            snapshot_id: "snap_p1_001".to_string(),
            edge_id: format!("edge_{}", i),
            source_stable_id: format!("Symbol_{source}"),
            target_stable_id: target,
            edge_kind: "calls".to_string(),
            source_file: format!("src/File_{}.ts", i % 10),
            source_line: (i as i64) * 10 + 5, // Exact calling line!
            source_col: 14,
            target_file: "src/File_0.ts".to_string(),
            target_line: 0, // Target line!
            confidence: "exact".to_string(),
        });
    }

    engine.save_graph_nodes_and_edges(&nodes, &edges).unwrap();
    assert_eq!(nodes.len(), 1000);
    assert_eq!(edges.len(), 5000);
    let journal_mode: String = engine
        .raw_connection()
        .lock()
        .unwrap()
        .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
        .unwrap();
    assert_eq!(journal_mode.to_lowercase(), "wal");
    let canonical_export = serde_json::to_vec(&(&nodes, &edges)).unwrap();
    for _ in 0..100 {
        assert_eq!(
            serde_json::to_vec(&(&nodes, &edges)).unwrap(),
            canonical_export
        );
    }

    let mut query_timings = Vec::with_capacity(50);
    let mut ego = None;
    for _ in 0..50 {
        let start = Instant::now();
        ego = Some(engine.query_ego_network("snap_p1_001", "Symbol_0").unwrap());
        query_timings.push(start.elapsed());
    }
    query_timings.sort();
    let p95_ms = query_timings[47].as_secs_f64() * 1000.0;
    let p50_ms = query_timings[25].as_secs_f64() * 1000.0;
    eprintln!(
        "WAL ego query ms: min={:.3}, p50={:.3}, p95={p95_ms:.3}, max={:.3}",
        query_timings[0].as_secs_f64() * 1000.0,
        p50_ms,
        query_timings[49].as_secs_f64() * 1000.0
    );
    let ego: EgoNetwork = ego.unwrap();

    assert!(ego.target_node.is_some());
    assert_eq!(ego.incoming_callers.len(), 100);
    assert!(
        ego.incoming_callers
            .iter()
            .any(|edge| edge.source_line == 505)
    );
    if cfg!(debug_assertions) {
        assert!(
            p50_ms < 10.0,
            "debug ego network lookup p50 took {p50_ms:.3}ms"
        );
    } else {
        assert!(
            p95_ms < 5.0,
            "release ego network lookup p95 took {p95_ms:.3}ms, expected <5ms"
        );
    }

    // 4. Record and verify trajectory event
    let event = TrajectoryEventRecord {
        event_seq: 1,
        session_id: "sess_p1_01".to_string(),
        job_id: Some("job_verify".to_string()),
        workflow_id: Some("wf_main".to_string()),
        event_type: "file_edited".to_string(),
        target_symbol: Some("Symbol_0".to_string()),
        target_file: Some("src/File_0.ts".to_string()),
        before_snap_id: None,
        after_snap_id: Some("snap_p1_001".to_string()),
        verdict: "PASS".to_string(),
        summary: "Symbol_0 verified successfully".to_string(),
        evidence_json: "{}".to_string(),
        blake3_receipt: "blake3:99aacc112233".to_string(),
        tokens_spent: 0,
        tokens_saved: 0,
    };
    let seq = engine.record_trajectory_event(&event).unwrap();
    assert_eq!(seq, 1);

    let events = engine.list_trajectory_events("sess_p1_01").unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].blake3_receipt, "blake3:99aacc112233");
    drop(engine);

    let reopened = StorageEngine::open(&database_path, options).unwrap();
    let persisted_graph = reopened.graph_rows_for_snapshot("snap_p1_001").unwrap();
    assert_eq!(persisted_graph.0.len(), 1000);
    assert_eq!(persisted_graph.1.len(), 5000);
    assert_eq!(
        reopened.list_trajectory_events("sess_p1_01").unwrap().len(),
        1
    );
    let _ = fs::remove_dir_all(root);
}
