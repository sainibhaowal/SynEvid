# Phase 6 implementation and proof plan

**Status:** Active; G6 is not yet verified. This plan separates code completion
from platform and evaluation evidence. It does not certify production readiness.

## Scope decisions

- Phase 6 requires a semantic Git-risk report and Git velocity/change
  concentration. The current report compares `HEAD` with its first parent and
  clearly labels bounded metadata. The Phase 6 PRD does not require semantic
  re-analysis of every historical commit; do not expand that scope silently.
- The Phase 3 history-reader requirement to enumerate history is separate from
  the Phase 6 semantic-risk evidence requirement. If all-history semantic risk
  is desired, add a versioned requirement, cost budget, and acceptance test
  before implementing it.
- OTLP intake is an authenticated daemon endpoint, not a background collector.
  It must parse and sanitize real OTLP/JSON and run the topology engine. A
  collector/exporter, transport listener, or service-specific instrumentation
  is not implied by the current PRD.
- Static zero-inbound results remain candidates, never absolute dead-code
  claims. Runtime absence remains unknown/partial until sufficient complete
  windows exist. Unsupported evidence is never promoted to `PASS`.
- No canvas renderer, camera, animation, layout, or styling work is needed.
  Repository/finding selection may update only the existing view-model path.

## Work plan and acceptance matrix

| Order | Work | Exact implementation surface | Acceptance evidence | Current state |
|---|---|---|---|---|
| 1 | Evidence-backed static candidates | `apps/synevid-ui/src/server.rs`, `crates/autopsy-storage/` migrations and finding rows | Known graph fixtures; exports/entrypoints excluded; low coverage is `unknown`; persistence round-trip | Implemented; current Windows daemon unit suite passed with 33 library and 33 binary tests |
| 2 | Real runtime intake | `POST /api/v1/analytics/otlp-ingest`, `POST /api/v1/analytics/runtime-window`, `crates/autopsy-telemetry/` | Strict OTLP parse → privacy sanitizer with private key → graph correlation → pinned window; malformed, empty, stale-snapshot, and insufficient-window cases never pass | Current Windows parser/sanitizer/correlator test and live Electron ingest passed; private `SYNEVID_TELEMETRY_SALT` remains an operator prerequisite |
| 3 | Durable report history | report builder and `report_history` storage/routes | Generate report, restart daemon, list and fetch same canonical receipt; deterministic ID/digest; operational timestamp excluded from digest; persistence failure is an error | Current Windows storage regression and browser restart/receipt flow passed; snapshot upsert no longer cascades deletion of report history |
| 4 | Metric provenance UI | `AnalyticsReportsSection.tsx`, web API client | Each card displays only source basis/coverage/digest actually emitted; receipt link resolves stored report; missing source remains visibly unavailable; candidate drill-down selects existing file/symbol | TypeScript check and full browser suite passed 21/21; provenance remains bounded to fields actually emitted |
| 5 | Browser E2E | `apps/web/test/phase1_daemon_browser.behavior.test.mjs`, runner, `phase6-analytics-browser-e2e` CI job | Real user path opens Repositories workspace, loads JSON report, fetches persisted receipt, then repeats after daemon restart; assert displayed scope/digest | Rebuilt-source local Windows E2E and full web suite passed 21/21; no Actions run for this uncommitted source |
| 6 | Schema/instance validation | `schemas/analytics-*.schema.json`, `schemas/runtime-observation-window.schema.json`, validator and testdata, CI | Validate draft, local `$ref`s, valid Phase 6 instances, malformed digest negative case; missing required fixtures fail CI | Windows Python validator passed all 18 schemas; positive/negative pytest passed 3/3 |
| 7 | Semantic risk boundaries | native Git differ and report builder | Fixtures prove file-count and semantic findings differ; `HEAD`/first-parent OIDs and digest present; missing/corrupt data is `UNKNOWN`/`ERROR`; merge-parent behavior stated | Bounded semantic path exists; broader fixture evaluation and merge-parent policy evidence remain |
| 8 | Evaluation and ablation | `docs/reports/phase-6/01-evaluation.md`, `02-ablation.md`, frozen corpus manifest/results | Versioned corpus and labels, independent review, reproducible commands, raw results, per-language precision/recall/unknown rate, resource samples, arm comparisons; no tuning on held-out split | Phase 6 corpus/ablation still absent. Existing RQ1-RQ5 impact benchmark is not a substitute; its poor precision/F1 and failed cost/failure-class criteria are reported |
| 9 | Platform proof | `.github/workflows/ci.yml` and exact-SHA workflow runs | Native jobs pass on platforms in the declared support matrix; preserve run/artifact URLs | Windows local checks only for this uncommitted worktree; no commit/Actions evidence. Linux/macOS were excluded from this immediate check by user direction; no cross-platform claim is made |
| 10 | Gate review | `docs/requirements/TRACEABILITY.md`, evidence dossier, changelog | Every requirement maps to a test and exact run; distinguish implemented, tested, and platform-verified; G6 is verified only if each required row has evidence | Must remain `PARTIAL` until all required proof is collected |

## Safe implementation sequence

1. Keep analysis and report generation read-only with respect to source and Git.
   OTLP persistence is an explicit telemetry intake action; it may write only
   SynEvid's local evidence database.
2. Pin every observation/report to the active repository and exact snapshot;
   reject stale or caller-selected foreign scope.
3. Use the telemetry crate parser, redactor, and correlation engine. Never
   accept a caller-supplied “complete” assertion as proof of completeness.
4. Persist canonical report JSON idempotently by semantic report ID. Store
   timestamps only as operational metadata.
5. Show source basis, coverage, digest, and persisted receipt in the UI. If a
   source query fails or emits no provenance, show unavailable; do not show 0.
6. Run narrow tests first, then the real Electron browser test, the full web
   suite, Rust focused tests, schema tests, and finally the workspace/platform
   matrix. Do not claim tests that were not run against the current source.
7. Run evaluation and ablation only against a frozen, licensed, reproducible
   Phase 6 corpus with fixed labels. Preserve raw results and environment
   metadata. The existing impact corpus is a different research question and
   cannot substitute for this evidence.

## Explicitly not needed for G6

- Rewriting the 2D/3D canvas or changing its visual design.
- Re-scanning all Git history semantically unless Phase 6 scope is formally
  expanded; current bounded scope must remain visible in each report.
- A cloud collector, LLM, paid observability service, GPU, or hosted database.
- Claiming static analysis proves dynamic/reflection/plugin behavior.
- Treating a test count as code coverage or platform proof.

## Completion rule

G6 is not complete when code merely compiles or a route returns HTTP success.
It is complete only after real stored evidence, schema instances, behavioral
browser flow, negative/error cases, evaluation records, and native platform
workflow runs all pass on one identified source revision. If any external or
corpus input is missing, record that blocker and keep the gate partial.
