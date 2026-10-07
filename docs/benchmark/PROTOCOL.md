# Code Autopsy validation protocol
## Primary question
Does Autopsy materially improve a frontier coding agent's ability to identify change impact compared with the same agent using ordinary repository/LSP tools?

## Corpus
- >=5 mature repositories in first language ecosystem.
- >=50 historical change tasks.
- Ground truth from merged commit/PR/tests and downstream changes.

## Baselines
A. Frontier agent + filesystem/grep.
B. Frontier agent + ordinary LSP/IDE structural tools.
C. Frontier agent + Code Autopsy.

## Metrics
Impact recall, impact precision, false negatives, false positives, tool calls, tokens/context, cost, wall-clock, completion rate.

## Pre-registered continuation gate
Continue if Autopsy produces >=10 percentage-point impact-recall lift over the strongest ordinary-tool baseline OR >=40% lower cost/context at comparable accuracy OR repeatedly catches a distinct high-value contract/invariant failure class the baselines miss.

Benchmark task truth must not be changed in the same PR as an engine change that is evaluated on it.
