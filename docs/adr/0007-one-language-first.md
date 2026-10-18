# ADR-007: One production language adapter first

Status: Accepted - 2026-09-28

Accepted. Semantic depth is prioritized over superficial breadth.

*Update (2026-10-17):* the adapter set is now TypeScript/JavaScript, Python, Go,
and Rust — all behind the same `adapter-api` trait, each with conformance suites
and honest `Unknown`/`Partial` coverage states (`docs/evidence/36-tier2-agent-quality.md`).
The one-deep-first rule is unchanged: a language ships depth (stable identity,
per-edge provenance, coverage honesty) before the next begins.
