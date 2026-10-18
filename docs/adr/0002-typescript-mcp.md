# ADR-002: TypeScript MCP gateway

Status: Accepted - 2026-09-28

Accepted. MCP transport is a separate TypeScript package using the official SDK; no analysis logic lives there.

*Update (2026-10-18):* the gateway now serves 11 read-only tools and 6 resources,
every answer travels in one evidence envelope `{command, exit_code, actionable,
report, repair?}`, and the machine-readable toolspec is served at the fixed URI
`autopsy://tools`. Protocol-level agent-loop tests through the real stdio server:
`apps/mcp-server/test/e2e_agent_find_and_envelope.test.js`,
`e2e_agent_session.test.js` (`docs/evidence/38-agent-gate-closure.md`).
