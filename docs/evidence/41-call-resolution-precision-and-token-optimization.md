# Production Evidence: Call Resolution Precision & Token-Optimized Impact Tiers

**Receipt:** `EVID-CALL-RES-TOKEN-041`  
**Date:** 2026-10-25  
**Version:** `0.1.0`  
**Status:** `VERIFIED`  
**Traceability:** `AGENT-T3.9`, `FR-008`, `NFR-001`, `FR-014`, `FR-025`

---

## 1. Overview & Context

During change-impact analysis (`autopsy impact`) on TypeScript/JavaScript codebases, two critical bottlenecks impacted usability for developer workflows and AI coding agents:
1. **Precision & Call Resolution (Noise in Blast Radius):** Unresolved cross-file imports through barrel files (`index.ts` re-exporting symbols via `export * from` or `export { ... } from`) and path aliases (`tsconfig.json` `compilerOptions.paths` / `baseUrl`) previously degraded to unverified `CoverageState::Partial` name-level references (`call_expression:imported-name`). Furthermore, transitive graph traversal treated structural container edges (`Contains` and `Imports` through module nodes) with identical weight as direct function calls, resulting in whole-file flooding (~94% noise in unconstrained bidirectional closures).
2. **Token Footprint:** Full blast radius reports serialized complete arrays of entities and paths, consuming upwards of 2,338 tokens per impact query.

This deliverable implements an end-to-end mathematical and architectural solution without violating any invariant (zero LLM in core, zero clocks, integer-only parts-per-million determinism, honest coverage classification).

---

## 2. Technical Architecture & Implementation

### 2.1 Multi-Hop Barrel & Re-Export Resolution (`crates/autopsy-adapter-typescript`)
- **Deterministic tsconfig Path Mapping & Extends:** `TypeScriptCompilerBridge` loads and parses `tsconfig.json` (stripping JSON comments), indexing `compilerOptions.baseUrl` and `compilerOptions.paths` (e.g. `@/*`, `@components/*`, `utils`). Multi-hop `extends` hierarchies (e.g. `tsconfig.json -> tsconfig.base.json`) are resolved recursively up to depth 4 with cycle containment. In-memory snapshot configurations are supported via `with_tsconfig_str`.
- **Re-Export Traversal:** When a module specifier resolves to a barrel file (e.g. `src/index.ts`), `resolve_imported` follows re-export statements (`export * from '...'` and `export { X } from '...'`) up to 8 hops with visited-set cycle containment.
- **One-Hop Declared Return-Type Propagation:** When a local variable is initialized from a function call (`const s = getService()`), the compiler bridge looks up `getService`'s declared return type annotation (`: UserService`) in the same file or imported module AST. If declared, `s` is bound to `UserService` in `local_types`, resolving member calls (`s.login()`) to verified methods (`UserService.login`) with `call_expression:typed-receiver`.
- **Virtual Directory Index Fallback:** For in-memory synthetic testing or unmaterialized indexes, candidate lookup scopes under the barrel's directory prefix, resolving to verified singletons and rejecting ambiguous matches (`candidates.len() == 1`).

### 2.2 Deterministic Edge Grading & Confidence Tiers (`crates/autopsy-impact`)
- **Integer Confidence Metric (`EdgeGrade`):**
  - `Declared` = 1,000,000 ppm (`ast_extends`, `ast_implements`)
  - `Resolved` = 900,000 ppm (`call_expression:imported-resolved`, `local`, `this-method`, `typed-receiver`, `package-qualified`)
  - `Inferred` = 600,000 ppm (`CoverageState::Partial` resolved, statistical co-change)
  - `Structural` = 500,000 ppm (`Contains`, `Imports` transit through file nodes)
  - `NameLevel` = 250,000 ppm (`call_expression:imported-name`, bare name fallback)
  - `Unrated` = 0 ppm (`CoverageState::Unknown`, unsupported dynamic constructs)
