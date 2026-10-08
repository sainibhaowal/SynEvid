<p align="center">
  <a href="https://github.com/sainibhaowal/SynEvid">
    <img src="assets/logo.svg" alt="SynEvid Logo" width="130">
  </a>
</p>

<h1 align="center">SynEvid</h1>

<p align="center">
  <strong>Deterministic Software Verification & Change-Impact Engine</strong><br>
  <em>Offline Ground-Truth Verification for Autonomous AI Coding Agents & Safety-Critical Systems</em>
</p>

<p align="center">
  <a href="https://github.com/sainibhaowal/SynEvid">
    <img src="assets/banner.jpg" alt="SynEvid Banner" width="100%">
  </a>
</p>

<p align="center">
  <a href="https://github.com/sainibhaowal/SynEvid/stargazers"><img src="https://img.shields.io/github/stars/sainibhaowal/SynEvid?style=for-the-badge&logo=github&color=00f0ff" alt="GitHub Stars"></a>
  <a href="https://github.com/sainibhaowal/SynEvid/network/members"><img src="https://img.shields.io/github/forks/sainibhaowal/SynEvid?style=for-the-badge&logo=github&color=7928ca" alt="GitHub Forks"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-Apache%202.0-blue.svg?style=for-the-badge" alt="License: Apache 2.0"></a>
  <a href="SECURITY.md"><img src="https://img.shields.io/badge/Security-Policy%20Active-00f0ff?style=for-the-badge" alt="Security Policy"></a>
  <a href="#deterministic-pipeline"><img src="https://img.shields.io/badge/Determinism-100%2F100%20BLAKE3-00f0ff?style=for-the-badge&logo=rust" alt="100/100 Determinism"></a>
  <a href="#hard-boundaries"><img src="https://img.shields.io/badge/Core%20Engine-Zero%20LLM%20%2F%20Offline-9d00ff?style=for-the-badge" alt="Zero LLM in Core"></a>
  <a href="#quality-matrix"><img src="https://img.shields.io/badge/Rust-1.85%2B%20(2024%20Edition)-orange?style=for-the-badge&logo=rust" alt="Rust 2024"></a>
  <a href="#test-coverage"><img src="https://img.shields.io/badge/Verification-84%2F84%20Tests%20Passing-00e676?style=for-the-badge" alt="Tests Passing"></a>
</p>

---

## 1. Overview & Core Philosophy

**SynEvid** (internally known as *Code Autopsy*) is an offline, mathematically deterministic software verification and change-impact engine.

### What SynEvid Is
SynEvid is **not** a chatbot, code synthesizer, or LLM wrapper. It is a forensic verification oracle engineered in Rust to calculate exact semantic blast radius, enforce architectural invariants, and generate verifiable cryptographic evidence receipts across code revisions.

Frontier AI coding agents (Claude, Codex, Gemini, Cursor) produce non-deterministic proposals and cannot reliably self-verify subtle architectural regressions. SynEvid acts as their external, deterministic verification ground truth:

$$\text{Baseline Snapshot } A \xrightarrow[\text{Impact Query}]{\text{Blast Radius}} \text{Agent Edit } A \to B \xrightarrow[\text{Invariant Check}]{\text{Verify}} \text{Pass / Evidence Receipt}$$

---

## 2. Hard Boundaries & Invariants

SynEvid enforces strict architectural axioms (governed by [`AGENTS.md`](AGENTS.md) and [`docs/requirements/TRACEABILITY.md`](docs/requirements/TRACEABILITY.md)):

