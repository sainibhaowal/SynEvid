# SynEvid System Architecture & Engineering Blueprint

This document details the architectural design, dependency boundaries, multi-tiered evidence plane, and core deterministic guarantees of **SynEvid (Code Autopsy)**.

---

## 1. High-Level Architectural Diagram

SynEvid enforces a strict **Architectural Inversion** principle: presentation, integration, and AI agent layers depend upon core public Rust interfaces, never the reverse (`ARCH_NO_CORE_TO_MCP`).

```
+-------------------------------------------------------------------------------------------------+
|                                 1. PRESENTATION & INTEGRATION LAYER                              |
|   CLI (`autopsy`) | Web Inspector (`apps/web-inspector`) | MCP Server v2                          |
|   IDE Surface: LSP stdio server (`apps/synevid-lsp`, inline impact hovers)                       |
+------------------------------------------------+------------------------------------------------+
                                                 | (Public Rust APIs & JSON Schemas)
                                                 v
+-------------------------------------------------------------------------------------------------+
|                                    2. MODULAR EVIDENCE PLANE                                    |
|                                                                                                 |
|   +-----------------------+   +-----------------------+   +---------------------------------+   |
|   | Pillar 1: Temporal    |   | Pillar 2: Predictive  |   | Pillar 3: Distributed Contracts |   |
|   | Semantic Graph        |   | Test Impact (PTS)     |   | (OpenAPI, AsyncAPI, Pact)       |   |
|   | (`autopsy-temporal`)  |   | (`autopsy-test-impact`)|   | (`autopsy-contracts-dist`)      |   |
|   +-----------------------+   +-----------------------+   +---------------------------------+   |
|   | Pillar 4: Runtime-    |   | Pillar 5: Hidden      |   | Pillar 6: Enterprise Multi-     |   |
|   | Static Fusion (OTel)  |   | Coupling Mining       |   | Source Catalog                  |   |
|   | (`autopsy-telemetry`) |   | (`autopsy-mining`)    |   | (`autopsy-catalog`)             |   |
|   +-----------------------+   +-----------------------+   +---------------------------------+   |
+------------------------------------------------+------------------------------------------------+
                                                 |
                                                 v
+-------------------------------------------------------------------------------------------------+
|                               3. CORE DETERMINISTIC ENGINE (Zero LLM)                           |
|                                                                                                 |
|   +---------------------+   +---------------------+   +---------------------+   +-----------+   |
|   |    Domain Models    |   |  AST Parsing (Tree- |   |   Call Multigraph   |   |  Storage  |   |
|   |  (`autopsy-domain`, |   |  sitter TS & Py)    |   |   & Bounded Impact  |   |  Journal  |   |
|   |  `autopsy-symbols`) |   |  (`autopsy-adapter`)|   | (`autopsy-graph/imp`)|  (`storage`) |   |
|   +---------------------+   +---------------------+   +---------------------+   +-----------+   |
|   | Invariants & Policy |   | Semantic Diff Engine|   | Evidence Receipts   |   | Reporting |   |
|   | (`autopsy-invariant`|   |  (`autopsy-diff`)   |   | (`autopsy-evidence`)|   | (`report`)|   |
|   +---------------------+   +---------------------+   +---------------------+   +-----------+   |
+-------------------------------------------------------------------------------------------------+
```

### 1.1 Agent Plane at a Glance (Tier 1–3)

The layer above the engine is exercised the way agents exercise it — over the MCP
stdio wire protocol — by the acceptance-gate tests in
`apps/mcp-server/test/` (`e2e_agent_find_and_envelope.test.js`,
`e2e_agent_session.test.js`). Every answer, CLI and MCP alike, travels in the same
envelope: `{command, exit_code, actionable, report, repair?}`.

