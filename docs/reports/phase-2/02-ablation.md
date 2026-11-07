# Phase 2 Ablation — Graph Determinism, Simulation Isolation, and Evidence Tiers

**Report ID:** `ABL-P2-2026-11-07`  
**Source:** `Resource/6.1 Fixing/03_PHASE_2_ANALYSIS_GRAPH_BLAST_RADIUS.md`  
**Status:** `DIRECTIONAL`; performance and recall effect sizes are `NOT MEASURED`

## Questions

1. Does canonical traversal ordering make repeated results byte-identical?
2. Does in-memory simulation avoid source and Git mutation?
3. Does preserving complete evidence separately from ranking avoid hidden loss?

## Control matrix

| Question | Control | Variant | Required invariant |
|---|---|---|---|
| Ordering | Input/hash-map insertion order | Canonical seed/edge ordering | Same sorted result and BLAKE3 digest |
| Simulation | Analyze a persisted source edit | Analyze isolated unsaved content | Bytes, mtime, index, and `HEAD` unchanged |
| Presentation | Emit only ranked rows | Retain complete evidence plus ranking | No evidence removed by ranking |
| Unsupported behavior | Guess a binding or verdict | Emit `UNKNOWN`/`PARTIAL` | No guessed semantic `PASS` |

## Observed evidence

The 100-run determinism fixtures, negative coverage tests, and simulator
read-only tests pass in evidence 102. That verifies the invariants for the
cited fixtures. No paired corpus-wide precision/recall, memory, or latency
comparison was collected; those values are intentionally not reported.

## Follow-up experiment

Use the same repository corpus, parser versions, seed symbols, edge filters,
budgets, and process environment. Run control and variant at least 100 times;
compare digest equality, complete/likely/possible/unknown counts, truncation,
source/Git hashes, and resource usage. Publish all raw receipts and report any
fixture where a variant changes a verified verdict.
