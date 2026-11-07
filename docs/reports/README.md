# SynEvid Reports

This directory is the publication-facing reporting layer for SynEvid. It
summarizes executable evidence from `docs/evidence/`, requirements from the
Phase 0–3 production documents, and native CI results. It does not replace
test logs, receipts, schemas, or source-level traceability.

## Report families

| Family | Purpose | Required cadence |
|---|---|---|
| Evaluation | Prove whether a requirement or release gate passed within a stated scope | Every phase, release candidate, and material regression fix |
| Ablation | Establish whether a specific algorithm, optimization, cache, or policy caused an observed change | Every material algorithm/performance/coverage change |
| Publication | Give users and reviewers a bounded, citable summary of proven behavior | Every release or external claim |
| Evolution | Preserve what changed, what was discovered, and how evidence changed over time | Every evidence update |

## Navigation

- [Reporting governance](00_REPORTING_GOVERNANCE.md)
- [Public benchmarking standard](01_PUBLIC_BENCHMARKING_STANDARDS.md)
- [Phase status matrix](02_PHASE_STATUS_MATRIX.md)
- [Phase 0 evaluation](phase-0/01-evaluation.md) and [ablation/control report](phase-0/02-ablation.md)
- [Phase 1 evaluation](phase-1/01-evaluation.md) and [ablation/control report](phase-1/02-ablation.md)
- [Phase 2 evaluation](phase-2/01-evaluation.md) and [ablation report](phase-2/02-ablation.md)
- [Phase 3 evaluation](phase-3/01-evaluation.md) and [ablation report](phase-3/02-ablation.md)
- [Phase 0–3 publication report](publication/PHASE_0_TO_3_PUBLICATION_REPORT.md)
- [Evolution log](evolution/README.md)
- [Evaluation template](templates/EVALUATION_TEMPLATE.md)
- [Ablation template](templates/ABLATION_TEMPLATE.md)

## Source-of-truth order

1. Executable test output, CI job logs, and persisted evidence receipts.
2. `docs/requirements/TRACEABILITY.md` and architecture decision records.
3. Phase PRDs under `Resource/6.1 Fixing/`.
4. These narrative reports.

If a report conflicts with executable evidence, the evidence wins and the
report must be corrected. A report may not promote `UNKNOWN`, `PARTIAL`,
`ERROR`, `UNAVAILABLE`, or `NOT MEASURED` into `PASS`.

## Status vocabulary

- `VERIFIED`: the stated claim passed with direct evidence for the stated scope.
- `PASS`: no violation was found within that verified scope.
- `PARTIAL`: some required behavior, platform, workload, or runtime coverage is incomplete.
- `DIRECTIONAL`: an observed comparison is useful but is not a controlled causal experiment.
- `NOT MEASURED`: no trustworthy measurement exists yet.
- `NOT APPLICABLE`: the report type does not apply; the reason must be recorded.

## Update rule

Every update adds an evolution entry containing the date, commit, runner or
host, changed claim, evidence links, result, limitations, and next action. Old
results are never overwritten; a later result supersedes them by reference.

