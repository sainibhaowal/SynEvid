# 3.1 Appendix — Full FR/NFR/SEC/UC/RQ Coverage (gap fill, source 02 CA-REQ-001 + 01 CA-FOUND-001)

This file duplicates every normative ID verbatim-grouped so `docs/3.1/` is 100% line-complete. Source PDFs remain truth on conflict. Wording rule: shall=must, should=desire, may=option.

## 1. UC-01..09 (02 Ch.6)
- UC-01 Create baseline — Developer/CI — snapshot+index with reproducible digest.
- UC-02 Impact of symbol/package — Developer/agent — bounded evidence-linked set.
- UC-03 Analyze git diff — Developer/agent — semantic change set.
- UC-04 Verify invariants — CI/agent — pass/fail with rule/evidence.
- UC-05 Detect contract changes — Reviewer/agent — changed contracts + consumers.
- UC-06 Export findings — CI — JSON/SARIF conforms to schemas.
- UC-07 Repair loop — Agent — consume failure, repair, re-verify to pass.
- UC-08 Benchmark task — Researcher — same task vs baselines + metrics.
- UC-09 Policy configuration — Architect/platform — versioned invariants validated before use.

## 2. FR-001..030 (02 Ch.7 — all Must unless noted)
- FR-001 Repository discovery — shall analyze local Git worktree/path without source upload. Must v0.0.1. Verify: Test.
- FR-002 Snapshot identity — shall assign reproducible identity from state+config+analyzer versions. Must v0.0.1. Test.
- FR-003 Adapter contract — core shall expose versioned adapter interface for parse/symbol/reference/contract. Must v0.0.1. Inspection/Test.
- FR-004 Syntax parsing — first adapter shall parse supported files and report unsupported/invalid explicitly. Must v0.0.1. Test.
- FR-005 Stable symbol IDs — shall create stable IDs resilient to line movement where semantics permit. Must v0.0.1. Benchmark/Test.
- FR-006 Reference graph — shall represent typed edges contains, imports/depends-on, references/calls minimum. Must v0.0.1. Test.
- FR-007 Semantic diff — shall classify added/removed/moved/renamed/changed symbols and signature/contract changes where detectable. Must v0.0.1. Test.
- FR-008 Impact query — shall compute bounded transitive impact over edge types/directions with path evidence. Must v0.0.1. Test/Benchmark.
- FR-009 Contract extraction — adapter shall emit normalized callable/interface contracts supported by language. Must v0.0.1. Test.
- FR-010 Contract delta — shall identify contract changes + affected consumers between snapshots. Must v0.0.1. Test/Benchmark.
- FR-011 Invariant policy — shall load versioned invariants from repo config. Must v0.0.1. Test.
- FR-012 Invariant evaluation — shall deterministically evaluate to pass/fail/not-evaluable. Must v0.0.1. Test.
- FR-013 Evidence receipt — every finding shall include rule/query ID, snapshots, analyzer version, locations, paths, digest. Must v0.0.1. Schema/Test.
- FR-014 Coverage state — findings shall distinguish verified/unsupported/unknown; absence shall not be proof of safety. Must v0.0.1. Test.
- FR-015 CLI — shall provide baseline, diff, impact, verify, explain, doctor, version. Must v0.0.1. CLI test.
- FR-016 Exit codes — verify shall use stable codes for pass, policy-failure, analysis-error, unsupported. Must v0.0.1. Test. Mapping: 0/2/3/4.
- FR-017 JSON output — every non-interactive command shall support versioned JSON. Must v0.0.1. Schema test.
- FR-018 SARIF output — findings suitable for scanning shall be exportable as SARIF 2.1.0 [R16]. Should v0.1. Schema test.
- FR-019 MCP tool interface — MCP server shall expose read-only query+verify tools per current MCP revision [R19][R20]. Must v0.2. Contract test.
- FR-020 Incremental analysis — shall avoid reparse/reindex when hashes+config unchanged. Must v0.1. Performance test.
- FR-021 Git diff modes — shall analyze commit-to-commit, merge-base-to-worktree, staged/unstaged. Must v0.1. Integration test.
- FR-022 Policy baseline mode — teams shall distinguish new violations from accepted debt. Should v0.2. Test.
- FR-023 CI artifact — shall write immutable artifact with canonical digest. Should v0.2. Integration test.
- FR-024 Plugin metadata — adapters/reporters shall declare versions/capabilities; incompatible schema fails explicitly. Must v0.1. Contract test.
- FR-025 Benchmark harness — repo shall include reproducible harness computing recall, precision, time, cost, tool calls. Must v0.0.1. Reproduction.
- FR-026 No auto-write core — core analysis commands shall not modify target source. Must v0.0.1. Security test.
- FR-027 Explain path — shall explain why entity is in impact set via exact graph paths. Must v0.1. Test.
- FR-028 Deterministic normalization — shall canonicalize ordering/hashing so identical inputs give identical results. Must v0.0.1. Repeatability test.
- FR-029 Config validation — invalid policy/config shall fail before analysis with location-aware diagnostics. Must v0.0.1. Test.
- FR-030 Adapter fallback — unsupported constructs shall be marked unknown/partial, never silently approximated. Must v0.0.1. Test.

