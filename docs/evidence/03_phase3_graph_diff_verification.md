# Phase 3 Verification Evidence: Dependency Graph & Semantic Diff Engine

**Date:** 2026-10-08  
**Author:** Synevid Systems Engineer (AI Pair)  
**Scope:** Phase 3 — Graph + Diff (`autopsy-graph`, `autopsy-diff`, FR-006, FR-007)  
**Status:** **PASSED & VERIFIED** (All 8 Pre-Commit Gates Green, 52/52 Tests Passing)

---

## 1. Executive Summary

Phase 3 implements the forensic dependency multigraph representation and deterministic semantic change-impact diff engine for **Synevid (Code Autopsy)**:
- **`crates/autopsy-graph` (FR-006):** Petgraph-backed directed multigraph supporting multiple typed edges (`Contains`, `Imports`, `References`, `Calls`, `Implements`, `Inherits`) between node pairs, explicit edge provenance (`adapter`, `location`, `method`), Tarjan's SCC cycle detection, bounded forward/backward reachability, incremental file updates ($O(|V_{file}| + |E_{file}|)$), and BLAKE3 graph digesting.
- **`crates/autopsy-diff` (FR-007):** Full semantic difference engine supporting file deltas (`Added`, `Deleted`, `Modified`, `Renamed`), symbol deltas (`Added`, `Removed`, `Moved`, `Renamed`, `BodyModified`, `SignatureModified`, `VisibilityModified`, `KindModified`), contract deltas with breaking change detection, edge deltas with coverage transition tracking, changed frontier calculation (direct symbols + 1-hop impact boundary), and the non-negotiable **Ambiguous Rename Rule** (ambiguous renames strictly remain `Added` + `Deleted` / `Added` + `Removed`).
- **`tests/tests/e2e_graph_and_diff_conformance.rs`:** End-to-end integration and golden diff fixture test suite asserting 100/100 determinism runs and exact JSON output match.

---

## 2. Architectural Invariants & Implementation Details

### A. Directed Multigraph with Edge Provenance (`autopsy-graph`, FR-006)
- **Multigraph Capability:** Leverages `petgraph::graph::DiGraph<Symbol, Edge>` with a secondary index `BTreeMap<SymbolId, NodeIndex>` and file index `BTreeMap<String, BTreeSet<SymbolId>>`.
- **Edge Provenance (FR-006):** Retains origin details for every edge, including adapter language, AST source location, extraction pass, and rule provenance.
- **Cycle Detection:** Employs Tarjan's Strongly Connected Components algorithm (`petgraph::algo::tarjan_scc`) to detect cyclic dependencies and self-loops deterministically.
- **Incremental Updates:** `update_file` invalidates and updates only symbols and edges belonging to the modified file without triggering an $O(V^2)$ full graph rebuild.

### B. Semantic Diff Engine (`autopsy-diff`, FR-007)
- **File Deltas:** Accurately classifies file additions, deletions, modifications, and renames.
- **Ambiguous Rename Safeguard:** If multiple deleted files share a content hash with an added file, or multiple added files match a single deleted file, the engine strictly outputs `Deleted` + `Added` (never guessing).
- **Symbol Deltas:** Distinguishes moves across files, signature modifications, body range expansions, visibility changes, and unambiguous symbol renames within scopes. Ambiguous symbol candidates remain `Removed` + `Added`.
- **Contract Compatibility (FR-007, FR-009):** Detects breaking changes when public visibility is narrowed, input parameter types or counts change, or return types change.
- **Frontier Calculation:** Automatically computes the directly changed symbols, the affected files, and the 1-hop dependent symbols forming the immediate blast radius boundary.

---

## 3. Test Execution Telemetry

### Workspace Verification Suite (52/52 Tests Passing)
```bash
$ cargo test --workspace --all-targets -- --nocapture

running 9 tests (autopsy-adapter-typescript) ... ok
running 4 tests (autopsy-diff) ... ok
running 4 tests (autopsy-domain) ... ok
running 5 tests (autopsy-graph) ... ok
running 5 tests (autopsy-repo) ... ok
running 4 tests (e2e_adapter_typescript_conformance) ... ok
running 6 tests (e2e_config_validation) ... ok
running 2 tests (e2e_domain_canonical_invariants) ... ok
running 5 tests (e2e_graph_and_diff_conformance) ... ok
running 4 tests (e2e_repo_scan_and_ignores) ... ok
running 4 tests (e2e_snapshot_determinism) ... ok

test result: ok. 52 passed; 0 failed; 0 ignored
```

### Pre-Commit Multi-Stack Matrix (All 8 Gates Passed)
```bash
$ pre-commit run --all-files

Rust Format Check (rustfmt).....................................................Passed
Rust Clippy Strict Lints (-D warnings)..........................................Passed
Workspace Test Stack (Unit & Integration).......................................Passed (52/52)
Rust Documentation Tests........................................................Passed
Golden 100-Run Determinism Invariant............................................Passed
JSON Schema Syntactic Validation................................................Passed
Engine Config & Invariant Syntax Validation.....................................Passed
Architectural Boundary Gate (Zero LLM/MCP in Core Crates).......................Passed
```

---

## 4. Requirements Traceability Verification

| Requirement ID | Description | Crates / Tests | Status |
|---|---|---|---|
| **FR-006** | Petgraph-backed typed multigraph & provenance | `autopsy-graph`, `e2e_graph_and_diff_conformance` | **VERIFIED** |
| **FR-007** | Semantic diff engine & ambiguous rename rule | `autopsy-diff`, `e2e_graph_and_diff_conformance` | **VERIFIED** |
