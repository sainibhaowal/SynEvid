# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - Phase 4.2 Pillars 1–3 (2026-10-16)

Steps 28–37 of the 4.2 implementation sequence. Pillars 4–6 (Steps 38–45)
are **not** implemented. See `docs/evidence/17_phase4_2_pillar1_temporal.md`,
`docs/evidence/18_phase4_2_pillar2_test_impact.md`, and
`docs/evidence/19_phase4_2_pillar3_openapi.md`.

### Added
- **Global entity identity — RFC-GLOBAL-001 (`autopsy-domain::global_id`, Step 28):**
  `urn:autopsy:repo:<provider>:<org>:<repo>[#<path>[:<symbol-path>]]` as a total,
  canonicalizing parser. Provider host lowercased; org/repo case-preserved.
  Paths POSIX-normalized (leading `./` and trailing `/` stripped, `//`
  collapsed, `..` rejected). Round-trip is total: `parse(display(id)) == id`.
  Backslashes are **rejected rather than rewritten**, so one entity cannot hold
  two identities across platforms.
- **SQLite migration v2 (`autopsy-storage`, Step 29):** tables
  `temporal_facts`, `test_mappings`, `distributed_contracts`, `co_change_matrix`
  plus 8 indexes. Verified to upgrade a hand-built v1 database in place and to
  be idempotent across reopens. `schema_version()` accessor added.
  `created_at`/`window_*` are bookkeeping only and never reach a digest.
- **Read-only pure-Rust git object store (`autopsy-temporal::git`):** refs
  (`HEAD`, loose, `packed-refs`, symbolic chains, tag deref), loose objects with
  a 256 MiB inflate cap and declared-length cross-check, and packfile v2 with
  fanout-bounded index search plus `ofs-delta` **and** `ref-delta` resolution.
  This exists because `INV-SANDBOX-001..004` forbid sub-process execution: the
  engine cannot shell out to `git log`. Unsupported features (`.idx` v1,
  SHA-256 object format, alternates, linked worktrees) are explicit
  `GitError::Unsupported`, never silently misread.
- **Temporal fact journal (`autopsy-temporal::fact`, `::timeline`, Step 30):**
  append-only `TemporalFact` records with half-open `[valid_from, valid_to)`
  intervals. `fact_id = BLAKE3(subject ‖ predicate ‖ object ‖ valid_from)`,
  NUL-separated and deliberately independent of `valid_to`. Commit ordering is
  topological with object-id tie-breaking, never chronological.
- **Symbol lineage (`autopsy-temporal::lineage`, Step 31):** content-digest
  equality with a unique counterpart is the *only* path to a `Verified`
  rename/move; identical-multiplicity and similarity matches are
  `CoverageState::Partial` with the full candidate set disclosed. Similarity
  alone can never reach `Verified`, which is what makes the
  zero-false-positive-rename gate hold.
- **Point-in-time reconstruction (`autopsy-temporal::timeline`, Step 32):**
  `TemporalJournal::reconstruct(revision)` returns a `GraphSnapshot` whose
  digest is byte-identical to a fresh parse at that revision, at every revision.
  `SourceExtractor` is the extension point for adapter-supplied symbol facts.
- **Test inventory discovery (`autopsy-test-impact::inventory`, Step 33):**
  discovers Vitest, Jest, and Mocha suites from `*.test.*`, `*.spec.*`, and
  `test`/`tests`/`__tests__` directories, extracts declared test cases, and
  resolves relative imports to repository paths with extension probing and
  `index.<ext>` fallback. Test-file detection is **segment-based**, so
  `src/latest.ts` is not mistaken for a test. Dynamic `import(...)`,
  `require(...)`, and template-literal specifiers are deliberately not guessed
  at. A computed test name is addressable by position (`@line<N>`) rather than
  an invented name.
- **Observed coverage ingestion (`autopsy-test-impact::lcov`, Step 34):**
  parses LCOV tracefiles and keeps **zero hits** strictly distinct from **no
  record** — a file absent from every tracefile is evidence that nothing was
  recorded, not that it is untested. Unrecognised records (`FN`, `FNDA`,
  `BRDA`, `LF`, `LH`, `TN`) are skipped; anything uninterpretable is an error,
  never a silent drop. Merging takes the worst coverage across inputs.
