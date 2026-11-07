# Production Evidence Receipt: Phase 3 Native CI and Git Performance

**Scope:** Phase 3 native Git history loading and cross-platform verification
**Commit tested:** `797be989fd703ebd31ba0f6ba12fed96f1fadce0`
**GitHub Actions run:** [run 79](https://github.com/sainibhaowal/SynEvid/actions/runs/42315333732)

## Result

The strict 5,000-loose-commit Galaxy history gate passed on all native runners.
The measured operation is the release-mode `autopsy-git-engine` acceptance test
with `SYNEVID_PHASE3_LOOSE_HISTORY_GATE=1`; every result was below the required
150 ms limit.

| Runner | Measured history read | Result |
|---|---:|---|
| Ubuntu | 44.65 ms | PASS |
| macOS (Apple Silicon) | 96.58 ms | PASS |
| Windows | 142.74 ms | PASS |

## Full run evidence

- Pre-Merge Verification Gate: `success`
- Linux, macOS, and Windows Rust workspace tests: `success`
- Linux, macOS, and Windows Electron packaged smoke: `success`
- Determinism and architecture gate: `success`
- MSRV 1.90 build: `success`
- Public schema/config validation: `success`
- TypeScript MCP and web inspector checks: `success`
- Cargo deny supply-chain audit: `success`
- Installer/update/host-wiring self-test: `success`

## Changes validated

- Loose-object preloading now sorts object IDs, uses bounded workers, and batches
  cache insertion instead of taking a mutex for every object.
- All Tree-sitter adapters use one 0.25.10 native runtime, which supports the
  ABI-15 C# grammar without conflicting native `links` packages.
- The dense renderer fixture test normalizes CRLF before source-boundary checks,
  so Windows checkout line endings do not alter test behavior.
- The Phase 3 history budget is enforced on Ubuntu, macOS, and Windows in CI.

## UI safety boundary

The 3D canvas renderer, animation loop, camera/orbit behavior, WebGL buffers,
node layout semantics, and styling were not changed by the performance or CI
work. The existing HUD layout change remains presentation-only.

## Evidence boundary

This receipt proves the stated Phase 3 history-loading gate and the listed CI
jobs for the tested commit. It does not claim arbitrary repository performance,
complete static understanding of dynamic runtime behavior, or correctness of
unrelated local modifications outside the tested commit.
