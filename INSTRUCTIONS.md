# Synevid Engineering Instructions

This document specifies the operational instructions for all human developers and AI coding agents working on **Synevid**.

Please refer to:
- [`AGENTS.md`](file:///home/ravi/Projects/SynEvid/AGENTS.md) — Core non-negotiable boundaries, loop for changes, and self-evolution invariants.
- [`.agents/rules/agent-engineering-rules.md`](file:///home/ravi/Projects/SynEvid/.agents/rules/agent-engineering-rules.md) — Detailed 5-step agent execution protocol, multi-stack linting commands, and evidence logging standards.
- [`.agents/skills/synevid-core-engineer/SKILL.md`](file:///home/ravi/Projects/SynEvid/.agents/skills/synevid-core-engineer/SKILL.md) — Architecture invariants, determinism rules, language adapter standards, and verification checklist.
- [`docs/evidence/01_phase0_phase1_implementation_and_verification.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/01_phase0_phase1_implementation_and_verification.md) — Phase 0 & Phase 1 verification telemetry.
- [`docs/evidence/02_phase2_typescript_adapter_verification.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/02_phase2_typescript_adapter_verification.md) — Phase 2 TypeScript adapter and conformance verification telemetry.

## Quick Command Reference
```bash
# Complete quality, boundary, and determinism check (all 8 gates):
make check

# Run all unit and end-to-end integration tests (29/29 passing):
make test

# Format code:
make fmt

# Execute all pre-commit hooks:
pre-commit run --all-files
```
