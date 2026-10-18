# Agent Tool Reference — Synevid (Code Autopsy)

This document is the human rendering of the machine-readable contract emitted by
`autopsy tools --format json`. When the two disagree, the JSON output wins — it is
the engine's own self-description, regenerated on every build.

**Tier-3.1 deliverable:** every subcommand, its flags, its exit-code contract, its
snapshot-pinning surface, and the envelope it travels in. An agent can learn the
entire surface from the engine instead of a hand-copied doc drifting out of date.

---

## 1. The one envelope

Every answer — CLI report and MCP alike — travels in this envelope:

| Field | Meaning |
|---|---|
| `command` | The subcommand that produced the answer |
| `exit_code` | `0` pass, `2` policy fail with evidence, `3` analysis could not run, `4` coverage gate |
| `actionable` | `true` only when `exit_code` is `0` |
| `report` | The engine report itself |
| `repair` | Present **only** on failures: `{what_failed, why, next_action}` parsed from the engine's own stderr markers. Never re-derived by the MCP layer. |

On a failure the engine prints three lines to stderr:

```
what_failed: <marker>
why: <explanation>
next_action: <concrete repair step>
```

## 2. Exit-code contract

| Code | Meaning |
|---|---|
| `0` | PASS — the answer is actionable |
| `2` | POLICY FAIL — evidence was produced and the pinned policy rejected it |
| `3` | ANALYSIS ERROR — the answer could not be produced; stderr carries repair advice |
| `4` | COVERAGE GATE — coverage was partial/unknown and `--strict` was set |

## 3. Global flags (every command)

| Flag | Meaning |
|---|---|
| `-r / --repo-root` | Target repository root directory |
| `-f / --format` | `text` \| `json` \| `sarif` (SARIF defined for finding reports only) |
| `--db-path / --cache-dir` | Storage and cache locations (defaults under `.autopsy/`) |
| `--wal` | SQLite write-ahead logging |
| `--persist` | Store the analysis in `.autopsy/storage.db` |
| `--suggestion <TEXT>` | Attach a correction receipt (Tier 3.3) |
| `--disable-suggestion` | Suppress repair hints and suggestion storage |

## 4. Subcommands

### 4.1 Analysis

| Command | Purpose | Pinning | Compact |
|---|---|---|---|
| `baseline` | Scan repo, compute deterministic snapshot, build dependency multigraph, store with `--persist` | `--incremental --baseline-id <id>` | no |
| `impact` | Deterministic bounded transitive impact analysis and blast radius for one symbol | `--snapshot-id <id>` | no |
| `diff` | Semantic AST symbol delta between a stored baseline and a state | `--before <id>` | no |
| `verify` | Evaluate pinned architectural invariants and contract models | `--baseline-id <id>` | no |
| `explain` | Explain one finding or symbol: provenance, evidence receipt, and neighborhood | none | no |

### 4.2 Browse

| Command | Purpose | Pinning | Compact |
|---|---|---|---|
| `symbols` | Paginated symbol index in stable-id order with optional kind filter | `--snapshot-id <id>` | no |
| `edges` | Paginated 1-hop neighborhood of one symbol | `--snapshot-id <id>` | no |
| `find` | Case-insensitive substring resolution over qualified names, paths, stable IDs | `--snapshot-id <id>` | no |
| `map` | Bounded whole-graph projection: kind/package/edge-kind counts and top-N nodes by degree | `--snapshot-id <id>` | no |

### 4.3 Lifecycle

| Command | Purpose | Pinning | Compact |
|---|---|---|---|
| `session` | Repair-loop receipts: record every evidence step, close with an honest verdict (Tier 2.5) | steps carry `--snapshot-id` | no |
| `workspace` | Multi-repo registry with honest per-repo accessibility and analysis status (Tier 2.3) | none | no |

### 4.4 Evidence (Pillar 1–6)

| Command | Purpose | Pinning | Compact |
|---|---|---|---|
| `temporal` | Replay history: the revision journal of facts added and removed | worktree/git-bound | yes |
| `lineage` | File identity across a revision pair: only byte-identical renames reach verified | worktree/git-bound | yes |
| `cochange` | Mined pairs of paths that historically changed together (co-occurrence, never causation) | worktree/git-bound | yes |
| `test-select` | Select the tests a changed file set requires, with the engine's own fallback and coverage state | worktree-bound | no |
| `contract-dist` | Diff two revisions of one distributed contract (OpenAPI/AsyncAPI/Pact) with a breaking verdict | none (document inputs) | yes |
| `telemetry` | Correlate an OTLP/JSON trace export against a declared surface | none (file inputs) | yes |
| `catalog` | Ingest Backstage catalog entities and dependency manifests (Pillar 6) | worktree-bound | yes |
| `blast-radius` | Organisational blast radius of one catalog component | worktree-bound | yes |

