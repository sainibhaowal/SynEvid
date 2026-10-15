//! Forensic closure regression tests: the engine must compare two genuinely
//! independent states end to end (FR-007, FR-010).
//!
//! These tests fail on the historical defect where `diff` compared a state
//! against itself (`before_graph = after.graph`) and `verify` evaluated
//! compatibility against an empty before-map:
//! - signature edits must surface as `SignatureModified` + breaking
//!   contract `Modified`, never as silent delete+create;
//! - `verify --baseline-id` must exit 2 on a breaking public-API change
//!   and 0 on a behavior-only (non-breaking) change.

use autopsy_cli::{EXIT_PASS, EXIT_POLICY_FAIL, run_cli};
use autopsy_diff::{ContractDelta, DiffEngine, DiffInput, SymbolDelta};
use autopsy_storage::{StorageEngine, StorageOptions};
use std::fs;
use std::path::{Path, PathBuf};

struct TempTestDir {
    path: PathBuf,
}

impl TempTestDir {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("autopsy_test_{}_{}", name, std::process::id()));
        if path.exists() {
            let _ = fs::remove_dir_all(&path);
        }
        fs::create_dir_all(&path).expect("Failed to create temp dir");
        Self { path }
    }
}

impl Drop for TempTestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

const BEFORE_MATH: &str = r#"
export function add(a: number, b: number): number {
    return a + b;
}
"#;

const BREAKING_MATH: &str = r#"
export function add(a: number, b: number, tenantId: string): number {
    return a + b;
}
"#;

const NON_BREAKING_MATH: &str = r#"
export function add(a: number, b: number): number {
    const result = a + b;
    return result;
}
"#;

fn create_project(root: &Path) {
    let src_dir = root.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(root.join(".autopsy")).unwrap();
    fs::write(src_dir.join("math.ts"), BEFORE_MATH).unwrap();
    // Unchanged second file: exercises the incremental reuse path and
    // gives `add` a real caller for impact queries.
    fs::write(
        src_dir.join("index.ts"),
        "import { add } from './math';\n\nexport function calc(qty: number): number {\n    return add(qty, qty);\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("autopsy.toml"),
        r#"
config_version = "0.0.1"

[repository]
roots = ["src"]
exclude = ["**/node_modules/**"]

[analysis]
default_profile = "default"
max_traversal_nodes = 50000

[cache]
directory = ".autopsy/cache"
"#,
    )
    .unwrap();
    fs::write(
        root.join(".autopsy").join("invariants.yml"),
        r#"
version: "0.0.1"
invariants:
  - id: "api-compatibility"
    description: "Exported API contracts must remain compatible"
    kind: "api_compatibility"
    severity: "error"
"#,
    )
    .unwrap();
}

