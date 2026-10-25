# Agent Documentation Index

Synevid's agent-facing surface is one evidence envelope, one machine-readable
contract, and three transports: the **MCP server** (`apps/mcp-server`), the
**CLI** (`autopsy`), and the **`synevid-lsp` editor server** (`apps/synevid-lsp`).
This directory indexes the material an agent — or the engineer wiring it — needs.

## Start here

| Document | What it gives you |
|---|---|
| [`TOOLS.md`](TOOLS.md) | The human rendering of the toolspec: the envelope, exit-code contract, all 23 subcommands, snapshot pinning, token budgets, the 11 MCP tools and 7 resources, environment variables, and the LSP/IDE plugin surfaces. |
| [`agents/recipes/repair-loop.md`](../../agents/recipes/repair-loop.md) | The end-to-end repair-loop playbook with session receipts. |
| [`agents/recipes/pinned-evidence.md`](../../agents/recipes/pinned-evidence.md) | Locking every answer to one immutable snapshot. |
| [`agents/recipes/pr-gate.md`](../../agents/recipes/pr-gate.md) | The CI merge gate: blocking exit codes and SARIF artifacts. |
| [`agents/recipes/multi-repo.md`](../../agents/recipes/multi-repo.md) | Registering several repositories and querying across them. |
| [`agents/skills/synevid-forensics.md`](../../agents/skills/synevid-forensics.md) | The installable skill file for agent hosts. |

## Verification & history

| Document | What it records |
|---|---|
| [`docs/evidence/12-agent-lifecycle-protocol.md`](../evidence/12-agent-lifecycle-protocol.md) | The original lifecycle protocol (baseline → impact → edit → verify → repair). |
| [`docs/evidence/35-tier1-agent-experience.md`](../evidence/35-tier1-agent-experience.md) | Tier 1: symbol resolution (`find`), one envelope, bounded map, snapshot pinning, token budgets. |
| [`docs/evidence/36-tier2-agent-quality.md`](../evidence/36-tier2-agent-quality.md) | Tier 2: repair advice, Go/Rust adapters, workspace registry, pinned browse, session receipts. |
| [`docs/evidence/37-tier3-polish.md`](../evidence/37-tier3-polish.md) | Tier 3: toolspec, `synevid-lsp`, suggestion receipts, agent docs, performance ceilings. |
| [`docs/evidence/38-agent-gate-closure.md`](../evidence/38-agent-gate-closure.md) | Protocol-level MCP end-to-end tests and the honest promotion-gate status. |
| [`docs/requirements/TRACEABILITY.md`](../requirements/TRACEABILITY.md) | `AGENT-T1.*`–`AGENT-T3.*` and `AGENT-GATE-1` requirement rows. |

## Host wiring

- [`scripts/setup-agent-hosts.py`](../../scripts/setup-agent-hosts.py) detects and
  configures Claude Code / Claude Desktop, Cursor, Windsurf, VS Code, opencode,
  and Gemini CLI (re-run safe; timestamped backups).
- [`agents/hooks/autopsy-session.sh`](../../agents/hooks/autopsy-session.sh) (opt-in)
  records a repair-loop session receipt per commit;
  [`agents/hooks/safe-edit.sh`](../../agents/hooks/safe-edit.sh) is an advisory,
  fail-open pre-edit blast-radius prompt.
- [`scripts/install.sh`](../../scripts/install.sh) installs both `autopsy` and
  `synevid-lsp` (prebuilt release binary or source build).
