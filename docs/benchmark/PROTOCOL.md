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

To prevent subjective evaluation bias, the promotion gate was pre-registered before running benchmarks:
pass = (ΔRecall ≥ +10pp) ∨ (ΔCost ≥ 40% at equal accuracy) ∨ (New Failure Class > 0).

| Gate Criterion | Pre-Registered Threshold | Committed Measurement | Gate Status |
| :--- | :--- | :--- | :---: |
| **Recall Lift** | ≥ +10.00pp over best baseline | **+28.33pp** (100.00% vs 71.67% mean recall) | **PASS** |
| **Cost Reduction** | ≥ 40% token cut at equal accuracy | **−246.44%** (2338 vs 675 tokens) | **FAIL** |
| **New Failure Class** | Discovery of structural failure classes | **0** (task vocabulary gap) | **FAIL** |

Promotion therefore rests on the recall leg alone — the pre-registered OR gate. The
committed artifacts are [`benchmarks/results/reproduction_report.md`](../../benchmarks/results/reproduction_report.md)
and `.json`.

Two further project invariants are tracked as standing gates (not promotion criteria):

| Standing Gate | Threshold | Status |
| :--- | :--- | :---: |
| **Zero false-negative invariant** | 0 missed breaking regressions on the ground-truth corpus | **PASSED** (follows from 100% task recall) |
| **Deterministic repeatability** | 100/100 runs with identical BLAKE3 digests | **PASSED** across all platforms |

### 4.1 Honest limits — what is measured and what is not

- **Tool calls (measured):** Baseline A = 2.50, Baseline B = 1.50, Baseline C = 3.00.
  The reading "fewer tool calls to reach a passing verify" does **not** hold on this
  corpus: Autopsy uses more calls and more tokens than the baselines, and wins on
  recall (100% vs 71.67%). Promotion passed on the pre-registered OR gate via the
  recall leg — never on cost.
- **Feature-level counterfactual (not measured):** there is no A/B measurement for the
  Tier 1–3 agent tooling itself (find/pinning/compact versus their absence); it is
  recorded as not measured in `docs/evidence/38-agent-gate-closure.md`.
- **RQ6–RQ10 (not measured):** temporal lineage precision/recall, test-impact
  false-negative rate, and related questions require curated corpora that are not
  present in the committed tree. They are recorded as *not measured* in
  [`benchmarks/results/4.2_pillar_report.md`](../../benchmarks/results/4.2_pillar_report.md)
  rather than approximated.

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
