# SynEvid (Code Autopsy) — Public Engineering & Documentation Guide

Welcome to **SynEvid (Code Autopsy)**, an offline, mathematically deterministic software verification and change-impact engine engineered in pure Rust.

This document is the **canonical public starting point** for developers, platform architects, security auditors, and AI agent engineers exploring the repository and its documentation. The agent-facing surface — one evidence envelope, `autopsy find`/`map`/`session`, snapshot pinning, the machine-readable toolspec, the MCP server, and the `synevid-lsp` editor surface — is summarized in [Agent Tool Reference](agents/TOOLS.md) and verified tier-by-tier in [Evidence 35–38](evidence/38-agent-gate-closure.md).

---

## 1. Documentation Sitemap & Reading Paths

Depending on your role and interest, follow the designated reading path below:

```
                                docs/START_HERE.md (You Are Here)
                                                |
        +-----------------------+---------------+-----------------------+
        |                       |               |                       |
        v                       v               v                       v
[Architecture & Design]   [Requirements & Trace] [Safety & Invariants]  [Evidence & Receipts]
docs/architecture/OVERVIEW.md  docs/requirements/TRACEABILITY.md  docs/security/THREAT_MODEL.md  docs/evidence/01..39
        |                       |               |                       |
        v                       v               v                       v
   ADR-0001..0009         schemas/*.json   scripts/verify-arch-*.sh  docs/benchmark/PROTOCOL.md
```

### Path A: System Architecture & Design
1. [**Architecture Overview**](architecture/OVERVIEW.md): Multi-tiered evidence plane, 6-pillar system design, and crate dependency topology.
2. [**Architectural Decision Records (ADRs)**](adr/):
   - [ADR-0001: Pure Rust Core Engine](adr/0001-rust-core.md)
   - [ADR-0002: TypeScript MCP Server Adapter](adr/0002-typescript-mcp.md)
   - [ADR-0003: Python Research-Only Benchmark Harness](adr/0003-python-benchmark-only.md)
   - [ADR-0004: Local SQLite Cache & Storage Journal](adr/0004-local-sqlite-cache.md)
   - [ADR-0005: Zero LLM in Correctness Crates](adr/0005-no-llm-core.md)
   - [ADR-0006: Presentation Layer Architectural Inversion](adr/0006-gui-later.md)
   - [ADR-0007: Language Adapter Strategy](adr/0007-one-language-first.md)
   - [ADR-0008: SARIF 2.1.0 Static Analysis Export](adr/0008-sarif-export.md)
   - [ADR-0009: Symbol Identity & Shape Hash Split](adr/0009-symbol-identity-shape-split.md)
3. [**Comparative Landscape Analysis**](COMPARISON.md): SynEvid vs CodeQL, Sourcegraph, Semgrep, and LSP.

### Path B: Requirements & Verification Traceability
1. [**Traceability Matrix**](requirements/TRACEABILITY.md): Full bidirectional mapping of requirements (FR-001..FR-030, FR-ADV-001..030, NFR-001, Invariants) to concrete crates and unit/integration tests.
2. [**Public JSON Schemas**](../schemas/): Formal schemas defining the contracts for snapshots, diffs, impacts, evidence receipts, and multi-source catalogs.

### Path C: Security, Sandboxing & Invariants
1. [**Threat Model & Sandbox Defense**](security/THREAT_MODEL.md): Analysis of hostile repositories, path traversal containment, and untrusted AST attacks.
2. [**Hardware & Architectural Boundaries**](../scripts/verify-arch-boundaries.sh): Script enforcing zero LLM, zero network sockets, and zero subprocess execution.

