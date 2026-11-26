# SynEvid Phase 0–3 Publication Evidence Report

**Publication revision:** `2026-11-07`  
**Latest verified commit:** `ed88e3ece4d099bdfb71a8506e3c844c9d8ec948`  
**Latest native CI evidence:** [GitHub Actions run 80](https://github.com/sainibhaowal/SynEvid/actions/runs/42316135982)  
**Scope:** Phase 0 through Phase 3 only

## Executive summary

SynEvid has verified, production-oriented evidence for its truth-governance,
core daemon/storage, supported-language impact analysis, and native Git forensic
workflows. The evidence is bounded by repository, fixture, parser, operating
system, and workload identity. The project does **not** claim perfect blast
radius, complete runtime understanding, or universal sub-150 ms performance.

The Phase 0–3 reports are publication-ready for the claims explicitly stated
below. Any claim outside that scope is `NOT CLAIMED` until a receipt and raw
test evidence are added.

## Claim-to-evidence matrix

| Claim | Status | Evidence |
|---|---|---|
| Results carry truthful verdict and coverage states | `VERIFIED` for audited paths | `phase-0/01-evaluation.md`, evidence 101–104 |
| Repository registration, storage recovery, replay, and watchers work | `VERIFIED` for tested workflows | `phase-1/01-evaluation.md`, evidence 101 |
| Supported-language deterministic impact analysis works across clients | `VERIFIED` for cited fixtures and clients | `phase-2/01-evaluation.md`, evidence 102 |
| Simulation is read-only | `VERIFIED` for cited tests | evidence 102 |
| Native Git history and packed/delta fixtures work | `VERIFIED` for cited repositories | `phase-3/01-evaluation.md`, evidence 103 |
| 5,000-loose-commit strict budget is below 150 ms | `VERIFIED` on Ubuntu/macOS/Windows run 80 | `phase-3/01-evaluation.md`, evidence 104 |
| First cited historical frame is below 100 ms | `VERIFIED` for the 8.48 ms fixture | evidence 103 |
| Optimization has a statistically proven causal percentage improvement | `NOT CLAIMED` | `phase-3/02-ablation.md` |
| Arbitrary runtime behavior is completely understood | `NOT CLAIMED` | Phase 0 and Phase 2 limitations |
| Every repository will load below 150 ms | `NOT CLAIMED` | Workload-specific benchmark rule |
| Visual canvas FPS/theme targets are proven by these reports | `NOT CLAIMED` | Separate visual evidence scope |

## Reproduction record

The authoritative reproduction path is the exact commit and workflow run above,
plus the raw evidence files under `docs/evidence/`. Native jobs execute the
workspace, Electron, schema, determinism, architecture, MSRV, TypeScript, and
release-gate checks on Windows, Ubuntu, and macOS. The strict Phase 3 history
gate is enabled with `SYNEVID_PHASE3_LOOSE_HISTORY_GATE=1`.

For local review, inspect the phase-specific evidence first, then compare each
claim to its traceability row. Do not replace raw CI output with a hand-entered
number in a report.

## Product behavior and boundaries

- Analysis is read-only: it does not edit source files or rewrite Git history.
- Explicit Git mutations require user action and recovery receipts.
- `UNKNOWN`, `PARTIAL`, `ERROR`, and `UNAVAILABLE` are non-success states.
- CLI, TUI, MCP, daemon, and web clients are evaluated against shared contracts
  for the tested flows; offline mode must be identified as independent mode.
- The canvas is a presentation surface. These reports cover data and contracts,
  not a change to renderer, animation, camera, layout, or styling.

## Known limitations

Dynamic imports, reflection, generated code, macros, runtime dispatch, and
undocumented external systems can remain unknown or partial. Detailed latency
distributions and a same-runner paired ablation are not yet complete for every
operation. The historical 445.74 ms measurement remains in the audit trail and
is not hidden by the later passing native gate.

## Publication decision

Publish as:

> **SynEvid Phase 0–3: bounded production-oriented verification evidence,
> with native Windows/Linux/macOS CI confirmation for the stated workflows.**

Do not publish as “formally proven safe,” “perfect,” “complete for all runtime
behavior,” or “sub-150 ms for every repository.”

## Referenced standards

The benchmarking and artifact-review conventions used by this report are
linked in `docs/reports/01_PUBLIC_BENCHMARKING_STANDARDS.md`, including official
GitHub runner documentation, Criterion measurement guidance, OpenTelemetry
semantic conventions, and ACM artifact-review guidance.
