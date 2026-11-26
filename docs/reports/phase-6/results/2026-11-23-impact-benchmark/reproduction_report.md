# SynEvid Legacy Impact Benchmark Output — WITHDRAWN / NOT EVALUABLE

> This report is preserved as historical raw telemetry only. Its accuracy
> metrics and promotion-gate result are invalid because the task oracle is not
> independently complete/reviewed, query scope is missing, and baselines read
> answer-key data (with the alleged LSP baseline implemented as text search).
> Do not cite these numbers. See [evidence 108](../../../../evidence/108-impact-benchmark-integrity-audit.md).

## 1. Executive Summary & Pre-Registered Gate Status

- **Pre-registered gate:** **WITHDRAWN / NOT EVALUABLE**; historical gate numbers below are not valid decisions.
- **Reason:** The benchmark oracle and comparators failed integrity review.
- This impact corpus is not the Phase 6 analytics dead-code/Git-risk evaluation and does not establish product readiness.

| Gate Criterion | Requirement | Observed | Status |
| :--- | :--- | :--- | :--- |
| **Recall Lift** | $\ge +10\text{pp}$ vs best baseline | NOT EVALUABLE | WITHDRAWN |
| **Cost Reduction** | $\ge 40\%$ token reduction at equal accuracy | NOT EVALUABLE | WITHDRAWN |
| **New Failure Class** | Discovery of structural failure classes | NOT EVALUABLE | WITHDRAWN |

## 2. Comparative Benchmark Telemetry (50 Tasks Across 5 TypeScript Repositories)

| Metric | Baseline A (Grep heuristic) | Baseline B (LSP heuristic) | Baseline C (SynEvid CLI) | Difference vs best baseline |
| :--- | :---: | :---: | :---: | :--- |
| **Mean Recall** | 26.00% | 68.27% | **100.00%** | **+31.73pp lift** |
| **Mean Precision** | 26.00% | 76.00% | **5.98%** | -70.02pp |
| **Mean F1 Score** | 26.00% | 70.71% | **10.80%** | -59.91pp |
| **Mean Tool Calls** | 2.92 | 1.82 | **3.00** | +1.18 vs lowest-count baseline |
| **Mean Tokens** | 926 | 819 | **3425** | 318.2% more than lowest-token baseline |
| **Mean Latency (ms)** | 121.7 ms | 100.4 ms | **558.6 ms** | +458.1 ms vs fastest baseline |
| **Failure Classes Detected** | 0 | 8 | **0** | 0 additional classes vs best baseline |

This harness uses deterministic grep/LSP heuristics as comparison baselines, not live autonomous agents. Token and tool-call counts are harness estimates. Results apply only to this task corpus; they do not measure Phase 6 analytics, general repositories, or production readiness.

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
