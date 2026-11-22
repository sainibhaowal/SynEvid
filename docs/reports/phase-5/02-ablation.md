# Phase 5 ablation report — not applicable

## Status

**NOT APPLICABLE for this change; no ablation result is claimed.**

This work corrects a desktop lifecycle-mode ambiguity and adds packaged
watchdog, security, tray, and shutdown acceptance checks. It does not alter an
analysis algorithm, ranking policy, cache, parser, risk model, or performance
optimization whose contribution can be isolated through an algorithmic
ablation.

The before/after packaged watchdog regression is a functional regression test,
not an ablation experiment: the previous packaged run failed to restore the
connected repository after a daemon crash; the explicit managed-runtime mode
then passed the same repository-identity acceptance. That establishes the fix
for this observed defect, not a generalized causal or performance claim.

If a later Phase 5 change modifies an optimization, security control, or
recovery strategy, create a controlled ablation with a fixed artifact, fixture,
host, repeated runs, defined outcome metrics, and raw results. Do not infer
ablation evidence from this report.
