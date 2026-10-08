"""
Metrics computation module for Synevid Phase 6 benchmark harness.
Computes Recall, Precision, F1, FN, FP, Tool Calls, Tokens, Time, and failure class detection.
"""

from dataclasses import dataclass, field
from typing import Dict, List, Set


@dataclass
class TaskResult:
    task_id: str
    baseline: str
    category: str
    is_held_out: bool
    predicted_symbols: Set[str]
    ground_truth_symbols: Set[str]
    true_positives: int
    false_positives: int
    false_negatives: int
    precision: float
    recall: float
    f1: float
    tool_calls: int
    tokens: int
    time_ms: float
    detected_failure_classes: List[str]
    missed_failure_classes: List[str]
    notes: str = ""

    def to_dict(self) -> Dict[str, object]:
        return {
            "task_id": self.task_id,
            "baseline": self.baseline,
            "category": self.category,
            "is_held_out": self.is_held_out,
            "predicted_count": len(self.predicted_symbols),
            "ground_truth_count": len(self.ground_truth_symbols),
            "tp": self.true_positives,
            "fp": self.false_positives,
            "fn": self.false_negatives,
            "precision": round(self.precision, 4),
            "recall": round(self.recall, 4),
            "f1": round(self.f1, 4),
            "tool_calls": self.tool_calls,
            "tokens": self.tokens,
            "time_ms": round(self.time_ms, 2),
            "detected_failure_classes": self.detected_failure_classes,
            "missed_failure_classes": self.missed_failure_classes,
            "notes": self.notes,
        }


@dataclass
class AggregateMetrics:
    baseline: str
    task_count: int
    total_tp: int
    total_fp: int
    total_fn: int
    mean_precision: float
    mean_recall: float
    mean_f1: float
    total_tool_calls: int
    mean_tool_calls: float
    total_tokens: int
    mean_tokens: float
    total_time_ms: float
    mean_time_ms: float
    failure_classes_detected: int
    failure_classes_missed: int

    def to_dict(self) -> Dict[str, object]:
        return {
            "baseline": self.baseline,
            "task_count": self.task_count,
            "total_tp": self.total_tp,
            "total_fp": self.total_fp,
            "total_fn": self.total_fn,
            "mean_precision": round(self.mean_precision, 4),
            "mean_recall": round(self.mean_recall, 4),
            "mean_f1": round(self.mean_f1, 4),
            "total_tool_calls": self.total_tool_calls,
            "mean_tool_calls": round(self.mean_tool_calls, 2),
            "total_tokens": self.total_tokens,
            "mean_tokens": round(self.mean_tokens, 2),
            "total_time_ms": round(self.total_time_ms, 2),
            "mean_time_ms": round(self.mean_time_ms, 2),
            "failure_classes_detected": self.failure_classes_detected,
            "failure_classes_missed": self.failure_classes_missed,
        }


def compute_task_metrics(
    task_id: str,
    baseline: str,
    category: str,
    is_held_out: bool,
    predicted: Set[str],
    ground_truth: Set[str],
    tool_calls: int,
    tokens: int,
    time_ms: float,
    expected_failure_classes: List[str],
    detected_failure_classes: List[str],
    notes: str = "",
) -> TaskResult:
    tp = len(predicted.intersection(ground_truth))
    fp = len(predicted - ground_truth)
    fn = len(ground_truth - predicted)

    precision = tp / (tp + fp) if (tp + fp) > 0 else (1.0 if not ground_truth else 0.0)
    recall = tp / (tp + fn) if (tp + fn) > 0 else 1.0
    f1 = (2 * precision * recall) / (precision + recall) if (precision + recall) > 0 else 0.0

    missed_fc = [fc for fc in expected_failure_classes if fc not in detected_failure_classes]

    return TaskResult(
        task_id=task_id,
        baseline=baseline,
        category=category,
        is_held_out=is_held_out,
        predicted_symbols=predicted,
        ground_truth_symbols=ground_truth,
        true_positives=tp,
        false_positives=fp,
        false_negatives=fn,
        precision=precision,
        recall=recall,
        f1=f1,
        tool_calls=tool_calls,
        tokens=tokens,
        time_ms=time_ms,
        detected_failure_classes=detected_failure_classes,
        missed_failure_classes=missed_fc,
        notes=notes,
    )


def compute_aggregate_metrics(baseline: str, results: List[TaskResult]) -> AggregateMetrics:
    if not results:
        return AggregateMetrics(
            baseline=baseline,
            task_count=0,
            total_tp=0,
            total_fp=0,
            total_fn=0,
            mean_precision=0.0,
            mean_recall=0.0,
            mean_f1=0.0,
            total_tool_calls=0,
            mean_tool_calls=0.0,
            total_tokens=0,
            mean_tokens=0.0,
            total_time_ms=0.0,
            mean_time_ms=0.0,
            failure_classes_detected=0,
            failure_classes_missed=0,
        )

    n = len(results)
    total_tp = sum(r.true_positives for r in results)
    total_fp = sum(r.false_positives for r in results)
    total_fn = sum(r.false_negatives for r in results)
    mean_precision = sum(r.precision for r in results) / n
    mean_recall = sum(r.recall for r in results) / n
    mean_f1 = sum(r.f1 for r in results) / n
    total_tool_calls = sum(r.tool_calls for r in results)
    mean_tool_calls = total_tool_calls / n
    total_tokens = sum(r.tokens for r in results)
    mean_tokens = total_tokens / n
    total_time_ms = sum(r.time_ms for r in results)
    mean_time_ms = total_time_ms / n
    detected_fc = sum(len(r.detected_failure_classes) for r in results)
    missed_fc = sum(len(r.missed_failure_classes) for r in results)

    return AggregateMetrics(
        baseline=baseline,
        task_count=n,
        total_tp=total_tp,
        total_fp=total_fp,
        total_fn=total_fn,
        mean_precision=mean_precision,
        mean_recall=mean_recall,
        mean_f1=mean_f1,
        total_tool_calls=total_tool_calls,
        mean_tool_calls=mean_tool_calls,
        total_tokens=total_tokens,
        mean_tokens=mean_tokens,
        total_time_ms=total_time_ms,
        mean_time_ms=mean_time_ms,
        failure_classes_detected=detected_fc,
        failure_classes_missed=missed_fc,
    )
