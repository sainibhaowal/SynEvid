# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-10-08

### Added
- **In-Session Agent Lifecycle, TypeScript MCP Server & Web Inspector (Phase 4.1):**
  - **Model Context Protocol (MCP) Server (`apps/mcp-server`):**
    - Out-of-process TypeScript server implementing official MCP SDK v2 (`@modelcontextprotocol/server` v2.3.1).
    - 5 mandatory read-only tools: `autopsy_status`, `autopsy_impact`, `autopsy_diff`, `autopsy_verify`, and `autopsy_explain`.
    - Automated Node.js integration tests asserting stdio CLI subprocess execution, tool registration, and typed parameter validation.
    - Verified architectural inversion (`ARCH_NO_CORE_TO_MCP`): core Rust crates maintain 0 dependencies on MCP or web layers.
  - **Web Inspector Evidence Board (`apps/web-inspector`):**
    - Standalone, zero-backend, read-only single-page web app for interactive forensic inspection.
    - Interactive Evidence Board visualizing hypotheses, supporting evidence cards with file/line provenance, and contradicting signal checks.
    - Transitive Blast Radius and dependency multigraph explorer showing caller hierarchy and downstream API risk.
    - Drag-and-drop local `autopsy.json` artifact loader for instant telemetry analysis.
  - **In-Session Agent Lifecycle Protocol (`tests/tests/e2e_agent_lifecycle.rs`):**
    - Integration test suite (3 tests) validating the closed-loop agent workflow: `Baseline A -> Impact Assessment -> Edit A to B -> Verify A vs B -> (On FAIL: Repair via Evidence -> Re-verify PASS)`.
    - Detection of breaking contract parameter weakening and architectural boundary violations.
  - **Baseline Debt Management & Waivers (`.autopsy/baseline.json`, CA-TECH-001 Ch.8):**
    - Schema validation via committed `schemas/baseline.schema.json` and `.autopsy/baseline.json`.
    - Verification engine support for `--baseline-id` and accepted debt fingerprints, distinguishing pre-existing legacy violations from new PR regressions.
    - Integration test `test_cli_verify_baseline_accepted_debt_waiver` verifying waiver handling and exit code conversion.
    - Added TypeScript MCP server typechecking and unit test execution into `.pre-commit-config.yaml` and `.github/workflows/ci.yml`.
- **Multi-Platform Hardening & Enterprise CI/CD (Phase 3.2):**
  - Universal hardware & architecture support: Linux (`x86_64`, `aarch64`), macOS Apple Silicon (`aarch64-apple-darwin`), macOS Intel (`x86_64-apple-darwin`), and Windows (`x86_64-pc-windows-msvc`).
  - Cross-platform path separator normalization (`/`) in `autopsy-repo` scanner, ensuring byte-identical BLAKE3 digests and snapshot IDs across Linux, macOS, and Windows.
  - Cross-platform CRLF vs LF line-ending normalization (`normalize_line_endings`) in `autopsy-repo`, guaranteeing byte-identical content hashes regardless of git checkout line endings.
  - Concurrent atomic cache write resilience in `autopsy-storage` with PID-isolated staging and collision-safe rename handling on Windows NTFS and POSIX filesystems.
  - Integration test suite `e2e_cross_platform_and_multi_arch.rs` (4 tests) asserting CRLF/LF invariance, path normalization, concurrent cache writes, and BLAKE3 boundary vectors.
  - Dedicated verification script `scripts/verify-cross-platform.sh` integrated into `Makefile` and `pre-commit` hooks.
  - Upgraded GitHub Actions CI/CD suite:
    - Pre-merge multi-platform matrix (`.github/workflows/ci.yml`) across Ubuntu, macOS (Apple Silicon), and Windows runners with concurrency cancellation (`cancel-in-progress: true`), 93 tests, and rich GITHUB_STEP_SUMMARY quality gate tables.
    - Post-merge release pipeline (`.github/workflows/post-merge.yml`) executing serially on `main` (`cancel-in-progress: false`), verifying golden baselines, compiling native release binaries across Linux (`x86_64`), macOS (`aarch64`, `x86_64`), and Windows (`x86_64`), packaging archives with licensing metadata, computing `SHA256SUMS.txt`, and uploading consolidated release bundles.
  - **Enterprise Sandboxing & Defense-in-Depth (Pillar C):**
    - Zero Code Execution (FR-026): In-memory AST parsing only, zero sub-process execution (`std::process::Command` = 0, `fork`/`exec` = 0), verified via architecture boundary gate.
    - Symlink Escape Containment (INV-SANDBOX-001): Explicit `follow_links(false)` on `WalkBuilder`, symlink entry filtering (`entry.path_is_symlink()`), and canonical root boundary checks.
    - Path Traversal Sanitization (INV-SANDBOX-003): Function `sanitize_relative_path` and `is_safe_relative_path` rejecting `..` traversal sequences and absolute paths across file scanning, `roots`, and `cache.directory`.
    - Resource Bounds & DOS Prevention (FR-008): Enforced `max_traversal_nodes` and impact budget ceilings guaranteeing linear termination on adversarial dense cyclic graphs.
    - Zero-Network Isolation (FR-028, INV-SANDBOX-002): Verified 100% offline analysis with zero networking crates and zero socket calls.
    - Integration test suite `tests/tests/e2e_enterprise_sandboxing_and_defense.rs` (5 tests) bringing total workspace test count to 100 tests.
  - Phase 3.2 Master Plan and Engineering Blueprints (`Resource/3.2/`).
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
- **Impact, Contracts, Invariants & Evidence Engine (Phase 4):**
  - `autopsy-impact`: Bounded forward/backward BFS graph traversal, traversal profiles (`edge_kinds`, `max_depth`, `budget`), deterministic shortest-path ranking, SCC cycle condensation, budget truncation tracking, and BLAKE3 impact digests.
  - `autopsy-contracts`: Normalized callable and interface contract models for TypeScript, formal backward compatibility rules per kind, breaking change detection, and cryptographic contract digests.
  - `autopsy-invariants`: YAML invariant policy loader (`.autopsy/invariants.yml`), deterministic evaluators for forbidden dependencies (`forbidden-dep`), layered architecture constraints (`layers`), cycle introduction prevention (`no-new-cycle`), and API backward compatibility (`api-compat`), strictly enforcing the `UNKNOWN != PASS` boundary.
  - `autopsy-evidence`: Immutable evidence receipts with canonical ordering of locations, paths, snapshots, and deltas before computing BLAKE3 digests, providing complete tamper verification.
  - E2E integration test suite (`tests/tests/e2e_phase4_impact_and_invariants.rs`) asserting golden fixtures, 100-run determinism, and `UNKNOWN != PASS` invariance.
