# SynEvid Benchmark Protocol & Empirical Evaluation Methodology

> **Current status: WITHDRAWN / NOT EVALUABLE.** The historical RQ1–RQ5
> comparison below was not methodologically valid: the current tasks do not
> define query scope or independently reviewed exhaustive labels, baseline
> implementations consume expected-answer fields, and the purported LSP tool
> is only a text heuristic. Historical metrics and the old promotion-gate PASS
> are superseded by [`docs/evidence/108-impact-benchmark-integrity-audit.md`](../evidence/108-impact-benchmark-integrity-audit.md).
> The runner refuses comparative scoring until the protocol requirements are met.

This document details the pre-registered benchmark protocol, comparative baseline design, evaluation corpus, and reproduction telemetry for **SynEvid (Code Autopsy)**.

---

## 1. Primary Empirical Question

> **Does SynEvid materially improve an autonomous coding agent's ability to identify true change impact compared to traditional filesystem/grep and standard LSP tools?**

---

## 2. Evaluation Corpus & Dataset Design

**Correction:** the existing committed suite does not satisfy the historical,
independently reviewed corpus described in this section. It contains generated
prompt fixtures with no explicit query scope or exhaustive-oracle review record.
Treat the old corpus description and downstream numerical tables as withdrawn
until a new frozen evaluation satisfies the requirements in evidence 108.

The evaluation suite exercises realistic, ground-truth refactoring tasks across representative TypeScript repositories:

- **Target Estates:** $\ge 5$ mature TypeScript repositories with established test suites and CI pipelines.
- **Legacy evaluation tasks:** 50 generated prompt fixtures. They are not the 50 historical refactoring tasks previously claimed and do not have independently reviewed exhaustive labels; this corpus is not evaluable for accuracy.
- **Ground Truth Integrity Rule:** Benchmark ground truth is committed prior to engine runs and **must never be modified to artificially inflate accuracy scores**.

---

## 3. Comparative Baselines

The names below describe the legacy intended baselines, not the verified
implementations. In particular, the current “LSP” implementation is a
source-text heuristic and both text baselines access expected-answer fields;
they are not eligible for comparative scoring.

| Baseline | Architecture & Tooling | Description |
| :--- | :--- | :--- |
| **Baseline A** | **Grep / String Search (ripgrep)** | Simulates an agent relying on lexical symbol occurrences and regular expressions. |
| **Baseline B** | **Language Server Protocol (LSP)** | Simulates an agent querying TypeScript LSP symbol references across files. |
| **Baseline C** | **SynEvid (Code Autopsy)** | Pure-Rust deterministic AST call multigraph, transitive impact BFS, and contract delta engine. |

---

## 4. Historical Pre-Registered Gate & Results — Withdrawn

**Status: NOT EVALUABLE.** The numeric results and gate outcomes in this
historical section are withdrawn. A pre-registered threshold cannot rescue an
invalid oracle or answer-leaking comparator. See evidence 108.

To prevent subjective evaluation bias, the promotion gate was pre-registered before running benchmarks:
pass = (ΔRecall ≥ +10pp) ∨ (ΔCost ≥ 40% at equal accuracy) ∨ (New Failure Class > 0).

| Gate Criterion | Pre-Registered Threshold | Committed Measurement | Gate Status |
| :--- | :--- | :--- | :---: |
| **Recall Lift** | ≥ +10.00pp over best baseline | NOT EVALUABLE | WITHDRAWN |
| **Cost Reduction** | ≥ 40% token cut at equal accuracy | NOT EVALUABLE | WITHDRAWN |
| **New Failure Class** | Discovery of structural failure classes | NOT EVALUABLE | WITHDRAWN |

Promotion therefore rests on the recall leg alone — the pre-registered OR gate. The
committed artifacts are [`benchmarks/results/reproduction_report.md`](../../benchmarks/results/reproduction_report.md)
and `.json`.

Two further project invariants are tracked as standing gates (not promotion criteria):

| Standing Gate | Threshold | Status |
| :--- | :--- | :---: |
| **Zero false-negative invariant** | 0 missed breaking regressions on a reviewed complete oracle | **NOT ESTABLISHED** |
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

The historical command below is retained for reference only. On the current
task set, the integrity audit intentionally exits with status `3` before
scoring. Do not treat the archived report files as reproducible accuracy
evidence.

The entire benchmark suite can be executed independently using the bundled test harness:

```bash
cargo test -p autopsy-tests --test e2e_phase6_benchmarks -- --nocapture
```

To inspect the raw machine-readable telemetry and reproduction markdown report:
```bash
cat benchmarks/results/reproduction_report.md
cat benchmarks/results/reproduction_report.json
```
