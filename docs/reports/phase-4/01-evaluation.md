# Phase 4 — Authenticated Runtime Evaluation

Status: `VERIFIED`

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

## Verified result

G4 is verified for the stated protocol and native CI scope by
[run 44769225870](https://github.com/sainibhaowal/SynEvid/actions/runs/44769225870)
and its successful `Pre-Merge Verification Gate` (`136409185910`). Windows,
Ubuntu, and macOS all passed the native workspace suite, Electron browser and
packaged smoke workflow, schema checks, and the authenticated runtime tests.

The strict 5,000-loose-commit acceptance measurements were 118.03 ms on
Windows, 70.16 ms on macOS, and 29.80 ms on Ubuntu in that run.

The claim is bounded: unsupported dynamic behavior remains `UNKNOWN` or
`PARTIAL`, and unsigned artifacts still require the normal OS trust warning.
The canvas renderer, animation, camera, WebGL buffers, and styling are outside
the change surface and were not modified by this protocol.

## Required evidence record

The exact commit, runner matrix, native test scope, authentication negatives,
replay behavior, and performance measurements are recorded in
`docs/evidence/106-phase4-g4-ci-closure.md`. Runtime receipts retain daemon,
repository, snapshot, lease, cursor, and digest identity for each operation.
Unavailable evidence is never replaced with a green placeholder.