### Path D: Empirical Benchmarks & Production Evidence
1. [**Benchmark Protocol**](benchmark/PROTOCOL.md): Pre-registered evaluation methodology across 50 refactoring tasks and 5 TypeScript target repositories.
2. [**Evidence Dossier (39 Production Receipts)**](evidence/): Chronological, mathematically backed logs from Phase 0 through the Tier-1–3 agent-experience build-out and the install lifecycle (e.g., [13: Forensic Closure](evidence/13-forensic-benchmark-closure.md), [17: Temporal Graph](evidence/17-temporal-semantic-graph.md), [18: Test Impact](evidence/18-predictive-test-selection.md), [33: How to Verify](evidence/33-verification-procedures.md), [35: Tier-1 Agent Experience](evidence/35-tier1-agent-experience.md), [38: Acceptance-Gate Closure](evidence/38-agent-gate-closure.md), [39: Install Lifecycle](evidence/39-install-update-lifecycle.md)).

### Path E: AI Agent Surfaces & Integration
1. [**Agent Tool Reference**](agents/TOOLS.md): the one envelope, the exit-code contract, all 22 subcommands, snapshot pinning, token budgets, the 11 MCP tools and 6 resources, environment variables, and the `synevid-lsp` editor surface (Tier 1–3).
2. [**Agent recipes**](../agents/recipes/repair-loop.md): step-by-step playbooks — [repair loop](../agents/recipes/repair-loop.md), [pinned evidence](../agents/recipes/pinned-evidence.md), [PR gate](../agents/recipes/pr-gate.md), [multi-repo](../agents/recipes/multi-repo.md).
3. [**Agent skill & hooks**](../agents/skills/synevid-forensics.md): opt-in host guidance, session-receipt hook ([`autopsy-session.sh`](../agents/hooks/autopsy-session.sh)), and the advisory fail-open [safe-edit hook](../agents/hooks/safe-edit.sh). Index: [Agent Documentation](agents/README.md).
4. [**Tier evidence & gate closure**](evidence/38-agent-gate-closure.md): what each tier added and how it is verified (35–38), including the protocol-level MCP agent-loop tests.

