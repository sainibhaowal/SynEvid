---
name: synevid-core-engineer
description: Canonical engineering rules, architecture invariants, determinism standards, and verification workflow for the Synevid (Code Autopsy) engine.
---

# Synevid Engineering Skill & Protocol

This skill encodes the mandatory standards, non-negotiable boundaries, and verification cycle for all development on **Synevid**.

---

## 1. Non-Negotiable Core Boundaries

1. **Zero LLM in Correctness Path:**
   - Pass/fail verification, invariant evaluation, graph traversal, and contract deltas are 100% deterministic code.
   - Core crates (`crates/*`) must never import or call LLM APIs.
2. **Offline & Zero Network I/O:**
   - Engine commands must succeed in an air-gapped environment with no internet access.
   - Core analysis crates must make zero network requests.
3. **Read-Only Engine (FR-026):**
   - Analysis commands (`baseline`, `diff`, `impact`, `verify`, `explain`) must NEVER mutate source code or git history.
4. **100/100 Reproducible Determinism (NFR-001, FR-028):**
   - 100 repeated runs on identical inputs must produce byte-identical BLAKE3 result digests.
   - Timestamps are strictly excluded from semantic identity (`SnapshotId`, `result_digest`).
   - Forbid iterating unordered collections (`std::collections::HashMap`, `HashSet`) in canonical paths. Use `BTreeMap` and `BTreeSet` or sorted vectors.
5. **Coverage Honesty (FR-014, FR-030):**
   - Unsupported constructs (reflection, dynamic imports, generated code) must emit explicit `CoverageState::Unknown` or `CoverageState::Partial`.
   - Never collapse `UNKNOWN` into `PASS`.
6. **Ground Truth Integrity:**
   - Never modify benchmark ground truth simply to increase benchmark scores.
7. **Clean Dependency Inversion:**
   - Presentation, GUI, and MCP layers depend on core public interfaces, NEVER the reverse.

---

## 2. Mandatory Pre-Commit & Verification Loop

Before any commit or merge, execute the full verification chain:

```bash
# 1. Format check
cargo fmt --all -- --check

# 2. Strict lints
cargo clippy --workspace --all-targets -- -D warnings

# 3. All workspace unit & integration tests
cargo test --workspace

# 4. JSON Schema validation
for s in schemas/*.json; do python3 -m json.tool "$s" >/dev/null; done

# 5. Golden determinism assertion (100 runs)
cargo test -p autopsy-repo -- test_compute_snapshot_id_golden_100_runs
```

---

## 3. Architecture & Traceability Mapping

When implementing features:
1. Identify affected requirements in [`docs/requirements/TRACEABILITY.md`](file:///home/ravi/Projects/SynEvid/docs/requirements/TRACEABILITY.md).
2. Follow established Architecture Decision Records in [`docs/adr/`](file:///home/ravi/Projects/SynEvid/docs/adr/).
3. Add end-to-end integration tests in [`tests/`](file:///home/ravi/Projects/SynEvid/tests) or crate unit tests.
4. Document all changes and verification evidence in [`docs/evidence/`](file:///home/ravi/Projects/SynEvid/docs/evidence/).
