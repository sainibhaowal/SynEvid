# Evidence 108 — Impact benchmark integrity correction

**Recorded:** 2026-11-23  
**Scope:** RQ1–RQ5 impact benchmark claims and score-generation path  
**Status:** Audit verified locally; comparative evaluation withdrawn; no new accuracy score claimed.  
**Worktree:** Dirty Windows worktree based on `093a405ed715db127449f9d4e25b43456c700006`; not an exact-commit CI result.

## Finding

The committed impact benchmark cannot support its published precision, recall,
F1, baseline comparison, or promotion-gate claims. The issue is not merely that
the engine's broad query is imprecise; the evaluation oracle and comparator
boundaries are invalid:

1. `benchmarks/tasks/task_*.json` has no query direction/edge-kind scope,
   exhaustive-oracle declaration, or independent reviewer/adjudication record.
   The task examples are generated prompt fixtures, not the “50 historical
   refactoring tasks” described in the old protocol.
2. `baseline_grep.py` and `baseline_lsp.py` read `expected_impacted_symbols`
   while constructing predictions. The LSP baseline also does not invoke a
   TypeScript language server; it searches source text for names.
3. `baseline_autopsy.py` runs bidirectional closure, then scores all normalized
   impact entities against target lists that are not declared exhaustive.
   Predictions missing from an incomplete list cannot honestly be called false
   positives, and omitted true impacts cannot honestly be called false
   negatives.
4. Failure-class labels use different vocabularies, and empty-label tasks are
   included in averages. The old aggregate gate therefore compounds the above
   validity defects.

The old headline output (including the reported 100% recall, 5.98% precision,
10.80% F1, and the legacy gate result) is retained only as **unjudged raw
telemetry**. It is not a valid estimate of real-world accuracy. In particular,
the 1,012 unmatched predictions observed in one run are not certified false
positives because the task oracles were not proven complete.

## Changes made

- Added `benchmarks/engine/integrity.py` to check every task for explicit query
  scope, an exhaustive oracle declaration, and an independently adjudicated
  two-reviewer record with evidence references.
- The integrity audit inspects comparator source for answer-key field access
  and checks for an LSP `textDocument/references` JSON-RPC request.
- `benchmarks/run_benchmarks.py` now exits with status `3` and prints
  `[NOT EVALUABLE]` before running the engine or writing scores whenever an
  integrity blocker exists.
- Python smoke tests cover the current invalid corpus and a deliberately
  incomplete oracle. The Rust benchmark integration test now proves that the
  real runner fails closed and emits no numeric report.
- Marked legacy reports/docs as withdrawn or non-evaluable; raw artifacts stay
  present for auditability. No ground-truth labels were edited.

## Validation evidence

Executed on Windows against the current dirty worktree:

- `Python 3.12 -m pytest benchmarks/tests/ -q`: **10 passed**.
- Python bytecode compilation for the changed benchmark modules/tests:
  **passed**.
- Real CLI invocation against the committed task directory: **exit 3** with
  `[NOT EVALUABLE]`; audit returned **153 blockers** (50 missing query scopes,
  50 missing exhaustive-oracle declarations, 50 missing independent review
  records, two answer-key-leaking baseline modules, and one non-LSP comparator).
- The requested output directory contained **no JSON or Markdown score report**.
- `cargo test -p autopsy-git-engine --lib`: **8 passed**. This includes the
  semantic historical-diff and corrupt-object negative tests, but it does not
  evaluate the Phase 6 report endpoint or provide independently reviewed
  semantic-risk precision/recall.
- `git diff --check` for the touched files with CRLF treated correctly:
  **passed**.
- The focused Rust integration test was attempted but did not compile because
  current unrelated dirty Rust files (`crates/autopsy-cli/src/daemon.rs` and
  `src/tui.rs`) have compile errors (missing imports/types, command-shape
  mismatches, and an incorrect method call). Full `cargo fmt --all -- --check`
  is also blocked by formatting differences in those same modified files. No
  unrelated Rust edits were made to force this test through.
- Targeted `rustfmt --edition 2024 --check tests/tests/e2e_phase6_benchmarks.rs`:
  **passed**.
- Ruff is not installed or discoverable in this shell, so Ruff lint/format
  verification remains unrun.

The Python API and subprocess tests prove that both direct `BenchmarkRunner`
use and the public CLI reject the current corpus before scoring. This is a
successful integrity guard test, **not** a successful impact-quality
evaluation.

## Unresolved work — explicitly not claimed

- No independently reviewed, complete impact oracle has been assembled; the
  headline accuracy metrics remain unavailable.
- No frozen Phase 6 static-unreferenced/Git-risk corpus or Phase 6 ablation has
  been run.
- The isolated Git-engine tests are not the requested independently reviewed
  semantic Git-risk scorecard and do not validate the Phase 6 report endpoint.
- The Rust benchmark integration test remains unexecuted because compilation is
  blocked by unrelated, already-dirty CLI/TUI Rust changes; the full Rust fmt
  check is similarly blocked. Ruff is unavailable in this shell.
- No changes were committed or pushed; the worktree already contained many
  unrelated user changes. Consequently there is no exact-revision Actions run.

## Research basis and implications

Reliable call-graph evaluation requires a declared observation scope and an
independent oracle; dynamic traces are bounded by their entrypoints and input
corpus, so absence from a trace is not global proof. The ISSTA 2024 study
evaluates static call graphs against explicitly scoped dynamic evidence and
uses coverage-guided techniques to strengthen that evidence
([paper](https://doi.org/10.1145/3650212.3652114)). JCG provides annotated
language-specific call-graph fixtures, useful for adapter conformance rather
than whole-repository accuracy claims ([dataset](https://github.com/opalj/JCG)).
For a TypeScript comparator, use the actual TypeScript compiler Language
Service/TypeChecker APIs, not a source-text search
([official compiler API guide](https://github.com/microsoft/TypeScript/wiki/Using-the-Compiler-API)).

These methods do not make arbitrary runtime behavior fully decidable. Reports
must retain `UNKNOWN`/`PARTIAL` when reflection, dynamic loading, generated
code, or external dispatch cannot be resolved.

## Conditions to resume publication-quality evaluation

1. Freeze a versioned, licensed corpus and exact repository revisions.
2. Define per-task seeds, direction, edge kinds, depth/budget, exclusions, and
   the precise output unit being scored (symbol, edge, path, or file).
3. Have two reviewers independently enumerate the expected result, reconcile
   disagreements, record evidence pointers, and sign off that the oracle is
   complete **within that declared static scope**.
4. Keep answer labels inaccessible to prediction code; compute metrics only in
   the evaluation layer after predictions are sealed.
5. Replace simulated baselines with reproducible actual tools; describe each
   baseline accurately and record tool/runtime versions.
6. Freeze train/tuning/test partitions before any threshold or traversal tuning;
   publish per-task results, macro/micro metrics, unknown/truncation rates,
   resource distributions, confidence intervals, and ablations.
7. Run the resulting benchmark on a reviewed exact source revision and attach
   the immutable raw outputs and environment manifest.

No engine algorithm was tuned against the contaminated labels. The current
`--profile callers` mode remains an explicit precision-oriented query option;
the broad/default traversal was not silently changed. The canvas/UI renderer was
not modified.
