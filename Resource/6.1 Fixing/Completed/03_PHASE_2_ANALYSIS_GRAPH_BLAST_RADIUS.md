# Phase 2 — Language Analysis, Graph, Diff, and Blast Radius

## Objective

Provide real, deterministic change-impact analysis for supported languages, with complete coverage accounting and no fabricated terminal or UI results.

## Current findings

- The Rust impact engine performs deterministic graph traversal and has focused tests.
- The simulator can analyze an unsaved file and identify callers and some newly dead symbols.
- The embedded terminal currently returns a fixed “14 affected callers” result instead of running the engine.
- Parser coverage is not universal for reflection, dynamic imports, generated code, macros, runtime dispatch, and external systems.

## Requirements

- Parse supported languages through versioned adapters.
- Generate stable symbol IDs independent of line numbers.
- Store typed edges with adapter, source location, and resolution method.
- Preserve ambiguous edges as name-level, partial, or unknown evidence.
- Compute direct symbols and impacted boundary separately.
- Support forward, backward, and bidirectional traversal.
- Apply depth, node, byte, and time budgets and report truncation.
- Keep complete evidence separate from ranked presentation results.
- Compare genuine before and after snapshots; never compare a state with itself.
- Make simulation read-only and isolated from source files.
- Replace the terminal fake path with a daemon-backed real impact request.

## Algorithm contract

The default impact algorithm is a bounded deterministic traversal over the typed dependency multigraph:

1. Resolve seed symbols.
2. Sort seeds canonically.
3. Traverse selected edge kinds in deterministic order.
4. Record minimum hop count, provenance, coverage, and weakest link.
5. Stop only at an explicit budget boundary.
6. Mark `truncated=true` when the boundary is reached.
7. Partition results into complete, likely, possible, and unknown evidence.
8. Serialize sorted output and calculate BLAKE3 digest.

No ranking model may delete evidence from the full result.

## Non-requirements

- no claim of complete runtime understanding from static parsing;
- no LLM decision inside the correctness path;
- no hidden source edits;
- no guessed cross-file binding;
- no “perfect safety” claim when coverage is partial.

## Coverage rules

- static resolved edge: `VERIFIED`;
- ambiguous name-level edge: `PARTIAL`;
- reflection or generated behavior: `UNKNOWN` or `PARTIAL`;
- parser failure: `ERROR` for the file or `PARTIAL` for the aggregate, never `PASS`;
- exceeded budget: `PARTIAL` with explicit truncation.

## CLI, TUI, and MCP work

- CLI exposes full evidence in JSON and SARIF.
- TUI shows selected symbol, graph direction, edge provenance, coverage, budget, and receipt.
- MCP returns the same envelope and never rewrites the verdict.
- The terminal command is parsed into a structured request; it does not print canned prose.
- An invalid or missing symbol returns an actionable refusal, not an empty PASS.

## Frontend and UX work

- Blast Radius panel shows affected files, symbols, edges, paths, grades, and unknown areas.
- Provide filters for callers, references, contracts, files, and coverage state.
- Display “not analyzed” and “truncated” visibly.
- Canvas focus is a presentation action only; it must not mutate graph truth or renderer behavior.
- Preserve existing 2D/3D node styling and animation.

## Tests

- adapter conformance tests for every supported language;
- golden symbol and edge fixtures;
- rename, overload, ambiguous call, dynamic import, reflection, generated code, macro, and runtime-dispatch fixtures;
- 100-run determinism tests;
- simulator edit → diff → impact → verify E2E test;
- real CLI, TUI, MCP, and terminal request parity test;
- negative tests for invalid syntax and unknown symbols;
- mutation test proving analysis leaves source and Git unchanged;
- Playwright browser test showing the same affected set as CLI JSON.

## Gate G2

G2 passes only when terminal, CLI, MCP, TUI, and UI all consume the same engine result and every unsupported behavior is visibly non-PASS.

