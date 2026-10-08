# Mandatory Rules & Operational Protocol for AI Coding Agents

Every AI coding assistant working on **Synevid (Code Autopsy)** must strictly follow these rules without exception across the entire development lifecycle (Phases 0 through 6, v0.0.1 through 4.1).

---

## 1. Non-Negotiable Core Boundaries (01 Ch.12, 02 Ch.1, 03 Ch.2)

1. **Zero LLM in Correctness Engine:**
   - Core crates (`crates/*`) must never import or call LLM APIs (OpenAI, Anthropic, Gemini, LangChain, etc.).
   - All invariant evaluation, snapshot creation, multigraph traversal, contract deltas, and verification must be 100% deterministic code.
2. **Offline & Air-Gapped Operation:**
   - The engine must build and execute without network access.
   - Core analysis crates must make zero external HTTP/network calls.
3. **Read-Only Safety (FR-026):**
   - Analysis commands (`baseline`, `diff`, `impact`, `verify`, `explain`) must NEVER mutate source code or git history.
4. **100/100 Deterministic Repeatability (NFR-001, FR-028):**
   - 100 repeated runs on identical inputs must produce byte-identical BLAKE3 result digests.
   - Timestamps and non-semantic metadata must NEVER be included in `SnapshotId` or `result_digest`.
   - Never iterate over unordered collections (`std::collections::HashMap`, `HashSet`) in serialized or canonical outputs. Always use `BTreeMap` and `BTreeSet` or sorted vectors.
5. **Coverage Honesty & Adapter Dynamic Construct Rules (FR-014, FR-030):**
   - Unresolved dynamic constructs (`eval`, dynamic `import(...)`) MUST emit explicit `CoverageState::Unknown`.
   - Metaprogramming constructs (`Reflect.*`, `Proxy`, generated files) MUST emit explicit `CoverageState::Partial`.
   - Fully resolved AST declarations emit `CoverageState::Verified`.
   - Never collapse `UNKNOWN` or `PARTIAL` into `PASS` or `VERIFIED`.
6. **Ground Truth Integrity:**
   - Never modify benchmark ground truth datasets to inflate benchmark scores.
7. **Clean Dependency Inversion:**
   - Presentation, GUI (`apps/web-inspector`), and MCP (`apps/mcp-server`) layers depend on core public interfaces, NEVER the reverse. Enforced by `ARCH_NO_CORE_TO_MCP`.
8. **Ambiguous Rename Rule (FR-007):**
   - If multiple files share identical content hashes or multiple symbols share a signature in the same scope, renames MUST stay `Added` + `Deleted` / `Added` + `Removed`. Never guess.
