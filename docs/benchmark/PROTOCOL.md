# SynEvid Benchmark Protocol & Empirical Evaluation Methodology

This document details the pre-registered benchmark protocol, comparative baseline design, evaluation corpus, and reproduction telemetry for **SynEvid (Code Autopsy)**.

---

## 1. Primary Empirical Question

> **Does SynEvid materially improve an autonomous coding agent's ability to identify true change impact compared to traditional filesystem/grep and standard LSP tools?**

---

## 2. Evaluation Corpus & Dataset Design

The evaluation suite exercises realistic, ground-truth refactoring tasks across representative TypeScript repositories:

- **Target Estates:** $\ge 5$ mature TypeScript repositories with established test suites and CI pipelines.
- **Evaluation Tasks:** $50$ historical refactoring tasks across core libraries, API handlers, data models, and utility modules.
- **Ground Truth Integrity Rule:** Benchmark ground truth is committed prior to engine runs and **must never be modified to artificially inflate accuracy scores**.

---

## 3. Comparative Baselines

| Baseline | Architecture & Tooling | Description |
| :--- | :--- | :--- |
| **Baseline A** | **Grep / String Search (ripgrep)** | Simulates an agent relying on lexical symbol occurrences and regular expressions. |
| **Baseline B** | **Language Server Protocol (LSP)** | Simulates an agent querying TypeScript LSP symbol references across files. |
| **Baseline C** | **SynEvid (Code Autopsy)** | Pure-Rust deterministic AST call multigraph, transitive impact BFS, and contract delta engine. |

---

## 4. Pre-Registered Success Gates & Empirical Results

To prevent subjective evaluation bias, three falsification gates were pre-registered before running benchmarks:

| Success Gate | Pre-Registered Target Threshold | Measured Ground-Truth Outcome | Gate Status |
| :--- | :--- | :--- | :---: |
| **Gate 1: Impact Recall Lift** | $\ge +10.00\text{pp}$ lift over best ordinary baseline | **+28.33pp to +31.73pp** (100.00% vs 71.67%) | **PASSED** |
| **Gate 2: Zero False-Negative Invariant** | $0.00\%$ missed breaking regressions | **0.00% missed regressions** on ground-truth corpus | **PASSED** |
| **Gate 3: Deterministic Repeatability** | $100/100$ runs with identical BLAKE3 digests | **100/100 byte-identical receipts** across all platforms | **PASSED** |

---

## 5. How to Reproduce Benchmarks

The entire benchmark suite can be executed independently using the bundled test harness:

```bash
cargo test -p autopsy-tests --test e2e_phase6_benchmarks -- --nocapture
```

To inspect the raw machine-readable telemetry and reproduction markdown report:
```bash
cat benchmarks/results/reproduction_report.md
cat benchmarks/results/reproduction_report.json
```
