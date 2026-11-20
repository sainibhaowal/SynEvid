# Phase 4 — Runtime Authority Ablation

Status: `NOT MEASURED`

The purpose of this report is to prove that the one-daemon protocol, rather
than client-local fallbacks, causes the observed parity and replay behavior.

## Planned controlled comparisons

| Variant | Change | Measurement | Expected interpretation |
|---|---|---|---|
| A — daemon authority | CLI, TUI, MCP, and UI use authenticated runtime endpoints | repository/session/snapshot/event cursor equality | Baseline production path |
| B — explicit offline CLI | CLI runs with `--offline` and no daemon | state is marked independent; no daemon event is claimed | Safe isolation, not synchronization |
| C — missing lease | authenticated request without a valid lease | structured runtime error or reconnect path | No silent client-local truth |
| D — missing cursor | replay from a cursor outside retained history | explicit gap metadata | No silent event loss |
| E — wrong token | wrong bearer or no bearer when required | `ERR-AUTH-401` | Loopback mutation is protected |

## Measurement rules

Use the same repository bytes, daemon configuration, client requests, and
snapshot pin for every variant. Report event counts, sequence sets, digests,
coverage, verdicts, latency distributions, and error codes. A comparison is
directional until repeated runs and native CI artifacts are attached.

No result may be fabricated or promoted to `PASS` because a client received an
empty response.