- **CLI, Storage & Reporting Engine (Phase 5):**
  - `autopsy-storage`: Embedded SQLite storage with versioned auto-migrations, thread-safe connection pooling, optional WAL mode, and content-addressed `.autopsy/cache/objects/xx/yy...` with atomic write-and-rename guarantees.
  - `autopsy-report`: Reporting engine with canonical indented JSON strictly matching `schemas/autopsy-result.schema.json v0.0.1`, formatted human-readable terminal text with line-span badges, and OASIS SARIF 2.1.0 output for CI / GitHub code scanning.
  - `autopsy-cli`: Unified binary CLI providing commands `baseline`, `diff`, `impact`, `verify`, `explain`, `doctor`, and `version` with pure machine-readable `stdout`, diagnostic `stderr`, deterministic exit codes (0 pass, 2 policy failure, 3 analysis error, 4 unsupported/unknown), and offline network-off execution guarantee.
  - E2E integration test suite (`tests/tests/e2e_phase5_cli_and_storage.rs`) asserting all 7 CLI commands, exit code mapping, storage roundtrips, offline isolation, and 100-run determinism.
- **Benchmark Harness & Evaluation Suite (Phase 6):**
  - Standardized benchmark corpus with 5 TypeScript repositories (`benchmarks/corpus/`: `repo-api`, `repo-migration`, `repo-relayer`, `repo-cross-package`, `repo-dead-call`).
  - 50 deterministic benchmark tasks (`benchmarks/tasks/`: `task_001.json` - `task_050.json`) spanning 5 categories (`api_change`, `migration`, `relayer`, `cross_package`, `dead_call`) strictly conforming to `schemas/benchmark-task.schema.json`, including partitioned held-out validation tasks.
  - Three comparative baselines: Baseline A (agent + grep text search), Baseline B (agent + LSP 1-hop AST references), and Baseline C (agent + Autopsy deterministic multigraph reachability).
  - Benchmark evaluation engine (`benchmarks/engine/`) calculating recall, precision, F1, tool calls, tokens, latency, and structural failure class discovery.
  - Pre-registered gate verification (01 Ch.12): **PASSED** with +31.73pp recall lift (threshold $\ge +10\text{pp}$), 56.90% cost reduction (threshold $\ge 40\%$), and 16 newly discovered structural failure classes.
  - Automated reproduction report generation (`benchmarks/results/reproduction_report.json` and `reproduction_report.md`).
  - E2E integration test suite (`tests/tests/e2e_phase6_benchmarks.rs`) asserting corpus integrity, task schema conformance, and gate passage.
- **Engine Core Verification Architecture:**
  - 14 Rust crates implementing foundation pipeline (`autopsy-repo`, `autopsy-domain`, `autopsy-symbols`, `autopsy-graph`, `autopsy-diff`, `autopsy-impact`, `autopsy-invariants`, `autopsy-evidence`, `autopsy-storage`, `autopsy-contracts`, `autopsy-report`, `autopsy-cli`).
  - Cryptographic BLAKE3 100-run snapshot determinism invariant test suite.
  - Strict architectural boundary verification script enforcing Zero LLM in correctness crates.

## [0.0.1] - 2026-09-29
- Repository architecture and public schema starters.
- Foundation schema contracts and ADR records.
