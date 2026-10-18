# SynEvid Forensics — Agent Skill

Use this skill whenever you edit code in a repository with an `autopsy.toml`
(or any TypeScript/JavaScript/Python project). It makes your edits verifiable
instead of hopeful.

## 1. Before editing a public symbol or an unfamiliar file

Run blast-radius first (callers profile = who breaks if you change this):

```bash
autopsy --repo-root . impact -s "<file>::<Symbol>" --profile callers --top-k 10
autopsy --repo-root . impact -s "<file>" --profile callers --top-k 10   # file seed also works
```

Read `ranked_top` (reviewer shortlist) and `total_impacted` (full scope).
`truncated: true` means the frontier is partial — say so, don't extrapolate.

**You only have a partial name?** Resolve it first — `impact` demands an exact
identity, `find` turns fragments into one:

```bash
autopsy --repo-root . find "userservice" --name-only      # case-insensitive substring
autopsy --repo-root . impact -s "<stable_id from find>" --profile callers
```

`find` returns `total` (the population searched) with `limit`/`offset`
pagination; an empty list is an honest "no such symbol", not an error.

**You don't know the shape of the repo?** Read the map before the graph:

```bash
autopsy --repo-root . map --top 25            # kind/package counts + most-coupled nodes
autopsy --repo-root . map --under src/checkout --top 25
```

## 1b. Keep answers inside the context window (token budgets)

Every evidence command answers `--compact --top-k N` (default slice 20):

```bash
autopsy --repo-root . temporal --compact --top-k 10
autopsy --repo-root . cochange --compact --top-k 10
autopsy --repo-root . blast-radius component:default/checkout --compact
```

Compaction slices each list in the engine's own canonical order, names the
residual (`facts_omitted: 37`), and flags `"compact": true` — counts, coverage,
digests, and the exit code stay exact. A report you quote with `--compact` says
so; do not quote a compact slice as if it were the full answer.

## 2. After editing, verify before you claim done

```bash
autopsy --repo-root . baseline                          # snapshot the new state
autopsy --repo-root . verify --baseline-id <before>     # exit 0 pass, 2 fail, 4 unknown
```

On exit 2, read `findings[].rule_id`, `message`, and `entities`, repair the
code following the evidence `locations`, and re-verify. Do not declare the
task complete while a `fail` finding stands.

## 3. Coverage honesty (non-negotiable)

- `coverage.state: unknown` (eval, dynamic import; Go cgo/plugin/reflect; Rust `include!`)
  or `partial` (reflection, generated code; Go `unsafe`/`//go:generate`; Rust
  `unsafe`/`dyn`/`env!`) limits what the engine can prove.
- Report `unknown` findings as unknown. Never convert them into a pass,
  and never let them silently justify skipping verification.
- If a callee resolves only to `symbol::<name>` (Partial), say the binding
  is name-level, not binding-level.

## 4. Digest discipline (how anyone checks your work)

Every report carries `result_digest` (BLAKE3 over canonical JSON). When you
present engine evidence, always quote the digest and the exact rerun command.
A human re-running the command must get the byte-identical digest —
otherwise the evidence is yours, not the engine's.

## 5. MCP equivalents (when the synevid MCP server is connected)

- `autopsy_find` (`pattern` only is enough) — resolves partial names to stable IDs
- `autopsy_impact` (use `profile: "callers"`, `top_k: 10`, optional `snapshot_id` pin)
- `autopsy_verify` (use `baseline_id` for two-state checks)
- `autopsy_diff` (`base` = before snapshot, `head` = after or worktree)
- `autopsy_temporal` / `autopsy_contract_diff` / `autopsy_distributed_blast_radius`
  accept `compact: true` + `top_k` for bounded answers
- `autopsy_test_selection` — conservative test plans for a changed file set (`changed`
  is an array; pass `fail_if_all_tests_required: true` when the plan must narrow)
- `autopsy_session` (`start` / `record` / `close` / `show` / `list`) — persist the
  repair loop's receipts (commands, exit codes, pinned snapshots, notes) in
  `.autopsy/sessions.json`
- `autopsy://graph/map{?under,top}` for the bounded code map,
  `autopsy://graph/symbols{?limit,offset,kind}` for browsing without dumps,
  `autopsy://tools` for the machine-readable toolspec (every tool + resource at runtime)

Pin one baseline across a whole session with `AUTOPSY_SNAPSHOT=<snapshot-id>`
(export it in the host env; a tool-argument `snapshot_id` still wins for a
single call). Raise the per-call ceiling for large repos with
`AUTOPSY_TIMEOUT_MS=<ms>`.
