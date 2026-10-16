# Gemini Instructions for Synevid

Please read and follow the mandatory engineering rules and protocols defined in:
1. [`AGENTS.md`](file:///home/ravi/Projects/SynEvid/AGENTS.md)
2. [`.agents/rules/agent-engineering-rules.md`](file:///home/ravi/Projects/SynEvid/.agents/rules/agent-engineering-rules.md)
3. [`.agents/skills/synevid-core-engineer/SKILL.md`](file:///home/ravi/Projects/SynEvid/.agents/skills/synevid-core-engineer/SKILL.md)
4. [`docs/evidence/01-core-foundation-verification.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/01-core-foundation-verification.md)
5. [`docs/evidence/02-typescript-adapter-verification.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/02-typescript-adapter-verification.md)
6. [`docs/evidence/03-dependency-graph-and-diff.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/03-dependency-graph-and-diff.md)
7. [`docs/evidence/04-impact-contracts-and-invariants.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/04-impact-contracts-and-invariants.md)
8. [`docs/evidence/05-cli-and-storage-engine.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/05-cli-and-storage-engine.md)
9. [`docs/evidence/06-benchmark-harness-verification.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/06-benchmark-harness-verification.md)
10. [`docs/evidence/07-cross-platform-hardening.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/07-cross-platform-hardening.md)
11. [`docs/evidence/08-hardware-and-multi-arch-determinism.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/08-hardware-and-multi-arch-determinism.md)
12. [`docs/evidence/09-dual-cicd-and-release-lifecycle.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/09-dual-cicd-and-release-lifecycle.md)
13. [`docs/evidence/10-enterprise-sandboxing-and-defense.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/10-enterprise-sandboxing-and-defense.md)
14. [`docs/evidence/11-mcp-server-and-web-inspector.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/11-mcp-server-and-web-inspector.md)
15. [`docs/evidence/12-agent-lifecycle-protocol.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/12-agent-lifecycle-protocol.md)
16. [`docs/evidence/13-forensic-benchmark-closure.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/13-forensic-benchmark-closure.md) (real-binary Baseline-C, identity/shape split ADR-0009)
17. [`docs/evidence/14-precision-incremental-pr-gate.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/14-precision-incremental-pr-gate.md) — current: ranked top-k, callers profile, incremental baseline, PR gate
18. [`docs/evidence/15-security-and-compliance-audit.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/15-security-and-compliance-audit.md) — current: 4.1 audit fixes (config fail-open, graph pagination, deny, eslint, SBOM)
19. [`docs/adr/0009-symbol-identity-shape-split.md`](file:///home/ravi/Projects/SynEvid/docs/adr/0009-symbol-identity-shape-split.md)

Always run `make check` or `pre-commit run --all-files` before finalizing changes.
19. [`docs/evidence/16-python-adapter-and-installer.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/16-python-adapter-and-installer.md) — current: universal install, Python adapter, agent pack
