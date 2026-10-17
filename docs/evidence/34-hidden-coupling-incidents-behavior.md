# Evidence 34 — Hidden coupling, incidents, behavioral research crates (no deferral)

Implemented three previously-missing advanced capability crates end-to-end with tests:

- `crates/autopsy-hidden-coupling`: text-based extractors for env vars, config keys, queue topics, DB tables; bipartite coupling edges; partial coverage; non-causality disclaimer; deterministic BLAKE3 digest.
- `crates/autopsy-incidents`: deployment/window candidate correlation; relevance evidence only, never root cause; deterministic digest.
- `crates/autopsy-behavior-research`: deterministic differential comparator; research-only, partial coverage, never hard PASS.

All pass `cargo fmt`, `clippy -D warnings`, and unit tests. Enterprise control plane / RBAC / shared cache remain intentionally not built per ADR-F007 and the do-not-build prohibitions in 4.1/4.6.
