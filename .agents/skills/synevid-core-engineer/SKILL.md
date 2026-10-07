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

All language adapters (starting with TypeScript in `crates/autopsy-adapter-typescript`) must satisfy the following invariants:
1. **Trait Conformance:** Must implement [`LanguageAdapter`](file:///home/ravi/Projects/SynEvid/crates/autopsy-adapter-api/src/lib.rs) and pass the reusable test suite [`verify_adapter_conformance`](file:///home/ravi/Projects/SynEvid/crates/autopsy-adapter-api/src/conformance.rs).
2. **Grammar & Tree-Sitter Bootstrap:** Use tree-sitter 0.24+ with official language grammar parsers (e.g. `tree-sitter-typescript` for `.ts` and `.tsx`).
3. **Resilient Symbol Identifiers (FR-005):** `SymbolId` must be derived from canonical namespace hierarchy, file path, and symbol kind—**never from transient line numbers**. Line shifts and comment additions must produce identical `SymbolId`s.
4. **Canonical Path & Specifier Resolution:** Use [`TypeScriptCompilerBridge`](file:///home/ravi/Projects/SynEvid/crates/autopsy-adapter-typescript/src/lib.rs) for canonicalizing relative imports (`./foo`, `../bar`, `.ts`/`.tsx`/index resolution) and package specifiers.
5. **Honest Semantic Coverage (FR-014, FR-030):**
   - Unresolved dynamic constructs (`eval`, dynamic `import(...)`) MUST emit [`CoverageState::Unknown`](file:///home/ravi/Projects/SynEvid/crates/autopsy-domain/src/lib.rs).
   - Metaprogramming constructs (`Reflect.*`, `Proxy`, generated files) MUST emit [`CoverageState::Partial`](file:///home/ravi/Projects/SynEvid/crates/autopsy-domain/src/lib.rs).
   - Fully resolved AST entities emit [`CoverageState::Verified`](file:///home/ravi/Projects/SynEvid/crates/autopsy-domain/src/lib.rs).
   - **Hard invariant:** Never convert `Unknown` or `Partial` into `Verified` or `Pass`.

---

## 3. Mandatory Pre-Commit & Verification Loop

Before any commit or merge, execute the full verification chain (all 8 gates must pass):

```bash
# 1. Format check
cargo fmt --all -- --check

# 2. Strict lints (-D warnings)
cargo clippy --workspace --all-targets -- -D warnings

# 3. All workspace unit & integration tests (29/29 passing)
cargo test --workspace --all-targets -- --nocapture

# 4. Doc tests
cargo test --workspace --doc

# 5. Golden determinism assertion (100 runs)
cargo test -p autopsy-repo -- test_compute_snapshot_id_golden_100_runs
./scripts/verify-determinism.sh

# 6. Architectural boundary gate (Zero LLM/MCP in core crates)
./scripts/verify-arch-boundaries.sh

# 7. JSON Schema validation
for s in schemas/*.json; do python3 -m json.tool "$s" >/dev/null; done

# 8. Pre-commit all-files check
make check
```

---

## 4. Architecture, Traceability & Evidence Records

When implementing features:
1. Identify affected requirements in [`docs/requirements/TRACEABILITY.md`](file:///home/ravi/Projects/SynEvid/docs/requirements/TRACEABILITY.md).
2. Follow established Architecture Decision Records in [`docs/adr/`](file:///home/ravi/Projects/SynEvid/docs/adr/).
3. Add end-to-end integration tests in [`tests/tests/`](file:///home/ravi/Projects/SynEvid/tests/tests/) using `autopsy-tests::TestSandbox`.
4. Document all changes and verification evidence in [`docs/evidence/`]:
   - [`01_phase0_phase1_implementation_and_verification.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/01_phase0_phase1_implementation_and_verification.md)
   - [`02_phase2_typescript_adapter_verification.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/02_phase2_typescript_adapter_verification.md)
5. Update [`CHANGELOG.md`](file:///home/ravi/Projects/SynEvid/CHANGELOG.md) and [`MANIFEST.txt`](file:///home/ravi/Projects/SynEvid/MANIFEST.txt).
