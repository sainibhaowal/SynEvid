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
  <a href="#quality-matrix"><img src="https://img.shields.io/badge/Rust-1.90%2B%20(2024%20Edition)-orange?style=for-the-badge&logo=rust" alt="Rust 2024"></a>
  <a href="#test-coverage"><img src="https://img.shields.io/badge/Verification-831%2F831%20Tests%20Passing-00e676?style=for-the-badge" alt="Tests Passing"></a>
  <a href="#7-benchmark-harness--empirical-evaluation-architecture"><img src="https://img.shields.io/badge/Gate%20(Ch.12)-PASSED%20on%20recall%20(%2B31.73pp)-00e676?style=for-the-badge" alt="Pre-Registered Gate: PASSED on recall"></a>
</p>

---

## 1. Overview & Core Philosophy

**SynEvid** (internally known as *Code Autopsy*) is an offline, mathematically deterministic software verification and change-impact engine.

### What SynEvid Is
SynEvid is **not** a chatbot, code synthesizer, or LLM wrapper. It is a forensic verification oracle engineered in Rust to calculate bounded, evidence-backed semantic blast radius, enforce architectural invariants, and generate verifiable cryptographic evidence receipts across code revisions. Blast-radius output is a recall-first transitive closure with explicit coverage states — over-approximation is disclosed, not tuned away (see §7).

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

### A. Core Verification Engine Flow (Phases 0–5)

```mermaid
flowchart TD
    Repo["Git Repository / Source Root"] --> Scan["autopsy-repo<br/>(Ignore-Aware Scan)"]
    Scan --> Snap["RepoSnapshot<br/>BLAKE3(Files + Config + Analyzer)"]
    Snap --> Adapter["autopsy-adapter-typescript<br/>(Tree-Sitter AST & Contract Extraction)"]
    Adapter --> Sym["autopsy-symbols<br/>(Stable Symbol Identifiers)"]
    Sym --> Graph["autopsy-graph<br/>(Petgraph Multigraph & Edge Provenance)"]
    Graph --> Diff["autopsy-diff<br/>(Semantic Symbol & Contract Delta)"]
    Diff --> Impact["autopsy-impact<br/>(Bounded BFS / SCC Condensation)"]
    Impact --> Contracts["autopsy-contracts<br/>(Compatibility & Breaking Change Detection)"]
    Contracts --> Inv["autopsy-invariants<br/>(Policy Evaluators: Layers, Cycles, Forbidden)"]
    Inv --> Evid["autopsy-evidence<br/>(Cryptographic BLAKE3 Receipts)"]
    Evid --> Storage["autopsy-storage<br/>(SQLite + Content-Addressed Cache)"]
    Evid --> Report["autopsy-report<br/>(Canonical JSON, Text & SARIF 2.1.0)"]
    Storage & Report --> CLI["autopsy-cli<br/>(Deterministic Exit Codes: 0, 2, 3, 4)"]
```

### B. Agent In-Session Verification Lifecycle Protocol (4.1 Ch.4.2)

```mermaid
flowchart LR
    BaseA["Baseline Snapshot A<br/>(autopsy baseline)"] --> ImpactQ["Impact Assessment<br/>(autopsy impact -s &lt;sym&gt;)"]
    ImpactQ --> AgentEdit["Agent Edit A &rarr; B<br/>(Code Modifications)"]
    AgentEdit --> VerifyStep["autopsy verify<br/>(Invariant & Contract Check)"]
    VerifyStep --> Check{"Gate Decision"}
    Check -- "PASS (Exit 0)" --> Receipt["Evidence Receipt<br/>BLAKE3 Tamper-Proof Digest"]
    Check -- "FAIL (Exit 2)" --> Trace["Repair via Evidence Paths<br/>(autopsy explain &lt;finding&gt;)"]
    Trace --> AgentEdit
    Check -- "UNKNOWN (Exit 4)" --> StrictCheck["Strict Enforcement<br/>UNKNOWN != PASS"]
```

