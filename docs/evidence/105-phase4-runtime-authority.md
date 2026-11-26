# Evidence 105 — Phase 4 Authenticated Runtime Authority

Status: `VERIFIED — GATE G4`

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

## Native CI closure evidence

GitHub Actions run [44769225870](https://github.com/sainibhaowal/SynEvid/actions/runs/44769225870)
passed the complete `CI Quality & Cross-Platform Matrix` and its
`Pre-Merge Verification Gate` (`136409185910`) on commit `6f78ba4`.

The run passed the real Windows, Ubuntu, and macOS jobs for Rust workspace
tests, MSRV, formatting and Clippy, determinism and architecture boundaries,
schema/config validation, cargo-deny, TypeScript/MCP/web checks, Electron
native browser/packaged smoke, and the strict 5,000-loose-commit history
budget. The Phase 4 native runtime test
`apps/synevid-ui/tests/phase4_runtime_native.rs` therefore ran on all three
native runners as part of the workspace suite, while the Electron browser
workflow verified repository identity, daemon restart, and replay cursor
behavior on all three operating systems.

The strict history measurements in this run were 118.03 ms on Windows,
70.16 ms on macOS, and 29.80 ms on Ubuntu for the 5,000-loose-commit
acceptance fixture. These are runner measurements, not a universal latency
guarantee for every repository.

This closes G4 for the stated protocol and CI scope. It does not claim that
static analysis proves arbitrary dynamic runtime behavior, nor that unsigned
desktop artifacts satisfy OS signing/notarization trust requirements.

No canvas renderer, animation, camera, WebGL buffer, or canvas styling file was
changed for this work.
