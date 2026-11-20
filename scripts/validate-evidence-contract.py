#!/usr/bin/env python3
"""Validate the shared evidence schema and its non-success boundary."""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCHEMA = ROOT / "schemas" / "evidence-envelope.schema.json"


def main() -> int:
    schema = json.loads(SCHEMA.read_text(encoding="utf-8"))
    required = set(schema["required"])
    expected = {
        "operation_id",
        "repository_id",
        "canonical_path",
        "snapshot_id",
        "coverage_state",
        "verdict",
        "warnings",
        "latency_ms",
        "blake3_digest",
        "evidence_receipt",
    }
    if required != expected:
        raise SystemExit(f"evidence schema required keys mismatch: {sorted(required)}")
    states = set(schema["properties"]["verdict"]["enum"])
    forbidden_success = {"UNKNOWN", "PARTIAL", "ERROR", "UNAVAILABLE"}
    if not forbidden_success <= states:
        raise SystemExit("evidence schema does not expose all non-success truth states")
    if schema["properties"]["blake3_digest"]["pattern"] != r"^blake3:[0-9a-f]{64}$":
        raise SystemExit("BLAKE3 digest format is not strict")
    print("Validated shared evidence envelope schema and truth-state boundary.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
