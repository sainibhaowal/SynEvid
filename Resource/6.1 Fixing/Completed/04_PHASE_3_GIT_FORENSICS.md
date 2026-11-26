# Phase 3 — Native Git Forensics, Time Travel, and Controlled Mutations

## Objective

Make Git history analysis truthful across real repositories and keep read-only analysis separate from explicit Git-writing controls.

## Current findings

- Native Git reading supports loose objects, packed objects, deltas, branches, tags, and merge parents.
- Semantic historical diff is stronger than the Galaxy summary.
- The 5,000-commit test measured 215.48 ms against the required target below 150 ms.
- A Git tree decoding error can currently return `PASS`; this must become `ERROR` or `UNKNOWN`.
- The Galaxy score is file-count heuristic unless semantic diff evidence is attached.

## Requirements

- Read loose and packed objects without invoking external Git for analysis.
- Verify object hashes, types, sizes, delta bases, and chain limits.
- Preserve commit parents, tags, branches, and dirty working-tree state.
- Time travel reads isolated snapshots and leaves working files and `HEAD` unchanged.
- Semantic commit diff compares reconstructed trees through the same language adapters as live analysis.
- Galaxy summaries label file heuristics separately from semantic findings.
- Git mutations require explicit user action, confirmation, pre-state receipt, post-state receipt, and recovery information.
- Checkout, revert, undo, stage, commit, branch, tag, merge, and bisect must have failure-safe behavior.

## Risk model

Risk is not one number. Report independent dimensions:

- changed file count;
- added/removed/modified symbols;
- breaking contract deltas;
- impacted callers and edges;
- boundary and invariant violations;
- unknown or partial coverage;
- dirty-tree conflict risk;
- historical confidence;
- operation cost.

If semantic evidence is unavailable, the result is `UNKNOWN` or “heuristic only,” never semantic `PASS`.

## Non-requirements

- no external Git subprocess in correctness analysis;
- no automatic checkout, revert, commit, or merge during analysis;
- no semantic claim from file-count score alone;
- no rewriting of user Git history without explicit mutation consent;
- no modification of working files for historical visualization.

## CLI, TUI, and MCP work

- `git inspect`, `git diff`, `git impact`, and `git verify` share the same snapshot IDs and receipts.
- Mutating commands use the daemon mutation API and return a recovery receipt.
- TUI shows pre-state and post-state side by side with a mutation boundary indicator.
- MCP exposes read-only Git analysis separately from mutation tools.

## Frontend and UX work

- Git Galaxy distinguishes commit topology, file heuristic, semantic diff, and unknown coverage.
- Time-travel slider shows exact commit ID, tree ID, dirty state, and read-only status.
- Mutating controls require explicit confirmation and display affected files before execution.
- Canvas only visualizes Git results; it must not change graph layout or animation semantics.

## Tests

- real repositories containing packed objects and ref/ofs deltas;
- tags, branches, merges, annotated tags, shallow history, missing objects, and corrupt objects;
- dirty worktree with staged and unstaged changes;
- time travel with byte and mtime preservation checks;
- semantic diff against historical tree;
- mutation recovery after failure;
- no external Git subprocess in correctness paths;
- 5,000-commit benchmark with cold and warm runs;
- error injection proving decode failures are never PASS.

## Gate G3

G3 passes only when Git evidence distinguishes verified semantic analysis from heuristic metadata and all failures remain non-success states.