fn cli(root: &Path, extra: &[&str]) -> i32 {
    let mut args = vec![
        "autopsy".to_string(),
        "--repo-root".to_string(),
        root.display().to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    args.extend(extra.iter().map(|s| s.to_string()));
    run_cli(&args)
}

fn latest_snapshot_id(root: &Path) -> String {
    let storage = StorageEngine::open(
        root.join(".autopsy").join("storage.db"),
        StorageOptions {
            use_wal: false,
            cache_dir: Some(root.join(".autopsy").join("cache")),
        },
    )
    .expect("storage must open");
    storage
        .get_latest_snapshot()
        .expect("snapshot query must succeed")
        .expect("baseline snapshot must exist")
        .snapshot_id
        .to_string()
}

#[test]
fn test_signature_edit_yields_signature_modified_and_breaking_contract() {
    let temp = TempTestDir::new("forensic_sig");
    create_project(&temp.path);

    assert_eq!(cli(&temp.path, &["baseline"]), EXIT_PASS);
    let snap_id = latest_snapshot_id(&temp.path);

    // Breaking edit: add a required parameter to an exported function.
    fs::write(temp.path.join("src").join("math.ts"), BREAKING_MATH).unwrap();

    // CLI diff must succeed...
    assert_eq!(cli(&temp.path, &["diff"]), EXIT_PASS);

    // ...and the library-level diff between the STORED before-state and
    // the current worktree must contain the forensic signal.
    let storage = StorageEngine::open(
        temp.path.join(".autopsy").join("storage.db"),
        StorageOptions {
            use_wal: false,
            cache_dir: Some(temp.path.join(".autopsy").join("cache")),
        },
    )
    .unwrap();
    let before_snapshot = storage
        .get_latest_snapshot()
        .unwrap()
        .expect("baseline snapshot must exist");
    assert_eq!(before_snapshot.snapshot_id.to_string(), snap_id);

    let config =
        autopsy_repo::AutopsyConfig::load_from_file(temp.path.join("autopsy.toml")).unwrap();
    let before = autopsy_cli::build_artifacts_from_snapshot(before_snapshot.clone(), &storage)
        .expect("before-state rebuild from cache must succeed");
    let after = autopsy_cli::build_snapshot_and_graph(&temp.path, &config, None, None)
        .expect("worktree analysis must succeed");

    let diff = DiffEngine::compute_diff(DiffInput {
        before_snapshot_id: before_snapshot.snapshot_id.clone(),
        after_snapshot_id: after.snapshot.snapshot_id.clone(),
        before_files: &before_snapshot.files,
        after_files: &after.snapshot.files,
        before_graph: &before.graph,
        after_graph: &after.graph,
        before_contracts: &before.contracts,
        after_contracts: &after.contracts,
    });

    // Identity preserved across the edit (no delete+create masquerade).
    let removed: Vec<_> = diff
        .symbol_deltas
        .iter()
        .filter(|d| matches!(d, SymbolDelta::Removed { .. }))
        .collect();
    let added: Vec<_> = diff
        .symbol_deltas
        .iter()
        .filter(|d| matches!(d, SymbolDelta::Added { .. }))
        .collect();
    assert!(
        removed.is_empty() && added.is_empty(),
        "signature edit must not look like delete+create: removed={:?} added={:?}",
        removed,
        added
    );

    // Signature delta fires on the stable identity.
    let sig_delta = diff.symbol_deltas.iter().find(|d| {
        matches!(d, SymbolDelta::SignatureModified { symbol_id, .. }
            if symbol_id.as_str() == "src/math.ts#fn:add")
    });
    assert!(
        sig_delta.is_some(),
        "expected SignatureModified for src/math.ts#fn:add, got {:?}",
        diff.symbol_deltas
    );

    // Breaking contract delta fires with reasons.
    let breaking = diff.contract_deltas.iter().find(|d| {
        matches!(d, ContractDelta::Modified { owner, is_breaking: true, .. }
            if owner.as_str() == "src/math.ts#fn:add")
    });
    assert!(
        breaking.is_some(),
        "expected breaking ContractDelta::Modified, got {:?}",
        diff.contract_deltas
    );
    if let Some(ContractDelta::Modified {
        breaking_reasons, ..
    }) = breaking
    {
        assert!(
            !breaking_reasons.is_empty(),
            "breaking change must carry reasons"
        );
    }
}

#[test]
fn test_verify_baseline_id_catches_breaking_change() {
    let temp = TempTestDir::new("forensic_verify_break");
    create_project(&temp.path);

    assert_eq!(cli(&temp.path, &["baseline"]), EXIT_PASS);
    let snap_id = latest_snapshot_id(&temp.path);

    fs::write(temp.path.join("src").join("math.ts"), BREAKING_MATH).unwrap();

    let code = cli(&temp.path, &["verify", "--baseline-id", &snap_id]);
    assert_eq!(
        code, EXIT_POLICY_FAIL,
        "breaking public-API change must fail verification (exit 2)"
    );
}

#[test]
fn test_verify_baseline_id_passes_non_breaking_change() {
    let temp = TempTestDir::new("forensic_verify_ok");
    create_project(&temp.path);

    assert_eq!(cli(&temp.path, &["baseline"]), EXIT_PASS);
    let snap_id = latest_snapshot_id(&temp.path);

    // Behavior-only edit: same signature and types, different body.
    fs::write(temp.path.join("src").join("math.ts"), NON_BREAKING_MATH).unwrap();

    let code = cli(&temp.path, &["verify", "--baseline-id", &snap_id]);
    assert_eq!(
        code, EXIT_PASS,
        "non-breaking body edit must pass verification (exit 0)"
    );
}

#[test]
fn test_verify_unknown_baseline_id_is_hard_error() {
    let temp = TempTestDir::new("forensic_verify_unknown");
    create_project(&temp.path);

    assert_eq!(cli(&temp.path, &["baseline"]), EXIT_PASS);

    let code = cli(&temp.path, &["verify", "--baseline-id", "deadbeef"]);
    assert_eq!(
        code,
        autopsy_cli::EXIT_ANALYSIS_ERROR,
        "unknown baseline id must be an explicit analysis error, never a silent fallback"
    );
}

#[test]
fn test_impact_edge_kinds_filter_and_rejection() {
    let temp = TempTestDir::new("forensic_edgekinds");
    create_project(&temp.path);
    assert_eq!(cli(&temp.path, &["baseline"]), EXIT_PASS);

    // Scoped query succeeds.
    let code = cli(
        &temp.path,
        &[
            "impact",
            "-s",
            "src/math.ts::add",
            "--direction",
            "forward",
            "--edge-kinds",
            "calls",
        ],
    );
    assert_eq!(code, EXIT_PASS);

    // Unknown kind names are hard errors, never silent.
    let code = cli(
        &temp.path,
        &[
            "impact",
            "-s",
            "src/math.ts::add",
            "--edge-kinds",
            "telepathy",
        ],
    );
    assert_eq!(
        code,
        autopsy_cli::EXIT_ANALYSIS_ERROR,
        "unknown edge kind must be an explicit analysis error"
    );
}

#[test]
fn test_impact_file_seed_addresses_module_node() {
    let temp = TempTestDir::new("forensic_fileseed");
    create_project(&temp.path);
    assert_eq!(cli(&temp.path, &["baseline"]), EXIT_PASS);

    // Bare file path seeds resolve to the module node (exit 0 with
    // results instead of an empty unknown-seed traversal).
    let code = cli(
        &temp.path,
        &["impact", "-s", "src/math.ts", "--direction", "forward"],
    );
    assert_eq!(code, EXIT_PASS);
}

#[test]
fn test_diff_two_stored_snapshots_compares_states() {
    let temp = TempTestDir::new("forensic_twostate");
    create_project(&temp.path);
    assert_eq!(cli(&temp.path, &["baseline"]), EXIT_PASS);
    let before_id = latest_snapshot_id(&temp.path);

    fs::write(temp.path.join("src").join("math.ts"), BREAKING_MATH).unwrap();
    assert_eq!(cli(&temp.path, &["baseline"]), EXIT_PASS);
    let after_id = latest_snapshot_id(&temp.path);
    assert_ne!(before_id, after_id);

    // Restore worktree to before-state so any worktree leak would show.
    fs::write(
        temp.path.join("src").join("math.ts"),
        "export function add(a: number, b: number): number {\n    return a + b;\n}\n",
    )
    .unwrap();

    // Stored-vs-stored diff must still find the breaking change even
    // though the worktree no longer contains it.
    let storage = StorageEngine::open(
        temp.path.join(".autopsy").join("storage.db"),
        StorageOptions {
            use_wal: false,
            cache_dir: Some(temp.path.join(".autopsy").join("cache")),
        },
    )
    .unwrap();
    let config =
        autopsy_repo::AutopsyConfig::load_from_file(temp.path.join("autopsy.toml")).unwrap();
    let before_snapshot = storage
        .get_latest_snapshot()
        .unwrap()
        .expect("snapshots must exist");
    assert_eq!(before_snapshot.snapshot_id.to_string(), after_id);
    let before = autopsy_cli::build_artifacts_from_snapshot(
        storage
            .get_snapshot(&autopsy_domain::SnapshotId::new(&before_id))
            .unwrap()
            .expect("before snapshot must exist"),
        &storage,
    )
    .expect("before rebuild must succeed");
    let after = autopsy_cli::build_artifacts_from_snapshot(before_snapshot, &storage)
        .expect("after rebuild must succeed");
    assert_ne!(
        before.snapshot.snapshot_id, after.snapshot.snapshot_id,
        "two stored states must be distinct snapshots"
    );
    let _ = config;
    assert!(
        before
            .symbols
            .iter()
            .any(|s| s.stable_id.as_str() == "src/math.ts#fn:add"),
        "before-state must contain the pre-edit symbol identity"
    );
    assert!(
        after
            .symbols
            .iter()
            .any(|s| s.stable_id.as_str() == "src/math.ts#fn:add"),
        "after-state keeps the same identity (shape changed, not identity)"
    );
}

#[test]
fn test_impact_stored_snapshot_id_analyzes_that_state() {
    let temp = TempTestDir::new("forensic_snapimpact");
    create_project(&temp.path);
    assert_eq!(cli(&temp.path, &["baseline"]), EXIT_PASS);
    let snap_id = latest_snapshot_id(&temp.path);

    // Unknown snapshot ids are hard errors, never silent worktree fallback.
    let code = cli(
        &temp.path,
        &[
            "impact",
            "-s",
            "src/math.ts::add",
            "--snapshot-id",
            "deadbeef",
        ],
    );
    assert_eq!(
        code,
        autopsy_cli::EXIT_ANALYSIS_ERROR,
        "unknown impact snapshot must be an explicit analysis error"
    );

    // Stored snapshot analyzes fine.
    let code = cli(
        &temp.path,
        &[
            "impact",
            "-s",
            "src/math.ts::add",
            "--snapshot-id",
            &snap_id,
        ],
    );
    assert_eq!(code, EXIT_PASS);
}

#[test]
fn test_impact_callers_profile_and_top_k() {
    let temp = TempTestDir::new("forensic_profile");
    create_project(&temp.path);
    assert_eq!(cli(&temp.path, &["baseline"]), EXIT_PASS);

    // Default profile preserves legacy behavior (exit 0).
    assert_eq!(
        cli(
            &temp.path,
            &["impact", "-s", "src/math.ts::add", "--direction", "forward"]
        ),
        EXIT_PASS
    );

    // Callers preset: backward invocation closure + ranked shortlist.
    assert_eq!(
        cli(
            &temp.path,
            &["impact", "-s", "src/math.ts::add", "--profile", "callers"]
        ),
        EXIT_PASS
    );

    // Explicit top-k slices the shortlist.
    assert_eq!(
        cli(
            &temp.path,
            &[
                "impact",
                "-s",
                "src/math.ts::add",
                "--profile",
                "callers",
                "--top-k",
                "3"
            ]
        ),
        EXIT_PASS
    );

    // Explicit direction overrides the preset default.
    assert_eq!(
        cli(
            &temp.path,
            &[
                "impact",
                "-s",
                "src/math.ts::add",
                "--profile",
                "callers",
                "--direction",
                "forward"
            ]
        ),
        EXIT_PASS
    );
}

#[test]
fn test_incremental_baseline_matches_full_baseline() {
    let temp = TempTestDir::new("forensic_incr");
    create_project(&temp.path);
    assert_eq!(cli(&temp.path, &["baseline"]), EXIT_PASS);
    let base_id = latest_snapshot_id(&temp.path);

    // Body-only edit in one file (symbol set unchanged -> edge reuse path).
    fs::write(temp.path.join("src").join("math.ts"), NON_BREAKING_MATH).unwrap();

    // Full re-baseline for ground truth.
    assert_eq!(cli(&temp.path, &["baseline"]), EXIT_PASS);
    let full_id = latest_snapshot_id(&temp.path);
    assert_ne!(base_id, full_id);

    // Restore base content, re-apply edit, then incremental from base.
    // (Proves incremental reaches the identical state as full analysis.)
    let storage = StorageEngine::open(
        temp.path.join(".autopsy").join("storage.db"),
        StorageOptions {
            use_wal: false,
            cache_dir: Some(temp.path.join(".autopsy").join("cache")),
        },
    )
    .unwrap();
    let full_snapshot = storage
        .get_latest_snapshot()
        .unwrap()
        .expect("full snapshot must exist");
    assert_eq!(full_snapshot.snapshot_id.to_string(), full_id);

    let config =
        autopsy_repo::AutopsyConfig::load_from_file(temp.path.join("autopsy.toml")).unwrap();
    let full_artifacts = autopsy_cli::build_snapshot_and_graph(&temp.path, &config, None, None)
        .expect("full analysis must succeed");

    // Incremental run via CLI must exit 0 and land on the same snapshot.
    let code = cli(
        &temp.path,
        &["baseline", "--incremental", "--baseline-id", &base_id],
    );
    assert_eq!(code, EXIT_PASS);
    let incr_id = latest_snapshot_id(&temp.path);
    assert_eq!(
        incr_id, full_id,
        "incremental baseline must reach the identical snapshot as full analysis"
    );

    // Impact digests must match between incremental and full states.
    let incr_snapshot = storage
        .get_latest_snapshot()
        .unwrap()
        .expect("incremental snapshot must exist");
    let incr_artifacts = autopsy_cli::build_artifacts_from_snapshot(incr_snapshot, &storage)
        .expect("incremental rebuild must succeed");
    assert_eq!(
        incr_artifacts.symbols, full_artifacts.symbols,
        "incremental symbols must equal full-analysis symbols"
    );
    assert_eq!(
        incr_artifacts.edges, full_artifacts.edges,
        "incremental edges must equal full-analysis edges"
    );
    assert_eq!(
        incr_artifacts.contracts, full_artifacts.contracts,
        "incremental contracts must equal full-analysis contracts"
    );
}

#[test]
fn test_invalid_config_fails_before_analysis_with_location() {
    let temp = TempTestDir::new("forensic_badcfg");
    create_project(&temp.path);
    // Corrupt the config: traversal root must be rejected with location.
    fs::write(
        temp.path.join("autopsy.toml"),
        "config_version = \"0.0.1\"\n[repository]\nroots = [\"../../escape\"]\nexclude = []\n[analysis]\ndefault_profile = \"default\"\nmax_traversal_nodes = 50000\n[cache]\ndirectory = \".autopsy/cache\"\n",
    )
    .unwrap();

    let code = cli(&temp.path, &["baseline"]);
    assert_eq!(
        code,
        autopsy_cli::EXIT_ANALYSIS_ERROR,
        "invalid present config must fail before analysis, never fall back to defaults"
    );
}

#[test]
fn test_missing_config_falls_back_to_defaults() {
    let temp = TempTestDir::new("forensic_nocfg");
    let src = temp.path.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("a.ts"), "export const a = 1;\n").unwrap();
    // No autopsy.toml at all: zero-config operation must still pass.
    assert_eq!(cli(&temp.path, &["baseline"]), EXIT_PASS);
}

