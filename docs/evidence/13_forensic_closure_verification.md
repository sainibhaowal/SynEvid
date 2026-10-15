# Evidence 08 — Forensic Closure Verification (2026-10-09)

End-to-end remediation of five defects that kept the engine from being a
real forensic lab, verified live on real data (no simulated baselines).

## 1. Defects fixed

| # | Defect | Fix | Proof |
|---|--------|-----|-------|
| 1 | `handle_diff` compared after-state against itself (`before_graph = after.graph`) | `baseline` persists canonical per-file bytes to CAS; `diff` rebuilds the genuine before-graph/contracts via `build_artifacts_from_snapshot` | `test_signature_edit_yields_signature_modified_and_breaking_contract` |
| 2 | `handle_verify` evaluated api-compat against an empty before-map; unknown `--baseline-id` silently fell back | Real before-contracts from CAS; unknown ids are hard errors; empty before-map yields `Unknown` (never vacuous `Pass`) | `test_verify_baseline_id_catches_breaking_change`, `test_verify_baseline_id_passes_non_breaking_change`, `test_verify_unknown_baseline_id_is_hard_error`, `test_api_compat_without_baseline_is_unknown_not_pass` |
| 3 | `autopsy-symbols` was a 4-line stub; identities embedded signature hashes (ADR-0009) | Implemented identity authority (identity/shape split, overload disambiguation, format validation) | 7 unit tests + adapter identity tests |
| 4 | TS adapter emitted no `Calls` edges, no variable/enum/arrow symbols, hierarchy edges dangled (sighash-less source) | `Calls` edges (local / `this.` / `new` / imported-resolved / typed-receiver DI + annotated params), module-scope variables/enums/arrow bindings, `function_signature` + `method_signature` overload/interface members, fixed hierarchy source ids | 19 adapter unit tests |
| 5 | Web inspector shipped hardcoded digests and demo case content | Zero hardcoded evidence; empty state with exact CLI commands; renders findings/evidence/blast/ranked paths from loaded artifacts only | Headless render run against real `impact` + `verify` artifacts (see §3) |
| 6 | `diff --after` was a revision label and `impact --snapshot-id` was silently ignored (false stored-state addressing; MCP claimed "git revision" support) | Both resolve stored snapshots via CAS rebuild; unknown impact snapshots are hard errors; MCP descriptions corrected to snapshot IDs | `test_diff_two_stored_snapshots_compares_states`, `test_impact_stored_snapshot_id_analyzes_that_state`, live stored A→B diff with worktree restored |
| 7 | Phase 3.2 gaps: release pipeline copied a nonexistent `synevid` binary; nightly benchmark built release but harnessed debug; no per-file byte cap (unbounded reads = DoS vector vs Pillar C) | `post-merge.yml` corrected to `autopsy`/`autopsy.exe`; `benchmarks.yml` passes `--autopsy-bin target/release/autopsy`; new `analysis.max_file_bytes` (default 10 MiB, serde-defaulted, hashed, validated) with size gate before read + `skipped_oversize_files` record + CLI diagnostic | `test_max_file_bytes_default_and_validation`, `test_oversize_files_excluded_and_reported`, `test_e2e_oversize_file_cap_never_reads_huge_files`, old-snapshot JSON back-compat proven |

Additionally: Baseline-C of the benchmark harness executed the real
binary per task instead of copying ground truth into predictions
(`benchmarks/engine/baseline_autopsy.py` rewritten; prior 100%/100%
scores were gate-gaming by construction and are withdrawn).

## 2. Live-fire transcript (real corpus repo, `benchmarks/corpus/repo-api`)

```
baseline   -> snap 6f9a3e33… coverage verified (7 files)
impact -s …UserService::createUser --direction backward --edge-kinds calls
  - src/controllers/userController.ts#method:UserController.handleCreate
  - src/routes/userRoutes.ts#fn:routeRequest
  - src/services/userService.ts#method:UserService.createUser
edit: createUser(dto) -> createUser(dto, tenantId: string)
diff --before <snap>
  - SignatureModified on …#method:UserService.createUser
  - ContractDelta::Modified is_breaking=true
    ("Inputs changed from [dto] to [dto, tenantId]")
verify --baseline-id <snap> -> exit 2
  - api-compatibility -> fail (1 breaking modification)
  - no-circular-deps -> pass
```

Call-chain reconstruction matches ground truth TASK-API-001/002 exactly.

## 3. Benchmark gate — honest re-measurement (50 tasks, real execution)

| Baseline | Recall | Precision | F1 | Tokens | Tool calls |
|----------|--------|-----------|----|--------|------------|
| A (Agent+Grep) | 68.27% | 75.00% | 70.04% | 926 | 2.92 |
| B (Agent+LSP) | 68.27% | 76.00% | 70.71% | 819 | 1.82 |
| C (Agent+Autopsy) | 100.00% | 5.98% | 10.80% | 2693 | 3.00 |

- Gate 1 (recall lift ≥ +10pp): **PASS** (+31.73pp), earned by real
  transitive closure, not copied answers.
- Gate 2 (cost cut ≥ 40%): **FAIL** (evidence JSON is verbose).
- Gate 3 (new failure class): **FAIL** (vocabulary gap: engine rule ids
  vs task change classes; single-state corpus checkouts cannot exercise
  two-state compat, which is covered by `e2e_forensic_closure` instead).
- Overall: **PASS** (recall criterion). Prior reported 100%/100%
  precision figures are withdrawn as invalid (simulated baseline).

### Known limitation (disclosed, not tuned)

Bidirectional default closure over-approximates (precision ≈ 6%):
file-node transit floods structurally adjacent symbols. Deliberately
left untuned — threshold-hacking the corpus would be benchmark
overfitting. Operator-side precision control shipped instead:
`impact --edge-kinds calls` (plus direction/depth/budget), ranked
summarization deferred to the v0.1 reviewer study (RQ3). Evidence:
calls-only backward query on the live-fire case returns exactly the
ground-truth set (3/3, 0 FP).

## 4. Quality gates executed

- `cargo fmt --all -- --check`: clean
- `cargo clippy --workspace --all-targets -- -D warnings`: zero warnings
- `cargo test --workspace`: 136 passing (adds: 7 symbols, 10 adapter
  net-new incl. typed receivers, 8 forensic e2e, 1 invariants, 3 resource-cap)
- `cargo test --workspace --doc`: clean
- `./scripts/verify-determinism.sh`: 100/100 byte-identical
- `./scripts/verify-arch-boundaries.sh`: clean (no LLM/net/process in core)
- `./scripts/verify-cross-platform.sh`: PASSED
- `schemas/*.json`: valid JSON
- `apps/mcp-server`: `npm run typecheck` + 5/5 tests pass
- `apps/web-inspector`: JS syntax-valid; headless render verified
  against real impact + verify artifacts
- `make check` consumable by CI (fmt, clippy, tests, determinism,
  boundaries, cross-platform, schemas, mcp-check)

## 5. Upgrade note

Pre-split snapshot identities (`…:hex12` suffixes) do not join new
shape-free identities. Re-run `baseline` after upgrading; `diff` against
snapshots whose contents were cached by the old binary errors
explicitly (`re-run baseline…`) instead of comparing garbage.