- **Conservative test selection (`autopsy-test-impact::selection`, Step 35):**
  `AffectedTests = ⋃ ReverseReachability(changed symbols) ∩ inventory`. The
  0%-false-negative gate holds by construction: (a) any changed symbol with
  `Unknown` semantics forces `RunAllTests`; (b) a suite whose mapping is not
  `Verified` **cannot be ruled out**, so it is included rather than excluded;
  (c) a change set with no positively-mapped suite forces `RunAllTests`, because
  "no test covers this" and "we failed to map this" are indistinguishable from
  inside the engine. When coverage data is missing but static imports still
  narrow, the plan falls back to `RunPackageTests` instead.
- **Advisory predictive ranking (`PredictiveRanking`, Step 36):**
  ranks selected tests by `observed_affected_sources / total_affected_sources`.
  The plan is a *separate* artifact and `select_affected_tests` takes no ranking
  input, so no probabilistic score can reach a merge gate — the types enforce
  this rather than a comment. Per the pillar plan, the sidecar is simulation and
  presentation only and must never block a merge.
- **`schemas/test-selection.schema.json`:** closed (`additionalProperties:
  false`) schema for the selection plan, plus a conformance test suite that
  cross-checks the schema against real serialised plans.
- **OpenAPI 3.2.1 contract engine (`autopsy-contracts-distributed`, Step 37):**
  parses `paths`, operations, parameters, request bodies, and responses into a
  canonical `BTreeMap`-ordered model with eager local `$ref` resolution, and
  classifies compatibility between two revisions. The verdict algebra is the
  point: **`breaking` is asserted from a positive finding and therefore does not
  depend on coverage**, while `compatible` is a claim that nothing was missed
  and is therefore asserted only over fully-modelled documents. Everything in
  between is `unknown`, which is never an approval —
  `CompatibilityVerdict::is_approval()` exists so that no consumer writes
  `verdict != Breaking` by hand and silently promotes `unknown` to a pass.
  Direction is respected: request-side narrowing is breaking, and where no
  directional evidence exists (a changed *response* schema) the change is
  reported as unclassified rather than guessed at.
- **`schemas/distributed-contract.schema.json`:** closed schema for the delta,
  with a conformance suite that enumerates the Rust enums exhaustively via
  `ChangeKind::ALL` and compares them against the schema through `serde`, so an
  added variant or a `#[serde(rename)]` cannot drift unnoticed.

### Fixed
- Temporal replay no longer mints a duplicate fact on re-observation. Because
  `valid_from` is part of the digest while a continuing assertion keeps its
  original `valid_from`, the first implementation created a new fact at every
  subsequent commit; the fixture exposed 9 facts where 5 were correct.
- Removed an item-invariant filter in the lineage similarity pass that could
  only ever match all candidates or none.
- `Document::pointer` refused array-index descent, so every legal JSON Pointer
  addressing a list element — including the extremely common
  `#/paths/.../parameters/0/schema` — failed to resolve, which silently made the
  whole document `Unknown`. Now follows RFC 6901 in full: the token's meaning is
  decided by the value it is applied to, so `0` is an index under an array and
  the key `"0"` under an object. Leading zeros and the append token `-` are
  refused. Found by the test suite, not by inspection.
- Resolved `$ref` nodes had their child pointers computed from the *reference
  site* rather than from where the node was found. A
  `{"$ref": "#/components/parameters/Q"}` parameter has no `/schema` child of its
  own, so the lookup always missed and the contract was permanently `Unknown`.
  `resolve_shallow` now returns the resolved pointer alongside the resolved
  value.
- A parameter the parser rejected (for example a path parameter absent from its
  route) was dropped with a note but without degrading coverage, so a document
  containing an unreadable parameter could still claim `Verified`. Dropped
  parameters now degrade coverage, because an unread parameter is exactly the
  kind of gap that must not read as completeness.
- Adding an *optional* request body was classified as breaking. Severity depends
  on `required`, so the kind must too; an added optional body rejects nothing
  that existing callers relied on. Caught by clippy after removing a branch that
  did nothing.
