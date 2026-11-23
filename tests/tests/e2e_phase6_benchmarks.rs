//! End-to-end integration tests for Phase 6: Benchmark Harness (FR-025, UC-08, RQ1-5).
//!
//! Validates:
//! - Corpus repositories (>= 5 TypeScript repositories with valid configurations).
//! - Task suite completeness (>= 50 tasks across 5 categories matching schema).
//! - Task shape and held-out partition.
//! - Fail-closed integrity audit refuses scores while oracle/comparator integrity is invalid.

use serde_json::Value;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get workspace root")
        .to_path_buf()
}

fn count_ts_files(dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                count += count_ts_files(&path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("ts") {
                count += 1;
            }
        }
    }
    count
}

/// Resolve a usable Python runtime without relying on a developer's virtualenv
/// or a stale WindowsApps alias. `PYTHON` is an explicit escape hatch for CI;
/// otherwise use the first launcher that can execute `--version`.
fn python_command() -> (String, Vec<String>) {
    if let Ok(explicit) = env::var("PYTHON")
        && !explicit.trim().is_empty()
    {
        return (explicit, Vec::new());
    }

    let candidates: Vec<(&str, Vec<&str>)> = if cfg!(windows) {
        vec![
            ("python3", Vec::new()),
            ("python", Vec::new()),
            ("py", vec!["-3"]),
        ]
    } else {
        vec![("python3", Vec::new()), ("python", Vec::new())]
    };

    for (program, prefix_args) in candidates {
        let mut probe = Command::new(program);
        probe.args(&prefix_args).arg("--version");
        if probe.output().is_ok_and(|output| output.status.success()) {
            return (
                program.to_owned(),
                prefix_args.into_iter().map(str::to_owned).collect(),
            );
        }
    }

    panic!("Python runtime not found; install Python 3.11+ or set the PYTHON environment variable");
}

#[test]
fn test_benchmark_corpus_repositories_exist() {
    let root = workspace_root();
    let corpus_dir = root.join("benchmarks").join("corpus");
    assert!(
        corpus_dir.exists(),
        "benchmarks/corpus directory must exist"
    );

    let expected_repos = [
        "repo-api",
        "repo-migration",
        "repo-relayer",
        "repo-cross-package",
        "repo-dead-call",
    ];

    for repo_name in &expected_repos {
        let repo_path = corpus_dir.join(repo_name);
        assert!(repo_path.exists(), "Corpus repo {} must exist", repo_name);

        // Verify configuration files exist
        assert!(
            repo_path.join("autopsy.toml").exists(),
            "Repo {} must contain autopsy.toml",
            repo_name
        );
        assert!(
            repo_path.join(".autopsy").join("invariants.yml").exists(),
            "Repo {} must contain .autopsy/invariants.yml",
            repo_name
        );

        // Verify source files exist
        let ts_file_count = count_ts_files(&repo_path);
        assert!(
            ts_file_count >= 5,
            "Repo {} must contain at least 5 TypeScript files (found {})",
            repo_name,
            ts_file_count
        );
    }
}

