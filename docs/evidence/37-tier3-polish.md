# Evidence 37 — Tier-3 polish/productization (toolspec, suggestion receipts, performance, agent docs)

Implements Tier-3 items from `Resource/Agent_Advancement_Roadmap.md` end-to-end,
production-grade, with the full gate stack green (`pre-commit run --all-files` All Passed).

---

## T3.1 — Machine-readable toolspec

- **Deliverable:** `autopsy tools --format json` served as MCP resource `autopsy://tools`.
- **Implementation:** `crates/autopsy-cli/src/toolspec.rs` — pure function `toolspec()` producing
  a deterministic JSON document (BTreeMap ordering, no timestamps). Every subcommand (22 total)
  with its args, pinning surface, compact flag, and coverage semantics. Plus global flags,
  exit-code contract, envelope description, error advice markers, and feedback channel docs.
- **CLI command:** `Commands::Tools` → `handle_tools()` in `lib.rs`.
- **MCP surface:** `autopsy://tools{?repo_root}` registered in `apps/mcp-server/src/server.ts`;
  the resource returns the same document, byte-identical across reads (node test).
- **Tests:** unit tests for determinism and spec coverage; integration via `autopsy tools --format json`.
- **Verification:** `autopsy tools --format json` emits valid, byte-identical JSON across runs.

## T3.2 — IDE-native surface (`synevid-lsp`)

- **Deliverable:** `apps/synevid-lsp` — a working Language Server Protocol server for
  inline impact hovers (Tier 3.2 is implemented, not a skeleton).
- **Implementation:**
  - `src/protocol.rs` — `Content-Length` JSON-RPC framing; tolerant header reader
    (`\r\n` or `\n`, extra headers) and byte-canonical writer; request/notification/
    error-response shapes; `-32601` for unsupported requests, unknown notifications ignored.
  - `src/hover.rs` — LSP UTF-16 column → char conversion; identifier extraction for
    `.`- and `::`-joined paths (`svc.UserService.create`, `crate::billing::Ledger::settle`);
    deterministic resolution (declaration-at-cursor → scope-qualified candidates most-specific
    first → same-file preference); 1-hop neighborhood counts; markdown rendering.
  - `src/lib.rs` — lazy `EngineBridge` over `autopsy_cli::build_snapshot_and_graph` (library
    call, no subprocess); full-sync overlay tracking (`didOpen`/`didChange`/`didClose`); cache
    invalidation on `didSave`/watch events; root resolution (`--repo-root` > `rootUri` >
    workspace folder > cwd); `file://` URI percent-decoding.
  - `src/main.rs` — stdio binary with `--help`/`--version` and spec-correct exit codes
    (`0` after shutdown, `1` on exit without shutdown, `2` on bad arguments).
- **Semantics (honesty rules):**
  - Callers/callees follow the engine's `callers` profile (`Calls` + `References`);
    structural `Contains` edges never inflate the number (unit-tested).
  - 1-hop impact follows `ImpactProfile::default()`, so the hover number and the CLI
    `autopsy impact` number describe the same graph.
  - Ambiguous names list up to five candidates in stable-id order and state the real
    match count; no match returns `null`; analysis failure returns an explicit
    "analysis unavailable" hover.
  - Hover output contains no timestamps or absolute workspace paths (repo-relative only).
- **Tests:** 24 crate tests — framing round-trip/malformed frames; cursor and UTF-16
  edge cases (astral chars); `.`/`::` path extraction; resolution and ambiguity rendering;
  neighborhood dedup and containment exclusion; sessions over in-memory buffers against a
  real two-file fixture (cross-file hover, declaration hover, live overlay, unknown method,
  exit codes, missing root).
- **Verification:** the real binary over stdio returns
  `Declared src/ledger.ts:2` + `Callers 1 · Callees 1 · 1-hop impact 1 symbols / 1 files`
  + `Engine coverage verified`; three independent sessions produce byte-identical stdout;
  stdout carries protocol frames only (stderr empty).
- **Docs:** `docs/agents/TOOLS.md` §13 — capabilities, freshness model, exit codes,
  Neovim `vim.lsp.start` example.

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
  - `docs/agents/TOOLS.md` — human rendering of the toolspec: 22 subcommands, global flags,
    exit codes, envelope, coverage semantics, pinning, repair advice, feedback channel, MCP
    resources, environment variables, and the `synevid-lsp` editor surface.
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
- MCP `npm test` (37 tests) ✅ (includes the `autopsy://tools` resource test)
- `synevid-lsp` crate tests (24) ✅ (part of the workspace run; includes real-fixture LSP sessions)