---

## 4. Foundation Command Interface
```bash# Snapshot repository baseline state into SQLite & content-addressed cache
autopsy baseline --format json

# Compute semantic change-set between a stored snapshot and the worktree
autopsy diff --before <snapshot_id> --format json

# Traverse bounded blast radius for a symbol with ranked shortest paths
autopsy impact -s "src/index.ts::main" --direction forward --max-depth 5
# Narrow traversal to call edges only (operator precision control)
autopsy impact -s "src/index.ts::main" --direction backward --edge-kinds calls
# Recommended precision-first query: caller closure + top-10 shortlist
autopsy impact -s "src/index.ts::main" --profile callers --top-k 10
# Incremental baseline: reuse cached per-file analyses for unchanged files
autopsy baseline --incremental --baseline-id <snapshot_id>

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
├── benchmarks/                     # Phase 6 Enterprise Benchmark Evaluation Harness
│   ├── corpus/                     # 5 TypeScript target repositories (repo-api, migration, relayer, etc.)
│   ├── tasks/                      # 50 task definitions strictly matching benchmark-task.schema.json
│   ├── engine/                     # Baselines A (grep), B (LSP), C (autopsy), metrics & gate evaluator
│   ├── results/                    # Committed reproduction reports (reproduction_report.json / .md)
│   ├── generate_tasks.py           # Deterministic task generation engine
│   └── run_benchmarks.py           # Executable benchmark CLI harness
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
2. **`rust-tests`**: 831 unit and integration tests across all crates (Phases 0 through 4.6)
3. **`determinism-gate`**: 100 sequential runs asserting byte-identical snapshot digests
4. **`arch-boundary-gate`**: Asserts core crates contain no LLM or presentation dependencies
5. **`schemas-and-configs`**: Validates JSON Schemas and engine TOML/YAML files
6. **`pre-merge-verification-gate`**: Aggregated status check gating merges

---

## 7. Benchmark Harness & Empirical Evaluation Architecture (FR-025, UC-08, RQ1–RQ5)

Phase 6 introduces a rigorous, offline benchmark evaluation suite comparing autonomous agent change-impact strategies across 50 realistic tasks and 5 TypeScript repositories:

### A. Benchmark Evaluation Architecture

```mermaid
flowchart TD
    subgraph Corpus["Benchmark Corpus (5 Repositories)"]
        R1["repo-api<br/>(Layered REST)"]
        R2["repo-migration<br/>(Adapter Migration)"]
        R3["repo-relayer<br/>(Hexagonal Architecture)"]
        R4["repo-cross-package<br/>(Monorepo Packages)"]
        R5["repo-dead-call<br/>(Reachability & Dynamic)"]
    end

    subgraph Tasks["Task Suite (50 Tasks, Draft-07 Schema)"]
        T1["api_change (10 Tasks)"]
        T2["migration (10 Tasks)"]
        T3["relayer (10 Tasks)"]
        T4["cross_package (10 Tasks)"]
        T5["dead_call (10 Tasks)"]
    end

    Corpus --> Tasks

    subgraph Execution["Parallel Baseline Execution"]
        BaseA["Baseline A: Agent + Grep<br/>(Textual Regex / Keyword Search)"]
        BaseB["Baseline B: Agent + LSP<br/>(1-Hop Compiler AST References)"]
        BaseC["Baseline C: Agent + Autopsy<br/>(Deterministic Multigraph Blast Radius)"]
    end

    Tasks --> BaseA & BaseB & BaseC

    subgraph MetricsEngine["Metrics & Gate Evaluation"]
        M["Metrics Engine<br/>Recall, Precision, F1, Tokens, Tool Calls, Latency"]
        Gate{"Pre-Registered Gate (Ch.12)<br/>Lift &ge; +10pp OR Cut &ge; 40% OR New Classes"}
        RepJSON["benchmarks/results/reproduction_report.json"]
        RepMD["benchmarks/results/reproduction_report.md"]
    end

    BaseA & BaseB & BaseC --> M
    M --> Gate
    Gate --> RepJSON & RepMD
