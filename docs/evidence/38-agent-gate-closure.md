# Evidence 38 — Agent acceptance-gate closure (protocol-level MCP e2e)

Closes the roadmap §6 acceptance gate with real artifacts: the agent loop is now
exercised end-to-end **through the MCP server** over its stdio wire protocol, not
only through the CLI or in-process handler calls. One honest-documentation item
from the gate (the pre-registered falsification) is recorded below rather than
claimed.

An audit at commit `a2dc9fc` found the gate's named artifacts missing
(`e2e_agent_find_and_envelope.rs`, `e2e_agent_session.rs` did not exist) and no
test anywhere that spoke JSON-RPC to a live MCP server process. The MCP server is
a TypeScript process, so the literal way to satisfy "through the MCP server" is a
protocol-level test in the server's own suite; the gate's `.rs` names were a
proposal, the substance is the wire-protocol exercise.

---

## Implementation

### Protocol-level MCP client (new)

- `apps/mcp-server/test/mcp_stdio.js` — minimal MCP stdio client for the tests:
  spawns the real `dist/index.js` server (the same process an agent host starts),
  speaks newline-delimited JSON-RPC 2.0, completes the `initialize` /
  `notifications/initialized` handshake, and decodes tool answers. Pins
  `AUTOPSY_BIN` to the workspace build so a stale `~/.cargo/bin` install can never
  satisfy the assertions. `freshWorkspaceCopy()` gives session tests a throwaway
  fixture copy so receipts never touch committed ground truth.

### Agent-loop e2e (new) — `apps/mcp-server/test/e2e_agent_find_and_envelope.test.js`

1. **3-call loop pin:** tool discovery (`tools/list` = 11 tools) → `autopsy_find`
   resolves a partial name to its stable id → `autopsy_impact` returns its bounded
   blast radius → `autopsy_verify` passes with `coverage.state = "verified"`.
   The test asserts the loop is *exactly* 3 MCP tool calls — the canonical
   interpretation of the §6 metric "tool calls to reach a passing verify".
2. **Refusal honesty:** a schema-level violation (empty pattern) is flagged
   `isError` with prose naming the tool and argument, never raw error JSON; an
   engine-level refusal (ghost snapshot pin) stays inside the envelope at exit 3
   with the `{what_failed, why, next_action}` repair triple, on both `find` and
   `impact`.
3. **Toolspec resource:** `resources/read autopsy://tools` returns the
   machine-readable toolspec (format 0.0.1, 22 tools, exit-code contract,
   feedback channel) at its documented URI.

### Session receipt e2e (new) — `apps/mcp-server/test/e2e_agent_session.test.js`

4. **Fail verdict:** start → record `verify --strict` at exit 2 → close yields
   `verdict = "fail"` with the envelope mirroring exit 2; `show` is byte-identical
   across reads and carries no wall-clock timestamps; recording on a closed
   receipt is refused at exit 3 with repair advice.
5. **Pass verdict + reproducibility:** start → record `verify` at exit 0 → close
   yields `verdict = "pass"` with the envelope mirroring exit 0. The store is then
   wiped and the identical loop replays: session id and stored bytes reproduce
   byte-for-byte (BLAKE3 over normalized root + counter; deterministic counter).

### Fixed while writing the gate tests

- **`autopsy://tools` was unreachable at its documented URI.**
  It was registered as a URI template (`autopsy://tools{?repo_root}`) even though
  the toolspec is repository-independent; the SDK's template matching refused the
  bare URI (`Resource not found: autopsy://tools`), and the parameterized form
  existed only for symmetry. Changed to a fixed-URI resource in
  `apps/mcp-server/src/server.ts`; `resources/read autopsy://tools` now works
  exactly as `docs/agents/TOOLS.md` documents it. `server.test.js` updated to
  assert the fixed registration and read path.
- Stale node test title ("registers all 10 read-only tools" → 11).

### Wiring

- `apps/mcp-server/package.json` `test` script now runs all three suites:
  `node --test test/server.test.js test/e2e_agent_find_and_envelope.test.js
  test/e2e_agent_session.test.js`.

## Verification

- `npm test` — **42/42 passing** (37 prior + 5 protocol-level), lint clean.
- The three fail-verdict exit codes (0 pass / 2 fail / 3 unknown-or-refused) are
  asserted from the wire responses, not from handler internals.
- Every new assertion reads values produced by the real `autopsy` binary through
  the real server process; nothing is mocked.

## Pre-registered falsification — honest status

The roadmap's §6 falsification reads: "a tool is not shipped until it demonstrably
reduces agent exploration (fewer tool calls to reach a passing verify) on the
Phase-6 corpus."

- The committed Phase-6 measurement (`benchmarks/results/reproduction_report.md`)
  reports mean tool calls **C (Autopsy) = 3.00 vs A (grep) = 2.50 and
  B (LSP) = 1.50**. The literal "fewer tool calls" reading is therefore **not**
  demonstrated; promotion passed on the pre-registered OR gate via the
  **+28.33 pp recall lift** (100% vs 71.67%).
- Tool calls are not the whole cost: Autopsy's calls are atomic verification
  calls, and the same report shows the loop reaches perfect recall where the
  baselines miss ~28 pp. But no feature-level A/B counterfactual exists for the
  Tier 1–3 agent features themselves (find/pinning/compact were not benchmarked
  against their absence), so this evidence records the gap as **not measured**
  rather than claiming a reduction.

## Gate stack

- `cargo fmt --all -- --check` ✅
- `cargo clippy --workspace --all-targets -- -D warnings` ✅
- `cargo test --workspace --all-targets` ✅
- `cargo test --workspace --doc` ✅
- MCP `npm test` (42 tests, lint clean) ✅
- `verify-determinism.sh` (100/100) ✅
- `verify-arch-boundaries.sh` ✅
- `pre-commit run --all-files` ✅ All Passed
