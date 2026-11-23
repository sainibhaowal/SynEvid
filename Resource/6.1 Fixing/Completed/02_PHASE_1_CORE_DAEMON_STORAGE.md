# Phase 1 — Core Registry, SQLite Storage, Watcher, and Daemon Truth

## Objective

Make repository registration, storage, scanning, recovery, live events, and daemon APIs reliable and deterministic.

## Current findings

- Repository canonicalization, connected-repository persistence, and daemon-backed
  attach/status are implemented and tested.
- Storage corruption recovery now preserves the main database and WAL/SHM sidecars,
  emits `ERR-STORAGE-102`, and has restart/rollback coverage.
- The live producer-to-consumer path uses immediate in-memory publication plus a
  bounded persistence queue; serial Windows measurements are below the configured
  wake-up budget for the tested process path.
- Filesystem browse, watcher, and simulation behavior are locally verified; native
  Linux/macOS evidence remains a GitHub Actions responsibility.

## Requirements

- Canonical repository identity is based on normalized, verified path—not display label.
- A selected path is canonicalized once and never appended again.
- Missing paths return a structured error and do not fall back to a parent or home directory.
- Storage uses explicit schema versions and migrations.
- Corruption creates a recoverable backup, rebuilds safely, and emits the documented corruption error.
- SQLite transactions are atomic; concurrent readers and one writer have defined behavior.
- Event sequence IDs are monotonic, persisted, and replayable after reconnect or restart.
- Watchers debounce changes, respect symlink and traversal boundaries, and emit source-backed events.
- Shutdown closes storage and WAL state cleanly.

## Data model

Required entities:

- `repository_registry`;
- `active_repository`;
- `snapshot`;
- `file_artifact` keyed by content hash;
- `event_log` with sequence ID;
- `session`, `workflow`, and `job`;
- `error_receipt`;
- `schema_migration`.

Every persisted row has a stable identity and deterministic serialization rules. Operational timestamps are separate from semantic identity.

## Non-requirements

- no automatic deletion of user repositories;
- no silent path fallback;
- no cloud storage requirement;
- no source mutation from scanning or watching;
- no UI-only repository state.

## Backend and core work

- Fix the event wake-up path; measure producer-to-consumer latency rather than polling latency.
- Add cancellation and back-pressure behavior.
- Make corruption recovery return `ERROR` or a dedicated storage error, never a successful empty state.
- Test WAL and `-shm` handling during copy, restart, crash, and recovery. SQLite WAL files are part of persistent database state and must not be separated from the database during backup operations.
- Add path normalization tests for Windows, Linux, WSL, UNC paths, symlinks, and traversal.

## CLI, TUI, and MCP work

- `synevid attach` registers and activates a repository through the daemon.
- `synevid status` reports daemon, active repository, snapshot, event cursor, and storage health.
- CLI offline mode is explicitly labeled and never silently confused with daemon mode.
- TUI reads the daemon event stream and uses the same repository/session IDs.
- MCP receives repository identity and authenticated daemon access from the same contract.

## Frontend and UX work

- Show the canonical path, repository ID, active snapshot, daemon connection, last event sequence, and freshness.
- Show “scanning,” “reconnecting,” “stale,” and “replay complete” states.
- Remove any path shortcut or remembered entry that is not backed by the registry.
- Do not alter the canvas renderer; only update repository data through view-model state.

## Tests

- unit tests for identity, migration, transaction, recovery, and event ordering;
- integration tests for restart replay and concurrent clients;
- real filesystem test with 2,000 files;
- real watcher test with external editor and CLI writes;
- corruption and partial-write tests;
- Windows and Linux path matrix;
- E2E UI test: select repo, restart daemon, reopen UI, verify same canonical repo;
- latency distribution test with p50, p95, p99 and sample count.

## Logging

Log structured events with `operation_id`, `repository_id`, `snapshot_id`, `event_seq`, duration, result state, and error code. Never log source contents, secrets, or full user paths unless explicitly enabled for local debugging.

## Gate G1

G1 passes when storage recovery, path selection, event replay, watcher behavior, and the latency target pass on supported platforms with real processes.

### G1 implementation record

The Windows evidence dossier records storage 19/19, `synevid-ui` unit tests
26/26, native daemon process E2E 1/1, Phase 1 HTTP replay/corruption coverage,
real Electron/browser repository selection plus daemon restart/replay, and
producer-to-consumer event wake-up p95 below the configured budget. The full
workspace gate also passes when Python 3.12 is supplied explicitly. The native
Linux/macOS portions are configured in `.github/workflows/ci.yml` and remain
runner evidence until those jobs execute; this document does not claim that a
workflow definition is the same as a completed remote run.

## Technical references

- SQLite WAL behavior: https://www.sqlite.org/wal.html


