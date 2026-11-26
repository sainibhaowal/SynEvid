# Phase 6 evaluation report

**Recorded:** 2026-11-23  
**Environment:** Windows local worktree  
**Base commit:** `093a405ed715db127449f9d4e25b43456c700006`  
**Source state:** uncommitted working-tree changes on top of the base commit  
**Status:** Windows engineering verification; not a Phase 6 G6 acceptance run.

> **Impact benchmark correction:** the legacy RQ1–RQ5 metrics summarized below
> are withdrawn as accuracy evidence. Its task labels lack independent exhaustive
> adjudication and query scope; the Grep/LSP baselines read answer-key fields,
> and the alleged LSP baseline is not a language-server client. The latest raw
> output is therefore unjudged telemetry only, not a precision/recall estimate.
> The benchmark runner now fails closed. See evidence 108.

## Scope and observed results

This records checks actually executed against the current Windows worktree. It
does not certify another OS, an installer/release artifact, or production
readiness. The full canvas renderer, camera, animation, layout, and styling were
not changed.

| Check | Result |
|---|---|
| Full Rust workspace | `cargo test --workspace --all-targets --release -- --quiet --test-threads=1`: 1,066 passed, 0 failed, 3 timing tests isolated/ignored in this broad run; current source and release CLI binary explicitly selected |
| Isolated timing gates | SSE wake-up p95 0.513 ms (<15 ms); 1,000-file simulation p95 25.076 ms (<50 ms), both release-mode isolated runs |
| Rust quality gates | `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace --doc` passed |
| Schema validation | Draft 2020-12 validator passed all 18 schemas and local references; `pytest scripts/tests/test_phase6_schemas.py -q`: 3/3 passed including negative-instance rejection |
| Python tests | `pytest benchmarks/tests/ scripts/tests/ -q`: 8/8 passed |
| Web typecheck and tests | TypeScript check passed; `npm test --prefix apps/web`: 21/21 passed |
| Electron/browser E2E | Rebuilt worktree daemon passed real OTLP ingest, report generation, persisted receipt/history, UI retrieval, daemon restart, and event replay |
| Impact benchmark integrity | Legacy runner emitted historical raw numbers, but the corpus fails the new oracle/query/comparator audit; no impact precision/recall/F1 or promotion-gate result is considered valid or publishable. See evidence 108 |
| GitHub Actions | No Actions run exists for this uncommitted worktree. User requested Windows-only validation for this check; Linux/macOS are not claimed |
| Phase 6 analytics evaluation/ablation | No frozen Phase 6 corpus measurement or ablation has been produced; protocol is documented separately |

The workspace suite was run serially to avoid timing noise; timing requirements
were measured separately. An earlier default-parallel run exposed a replay
sequence collision between independent daemon instances. Live event cursors
are now per-daemon and initialized from that daemon's persisted sequence. The
isolated timing tests pass, but the measurements are not general latency
guarantees.

## Interpretation and remaining gate evidence

G6 remains **PARTIAL**. The missing Phase 6 frozen corpus and ablation are
substantive evaluation evidence, not merely platform setup. Semantic Git-risk
quality also lacks an independently reviewed real-repository scorecard, and
this uncommitted worktree has no exact-revision Actions evidence. The adjacent
impact benchmark is not evaluable; none of its apparent precision/recall/F1 or
gate values are accepted. It cannot establish analytics correctness. See the
[execution plan](00-execution-plan.md), [ablation protocol](02-ablation.md),
and [evidence dossier 107](../../evidence/107-phase6-evidence-backed-reports.md).