- A forced full run now emits the mandatory `plan_summary` reason. Previously
  only the narrowed path emitted it, so a consumer reporting the narrowing ratio
  got nothing on exactly the runs where "we narrowed nothing" matters most. The
  schema-conformance suite caught this.

### Notes
- New dependencies `flate2` (rust_backend, for cross-platform-identical
  decompression) and `sha1`; both MIT/Apache-2.0 and within the existing
  `deny.toml` allow-list.
- The git hash implementation is anchored to published git constants (empty
  tree `4b825dc6...`, `"hello world\n"` blob `3b18e512...`) and was validated
  end-to-end against a real `git gc --aggressive` repository.
- Symbol-level temporal extraction is **not yet wired to the language
  adapters**; only the built-in `TreeExtractor` (file presence) runs in
  production paths today.
- `schemas/temporal.schema.json` and CLI surfacing of the journal are not yet
  provided; the journal is a library API only.
- `autopsy-test-impact` performs a **lexical** scan of test files rather than an
  AST parse, because it sits below the language adapters in the dependency DAG.
  The consequence is stated rather than hidden behind a confidence score:
  constructs the scanner cannot interpret degrade the suite's coverage and force
  a fallback. A helper file that contains neither a framework import nor any
  test declaration is classified `Unknown` and therefore always runs; this is
  deliberately noisy in exchange for never silently excluding a suite we did not
  understand.
- `autopsy-contracts-distributed` adds **no new external dependency**:
  `serde_yaml` is already a production dependency of `autopsy-invariants` and
  `autopsy-repo`, so no new supply-chain edge is introduced. The pre-existing
  `serde_yaml` advisory (`RUSTSEC-2024-0449`) is unchanged by this work.
- **YAML merge keys (`<<:`) are rejected outright** rather than parsed.
  `serde_yaml` does not apply them, so a document using them parses with every
  inherited field silently absent — and a compatibility checker would then
  report those fields as *removed*, producing a confident false positive. The
  detection is line-based and can therefore refuse a document that merely
  contains `<<:` inside a block scalar. That asymmetry is deliberate: refusing
  to analyse is recoverable, a confident wrong answer is not.
- JSON Schema composition (`allOf`, `oneOf`, `anyOf`, `not`, `if`/`then`/`else`)
  is recorded but **not interpreted**. Every occurrence is named by JSON pointer
  and degrades that subtree, so a composed schema cannot yield a `compatible`
  verdict. This is deliberately pessimistic: almost every real specification
  uses composition somewhere, and pretending to model it would produce the worst
  possible answer for a compatibility gate.
- Response-body schema changes are reported with severity `unknown`, not
  `breaking` and not `non_breaking`. Request-side narrowing rules do not apply to
  responses, and this engine holds no directional evidence for them, so it says
  so instead of guessing.
- `autopsy-contracts-distributed` is a **library API only**. Steps 38 (AsyncAPI
  3.1.0) and 39 (Pact) are not implemented; no CLI subcommand, storage
  write-back to `distributed_contracts`, or MCP surface is wired.
- `autopsy-test-impact` is a **library API only**. No CLI subcommand, storage
  write-back to `test_mappings`, or MCP surface is wired yet.
- `Cargo.toml` version remains `0.0.1`.

## [Unreleased] - Forensic closure (2026-10-09)

### Fixed
- **Real two-state `diff`/`verify` (FR-007, FR-010):** `baseline` now persists
  canonical per-file bytes to the content-addressed cache; `diff --before`
  and `verify --baseline-id` rebuild the genuine before-state graph and
  contracts instead of comparing a state against itself. Unknown baseline
  ids are hard errors; api-compat without a baseline reports `Unknown`,
  never a vacuous pass.
- **Symbol identity/shape split (ADR-0009, FR-005):** `autopsy-symbols`
  implemented as the identity authority; identities no longer embed
  signature hashes, so signature edits surface as `SignatureModified` +
  breaking contract deltas instead of delete+create pairs.
