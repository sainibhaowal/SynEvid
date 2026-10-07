# Contributing to SynEvid

Thank you for your interest in contributing to **SynEvid (Code Autopsy)**!

SynEvid is an offline, mathematically deterministic software verification and change-impact engine designed to provide external ground truth for autonomous AI coding agents and safety-critical engineering pipelines.

To preserve the engine's cryptographic repeatability and integrity, all contributions must strictly adhere to the standards outlined in this guide.

---

## 1. Non-Negotiable Hard Boundaries

Before contributing, ensure you are familiar with the architectural boundaries defined in [`AGENTS.md`](AGENTS.md) and [`docs/architecture/OVERVIEW.md`](docs/architecture/OVERVIEW.md):

1. **Zero LLM in Correctness Crates (`crates/*`):** Core verifiers, diff engines, and graph analyzers must never import or call LLM APIs. Invariants and verifications are 100% deterministic code.
2. **Read-Only Analyzer Engine (FR-026):** Engine commands must never mutate target source code, working trees, or git history.
3. **100/100 Deterministic Repeatability (NFR-001, FR-028):** Every run on identical inputs must produce byte-identical BLAKE3 digests.
   - **No unordered collections:** Never serialize or iterate `HashMap` / `HashSet` in canonical outputs. Always use `BTreeMap` and `BTreeSet`.
   - **No non-deterministic inputs:** Never inject system timestamps, random seeds, or machine-specific absolute paths into canonical `SnapshotId`, evidence receipts, or result contracts.
4. **Honest Semantic Coverage (FR-014, FR-030):** Dynamic or unsupported language constructs must emit explicit `CoverageState::Unknown` or `Partial`. **Never collapse `UNKNOWN` into `PASS`**.
5. **Clean Architectural Inversion (`ARCH_NO_CORE_TO_MCP`):** Presentations, web inspectors, and MCP adapters depend on core crate interfaces—never the reverse.
6. **Ground Truth Integrity:** Never alter benchmark ground truth to artificially inflate benchmark scores.

---

## 2. Development Environment Setup

### Prerequisites
* **Rust:** MSRV `1.85+` (Rust 2024 edition). Install via [rustup](https://rustup.rs/).
* **Python:** `3.11+` with `tomllib` and `pyyaml` (used by schema validation hooks).
* **Pre-commit:** Install via `pip install pre-commit` or your system package manager.
* **Make:** Standard POSIX `make`.

### Quick Setup
```bash
# Clone the repository
git clone git@github.com:sainibhaowal/SynEvid.git
cd SynEvid

# Install Git pre-commit and pre-push hooks
pre-commit install --hook-type pre-commit --hook-type pre-push
```

---

## 3. Mandatory Verification Loop

Every code modification must pass the full multi-stack verification gate before a pull request can be merged:

```bash
# Run the complete verification suite (recommended)
make check
```

Under the hood, `make check` executes:
1. **Formatting:** `cargo fmt --all -- --check`
2. **Strict Lints:** `cargo clippy --workspace --all-targets -- -D warnings`
3. **Unit & Integration Tests:** `cargo test --workspace --all-targets -- --nocapture`
4. **Documentation Tests:** `cargo test --workspace --doc`
5. **100-Run Determinism Gate:** `./scripts/verify-determinism.sh` (validates byte-identical BLAKE3 snapshot digests across 100 runs)
6. **Architectural Boundary Gate:** `./scripts/verify-arch-boundaries.sh` (ensures zero LLM/MCP leaks in core crates)
7. **Schema & Config Validation:** JSON Schema validation and configuration syntax checks

---

## 4. Branching & Commit Guidelines

### Branch Naming
Create focused, short-lived feature branches branching from `main`:
- `feat/<short-description>`: New verifiers, adapters, or capabilities
- `fix/<short-description>`: Bug fixes or determinism repairs
- `perf/<short-description>`: Performance improvements (with benchmark evidence)
- `docs/<short-description>`: Documentation, ADRs, or specification updates
- `refactor/<short-description>`: Code restructuring without functional change

### Commit Messages
We follow [Conventional Commits](https://www.conventionalcommits.org/):
```
<type>(<scope>): <concise description in imperative mood>

[optional body explaining rationale and requirements context]

[optional footer referencing requirements, e.g., Refs: FR-012]
```
Examples:
- `feat(graph): implement strongly-connected component cycle detection`
- `fix(storage): replace HashMap with BTreeMap for deterministic serialization`
- `docs(adr): add ADR-0009 for adapter streaming architecture`

---

## 5. Pull Request Process

1. **Keep PRs Minimal and Coherent:** Avoid kitchen-sink PRs. Focus each PR on one verifiable objective.
2. **Include Deterministic Tests:** Add unit tests within crates and end-to-end integration tests in [`tests/tests/`](tests/tests/). Include negative test cases for invalid inputs and edge conditions.
3. **Update Traceability:** If implementing or modifying requirements, update [`docs/requirements/TRACEABILITY.md`](docs/requirements/TRACEABILITY.md) and [`CHANGELOG.md`](CHANGELOG.md).
4. **Architecture Decisions (ADR):** Substantive design changes require an ADR in [`docs/adr/`](docs/adr/).
5. **Pre-Commit Green:** Ensure `make check` passes 100% locally before submitting the PR.

---

## 6. Code of Conduct

All contributors and maintainers are expected to follow our [Code of Conduct](CODE_OF_CONDUCT.md). Please report any violations to `rav.singh039@gmail.com`.

---

## 7. Security Disclosures

To report security vulnerabilities or invariant bypasses, please review our [Security Policy](SECURITY.md) and submit a report privately.
