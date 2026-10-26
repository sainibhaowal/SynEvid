# Mandatory Rules & Operational Protocol for AI Coding Agents

Every AI coding assistant working on **Synevid (Code Autopsy)** must strictly follow these rules without exception across the entire development lifecycle (Phases 0 through 6, v0.0.1 through 4.1, and the Tier-1–3 agent-experience, install-lifecycle & P0 release/plugin/forensic-trajectory build-out — see `docs/evidence/35`–`40`).

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
   - Go (Tier 2.2): `cgo`/`plugin`/`reflect` produce `Unknown`; `unsafe`/`//go:generate` produce `Partial`.
   - Rust (Tier 2.2): `include!` code-inclusion produces `Unknown`; `unsafe`/`dyn`/`env!` runtime behavior produces `Partial`.
   - Fully resolved AST declarations emit `CoverageState::Verified`.
   - Never collapse `UNKNOWN` or `PARTIAL` into `PASS` or `VERIFIED`.
6. **Ground Truth Integrity:**
   - Never modify benchmark ground truth datasets to inflate benchmark scores.
7. **Clean Dependency Inversion:**
   - Presentation, GUI (`apps/web-inspector`), MCP (`apps/mcp-server`), and LSP (`apps/synevid-lsp`) layers depend on core public interfaces, NEVER the reverse. Enforced by `ARCH_NO_CORE_TO_MCP` and the repo's own `.autopsy/invariants.yml`.
8. **Ambiguous Rename Rule (FR-007):**
   - If multiple files share identical content hashes or multiple symbols share a signature in the same scope, renames MUST stay `Added` + `Deleted` / `Added` + `Removed`. Never guess.
9. **Cross-Platform Path Determinism (INV-CROSS-001):**
   - Every relative path emitted in snapshots, multigraphs, or diffs MUST strictly use POSIX forward slash (`/`). Windows backslashes (`\`) MUST be normalized to forward slashes at scanner ingestion.
10. **Enterprise Sandboxing & Defense-in-Depth (INV-SANDBOX-001..004):**
   - Symlinks are NEVER followed (`follow_links(false)`). Symlink entries are rejected to prevent escaping workspace roots.
   - Normalized relative paths cannot contain `..` escape sequences or absolute prefixes.
   - Resource bounds (`max_traversal_nodes` for traversal, `max_file_bytes` per-file read cap with explicit `skipped_oversize_files` record) are enforced to prevent denial-of-service on adversarial cyclic graphs and multi-gigabyte files.
   - Zero code execution (in-memory AST parsing only, zero sub-processes).
   - Zero network socket requests (100% air-gapped, offline analysis).
11. **Permanent Zero-Major Versioning Invariant (`v0.xx.xxx`):**
    - SynEvid permanently operates under `v0.`. The version format is strictly `v0.<milestone>.<fix>` (where `v0` is the unchangeable super-major anchor, NEVER incrementing to `v1.0.0`; `<milestone>` ranges `1..99` for major architectural capabilities; and `<fix>` ranges `0..999` incrementing continuously with commits, bugfixes, and verified tests).

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
- Adhere to Rust edition 2024 and MSRV 1.90+ (measured: `let`-chains need 1.88, `tree-sitter-language@0.1.8` needs 1.90).

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
cargo test --workspace --all-targets -- --nocapture   # includes the synevid-lsp suite
cargo test --workspace --doc
./scripts/verify-determinism.sh
./scripts/verify-arch-boundaries.sh
bash scripts/tests/test-install.sh
```

#### B. Python (Benchmarks & Evaluation):
If Python scripts or benchmark tasks in `benchmarks/` are modified:
```bash
ruff check . --fix
ruff format --check .
bandit -r benchmarks/
pytest
```

#### C. TypeScript / Web / IDE Apps (MCP Server, Web Inspector & VS Code Extension):
If `apps/mcp-server`, `apps/web-inspector`, or `apps/vscode-extension` are touched:
```bash
npm run lint      # ESLint with zero errors (mcp-server)
npm test          # node:test — MCP server CLI integration tests, web-inspector headless
                  # render tests, and vscode-extension headless tests
npx tsc --noEmit  # Strict TypeScript compilation check (mcp-server)
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
- Evidence dossiers are numbered `01`–`40` (one record per phase, pillar, agent tier, or lifecycle closure). The canonical verification checklist is [`33-verification-procedures.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/33-verification-procedures.md); the agent-experience, install-lifecycle, and P0 release/plugin/forensic-trajectory records are [`35-tier1-agent-experience.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/35-tier1-agent-experience.md), [`36-tier2-agent-quality.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/36-tier2-agent-quality.md), [`37-tier3-polish.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/37-tier3-polish.md), [`38-agent-gate-closure.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/38-agent-gate-closure.md), [`39-install-update-lifecycle.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/39-install-update-lifecycle.md), and [`40-p0-release-plugin-and-forensic-trajectory.md`](file:///home/ravi/Projects/SynEvid/docs/evidence/40-p0-release-plugin-and-forensic-trajectory.md).
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