* **Zero LLM in Correctness Crates:** The core engine (`crates/*`) never imports or calls LLM APIs. Invariants, graph traversals, and contract checks are 100% deterministic code.
* **100/100 Deterministic Repeatability (NFR-001, FR-028):** Every run on identical inputs produces byte-identical BLAKE3 digests. No timestamps or unordered hash-map iterations exist in canonical serialization paths (strictly using `BTreeMap` and `BTreeSet`).
* **Read-Only Analyzer Engine (FR-026):** Commands strictly inspect repository state and change deltas; they **never mutate source code or git history**.
* **Honest Semantic Coverage (FR-014, FR-030):** Unsupported or dynamic constructs (reflection, dynamic imports, generated code) emit explicit `CoverageState::Unknown` or `Partial`. **`UNKNOWN` never collapses into `PASS`**.
* **Clean Architectural Inversion (`ARCH_NO_CORE_TO_MCP`):** Presentation, GUI, and MCP layers depend on core public interfaces, never the reverse.

---

## 3. Deterministic Pipeline Architecture

```mermaid
flowchart TD
    Repo["Git Repository / Source Root"] --> Discover["autopsy-repo<br/>(Ignore-Aware Scan)"]
    Discover --> Snap["RepoSnapshot<br/>BLAKE3(Files + Config + Analyzer)"]
    Snap --> Sym["autopsy-symbols<br/>(Stable Symbol Identifiers)"]
    Sym --> Graph["autopsy-graph<br/>(Typed Dependency Multigraph)"]
    Graph --> Diff["autopsy-diff<br/>(Semantic Symbol & Contract Delta)"]
    Diff --> Impact["autopsy-impact<br/>(Bounded BFS / SCC Condensation)"]
    Impact --> Inv["autopsy-invariants<br/>(Policy Evaluation: Pass/Fail/Unknown)"]
    Inv --> Evid["autopsy-evidence<br/>(Cryptographic Receipts & Exact Paths)"]
    Evid --> CLI["CLI / Canonical JSON / MCP Engine"]
```

---

## 4. Foundation Command Interface

```bash
# Snapshot repository baseline state into SQLite & content-addressed cache
autopsy baseline --format json

# Compute semantic change-set between snapshots or against worktree
autopsy diff --before <snapshot_id> --after <snapshot_id> --format json

# Traverse bounded blast radius for a symbol with ranked shortest paths
autopsy impact -s "src/index.ts::main" --direction forward --max-depth 5

# Verify architectural policies and contract compatibility (exit: 0 pass, 2 fail, 4 unknown)
autopsy verify --strict --invariants-file .autopsy/invariants.yml

# Trace exact evidence derivation, line spans, and paths for a finding
autopsy explain <finding_id>

# Run environment, storage integrity, adapter, and offline diagnostics
autopsy doctor

# Print engine version and adapter capabilities
autopsy version
```

---

## 5. Monorepo Crate Structure

```
SynEvid/
├── crates/
│   ├── autopsy-domain/             # Core typed IDs, entities, BTreeMap canonical models
│   ├── autopsy-repo/               # Discovery, ignore rules, BLAKE3 digest computation
│   ├── autopsy-adapter-api/        # Extensible language AST & symbol parser traits
│   ├── autopsy-adapter-typescript/ # TypeScript AST compiler & tree-sitter bridge
│   ├── autopsy-symbols/            # Stable symbol identities across line shifts
│   ├── autopsy-graph/              # Petgraph-backed typed multigraph with edge provenance
│   ├── autopsy-diff/               # Semantic symbol, edge, and contract deltas
│   ├── autopsy-impact/             # Bounded traversal, SCC cycle condensation, ranking
│   ├── autopsy-contracts/          # Callable/interface normalized contract models
│   ├── autopsy-invariants/         # Invariant YAML evaluators (layers, cycles, forbidden)
│   ├── autopsy-evidence/           # Verifiable finding receipts & path sequences
│   ├── autopsy-storage/            # SQLite & content-addressed local disk cache
│   ├── autopsy-report/             # Canonical JSON, human text & SARIF reporting
│   └── autopsy-cli/                # Deterministic CLI binary entrypoint
├── apps/
│   ├── mcp-server/                 # Model Context Protocol adapter (read-only)
│   └── web-inspector/              # Local evidence & dependency graph inspector
├── tests/                          # End-to-end integration test harness (autopsy-tests)
├── schemas/                        # Frozen public JSON Schemas (Draft-07)
├── scripts/                        # Automated verification & determinism gates
├── docs/                           # Architecture, ADRs, Traceability, and Evidence records
└── assets/                         # Visual emblems, diagrams, and identity assets
```

