//! End-to-end integration tests for Phase 5: CLI + Storage + Report (FR-015, FR-016, FR-017, FR-024, FR-028).

use autopsy_cli::{EXIT_ANALYSIS_ERROR, EXIT_PASS, EXIT_POLICY_FAIL, EXIT_UNSUPPORTED, run_cli};
use autopsy_domain::SnapshotId;
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

fn create_test_project(root: &Path) {
    let src_dir = root.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let autopsy_dir = root.join(".autopsy");
    fs::create_dir_all(&autopsy_dir).unwrap();

    // Write source files
    fs::write(
        src_dir.join("math.ts"),
        r#"
export function add(a: number, b: number): number {
    return a + b;
}
export function multiply(a: number, b: number): number {
    return a * b;
}
"#,
    )
    .unwrap();

    fs::write(
        src_dir.join("index.ts"),
        r#"
import { add } from './math';

export function calculateTotal(qty: number, price: number): number {
    return add(qty, price);
}
"#,
    )
    .unwrap();

    // Write autopsy.toml
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

    // Write invariants.yml
    fs::write(
        autopsy_dir.join("invariants.yml"),
        r#"
version: "0.0.1"
invariants:
  - id: "no-circular-deps"
    description: "No circular dependencies permitted"
    kind: "no_new_cycle"
    severity: "error"
  - id: "api-compatibility"
    description: "Exported API contracts must remain compatible"
    kind: "api_compatibility"
    severity: "error"
"#,
    )
    .unwrap();
}

#[test]
fn test_cli_version_and_doctor() {
    let temp = TempTestDir::new("version_doctor");
    create_test_project(&temp.path);

    // 1. Version command
    let args = vec!["autopsy".to_string(), "version".to_string()];
    let code = run_cli(&args);
    assert_eq!(code, EXIT_PASS);

    // 2. Doctor command
    let args = vec![
        "autopsy".to_string(),
        "--repo-root".to_string(),
        temp.path.display().to_string(),
        "doctor".to_string(),
    ];
    let code = run_cli(&args);
    assert_eq!(code, EXIT_PASS);
}

#[test]
fn test_cli_baseline_storage_and_cache_roundtrip() {
    let temp = TempTestDir::new("baseline_roundtrip");
    create_test_project(&temp.path);

    let db_path = temp.path.join(".autopsy").join("storage.db");
    let cache_dir = temp.path.join(".autopsy").join("cache");

    let args = vec![
        "autopsy".to_string(),
        "--repo-root".to_string(),
        temp.path.display().to_string(),
        "--db-path".to_string(),
        db_path.display().to_string(),
        "--cache-dir".to_string(),
        cache_dir.display().to_string(),
        "--format".to_string(),
        "json".to_string(),
        "baseline".to_string(),
    ];

    let code = run_cli(&args);
    assert_eq!(code, EXIT_PASS);

    // Verify SQLite file was created and contains snapshot
    assert!(db_path.exists(), "storage.db should exist");
    let storage = StorageEngine::open(
        &db_path,
        StorageOptions {
            use_wal: false,
            cache_dir: Some(cache_dir.clone()),
        },
    )
    .expect("StorageEngine should open DB");

    let latest = storage
        .get_latest_snapshot()
        .expect("Should query latest snapshot")
        .expect("Snapshot should be present in database");

    assert!(!latest.snapshot_id.as_str().is_empty());
    assert_eq!(latest.files.len(), 2);
    assert!(latest.files.contains_key("src/math.ts"));
    assert!(latest.files.contains_key("src/index.ts"));

    // Verify cache directory has object
    let objects_dir = cache_dir.join("objects");
    assert!(objects_dir.exists(), "objects cache directory should exist");
}

#[test]
fn test_cli_diff_and_impact() {
    let temp = TempTestDir::new("diff_impact");
    create_test_project(&temp.path);

    // Run baseline first
    let args = vec![
        "autopsy".to_string(),
        "--repo-root".to_string(),
        temp.path.display().to_string(),
        "baseline".to_string(),
    ];
    assert_eq!(run_cli(&args), EXIT_PASS);

    // Modify file
    let math_file = temp.path.join("src").join("math.ts");
    fs::write(
        &math_file,
        r#"
export function add(a: number, b: number): number {
    return a + b;
}
export function subtract(a: number, b: number): number {
    return a - b;
}
"#,
    )
    .unwrap();

    // Run diff
    let args = vec![
        "autopsy".to_string(),
        "--repo-root".to_string(),
        temp.path.display().to_string(),
        "--format".to_string(),
        "json".to_string(),
        "diff".to_string(),
    ];
    assert_eq!(run_cli(&args), EXIT_PASS);

    // Run impact from seed symbol
    let args = vec![
        "autopsy".to_string(),
        "--repo-root".to_string(),
        temp.path.display().to_string(),
        "--format".to_string(),
        "json".to_string(),
        "impact".to_string(),
        "-s".to_string(),
        "src/math.ts::add".to_string(),
        "--direction".to_string(),
        "forward".to_string(),
    ];
    assert_eq!(run_cli(&args), EXIT_PASS);
}

