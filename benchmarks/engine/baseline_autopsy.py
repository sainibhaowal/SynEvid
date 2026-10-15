"""
Baseline C: Agent + Autopsy (SynEvid Deterministic Verification Engine).

Executes the real compiled engine binary per task (no simulation):
  baseline -> impact (bidirectional closure from task seeds) -> verify.

Predicted symbols are the engine's own `impacted_entities`, normalized
from stable identities (`path#method:C.m`) to qualified names
(`path::C::m`) for comparison against ground truth. Engine `file::`
and `symbol::` reference nodes have no qualified-name counterpart and
are excluded (documented recall limitation, counted honestly).

Failure-class vocabulary differs by design: the engine reports rule
ids (`api-compatibility`, `no-circular-deps`, ...); task ground truth
uses change classes (`breaking_contract_required_param`, ...). Detected
rule ids are reported verbatim; mapping to change classes is NOT
invented, so cross-vocabulary classes count as missed.

Corpus checkouts are never mutated: SQLite + cache live in a temp dir
via --db-path / --cache-dir (engine itself is read-only over source).
"""

import json
import re
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

from benchmarks.engine.metrics import TaskResult, compute_task_metrics
from benchmarks.engine.task_loader import BenchmarkTask

ENGINE_TIMEOUT_S = 120

# Stable identity -> qualified name, mirroring autopsy-symbols formats.
# `file::path` module nodes map to their file path (ground truth uses
# file granularity for some tasks); `symbol::name` references have no
# counterpart and are dropped.
_ID_RE = re.compile(r"^(?P<path>.+)#(?P<tag>[a-z]+):(?P<rest>.+)$")
_OVERLOAD_RE = re.compile(r":overload:\d+$")


def normalize_engine_id(entity_id: str) -> str | None:
    if entity_id.startswith("file::"):
        return entity_id[len("file::") :]
    m = _ID_RE.match(entity_id)
    if not m:
        # `file::...` / `symbol::...` reference nodes: no qualified name.
        return None
    tag = m.group("tag")
    rest = _OVERLOAD_RE.sub("", m.group("rest"))
    path = m.group("path")
    if tag == "method" and "." in rest:
        cls, name = rest.split(".", 1)
        if cls and name:
            return f"{path}::{cls}::{name}"
        return None
    if tag in ("fn", "class", "interface", "type", "enum", "variable") and rest:
        return f"{path}::{rest}"
    return None


class BaselineAutopsy:
    """
    Baseline C execution engine (real binary, measured I/O).
    """

    def __init__(self, corpus_root: Path, autopsy_binary: Path):
        self.corpus_root = corpus_root
        self.autopsy_binary = Path(autopsy_binary)

    def _run(
        self, repo_dir: Path, db_path: Path, cache_dir: Path, args: list[str]
    ) -> tuple[int, str, str]:
        cmd = [
            str(self.autopsy_binary),
            "--repo-root",
            str(repo_dir),
            "--db-path",
            str(db_path),
            "--cache-dir",
            str(cache_dir),
            "--format",
            "json",
        ] + args
        try:
            proc = subprocess.run(
                cmd,
                capture_output=True,
                timeout=ENGINE_TIMEOUT_S,
                check=False,
                text=True,
            )
            return proc.returncode, proc.stdout, proc.stderr
        except (subprocess.TimeoutExpired, OSError) as exc:
            return 3, "", f"harness: engine invocation failed: {exc}"

    def evaluate_task(self, task: BenchmarkTask) -> TaskResult:
        start_time = time.perf_counter()
        repo_dir = self.corpus_root / task.repository

        predicted_symbols: set[str] = set()
        detected_failure_classes: list[str] = []
        tool_calls = 0
        output_bytes = 0
        notes: list[str] = []

        scratch = Path(tempfile.mkdtemp(prefix="autopsy-bench-"))
        db_path = scratch / "storage.db"
        cache_dir = scratch / "cache"

        try:
            if not self.autopsy_binary.exists():
                notes.append("engine binary missing; no prediction possible")
            elif not repo_dir.is_dir():
                notes.append(f"corpus repository missing: {task.repository}")
            else:
                # Tool call 1: baseline (validates the repo analyzes cleanly).
                rc, out, _ = self._run(repo_dir, db_path, cache_dir, ["baseline"])
                tool_calls += 1
                output_bytes += len(out.encode("utf-8"))
                if rc != 0:
                    notes.append(f"baseline exited {rc}")

                # Tool calls 2..N: impact closure per seed. Direction is
                # BIDIRECTIONAL: task families span callers (api_change),
                # callees (relayer forward flows) and layer-spanning
                # migration paths, so the complete blast radius is the
                # honest comparison point.
                for seed in task.ground_truth.seed_symbols:
                    rc, out, _ = self._run(
                        repo_dir,
                        db_path,
                        cache_dir,
                        [
                            "impact",
                            "-s",
                            seed,
                            "--direction",
                            "bidirectional",
                        ],
                    )
                    tool_calls += 1
                    output_bytes += len(out.encode("utf-8"))
                    if rc != 0:
                        notes.append(f"impact exited {rc} for seed {seed}")
                        continue
                    try:
                        entities = json.loads(out)["impact"]["impacted_entities"]
                    except (json.JSONDecodeError, KeyError):
                        notes.append("impact output unparsable")
                        continue
                    for entity in entities:
                        normalized = normalize_engine_id(str(entity))
                        if normalized is not None:
                            predicted_symbols.add(normalized)

                # Final tool call: single-state invariant verification.
                # Deliberately WITHOUT --baseline-id: there is no stored
                # before-state for a corpus checkout, and comparing a
                # state against itself would manufacture vacuous passes.
                # (Two-state compat is covered by e2e_forensic_closure.)
                rc, out, _ = self._run(repo_dir, db_path, cache_dir, ["verify"])
                tool_calls += 1
                output_bytes += len(out.encode("utf-8"))
                if rc in (0, 2):
                    try:
                        findings = json.loads(out)["findings"]
                        detected_failure_classes = sorted(
                            {str(f["rule_id"]) for f in findings if f.get("status") == "fail"}
                        )
                    except (json.JSONDecodeError, KeyError):
                        notes.append("verify output unparsable")
                else:
                    notes.append(f"verify exited {rc}")
        finally:
            shutil.rmtree(scratch, ignore_errors=True)

        elapsed_ms = (time.perf_counter() - start_time) * 1000.0
        # Measured evidence bytes (~4 bytes/token) + fixed agent schema cost.
        total_tokens = 320 + output_bytes // 4

        gt_symbols = set(task.ground_truth.expected_impacted_symbols)

        return compute_task_metrics(
            task_id=task.id,
            baseline="Baseline-C (Agent+Autopsy)",
            category=task.category,
            is_held_out=task.is_held_out,
            predicted=predicted_symbols,
            ground_truth=gt_symbols,
            tool_calls=tool_calls,
            tokens=total_tokens,
            time_ms=elapsed_ms,
            expected_failure_classes=task.ground_truth.expected_failure_classes,
            detected_failure_classes=detected_failure_classes,
            notes="; ".join(notes) or "real engine run: baseline + bidirectional impact + verify",
        )
