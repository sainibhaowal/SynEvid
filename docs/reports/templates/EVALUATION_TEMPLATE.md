# Evaluation Report — `<phase or component>`

**Report ID:** `EVAL-<id>`  
**Date:** `<UTC date>`  
**Source requirement/PRD:** `<exact path and section>`  
**Gate:** `<G0/G1/G2/G3 or component gate>`  
**Status:** `VERIFIED | PARTIAL | BLOCKED | NOT MEASURED`  
**Commit:** `<full commit>`  
**Workflow/run:** `<URL or local receipt>`

## Decision

State exactly what is proven, for which scope, and what is not claimed.

## Requirements and evidence

| Requirement | Implementation path | Test/evidence | Result | Limitation |
|---|---|---|---|---|
| `<ID>` | `<file/API/component>` | `<test/receipt>` | `<state>` | `<scope>` |

## Inputs and environment

- Repository/fixture identity and BLAKE3:
- OS/runner/image:
- Toolchain and parser versions:
- Configuration and budgets:
- Sample count and cache state:

## Results

Include raw artifact links, verdict/coverage states, latency samples, output
digests, and error receipts. Do not hand-enter measurements without a source.

## Limitations and non-claims

List unsupported behavior, truncation, unmeasured targets, and external
dependencies. `UNKNOWN`, `PARTIAL`, and `ERROR` must remain visible.

## Reproduction

List exact commands, workflow job, environment variables, and expected result.

## Safety review

State whether source, Git history, data, auth boundaries, and canvas UI code
were touched, and list regression checks.
