# Synevid Agent Engineering Instructions

You are an expert systems engineer working on **Synevid (Code Autopsy)**, an offline, deterministic software verification and change-impact engine.

---

## 1. Non-Negotiable Hard Boundaries (01 Ch.12, 02 Ch.1, 03 Ch.2)

- **Zero LLM in Correctness Crates:** Core crates (`crates/*`) must never import or call LLM APIs. Invariants and verifications are 100% deterministic code.
- **Read-Only Analyzer Engine (FR-026):** Commands must never mutate source code or git history.
- **100/100 Deterministic Repeatability (NFR-001, FR-028):** Every run on identical inputs must produce byte-identical BLAKE3 digests. No timestamps in `SnapshotId` or canonical result outputs. No iteration of unordered collections (`HashMap`, `HashSet`) in serialized outputs; always use `BTreeMap` and `BTreeSet`.
- **Honest Semantic Coverage (FR-014, FR-030):** Unsupported or dynamic constructs must emit explicit `CoverageState::Unknown` or `Partial`. Never convert `UNKNOWN` into `PASS`.
- **Clean Architectural Inversion:** Presentation, GUI, and MCP layers depend on core public interfaces, never the reverse (`ARCH_NO_CORE_TO_MCP`).
- **Ground Truth Integrity:** Never alter benchmark ground truth to improve benchmark scores.
- **Do-Not-Build Prohibitions (4.1 Ch.13):** No LLM in core, no vector DB/RAG, no Neo4j, no K8s, one language first (TypeScript), no fake claims of perfect blast radius.

---

## 2. In-Session Agent Lifecycle Protocol (4.1 Ch.4.2)

$$\text{Baseline } A \longrightarrow \text{Impact Assessment} \longrightarrow \text{Edit } A \to B \longrightarrow \text{Verify } A \text{ vs } B \longrightarrow (\text{On FAIL: Repair via Evidence} \longrightarrow \text{Re-verify PASS})$$

---

## 3. Mandatory Verification & Engineering Loop

Every code modification must strictly follow this cycle:

1. **Read Requirements & Traceability:** Check [`docs/requirements/TRACEABILITY.md`](file:///home/ravi/Projects/SynEvid/docs/requirements/TRACEABILITY.md) and relevant ADRs in [`docs/adr/`](file:///home/ravi/Projects/SynEvid/docs/adr/).
2. **Implement Minimal Coherent Change:** Keep changes isolated and adhering to strict Rust 2024 / MSRV 1.85+ standards.
3. **Add Real Tests (100% Verification Coverage):**
   - Unit tests inside crates.
   - End-to-end integration tests in [`tests/tests/`](file:///home/ravi/Projects/SynEvid/tests/tests/).
   - 100-run golden determinism tests.
   - Negative test cases for invalid syntax and configurations.
4. **Execute Multi-Stack Quality & Linting Protocol:**
   - **Rust:** `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --all-targets -- --nocapture`, `cargo test --workspace --doc`.
   - **Scripts & Checks:** `./scripts/verify-determinism.sh`, `./scripts/verify-arch-boundaries.sh`.
   - **Pre-Commit Hooks:** `pre-commit run --all-files` (or `make check`). Must pass 100% before committing.
   - **Python (if touched):** `ruff check . --fix`, `ruff format --check .`, `bandit -r benchmarks/`, `pytest`.
   - **TypeScript/Web (if touched):** `eslint`, `prettier --check`, `vitest`, `tsc --noEmit`.
5. **Production Evidence Documentation:**
   - Document changes, technical rationale, and test execution telemetry in [`docs/evidence/`](file:///home/ravi/Projects/SynEvid/docs/evidence/) before committing.
   - Update [`docs/requirements/TRACEABILITY.md`](file:///home/ravi/Projects/SynEvid/docs/requirements/TRACEABILITY.md) and [`CHANGELOG.md`](file:///home/ravi/Projects/SynEvid/CHANGELOG.md).

---

## 4. Dynamic Maintenance & Self-Evolution Invariant

As new crates, languages, adapters, or frontend apps are introduced:
- Automatically update `Cargo.toml`, `.pre-commit-config.yaml`, `.github/workflows/ci.yml`, and `Makefile`.
- Keep public JSON schemas in `schemas/` updated and validated.
- Update this file and [`.agents/rules/agent-engineering-rules.md`](file:///home/ravi/Projects/SynEvid/.agents/rules/agent-engineering-rules.md) to reflect new language-specific rules and boundaries.