| Surface | What ships | Where |
|---|---|---|
| **MCP server** | 11 read-only tools (`status`, `impact`, `diff`, `verify`, `explain`, `find`, `temporal`, `test_selection`, `contract_diff`, `distributed_blast_radius`, `session`) + 6 resources (`graph`, `graph/map`, `graph/symbols`, `graph/edges`, `workspace`, `tools`) | `apps/mcp-server` |
| **CLI agent commands** | `find` (symbol resolution), `map` (bounded whole-graph projection), `tools` (self-description), `session` (repair-loop receipts), `workspace` (multi-repo registry) | `crates/autopsy-cli` |
| **Editor surface** | `synevid-lsp` — hover returns signature, declaration, callers/callees, 1-hop impact, and coverage | `apps/synevid-lsp` |
| **Repair advice** | every error carries `{what_failed, why, next_action}` parsed from the engine's own stderr markers | `autopsy-cli` → MCP envelope |
| **Determinism & budget knobs** | `AUTOPSY_SNAPSHOT` pin · `--snapshot-id` per call · `--compact`/`--top-k` · `AUTOPSY_TIMEOUT_MS` · `AUTOPSY_MAX_BUFFER_BYTES` | CLI + MCP |

Full contract: [`docs/agents/TOOLS.md`](../agents/TOOLS.md). Verification:
[`docs/evidence/38-agent-gate-closure.md`](../evidence/38-agent-gate-closure.md)
and the `AGENT-T1.*`–`AGENT-GATE-1` rows in
[`docs/requirements/TRACEABILITY.md`](../requirements/TRACEABILITY.md).

---

## 2. Core Invariants & Boundaries

Every crate within `crates/*` is bound by five non-negotiable architectural axioms:

1. **Zero LLM in Correctness Crates (`crates/*`):**
   - Correctness, blast-radius calculation, invariant evaluation, and test selection are 100% deterministic code. Core crates never import or make calls to LLM APIs.
   - Verified by: [`./scripts/verify-arch-boundaries.sh`](../../scripts/verify-arch-boundaries.sh).

2. **100/100 Deterministic Repeatability (NFR-001, FR-028):**
   - Runs across identical inputs produce byte-identical BLAKE3 output digests.
   - Canonical serializations strictly prohibit unordered map iteration (`HashMap`, `HashSet`); all collections use ordered `BTreeMap` and `BTreeSet`.
   - Verified by: [`./scripts/verify-determinism.sh`](../../scripts/verify-determinism.sh).

3. **Read-Only Analyzer Engine (FR-026):**
   - SynEvid strictly inspects source state, git commit logs, and change deltas; it **never mutates source code, branches, or git history**.

4. **Honest Semantic Coverage (`UNKNOWN != PASS`):**
   - Incomplete AST models, dynamic imports, unanalyzed reflection, or sampled traces emit explicit `CoverageState::Unknown` or `Partial`.
   - In strict mode (`--strict`), uncertainty is never converted into an optimistic passing gate.

5. **Universal Hardware & Sandboxing Defense:**
   - Strict path normalization (`/`) and line-ending determinism (CRLF vs LF) across Linux, macOS, and Windows.
   - Symlink containment (`follow_links(false)`), path traversal sanitization (rejection of `..`), zero sub-process spawning, and zero outbound network sockets.

---

## 3. Crate Topology & Responsibilities

The monorepo contains 28 core Rust crates (plus the `synevid-lsp` presentation app and the workspace integration-test crate `tests/`), organized by strict layered dependency:

### Core Foundation Layer
- **`autopsy-domain`**: Canonical domain types (`SnapshotId`, `SymbolId`, `CoverageState`, `GlobalEntityId`, `EvidenceReceipt`).
- **`autopsy-symbols`**: Stable symbol identity extraction resilient to line number drift (ADR-0009).
- **`autopsy-repo`**: Secure repository discovery, `.gitignore` parsing, symlink escape prevention, and BLAKE3 snapshot digesting.
- **`autopsy-storage`**: SQLite migration engine, content-addressed blob cache, and local state store.

### Parsing & Adaptation Layer
- **`autopsy-adapter-api`**: Language-agnostic adapter trait definitions and capability declarations.
- **`autopsy-adapter-typescript`**: Tree-sitter powered TypeScript/JavaScript AST visitor and contract extractor.
- **`autopsy-adapter-python`**: Tree-sitter powered Python module, class, function, and import extractor.
- **`autopsy-adapter-go`**: Tree-sitter powered Go extractor — receiver-typed methods, struct/interface embedding, filesystem-anchored imports, honest `Unknown` for cgo/plugin/reflect (Tier 2.2).
- **`autopsy-adapter-rust`**: Tree-sitter powered Rust extractor — module bridge, impl/trait edges, `unsafe`/`async` contract effects, honest `Unknown` for `include!` inlining (Tier 2.2).

