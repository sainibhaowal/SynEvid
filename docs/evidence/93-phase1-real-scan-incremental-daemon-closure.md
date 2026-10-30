# Phase 1 Real Scan, Incremental Daemon, and Persistence Closure

**Date:** 2026-10-29  
**Scope:** Phase 1 MUST/SHOULD requirements in `01_PHASE_1_v0.2.0_PLUGGABLE_CORE_SQLITE_AND_REALTIME_DAEMON.md`

## Delivered

- Repository connect, refresh, switching, removal, and restart recovery use real repository scans and persisted snapshots/graphs. Status, language/workspace capabilities, graph edges, call sites, counts, and file discovery come from analyzed repository data rather than fixed examples.
- The plugin registry now dispatches language adapters, workspace resolvers, verification gates, graph layouts, and storage providers. Six analyzer families and eight workspace resolvers are registered; tests also exercise a mock extension without editing the CLI or repository scanner.
- SQLite uses WAL for repository storage. Snapshot content, graph nodes/edges, sessions, workflows, and live events persist; schema migration 007 adds incoming/outgoing ego-query indexes. Corruption recovery preserves the damaged database and rebuilds missing analysis from source.
- Incremental overlays parse changed files, update file-local graph edges, compute contract/caller impact, and report measured elapsed time without writing the editor buffer to disk. Cache reuse includes a deterministic config/workspace-resolution digest; a `tsconfig.json` alias change now forces edge re-resolution rather than returning stale callers.
- File and Git metadata watchers publish replayable events. SSE waits on a condition-variable signal rather than polling every 200 ms; event sequence IDs and missed-event replay remain backed by SQLite.
- Filesystem browsing detects Git repositories and lists directories in deterministic order. Git-marker checks are bounded to four workers. Removal unregisters a repository without deleting its source tree.
- Snapshot/layout/delta determinism, canonical BLAKE3 export, SQLite recovery, line-accurate semantic edges, and honest `Unknown`/`Partial` coverage have regression tests.

## Verification

- `cargo test --workspace --all-targets` — passed; the final UI-specific debug and release suites were rerun after the bounded filesystem-browse change.
- `cargo test --workspace --doc` — passed.
- `cargo fmt --all -- --check`, focused `cargo check`, and `git diff --check` — passed.
- `cargo test -p synevid-ui` and `cargo test --release -p synevid-ui` — 14/14 each, including repository lifecycle, real file/Git watcher replay, live-event wake-up, browse, and 2,000-file simulation.
- `cargo test -p autopsy-tests --test phase1_incremental_determinism` — 6/6 debug and release; includes 100 incremental-vs-cold overlay comparisons, resolver-config invalidation, 1,000-file polyglot performance, 1,000 nodes/5,000 edges, recovery, and 100-run layout determinism.
- Release measurements on WSL2, x86_64, 8 vCPUs, 12th Gen Intel Core i5-12450H, 15 GiB RAM: 2,000-file polyglot daemon handler p95 **42.943 ms**; 1,000-directory browse p95 **10.738 ms**; live-event wake/replay query p95 **2.316 ms**; 1,000-node/5,000-edge SQLite ego query p95 **3.351 ms**; 1,000-file incremental overlay p95 **17.073 ms**.
- `./scripts/verify-determinism.sh` — passed 100/100; `./scripts/verify-arch-boundaries.sh` — passed; JSON schema validation — passed.
- `make check` — passed, including workspace tests and doc tests; the Makefile skipped Clippy because `cargo-clippy` is not installed.
- `cd apps/web-inspector && npm test` — 38/38; `cd apps/web && npm run build` — passed; `cd apps/desktop-electron && npm test` — 7/7; `bash scripts/tests/test-install.sh` — 44/44.

## Limits and Scope

- The file/Git watcher tests verify real OS notifications and eventual persisted replay. The 15 ms measurement covers event wake-up after the daemon broadcasts an event; it does **not** claim end-to-end filesystem-change-to-client latency (the watcher intentionally debounces file changes).
- The browse and simulation latency gates are measured in the release profile on the stated WSL2 machine; results are hardware/load specific. The HTTP handler benchmark invokes the daemon handler directly rather than measuring network transport.
- `FR-P1-007`, the `.wasm` policy loader, is MoSCoW **Could** and remains explicitly deferred; no WASM runtime or plugin execution is claimed. All Phase 1 MUST and SHOULD criteria are verified.
- `cargo clippy --workspace --all-targets -- -D warnings` could not run because this environment has no `cargo-clippy` command and no `rustup` component manager. The workspace test and compile gates passed; clippy remains an environment limitation.
