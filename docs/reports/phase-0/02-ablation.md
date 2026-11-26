# Phase 0 Ablation — Truth Controls and False-Positive Prevention

**Report ID:** `ABL-P0-2026-11-07`  
**Status:** `VERIFIED` for control behavior; numerical effect-size ablation is
`NOT APPLICABLE` to the governance gate  
**Source:** `Resource/6.1 Fixing/01_TRUTH_GOVERNANCE_AND_REQUIREMENTS.md`

## Purpose

Phase 0 is a correctness and reporting control layer. Its primary question is
whether misleading results are blocked, not whether a model score increases.
Therefore this report records paired control cases and does not invent a
precision, recall, or latency improvement.

## Control cases

| Control removed or bypassed | Required observable outcome | Verified outcome |
|---|---|---|
| Evidence scope omitted | Result is incomplete or refused | Non-success states remain explicit in tested envelopes |
| Corrupt storage or Git object | Recovery/error receipt, never successful empty state | `ERR-STORAGE-102` and Git decode negative paths are covered |
| Fixed terminal blast count | Request must execute the real engine | Phase 2 evidence confirms terminal parity path |
| Hardcoded AI metrics | Metric is absent or marked unavailable without session evidence | Governance requires real session/cache evidence |
| Unsupported dynamic behavior | `UNKNOWN`/`PARTIAL`, never `PASS` | Unsupported fixtures and negative tests are covered |

## What this proves

The controls prevent the specific classes of false success identified in the
Phase 0 findings. It does not establish the completeness of a parser or the
accuracy of an analysis outside the supported language and snapshot scope.

## What is not measured

No paired benchmark was collected for storage size, CPU, latency, or analyst
time with and without the governance envelope. Such a measurement would belong
to a future performance/operability study and must use identical inputs, runner,
configuration, and sample plan.

## Future ablation protocol

For any future change to evidence controls, run the same fixture corpus in
control and variant modes; compare verdict equality, coverage-state equality,
receipt completeness, false-success count, serialization digest, and overhead.
Publish raw samples and the exact commit before claiming an effect.
