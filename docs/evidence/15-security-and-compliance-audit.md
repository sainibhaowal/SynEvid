# Evidence 15 — Phase 4.1 Gap Closure (Agent/MCP/CI Hardening)

Date: 2026-10-09. Audited `Resource/Completed/4.1/*.md` end to end
against the implementation; every gap below was reproduced before fixing.

## 1. Gaps found and fixed

| # | Plan requirement | Gap (reproduced) | Fix | Proof |
|---|------------------|------------------|-----|-------|
| 1 | 02 §3: invalid config fails before verification | `load_or_default_config` swallowed invalid `autopsy.toml` into silent defaults (fail-open) | Present-but-invalid file is now a hard `CliError::Config` with path; only a missing file falls back | `test_invalid_config_fails_before_analysis_with_location`, `test_missing_config_falls_back_to_defaults` |
| 2 | 01 §2: graphs paginated as resources, never full dumps | Graph resource returned a summary only; no pagination existed anywhere | CLI `symbols`/`edges` (stable order, limit/offset/kind, SARIF explicitly rejected) + MCP `autopsy://graph/symbols{?limit,offset,kind}` and `autopsy://graph/edges{?source,…}` templates | `test_symbols_and_edges_browsing_commands`, MCP pagination/overlap/rerun tests (8/8) |
| 3 | 01 §2: versioned MCP schemas, honest params | `autopsy_verify.profile` was a dead param; impact lacked precision controls | Dead param removed; `profile`, `edge_kinds`, `top_k` added (all optional, back-compat) | typecheck + tests green |
| 4 | 02 §2: `cargo-deny/audit` in PR | `deny.toml` existed but nothing ran it; schema targeted a pre-0.19 deny (rejected by 0.20) | `supply-chain-audit` CI job wired into the merge gate; policy rewritten to valid 0.20; Zlib allowed (foldhash, OSI-approved) | `cargo deny check`: advisories/bans/licenses/sources ok |
| 5 | 02 §2: `eslint + tsc`, frozen lockfile | No eslint config, script, or gate | eslint 9 + typescript-eslint, `npm run lint`, wired into Makefile/pre-commit/CI; 0 errors | `npm run lint` clean |
| 6 | 02 §2: `ruff + pytest` for benchmarks | 86 ruff findings; zero pytest tests | All fixed (narrowed excepts, collapsed ifs, modernized annotations, shebang removed, 1 justified noqa); `benchmarks/tests/test_harness_smoke.py` (4 tests incl. ground-truth non-leak); CI schemas job runs ruff + pytest | ruff clean, pytest 4/4, harness category run green |
| 7 | 02 §2: perf regression by tier (scheduled) | No perf harness existed | `scripts/verify-perf-tiers.sh` (baseline+callers impact per corpus repo, generous smoke budgets, isolated stores) wired into nightly `benchmarks.yml` | All 5 repos pass with ~1000x headroom |
| 8 | 02 §2: SBOM/provenance in releases | SHA256SUMS only | CycloneDX SBOM (`cargo-sbom`) generated and bundled with checksums | 97-component SBOM generated locally |
| 9 | 03 §1: every public schema field documented | 22 undocumented properties across 4 schemas | Descriptions added to every property (4 files) | JSON-validated; plan item closed |

## 2. Test-harness flaw found en route (fixed)

MCP tests resolved the CLI via `~/.cargo/bin` first, silently testing a
stale install instead of the workspace build (masked the new commands).
Tests now pin `AUTOPSY_BIN` to the workspace debug binary when present.

## 3. Deliberately not built (plan-consistent)

- `mypy/pyright` on the harness: untyped legacy code would need broad
  `ignore` scaffolding to pass; recorded as deferred rather than gamed.
  Ruff + pytest smoke are enforced instead.
- Git-object reads (`gix`): plan names it as a means, not an end;
  snapshot-store comparison satisfies the ends offline. No new dependency.
- GUI expansion and hosted services: step 27 stays locked behind
  design-partner evidence (none exists).
- `benchmarks.yml` still shells the real gate (PASS on recall); numbers
  re-verified after the Python refactor.

## 4. Quality gates executed

- `cargo fmt --check`: clean; `clippy -D warnings`: zero warnings
  (after factoring the impact-request bundle and artifact type alias).
- `cargo test --workspace --all-targets`: all green.
- Determinism / arch-boundary / cross-platform scripts: PASS.
- `apps/mcp-server`: typecheck + lint + 8/8 tests.
- `make check`: exit 0. All workflow YAML re-validated.