#[test]
fn test_cli_verify_policy_failure_exit_code() {
    let temp = TempTestDir::new("verify_policy");
    create_test_project(&temp.path);

    // Run verify on clean project -> should pass (code 0)
    let args = vec![
        "autopsy".to_string(),
        "--repo-root".to_string(),
        temp.path.display().to_string(),
        "--format".to_string(),
        "text".to_string(),
        "verify".to_string(),
    ];
    assert_eq!(run_cli(&args), EXIT_PASS);

    // Introduce circular dependency: A -> B and B -> A
    let a_file = temp.path.join("src").join("a.ts");
    let b_file = temp.path.join("src").join("b.ts");
    fs::write(&a_file, "import './b';\nexport const a = 1;\n").unwrap();
    fs::write(&b_file, "import './a';\nexport const b = 2;\n").unwrap();

    let args = vec![
        "autopsy".to_string(),
        "--repo-root".to_string(),
        temp.path.display().to_string(),
        "verify".to_string(),
    ];
    let exit_code = run_cli(&args);
    // Cycle violation must trigger policy failure (code 2)
    assert_eq!(exit_code, EXIT_POLICY_FAIL);
}

#[test]
fn test_cli_verify_strict_mode_unsupported_exit_code() {
    let temp = TempTestDir::new("verify_strict");
    create_test_project(&temp.path);

    // Introduce unsupported dynamic construct (eval)
    let dynamic_file = temp.path.join("src").join("dynamic.ts");
    fs::write(
        &dynamic_file,
        r#"
export function runDynamic(code: string): any {
    return eval(code);
}
"#,
    )
    .unwrap();

    // In normal mode without --strict, may pass if invariants allow
    // In strict mode with --strict, unsupported dynamic construct must return exit code 4
    let args = vec![
        "autopsy".to_string(),
        "--repo-root".to_string(),
        temp.path.display().to_string(),
        "verify".to_string(),
        "--strict".to_string(),
    ];
    let exit_code = run_cli(&args);
    assert_eq!(exit_code, EXIT_UNSUPPORTED);
}

#[test]
fn test_cli_analysis_error_on_invalid_input() {
    // Non-existent directory should return exit code 3 (analysis error)
    let args = vec![
        "autopsy".to_string(),
        "--repo-root".to_string(),
        "/non/existent/path/for/autopsy/test".to_string(),
        "doctor".to_string(),
    ];
    let exit_code = run_cli(&args);
    assert_eq!(exit_code, EXIT_ANALYSIS_ERROR);
}

#[test]
fn test_offline_network_off_isolation() {
    // Verify analyzer functions without any network configuration
    let temp = TempTestDir::new("offline_isolation");
    create_test_project(&temp.path);

    let args = vec![
        "autopsy".to_string(),
        "--repo-root".to_string(),
        temp.path.display().to_string(),
        "--format".to_string(),
        "sarif".to_string(),
        "verify".to_string(),
    ];
    let exit_code = run_cli(&args);
    assert_eq!(exit_code, EXIT_PASS);
}

#[test]
fn test_phase5_100_runs_determinism() {
    let temp = TempTestDir::new("phase5_determinism");
    create_test_project(&temp.path);

    let config =
        autopsy_repo::AutopsyConfig::load_from_file(temp.path.join("autopsy.toml")).unwrap();

    let mut first_digest: Option<String> = None;
    let mut first_snapshot_id: Option<SnapshotId> = None;

    for i in 0..100 {
        let artifacts = autopsy_cli::build_snapshot_and_graph(&temp.path, &config, None)
            .expect("Build snapshot should succeed");

        if i == 0 {
            first_snapshot_id = Some(artifacts.snapshot.snapshot_id.clone());
            first_digest = Some(artifacts.snapshot.file_set_digest.clone());
        } else {
            assert_eq!(
                artifacts.snapshot.snapshot_id,
                first_snapshot_id.as_ref().unwrap().clone(),
                "Snapshot ID diverged on iteration {}",
                i
            );
            assert_eq!(
                artifacts.snapshot.file_set_digest,
                *first_digest.as_ref().unwrap(),
                "File set digest diverged on iteration {}",
                i
            );
        }
    }
}