#[test]
fn test_benchmark_task_suite_conformance() {
    let root = workspace_root();
    let tasks_dir = root.join("benchmarks").join("tasks");
    assert!(tasks_dir.exists(), "benchmarks/tasks directory must exist");

    let schema_path = root.join("schemas").join("benchmark-task.schema.json");
    assert!(schema_path.exists(), "benchmark task schema must exist");

    let entries = fs::read_dir(&tasks_dir).expect("Failed to read tasks dir");
    let mut task_files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|s| s.starts_with("task_") && s.ends_with(".json"))
        })
        .collect();

    task_files.sort();
    assert!(
        task_files.len() >= 50,
        "Benchmark suite must contain >= 50 tasks (found {})",
        task_files.len()
    );

    let mut categories: BTreeMap<String, usize> = BTreeMap::new();
    let mut held_out_count = 0;

    for task_path in &task_files {
        let content = fs::read_to_string(task_path).expect("Failed to read task file");
        let val: Value = serde_json::from_str(&content).expect("Task file must be valid JSON");

        // Verify required schema fields
        assert!(val.get("id").is_some(), "Task must contain id");
        assert!(
            val.get("repository").is_some(),
            "Task must contain repository"
        );
        assert!(val.get("base_ref").is_some(), "Task must contain base_ref");
        assert!(val.get("prompt").is_some(), "Task must contain prompt");
        assert!(
            val.get("ground_truth").is_some(),
            "Task must contain ground_truth"
        );

        let category = val
            .get("category")
            .and_then(|c| c.as_str())
            .unwrap_or("unknown")
            .to_string();
        *categories.entry(category).or_insert(0) += 1;

        if val.get("is_held_out").and_then(|h| h.as_bool()) == Some(true) {
            held_out_count += 1;
        }

        // Verify ground truth fields
        let gt = val.get("ground_truth").unwrap();
        assert!(
            gt.get("seed_symbols").is_some(),
            "Ground truth must contain seed_symbols"
        );
        assert!(
            gt.get("expected_impacted_symbols").is_some(),
            "Ground truth must contain expected_impacted_symbols"
        );
        assert!(
            gt.get("expected_failure").is_some(),
            "Ground truth must contain expected_failure"
        );
        assert!(
            gt.get("expected_failure_classes").is_some(),
            "Ground truth must contain expected_failure_classes"
        );
    }

    // Assert coverage across all 5 required categories
    let required_categories = [
        "api_change",
        "migration",
        "relayer",
        "cross_package",
        "dead_call",
    ];
    for req_cat in &required_categories {
        let count = categories.get(*req_cat).copied().unwrap_or(0);
        assert!(
            count >= 10,
            "Category {} must have >= 10 tasks (found {})",
            req_cat,
            count
        );
    }

    // Assert held-out split exists (>= 10 held-out tasks)
    assert!(
        held_out_count >= 10,
        "Held-out validation set must contain >= 10 tasks (found {})",
        held_out_count
    );
}

#[test]
fn test_benchmark_harness_refuses_unreviewed_oracle_and_leaky_comparators() {
    let root = workspace_root();
    let runner_script = root.join("benchmarks").join("run_benchmarks.py");
    assert!(runner_script.exists(), "run_benchmarks.py must exist");

    let temp_out = std::env::temp_dir().join(format!("autopsy_bench_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_out);
    fs::create_dir_all(&temp_out).unwrap();

    let (python_program, python_prefix_args) = python_command();
    let mut cmd = Command::new(python_program);
    cmd.args(python_prefix_args);
    cmd.arg(&runner_script)
        .arg("--check-gate")
        .arg("--output-dir")
        .arg(&temp_out)
        .current_dir(&root);

    let bin_name = if cfg!(windows) {
        "autopsy.exe"
    } else {
        "autopsy"
    };
    let release_bin = root.join("target").join("release").join(bin_name);
    let debug_bin = root.join("target").join("debug").join(bin_name);
    if let Ok(bin) = std::env::var("AUTOPSY_BIN") {
        cmd.arg("--autopsy-bin").arg(bin);
    } else if debug_bin.exists() {
        cmd.arg("--autopsy-bin").arg(&debug_bin);
    } else if release_bin.exists() {
        cmd.arg("--autopsy-bin").arg(&release_bin);
    }

    let output = cmd.output().expect("Failed to execute run_benchmarks.py");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("Benchmark runner stdout:\n{}", stdout);
    if !stderr.is_empty() {
        eprintln!("Benchmark runner stderr:\n{}", stderr);
    }

    assert_eq!(
        output.status.code(),
        Some(3),
        "invalid benchmark integrity must fail closed before scoring"
    );
    assert!(stdout.contains("[NOT EVALUABLE]"));
    assert!(stdout.contains("answer-key fields"));

    // A rejected benchmark must not emit numeric scores or a passing gate.
    let json_report = temp_out.join("reproduction_report.json");
    let md_report = temp_out.join("reproduction_report.md");
    assert!(
        !json_report.exists(),
        "invalid benchmark must not emit JSON scores"
    );
    assert!(
        !md_report.exists(),
        "invalid benchmark must not emit a gate report"
    );

    let _ = fs::remove_dir_all(&temp_out);
}
