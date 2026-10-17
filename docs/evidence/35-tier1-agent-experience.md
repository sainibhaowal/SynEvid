# Evidence 35 — Tier-1 agent-experience surface (find, map, envelope, pinning, token budgets)

Implements the five Tier-1 gaps from `Resource/Agent_Advancement_Roadmap.md` end-to-end,
production-grade, with the full gate stack green. Changes are deterministic, read-only, and
honor every hard boundary (zero LLM, no timestamps in outputs, `BTreeMap/BTreeSet` only).

## T1.1 — Symbol resolution: `autopsy find` + `autopsy_find`

- CLI: `autopsy find <pattern> [--kind K] [--name-only] [--limit N] [--offset N]`.
  Case-insensitive substring over qualified name, declaring path, and (unless
  `--name-only`) stable id. Stable-id-then-qualified-name deterministic order; honest
  `total` beside `limit`/`offset`; page ceiling 1000 (same as `symbols`); empty pattern
  refused with exit 3 *before* any repository work.
- Pure core `find_symbols()` (`crates/autopsy-cli/src/lib.rs`) exposed for tests:
  matching contract, scoping rule, pagination, determinism all asserted in
  `crates/autopsy-cli/tests/step46_find_and_compact.rs`.
- MCP: new `autopsy_find` tool (10 tools total), envelope-wrapped, so an agent routes
  `find` → stable id → `impact -s` without human help.

## T1.2 — One envelope for all answers

- Tools 1–5 (`status/impact/diff/verify/explain`) now return
  `{command, exit_code, actionable, report}` like the Phase 4.2 tools; bare `{error}`
  JSON is eliminated (explain's missing-target and blast-radius's missing-salt refusals
  use `refusedEnvelope`, exit-code 3 semantics preserved).

## T1.3 — Bounded code map: `autopsy map` + `autopsy://graph/map`

- CLI `map [--top N] [--under PREFIX]`: kind counts, package counts (first two path
  segments), edge-kind counts, and top-N nodes by degree (in+out), degree-descending
  with stable-id tie-break, top clamped 1..=500, scope stated in the report.
  `--under` narrows *everything it counts*, including edge endpoints — a scoped map is
  ranked inside its scope, not by edges leaving it.
- Pure core `map_projection()` tested in the same gate file; MCP resource
  `autopsy://graph/map{?under,top}` serves it.

## T1.4 — Snapshot pinning across calls

- `runAutopsy` gained `snapshotId` pinning: per-subcommand flag mapping
  (`impact --snapshot-id`, `diff --before`, `verify --baseline-id`,
  `baseline --incremental --baseline-id`); pin flags are injected *before* caller args so
  an explicit argument always wins (clap last-occurrence rule).
- Session default via `AUTOPSY_SNAPSHOT` env; `AUTOPSY_TIMEOUT_MS` overrides the 30 s
  ceiling for large repositories (garbage/zero values ignored, not trusted).
- Test proves pinned `impact` equals the worktree analysis byte-for-byte at the same
  revision, and an unresolvable pin refuses with exit 3 instead of silently re-parsing.

## T1.5 — Token budgets: `--compact` / `--top-k`

- Every evidence command (`temporal`, `lineage`, `contract-dist`, `telemetry`, `cochange`,
  `catalog`, `blast-radius`) gains `--compact` and `--top-k`. Compaction slices each
  top-level array above top-k (default 20, clamped 1..=1000) **in the engine's own
  canonical order**, adds `<key>_omitted` residuals and `"compact": true`, keeps every
  scalar (counts, coverage, digests) exact, never touches the text summary, never moves
  an exit code, and is applied *after* persistence so storage keeps the full report.
- MCP exposure on temporal (all views), contract diff, and blast radius.

## Test & gate telemetry

- Rust: `crates/autopsy-cli/tests/step46_find_and_compact.rs` — 22 tests
  (find contract, map contract, compact contract, determinism, live-surface e2e).
  Full workspace `cargo fmt --check`, `clippy -D warnings`, `cargo test --workspace`
  all green.
- MCP: `apps/mcp-server/test/server.test.js` — 27 tests, 0 failures (tsc clean), including
  the tool registry now pinning exactly 10 tools and the map template.
- Invariants respected: compaction is presentation only; `UNKNOWN` never becomes `PASS`
  through it; no unordered collection touches serialized output (kind/package counts are
  `BTreeMap`; the map's array order is engine-canonical).
