#!/usr/bin/env python3
"""Validate the machine-readable cross-phase requirement registry."""

from __future__ import annotations

import re
import sys
from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[1]
REGISTRY = ROOT / "docs" / "requirements" / "registry.yaml"
TRACEABILITY = ROOT / "docs" / "requirements" / "TRACEABILITY.md"
VALID_STATES = {"PLANNED", "IMPLEMENTED", "PARTIAL", "BLOCKED", "VERIFIED"}


def fail(message: str) -> None:
    print(f"ERROR: requirement registry: {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> int:
    try:
        document = yaml.safe_load(REGISTRY.read_text(encoding="utf-8"))
    except Exception as exc:  # pragma: no cover - exercised by CI on malformed YAML
        fail(f"cannot parse {REGISTRY}: {exc}")

    if not isinstance(document, dict) or document.get("version") != 1:
        fail("version must be 1")
    requirements = document.get("requirements")
    if not isinstance(requirements, list) or not requirements:
        fail("requirements must be a non-empty list")

    traceability = TRACEABILITY.read_text(encoding="utf-8")
    seen: set[str] = set()
    for index, requirement in enumerate(requirements):
        if not isinstance(requirement, dict):
            fail(f"entry {index} is not a mapping")
        missing = [
            key
            for key in ("id", "source", "owner", "acceptance_test", "evidence", "status")
            if not str(requirement.get(key, "")).strip()
        ]
        if missing:
            fail(f"entry {index} is missing: {', '.join(missing)}")
        requirement_id = str(requirement["id"])
        if requirement_id in seen:
            fail(f"duplicate ID: {requirement_id}")
        seen.add(requirement_id)
        if requirement["status"] not in VALID_STATES:
            fail(f"{requirement_id}: invalid status {requirement['status']!r}")
        source = str(requirement["source"])
        source_file = source.split("#", 1)[0]
        if not (ROOT / source_file).exists():
            fail(f"{requirement_id}: source file does not exist: {source_file}")
        if not (ROOT / str(requirement["acceptance_test"])).exists():
            fail(f"{requirement_id}: acceptance test does not exist")
        if not (ROOT / str(requirement["evidence"])).exists():
            fail(f"{requirement_id}: evidence file does not exist")

        # Registry IDs must be visible in the public traceability matrix.  This
        # prevents a private registry entry from silently diverging from release docs.
        if requirement_id not in traceability:
            fail(f"{requirement_id}: missing from TRACEABILITY.md")

    # A registry entry is not allowed to be an unowned wildcard.
    if any(re.fullmatch(r"[A-Z0-9._/-]+", item) is None for item in seen):
        fail("all IDs must be stable ASCII requirement identifiers")
    print(f"Validated {len(requirements)} requirements with complete ownership, test, evidence, and status metadata.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