- **TypeScript call graph (FR-008):** `Calls` edges (local, `this.`,
  `new`, import-resolved, typed-receiver DI/annotated params), module-scope
  variable/enum/arrow symbols, overload + interface-member symbols; fixed
  dangling hierarchy edge sources. New `impact --edge-kinds` operator
  precision control and bare file-path seeds.
- **Benchmark honesty (FR-025):** Baseline-C executes the real engine
  binary per task; prior 100%/100% precision figures withdrawn (simulated).
  Re-measured gate: recall +31.73pp PASS; cost/failure-class FAILs disclosed
  (see `docs/evidence/13_forensic_closure_verification.md`).
- **Web inspector:** zero hardcoded evidence; renders live artifacts only
  (verified headlessly against real `impact` + `verify` output).
- **Pillar C resource bound (3.2):** new `analysis.max_file_bytes`
  (default 10 MiB): the scanner gates on metadata size *before* reading,
  oversized files are excluded and recorded in
  `RepoSnapshot.skipped_oversize_files` with a CLI diagnostic (never
  silent); old snapshot JSON without the field still loads.
- **Honest stored-state addressing:** `diff --after <snapshot-id>` and
  `impact --snapshot-id <snapshot-id>` analyze the stored state rebuilt
  from cache (previously `--after` was a revision label and
  `--snapshot-id` was ignored); unknown impact snapshots are hard errors.
  MCP `head`/`after` descriptions corrected to snapshot IDs only.
- **Landscape parity:** one-line `scripts/install.sh` (prebuilt release →
  source-build fallback, host wiring opt-in); `scripts/setup-agent-hosts.py`
  auto-wires 7 agent hosts (Claude Code/Desktop, Cursor, Windsurf, VS Code,
  opencode, Gemini CLI) backup-first and idempotently, `--hooks` adds an
  advisory fail-open safe-edit pre-edit prompt; **Python adapter**
  (`autopsy-adapter-python`): symbols/contracts/calls incl. sibling-import,
  `self.`/`super()`/typed-receiver resolution + honest coverage; agent skill
  + settings example; `docs/COMPARISON.md` landscape record with
  vendor-reported claims labelled.
- Fixed doubled-colon rendering in normalized signatures
  (`f(): : T` → `f(): T`).
- **Precision-first impact (FR-008):** ranked top-k shortlist
  (`--top-k`, `ranked_top` + `total_impacted` in reports/schema, full set
  preserved for evidence); `--profile callers|full` presets (caller
  invocation closure, file-node transit suppression; `full` default
  unchanged); `ImpactProfile.traverse_file_nodes` engine flag.
- **Incremental baseline (FR-020, early):** `baseline --incremental
  --baseline-id` reuses content-hashed per-file artifacts for unchanged
  files (no parse), with global symbol-set soundness gate and full-analysis
  fallback; write-through artifact cache on every baseline.
- **PR forensic gate:** `.github/workflows/autopsy-pr-gate.yml` (verify
  gates merge, per-file callers impact, SARIF + JSON artifacts, PR
  comment); `scripts/forensic-case.sh` design-partner case bundles.
- **4.1 gap closure:** invalid `autopsy.toml` now fails before analysis
  (was silent fallback); `symbols`/`edges` paginated graph-browsing CLI
  commands + MCP resource templates (full dumps intentionally
  unavailable); MCP precision params (`profile`, `top-k`, `edge-kinds`),
  dead `profile` param removed; every public schema field documented;
  `cargo-deny` 0.20 policy enforced in CI; ESLint + ruff + pytest smoke
  enforced; nightly perf-tier tripwire; CycloneDX SBOM in releases.

## [0.1.0] - 2026-10-08

