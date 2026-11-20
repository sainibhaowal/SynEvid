# Phase 4 — Authenticated Runtime Evaluation

Status: `IMPLEMENTED — VERIFICATION PENDING`

This report evaluates the stricter Phase 4 runtime protocol: one authenticated
daemon authority for CLI, TUI, MCP, and UI state. It supersedes older Phase 4.2
claims that covered trajectory persistence or live transport without proving
client authority and lease parity.

## Claim under test

For one daemon instance, every client identifies itself, authenticates, attaches
to the canonical active repository, receives a lease and cursor, consumes
ordered replay, and sends actions through the daemon. Offline CLI analysis is
explicit and is not presented as daemon-connected state.

## Implementation map

| Requirement | Implementation | Evidence required |
|---|---|---|
| Stable daemon authority | `DashboardServer::daemon_id` derived from canonical daemon root | Runtime status and restart parity test |
| Client identity and lease | `/api/v1/runtime/connect`, `/heartbeat`, `/disconnect`; `X-Synevid-*` headers | Lease lifecycle test |
| Shared state | `/api/v1/status` runtime envelope; persisted active session and repository | CLI → daemon → UI parity test |
| Replay | Existing persisted live-event store plus `gap`, `gap_from`, `gap_to`, `replay_complete` | Restart and missing-cursor negative test |
| Authentication | Existing loopback bearer guard and `ERR-AUTH-401` | Wrong/missing/valid token process test |
| MCP parity | MCP impact/verify default to daemon; `offline=true` is explicit | MCP daemon-path test |
| UI parity | Web requests identify `ui:web` and acquire a UI lease | Browser/Electron runtime test |
| TUI parity | TUI refuses silent fallback and requires `--offline` for independent mode | CLI/TUI behavior test |

## Truth boundary

This report does not claim G4 `VERIFIED` until the native Rust test suite and
the Windows/Linux/macOS CI jobs produce artifacts for the rows above. The
canvas renderer, animation, camera, WebGL buffers, and styling are outside the
change surface and are not modified by this protocol.

## Required evidence record

Record the exact commit, runner, test command, pass/fail count, daemon ID,
repository ID, snapshot ID, first and last event sequence, replay gap state,
authentication negative results, and any unavailable platform. Do not replace
unavailable evidence with a green placeholder.
