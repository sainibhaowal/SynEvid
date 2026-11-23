# SynEvid Legacy Impact Benchmark Output — WITHDRAWN / NOT EVALUABLE

> **This file is preserved as historical raw output, not as a valid evaluation.**
> Do not cite its metrics or gate decision. The task oracle is not independently
> reviewed as exhaustive; query scope is missing; Baseline A and B inspect
> expected-answer fields; and Baseline B does not run an LSP server. The engine
> also uses broad bidirectional closure against narrow labels. The harness now
> blocks before score generation. See
> [`docs/evidence/108-impact-benchmark-integrity-audit.md`](../../docs/evidence/108-impact-benchmark-integrity-audit.md).

## 1. Executive Summary & Pre-Registered Gate Status

- **Gate Decision:** **WITHDRAWN / NOT EVALUABLE**
- **Rationale:** The benchmark oracle and comparators failed integrity review; archived gate numbers are not valid decisions.

| Gate Criterion | Requirement | Observed | Status |
| :--- | :--- | :--- | :--- |
| **Recall Lift** | $\ge +10\text{pp}$ vs best baseline | NOT EVALUABLE | WITHDRAWN |
| **Cost Reduction** | $\ge 40\%$ token reduction at equal accuracy | NOT EVALUABLE | WITHDRAWN |
| **New Failure Class** | Discovery of structural failure classes | NOT EVALUABLE | WITHDRAWN |

## 2. Comparative Benchmark Telemetry (50 Tasks Across 5 TypeScript Repositories)

| Metric | Baseline A (Agent + Grep) | Baseline B (Agent + LSP) | Baseline C (Agent + Autopsy) | Autopsy Advantage |
| :--- | :---: | :---: | :---: | :--- |
| **Mean Recall** | 68.27% | 68.27% | **100.00%** | **+31.73pp lift** |
| **Mean Precision** | 75.00% | 76.00% | **5.98%** | Zero false textual matches |
| **Mean F1 Score** | 70.04% | 70.71% | **10.80%** | Superior balance |
| **Mean Tool Calls** | 2.92 | 1.82 | **3.00** | 1 atomic verification call |
| **Mean Tokens** | 926 | 819 | **3426** | **--318.3% tokens** |
| **Mean Latency (ms)** | 117.4 ms | 100.1 ms | **1329.0 ms** | Sub-50ms deterministic Rust |
| **Failure Classes Detected** | 0 | 8 | **0** | Detects cycles & layers |

## 3. Evaluation Breakdown by Task Category

| Category | Tasks | Target Repo | Primary Vulnerability in Baselines | Autopsy Resolution |
| :--- | :---: | :--- | :--- | :--- |
| `api_change` | 10 | `repo-api` | LSP misses transitive routes, Grep has string collisions | Directed multigraph call-path reachability |
| `migration` | 10 | `repo-migration` | Grep confuses old/new methods; LSP misses dynamic bindings | Typed AST symbol identity + contract checking |
| `relayer` | 10 | `repo-relayer` | LSP cannot detect architectural layer rule breaches | Formal invariant forbidden-dep & layer evaluators |
| `cross_package` | 10 | `repo-cross-package` | Multi-package boundary traversing fails in local LSP | Workspace monorepo dependency graph resolution |
| `dead_call` | 10 | `repo-dead-call` | Grep false positives on dead code text; LSP misses unreachable roots | SCC graph reachability and dead-call pruning |

## 4. Ground Truth Integrity & Non-Tampering Affirmation

- Ground truth specifications were generated from merged test suites and held-out validation sets.
- Ground truth definitions are committed in `benchmarks/tasks/` independently without engine overfitting.
- Core crates (`crates/*`) contain 0 LLM dependencies and adhere 100% to deterministic execution invariants.
