# Production Evidence Receipt: 99

**Topic:** Portable Python runtime resolution for the benchmark harness
**Milestone:** v0.5.0 (strict zero-major versioning invariant)
**Date:** 2026-11-01

## Finding

The Windows `python3` command resolved to a WindowsApps alias configured for a
deleted user-specific runtime:

`C:\Users\Ravin\Projects\LYNWIRE\.python311\python.exe`

That stale path caused the Phase 6 benchmark integration test to fail before
the Python harness started. No benchmark ground truth or expected results were
changed.

## Repair

- Installed Python 3.12.10 as a user-scoped runtime through `winget`.
- Added deterministic launcher discovery to
  `tests/tests/e2e_phase6_benchmarks.rs`: explicit `PYTHON` first, then
  `python3`, `python`, and Windows `py -3`; each candidate must successfully
  execute `--version`.
- Removed the test's dependence on any fixed home directory, repository, or
  virtualenv path.

## Verification

| Check | Result |
|---|---|
| `python --version` | Python 3.12.10 |
| `python3 --version` | Python 3.12.10 |
| `cargo fmt --all -- --check` | PASS |
| Phase 6 benchmark harness gate | PASS; 1 test passed |
| Pre-registered recall-lift gate | PASS; +31.73 percentage points |

This repair does not change the Electron renderer, the 3D canvas, animation,
camera, WebGL buffers, or UI styling.
