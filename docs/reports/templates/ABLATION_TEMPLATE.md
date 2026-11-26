# Ablation Report — `<change or control>`

**Report ID:** `ABL-<id>`  
**Date:** `<UTC date>`  
**Hypothesis:** `<testable statement>`  
**Source requirement/PRD:** `<exact path and section>`  
**Commit:** `<full commit>`  
**Status:** `VERIFIED | DIRECTIONAL | PARTIAL | NOT MEASURED`

## Control and variant

| Dimension | Control | Variant |
|---|---|---|
| Implementation | `<commit/config>` | `<commit/config>` |
| Workload | `<fixture digest>` | `<same fixture digest>` |
| Environment | `<runner/toolchain>` | `<same runner/toolchain>` |
| Cache/process state | `<cold/warm>` | `<cold/warm>` |

## Correctness equivalence

Compare verdict, coverage, sorted output, BLAKE3 digest, source/Git hashes,
error state, and receipt completeness before considering performance.

## Measurements

| Metric | Control | Variant | Samples | Raw artifact |
|---|---:|---:|---:|---|
| p50 | | | | |
| p95 | | | | |
| p99 | | | | |
| CPU/RSS/I/O | | | | |

## Interpretation

Report uncertainty, paired differences, confidence interval or MAD, outliers,
and whether the evidence supports a causal claim. If the comparison is not
paired or the sample is too small, use `DIRECTIONAL` or `NOT MEASURED`.

## Safety and follow-up

Document mutation boundaries, regression tests, canvas/UI impact, unresolved
limitations, and the exact next experiment needed for a stronger claim.
