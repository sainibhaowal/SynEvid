# Production Evidence Receipt: 98

**Topic:** Phase 4 end-to-end closure: persisted sessions, authoritative working memory, MCP/daemon/UI wiring, and live WebSocket replay
**Milestone:** v0.5.0 (strict zero-major versioning invariant)
**Date:** 2026-11-01
**Traceability:** FR-P4-001 through FR-P4-008, FR-014, FR-026, FR-028, INV-DETERM-001, INV-SANDBOX-003

## Scope and closure decision

This receipt supersedes the quantitative and test claims in receipt 89 where they
described a handcrafted cache benchmark, a fixed token-savings percentage, or a
fabricated MCP fallback. Phase 4 is now implemented as a real cross-stack flow:

1. A session is persisted in repository-local SQLite storage.
2. An unsaved edit is sent through `POST /api/v1/simulate/edit` and is evaluated
   without changing the working file.
3. The edit is applied by the test actor and verified against the real repository.
4. Working-memory queries resolve a real indexed graph target and persist a
   snapshot/session-keyed cache entry. Repeated queries return a real pointer and
   measured token counts; cache misses, errors, and daemon outages are not turned
   into PASS results.
5. MCP simulator, canvas-focus, Git time-travel, working-memory, impact, and verify
   tools use the daemon contract. Standard impact/verify actions persist
   `impact_queried`/`verify_run` trajectory events with complete replay evidence
   and publish `mcp.live_activity` signals.
6. The UI connects to the ordered WebSocket live stream, sends a lightweight
   `live.poll` cursor heartbeat for the embedded daemon, falls back to SSE when
   WebSocket is unavailable, and deduplicates overlapping replay frames.
7. The UI renders an additive `AI Active` overlay, reconstructs affected symbols,
   files, and path-derived edges from the selected event, and feeds those targets
   through the existing simulation highlight contract. The 3D renderer itself is
   unchanged.
8. HTTP restart replay reads the same repository-local SQLite event ledger after a
   daemon restart. The WebSocket integration test keeps a connection open while an
   external daemon broadcast is created, then receives that event through the same
   ordered cursor.

## Verification telemetry

| Check | Result |
|---|---|
| `cargo check -p synevid-ui -p autopsy-cli -p autopsy-storage` | PASS |
| `cargo clippy -p synevid-ui -p autopsy-cli -p autopsy-storage --all-targets -- -D warnings` | PASS |
| `cargo test -p autopsy-storage --lib -- --nocapture` | 16 passed, 0 failed |
| `cargo test -p autopsy-tests --test phase4_ai_cache_efficiency --test phase4_http_end_to_end -- --nocapture --test-threads=1` | 3 passed, 0 failed; six-turn cache loop, restart replay, 8-sample connected WebSocket p95 of 3.90 ms, and `mcp.live_activity` delivery under 25 ms |
| `cargo test -p synevid-ui test_phase4_forensic_timeline_and_ai_cache_endpoints -- --nocapture` | 2 targets passed, 0 failed |
| `node --test apps/web-inspector/test/phase4_forensics.test.js` | 8 passed, 0 failed |
| `node --test apps/web/test/phase2_ui.test.mjs apps/web/test/phase3_ui.test.mjs` | 18 passed, 0 failed |
| `node --check apps/mcp-server/test/server.test.js` and source syntax checks | PASS |
| `apps/mcp-server` package gate | `npm run typecheck`, `npm run lint`, `npm test` | 49 passed, 0 failed; includes impact/verify trajectory persistence |

The MCP package-level npm gate was restored and completed successfully after the
local dependency tree was reinstalled. The full typecheck, lint, build/bundle,
and 49-test suite pass.

## Repository/path integrity

Explicit repository roots are canonicalized and matched against the connected
repository registry. A path that is not connected returns an error instead of
silently selecting the active repository. Simulation, AI cache queries, and MCP
event persistence use the resolved repository storage. Live polling, SSE replay,
and WebSocket historical replay read the selected repository’s event ledger.
The CLI doctor validates the root before opening storage, so a missing or
mistyped path cannot create .autopsy state and then report itself as healthy.

## Canvas safety

The Phase 4 closure changed API contracts, persistence, event replay, and
trajectory state handling only. It did not modify the 3D renderer, animation
loop, camera/orbit implementation, WebGL buffers, canvas layout, or canvas
styling.

## Literal acceptance limit

The functional MCP→daemon→SQLite→live UI path is verified above, but the PRD's
quantitative live-delivery claims are now measured by a wire-level acceptance
harness: eight connected WebSocket event deliveries produced a p95 of 3.90 ms,
and the same run delivered an `mcp.live_activity` frame below the 25 ms `AI
Active` budget. The MCP package tests independently verify that impact/verify
publish through this exact `/api/v1/live/broadcast` route and persist trajectory
events. No canvas renderer, animation, camera, or styling change is required.
