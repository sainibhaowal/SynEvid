# Production Evidence Receipt: 101

> **Benchmark correction (2026-11-23):** this receipt's legacy impact-gate
> telemetry is superseded as accuracy evidence by [evidence 108](108-impact-benchmark-integrity-audit.md).

**Topic:** Phase 0 truth governance and Phase 1 daemon/storage closure
**Milestone:** v0.6.0
**Date:** 2026-11-05
**Scope:** `Resource/6.1 Fixing/00_MASTER_PRODUCTION_PROGRAM.md`,
`01_TRUTH_GOVERNANCE_AND_REQUIREMENTS.md`, and
`02_PHASE_1_CORE_DAEMON_STORAGE.md`

## Outcome

The Phase 0 and Phase 1 implementation gaps in the audited production paths
are repaired and verified on the current Windows host. The implementation does
not claim arbitrary runtime coverage or cross-platform performance from a
Windows-only run. Unsupported or unavailable evidence remains `UNKNOWN`,
`PARTIAL`, `ERROR`, or `UNAVAILABLE` rather than becoming `PASS`.

## Implemented changes

### Truth and evidence governance

- Removed the embedded terminal's fixed blast-radius sentence and wired
  `autopsy impact -s <stable-id>` to the indexed repository graph through
  `ImpactEngine` in `apps/synevid-ui/src/server.rs`.
- Changed Git tree-decoding failures in
  `crates/autopsy-git-engine/src/reader.rs` to `UNKNOWN`; a corrupt or
  incomplete tree is never treated as a safe `PASS`.
- Removed hard-coded AI cockpit savings, hit-rate, and latency values. The
  cockpit now displays measured session data or an explicit not-measured state.
- Removed static demo transcripts and fake repository results from
  `apps/web/src/components/TerminalWorkbench.tsx`. Terminal output now comes
  from the daemon command endpoint; the quick-run actions remain real commands.
- Replaced legacy dashboard placeholders (demo symbols, counts, latencies,
  verdicts, and buffers) with repository-backed, not-measured, or unknown
  states in `apps/synevid-ui/src/dashboard.html`.
- Historical canvas selection now uses `UNKNOWN` coverage and no latency claim
  because selecting an event is not the same operation as running a fresh
  simulation (`apps/web/src/app/page.tsx`).

### Canonical daemon and client contract

- Added authenticated CLI commands `synevid attach <path>` and
  `synevid status` in `crates/autopsy-cli/src/daemon.rs`. Attach canonicalizes
  the path once and refuses missing/non-directory paths without fallback.
- Added the `/api/v1/status` alias and status fields for daemon connection,
  active repository, event cursor, freshness, and storage health.
- Added MCP loopback bearer-token propagation in
  `apps/mcp-server/src/server.ts`.
- Changed live-event publication to wake in-memory subscribers immediately and
  persist through a bounded daemon-owned queue worker. This removes synchronous
  SQLite work from the producer path, applies back-pressure when persistence
  falls behind, and preserves persisted sequence IDs and replay after restart.
- Kept the existing canvas renderer, animation, camera, WebGL behavior, and
  styling outside this change. The pre-existing worktree modification in
  `apps/web/src/components/CodeUniverse3D.tsx` was preserved and not edited by
  this implementation.