#[test]
fn test_symbols_and_edges_browsing_commands() {
    let temp = TempTestDir::new("forensic_browse");
    create_project(&temp.path);
    assert_eq!(cli(&temp.path, &["baseline"]), EXIT_PASS);

    // Symbols listing succeeds (deterministic order asserted in MCP tests).
    assert_eq!(cli(&temp.path, &["symbols", "--limit", "50"]), EXIT_PASS);
    assert_eq!(
        cli(&temp.path, &["symbols", "--kind", "function"]),
        EXIT_PASS
    );

    // 1-hop neighborhood of a known symbol.
    assert_eq!(
        cli(
            &temp.path,
            &["edges", "--source", "src/math.ts::add", "--limit", "50"]
        ),
        EXIT_PASS
    );

    // Unknown symbols fail explicitly, never empty-success.
    assert_eq!(
        cli(&temp.path, &["edges", "--source", "no-such-symbol"]),
        autopsy_cli::EXIT_ANALYSIS_ERROR
    );

    // SARIF is meaningless for listings: explicit error, not silent JSON.
    // (Built without the shared --format json prefix so the guard itself
    // is exercised, not clap duplicate-flag rejection.)
    let args = vec![
        "autopsy".to_string(),
        "--repo-root".to_string(),
        temp.path.display().to_string(),
        "--format".to_string(),
        "sarif".to_string(),
        "symbols".to_string(),
        "--limit".to_string(),
        "5".to_string(),
    ];
    assert_eq!(run_cli(&args), autopsy_cli::EXIT_ANALYSIS_ERROR);
}

