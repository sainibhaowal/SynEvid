# Phase 0–3 Status and Evidence Matrix

This matrix is current as of 2026-11-07 and is scoped to the requirements in
`Resource/6.1 Fixing/00_MASTER_PRODUCTION_PROGRAM.md` through
`04_PHASE_3_GIT_FORENSICS.md`.

| Phase | Gate | Current status | Direct evidence | Main boundary |
|---|---|---|---|---|
| 0 | G0 truth/evidence governance | VERIFIED for audited paths | Evidence 101; schema/config validation; determinism and architecture gates in [Actions run 80](https://github.com/sainibhaowal/SynEvid/actions/runs/42316135982) | This proves the tested contracts, not arbitrary runtime understanding |
| 1 | G1 registry/storage/daemon | VERIFIED for tested workflows | Evidence 101; real process/browser tests; Linux/macOS/Windows workspace and Electron jobs in run 80 | Detailed latency samples are Windows-local; native CI job success is recorded separately |
| 2 | G2 graph/impact/blast radius | VERIFIED for the literal functional scope tested on Windows; native workspace gate green | Evidence 102; impact 9/9; CLI 25/25; adapter/graph/diff E2E 21/21; inspector 38/38 | Dynamic/reflection/generated behavior remains `UNKNOWN`/`PARTIAL`; visual FPS/theme row remains separate |
| 3 | G3 native Git forensics | VERIFIED for tested functional flows and strict native history budget | Evidence 103/104; real Git fixture; run 80 strict matrix | Universal performance for arbitrary repositories and dynamic semantic behavior is not claimed |

## Phase 3 strict timing evidence

The release-mode 5,000-loose-commit acceptance test passed below 150 ms on all
native runners in run 80:

| Runner | Read time | Limit | Result |
|---|---:|---:|---|
| Ubuntu | 46.56 ms | <150 ms | PASS |
| macOS | 50.57 ms | <150 ms | PASS |
| Windows | 112.76 ms | <150 ms | PASS |

## Required interpretation

The matrix is a release evidence index, not a claim that every requirement is
proven by one test. A phase is eligible for publication only when its report
links each claim to an executable receipt and lists all remaining limitations.

