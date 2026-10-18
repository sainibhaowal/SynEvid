# Claude Instructions for Synevid

Please read and follow the mandatory engineering rules and protocols defined in:
1. [`AGENTS.md`](file:///home/ravi/Projects/SynEvid/AGENTS.md) — Hard boundaries, mandatory verification loop, self-evolution invariant.
2. [`.agents/rules/agent-engineering-rules.md`](file:///home/ravi/Projects/SynEvid/.agents/rules/agent-engineering-rules.md) — The 5-step agent execution protocol and multi-stack quality commands.
3. [`.agents/skills/synevid-core-engineer/SKILL.md`](file:///home/ravi/Projects/SynEvid/.agents/skills/synevid-core-engineer/SKILL.md) — Architecture invariants and the pre-commit verification chain.
4. [`docs/requirements/TRACEABILITY.md`](file:///home/ravi/Projects/SynEvid/docs/requirements/TRACEABILITY.md) — FR/NFR/FR-ADV plus `AGENT-T1.*`–`AGENT-GATE-1` rows mapped to crates and tests.
5. [`docs/agents/TOOLS.md`](file:///home/ravi/Projects/SynEvid/docs/agents/TOOLS.md) — The agent-facing contract: one envelope, exit codes, all 22 subcommands, 11 MCP tools, 6 resources, LSP surface.
6. [`docs/evidence/`](file:///home/ravi/Projects/SynEvid/docs/evidence/) — Production receipts 01–38. Start with [`33-verification-procedures.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/33-verification-procedures.md); the agent-experience records are [`35`](file:///home/ravi/Projects/SynEvid/docs/evidence/35-tier1-agent-experience.md), [`36`](file:///home/ravi/Projects/SynEvid/docs/evidence/36-tier2-agent-quality.md), [`37`](file:///home/ravi/Projects/SynEvid/docs/evidence/37-tier3-polish.md), and [`38`](file:///home/ravi/Projects/SynEvid/docs/evidence/38-agent-gate-closure.md).

Always run `make check` or `pre-commit run --all-files` before finalizing changes.