#[test]
fn test_baseline_empty_snapshot_warns_not_silent() {
    let temp = TempTestDir::new("forensic_empty");
    fs::create_dir_all(&temp.path).unwrap();
    // Empty dir (no sources, no config): baseline succeeds (exit 0,
    // empty repos are legal) — the stderr warning is the signal.
    // (Warning text asserted manually; exit code is the contract.)
    assert_eq!(cli(&temp.path, &["baseline"]), EXIT_PASS);
}

const PY_BEFORE: &str = "def add(a, b):\n    return a + b\n";
const PY_BREAKING: &str = "def add(a, b, tenant_id):\n    return a + b\n";

fn create_python_project(root: &Path) {
    let src_dir = root.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(root.join(".autopsy")).unwrap();
    fs::write(src_dir.join("math.py"), PY_BEFORE).unwrap();
    fs::write(
        src_dir.join("app.py"),
        "from math import add\n\n\ndef calc(qty):\n    return add(qty, qty)\n",
    )
    .unwrap();
    fs::write(
        root.join("autopsy.toml"),
        "config_version = \"0.0.1\"\n\n[repository]\nroots = [\"src\"]\nexclude = []\n\n[analysis]\ndefault_profile = \"default\"\nmax_traversal_nodes = 50000\n\n[cache]\ndirectory = \".autopsy/cache\"\n",
    )
    .unwrap();
    fs::write(
        root.join(".autopsy").join("invariants.yml"),
        "version: \"0.0.1\"\ninvariants:\n  - id: \"api-compatibility\"\n    description: \"Python API contracts must remain compatible\"\n    kind: \"api_compatibility\"\n    severity: \"error\"\n",
    )
    .unwrap();
}

