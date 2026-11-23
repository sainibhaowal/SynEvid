# Phase 6 — Ablation Protocol (Not Yet Executed)

**Status:** Protocol only. No ablation measurements or quality conclusions are
claimed in this document.

The existing 50-task impact benchmark is not an ablation for Phase 6 dead-code
or Git-risk evidence. Its legacy output is withdrawn as accuracy evidence:
the oracle/query scope and comparator integrity are invalid. Do not cite any
accuracy metrics or gate status as evidence that Phase 6 analytics are accurate
or production-ready; see evidence 108.

## Purpose

Measure whether the evidence controls materially improve the correctness and
usefulness of static-unreferenced and Git-risk reports, without changing the
frozen ground truth or tuning on the evaluation set.

## Frozen corpus and labels required before execution

Prepare a versioned corpus of real repositories and hand-reviewed fixtures.
For every declaration, label whether it is statically referenced within the
declared analyzed scope, an intended entrypoint/export/framework/plugin hook,
generated or test-only, dynamically reachable/unknown, or genuinely
unreferenced under the stated static model. For Git, label semantic breaking
changes from reviewed symbol/contract diffs independently from changed-file
counts. Record license, revision, language, size, parser versions, and split.

Do not tune exclusion lists or thresholds on the held-out test split. Freeze the
corpus manifest and label digests before running the experiment.

## Ablation arms

1. Graph inbound-edge evidence only.
2. Arm 1 plus visibility/export exclusions.
3. Arm 2 plus conventional entrypoint exclusions.
4. Arm 3 plus generated/test-only path exclusions.
5. Full evidence model, including parser coverage and explicit unknown results.
6. Git file-count heuristic alone versus adapter-backed semantic diff evidence.

## Required measurements

Report per-language precision, recall, false-positive rate, unknown/partial
rate, excluded-candidate counts by reason, runtime, peak memory, and digest
stability. For Git, report separate file-heuristic and semantic-risk metrics;
never combine them into one accuracy score. Include confidence intervals and
corpus/revision manifests. Every arm must preserve ground-truth files unchanged.

## Acceptance and publication rule

The experiment is publishable only with repeatable commands, raw machine-readable
outputs, environment details, corpus/license manifest, and independent review
of label sampling. If a required input or label is missing, mark the result
`UNKNOWN` and do not claim that one arm is superior. Store executed results in a
new dated report rather than editing this protocol into a claim of success.
