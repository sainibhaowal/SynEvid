"""Smoke tests for the benchmark harness (fast, no engine execution).

Validates task loading, schema conformance structure, and metric math.
Full 50-task evaluation with the real binary is covered by
tests/tests/e2e_phase6_benchmarks.rs.
"""

import json
import subprocess
import sys
from pathlib import Path

import pytest

from benchmarks.engine import gate_evaluator, metrics
from benchmarks.engine.integrity import audit_benchmark
from benchmarks.engine.runner import BenchmarkRunner
from benchmarks.engine.task_loader import load_all_tasks

TASKS_DIR = Path(__file__).resolve().parent.parent / "tasks"


def test_task_suite_loads_fifty_tasks():
    tasks = load_all_tasks(TASKS_DIR)
    assert len(tasks) >= 50


def test_task_categories_and_blindness():
    tasks = load_all_tasks(TASKS_DIR)
    categories = {t.category for t in tasks}
    assert {"api_change", "migration", "relayer", "cross_package", "dead_call"} <= categories
    for task in tasks:
        assert task.id
        assert task.ground_truth.seed_symbols, f"{task.id} needs seeds"
        # Blind evaluation: prompts must not leak expected symbol lists.
        for expected in task.ground_truth.expected_impacted_symbols:
            assert expected not in task.prompt, f"{task.id} leaks ground truth"


def test_current_benchmark_fails_closed_for_unreviewed_labels_and_leaky_baselines():
    blockers = audit_benchmark(TASKS_DIR)
    assert any("query direction" in blocker for blocker in blockers)
    assert any("oracle completeness" in blocker for blocker in blockers)
    assert any("two-reviewer oracle" in blocker for blocker in blockers)
    assert any("answer-key fields" in blocker for blocker in blockers)
    assert any("not an LSP baseline" in blocker for blocker in blockers)


def test_audit_requires_exhaustive_adjudicated_oracle_and_query_scope(tmp_path):
    import json

    tasks_dir = tmp_path / "tasks"
    engine_dir = tmp_path / "engine"
    tasks_dir.mkdir()
    engine_dir.mkdir()
    (engine_dir / "baseline_grep.py").write_text("def predict(): return set()", encoding="utf-8")
    (engine_dir / "baseline_lsp.py").write_text(
        "def predict(): return set()  # tsserver", encoding="utf-8"
    )
    task = {"id": "review-required", "ground_truth": {"oracle_complete": False}}
    (tasks_dir / "task_001.json").write_text(json.dumps(task), encoding="utf-8")

    blockers = audit_benchmark(tasks_dir, engine_dir)
    assert any("query direction" in blocker for blocker in blockers)
    assert any("not established" in blocker for blocker in blockers)
    assert any("two-reviewer oracle" in blocker for blocker in blockers)


def test_runner_api_cannot_bypass_the_fail_closed_integrity_gate(tmp_path):
    with pytest.raises(RuntimeError, match="NOT EVALUABLE"):
        BenchmarkRunner(
            tasks_dir=TASKS_DIR,
            corpus_dir=tmp_path / "corpus",
            output_dir=tmp_path / "results",
            autopsy_binary=tmp_path / "autopsy.exe",
        ).run()


def test_cli_refuses_scores_and_writes_no_report_for_invalid_corpus(tmp_path):
    repo_root = Path(__file__).resolve().parents[2]
    output_dir = tmp_path / "reports"
    runner = repo_root / "benchmarks" / "run_benchmarks.py"
    result = subprocess.run(
        [sys.executable, str(runner), "--output-dir", str(output_dir)],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=False,
    )

    assert result.returncode == 3
    assert "[NOT EVALUABLE]" in result.stdout
    assert "answer-key fields" in result.stdout
    assert "not an LSP baseline" in result.stdout
    assert not (output_dir / "reproduction_report.json").exists()
    assert not (output_dir / "reproduction_report.md").exists()


def test_archived_accuracy_reports_are_machine_marked_withdrawn():
    repo_root = Path(__file__).resolve().parents[2]
    report_paths = [
        repo_root / "benchmarks" / "results" / "reproduction_report.json",
        repo_root
        / "docs"
        / "reports"
        / "phase-6"
        / "results"
        / "2026-11-23-impact-benchmark"
        / "reproduction_report.json",
    ]

    for report_path in report_paths:
        report = json.loads(report_path.read_text(encoding="utf-8"))
        assert report["evaluation_status"] == "WITHDRAWN_NOT_EVALUABLE"
        legacy_gate = report["legacy_untrusted_gate_telemetry"]
        assert legacy_gate["validity"] == "WITHDRAWN_NOT_EVALUABLE"
        assert legacy_gate["passed"] is None
        assert legacy_gate["gate_1_recall_lift_passed"] is None


def test_metrics_recall_precision_math():
    result = metrics.compute_task_metrics(
        task_id="SMOKE",
        baseline="smoke",
        category="api_change",
        is_held_out=False,
        predicted={"a", "b"},
        ground_truth={"b", "c"},
        tool_calls=2,
        tokens=100,
        time_ms=5.0,
        expected_failure_classes=["x"],
        detected_failure_classes=[],
        notes="",
    )
    assert result.true_positives == 1
    assert result.false_positives == 1
    assert result.false_negatives == 1
    assert result.precision == 0.5
    assert result.recall == 0.5
    assert result.missed_failure_classes == ["x"]


def test_gate_requires_recall_lift():
    agg_a = metrics.compute_aggregate_metrics("A", [])
    assert agg_a.task_count == 0
    assert agg_a.mean_recall == 0.0
    assert gate_evaluator.GateEvaluator is not None


def test_gate_report_does_not_overstate_other_failed_metrics(tmp_path):
    def aggregate(name, precision, recall, f1, calls, tokens, latency, failures):
        return metrics.AggregateMetrics(
            baseline=name,
            task_count=50,
            total_tp=0,
            total_fp=0,
            total_fn=0,
            mean_precision=precision,
            mean_recall=recall,
            mean_f1=f1,
            total_tool_calls=calls * 50,
            mean_tool_calls=calls,
            total_tokens=tokens * 50,
            mean_tokens=tokens,
            total_time_ms=latency * 50,
            mean_time_ms=latency,
            failure_classes_detected=failures,
            failure_classes_missed=0,
        )

    baseline_a = aggregate("A", 0.26, 0.26, 0.26, 2.92, 926, 122.0, 0)
    baseline_b = aggregate("B", 0.76, 0.6827, 0.7071, 1.82, 819, 100.5, 8)
    synevid = aggregate("C", 0.0598, 1.0, 0.108, 3.0, 3425, 546.5, 0)
    result = gate_evaluator.GateEvaluator.evaluate(baseline_a, baseline_b, synevid)
    assert result.passed  # The pre-registered OR-gate passes on recall alone.
    assert not result.gate_2_cost_cut_passed
    assert not result.gate_3_new_failure_class_passed
    assert result.new_failure_classes_found == 0

    _, markdown_path = gate_evaluator.write_reproduction_report(
        tmp_path, result, baseline_a, baseline_b, synevid, {}
    )
    report = markdown_path.read_text(encoding="utf-8")
    assert "not a production-readiness or overall quality verdict" in report
    assert "-70.02pp" in report  # Precision is materially worse than baseline B.
    assert "-59.91pp" in report  # F1 is materially worse than baseline B.
    assert "546.5 ms" in report
    assert "Zero false textual matches" not in report
    assert "Superior balance" not in report
    assert "Sub-50ms deterministic Rust" not in report