### Added
- **In-Session Agent Lifecycle, TypeScript MCP Server & Web Inspector (Phase 4.1):**
  - **Model Context Protocol (MCP) Server (`apps/mcp-server`):**
    - Out-of-process TypeScript server implementing official MCP SDK v2 (`@modelcontextprotocol/server` v2.3.1).
    - 5 mandatory read-only tools: `autopsy_status`, `autopsy_impact`, `autopsy_diff`, `autopsy_verify`, and `autopsy_explain`.
    - Automated Node.js integration tests asserting stdio CLI subprocess execution, tool registration, and typed parameter validation.
    - Verified architectural inversion (`ARCH_NO_CORE_TO_MCP`): core Rust crates maintain 0 dependencies on MCP or web layers.
  - **Web Inspector Evidence Board (`apps/web-inspector`):**
    - Standalone, zero-backend, read-only single-page web app for interactive forensic inspection.
    - Interactive Evidence Board visualizing hypotheses, supporting evidence cards with file/line provenance, and contradicting signal checks.
    - Transitive Blast Radius and dependency multigraph explorer showing caller hierarchy and downstream API risk.
    - Drag-and-drop local `autopsy.json` artifact loader for instant telemetry analysis.
  - **In-Session Agent Lifecycle Protocol (`tests/tests/e2e_agent_lifecycle.rs`):**
    - Integration test suite (3 tests) validating the closed-loop agent workflow: `Baseline A -> Impact Assessment -> Edit A to B -> Verify A vs B -> (On FAIL: Repair via Evidence -> Re-verify PASS)`.
    - Detection of breaking contract parameter weakening and architectural boundary violations.
  - **Baseline Debt Management & Waivers (`.autopsy/baseline.json`, CA-TECH-001 Ch.8):**
    - Schema validation via committed `schemas/baseline.schema.json` and `.autopsy/baseline.json`.
    - Verification engine support for `--baseline-id` and accepted debt fingerprints, distinguishing pre-existing legacy violations from new PR regressions.
    - Integration test `test_cli_verify_baseline_accepted_debt_waiver` verifying waiver handling and exit code conversion.
    - Added TypeScript MCP server typechecking and unit test execution into `.pre-commit-config.yaml` and `.github/workflows/ci.yml`.
- **Multi-Platform Hardening & Enterprise CI/CD (Phase 3.2):**
  - Universal hardware & architecture support: Linux (`x86_64`, `aarch64`), macOS Apple Silicon (`aarch64-apple-darwin`), macOS Intel (`x86_64-apple-darwin`), and Windows (`x86_64-pc-windows-msvc`).
  - Cross-platform path separator normalization (`/`) in `autopsy-repo` scanner, ensuring byte-identical BLAKE3 digests and snapshot IDs across Linux, macOS, and Windows.
  - Cross-platform CRLF vs LF line-ending normalization (`normalize_line_endings`) in `autopsy-repo`, guaranteeing byte-identical content hashes regardless of git checkout line endings.
  - Concurrent atomic cache write resilience in `autopsy-storage` with PID-isolated staging and collision-safe rename handling on Windows NTFS and POSIX filesystems.
  - Integration test suite `e2e_cross_platform_and_multi_arch.rs` (4 tests) asserting CRLF/LF invariance, path normalization, concurrent cache writes, and BLAKE3 boundary vectors.
  - Dedicated verification script `scripts/verify-cross-platform.sh` integrated into `Makefile` and `pre-commit` hooks.
  - Upgraded GitHub Actions CI/CD suite:
    - Pre-merge multi-platform matrix (`.github/workflows/ci.yml`) across Ubuntu, macOS (Apple Silicon), and Windows runners with concurrency cancellation (`cancel-in-progress: true`), 93 tests, and rich GITHUB_STEP_SUMMARY quality gate tables.
    - Post-merge release pipeline (`.github/workflows/post-merge.yml`) executing serially on `main` (`cancel-in-progress: false`), verifying golden baselines, compiling native release binaries across Linux (`x86_64`), macOS (`aarch64`, `x86_64`), and Windows (`x86_64`), packaging archives with licensing metadata, computing `SHA256SUMS.txt`, and uploading consolidated release bundles.
  - **Enterprise Sandboxing & Defense-in-Depth (Pillar C):**
    - Zero Code Execution (FR-026): In-memory AST parsing only, zero sub-process execution (`std::process::Command` = 0, `fork`/`exec` = 0), verified via architecture boundary gate.
    - Symlink Escape Containment (INV-SANDBOX-001): Explicit `follow_links(false)` on `WalkBuilder`, symlink entry filtering (`entry.path_is_symlink()`), and canonical root boundary checks.
    - Path Traversal Sanitization (INV-SANDBOX-003): Function `sanitize_relative_path` and `is_safe_relative_path` rejecting `..` traversal sequences and absolute paths across file scanning, `roots`, and `cache.directory`.
    - Resource Bounds & DOS Prevention (FR-008): Enforced `max_traversal_nodes` and impact budget ceilings guaranteeing linear termination on adversarial dense cyclic graphs.
    - Zero-Network Isolation (FR-028, INV-SANDBOX-002): Verified 100% offline analysis with zero networking crates and zero socket calls.
    - Integration test suite `tests/tests/e2e_enterprise_sandboxing_and_defense.rs` (5 tests) bringing total workspace test count to 100 tests.
  - Phase 3.2 Master Plan and Engineering Blueprints (`Resource/3.2/`).
