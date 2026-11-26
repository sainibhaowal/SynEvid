# Phase 2 Evaluation — Language Analysis, Graph, Diff, and Blast Radius

**Report ID:** `EVAL-P2-2026-11-07`  
**Source:** `Resource/6.1 Fixing/03_PHASE_2_ANALYSIS_GRAPH_BLAST_RADIUS.md`  
**Gate:** G2  
**Status:** `VERIFIED` for the literal functional scope tested; dynamic and
unsupported behavior remains `PARTIAL` or `UNKNOWN`  
**Primary evidence:** `docs/evidence/102-phase2-language-graph-blast-radius-closure.md`

## Decision

The tested CLI, daemon, terminal, MCP, TUI, and web paths consume the same
deterministic impact result for supported fixtures. The engine does not claim
to prove reflection, dynamic imports, generated code, macros, runtime dispatch,
or undocumented external systems when adapters cannot resolve them.

## Functional evidence

| Area | Evidence | Result |
|---|---|---|
| Impact engine | Focused impact suite 9/9 | `VERIFIED` |
| CLI parity | CLI suite 25/25 | `VERIFIED` |
| Adapters, graph, diff, impact | End-to-end suite 21/21 | `VERIFIED` |
| Daemon/API path | `synevid-ui` daemon suite 27/27 | `VERIFIED` |
| Browser behavior | Web inspector 38/38, including a real temporary repository flow | `VERIFIED` |
| Read-only simulation | Source bytes and Git state remain unchanged | `VERIFIED` |
| Determinism | 100-run fixtures and unsupported-coverage tests | `VERIFIED` for cited fixtures |
| Terminal parity | Structured request reaches the daemon-backed engine; canned affected count is removed | `VERIFIED` for cited path |

## Algorithm and evidence behavior

Seeds are resolved and canonically ordered. Typed edges retain provenance and
resolution method. Bounded traversal reports depth/node/byte/time truncation.
Complete evidence is kept separate from ranked presentation. Serialization is
sorted and digested with BLAKE3. Ambiguous, unsupported, or truncated results
are not promoted to semantic `PASS`.

## User-visible scope

The Blast Radius panel exposes affected files, symbols, edges, paths, coverage,
budgets, and unknown areas. Canvas focus is presentation-only. Existing 2D/3D
node styling, animation, camera, layout, and renderer behavior are outside this
analysis change and were not modified by the cited work.

## Limitations

“Not referenced in the analyzed graph” is not the same as global dead code.
Runtime-unobserved code and static unreferenced code are separate claims. Parser
coverage is language/version/fixture dependent, and a bounded result can be
`PARTIAL` when the budget is reached. No performance claim is made here for all
repository sizes.

## Publication conclusion

G2 is publishable as **deterministic supported-language impact analysis verified
for the cited client paths**. It must not be advertised as perfect blast-radius
knowledge for arbitrary runtime systems.
