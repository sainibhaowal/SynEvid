# Phase 1 Literal PRD Closure

**Date:** 2026-11-01  
**Scope:** Complete the five previously identified Phase 1 gaps, including the optional FR-P1-007 capability.

## Implemented

- `autopsy-storage` now retains a structured recovery notice after preserving a corrupt SQLite database. The daemon health and discovery responses expose `ERR-STORAGE-102`, the normalized `.bak` path, and the rebuild message.
- `autopsy-wasm-plugin` loads `.autopsy/plugins/*.wasm` in normalized path order. Wasmi execution rejects all imports, requires the documented memory/`alloc`/`evaluate` ABI, caps modules at 16 pages and 2,000,000 fuel, and validates the deterministic JSON gate result.
- `tests/tests/phase1_http_end_to_end.rs` starts a real tiny-http listener and sends 100 randomized edit/revert requests through `/api/v1/simulate/edit`. Every returned candidate snapshot ID matches an independent cold scan. The same test covers the ego path/query forms, SSE replay, same-path live POST, and a WebSocket handshake plus client-to-server/server-to-client event roundtrip.
- `/api/v1/graph/ego/:id` is supported without removing the existing query form. The web cockpit now prefers the PRD path form.
- `/api/v1/live` supports a true WebSocket upgrade, typed JSON event frames, monotonic persisted sequence IDs, replay cursors, ping/pong, close handling, and the existing SSE/POST/poll compatibility surfaces.

## Verification evidence

- `cargo test -p autopsy-storage test_phase1_sqlite_corruption_self_healing` — passed.
- `cargo test -p autopsy-wasm-plugin -- --nocapture` — 4/4 passed.
- `cargo test -p synevid-ui test_phase1_corruption_recovery_is_reported_by_daemon_api` — passed in both library and binary targets.
- `cargo test -p autopsy-tests --test phase1_http_end_to_end -- --nocapture` — 2/2 passed; 100 HTTP edit/revert iterations plus the HTTP corruption contract, route/live/WebSocket assertions.
- `cargo test -p autopsy-tests --test phase1_incremental_determinism -- --nocapture --test-threads=1` — 6/6 passed, including the existing 100-run golden and incremental edit coverage.
- `cargo clippy -p autopsy-wasm-plugin -p synevid-ui --no-deps --all-targets -- -D warnings` — passed.
- `cargo check --workspace --all-targets` and `./scripts/verify-arch-boundaries.sh` — passed.
- `verify-determinism.sh` passed through Git for Windows Bash with 100/100 byte-identical runs. `verify-arch-boundaries.sh` passed through the same Bash runtime. `tests/tests/test-install.sh` executed and correctly skipped its Linux/macOS-only installer checks on Windows.
- `cargo fmt --all` and focused `cargo check -p synevid-ui -p autopsy-tests` — passed.

`cargo test --workspace --all-targets -- --nocapture --test-threads=1` passed all workspace targets on the final run, including the Phase 1 HTTP, corruption, replay, determinism, and Python benchmark suites. The available Bash gates also ran successfully; only the installer script's Linux/macOS-specific checks were skipped on this Windows host.

The implementation does not modify canvas animation, graph rendering, or UI visual design. The WebSocket is an additional daemon transport contract; existing browser SSE behavior remains supported.
