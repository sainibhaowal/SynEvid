#!/usr/bin/env python3
"""
Synevid Phase 6 Benchmark Harness CLI.
Executes 50 benchmark tasks across 5 TypeScript repositories evaluating Baselines A, B, and C.
Evaluates pre-registered gate (>=10pp recall lift OR >=40% cost cut at equal accuracy OR new failure class).
"""

import argparse
from pathlib import Path
import sys

# Ensure repository root is on sys.path
REPO_ROOT = Path(__file__).resolve().parent.parent
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from benchmarks.engine.runner import BenchmarkRunner


def parse_args():
    parser = argparse.ArgumentParser(description="Synevid Benchmark Evaluation Suite (Phase 6)")
    parser.add_argument(
        "--tasks-dir",
        type=Path,
        default=REPO_ROOT / "benchmarks" / "tasks",
        help="Path to tasks directory",
    )
    parser.add_argument(
        "--corpus-dir",
        type=Path,
        default=REPO_ROOT / "benchmarks" / "corpus",
        help="Path to corpus repositories directory",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=REPO_ROOT / "benchmarks" / "results",
        help="Path to output results directory",
    )
    parser.add_argument(
        "--autopsy-bin",
        type=Path,
        default=REPO_ROOT / "target" / "debug" / "autopsy",
        help="Path to compiled autopsy CLI binary",
    )
    parser.add_argument(
        "--category",
        type=str,
        default=None,
        choices=["api_change", "migration", "relayer", "cross_package", "dead_call"],
        help="Filter tasks by category",
    )
    parser.add_argument(
        "--held-out-only",
        action="store_true",
        help="Evaluate only held-out validation tasks",
    )
    parser.add_argument(
        "--check-gate",
        action="store_true",
        default=True,
        help="Exit with non-zero status if pre-registered gate fails",
    )
    return parser.parse_args()


def main():
    args = parse_args()

    print("=" * 80)
    print(" SYNEVID PHASE 6 BENCHMARK HARNESS (FR-025, UC-08, RQ1-5)")
    print("=" * 80)
    print(f"Tasks Directory:   {args.tasks_dir}")
    print(f"Corpus Directory:  {args.corpus_dir}")
    print(f"Output Directory:  {args.output_dir}")
    print(f"Autopsy Binary:    {args.autopsy_bin}")
    if args.category:
        print(f"Category Filter:   {args.category}")
    if args.held_out_only:
        print("Set Filter:        Held-out tasks only")
    print("=" * 80)

    runner = BenchmarkRunner(
        tasks_dir=args.tasks_dir,
        corpus_dir=args.corpus_dir,
        output_dir=args.output_dir,
        autopsy_binary=args.autopsy_bin,
    )

    try:
        run_res = runner.run(
            category_filter=args.category,
            held_out_only=args.held_out_only,
        )
    except Exception as e:
        print(f"ERROR: Benchmark execution failed: {e}", file=sys.stderr)
        sys.exit(1)

    print("\nBENCHMARK RESULTS SUMMARY:")
    print("-" * 80)
    print(f"{'Baseline':<28} | {'Recall':<8} | {'Precision':<10} | {'F1':<6} | {'Tokens':<8} | {'Tool Calls':<10}")
    print("-" * 80)
    for agg in [run_res.agg_a, run_res.agg_b, run_res.agg_c]:
        print(
            f"{agg.baseline:<28} | "
            f"{agg.mean_recall * 100:>6.2f}% | "
            f"{agg.mean_precision * 100:>8.2f}% | "
            f"{agg.mean_f1 * 100:>5.2f}% | "
            f"{agg.mean_tokens:>7.0f} | "
            f"{agg.mean_tool_calls:>10.2f}"
        )
    print("-" * 80)

    gate = run_res.gate_result
    print("\nPRE-REGISTERED GATE STATUS (01 Ch.12):")
    print(f"  Decision:              {'[ PASS ]' if gate.passed else '[ FAIL - STOP ]'}")
    print(f"  Recall Lift:           +{gate.recall_lift_pp:.2f}pp (Threshold: >= +10.0pp) -> {'PASS' if gate.gate_1_recall_lift_passed else 'FAIL'}")
    print(f"  Cost Cut:              {gate.cost_cut_percent:.2f}% (Threshold: >= 40.0% at equal accuracy) -> {'PASS' if gate.gate_2_cost_cut_passed else 'FAIL'}")
    print(f"  New Failure Classes:   {gate.new_failure_classes_found} detected -> {'PASS' if gate.gate_3_new_failure_class_passed else 'FAIL'}")
    print(f"  Rationale:             {gate.rationale}")
    print("-" * 80)
    print(f"Reproduction JSON Report: {run_res.json_report_path}")
    print(f"Reproduction Markdown:    {run_res.md_report_path}")
    print("=" * 80)

    if args.check_gate and not gate.passed:
        print("PRE-REGISTERED GATE FAILED: Execution stopped per 01 Ch.12", file=sys.stderr)
        sys.exit(2)

    sys.exit(0)


if __name__ == "__main__":
    main()