### 4.5 Bookkeeping

| Command | Purpose | Pinning | Compact |
|---|---|---|---|
| `doctor` | Verify repository configuration, storage integrity, and analyzer health | none | no |
| `tools` | Emit this specification (Tier 3.1) | none | no |
| `version` | Display the engine version and adapter capabilities | none | no |

## 5. Coverage semantics

- **verified** — all analyzed files carried full adapter coverage for the construct in question.
- **partial** — a stated construct (dynamic/unsupported) limited the graph; the report names it.
- **unknown** — the engine refuses to pretend. Never converted into PASS.

`--strict` exits `4` when coverage is partial as well as unknown.

## 6. Snapshot pinning (Tier 1.4, Tier 2.4)

A pinned run re-derives its report from one stored snapshot instead of re-parsing the
worktree. Pin via:

- `--snapshot-id <id>` on browse/impact/verify commands
- `--before <id>` on diff
- `--baseline-id <id>` on verify/baseline
- `AUTOPSY_SNAPSHOT=<id>` env var (session-wide pin; explicit flags win)

Blank pin values are treated as unpinned. Unknown IDs fail exit `3` with
`analysis_snapshot_missing` advice.

## 7. Repair advice (Tier 2.1)

Every error carries `{what_failed, why, next_action}`. The MCP layer parses these
from stderr markers — it never re-derives them.

## 8. Agent feedback channel (Tier 3.3)

Attach a correction receipt to any run:

```bash
autopsy <cmd> --suggestion "the fix that actually repaired this"
```

Receipts are stored deterministically in `.autopsy/suggestions.json` (BLAKE3 id over
normalized root + counter, no timestamps). On failure, a one-line hint is printed to
stderr; suppress everything with `--disable-suggestion`.

## 9. Token budgets (Tier 1.5)

Every evidence command supports `--compact` and `--top-k <N>`. Counts stay exact;
arrays are sliced with a named `_omitted` residual so the truncation is stated,
never silent.

## 10. MCP resources

| URI | Description |
|---|---|
| `autopsy://graph{?repo_root}` | Compact graph summary |
| `autopsy://graph/map{?under,top,repo_root,snapshot}` | Bounded whole-graph projection |
| `autopsy://graph/symbols{?limit,offset,kind,repo_root,snapshot}` | Paginated symbol index |
| `autopsy://graph/edges{?source,direction,kind,limit,offset,repo_root,snapshot}` | Paginated 1-hop neighborhood |
| `autopsy://workspace{?repo_root}` | Multi-repo registry |
| `autopsy://tools{?repo_root}` | This specification (Tier 3.1) |

## 11. Environment variables

| Variable | Purpose |
|---|---|
| `AUTOPSY_SNAPSHOT` | Session-wide snapshot pin |
| `AUTOPSY_TIMEOUT_MS` | MCP per-call timeout ceiling (default 30 s) |
| `AUTOPSY_MAX_BUFFER_BYTES` | MCP maxBuffer ceiling (default 20 MB) |
| `AUTOPSY_BIN` | Path to the `autopsy` binary (MCP tests) |

## 12. Machine-readable spec

Run `autopsy tools --format json` for the full contract. The MCP server exposes it
as the `autopsy://tools` resource.

## 13. IDE surface — `synevid-lsp` (Tier 3.2)

`synevid-lsp` is a Language Server Protocol server that answers hover requests with
the engine's impact analysis. It is a presentation layer over the same library API
the CLI uses (`build_snapshot_and_graph`), so an editor hover and a terminal run can
never disagree about the graph.

```bash
synevid-lsp [--repo-root <PATH>]
```

- **Transport:** JSON-RPC 2.0 over stdin/stdout (`Content-Length` framing).
- **Capabilities:** `hover` and full-sync text documents.
- **Root resolution:** `--repo-root`, else the client's `rootUri`, else the first
  workspace folder, else the process working directory.
- **Hover content:** signature, kind, scope-qualified name, declaration site, direct
  callers/callees, 1-hop impact size, and the coverage state of the edges counted.
  Ambiguous names list up to five candidates in stable-id order; no match returns
  `null` — the server never invents a symbol.
- **Freshness:** analysis is built lazily on the first hover and cached; a save
  (`textDocument/didSave`) invalidates it. Hover reads the live buffer for the word
  under the cursor, so unsaved edits can move the cursor onto a word the last saved
  graph does not contain — the honest answer there is `null`.
- **Exit codes:** `0` after `shutdown`+`exit`, `1` on `exit` without `shutdown`
  (LSP spec), `2` on a bad command-line argument.

Example editor configuration (Neovim, built-in LSP):

```lua
vim.lsp.start({
  name = 'synevid',
  cmd = { 'synevid-lsp', '--repo-root', vim.fn.getcwd() },
  root_dir = vim.fn.getcwd(),
})
```

