# Phase 3 Ablation — Loose-Object History Loading

**Report ID:** `ABL-P3-2026-11-07`  
**Source:** `Resource/6.1 Fixing/04_PHASE_3_GIT_FORENSICS.md`  
**Change under test:** loose-object preloading and related history-load
optimization, commit `c64ea1f` as recorded by the native evidence  
**Status:** `DIRECTIONAL`; full paired statistical ablation is still `PARTIAL`

## Hypothesis

Preloading and bounded, deterministic object access reduce repeated loose-object
history I/O and lock overhead without changing reconstructed commits, trees,
parents, object verification, or output digest.

## Acceptance comparison

| Measure | Required control | Required variant | Guardrail |
|---|---|---|---|
| Correctness | Previous loader | Optimized loader | Same commit/tree identities and BLAKE3 outputs |
| Workload | 5,000 loose commits | Same repository and commit count | Same fixture digest and runner |
| Timing | Cold and warm samples | Cold and warm samples | p50/p95/p99 and sample count published |
| Resource use | CPU/RSS/I/O | CPU/RSS/I/O | No unbounded cache or worker growth |

## Native acceptance observations

Run 80 strict-gate timings were:

| Runner | Optimized result | Target | Result |
|---|---:|---:|---|
| Ubuntu | 46.56 ms | <150 ms | `PASS` |
| macOS | 50.57 ms | <150 ms | `PASS` |
| Windows | 112.76 ms | <150 ms | `PASS` |

The earlier Windows 445.74 ms loose-object cold-start result is retained in
evidence 103. Because the earlier and later measurements were not a paired
same-runner baseline/variant sample set, no causal percentage improvement is
claimed.

## What is proven

The optimized implementation passes the strict 5,000-commit acceptance gate on
all three native runners in run 80, while the surrounding Git integrity tests
remain green. This is strong directional evidence of the intended improvement.

## What remains for a full publication-grade ablation

Run a same-commit control and variant on each native runner with at least 30
cold and 30 warm samples. Fix the repository fixture digest, CPU class, runner
image, cache state, commit count, and process flags. Publish raw timings, p50,
p95, p99, spread/confidence interval, CPU/RSS/I/O, object-cache hit rate,
digest equality, and a regression threshold. Until then, label the causal
effect `DIRECTIONAL`, not statistically proven.

## Safety result

No canvas renderer, animation, camera, layout, or styling code is part of this
optimization. Historical snapshots remain read-only and Git integrity tests
remain the correctness gate.