```

### B. Comparative Empirical Telemetry (50 Tasks Across 5 Repositories)

| Metric | Baseline A (Agent + Grep) | Baseline B (Agent + LSP) | Baseline C (Agent + Autopsy) | Autopsy Advantage |
| :--- | :---: | :---: | :---: | :--- |
| **Mean Recall** | 68.27% | 68.27% | **100.00%** | **+31.73pp Recall Lift** |
| **Mean Precision** | 75.00% | 76.00% | **5.98%** | **Over-approximates: full transitive closure, needs ranked summarization (RQ3)** |
| **Mean F1 Score** | 70.04% | 70.71% | **10.80%** | **Recall-first evidence; precision work deferred, not tuned** |
| **Mean Tool Calls** | 2.92 calls | 1.82 calls | **3.00 calls** | **baseline + impact + verify (real binary runs)** |
| **Mean Tokens** | 926 tokens | 819 tokens | **2693 tokens** | **Evidence JSON is verbose; cost gate FAILs (-228.78%)** |
| **Mean Latency** | ~118 ms | ~100 ms | **~112 ms** | **Measured wall-clock per task** |
| **Failure Classes Detected** | 0 | 8 | **0 (task vocabulary)** | **Engine rule ids vs task change classes: mapping not invented** |

> **Honesty note (2026-10-09):** Baseline-C executes the real engine binary
> per task (see `benchmarks/engine/baseline_autopsy.py`). Earlier 100%
> precision / 353-token figures came from a simulated baseline that copied
> ground truth and are withdrawn. Full telemetry and limitations in
> `docs/evidence/13_forensic_closure_verification.md`.

### C. Pre-Registered Gate Status (01 Ch.12)

$$\text{Gate Decision} = (\Delta\text{Recall} \ge +10\text{pp}) \lor (\Delta\text{Cost} \ge 40\% \land \text{Acc}_C \ge \text{Acc}_{\text{base}}) \lor (\text{New Failure Class} > 0)$$

```
+-----------------------------------------------------------------------------------------+
|                              PRE-REGISTERED GATE: PASSED                                |
+-----------------------------------------------------------------------------------------+
|  Gate Criterion       | Pre-Registered Threshold | Observed Telemetry | Gate Decision   |
+-----------------------+--------------------------+--------------------+-----------------+
|  Recall Lift          | >= +10.00pp              | +31.73pp           | PASS            |
|  Cost Reduction       | >= 40.00% (equal acc)    | -228.78%           | FAIL            |
|  New Failure Classes  | Discovery of new classes | 0 (vocab gap)      | FAIL            |
+-----------------------+--------------------------+--------------------+-----------------+
```

---

## 8. Developer & Agent Workflow

### Install as a user (4 paths)

**A. One-line installer (recommended).** Tries a prebuilt release binary for
your OS/arch; falls back to a source build (installs Rust via rustup if
missing); optionally wires detected agent hosts.
```bash
curl -fsSL https://raw.githubusercontent.com/sainibhaowal/SynEvid/main/scripts/install.sh | bash
# env: AUTOPSY_SKIP_HOSTS=1 (MCP only) · AUTOPSY_SKIP_BUILD=1 · SYNevid_PREFIX=~/.local
autopsy version   # verify
```

**B. CLI binary from source.** Prerequisites: Rust 1.85+.
```bash
git clone https://github.com/sainibhaowal/SynEvid.git && cd SynEvid
cargo build --release -p autopsy-cli
export PATH="$PWD/target/release:$PATH"
autopsy --repo-root /path/to/your/repo version
```

**C. Wire your agents automatically (Claude Code, Cursor, Windsurf,
VS Code, opencode, Gemini CLI).** Auto-detects installed hosts, writes
timestamped backups, and is re-run safe.
```bash
python3 scripts/setup-agent-hosts.py --check            # dry run: what would change
python3 scripts/setup-agent-hosts.py --hooks            # MCP + advisory safe-edit hook
```
Manual alternative for any MCP client (Prerequisites: Node 22+,
`npm ci && npm run build` in `apps/mcp-server`):
```json
{
  "mcpServers": {
    "synevid": {
      "command": "node",
      "args": ["/absolute/path/to/SynEvid/apps/mcp-server/dist/index.js"],
      "env": { "AUTOPSY_BIN": "/absolute/path/to/SynEvid/target/release/autopsy" }
    }
  }
}
```
`AUTOPSY_BIN` is optional: without it the server tries `~/.local/bin/autopsy`,
then `~/.cargo/bin/autopsy`, then workspace `target/release` and
`target/debug` builds, then `PATH`.
Opt-in agent guidance lives in `agents/skills/synevid-forensics.md`, and
`agents/hooks/safe-edit.sh` is an advisory, fail-open pre-edit blast-radius
prompt (never blocks an edit).

**D. PR gate (team use).** Copy `.github/workflows/autopsy-pr-gate.yml` into
your repository. Every PR then gets: invariant verdict (fails the merge on
exit 2), per-file caller blast radius, and JSON + SARIF artifacts.

**Language coverage:** TypeScript/JavaScript and Python adapters (both
deterministic, tree-sitter, conformance-tested). Others land only when a
design partner requires one — breadth deliberately follows demand.

### Prove it's working on your machine (60-second self-check)
Run these verbatim against any TypeScript or Python repo. Every step has an
unmistakable pass signal — no trust required:

```bash
R=/path/to/your/repo
autopsy --repo-root $R doctor            # expect: Overall Status: HEALTHY
autopsy --repo-root $R baseline --format json > /tmp/a.json
autopsy --repo-root $R baseline --format json > /tmp/b.json
python3 -c "import json; a=json.load(open('/tmp/a.json')); b=json.load(open('/tmp/b.json')); assert a['result_digest']==b['result_digest'], 'NON-DETERMINISTIC'; print('deterministic: identical digest', a['result_digest'][:16])"
autopsy --repo-root $R impact -s "<file>::<symbol>" --profile callers --format json
autopsy --repo-root $R verify --format json   # expect exit 0, or exit 2 naming a real rule
```

### How to tell engine output apart from AI output — on every call

This is the most important section for users. **SynEvid never calls any AI
model — not on install, not per call, not ever.** The direction is always:

> **Agent (Claude/Cursor/you) → MCP tool → `autopsy` CLI → canonical JSON → back**

On every single call you can verify which side spoke:

| Signal | Engine (forensic lab) | AI model (agent chat) |
|---|---|---|
| Exit code | `0` pass, `2` policy-fail, `3` error, `4` unknown — always printed | No exit codes |
| Digest | Every JSON carries `result_digest` (BLAKE3); rerun → byte-identical | Text changes every run |
| Errors | Machine JSON on stdout, human diagnostics on stderr | Prose only |
| Network | Zero sockets — pull your cable and rerun; identical output | Fails offline |
| Unknowns | Reported as `"unknown"`, never hidden | May confidently guess |

Concretely: if your agent shows you a blast-radius list, ask it for the
`result_digest` and rerun `autopsy impact -s …` yourself. Same digest =
the lab said it. Different or missing = the agent improvised it.

### Prerequisites
* **Rust**: `1.85+` (2024 Edition)
* **Python**: `3.11+` (for schema validation and benchmark harness)
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

# 5. Run Phase 6 Benchmark Evaluation Suite (50 tasks across 5 TS repos)
python3 benchmarks/run_benchmarks.py --check-gate
```

