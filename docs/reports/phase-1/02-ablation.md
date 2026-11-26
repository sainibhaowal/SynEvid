# Phase 1 Ablation — Event Publication and Daemon Synchronization

**Report ID:** `ABL-P1-2026-11-07`  
**Source:** `Resource/6.1 Fixing/02_PHASE_1_CORE_DAEMON_STORAGE.md`  
**Status:** `DIRECTIONAL`; a paired statistical baseline is not yet measured

## Question

Does the current bounded event publication/replay path preserve correctness
while reducing producer-to-consumer wake-up time and keeping reconnect replay
possible?

## Comparison design

| Dimension | Control | Variant |
|---|---|---|
| Publication | Synchronous or polling comparison, when available | Current event publication path |
| Input | Same repository, event payload, subscriber count, and queue limits | Same |
| Correctness | Sequence monotonicity, replay, digest, source immutability | Same |
| Metrics | p50/p95/p99, sample count, queue drops, CPU/RSS | p50/p95/p99, sample count, queue drops, CPU/RSS |

## Observed evidence

The available Windows evidence reports event wake-up p95 of 0.214–0.453 ms for
the tested serial workloads. It also verifies replay and restart behavior. No
paired control run with the same workload and runner was retained, so a causal
percentage improvement is intentionally **not claimed**.

## Correctness guardrails

The variant is acceptable only if event sequence IDs remain monotonic, missed
events replay after reconnect/restart, back-pressure is bounded, shutdown does
not silently lose committed events, and source/Git state remains unchanged.

## Required follow-up before a quantitative publication claim

Run control and variant on the same native runner, with at least 30 independent
cold and warm samples, identical payloads/subscribers, and fixed queue limits.
Publish p50/p95/p99, median absolute deviation or confidence interval, CPU/RSS,
drop count, replay count, and raw samples. Do not convert the current p95 into
an improvement percentage without that paired baseline.
