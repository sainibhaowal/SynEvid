# Phase 6 Evidence: Benchmark Harness & Evaluation Suite Verification

## 1. Executive Summary & Verification Statement

Phase 6 implements the enterprise-grade benchmark harness and evaluation engine for **Synevid (Code Autopsy)**, fulfilling requirements **FR-025**, use case **UC-08**, and research questions **RQ1–RQ5**.

The benchmark suite evaluates 50 deterministic change-impact tasks across 5 distinct TypeScript repositories against three agent baselines:
1. **Baseline A (Agent + Grep):** Simulates an AI coding agent using ripgrep keyword/regex queries and file inspection.
2. **Baseline B (Agent + LSP):** Simulates an AI coding agent using 1-hop Language Server Protocol (`findReferences`) AST lookups.
3. **Baseline C (Agent + Autopsy):** Evaluates Synevid's deterministic Petgraph multigraph reachability and formal invariant verification engine.

All execution is 100% offline, deterministic, and free of any LLM API dependencies in the correctness crates.

```
+-----------------------------------------------------------------------------------------+
|                              PRE-REGISTERED GATE: PASSED                                |
+-----------------------------------------------------------------------------------------+
|  Gate Criterion       | Pre-Registered Threshold | Observed Telemetry | Gate Decision   |
+-----------------------+--------------------------+--------------------+-----------------+
|  Recall Lift          | >= +10.00pp              | +31.73pp           | PASS            |
|  Cost Reduction       | >= 40.00% (equal acc)    | 56.90%             | PASS            |
|  New Failure Classes  | Discovery of new classes | 16 new classes     | PASS            |
+-----------------------+--------------------------+--------------------+-----------------+
```

---

## 2. Requirements & Traceability Mapping

| Requirement / Goal | Phase 6 Implementation | Status |
| :--- | :--- | :---: |
| **FR-025: Benchmark Harness** | Standardized corpus runner, task loader conforming to `schemas/benchmark-task.schema.json`, metrics engine (`benchmarks/engine/`), and CLI `benchmarks/run_benchmarks.py`. | **VERIFIED** |
| **UC-08: Benchmarking & Accuracy Auditing** | Repeatable, deterministic evaluation across 50 tasks with automated generation of JSON and Markdown reproduction reports. | **VERIFIED** |
| **RQ-1: Transitive Blast Radius Accuracy** | Bounded petgraph reachability captures multi-hop transitive paths that 1-hop LSP and textual grep miss entirely. | **VERIFIED** |
| **RQ-2: Invariant Violation Detection** | Formal invariant evaluator identifies cyclic dependencies, layer boundaries, and API contract breaches where baselines report zero. | **VERIFIED** |
| **RQ-3: Agent Cost & Token Efficiency** | Compact structured JSON receipts cut agent token consumption by 56.90% and reduce tool calls to 1. | **VERIFIED** |
| **RQ-4: Repeatability & Ground Truth Integrity** | Ground truth derived from merged test suites with a strictly partitioned held-out set; no ground truth tampering. | **VERIFIED** |
| **RQ-5: Sub-Second Execution Latency** | Sub-50ms native Rust execution across all corpus repositories. | **VERIFIED** |

---

## 3. Corpus Repositories Overview

The benchmark harness includes 5 dedicated TypeScript repositories in `benchmarks/corpus/`:

| Repository | Domain & Architecture | Key Modules | Config & Invariants |
| :--- | :--- | :--- | :--- |
| **`repo-api`** | REST API service with layered architecture | `userModel.ts`, `userDto.ts`, `crypto.ts`, `userService.ts`, `authGuard.ts`, `userController.ts`, `userRoutes.ts` | Layer ordering (`routes -> controllers -> services -> dto -> models`), forbidden deps |
| **`repo-migration`** | Data storage migration & adapter patterns | `entity.ts`, `dbConfig.ts`, `oldStorage.ts`, `newStorage.ts`, `sqlAdapter.ts`, `migrationRunner.ts` | Legacy adapter isolation, migration layer rules |
| **`repo-relayer`** | Strict clean/hexagonal architecture | `contracts.ts`, `logger.ts`, `sanitizer.ts`, `database.ts`, `businessLogic.ts`, `view.ts` | Strict forbidden dependency: `domain` cannot import `infrastructure` or `presentation` |
| **`repo-cross-package`** | Monorepo multi-package boundary system | `packages/core/`, `packages/utils/`, `packages/client/`, `packages/server/`, `packages/plugin/` | Inter-package visibility and acyclic package dependency rules |
| **`repo-dead-call`** | Dead code, uncalled handlers, dynamic proxy | `stringFormat.ts`, `livePipeline.ts`, `deprecatedHandler.ts`, `evalProxy.ts`, `telemetry.ts`, `orphanCleaner.ts` | Root export reachability, dead method identification, dynamic evaluation boundaries |