- **Community & Governance Infrastructure:**
  - `SECURITY.md` establishing vulnerability disclosure protocol, response timelines, and core security invariants (Zero LLM, Read-Only, Offline-first).
  - `NOTICE` adhering to Apache-2.0 copyright and third-party attribution specifications.
  - `CODE_OF_CONDUCT.md` adopting Contributor Covenant v2.1.
  - Comprehensive `CONTRIBUTING.md` defining deterministic verification protocol, non-negotiable hard boundaries, and pull request procedures.
  - GitHub issue templates (`.github/ISSUE_TEMPLATE/`) for bug reports, feature requests, invariant violation alerts, and security guidance.
  - GitHub pull request template (`.github/PULL_REQUEST_TEMPLATE.md`) with determinism and architecture checklist.
- **Language Adapter Architecture (Phase 2):**
  - `autopsy-adapter-api`: Versioned `LanguageAdapter` trait, `AdapterCapabilities`, `ParsedFile`, `DynamicConstruct`, and reusable conformance test suite `verify_adapter_conformance`.
  - `autopsy-adapter-typescript`: Production-grade tree-sitter bootstrap (TS & TSX), TypeScript compiler bridge for relative and package module resolution, stable `SymbolId` calculation resilient to line shifts, typed multigraph edge extraction (`Contains`, `Imports`, `Inherits`, `Implements`), normalized callable and interface contract extraction, and honest coverage state emission (`Verified`, `Partial`, `Unknown`).
  - E2E integration test suite (`tests/tests/e2e_adapter_typescript_conformance.rs`) verifying conformance, real multi-file repository parsing, cross-file imports, and 100-run determinism.
- **Dependency Multigraph & Semantic Diff Engine (Phase 3):**
  - `autopsy-graph`: Petgraph-backed directed multigraph, edge provenance tracking (adapter, location, method), Tarjan SCC cycle detection, bounded BFS reachability (forward and backward), incremental file updates ($O(|V_{file}| + |E_{file}|)$), and BLAKE3 graph digest hashing.
  - `autopsy-diff`: Semantic diff engine with file deltas (add, del, modify, rename), symbol deltas (add, remove, move, rename, body, signature, visibility, kind), contract deltas with breaking change detection, edge deltas with coverage state tracking, changed frontier calculation, and strict Ambiguous Rename Rule enforcement.
  - E2E conformance test suite (`tests/tests/e2e_graph_and_diff_conformance.rs`) covering cycles, reachability, ambiguous rename safety, golden diff fixtures, and 100-run determinism.
- **Impact, Contracts, Invariants & Evidence Engine (Phase 4):**
  - `autopsy-impact`: Bounded forward/backward BFS graph traversal, traversal profiles (`edge_kinds`, `max_depth`, `budget`), deterministic shortest-path ranking, SCC cycle condensation, budget truncation tracking, and BLAKE3 impact digests.
  - `autopsy-contracts`: Normalized callable and interface contract models for TypeScript, formal backward compatibility rules per kind, breaking change detection, and cryptographic contract digests.
  - `autopsy-invariants`: YAML invariant policy loader (`.autopsy/invariants.yml`), deterministic evaluators for forbidden dependencies (`forbidden-dep`), layered architecture constraints (`layers`), cycle introduction prevention (`no-new-cycle`), and API backward compatibility (`api-compat`), strictly enforcing the `UNKNOWN != PASS` boundary.
  - `autopsy-evidence`: Immutable evidence receipts with canonical ordering of locations, paths, snapshots, and deltas before computing BLAKE3 digests, providing complete tamper verification.
  - E2E integration test suite (`tests/tests/e2e_phase4_impact_and_invariants.rs`) asserting golden fixtures, 100-run determinism, and `UNKNOWN != PASS` invariance.
