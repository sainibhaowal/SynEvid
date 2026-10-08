"""
Task loader and schema validator for Synevid Phase 6 benchmark harness.
"""

from dataclasses import dataclass, field
import json
from pathlib import Path
from typing import Any, Dict, List, Optional


@dataclass(frozen=True)
class GroundTruth:
    seed_symbols: List[str]
    expected_impacted_symbols: List[str]
    expected_failure: bool
    expected_failure_classes: List[str]
    expected_coverage: str

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "GroundTruth":
        return cls(
            seed_symbols=list(data.get("seed_symbols", [])),
            expected_impacted_symbols=list(data.get("expected_impacted_symbols", [])),
            expected_failure=bool(data.get("expected_failure", False)),
            expected_failure_classes=list(data.get("expected_failure_classes", [])),
            expected_coverage=str(data.get("expected_coverage", "verified")),
        )


@dataclass(frozen=True)
class BenchmarkTask:
    id: str
    repository: str
    base_ref: str
    prompt: str
    category: str
    is_held_out: bool
    ground_truth: GroundTruth
    raw_data: Dict[str, Any] = field(default_factory=dict, repr=False)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "BenchmarkTask":
        for req in ["id", "repository", "base_ref", "prompt", "ground_truth"]:
            if req not in data:
                raise ValueError(f"Missing required field '{req}' in benchmark task")

        gt = GroundTruth.from_dict(data["ground_truth"])
        category = str(data.get("category", "unknown"))
        is_held_out = bool(data.get("is_held_out", False))

        return cls(
            id=data["id"],
            repository=data["repository"],
            base_ref=data["base_ref"],
            prompt=data["prompt"],
            category=category,
            is_held_out=is_held_out,
            ground_truth=gt,
            raw_data=data,
        )


def load_task_file(path: Path) -> BenchmarkTask:
    with open(path, "r", encoding="utf-8") as f:
        data = json.load(f)
    return BenchmarkTask.from_dict(data)


def load_all_tasks(tasks_dir: Path) -> List[BenchmarkTask]:
    task_files = sorted(tasks_dir.glob("task_*.json"))
    tasks: List[BenchmarkTask] = []
    for tf in task_files:
        tasks.append(load_task_file(tf))
    return tasks
