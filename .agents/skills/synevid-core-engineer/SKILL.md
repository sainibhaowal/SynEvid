---
name: synevid-core-engineer
description: Canonical engineering rules, architecture invariants, determinism standards, and verification workflow for the Synevid (Code Autopsy) engine.
---

# Synevid Engineering Skill & Protocol

This skill encodes the mandatory standards, non-negotiable boundaries, and verification cycle for all development on **Synevid**.

---

## 1. Non-Negotiable Core Boundaries

1. **Zero LLM in Correctness Path:**
   - Pass/fail verification, invariant evaluation, graph traversal, and contract deltas are 100% deterministic code.
   - Core crates (`crates/*`) must never import or call LLM APIs.
2. **Offline & Zero Network I/O:**
   - Engine commands must succeed in an air-gapped environment with no internet access.
   - Core analysis crates must make zero network requests.
3. **Read-Only Engine (FR-026):**
   - Analysis commands (`baseline`, `diff`, `impact`, `verify`, `explain`) must NEVER mutate source code or git history.
4. **100/100 Reproducible Determinism (NFR-001, FR-028):**
   - 100 repeated runs on identical inputs must produce byte-identical BLAKE3 result digests.
   - Timestamps are strictly excluded from semantic identity (`SnapshotId`, `result_digest`).
   - Forbid iterating unordered collections (`std::collections::HashMap`, `HashSet`) in canonical paths. Use `BTreeMap` and `BTreeSet` or sorted vectors.
5. **Coverage Honesty (FR-014, FR-030):**
   - Unsupported constructs (reflection, dynamic imports, generated code) must emit explicit `CoverageState::Unknown` or `CoverageState::Partial`.
   - Never collapse `UNKNOWN` into `PASS`.
6. **Ground Truth Integrity:**
   - Never modify benchmark ground truth simply to increase benchmark scores.
7. **Clean Dependency Inversion:**
   - Presentation, GUI, and MCP layers depend on core public interfaces, NEVER the reverse.

---

## 2. Language Adapter Implementation & Conformance Standards (Phase 2)

