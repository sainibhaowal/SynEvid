# Production Evidence Receipt 103: Phase 3 Git Integrity and Honest Galaxy

**Date:** 2026-11-06  
**Scope:** Phase 3 native Git reader, Galaxy evidence classification, corruption
handling, historical snapshots, mutation boundary, and Windows client parity.

## Changes verified

- Pure-Rust Git object reads now verify the requested SHA-1 object identity after
  loose reads, packed reads, delta resolution, and cache hits.
- Packed object declared sizes are checked for concrete objects and delta
  instruction streams. Existing delta-base and maximum-chain guards remain
  active.
- Corrupt or missing commit objects and malformed loose/packed refs now fail the
  Galaxy read. They are not silently skipped and cannot become a successful
  empty history.
- Galaxy commit risk is explicitly marked `file_heuristic`; its public `verdict`
  is `UNKNOWN` until semantic language-adapter evidence is attached. The
  file-count heuristic is retained separately as `heuristic_verdict`.
- Analytics, Galaxy, terminal branch/tag history, bisect, and forensic undo
  error paths expose non-success states instead of fabricated or empty success.
- Historical reads remain in-memory/read-only. The existing 2D/3D canvas
  renderer, animation, camera, layout, WebGL buffers, and styling were not
  changed.

## Focused evidence

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo test -p autopsy-temporal --lib` | 29/29 PASS |
| `cargo test -p autopsy-git-engine --lib` | 8/8 PASS, including corrupt-object rejection |
| `phase3_native_git_end_to_end` | 1/1 PASS |
| `phase3_real_git_repository` | 1/1 PASS; packed objects, delta chain, refs/tags, merge parents, dirty worktree, mtime/bytes preservation, semantic diff/blame, guarded mutations and undo |
| `synevid-ui test_phase3_git_galaxy_and_time_travel_endpoints` | PASS in both library and binary targets |
| `apps/web npm test` | 20/20 PASS |
| `apps/web npm exec tsc -- --noEmit` | PASS |
| `apps/web-inspector/test/phase3_git_galaxy.test.js` | 10/10 PASS |
| `apps/mcp-server npm test` | 49/49 PASS |
| Release packed 5,000-commit Galaxy | 140.61 ms, PASS against `<150 ms` |
| Release loose 5,000-commit Galaxy | 445.74 ms, FAIL against `<150 ms`; reproducible Windows loose-object cold-start limitation |
| First historical frame in real packed fixture | 8.48 ms, PASS against `<100 ms` |

## Gate decision

Functional Phase 3 behavior is verified on Windows for the defined packed real
repository flow and all tested client paths. The literal Phase 3 performance
gate remains **PARTIAL**, not `VERIFIED`, because the PRD does not exempt loose
object repositories and their measured cold-start path is above `<150 ms`.
This is an honest storage-format performance limitation, not a correctness
failure. CI must continue to run both packed and loose measurements.

