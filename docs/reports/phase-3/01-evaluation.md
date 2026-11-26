# Phase 3 Evaluation — Native Git Forensics, Time Travel, and Controlled Mutations

**Report ID:** `EVAL-P3-2026-11-07`  
**Source:** `Resource/6.1 Fixing/04_PHASE_3_GIT_FORENSICS.md`  
**Gate:** G3  
**Status:** `VERIFIED` for the tested functional flows and strict native history
budget  
**Evidence:** `docs/evidence/103-phase3-git-integrity-and-honest-galaxy.md`,
`docs/evidence/104-phase3-native-ci-performance.md`, GitHub Actions run 80

## Decision

Native Git reading, historical snapshots, semantic comparison, read-only time
travel, explicit mutation receipts, and the tested client paths are verified.
The evidence distinguishes semantic analysis from file-count heuristics and
preserves the historical pre-optimization failure rather than rewriting it.

## Functional evidence

| Area | Evidence | Result |
|---|---|---|
| Native object reading | Loose, packed, ref-delta/ofs-delta, tags, branches, merge parents | `VERIFIED` for real fixtures |
| Integrity handling | Object type/hash/size and decode/error behavior | `VERIFIED` for cited tests |
| Historical snapshots | Isolated reads; working files and `HEAD` untouched | `VERIFIED` |
| Semantic history | Reconstructed trees compared through analysis adapters | `VERIFIED` for cited fixtures |
| Mutation controls | Stage/commit/branch/checkout/revert/undo paths use explicit receipts | `VERIFIED` for tested flows |
| Native temporal suite | 29/29 | `VERIFIED` |
| Git engine suite | 8/8; real packed repository 1/1 | `VERIFIED` |
| Web/MCP integration | Web 20/20, inspector 10/10, MCP 49/49 | `VERIFIED` |
| Historical frame | 8.48 ms in the cited packed fixture, below 100 ms | `VERIFIED` for that fixture |

## Strict native history budget

Run 80 used the strict `SYNEVID_PHASE3_LOOSE_HISTORY_GATE=1` acceptance path:

| Native runner | 5,000 loose-commit load | Target | Result |
|---|---:|---:|---|
| Ubuntu | 46.56 ms | <150 ms | `PASS` |
| macOS | 50.57 ms | <150 ms | `PASS` |
| Windows | 112.76 ms | <150 ms | `PASS` |

These are operation-specific acceptance measurements, not a promise that every
repository, disk, cache state, or workload will remain below 150 ms.

## Historical record retained

Evidence 103 records the earlier Windows loose-object cold-start measurement of
445.74 ms and the packed measurement of 140.61 ms. That earlier result remains
part of the audit trail. The later optimized native CI run demonstrates a
passing strict gate; it does not erase or retroactively change the old result.

## Risk and UI boundary

Galaxy file-count summaries are labeled heuristic. If semantic evidence is not
available, the result is `UNKNOWN` or “heuristic only,” never semantic `PASS`.
Checkout/revert/merge/commit remain explicit mutations with pre-state, post-state,
and recovery receipts. Time travel and visualization do not modify working
files, `HEAD`, canvas layout, animation, camera, or styling.

## Publication conclusion

G3 is publishable as **native Git forensic functionality verified for the cited
repositories, clients, and native CI gate**. It is not a claim of universal Git
performance or complete runtime semantic understanding.
