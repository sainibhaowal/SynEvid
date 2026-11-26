"""Fail-closed integrity checks for public benchmark claims."""

import ast
from pathlib import Path


class BenchmarkNotEvaluableError(RuntimeError):
    """Raised when the task oracle or comparator setup cannot support claims."""


def audit_benchmark(tasks_dir: Path, engine_dir: Path | None = None) -> list[str]:
    """Return blockers that make precision/recall or promotion claims invalid.

    A task must specify its query semantics and whether its impact labels are
    exhaustive. A comparator must not read the expected-answer list while
    constructing predictions. Until these conditions hold, the benchmark may
    not emit comparative quality scores or a passing promotion gate.
    """
    blockers: list[str] = []
    task_paths = sorted(tasks_dir.glob("task_*.json"))
    if not task_paths:
        return [f"no benchmark task files found in {tasks_dir}"]

    import json

    for task_path in task_paths:
        try:
            task = json.loads(task_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            blockers.append(f"{task_path.name}: unreadable task definition: {exc}")
            continue

        query = task.get("query")
        if (
            not isinstance(query, dict)
            or not isinstance(query.get("direction"), str)
            or not query["direction"].strip()
            or not isinstance(query.get("edge_kinds"), list)
            or not query["edge_kinds"]
        ):
            blockers.append(
                f"{task_path.name}: query direction and edge-kind scope are not specified"
            )

        ground_truth = task.get("ground_truth")
        if (
            not isinstance(ground_truth, dict)
            or ground_truth.get("oracle_complete") is not True
        ):
            blockers.append(
                f"{task_path.name}: exhaustive oracle completeness is not established"
            )
        review = task.get("oracle_review")
        if (
            not isinstance(review, dict)
            or not isinstance(review.get("reviewers"), list)
            or len({name for name in review["reviewers"] if isinstance(name, str)}) < 2
            or review.get("adjudicated") is not True
            or not isinstance(review.get("evidence"), list)
            or not review["evidence"]
        ):
            blockers.append(
                f"{task_path.name}: independent two-reviewer oracle record is missing"
            )

    root = engine_dir or Path(__file__).resolve().parent
    for filename in ("baseline_grep.py", "baseline_lsp.py"):
        path = root / filename
        try:
            source = path.read_text(encoding="utf-8")
            tree = ast.parse(source, filename=str(path))
        except (OSError, SyntaxError) as exc:
            blockers.append(f"{filename}: cannot audit comparator source: {exc}")
            continue

        answer_key_fields = {
            "expected_impacted_symbols",
            "expected_failure",
            "expected_failure_classes",
            "expected_coverage",
        }
        leaked_fields = sorted(
            {
                node.attr
                for node in ast.walk(tree)
                if isinstance(node, ast.Attribute) and node.attr in answer_key_fields
            }
        )
        if leaked_fields:
            blockers.append(
                f"{filename}: comparator implementation reads answer-key fields: "
                + ", ".join(leaked_fields)
            )

        has_lsp_reference_request = (
            "textdocument/references" in source.lower()
            and "jsonrpc" in source.lower()
        )
        if filename == "baseline_lsp.py" and not has_lsp_reference_request:
            blockers.append(
                "baseline_lsp.py: no LSP textDocument/references JSON-RPC request is present; "
                "the comparator is a text heuristic, not an LSP baseline"
            )

    return blockers


def require_evaluable_benchmark(tasks_dir: Path, engine_dir: Path | None = None) -> None:
    blockers = audit_benchmark(tasks_dir, engine_dir)
    if blockers:
        details = "\n".join(f"- {blocker}" for blocker in blockers)
        raise BenchmarkNotEvaluableError(
            "benchmark is NOT EVALUABLE; refusing comparative scores and promotion gate:\n"
            + details
        )
