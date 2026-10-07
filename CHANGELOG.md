# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-10-08

### Added
- **Community & Governance Infrastructure:**
  - `SECURITY.md` establishing vulnerability disclosure protocol, response timelines, and core security invariants (Zero LLM, Read-Only, Offline-first).
  - `NOTICE` adhering to Apache-2.0 copyright and third-party attribution specifications.
  - `CODE_OF_CONDUCT.md` adopting Contributor Covenant v2.1.
  - Comprehensive `CONTRIBUTING.md` defining deterministic verification protocol, non-negotiable hard boundaries, and pull request procedures.
  - GitHub issue templates (`.github/ISSUE_TEMPLATE/`) for bug reports, feature requests, invariant violation alerts, and security guidance.
  - GitHub pull request template (`.github/PULL_REQUEST_TEMPLATE.md`) with determinism and architecture checklist.
- **Language Adapter Architecture (Phase 2):**
  - `autopsy-adapter-api`: Versioned `LanguageAdapter` trait, `AdapterCapabilities`, `ParsedFile`, `DynamicConstruct`, and reusable conformance test suite `verify_adapter_conformance`.
  - `autopsy-adapter-typescript`: Production-grade tree-sitter bootstrap (TS & TSX), TypeScript compiler bridge for relative and package module resolution, stable `SymbolId` calculation resilient to line shifts, typed multigraph edge extraction (`Contains`, `Imports`, `Inherits`, `Implements`), normalized callable and interface contract extraction, and honest coverage state emission (`Verified`, `Partial`, `Unknown`).
  - E2E integration test suite (`tests/tests/e2e_adapter_typescript_conformance.rs`) verifying conformance, real multi-file repository parsing, cross-file imports, and 100-run determinism.
- **Dependency Multigraph & Semantic Diff Engine (Phase 3):**
  - `autopsy-graph`: Petgraph-backed directed multigraph, edge provenance tracking (adapter, location, method), Tarjan SCC cycle detection, bounded BFS reachability (forward and backward), incremental file updates ($O(|V_{file}| + |E_{file}|)$), and BLAKE3 graph digest hashing.
  - `autopsy-diff`: Semantic diff engine with file deltas (add, del, modify, rename), symbol deltas (add, remove, move, rename, body, signature, visibility, kind), contract deltas with breaking change detection, edge deltas with coverage state tracking, changed frontier calculation, and strict Ambiguous Rename Rule enforcement.
  - E2E conformance test suite (`tests/tests/e2e_graph_and_diff_conformance.rs`) covering cycles, reachability, ambiguous rename safety, golden diff fixtures, and 100-run determinism.
- **Engine Core Verification Architecture:**
  - 14 Rust crates implementing foundation pipeline (`autopsy-repo`, `autopsy-domain`, `autopsy-symbols`, `autopsy-graph`, `autopsy-diff`, `autopsy-impact`, `autopsy-invariants`, `autopsy-evidence`, `autopsy-storage`, `autopsy-contracts`, `autopsy-report`, `autopsy-cli`).
  - Cryptographic BLAKE3 100-run snapshot determinism invariant test suite.
  - Strict architectural boundary verification script enforcing Zero LLM in correctness crates.

## [0.0.1] - 2026-09-29
- Repository architecture and public schema starters.
- Foundation schema contracts and ADR records.
