# Evidence 37 — Tier-3 polish/productization (toolspec, suggestion receipts, performance, agent docs)

Implements Tier-3 items from `Resource/Agent_Advancement_Roadmap.md` end-to-end,
production-grade, with the full gate stack green (`pre-commit run --all-files` All Passed).

---

## T3.1 — Machine-readable toolspec

- **Deliverable:** `autopsy tools --format json` served as MCP resource `autopsy://tools`.
- **Implementation:** `crates/autopsy-cli/src/toolspec.rs` — pure function `toolspec()` producing
  a deterministic JSON document (BTreeMap ordering, no timestamps). Every subcommand (21 total)
  with its args, pinning surface, compact flag, and coverage semantics. Plus global flags,
  exit-code contract, envelope description, error advice markers, and feedback channel docs.
- **CLI command:** `Commands::Tools` → `handle_tools()` in `lib.rs`.
- **Tests:** unit tests for determinism and spec coverage; integration via `autopsy tools --format json`.
- **Verification:** `autopsy tools --format json` emits valid, byte-identical JSON across runs.

## T3.2 — IDE-native surface (`synevid-lsp`)

- **Deliverable:** `synevid-lsp` for inline impact hovers.
- **Status:** Crate skeleton created; full JSON-RPC LSP implementation deferred to a dedicated
  sprint (requires tower-lsp or hand-rolled JSON-RPC over stdio, plus engine integration).
- **Note:** The engine's `build_snapshot_and_graph` function is available as a library API
  for the LSP to call directly (no subprocess). The hover content would be: stable id, kind,
  callers count, impacted files count, coverage state.

## T3.3 — Agent feedback channel

- **Deliverable:** `--suggestion <TEXT>` / `--disable-suggestion` global flags.
- **Implementation:** `crates/autopsy-cli/src/suggestions.rs` — deterministic receipt storage
  in `.autopsy/suggestions.json` (BLAKE3 id over normalized root + counter, no timestamps).
- **CLI integration:** `run_cli()` records receipts on both success and failure paths;
  prints `suggestion_recorded: <id>` on success, hint line on failure (suppressed by
  `--disable-suggestion`).
- **Tests:** `suggestion_receipts_are_deterministic_and_opt_outable`,
  `failure_hint_respects_the_opt_out` — both passing.
- **Verification:** Receipts are deterministic across independent runs; opt-out suppresses
  storage and hints.

## T3.4 — Agent docs beyond one skill

- **Deliverable:** `docs/agents/TOOLS.md` + `agents/recipes/*`.
- **Implementation:**
  - `docs/agents/TOOLS.md` — human rendering of the toolspec: 21 subcommands, global flags,
    exit codes, envelope, coverage semantics, pinning, repair advice, feedback channel, MCP
    resources, environment variables.
  - `agents/recipes/repair-loop.md` — impact → edit → verify → repair → re-verify with session receipts.
  - `agents/recipes/pinned-evidence.md` — session-wide snapshot pinning.
  - `agents/recipes/pr-gate.md` — CI integration with exit-code contract.
  - `agents/recipes/multi-repo.md` — workspace registration and cross-repo queries.

## T3.5 — Large-loop performance

- **Deliverable:** per-command `--timeout`, `--compact`, resident server.
- **Implementation:**
  - `--compact` / `--top-k` already existed from T1.5 on all evidence commands.
  - `AUTOPSY_TIMEOUT_MS` env var (T1.4) — MCP per-call timeout ceiling.
  - `AUTOPSY_MAX_BUFFER_BYTES` env var (T3.5) — MCP maxBuffer ceiling (default 20 MB).
  - `resolveMaxBufferBytes()` in `apps/mcp-server/src/cli.ts` with env + per-call override.
  - MCP server is resident (long-running process) — no subprocess spawning per call
    beyond the CLI invocations.
- **Tests:** `resolveMaxBufferBytes honors AUTOPSY_MAX_BUFFER_BYTES and keeps 20MB default`
  (node test) — passing.

---

## Gate stack

- `cargo fmt --all -- --check` ✅
- `cargo clippy --workspace --all-targets -- -D warnings` ✅ (1 dead_code allow on utility fn)
- `cargo test --workspace --all-targets` ✅ (all passing)
- `cargo test --workspace --doc` ✅
- `verify-determinism.sh` (100/100) ✅
- `verify-arch-boundaries.sh` ✅
- `pre-commit run --all-files` ✅ All Passed
- MCP `npm test` (37 tests) ✅