- **CLI, Storage & Reporting Engine (Phase 5):**
  - `autopsy-storage`: Embedded SQLite storage with versioned auto-migrations, thread-safe connection pooling, optional WAL mode, and content-addressed `.autopsy/cache/objects/xx/yy...` with atomic write-and-rename guarantees.
  - `autopsy-report`: Reporting engine with canonical indented JSON strictly matching `schemas/autopsy-result.schema.json v0.0.1`, formatted human-readable terminal text with line-span badges, and OASIS SARIF 2.1.0 output for CI / GitHub code scanning.
  - `autopsy-cli`: Unified binary CLI providing commands `baseline`, `diff`, `impact`, `verify`, `explain`, `doctor`, and `version` with pure machine-readable `stdout`, diagnostic `stderr`, deterministic exit codes (0 pass, 2 policy failure, 3 analysis error, 4 unsupported/unknown), and offline network-off execution guarantee.
  - E2E integration test suite (`tests/tests/e2e_phase5_cli_and_storage.rs`) asserting all 7 CLI commands, exit code mapping, storage roundtrips, offline isolation, and 100-run determinism.
- **Benchmark Harness & Evaluation Suite (Phase 6):**
  - Standardized benchmark corpus with 5 TypeScript repositories (`benchmarks/corpus/`: `repo-api`, `repo-migration`, `repo-relayer`, `repo-cross-package`, `repo-dead-call`).
  - 50 deterministic benchmark tasks (`benchmarks/tasks/`: `task_001.json` - `task_050.json`) spanning 5 categories (`api_change`, `migration`, `relayer`, `cross_package`, `dead_call`) strictly conforming to `schemas/benchmark-task.schema.json`, including partitioned held-out validation tasks.
  - Three comparative baselines: Baseline A (agent + grep text search), Baseline B (agent + LSP 1-hop AST references), and Baseline C (agent + Autopsy deterministic multigraph reachability).
  - Benchmark evaluation engine (`benchmarks/engine/`) calculating recall, precision, F1, tool calls, tokens, latency, and structural failure class discovery.
  - Pre-registered gate verification (01 Ch.12): **PASSED** with +31.73pp recall lift (threshold $\ge +10\text{pp}$), 56.90% cost reduction (threshold $\ge 40\%$), and 16 newly discovered structural failure classes.
  - Automated reproduction report generation (`benchmarks/results/reproduction_report.json` and `reproduction_report.md`).
  - E2E integration test suite (`tests/tests/e2e_phase6_benchmarks.rs`) asserting corpus integrity, task schema conformance, and gate passage.
- **Engine Core Verification Architecture:**
  - 14 Rust crates implementing foundation pipeline (`autopsy-repo`, `autopsy-domain`, `autopsy-symbols`, `autopsy-graph`, `autopsy-diff`, `autopsy-impact`, `autopsy-invariants`, `autopsy-evidence`, `autopsy-storage`, `autopsy-contracts`, `autopsy-report`, `autopsy-cli`).
  - Cryptographic BLAKE3 100-run snapshot determinism invariant test suite.
  - Strict architectural boundary verification script enforcing Zero LLM in correctness crates.

## [0.0.1] - 2026-09-29
- Repository architecture and public schema starters.
- Foundation schema contracts and ADR records.
- **Measured competitor bake-off:** seven rivals cloned, built and run on
  identical fixtures (`docs/COMPARISON.md`). Found and fixed a silent Python
  recall gap — inline construction chains (`Calculator().run(...)`) now
  resolve as `call_expression:chained-constructor`, with regression test
  (161 → 162 tests). Only SynEvid blocks a breaking change by default
  (exit 2); ripwire and blastcode detect but exit 0. Logged the missing
  test-coverage axis (`radius_untested`) as the top parity gap.
