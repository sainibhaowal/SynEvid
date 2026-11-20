# Evidence 106 — Phase 4 G4 Native CI Closure

Status: `VERIFIED`

Date: 2026-11-20

## Claim

For the Phase 4 protocol in `Resource/6.1 Fixing/05_PHASE_4...`, one
authenticated daemon is authoritative for CLI, TUI, MCP, and UI repository,
session, snapshot, lease, and event-cursor state. Offline CLI operation is
explicit and is not reported as daemon-connected state.

## Source of truth

- Commit: `6f78ba4`
- Workflow: [CI Quality & Cross-Platform Matrix](https://github.com/sainibhaowal/SynEvid/actions/runs/44769225870)
- Final gate: `Pre-Merge Verification Gate`, job `136409185910`, `success`
- Platforms: `windows-latest`, `ubuntu-latest`, `macos-latest` (Apple Silicon)

## Verified controls

| Control | Evidence | Result |
|---|---|---|
| Authenticated client identity and leases | `apps/synevid-ui/tests/phase4_runtime_native.rs` | PASS |
| Missing/wrong token refusal | Native runtime test and daemon auth contract | PASS |
| Active repository and daemon identity parity | Native runtime test plus Electron browser flow | PASS |
| Persisted event sequence and restart replay | Native runtime test and browser restart workflow | PASS |
| MCP action source and evidence metadata | MCP daemon route and event replay assertions | PASS |
| Explicit offline boundary | CLI/TUI/MCP implementation and workspace tests | PASS |
| Shared runtime envelope | `schemas/runtime-envelope.schema.json` and schema job | PASS |
| Native Electron repository/replay workflow | Windows, Ubuntu, macOS smoke jobs | PASS |
| Rust/CLI/TUI/server regression coverage | Three native workspace jobs | PASS |

## Cross-platform performance evidence

The strict 5,000-loose-commit Galaxy acceptance test passed on all native
runners:

| Runner | Measured read |
|---|---:|
| Windows | 118.03 ms |
| macOS | 70.16 ms |
| Ubuntu | 29.80 ms |

These are measurements of the checked-in synthetic acceptance fixture on the
listed hosted runners. They are not a promise of a fixed latency for every
repository, filesystem, or workload.

## Boundary and limitations

The result is `VERIFIED` for the stated G4 protocol and CI matrix. SynEvid
still reports `UNKNOWN` or `PARTIAL` for unsupported dynamic behavior, bounded
or truncated analysis, unavailable services, and incomplete parser coverage.
This evidence does not convert those states into `PASS`. Desktop artifacts
remain unsigned where release credentials are unavailable; signing and
notarization are separate distribution requirements.

No 2D/3D canvas renderer, animation loop, camera/orbit behavior, WebGL buffer,
or canvas styling file was changed by the Phase 4 closure work.
