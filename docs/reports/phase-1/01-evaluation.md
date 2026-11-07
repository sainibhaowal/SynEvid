# Phase 1 Evaluation — Core Registry, SQLite Storage, Watcher, and Daemon

**Report ID:** `EVAL-P1-2026-11-07`  
**Source:** `Resource/6.1 Fixing/02_PHASE_1_CORE_DAEMON_STORAGE.md`  
**Gate:** G1  
**Status:** `VERIFIED` for the tested workflows and native CI matrix  
**Primary evidence:** `docs/evidence/101-phase0-phase1-truth-and-daemon.md`

## Decision

Repository identity, persistence, recovery, event replay, watcher behavior, and
daemon APIs are verified for the cited workflows. The report does not claim
that every filesystem or operating-system configuration has been exhaustively
tested.

## Functional evidence

| Area | Evidence | Result |
|---|---|---|
| Registry and canonical path | Canonical path tests; selected paths are not appended twice | `VERIFIED` |
| Missing-path behavior | Structured error without parent/home fallback | `VERIFIED` |
| Storage and migrations | 19/19 storage tests, atomic transaction/recovery paths | `VERIFIED` |
| Corruption recovery | Backup/rebuild and `ERR-STORAGE-102` end-to-end path | `VERIFIED` |
| Event sequence and replay | Restart/reconnect replay and persisted sequence IDs | `VERIFIED` |
| Watchers | Debounce, external edit, Git-head, and source-backed events | `VERIFIED` for tested fixtures |
| Client contract | Native process E2E 1/1; web/daemon suite 26/26 | `VERIFIED` |
| MCP and web integration | MCP 49/49; inspector 38/38 | `VERIFIED` |

## Measured Windows evidence

- Event wake-up p95: 0.214–0.453 ms in the cited serial Windows tests.
- Filesystem browse p95: 7.580–13.023 ms.
- 2,000-file daemon path p95: 23.132–23.346 ms.

These figures are measurements of the named workloads, not universal platform
guarantees. Run 80 additionally passed the corresponding native Windows,
Ubuntu, and macOS workspace and Electron jobs.

## Safety and UX boundary

Path selection updates canonical repository view-model state only. Scanning and
watching do not mutate source files or Git history. The 2D/3D canvas renderer,
animation, camera, layout, and styling were not changed by this evidence work.

## Remaining qualification

The report does not replace a full cross-platform latency distribution, nor does
it prove every filesystem edge case such as every network share, unusual mount,
or hostile symlink topology. Those cases must remain explicitly scoped in a
future evidence receipt.

## Publication conclusion

G1 is publishable as **core daemon behavior verified for the cited workflows**.
Use the raw evidence and run identity when making deployment decisions.
