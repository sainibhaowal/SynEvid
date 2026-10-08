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
| `autopsy_status` | `path?: string` | Inspects workspace indexing state, adapter capabilities, and BLAKE3 snapshot identity. | Index status, snapshot ID, discovered language files. |
| `autopsy_impact` | `symbol: string`, `path?: string`, `direction?: "forward"\|"backward"\|"bidirectional"`, `depth?: number` | Performs deterministic BFS blast radius traversal over the symbol dependency multigraph. | Reachable entities, edge counts, truncation flags, coverage state. |
| `autopsy_diff` | `base?: string`, `head?: string`, `path?: string` | Computes semantic delta between two snapshot revisions or git references. | Symbol deltas (added, removed, modified), signature breaks. |
| `autopsy_verify` | `path?: string`, `profile?: string`, `base?: string` | Evaluates architectural and contract invariants against the target codebase. | Findings (`pass`/`fail`/`unknown`), cryptographic receipt, exit code telemetry. |
| `autopsy_explain` | `finding_id: string`, `path?: string` | Derives the full causal path and evidence receipt for a specific finding. | Exact source locations, multigraph edges, evidence bundle. |

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
      "args": ["/absolute/path/to/SynEvid/apps/mcp-server/dist/index.js"]
    }
  }
}
```

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