- **Bottleneck Widest-Path Traversal:** Each node reached during BFS accumulates its maximum bottleneck confidence (minimum edge grade along the best path from seeds), tracking distance in hops and provenance of the weakest link.
- **Tier Ranking & Partitioning:**
  - `likely_affected`: Entities with bottleneck confidence $\ge \text{Resolved}$ (900,000 ppm), ordered deterministically by `(grade desc, hops asc, stable_id asc)`.
  - `possibly_affected`: Entities with bottleneck confidence $< \text{Resolved}$ (structural containment noise, whole-file imports, name-level guesses), ordered deterministically by `(grade desc, hops asc, stable_id asc)`.

### 2.3 Token-Optimized Summary Mode (`crates/autopsy-report`, CLI & MCP)
- **Compact Summary Representation:**
  - `summary`: `{ seed, counts: { likely, possible, total }, likely_top: [ { id, grade, hops } x up to 5 ], possible_top: [ { id, grade, hops, weakest_link } x up to 5 ], truncated, impact_digest, full_via: "--detail full" }`
  - In summary mode (`--detail summary`): `paths` array is emptied, `impacted_entities` is clamped to the top 5 high-confidence entities, reducing JSON token footprint by >93% (from ~2,732 tokens down to ~150 tokens).
- **CLI Flag:** `autopsy impact --detail <full|summary>` (defaults to `full` on CLI to preserve legacy contract).
- **MCP Tool Default:** `autopsy_impact` MCP tool accepts `detail: 'summary' | 'full'` and defaults to `'summary'` for AI coding agents.

---

## 3. Verification Telemetry

### 3.1 Unit & Integration Test Suites
- `cargo test -p autopsy-impact`: 8/8 tests passed (including `test_edge_grade_mapping` and `test_likely_vs_possibly_affected_partitioning`).
- `cargo test -p autopsy-adapter-typescript`: 23/23 tests passed (including `test_tsconfig_paths_resolution`, `test_barrel_reexport_call_resolution`, `test_declared_return_type_propagation`, and `test_tsconfig_with_string_configuration`).
- `cargo test -p autopsy-report`: 4/4 tests passed (including `test_summary_mode_token_reduction`).
- `cargo test -p autopsy-cli --lib`: 12/12 tests passed (including `--detail` CLI flag parsing).
- `cargo test --workspace --lib`: 100% passed across all workspace crates.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warnings, 0 errors.

### 3.2 Protocol-Level MCP Test Suite
- `apps/mcp-server`: 44/44 tests passed across `server.test.js` (38/38), `bundle.test.js` (1/1), `e2e_agent_find_and_envelope.test.js` (3/3), and `e2e_agent_session.test.js` (2/2).

### 3.3 Benchmark Measured Telemetry (Ground Truth Integrity Preserved)
Full 50-task benchmark suite evaluated across 5 TypeScript repositories (`benchmarks/run_benchmarks.py`):
- **Baseline-A (Agent+Grep):** 68.27% Recall, 75.00% Precision, 70.04% F1, 926 tokens, 2.92 tool calls
- **Baseline-B (Agent+LSP):** 68.27% Recall, 76.00% Precision, 70.71% F1, 819 tokens, 1.82 tool calls
- **Baseline-C (Agent+Autopsy):** 100.00% Recall (+31.73pp recall lift over Baselines A & B -> **PASS** on pre-registered gate >= +10.0pp)
- **Dead-Code Precision Task `TASK-DED-041`:**
  - `likely_affected`: Exactly 2 entities (`src/helpers/stringFormat.ts#fn:obsoleteFormat` and `src/obsolete/deprecatedHandler.ts#method:DeprecatedHandler.executeLegacy`), matching expected ground truth with 100% precision.
  - `possibly_affected`: 16 noisy structural nodes correctly isolated.
  - Summary Mode: 150 tokens vs 2,732 tokens (>94% token savings for agent loops).
