# Evidence 107 — Phase 6 Analytics Truthfulness Follow-up

**Recorded:** 2026-11-23 (Windows verification follow-up)  
> **Impact benchmark correction (2026-11-23):** any benchmark accuracy/gate
> values in this dossier are superseded by [evidence 108](108-impact-benchmark-integrity-audit.md).
**Environment:** Windows, local worktree  
**Base commit:** `093a405ed715db127449f9d4e25b43456c700006` (changes remain uncommitted)  
**Overall G6:** **PARTIAL**; this dossier does not assert Phase 6 production readiness.

## Change made

### Follow-up implementation in this work session

- Added storage migrations 008 and 009. `GraphNodeRow` and indexer writes no
  longer expose or read `is_dead_code`; SQLite retains the legacy column only
  to avoid destructive table rebuilds. Legacy values are not promoted into
  findings. The new `static_findings` records include version, state, basis,
  coverage, reason, and digest.
- The daemon now computes the static findings, atomically replaces the records
  for the exact snapshot/version, reads them back, and fails the report request
  if validation or persistence fails.
- Added persisted runtime topology-window records with payload digest and
  required/observed/complete coverage. Authenticated `POST /api/v1/analytics/runtime-window`
  accepts a typed, engine-digest-verified topology report only for the active
  repository's current snapshot. Findings are included only from complete,
  verified, sufficient windows.
- Corrected `POST /api/v1/analytics/otlp-ingest`: it now routes OTLP/JSON through
  the telemetry crate's strict parser, sanitizer, and topology correlator over
  the active snapshot's declared symbols. It requires a private 32-byte hex
  `SYNEVID_TELEMETRY_SALT`; it does not use the public test salt. A single
  export is not treated as complete observation (required window count is 3,
  complete is false). Ingestion identity is content-derived, not process-ID
  derived. The current Windows Rust unit test and rebuilt-source Electron
  integration both pass this route against a real indexed fixture repository.
- Report generation now writes a canonical JSON `ReportHistoryRecord` with the
  report ID, repository/snapshot scope, and BLAKE3 digest. The existing history
  list/detail routes can therefore return generated reports rather than only
  expose an unused table. A generate/list/fetch receipt regression was added
  and passed against the current Windows source build.
- Corrected a data-loss edge in `StorageEngine::save_snapshot`: SQLite
  `INSERT OR REPLACE` deletes and reinserts the snapshot, firing foreign-key
  cascades that erased snapshot-pinned report receipts (and runtime windows)
  when an identical snapshot was re-indexed. Snapshot persistence now uses an
  in-place `ON CONFLICT DO UPDATE`; a storage regression verifies that report
  history survives saving the same snapshot again.
- The daemon's semantic Git-risk section now runs the native Git differ on HEAD
  versus its first parent and reports the diff digest, symbol/edge/contract
  deltas, coverage, and verdict. This is a bounded current-HEAD comparison, not
  a semantic scan of all historical commits. Failures remain `ERROR`; absent
  parent/HEAD remains `UNKNOWN`/`UNAVAILABLE`.
- Added CLI `analytics-report`, MCP `autopsy_analytics_report`, and TUI `a`
  shortcut, all fetching the daemon report instead of maintaining parallel
  analytics logic. The web client already reads the same route.

- Analytics computes static-unreferenced candidates from the persisted graph,
  not from `GraphNodeRow.is_dead_code`. The candidate includes stable symbol ID,
  source path/line, snapshot, coverage, reason, residual unknowns, zero inbound
  non-containment edge count, and a BLAKE3 digest of the canonical edge set.
- Candidate wording expressly does not claim global dead code. Public/exported
  declarations, conventional entrypoints, generated directories, and test-only
  paths are excluded. Low-coverage no-inbound results are `unknown`, not
  candidates.
- Git file-change scores are exposed only as `file_heuristic_findings`; the
  current HEAD/first-parent semantic result comes from the adapter-backed native
  differ. Other historical commits are not represented as semantically checked.
  A Git read error yields `ERROR`/null rather than empty success counts.
- Analytics sections identify scope and status. Newly-dead-after-edit,
  performance/resource, and error/recovery sections remain `UNAVAILABLE` until
  their real query sources are integrated. Runtime-unobserved is unavailable
  with no stored window and partial when only insufficient windows exist.
  AI-ledger query failure no longer renders fabricated zero metrics.
- Added JSON, Markdown, HTML, SARIF, and a deterministic JSON bundle containing
  all four artifacts with per-artifact BLAKE3 digests.
- The web report panel shows section availability, scope, emitted evidence
  basis/coverage/digest, and a link to fetch the persisted report receipt;
  missing provenance remains labeled as not emitted. Selecting a static
  candidate selects its file/symbol in the existing canvas data model. No
  renderer, camera, animation, WebGL, or canvas styling file was changed.
