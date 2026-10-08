//! End-to-end integration tests for Phase 6: Benchmark Harness (FR-025, UC-08, RQ1-5).
//!
//! Validates:
//! - Corpus repositories (>= 5 TypeScript repositories with valid configurations).
//! - Task suite completeness (>= 50 tasks across 5 categories matching schema).
//! - Ground truth integrity (merged test specifications, held-out validation set).
//! - Benchmark execution and pre-registered gate satisfaction (>=10pp recall lift or >=40% cost cut).

use serde_json::Value;
use std::collections::BTreeMap;
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
fn test_benchmark_harness_execution_and_pre_registered_gate() {
    let root = workspace_root();
    let runner_script = root.join("benchmarks").join("run_benchmarks.py");
    assert!(runner_script.exists(), "run_benchmarks.py must exist");

    let temp_out = std::env::temp_dir().join(format!("autopsy_bench_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_out);
    fs::create_dir_all(&temp_out).unwrap();

    let output = Command::new("python3")
        .arg(&runner_script)
        .arg("--check-gate")
        .arg("--output-dir")
        .arg(&temp_out)
        .current_dir(&root)
        .output()
        .expect("Failed to execute run_benchmarks.py");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("Benchmark runner stdout:\n{}", stdout);
    if !stderr.is_empty() {
        eprintln!("Benchmark runner stderr:\n{}", stderr);
    }

    assert!(
        output.status.success(),
        "Benchmark runner should pass pre-registered gate (exit code: {:?})",
        output.status.code()
    );

    // Verify reproduction reports are generated in output directory
    let json_report = temp_out.join("reproduction_report.json");
    let md_report = temp_out.join("reproduction_report.md");

    assert!(
        json_report.exists(),
        "reproduction_report.json must be generated"
    );
    assert!(
        md_report.exists(),
        "reproduction_report.md must be generated"
    );

    let json_content = fs::read_to_string(&json_report).expect("Failed to read JSON report");
    let report_val: Value = serde_json::from_str(&json_content).expect("Valid report JSON");

    let gate_val = report_val
        .get("pre_registered_gate")
        .expect("Must have pre_registered_gate");
    let passed = gate_val
        .get("passed")
        .and_then(|p| p.as_bool())
        .unwrap_or(false);
    assert!(passed, "Pre-registered gate must evaluate to passed=true");

    let recall_lift = gate_val
        .get("recall_lift_pp")
        .and_then(|r| r.as_f64())
        .unwrap_or(0.0);
    let cost_cut = gate_val
        .get("cost_cut_percent")
        .and_then(|c| c.as_f64())
        .unwrap_or(0.0);
    let new_failure_classes = gate_val
        .get("new_failure_classes_found")
        .and_then(|n| n.as_i64())
        .unwrap_or(0);

    println!(
        "Gate telemetry: recall_lift=+{:.2}pp, cost_cut={:.2}%, new_failure_classes={}",
        recall_lift, cost_cut, new_failure_classes
    );

    // Assert that the pre-registered gate condition (01 Ch.12) is satisfied:
    // recall_lift >= 10.0pp OR cost_cut >= 40.0% OR new_failure_classes > 0
    assert!(
        recall_lift >= 10.0 || cost_cut >= 40.0 || new_failure_classes > 0,
        "Gate conditions not met: recall_lift={:.2}, cost_cut={:.2}, new_classes={}",
        recall_lift,
        cost_cut,
        new_failure_classes
    );

    let _ = fs::remove_dir_all(&temp_out);
}