## 3. NFR-001..018 (02 Ch.8)
- NFR-001 Determinism: same snapshot/config/versions -> same digest. 100/100 fixture runs identical.
- NFR-002 Precision/recall visibility: publish methodology, never collapse unknown into pass. Report per-task P/R/unsupported.
- NFR-003 Incremental latency: PR-scale CI. Target p95 <=60s designated tier.
- NFR-004 Warm query: interactive. Target p95 <=2s designated tier.
- NFR-005 Scalability: no full O(V^2) on incremental. Regression suite by tier.
- NFR-006 Memory safety: Rust, clippy/audit, unsafe documented+reviewed.
- NFR-007 Source privacy: local by default. Network-off test succeeds.
- NFR-008 Network independence: no internet/model API for core. Offline CI test.
- NFR-009 Least privilege: read repo, write cache/output only. Threat-model verified.
- NFR-010 Auditability: provenance to reconstruct inputs/version/config. Evidence schema.
- NFR-011 Interoperability: schema-versioned, SARIF where apt [R16]. Compat tests.
- NFR-012 Observability: structured logs + optional OTel [R18]. Telemetry test.
- NFR-013 Portability: Linux first, macOS next, Windows/WSL assessed before v0.3.
- NFR-014 Reproducible builds: versioned CI + provenance, SLSA concepts [R21].
- NFR-015 Maintainability: depend on domain interfaces, not CLI/MCP layers. Arch tests.
- NFR-016 Testability: deterministic pure/controlled-I/O core. Unit/property/golden.
- NFR-017 Usability: default text answers what-failed/why/where/affected/unknown without GUI. Task study.
- NFR-018 Backward compat: semver + changelog + contract tests.

## 4. SEC-001..010 (02 Ch.10, NIST SSDF [R17] input)
- SEC-001 Untrusted repo: parsers/external tools bounded resources.
- SEC-002 No source leaves execution env by default.
- SEC-003 MCP read-only on source in foundation.
- SEC-004 External compiler/LSP: allowlist + timeouts + captured output.
- SEC-005 Cache resists traversal/symlink escape.
- SEC-006 Logs/results avoid secrets/source bodies unless requested.
- SEC-007 Plugin loading pinned/versioned, no remote exec by default.
- SEC-008 CI artifacts carry provenance/config hash, exclude source unless configured.
- SEC-009 DoS caps: recursion, traversal, file size, time, output volume.
- SEC-010 External findings ingested with source/tool identity retained.

## 5. Elicitation Q17-26 + saturation (02 Ch.5.1-5.2)
Q17 last unsure-break change; Q18 how affected code found + tools; Q19 what took longest; Q20 what missed + how found; Q21 which changes need seniors; Q22 unwritten arch rules; Q23 what evidence approves agent change; Q24 what makes report useless; Q25 where code may be processed; Q26 which PR/CI outputs must integrate. Target 12-20 interviews, 3+ segments, >=5 walkthroughs, stop at saturation.

## 6. Prototypes P0-P5, verification levels, releases, risks (02 Ch.11-15)
P0 benchmark notebook (evolve), P1 CLI spike (review), P2 diff engine (evolve), P3 invariant DSL (iterate), P4 MCP bridge (evolve), P5 HTML inspector (throwaway until proven). Levels: Unit/Contract/Integration/System/Benchmark/User/Security/Performance per plan. Releases: v0.0.1 FR-001..017+025..030; v0.1 incremental+SARIF+explain+bench; v0.2 MCP+baseline+CI artifact; v0.3 CI/PR+HTML; v0.4+ cross-repo. Open Qs: first-language choice, stable IDs under rename, static contract soundness, edge-type signal, unknown representation, DSL simplicity, runtime evidence role, paid tasks before SaaS.

## 7. 01 gates/metrics (CA-FOUND-001 Ch.9-12)
RQs: RQ1 impact discovery (recall/precision), RQ2 search cost (tool calls/tokens/time), RQ3 actionability (time-to-understand/usefulness), RQ4 determinism (100-run digest), RQ5 contract/invariant misses (TP/FP/FN). Corpus >=5 repos, >=50 tasks, families API/migration/relayer/schema/cross-package/dead-call, blind. Gates: +10pp recall OR -40% cost at equal accuracy; precision >=80% actionable; 100/100 digest; p95 <=60s PR. Kill on: agent+LSP parity, vendor parity, noisy static, no pain, language cost, vendor absorption.
