# SynEvid (Code Autopsy) TypeScript MCP Server

**Out-of-Process Deterministic Verification Bridge for Frontier AI Coding Agents**

Official Model Context Protocol (MCP) Server implementing the **2026-07-28 MCP specification** via `@modelcontextprotocol/server` (SDK v2).

---

## 1. Architectural Role & Boundary Invariants

- **Zero Core Logic:** This server is strictly a transport and protocol adapter. It contains zero program analysis or heuristics; it invokes the compiled, offline, deterministic `autopsy` CLI binary.
- **Architectural Inversion (`ARCH_NO_CORE_TO_MCP`):** Core Rust crates (`crates/*`) have zero awareness of MCP. The MCP layer runs out-of-process and communicates with `autopsy` via stdio and canonical JSON serialization.
- **Strict Read-Only Guarantee:** All exposed tools are read-only (`--read-only`, no git history mutations, no filesystem writes).

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

One answer envelope for every tool: `{command, exit_code, actionable, report}` — `actionable`
is exactly `exit_code === 0`; refusals (exit 4) and policy gates (exit 2) travel with their
evidence rather than as transport errors.

Paginated graph resources (never full dumps): `autopsy://graph` (index
summary), `autopsy://graph/symbols{?limit,offset,kind}` (stable-id-ordered
symbol pages), `autopsy://graph/edges{?source,direction,kind,limit,offset}`
(1-hop neighborhood pages), `autopsy://graph/map{?under,top}` — a bounded
whole-graph projection (kind/package counts, top-N nodes by degree) for
"where is everything and what do I not dare touch" without a dump. CLI
equivalents: `autopsy symbols --limit 50`, `autopsy edges --source <id>`,
`autopsy map --top 25 --under src/checkout`.
and `autopsy edges --source <id>`.

---

## 3. In-Session Agent Lifecycle Protocol

Frontier coding agents (Codex, Claude, Cursor, Antigravity) integrate Code Autopsy into their generation loop:

$$\text{Baseline } A \longrightarrow \text{Pre-Edit Impact} \longrightarrow \text{Agent Edit } A \to B \longrightarrow \text{Verify } A \text{ vs } B \longrightarrow (\text{On FAIL: Repair via Evidence} \longrightarrow \text{Re-verify PASS})$$

1. **Before modifying sensitive files / interfaces:** Call `autopsy_impact` to discover the blast radius.
2. **After making code edits:** Call `autopsy_verify` to check for architectural drift or contract regressions.
3. **If findings fail:** Inspect the exact causal chain in the receipt and repair the code until `autopsy_verify` passes with a verified cryptographic receipt.

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
`~/.cargo/bin/autopsy` → workspace `target/release/autopsy` →
workspace `target/debug/autopsy` → `PATH`. Tests pin the workspace
debug build so they validate current code, not stale installs.

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

# Run unit tests
npm test

# Type check
npm run typecheck
```
