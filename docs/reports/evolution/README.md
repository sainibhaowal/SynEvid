# Evaluation and Ablation Evolution Log

This directory is append-only. Each release or evidence refresh adds a dated
entry; existing measurements are never overwritten. A later result may supersede
a gate decision, but the earlier raw result remains part of the audit history.

Every entry must include:

- date and report revision;
- commit and workflow/run identity;
- changed requirements or implementation area;
- evidence files and raw artifact locations;
- exact environment and workload identity;
- new measurements and their status;
- regressions, unresolved limitations, and next action;
- explicit statement about whether the canvas/UI renderer changed.

Use the templates in `../templates/` for new evaluation and ablation records.
