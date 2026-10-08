"""
Benchmark runner module for Synevid Phase 6.
Executes all tasks across Baselines A, B, and C, aggregates metrics, and evaluates gates.
"""

from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Optional

from benchmarks.engine.baseline_autopsy import BaselineAutopsy
from benchmarks.engine.baseline_grep import BaselineGrep
from benchmarks.engine.baseline_lsp import BaselineLSP
from benchmarks.engine.gate_evaluator import (
    GateEvaluationResult,
    GateEvaluator,
    write_reproduction_report,
)
from benchmarks.engine.metrics import (
    AggregateMetrics,
    TaskResult,
    compute_aggregate_metrics,
)
from benchmarks.engine.task_loader import BenchmarkTask, load_all_tasks


@dataclass
class BenchmarkRunResult:
    tasks: List[BenchmarkTask]
    results_a: List[TaskResult]
    results_b: List[TaskResult]
    results_c: List[TaskResult]
    agg_a: AggregateMetrics
    agg_b: AggregateMetrics
    agg_c: AggregateMetrics
    gate_result: GateEvaluationResult
    json_report_path: Path
    md_report_path: Path


class BenchmarkRunner:
    """
    Orchestrates the Phase 6 benchmark execution pipeline.
    """

    def __init__(
        self,
        tasks_dir: Path,
        corpus_dir: Path,
        output_dir: Path,
        autopsy_binary: Path,
    ):
        self.tasks_dir = tasks_dir
        self.corpus_dir = corpus_dir
        self.output_dir = output_dir
        self.autopsy_binary = autopsy_binary

    def run(
        self,
        category_filter: Optional[str] = None,
        held_out_only: bool = False,
    ) -> BenchmarkRunResult:
        tasks = load_all_tasks(self.tasks_dir)

        if category_filter:
            tasks = [t for t in tasks if t.category == category_filter]
        if held_out_only:
            tasks = [t for t in tasks if t.is_held_out]

        if not tasks:
            raise ValueError("No tasks found matching criteria")

        baseline_grep = BaselineGrep(self.corpus_dir)
        baseline_lsp = BaselineLSP(self.corpus_dir)
        baseline_autopsy = BaselineAutopsy(self.corpus_dir, self.autopsy_binary)

        results_a: List[TaskResult] = []
        results_b: List[TaskResult] = []
        results_c: List[TaskResult] = []

        for task in tasks:
            ra = baseline_grep.evaluate_task(task)
            rb = baseline_lsp.evaluate_task(task)
            rc = baseline_autopsy.evaluate_task(task)

            results_a.append(ra)
            results_b.append(rb)
            results_c.append(rc)

        agg_a = compute_aggregate_metrics("Baseline-A (Agent+Grep)", results_a)
        agg_b = compute_aggregate_metrics("Baseline-B (Agent+LSP)", results_b)
        agg_c = compute_aggregate_metrics("Baseline-C (Agent+Autopsy)", results_c)

        gate_result = GateEvaluator.evaluate(agg_a, agg_b, agg_c)

        task_results_map = {
            "Baseline-A": results_a,
            "Baseline-B": results_b,
            "Baseline-C": results_c,
        }

        json_path, md_path = write_reproduction_report(
            self.output_dir,
            gate_result,
            agg_a,
            agg_b,
            agg_c,
            task_results_map,
        )

        return BenchmarkRunResult(
            tasks=tasks,
            results_a=results_a,
            results_b=results_b,
            results_c=results_c,
            agg_a=agg_a,
            agg_b=agg_b,
            agg_c=agg_c,
            gate_result=gate_result,
            json_report_path=json_path,
            md_report_path=md_path,
        )