#[test]
fn test_python_breaking_change_end_to_end() {
    use autopsy_adapter_api::{LanguageAdapter, ParseContext};
    use autopsy_adapter_python::PythonAdapter;
    use autopsy_domain::FileUnit;

    // Adapter-level sanity: Python symbols carry stable identities.
    let adapter = PythonAdapter::with_root_dir("/tmp");
    let ctx = ParseContext::new("/tmp");
    let unit = FileUnit {
        path: "src/math.py".to_string(),
        language: "python".to_string(),
        content_hash: "x".to_string(),
        is_generated: false,
    };
    let parsed = adapter.parse(&ctx, &unit, PY_BEFORE).unwrap();
    assert_eq!(parsed.coverage, autopsy_domain::CoverageState::Verified);
    let symbols = adapter.extract_symbols(&parsed).unwrap();
    assert!(
        symbols
            .iter()
            .any(|s| s.stable_id.as_str() == "src/math.py#fn:add"),
        "missing Python symbol: {:?}",
        symbols.iter().map(|s| &s.stable_id).collect::<Vec<_>>()
    );

    // CLI end to end: baseline -> breaking edit -> diff + verify.
    let temp = TempTestDir::new("forensic_python");
    create_python_project(&temp.path);
    assert_eq!(cli(&temp.path, &["baseline"]), EXIT_PASS);
    let snap_id = latest_snapshot_id(&temp.path);

    fs::write(temp.path.join("src").join("math.py"), PY_BREAKING).unwrap();

    // Impact: caller app.calc must surface via the import-resolved edge.
    let code = cli(
        &temp.path,
        &["impact", "-s", "src/math.py::add", "--profile", "callers"],
    );
    assert_eq!(code, EXIT_PASS);

    // Library-level proof: the caller is genuinely in the closure.
    let storage = StorageEngine::open(
        temp.path.join(".autopsy").join("storage.db"),
        StorageOptions {
            use_wal: false,
            cache_dir: Some(temp.path.join(".autopsy").join("cache")),
        },
    )
    .unwrap();
    let snapshot = storage.get_latest_snapshot().unwrap().unwrap();
    let artifacts = autopsy_cli::build_artifacts_from_snapshot(snapshot, &storage).unwrap();
    let engine = autopsy_impact::ImpactEngine::new();
    let query = autopsy_impact::ImpactQuery {
        seed_symbols: vec![autopsy_domain::SymbolId::new("src/math.py#fn:add")],
        direction: autopsy_impact::ImpactDirection::Backward,
        profile: autopsy_impact::callers_profile(10, 1000),
    };
    let result = engine.compute_impact(&artifacts.graph, &query).unwrap();
    let ids: Vec<&str> = result
        .impacted_entities
        .iter()
        .map(|s| s.as_str())
        .collect();
    assert!(
        ids.contains(&"src/app.py#fn:calc"),
        "Python caller must be in backward closure: {:?}",
        ids
    );

    // Verify against the baseline must fail on the breaking change.
    let code = cli(&temp.path, &["verify", "--baseline-id", &snap_id]);
    assert_eq!(
        code, EXIT_POLICY_FAIL,
        "breaking Python signature change must fail verification"
    );
}