### PR Forensic Gate & Design-Partner Kit
Every pull request runs [`.github/workflows/autopsy-pr-gate.yml`](.github/workflows/autopsy-pr-gate.yml):
`verify` gates the merge (exit 2 fails), per-file `impact` (callers profile,
top-10) is computed for changed TypeScript files, and JSON + SARIF reports
are uploaded as immutable artifacts with a PR summary comment.

To run SynEvid on a partner's historical change (base ref → worktree):
```bash
./scripts/forensic-case.sh /path/to/repo <base-ref> /tmp/case-dir [seed ...]
# → case bundle: baselines, diff, verify + SARIF, per-seed impacts, verdict
```

---

## 9. Requirements Traceability & Evidence

* **Requirements Matrix:** [`docs/requirements/TRACEABILITY.md`](docs/requirements/TRACEABILITY.md) (FR-001 through FR-030).
* **Engineering Evidence Log:**
  - [`01_phase0_phase1_implementation_and_verification.md`](docs/evidence/01_phase0_phase1_implementation_and_verification.md)
  - [`02_phase2_typescript_adapter_verification.md`](docs/evidence/02_phase2_typescript_adapter_verification.md)
  - [`03_phase3_graph_diff_verification.md`](docs/evidence/03_phase3_graph_diff_verification.md)
  - [`04_phase4_impact_contracts_invariants_evidence_verification.md`](docs/evidence/04_phase4_impact_contracts_invariants_evidence_verification.md)
  - [`05_phase5_cli_storage_report_verification.md`](docs/evidence/05_phase5_cli_storage_report_verification.md)
  - [`06_phase6_benchmark_harness_verification.md`](docs/evidence/06_phase6_benchmark_harness_verification.md) (Baseline-C figures superseded — see 13)
  - [`07_phase3_2_cross_platform_and_enterprise_hardening.md`](docs/evidence/07_phase3_2_cross_platform_and_enterprise_hardening.md)
  - [`08_pillar_a_universal_hardware_and_multi_arch.md`](docs/evidence/08_pillar_a_universal_hardware_and_multi_arch.md)
  - [`09_pillar_b_dual_cicd_lifecycle_and_release_matrix.md`](docs/evidence/09_pillar_b_dual_cicd_lifecycle_and_release_matrix.md)
  - [`10_pillar_c_enterprise_sandboxing_and_defense.md`](docs/evidence/10_pillar_c_enterprise_sandboxing_and_defense.md)
  - [`11_phase4_1_agent_lifecycle_mcp_and_web_inspector.md`](docs/evidence/11_phase4_1_agent_lifecycle_mcp_and_web_inspector.md)
  - [`12_phase4_1_master_tech_plan_end_to_end_verification.md`](docs/evidence/12_phase4_1_master_tech_plan_end_to_end_verification.md)
  - [`13_forensic_closure_verification.md`](docs/evidence/13_forensic_closure_verification.md)
  - [`14_precision_incremental_pr_gate.md`](docs/evidence/14_precision_incremental_pr_gate.md) (current: precision controls, incremental, PR gate)
  - [`16_landscape_parity.md`](docs/evidence/16_landscape_parity.md) (landscape audit, Python adapter, universal install)
  - [`15_phase4_1_gap_closure.md`](docs/evidence/15_phase4_1_gap_closure.md)
* **AI Coding Agent Rules:** [`AGENTS.md`](AGENTS.md) and [`.agents/rules/agent-engineering-rules.md`](.agents/rules/agent-engineering-rules.md).
* **Architecture Decision Records:** [`docs/adr/`](docs/adr/).

---

## 10. Governance & Community

* **Security Policy:** [`SECURITY.md`](SECURITY.md) — Vulnerability reporting protocol and cryptographic invariants.
* **Contributing Guide:** [`CONTRIBUTING.md`](CONTRIBUTING.md) — Local development workflow, branch naming, and determinism standards.
* **Code of Conduct:** [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) — Contributor Covenant v2.1.
* **Notice:** [`NOTICE`](NOTICE) — Apache-2.0 copyright and attribution notices.

---

## 11. License

Licensed under the Apache License, Version 2.0 (the "License"). You may obtain a copy of the License at [`LICENSE`](LICENSE) or at [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0).

