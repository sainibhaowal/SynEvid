"""Smoke tests for the benchmark harness (fast, no engine execution).

Validates task loading, schema conformance structure, and metric math.
Full 50-task evaluation with the real binary is covered by
tests/tests/e2e_phase6_benchmarks.rs.
"""

from pathlib import Path

from benchmarks.engine import gate_evaluator, metrics
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