## Verification evidence

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo check -p synevid-ui -p autopsy-cli` | PASS |
| `cargo test --workspace --all-targets -- --nocapture --test-threads=1` | PASS; zero failed tests, with `PYTHON` explicitly set to the installed Python 3.12 runtime |
| Phase 1 HTTP end-to-end | PASS; corruption `ERR-STORAGE-102` and randomized edit determinism |
| Storage suite | PASS; 19/19, including migrations, WAL/SHM, corruption recovery, transaction rollback, and lifecycle persistence |
| `synevid-ui` Rust tests | PASS; both unit binaries 26/26 plus native process E2E 1/1 |
| Phase 1 event wake-up | PASS; measured producer-to-consumer SSE p95 0.214–0.453 ms in serial runs |
| Phase 1 filesystem browse | PASS; 1,000-directory p95 7.580–13.023 ms in serial runs |
| Phase 1 2,000-file daemon test | PASS; measured p95 23.132–23.346 ms, analysis-reported p95 21 ms |
| MCP typecheck and tests | PASS; 49/49 |
| Web production build | PASS; Next.js compiled and TypeScript completed |
| Web Inspector suite | PASS; 38/38 behavioral/source-contract checks |
| Python benchmark harness | PASS; 3/3 with Python 3.12 explicitly configured. Its report still records failed cost-cut/new-failure sub-gates; this is not promoted to a stronger claim. |
| Native CLI process E2E | PASS; real `attach` and `status` against the workspace daemon, canonical path and persisted event cursor returned |
| Native daemon process E2E | PASS; `synevid-ui` child process attach/status, live broadcast, cursor replay, shutdown, restart, and persisted replay |
| Targeted fabricated-output scan | PASS for audited production paths; former fake counts, metrics, transcripts, and fallback verdicts absent. Test fixtures and documentation examples remain explicitly non-production. |

## Honest boundary

- The full workspace gate is green on this Windows host when the installed
  Python 3.12 executable is supplied through `PYTHON`. A run without that
  environment setting fails only because the benchmark test cannot discover
  Python in this shell's `PATH`; it is not counted as product evidence.
  Linux/macOS native runner evidence is not produced by a local Windows
  process and must come from the repository's native CI jobs.
- The full benchmark harness test passes, but its printed pre-registered
  telemetry reports recall lift `+31.73pp` while its separate cost-cut and
  new-failure-class sub-gates are reported as failed. This receipt does not
  convert those measurements into a stronger claim.
- `PASS` means no violation was found within the analyzed and verified scope;
  it does not prove arbitrary dynamic runtime behavior.
- Performance numbers above are samples from serial Windows test runs. They
  are evidence for those runs, not a universal hardware guarantee.

## Production-program completion additions

The following contract work is now executable rather than narrative:

- `docs/requirements/registry.yaml` registers each Phase 0/1 requirement with a
  source, owner, acceptance test, evidence file, and state.
- `scripts/validate-requirements.py` fails when any registry field, referenced file,
  or traceability entry is missing.
- `schemas/evidence-envelope.schema.json` defines the shared daemon/MCP contract.
  Every daemon JSON response is decorated with repository identity, canonical path,
  snapshot, coverage, verdict, warnings, latency (or null when no deterministic
  producer sample exists), BLAKE3 digest, and receipt fields.
- `docs/adr/0011-one-daemon-multi-client-synchronization.md` records the daemon as
  the source of truth for CLI, TUI, MCP, Electron, and web clients.
- The TUI now reads `/api/v1/status` and replays `/api/v1/live/poll` from its last
  cursor when connected; disconnected analysis is explicitly labeled offline.
- SQLite tests cover event cursor restart, WAL/SHM sidecar preservation during
  corruption recovery, and transaction rollback atomicity.
- `SYNEVID_API` structured logs carry operation ID, repository ID, snapshot ID,
  event sequence, duration, result state, and error code without logging source
  contents or canonical paths.
- `apps/web/test/phase1_daemon_browser.behavior.test.mjs` drives a real
  Electron instance against a real `synevid-ui` process and a temporary
  repository; the 20-test web suite passed, including daemon restart and event
  replay checks.
- The graph worker now uses the configured daemon URL rather than the Next.js
  origin, so browser tests and alternate daemon ports use the same authority as
  the desktop runtime.
- The explicit live-broadcast acknowledgement persists the event before
  returning, closing the persistence/replay race exercised by the native process
  test. Internal watcher publication remains queued and bounded.
- The ordinary indexer now marks only conservatively proven private/internal
  symbols as statically unreferenced; dynamic or partial coverage is not
  converted into a dead-code claim.
- The aggregate GitHub Actions gate now includes the Electron packaged-smoke
  and Phase 3 history-budget job results in both its summary and failure
  condition; a failed required job cannot be hidden by the aggregate report.

These additions are implemented and locally verified on Windows. The registry
statuses for cross-platform native process/browser behavior remain `IMPLEMENTED`
until the corresponding GitHub Actions runner jobs execute; this dossier does not
turn CI configuration into evidence that has not run.