- Extended the real Electron + Next + daemon browser workflow to load the
  analytics report and retrieve its durable receipt, including after daemon
  restart and inspect repository report history. It now first posts real OTLP
  JSON for a known indexed symbol and checks the parsed span count, snapshot,
  conservative incomplete-window state, report history, and receipt digest.
  The browser test also checks that the report displayed by the UI has a
  persisted list entry and matching receipt before confirming UI receipt
  rendering. A fresh current-source Windows build now passes the complete
  Electron/daemon workflow, including restart and event replay. Earlier 404 and
  missing-history failures were from the stale prebuilt binary and exposed the
  snapshot replacement cascade fixed above.
- Added a Phase 6 execution plan separating the actual PRD scope from
  out-of-scope work. Phase 6 requires semantic Git risk, not a semantic scan of
  every historical commit. Current `HEAD` versus first-parent comparison is
  explicitly bounded. Do not silently expand the requirement to full-history
  semantic analysis.
- Fixed the CI dependency install command to use `scripts/requirements.txt`
  (the previous unquoted `>=` could be interpreted as shell redirection), made
  Phase 6 schema fixtures mandatory, registered local schemas by `$id`, and
  added positive/negative schema-instance tests.
- Added `phase6-analytics-browser-e2e` to the GitHub Actions pre-merge gate. It
  builds the daemon from the checked-out revision, installs web/Electron
  dependencies, and runs the receipt/history browser journey under Xvfb. The
  workflow YAML was parsed locally; Actions has not yet run for these edits.

## Verification evidence

| Check | Result |
|---|---|
| Full Rust workspace | Passed: release/all-targets serial gate; 1,066 passed, 0 failed, 3 isolated timing tests ignored in broad run |
| Rust formatting, clippy, docs | `cargo fmt --all -- --check`, workspace Clippy with `-D warnings`, and workspace doc tests passed |
| Isolated performance checks | Release SSE wake-up p95 0.513 ms (<15 ms); 1,000-file simulation p95 25.076 ms (<50 ms) |
| Python schema validation | Passed: all 18 Draft 2020-12 schemas/references; `test_phase6_schemas.py` 3/3 including malformed-instance rejection |
| Python test suites | `pytest benchmarks/tests/ scripts/tests/ -q`: 8/8 passed |
| Current web checks | TypeScript passed; web suite 21/21, including real Electron + Next.js + rebuilt daemon, OTLP ingest, report/history/receipt, restart replay |
| Impact benchmark integrity | Legacy RQ1-RQ5 output was withdrawn after audit found incomplete/unreviewed oracles, missing query scope, and answer-key leakage. No impact accuracy or gate score is accepted as valid evidence; see dossier 108 |
| Earlier default-parallel run | Exposed a live sequence collision across daemon instances. Fixed by making event sequence state per-daemon and persisted-sequence-derived; full suite subsequently passed serially. Isolated timing tests pass; no general latency guarantee is implied |
| GitHub Actions | No Actions run exists for this uncommitted source state. This check was Windows-only by user direction; Linux/macOS are not claimed |
| Phase 6 corpus evaluation and ablation | Protocol only; no frozen Phase 6 analytics results are claimed |

## Explicit remaining work (G6 blockers)

1. The OTLP route requires operator configuration of a private
   `SYNEVID_TELEMETRY_SALT`; CI/package setup must provide it securely or
   surface the structured configuration error. An external collector is not
   implemented or claimed.
2. Semantic Git risk is bounded to HEAD versus first parent; this is consistent
   with Phase 6's report scope. Merge-parent behavior and real repository
   semantic-diff fixtures still need verification; all-history semantic risk is
   not a Phase 6 requirement unless formally added.
3. Report history now persists on generation, validates read-back, and survives
   re-indexing the same snapshot without FK-cascade deletion. Rust and Electron
   Windows regressions pass on the current worktree.
4. Provenance rendering and a real Electron receipt/history workflow pass
   locally. Full metric-specific source drill-down remains limited to evidence
   fields actually emitted by each section; missing fields remain visibly
   unavailable.
5. Schema validation now passes locally on Windows. Still unproven: a frozen
   Phase 6 analytics evaluation and ablation, independent semantic Git-risk
   scoring on reviewed real repositories, and exact-commit CI evidence. Linux
   and macOS are intentionally outside this Windows-only verification scope.
   The broad Rust gate passed serially; isolated timing gates passed separately.

No performance, complete runtime-observation, or semantic-risk quality result
is claimed without current source-backed measurements. The user's pre-existing
`apps/web/tsconfig.json` changes were preserved; temporary type roots added by
Next.js during the browser test were removed without reverting the prior diff.