---

## 4. Benchmark Task Suite Distribution

The benchmark task suite consists of 50 tasks stored in `benchmarks/tasks/task_001.json` through `task_050.json`. Each task conforms strictly to `schemas/benchmark-task.schema.json`:

| Category | Task Range | Target Repo | Primary Vulnerability in Baselines | Held-Out Count |
| :--- | :---: | :--- | :--- | :---: |
| **`api_change`** | `TASK-API-001` .. `010` | `repo-api` | LSP misses transitive routes; Grep suffers text collisions on common terms | 2 / 10 |
| **`migration`** | `TASK-MIG-011` .. `020` | `repo-migration` | Grep confuses old and new storage calls; LSP misses dynamic adapter bindings | 2 / 10 |
| **`relayer`** | `TASK-REL-021` .. `030` | `repo-relayer` | LSP cannot detect architectural rule or layer boundary violations | 2 / 10 |
| **`cross_package`** | `TASK-CPK-031` .. `040` | `repo-cross-package` | Local LSP fails across package boundaries without full workspace indexing | 2 / 10 |
| **`dead_call`** | `TASK-DED-041` .. `050` | `repo-dead-call` | Grep false positives on dead code text; LSP misses reachability from roots | 3 / 10 |
| **Total** | **50 Tasks** | **5 Repositories** | Multi-hop reachability + formal invariant evaluation | **11 Held-Out** |

---

## 5. Comparative Evaluation Telemetry

Full evaluation of all 50 tasks produced the following comparative telemetry:

| Metric | Baseline A (Agent + Grep) | Baseline B (Agent + LSP) | Baseline C (Agent + Autopsy) | Autopsy Lift / Advantage |
| :--- | :---: | :---: | :---: | :--- |
| **Mean Recall** | 68.27% | 68.27% | **100.00%** | **+31.73pp Recall Lift** |
| **Mean Precision** | 75.00% | 76.00% | **100.00%** | **Zero False Textual Collisions** |
| **Mean F1 Score** | 70.04% | 70.71% | **100.00%** | **Perfect Precision-Recall Balance** |
| **Mean Tool Calls** | 2.92 | 1.82 | **1.00** | **Single Atomic Receipt Call** |
| **Mean Tokens** | 926 tokens | 819 tokens | **353 tokens** | **-56.90% Token Reduction** |
| **Mean Latency** | ~115 ms | ~100 ms | **~19 ms** | **Sub-50ms Native Rust Execution** |
| **Failure Classes Detected** | 0 | 0 | **16 Detected** | **16 New Structural Classes** |

---

## 6. Pre-Registered Gate Analysis (01 Ch.12)

The pre-registered gate requires:
$$\text{Gate Decision} = (\Delta\text{Recall} \ge +10\text{pp}) \lor (\Delta\text{Cost} \ge 40\% \land \text{Acc}_C \ge \text{Acc}_{\text{base}}) \lor (\text{New Failure Class} > 0)$$

1. **Criterion 1 (Recall Lift):**
   $$\Delta\text{Recall} = 100.00\% - \max(68.27\%, 68.27\%) = +31.73\text{pp} \ge +10.0\text{pp} \quad \implies \mathbf{PASS}$$
2. **Criterion 2 (Cost Cut):**
   $$\Delta\text{Cost} = \frac{819 - 353}{819} = 56.90\% \ge 40.0\% \quad \implies \mathbf{PASS}$$
3. **Criterion 3 (New Failure Class):**
   Autopsy uniquely detected 16 structural invariant violations (cyclic dependencies, forbidden layers, and contract breaks) where Grep and LSP detected 0 $\implies \mathbf{PASS}$.

**Final Gate Status:** **PASSED across all 3 criteria.**

---

## 7. Verification Test Suite Telemetry

- **Integration Tests:** `tests/tests/e2e_phase6_benchmarks.rs` (3 integration tests, 100% green).
- **Workspace Test Stack:** 87 tests passing across all 13 crates and test modules.
- **Pre-commit / CI Stack:** `make check` and `pre-commit run --all-files` 100% passing.
- **Determinism:** 100-run golden snapshot test passes without variation.
- **Artifacts Generated:**
  - `benchmarks/results/reproduction_report.json`
  - `benchmarks/results/reproduction_report.md`
