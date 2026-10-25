# Synevid Engineering Instructions

This document specifies the operational instructions for all human developers and AI coding agents working on **Synevid**.

Please refer to:
- [`AGENTS.md`](file:///home/ravi/Projects/SynEvid/AGENTS.md) — Core non-negotiable boundaries, loop for changes, and self-evolution invariants.
- [`.agents/rules/agent-engineering-rules.md`](file:///home/ravi/Projects/SynEvid/.agents/rules/agent-engineering-rules.md) — Detailed 5-step agent execution protocol, multi-stack linting commands, and evidence logging standards.
- [`.agents/skills/synevid-core-engineer/SKILL.md`](file:///home/ravi/Projects/SynEvid/.agents/skills/synevid-core-engineer/SKILL.md) — Architecture invariants, determinism rules, language adapter standards, and verification checklist.
- [`docs/requirements/TRACEABILITY.md`](file:///home/ravi/Projects/SynEvid/docs/requirements/TRACEABILITY.md) — Requirement rows (FR/NFR/FR-ADV plus `AGENT-T1.*`–`AGENT-GATE-1`) mapped to crates and tests.
- [`docs/agents/TOOLS.md`](file:///home/ravi/Projects/SynEvid/docs/agents/TOOLS.md) — The agent-facing contract: envelope, exit codes, all 23 subcommands, MCP tools/resources, LSP and IDE plugin surfaces.
- [`docs/evidence/`](file:///home/ravi/Projects/SynEvid/docs/evidence/) — Production receipts 01–40. The canonical verification checklist is [`33-verification-procedures.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/33-verification-procedures.md); the agent-experience, install-lifecycle, and P0 release/plugin/forensic-trajectory records are [`35-tier1-agent-experience.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/35-tier1-agent-experience.md), [`36-tier2-agent-quality.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/36-tier2-agent-quality.md), [`37-tier3-polish.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/37-tier3-polish.md), [`38-agent-gate-closure.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/38-agent-gate-closure.md), [`39-install-update-lifecycle.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/39-install-update-lifecycle.md), and [`40-p0-release-plugin-and-forensic-trajectory.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/40-p0-release-plugin-and-forensic-trajectory.md).

## Quick Command Reference
```bash
# Complete quality, boundary, and determinism check (entire gate stack):
make check

# Run all workspace unit and end-to-end tests:
make test

# Format code:
make fmt

# Execute all pre-commit hooks (14 hooks):
pre-commit run --all-files
```
