# SynEvid (Code Autopsy) TypeScript MCP Server

**Out-of-Process Deterministic Verification Bridge for Frontier AI Coding Agents**

Official Model Context Protocol (MCP) Server implementing the **2026-07-28 MCP specification** via `@modelcontextprotocol/server` (SDK v2).

---

## 1. Architectural Role & Boundary Invariants

- **Zero Core Logic:** This server is strictly a transport and protocol adapter. It contains zero program analysis or heuristics; it invokes the compiled, offline, deterministic `autopsy` CLI binary.
- **Architectural Inversion (`ARCH_NO_CORE_TO_MCP`):** Core Rust crates (`crates/*`) have zero awareness of MCP. The MCP layer runs out-of-process and communicates with `autopsy` via stdio and canonical JSON serialization.
- **Strict Read-Only Guarantee (FR-026):** Every exposed tool is read-only with respect to source code and git history. The only writes are engine-store artifacts under the analyzed workspace's `.autopsy/` directory (baseline index state and session receipts) — never a source file, never git history.

---

## 2. Mandatory Read-Only Tool Suite

| Tool | Parameters | Description | Output Structure |
|---|---|---|---|
| `autopsy_status` | `repo_root?: string` | Inspects workspace indexing state, adapter capabilities, and BLAKE3 snapshot identity. Runs `baseline` (writes only to the engine store, never to source). | Index status, snapshot ID, discovered language files. |
| `autopsy_impact` | `symbol: string`, `repo_root?: string`, `direction?: "forward"\|"backward"\|"bidirectional"`, `max_depth?: number`, `budget?: number`, `profile?: "full"\|"callers"`, `edge_kinds?: string`, `top_k?: number` | Performs deterministic BFS blast radius traversal over the symbol dependency multigraph. `profile: "callers"` is the recommended precision-first preset (backward invocation closure + top-10 shortlist). | Reachable entities, ranked shortlist, edge counts, truncation flags, coverage state. |
| `autopsy_diff` | `repo_root?: string`, `base?/before?/base_snapshot?: string`, `head?/after?: string` | Computes semantic delta between a stored baseline snapshot and the worktree (or two stored snapshots). IDs are engine snapshot IDs, not git revisions. | Symbol deltas (added, removed, modified), signature breaks. |
| `autopsy_verify` | `repo_root?: string`, `base?/baseline_id?: string`, `invariants_file?: string`, `strict?: boolean` | Evaluates architectural and contract invariants against the target codebase, optionally two-state via `base`. | Findings (`pass`/`fail`/`unknown`), cryptographic receipt, exit code telemetry. |
| `autopsy_explain` | `target?/finding_id?: string`, `repo_root?: string` | Derives the full causal path and evidence receipt for a specific finding. | Exact source locations, multigraph edges, evidence bundle. |
| `autopsy_find` | `pattern: string`, `repo_root?: string`, `kind?: string`, `name_only?: boolean`, `limit?: number`, `offset?: number` | Resolves a partial, case-insensitive name against the analyzed graph (qualified names, declaring paths, stable IDs). The stable IDs it returns route straight into `autopsy_impact`. | `total`/`limit`/`offset` plus `symbols[]` (`stable_id`, `qualified_name`, `kind`, `visibility`, `path`, `signature`). |
| `autopsy_temporal` | `repo_root?: string`, `view?: "timeline"\|"co_change"\|"lineage"`, `at?`, `from?`, `to?`, `max_files?`, `max_commits?`, `window_seconds?`, `min_pair_commits?`, `compact?`, `top_k?`, `strict?` | Replays repository history: the revision journal of facts (`timeline`), mined co-change pairs (`co_change`), or file identity transitions between two revisions (`lineage`). Co-change is temporal co-occurrence, never causation; only a byte-identical rename reaches `verified` and every similarity match is `partial`. | Revision journal, co-change edge set, or lineage events — all with coverage and ambiguity stated. |
| `autopsy_test_selection` | `repo_root?: string`, `changed: string[]`, `lcov?: string[]`, `fail_if_all_tests_required?: boolean`, `strict?: boolean` | Conservative test selection for a changed file set, optionally narrowed by LCOV coverage reports. A selection that could not narrow is reported as `run-all-tests` rather than guessed at. | Planned test suites, skip rationale, coverage state. `fail_if_all_tests_required` turns a non-narrowing plan into exit 2 for CI gating. |
| `autopsy_contract_diff` | `base_dir?`, `from: string`, `to: string`, `protocol?: "openapi"\|"asyncapi"\|"pact"`, `compact?`, `top_k?`, `strict?` | Deterministic breaking-change diff of two API documents: OpenAPI revision pairs, AsyncAPI, or a Pact file against the provider's OpenAPI spec. | Verdict, breaking changes with locations, coverage, digest. |
| `autopsy_distributed_blast_radius` | `component: string`, `repo_root?`, `catalog_root?`, `traces?`, `surface?`, `salt_hex?`, `windows?`, `compact?`, `top_k?`, `strict?` | Organisational blast radius of one catalog component through the service graph, optionally fused with an OTLP/JSON trace export. Runtime evidence only narrows claims; unobserved dependencies degrade coverage and are never counted as edges. | Component ranks and edges, observed vs. declared surface, coverage state. |
| `autopsy_session` | `action: "start"\|"record"\|"close"\|"show"\|"list"`, `repo_root?`, `session_id?`, `invoked?`, `exit_code?`, `snapshot_id?`, `note?` | Repair-loop receipts: persists the agent's evidence loop (commands as invoked, exit codes, pinned snapshot IDs, verbatim notes) in the workspace's `.autopsy/sessions.json`. `show`/`list` re-read it. | Session record or summary; `close` mirrors the evidence exit code (0 pass, 2 fail, 3 unknown). |