### Path F: Install & Operate (users)
1. [**One-line install / update / uninstall**](../README.md#8-developer--agent-workflow): per-user install under `~/.local` (one install serves every repository), checksum-verified release downloads, managed PATH block, a bundled single-file MCP server, automatic agent-host wiring — update by re-running the same command; `--check` and `--uninstall` supported. Releases are refreshed from every green `main` post-merge run.
2. [**Offline self-test of that path**](../scripts/tests/test-install.sh): `bash scripts/tests/test-install.sh` installs from a fake release into a throwaway HOME — no network, no Rust — and asserts install, checksum-abort, update idempotency, `--check`, and `--uninstall`. It runs in the pre-commit gate and in CI.

---

## 2. The Six Evolution Pillars Overview

SynEvid is organized around six core evolutionary pillars implemented in pure Rust (`crates/*`):

| Pillar | Focus | Implementation Crate | Primary CLI Command |
| :--- | :--- | :--- | :--- |
| **Pillar 1: Temporal Semantic Graph** | Commit evolution journal, `valid_from`/`valid_to` intervals, stable symbol lineage across renames. | [`autopsy-temporal`](../crates/autopsy-temporal) | `autopsy lineage`, `autopsy temporal` |
| **Pillar 2: Predictive Test Impact** | Test-to-symbol mapping, LCOV ingestion, 0% false-negative affected test selection, PTS advisory ranking. | [`autopsy-test-impact`](../crates/autopsy-test-impact) | `autopsy test-select` |
| **Pillar 3: Distributed Contracts** | OpenAPI 3.2.1, AsyncAPI 3.1.0, Pact 4.0.0 breaking change diff, global URN identities. | [`autopsy-contracts-distributed`](../crates/autopsy-contracts-distributed) | `autopsy contract-dist` |
| **Pillar 4: Runtime-Static Fusion** | OpenTelemetry SemConv 1.44 trace ingestion, static AST span correlation, runtime topology overlay. | [`autopsy-telemetry`](../crates/autopsy-telemetry) | `autopsy telemetry` |
| **Pillar 5: Hidden-Coupling Discovery**| Sliding-window Git co-change mining, statistical confidence/lift, non-causal association tagging. | [`autopsy-mining`](../crates/autopsy-mining) | `autopsy cochange` |
| **Pillar 6: Enterprise Platform** | Backstage `catalog-info.yaml`, multi-source dependency manifests, unified blast-radius traversal views. | [`autopsy-catalog`](../crates/autopsy-catalog) | `autopsy catalog`, `autopsy blast-radius` |

---

## 3. Quick Start: Independent Verification in 60 Seconds

You can independently verify the entire codebase directly from your terminal:

### Step 1: Clone & Verify Rust MSRV
SynEvid requires modern Rust (MSRV 1.90+ for tree-sitter language bindings and stable let-chains):
```bash
cargo --version
```

### Step 2: Run Automated Architectural Invariant Verification
Verifies that no core crate contains LLM dependencies, network sockets, or subprocess calls:
```bash
./scripts/verify-arch-boundaries.sh
```

### Step 3: Run 100-Run BLAKE3 Determinism Test
Verifies that 100 consecutive full snapshot calculations produce byte-identical BLAKE3 hashes:
```bash
./scripts/verify-determinism.sh
```

### Step 4: Run the Complete Test Suite (924 workspace tests)
Runs all unit, integration, fuzz, and schema conformance suites (the MCP server adds 44 protocol-level tests via `npm test`, including the shipped single-file bundle):
```bash
cargo test --workspace --all-targets
```

### Step 5: Open the Web Inspector (zero-build, tracked)
The inspector is a single tracked HTML file — no build step, no server:
```bash
xdg-open apps/web-inspector/index.html   # macOS: open ...   Windows: start ...
npm --prefix apps/web-inspector test     # headless render tests in the gate
```

---

## 4. Key CLI Commands Reference

SynEvid operates via a unified CLI (`autopsy`):

| Command | Purpose | Strict Mode Exit Codes |
| :--- | :--- | :---: |
| `autopsy baseline` | Takes an immutable cryptographic snapshot of the repository AST. | `0` (Success) |
| `autopsy diff` | Computes semantic symbol deltas and breaking contract changes between snapshots. | `0` (Compatible), `2` (Breaking) |
| `autopsy impact` | Calculates the bounded transitive call-graph blast radius for a given symbol. | `0` (Verified), `4` (Refusal / Unknown) |
| `autopsy verify` | Mathematically evaluates architectural layers, cycles, and invariants. | `0` (Pass), `2` (Policy Violation), `3` (Error) |
| `autopsy lineage` | Traces a symbol's identity across file renames and moves over Git history. | `0` (Verified), `4` (Ambiguous) |
| `autopsy catalog` | Discovers Backstage components, team ownership, and dependency manifests. | `0` (Verified), `4` (Unregistered) |
| `autopsy explain` | Provides an evidence trace and recommended agent repair strategy for any finding. | `0` (Informational) |
| `autopsy find` | Resolves a partial name/path/ID to ranked stable symbol IDs (Tier 1.1). | `0` (Verified), `3` (Blank pattern) |
| `autopsy map` | Bounded whole-graph projection: counts plus top-N ranked nodes (Tier 1.3). | `0` (Verified), `4` (`--strict`, unknown coverage) |
| `autopsy session` | Repair-loop receipts: `start` / `record` / `show` / `close` / `list` (Tier 2.5). | `0` (Pass), `2` (Fail), `3` (Refused) |
| `autopsy tools` | Emits the machine-readable toolspec for agents (Tier 3.1). | `0` (Success) |

All 22 subcommands — including `symbols`, `edges`, `workspace`, `temporal`, `test-select`, `contract-dist`, `telemetry`, `cochange`, `blast-radius`, `doctor`, and `version` — are documented in [Agent Tool Reference](agents/TOOLS.md) and emitted by `autopsy tools --format json`.