### Graph & Verification Layer
- **`autopsy-graph`**: Directed multigraph with edge provenance, cycle detection (Tarjan's SCC), and reachability traversals.
- **`autopsy-impact`**: Bounded BFS blast radius calculation with call-graph pruning and caller ranking.
- **`autopsy-diff`**: Semantic change delta engine classifying symbol additions, modifications, and removals.
- **`autopsy-contracts`**: Callable and interface contract breaking change evaluation.
- **`autopsy-invariants`**: Policy evaluation engine enforcing layer rules, forbidden dependencies, and acyclic architectures.
- **`autopsy-evidence`**: Cryptographic evidence receipt generator with canonical BLAKE3 content-addressing.
- **`autopsy-report`**: Formatter for human-readable terminal output, canonical JSON, and SARIF 2.1.0 static analysis reports.

### Advanced Evolution Layer (Pillars 1–6)
- **`autopsy-temporal`**: Pure-Rust Git object store (loose/packed), commit walker, and symbol lineage engine.
- **`autopsy-test-impact`**: Test inventory discovery, LCOV coverage ingestion, conservative test selection, and advisory PTS ranking.
- **`autopsy-contracts-distributed`**: OpenAPI 3.2.1, AsyncAPI 3.1.0, and Pact 4.0.0 breaking change diff engine.
- **`autopsy-telemetry`**: OpenTelemetry SemConv 1.44 trace sanitizer, PII redaction, and runtime service topology graph.
- **`autopsy-mining`**: Sliding-window Git co-change miner calculating support, confidence, and lift without claiming causation.
- **`autopsy-hidden-coupling`**: Conservative extractors for environment variables, config keys, message-queue topics, and database table names — every report carries `CoverageState::Partial` and a non-causality disclaimer.
- **`autopsy-catalog`**: Backstage `catalog-info.yaml` and multi-ecosystem package manifest ingestion engine.

### Research & Federation Layer
- **`autopsy-behavior-research`**: Confidence-scored behavioral research analyzers (FR-ADV-023/024); never emits a hard PASS without a formal proof.
- **`autopsy-incidents`**: Read-only incident correlation with candidate ranking only — never claims root cause (FR-ADV-025/026).
- **`autopsy-schema-registry`**: Pluggable schema-registry compatibility policies over field sets (FR-ADV-015).
- **`autopsy-workspace-registry`**: Read-only multi-repo / tenant federation metadata and honest live accessibility status (FR-ADV-010..012).
- **`autopsy-cli`**: Unified CLI application orchestrating all core engine commands and deterministic exit codes.

---

## 4. Public JSON Schemas & Verification

All structured outputs emitted by SynEvid conform to public JSON schemas in [`schemas/`](../../schemas/):
- `autopsy-result.schema.json` (canonical result envelopes and receipts)
- `baseline.schema.json` (baseline snapshot records)
- `benchmark-task.schema.json` (ground-truth refactoring tasks)
- `blast-radius.schema.json` (multi-system blast radius views)
- `catalog-report.schema.json` (Backstage catalog & package manifests)
- `cochange-report.schema.json` (Git co-change mining and associations)
- `distributed-contract.schema.json` (OpenAPI, AsyncAPI, Pact diffs)
- `invariant.schema.json` (layer rule and invariant policies)
- `lineage.schema.json` (symbol evolution and rename history)
- `telemetry-fusion.schema.json` (static-runtime trace correlation)
- `temporal.schema.json` (commit intervals and semantic evolution)
- `test-selection.schema.json` (predictive test selection matrices)
- `topology-report.schema.json` (service dependency topology)

Every schema is exercised by automated conformance test suites in `tests/schema_conformance.rs` asserting bidirectional serialization and required field validation.
