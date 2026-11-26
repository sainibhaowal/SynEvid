# Evolution Entry — Phase 0–3 Reporting Baseline

**Date:** 2026-11-07  
**Report revision:** `EVO-2026-11-07-P0-P3`  
**Commit:** `ed88e3ece4d099bdfb71a8506e3c844c9d8ec948`  
**Workflow:** [GitHub Actions run 80](https://github.com/sainibhaowal/SynEvid/actions/runs/42316135982)

## Change recorded

Added a durable reporting layer under `docs/reports/`:

- reporting governance and truth-status rules;
- public benchmarking standards;
- Phase 0–3 status matrix;
- evaluation and ablation report for each phase;
- publication dossier;
- append-only evolution log and templates.

## Evidence recorded

Run 80 passed the native Windows, Ubuntu, and macOS workspace/Electron and
pre-merge gates. The strict Phase 3 5,000-loose-commit gate measured:

| Runner | Time | Threshold |
|---|---:|---:|
| Ubuntu | 46.56 ms | <150 ms |
| macOS | 50.57 ms | <150 ms |
| Windows | 112.76 ms | <150 ms |

The earlier Windows 445.74 ms result remains preserved in evidence 103. The
current report labels the optimization ablation `DIRECTIONAL` because the old
and new measurements are not a paired same-runner statistical experiment.

## Safety statement

This reporting-only change does not alter the analyzer, daemon, Git engine,
canvas renderer, animation, camera, layout, or styling. Existing dirty and
untracked user changes were preserved.

## Next action

If a quantitative optimization claim is needed, collect paired cold/warm
control-versus-variant samples on each native runner and append the raw samples
and statistical summary. Do not edit this entry to make the claim stronger.