---

## 6. Verification Quality Matrix & Pre-Commit Enforcement

SynEvid maintains a zero-tolerance policy for compiler warnings, test regressions, and non-deterministic behavior.

### Automated Pre-Commit Stack (8 Local Hooks)
All commits and pushes are verified locally via `.pre-commit-config.yaml`:
```bash
$ pre-commit run --all-files

Rust Format Check (rustfmt).................................Passed
Rust Clippy Strict Lints (-D warnings)......................Passed
Workspace Test Stack (Unit & Integration)...................Passed
Rust Documentation Tests....................................Passed
Golden 100-Run Determinism Invariant........................Passed
JSON Schema Syntactic Validation............................Passed
Engine Config & Invariant Syntax Validation.................Passed
Architectural Boundary Gate (Zero LLM/MCP in Core Crates)...Passed
```

### GitHub Actions Pre-Merge Gate
Every pull request and push to `main` executes a multi-job verification matrix ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)):
1. **`rust-quality`**: `cargo fmt` + `cargo clippy --workspace --all-targets -- -D warnings`
2. **`rust-tests`**: 84 unit and integration tests across all crates (Phases 0, 1, 2, 3, 4, 5)
3. **`determinism-gate`**: 100 sequential runs asserting byte-identical snapshot digests
4. **`arch-boundary-gate`**: Asserts core crates contain no LLM or presentation dependencies
5. **`schemas-and-configs`**: Validates JSON Schemas and engine TOML/YAML files
6. **`pre-merge-verification-gate`**: Aggregated status check gating merges

---

## 7. Developer & Agent Workflow

### Prerequisites
* **Rust**: `1.85+` (2024 Edition)
* **Python**: `3.11+` (for schema validation)
* **Pre-commit**: `pre-commit` binary installed

### Quick Start
```bash
# 1. Clone repository
git clone git@github.com:sainibhaowal/SynEvid.git
cd SynEvid

# 2. Install pre-commit hooks
pre-commit install --hook-type pre-commit --hook-type pre-push

# 3. Run full verification suite (fmt, clippy, goldens, schemas, boundaries)
make check

# 4. Run workspace unit and E2E integration tests
make test
```

---

## 8. Requirements Traceability & Evidence

* **Requirements Matrix:** [`docs/requirements/TRACEABILITY.md`](docs/requirements/TRACEABILITY.md) (FR-001 through FR-030).
* **Engineering Evidence Log:** [`docs/evidence/01_phase0_phase1_implementation_and_verification.md`](docs/evidence/01_phase0_phase1_implementation_and_verification.md).
* **AI Coding Agent Rules:** [`AGENTS.md`](AGENTS.md) and [`.agents/rules/agent-engineering-rules.md`](.agents/rules/agent-engineering-rules.md).
* **Architecture Decision Records:** [`docs/adr/`](docs/adr/).

---

## 9. Governance & Community

* **Security Policy:** [`SECURITY.md`](SECURITY.md) — Vulnerability reporting protocol and cryptographic invariants.
* **Contributing Guide:** [`CONTRIBUTING.md`](CONTRIBUTING.md) — Local development workflow, branch naming, and determinism standards.
* **Code of Conduct:** [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) — Contributor Covenant v2.1.
* **Notice:** [`NOTICE`](NOTICE) — Apache-2.0 copyright and attribution notices.

---

## 10. License

Licensed under the Apache License, Version 2.0 (the "License"). You may obtain a copy of the License at [`LICENSE`](LICENSE) or at [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0).

