# Evidence 14 — Precision-First Impact, Incremental Baseline & PR Gate

Date: 2026-10-09. Follows evidence 13 (forensic closure) and the 3.2 audit.

## 1. Precision gap: what was built (no benchmark tuning)

The default bidirectional closure over-approximates (precision ≈ 6%):
file-node transit floods structurally adjacent symbols. Instead of
threshold-hacking the corpus, three reviewer-facing controls shipped:

1. **Ranked top-k shortlist (FR-008/FR-017):** entities ranked by
   shortest-path distance (then lexical); `--top-k N` presents
   `ranked_top` while `impacted_entities` keeps the complete evidence
   set. Schema extended additively (`ranked_top`, `total_impacted`).
2. **`--profile callers|full` (FR-008):** `callers` = backward
   invocation closure over Calls/References with file-node transit
   suppressed + top-10 default. `full` (default) is byte-identical to
   legacy behavior — zero back-compat risk (MCP + e2e suites rely on it).
3. **Engine `traverse_file_nodes` flag (FR-008):** file hubs are
   reported but not expanded (seeds still expand). Default true.

Live proof (real corpus repo, `benchmarks/corpus/repo-api`):
`impact -s …UserService::createUser --profile callers --top-k 10`
returns exactly the ground-truth set (controller + route + seed, 0 FP).

## 2. Incremental baseline (FR-020, implemented early)

`baseline --incremental --baseline-id` reuses content-hashed per-file
artifacts (symbols/edges/contracts/coverage) for byte-identical files —
no parse at all — and skips edge re-extraction when the global
symbol-id set is unchanged. Any cache miss degrades to full analysis.
Live proof: `reused 1/2 file analyses from cache`, incremental snapshot
id and full symbols/edges/contracts byte-identical to full analysis
(`test_incremental_baseline_matches_full_baseline`).

## 3. PR gate + design-partner kit

- `.github/workflows/autopsy-pr-gate.yml`: verify gates merge (exit 2
  fails), per-file callers impact for changed TS files, SARIF + JSON
  artifact upload, PR summary comment. Zero third-party actions beyond
  checkout (supply-chain minimalism).
- `scripts/forensic-case.sh <repo> <base-ref> <case-dir> [seeds]`
  builds a review bundle (baselines, diff, verify + SARIF, impacts,
  verdict) in a case-local store without touching the repo's own
  `.autopsy`. Live proof on a scratch repo with a breaking change:
  exit 2, `api-compatibility` fail, breaking reasons recorded.

## 4. Deferred honestly (not built)

- Ranked summarization beyond path distance (needs reviewer study, RQ3).
- Wall-clock parse timeouts (bounded instead by `max_file_bytes` +
  traversal budget; documented limitation).
- Second language adapter (partner-driven only, per prohibitions).

## 5. Quality gates executed

- `cargo fmt --check`: clean; `clippy -D warnings`: zero warnings.
- `cargo test --workspace --all-targets`: 145 green (adds: 3 impact
  engine incl. file-transit, 1 report ranking, 2 CLI profile/incremental
  e2e; forensic file now 11 tests).
- Determinism / arch-boundary / cross-platform scripts: PASS.
- `apps/mcp-server`: typecheck + 5/5 tests (defaults unchanged).
- `make check`: exit 0. Benchmark gate re-run: PASS (recall criterion).
