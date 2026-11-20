# Evidence 105 — Phase 4 Authenticated Runtime Authority

Status: `IMPLEMENTED — GATE PENDING`

Date: 2026-11-20

## Scope

This dossier covers the stricter Phase 4 G4 protocol in
`Resource/6.1 Fixing/05_PHASE_4...` (one daemon authority for CLI, TUI, MCP,
and UI). It does not promote the older Phase 4.2 trajectory evidence to G4.

## Changes made

- Added a stable daemon identity derived from the canonical daemon root.
- Added authenticated runtime connect, heartbeat, and disconnect endpoints with
  client identity, client type, repository/session/workflow context, lease ID,
  and event cursor.
- Added runtime metadata to `/api/v1/status` and service discovery.
- Added source-client and evidence-digest metadata to persisted live events and
  forensic trajectory evidence.
- Added explicit replay gap metadata (`gap`, `gap_from`, `gap_to`,
  `replay_complete`) and preserved monotonic sequence initialization on restart.
- Added CLI `active-repo`, `events`, and `disconnect` commands and an explicit
  `--offline` flag. TUI fallback is refused unless offline mode is explicit.
- Changed MCP impact/verify to use authenticated daemon endpoints by default;
  independent CLI execution requires `offline=true`.
- Added UI client identity and runtime lease acquisition.
- Added `schemas/runtime-envelope.schema.json`.

## Local evidence

| Check | Result |
|---|---|
| MCP TypeScript typecheck | PASS |
| MCP lint | PASS |
| MCP test suite | PASS — 49/49 |
| Web TypeScript (`npx tsc --noEmit`) | PASS |
| Web ESLint | FAIL — existing 42 errors/119 warnings across UI files; no canvas files were changed for this Phase 4 protocol |
| Web tests | 18/20 passed; 2 native-browser daemon tests unavailable because the local Rust daemon could not link without `link.exe` |
| Rust formatting (`cargo fmt --all -- --check`) | PASS |
| Rust targeted compile | BLOCKED by local environment: MSVC `link.exe` is not installed |
| Python validators | NOT RUN: Python is not installed on this Windows host |

The Rust unit/integration runtime tests are present in
`apps/synevid-ui/src/server.rs` and must be executed by the native CI matrix.
The two web failures are environment evidence, not a pass: they time out while
waiting for a daemon process that could not be built locally.

## Required G4 closure evidence

The following must be attached from native CI before marking this dossier
`VERIFIED`:

1. Rust format, clippy, targeted runtime tests, and workspace tests on Windows,
   Ubuntu, and macOS.
2. Native process test for missing token, wrong token, valid token, expired
   lease, attach/status/events/disconnect, and replay after restart.
3. CLI attach → daemon → UI status parity and MCP impact/verify → daemon event
   → UI replay parity.
4. Runtime envelope schema validation and exact event sequence/digest parity.

No canvas renderer, animation, camera, WebGL buffer, or canvas styling file was
changed for this work.