Each language adapter (TypeScript/JavaScript, Python, Go, Rust) must satisfy the following invariants:
1. **Trait Conformance:** Must implement [`LanguageAdapter`](file:///home/ravi/Projects/SynEvid/crates/autopsy-adapter-api/src/lib.rs) and pass the reusable test suite [`verify_adapter_conformance`](file:///home/ravi/Projects/SynEvid/crates/autopsy-adapter-api/src/conformance.rs).
2. **Grammar & Tree-Sitter Bootstrap:** Use tree-sitter 0.24+ with official language grammar parsers (e.g. `tree-sitter-typescript` for `.ts` and `.tsx`).
3. **Resilient Symbol Identifiers (FR-005, ADR-0009):** `SymbolId` must be derived from canonical namespace hierarchy, file path, and symbol kind—**never from transient line numbers**. Line shifts and comment additions must produce identical `SymbolId`s.
4. **Identity / Shape Split (ADR-0009, FR-007/FR-009/FR-010):** Identities MUST NOT embed signature hashes or any shape material. Shape lives in `Symbol.normalized_signature` and `Contract.inputs/output`, compared explicitly by diff/compat engines. A signature edit must surface as `SignatureModified` + breaking contract delta, never delete+create. Overload collisions receive deterministic `:overload:N` suffixes ordered by signature bytes. All identity construction goes through `autopsy-symbols`.
5. **Call-Edge Honesty Tiers (FR-008):** `Calls` targets resolve in tiers — same-file binding, `this.`-method, `new` class, import-resolved (exactly one same-named symbol in the resolved target module via the global index), typed receivers (declared field/parameter/local class types). Ambiguous or untyped callees keep name-level `symbol::{name}` targets with `Partial` coverage and explicit provenance. Never invent cross-file bindings.
6. **Canonical Path & Specifier Resolution:** Use [`TypeScriptCompilerBridge`](file:///home/ravi/Projects/SynEvid/crates/autopsy-adapter-typescript/src/lib.rs) for canonicalizing relative imports (`./foo`, `../bar`, `.ts`/`.tsx`/index resolution) and package specifiers.
7. **Honest Semantic Coverage (FR-014, FR-030):**
   - Unresolved dynamic constructs (`eval`, dynamic `import(...)`) MUST emit [`CoverageState::Unknown`](file:///home/ravi/Projects/SynEvid/crates/autopsy-domain/src/lib.rs).
   - Metaprogramming constructs (`Reflect.*`, `Proxy`, generated files) MUST emit [`CoverageState::Partial`](file:///home/ravi/Projects/SynEvid/crates/autopsy-domain/src/lib.rs).
   - Fully resolved AST entities emit [`CoverageState::Verified`](file:///home/ravi/Projects/SynEvid/crates/autopsy-domain/src/lib.rs).
   - **Hard invariant:** Never convert `Unknown` or `Partial` into `Verified` or `Pass`.

---

## 3. Dependency Graph & Semantic Diff Standards (Phase 3)

All graph and diff operations must satisfy the following invariants:
1. **Petgraph Directed Multigraph (FR-006):** In `crates/autopsy-graph`, use `petgraph::graph::DiGraph<Symbol, Edge>` with a secondary index `BTreeMap<SymbolId, NodeIndex>`. Multiple typed edges between identical node pairs must be supported.
2. **Explicit Edge Provenance (FR-006):** Every edge must retain provenance metadata (`adapter`, `location`, `method`) linking it back to the AST extraction pass.
3. **Deterministic SCC & Cycle Traversal:** Cycle detection and Strongly Connected Components condensation must use deterministic sorting over Tarjan's SCC results.
4. **Incremental Subgraph Updates:** File updates in the graph (`update_file`) must only remove and replace symbols from the modified file in $O(|V_{file}| + |E_{file}|)$, avoiding an $O(V^2)$ full graph rebuild.
5. **Ambiguous Rename Rule (FR-007):** In `crates/autopsy-diff`, file and symbol renames are only classified as `Renamed` if the candidate deleted and added pair is **unambiguous** (exactly 1:1 match). If multiple files share identical content hashes, or multiple symbols share a signature in the same scope, they **strictly remain Added + Deleted / Added + Removed**.
6. **Breaking Contract Detection (FR-007, FR-009):** Contract modifications must explicitly flag `is_breaking: true` when public visibility is narrowed, input parameter types/counts change, or return types change.
7. **Changed Frontier:** Diff outputs must calculate both the directly changed symbols (`direct_symbols`) and the 1-hop dependent boundary (`impacted_boundary`).
8. **Golden Diff Fixture Gate:** Diff calculations on frozen fixture repositories must produce exact byte-for-byte JSON matches.
   - Go specifics (T2.2): cgo/plugin/reflect produce `Unknown` (code outside the analyzed bytes), `unsafe`/`//go:generate` produce `Partial`; imports anchor package dirs via `go_import:` provenance and unresolved imports emit name-level Partial edges; pointer receivers assert `mutates-receiver`; the Rust bridge prefers `<path>.rs` over `<path>/mod.rs` for `crate::` probes and `include!` is `Unknown`.
9. **Two-State Honesty (FR-007, FR-010):** `diff` and `verify --baseline-id` must rebuild the genuine before-state graph/contracts from content-addressed snapshot bytes — never compare a state against itself. Unknown baseline ids are hard errors, never silent fallbacks. Compatibility with an empty before-map reports `Unknown`, never a vacuous `Pass`.
10. **Precision-First Impact (FR-008):** Default (`full`) traversal preserves maximum recall. The `callers` preset (backward + Calls/References, `traverse_file_nodes=false`, top-k shortlist) is the recommended reviewer workflow. `ranked_top` is presentation only — `impacted_entities` always carries the complete evidence set. Never tune ranking weights against the benchmark corpus.
11. **Incremental Soundness (FR-020):** Per-file artifacts key by content hash. Edge reuse additionally requires an identical global symbol-id set; otherwise recompute all edges. Any cache miss degrades to full analysis, never to a partial answer.

---

## 4. Mandatory Pre-Commit & Verification Loop

Before any commit or merge, execute the full verification chain (all gates must pass):

```bash
# 1. Format check
cargo fmt --all -- --check

# 2. Strict lints (-D warnings)
cargo clippy --workspace --all-targets -- -D warnings

# 3. All workspace unit & integration tests (924 at the 2026-10-18 gate — always
#    report the count you observe; includes the synevid-lsp suite)
cargo test --workspace --all-targets -- --nocapture

# 4. Doc tests
cargo test --workspace --doc

# 5. Golden determinism assertion (100 runs)
cargo test -p autopsy-repo -- test_compute_snapshot_id_golden_100_runs
./scripts/verify-determinism.sh

# 6. Architectural boundary gate (zero LLM/MCP/LSP in core crates)
./scripts/verify-arch-boundaries.sh

# 7. JSON Schema validation
for s in schemas/*.json; do python3 -m json.tool "$s" >/dev/null; done

# 8. MCP server, Web Inspector, VS Code Extension, and Installer suites
cd apps/mcp-server && npm run typecheck && npm run lint && npm test
cd ../web-inspector && npm test
cd ../vscode-extension && npm test
cd ../.. && bash scripts/tests/test-install.sh

# 9. Pre-commit all-files check (14 hooks — the aggregated gate)
make check
```

---

## 5. Architecture, Traceability & Evidence Records

When implementing features:
1. Identify affected requirements in [`docs/requirements/TRACEABILITY.md`](file:///home/ravi/Projects/SynEvid/docs/requirements/TRACEABILITY.md).
2. Follow established Architecture Decision Records in [`docs/adr/`](file:///home/ravi/Projects/SynEvid/docs/adr/).
3. Add end-to-end integration tests in [`tests/tests/`](file:///home/ravi/Projects/SynEvid/tests/tests/) using `autopsy-tests::TestSandbox`.
4. Document all changes and verification evidence in [`docs/evidence/`](file:///home/ravi/Projects/SynEvid/docs/evidence/) — dossiers are numbered `01`–`40` (one per phase, pillar, agent tier, or lifecycle closure). Start with the verification checklist [`33-verification-procedures.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/33-verification-procedures.md); the agent-experience, install-lifecycle, and P0 release/plugin/forensic-trajectory records are [`35-tier1-agent-experience.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/35-tier1-agent-experience.md), [`36-tier2-agent-quality.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/36-tier2-agent-quality.md), [`37-tier3-polish.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/37-tier3-polish.md), [`38-agent-gate-closure.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/38-agent-gate-closure.md), [`39-install-update-lifecycle.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/39-install-update-lifecycle.md), and [`40-p0-release-plugin-and-forensic-trajectory.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/40-p0-release-plugin-and-forensic-trajectory.md).
5. Update [`CHANGELOG.md`](file:///home/ravi/Projects/SynEvid/CHANGELOG.md) and [`MANIFEST.txt`](file:///home/ravi/Projects/SynEvid/MANIFEST.txt).