9. **Cross-Platform Path Determinism (INV-CROSS-001):**
   - Every relative path emitted in snapshots, multigraphs, or diffs MUST strictly use POSIX forward slash (`/`). Windows backslashes (`\`) MUST be normalized to forward slashes at scanner ingestion.

---

## 2. In-Session Agent Lifecycle Protocol (4.1 Ch.4.2)

When an AI coding agent performs code modifications:
$$\text{Baseline } A \longrightarrow \text{Impact Assessment} \longrightarrow \text{Edit } A \to B \longrightarrow \text{Verify } A \text{ vs } B \longrightarrow (\text{On FAIL: Repair via Evidence} \longrightarrow \text{Re-verify PASS})$$

- **Before public-contract or architecture-sensitive edits:** Assess impact and downstream dependents.
- **After code edits:** Execute verification suite (`cargo test`, `make check`).
- **On FAIL finding:** Follow the exact failure paths emitted in [`Evidence`](file:///home/ravi/Projects/SynEvid/crates/autopsy-domain/src/lib.rs#L200) to locate the root cause, repair it, and re-verify until green.
- **Do not block on unsupported semantics:** Unless the strict CI policy explicitly demands full verification.

---

## 3. Explicit "Do-Not-Build" Prohibitions (4.1 Ch.13)

Violations of these prohibitions fail architecture review immediately:
1. **No LLM chatbot in core engine:** Correctness is deterministic.
2. **No vector DB / RAG:** Not permitted before empirical proof of necessity on structured graphs.
3. **No Neo4j or distributed graph:** Use memory-mapped and local petgraph structures until local limits are measured.
4. **No Kubernetes for local CLI:** Zero heavy infrastructure footprint.
5. **No 8 shallow language parsers:** One deep language first (TypeScript), complete and robust.
6. **No billing, auth, or tenant databases:** Never build before a multi-tenant cloud service exists.
7. **No 3D WebGL visualizations:** Simple, high-information 2D graphs and inspections only.
8. **No claims of perfect blast radius:** Always honestly declare `CoverageState::Unknown` on dynamic code.
9. **No agent modifications to benchmark ground truth:** Benchmark truth must be independently verified.

---

## 4. Mandatory 5-Step Agent Execution Workflow

Every coding session or agent task must follow this exact loop:

### Step 1: Inspect Requirements & Traceability
- Check affected requirements in [`docs/requirements/TRACEABILITY.md`](file:///home/ravi/Projects/SynEvid/docs/requirements/TRACEABILITY.md).
- Review relevant Architecture Decision Records in [`docs/adr/`](file:///home/ravi/Projects/SynEvid/docs/adr/).
- Inspect existing patterns and test fixtures before writing code.

### Step 2: Implement Smallest Coherent Changes
- Write idiomatic, memory-safe, deterministic code.
- Avoid unnecessary external dependencies.
- Adhere to Rust edition 2024 and MSRV 1.85+.

### Step 3: Implement Real Tests (100% Verification Coverage)
- **Zero Untested Changes:** Every feature, fix, or module MUST have accompanying tests.
- **Unit Tests:** Place in `#[cfg(test)] mod tests` within the crate.
- **End-to-End Tests:** Place in [`tests/tests/`](file:///home/ravi/Projects/SynEvid/tests/tests/) using [`autopsy-tests::TestSandbox`](file:///home/ravi/Projects/SynEvid/tests/src/lib.rs).
- **Test Categories Required:**
  - Real read/scan tests on realistic multi-file repository hierarchies.
  - Language adapter conformance test suite (`autopsy_adapter_api::conformance::verify_adapter_conformance`).
  - Negative test cases: invalid syntax, illegal characters, out-of-range values, missing files.
  - 100-run golden determinism loop assertions.
  - Never use mock placeholders that bypass core verification logic.

### Step 4: Multi-Stack Quality & Linting Protocol
Before concluding or proposing a commit, the agent MUST run the full verification matrix and resolve ALL errors and warnings:

#### A. Rust Engine & Crates:
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets -- --nocapture
cargo test --workspace --doc
./scripts/verify-determinism.sh
./scripts/verify-arch-boundaries.sh
```

#### B. Python (Benchmarks & Evaluation):
If Python scripts or benchmark tasks in `benchmarks/` are modified:
```bash
ruff check . --fix
ruff format --check .
bandit -r benchmarks/
pytest
```

#### C. TypeScript / Web Apps (MCP & GUI):
If `apps/mcp-server` or `apps/web-inspector` are touched:
```bash
npm run lint      # ESLint with zero errors
npm run format    # Prettier check
npm test          # Vitest / unit test runner
npx tsc --noEmit  # Strict TypeScript compilation check
```

#### D. Pre-Commit Enforcement:
```bash
pre-commit run --all-files   # OR: make check
```
All hooks must pass with exit code 0. Never use `--no-verify`.

### Step 5: Production-Grade Evidence Documentation
Before any commit:
- Create or update an evidence document in [`docs/evidence/`](file:///home/ravi/Projects/SynEvid/docs/evidence/) detailing:
  - What was changed and why.
  - How the solution works technically.
  - Exact command execution output and test telemetry.
  - Historical evidence records:
    - [`01_phase0_phase1_implementation_and_verification.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/01_phase0_phase1_implementation_and_verification.md)
    - [`02_phase2_typescript_adapter_verification.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/02_phase2_typescript_adapter_verification.md)
    - [`03_phase3_graph_diff_verification.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/03_phase3_graph_diff_verification.md)
    - [`04_phase4_impact_contracts_invariants_evidence_verification.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/04_phase4_impact_contracts_invariants_evidence_verification.md)
    - [`05_phase5_cli_storage_report_verification.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/05_phase5_cli_storage_report_verification.md)
    - [`06_phase6_benchmark_harness_verification.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/06_phase6_benchmark_harness_verification.md)
- Update [`docs/requirements/TRACEABILITY.md`](file:///home/ravi/Projects/SynEvid/docs/requirements/TRACEABILITY.md) when functional requirements are addressed.
- Update [`CHANGELOG.md`](file:///home/ravi/Projects/SynEvid/CHANGELOG.md).
- Keep [`MANIFEST.txt`](file:///home/ravi/Projects/SynEvid/MANIFEST.txt) synchronized with `git ls-files`.

---

## 5. Dynamic Rule Maintenance Invariant

As the repository evolves:
1. **Adding new crates:** Update `Cargo.toml` workspace members, `.pre-commit-config.yaml`, `.github/workflows/ci.yml`, and `Makefile`.
2. **Adding new language adapters or apps:** Add corresponding linting, formatting, and testing hooks to `.pre-commit-config.yaml` and `.github/workflows/ci.yml`.
3. **Updating contracts or result structures:** Update and validate JSON schemas in `schemas/` and update `docs/adr/`.
4. **Never allow rules to become stale:** The agent must proactively keep `AGENTS.md`, `INSTRUCTIONS.md`, and this rule file aligned with current codebase capabilities.