One answer envelope for every tool: `{command, exit_code, actionable, report}` — `actionable`
is exactly `exit_code === 0`; refusals (exit 4) and policy gates (exit 2) travel with their
evidence rather than as transport errors.

Bounded, paginated resources (never full dumps):
- `autopsy://graph{?repo_root}` — index summary;
- `autopsy://graph/symbols{?limit,offset,kind,repo_root,snapshot}` — stable-id-ordered symbol pages;
- `autopsy://graph/edges{?source,direction,kind,limit,offset,repo_root,snapshot}` — 1-hop neighborhood pages;
- `autopsy://graph/map{?under,top,repo_root,snapshot}` — a bounded whole-graph projection (kind/package counts, top-N nodes by degree) for "where is everything and what do I not dare touch" without a dump;
- `autopsy://workspace{?repo_root}` — the workspace registry (declared repositories and paths);
- `autopsy://tools` — the machine-readable toolspec (fixed URI, no parameters).

CLI equivalents: `autopsy symbols --limit 50`, `autopsy edges --source <id>`, `autopsy map --top 25 --under src/checkout`.

---

## 3. In-Session Agent Lifecycle Protocol

Frontier coding agents (Codex, Claude, Cursor, Antigravity) integrate Code Autopsy into their generation loop:

$$\text{Baseline } A \longrightarrow \text{Pre-Edit Impact} \longrightarrow \text{Agent Edit } A \to B \longrightarrow \text{Verify } A \text{ vs } B \longrightarrow (\text{On FAIL: Repair via Evidence} \longrightarrow \text{Re-verify PASS})$$

1. **Before modifying sensitive files / interfaces:** Call `autopsy_impact` to discover the blast radius.
2. **After making code edits:** Call `autopsy_verify` to check for architectural drift or contract regressions.
3. **If findings fail:** Inspect the exact causal chain in the receipt and repair the code until `autopsy_verify` passes with a verified cryptographic receipt.
4. **Wrap the loop in a session:** call `autopsy_session` (`start` → `record` per step → `close`) to persist the loop's receipts — commands as invoked, exit codes, pinned snapshot IDs, notes — in `.autopsy/sessions.json`; `show`/`list` re-read them.

---

## 4. Agent Configuration

### Antigravity IDE (`mcp_config.json`):
```json
{
  "mcpServers": {
    "synevid": {
      "command": "node",
      "args": ["/absolute/path/to/SynEvid/apps/mcp-server/dist/index.js"],
      "env": {
        "AUTOPSY_BIN": "/absolute/path/to/SynEvid/target/debug/autopsy"
      }
    }
  }
}
```

### Claude Desktop (`claude_desktop_config.json`):
```json
{
  "mcpServers": {
    "synevid": {
      "command": "node",
      "args": ["/absolute/path/to/SynEvid/apps/mcp-server/dist/index.js"],
      "env": { "AUTOPSY_BIN": "/absolute/path/to/SynEvid/target/release/autopsy" }
    }
  }
}
```

### Binary resolution (`AUTOPSY_BIN`)
`AUTOPSY_BIN` is optional. Resolution order: `AUTOPSY_BIN` env →
`~/.local/bin/autopsy` (the one-line installer's prefix) → `~/.cargo/bin/autopsy` →
workspace `target/release/autopsy` → workspace `target/debug/autopsy` → `PATH`.
Tests pin the workspace debug build so they validate current code, not stale installs.

### Prove the bridge works (no agent needed)
Always use absolute repository paths: the CLI resolves `--repo-root`
against its own working directory, so a relative path from another
directory silently analyzes the wrong tree.
```bash
export AUTOPSY_BIN=/absolute/path/to/SynEvid/target/debug/autopsy
export REPO=/absolute/path/to/SynEvid/benchmarks/corpus/repo-api
node -e "
import('./dist/cli.js').then(async ({ runAutopsy }) => {
  const repo = process.env.REPO;
  const v = await runAutopsy('version', [], { repoRoot: repo });
  console.log('version ok:', v.exitCode === 0);
  const imp = await runAutopsy('impact', ['-s', 'UserService', '--profile', 'callers', '--top-k', '5'], { repoRoot: repo });
  console.log('impact ok:', imp.exitCode === 0, '| digest:', imp.data.result_digest.slice(0, 16));
});"
# Both lines must print ok:true. The digest must match a direct CLI rerun:
# $AUTOPSY_BIN --repo-root $REPO impact -s UserService --profile callers --top-k 5 --format json

---

## 5. Development & Testing

```bash
# Build TypeScript
npm run build

# Type check (no emit)
npm run typecheck

# Lint (ESLint)
npm run lint

# Unit + protocol-level agent-loop suites (node:test)
npm test
```
