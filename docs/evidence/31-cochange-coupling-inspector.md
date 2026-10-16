# Evidence 31 — Co-change gets its view, and the view says what the engine says

**Date:** 2026-10-16
**Scope:** item 4 of the six outstanding closure items: the co-change engine (Step 42,
Pillar 5) had a CLI subcommand, a closed schema and an MCP tool — but no Web Inspector view.
The temporal and blast-radius views existed; a mined coupling report could not be looked at.

---

## 1. What was built

A sixth tab, **Co-Change Coupling**, rendering `autopsy cochange --format json`:

* the **association banner** — "Association, never causation", with the engine's own
  `temporal_co_occurrence_only` tag shown per edge, not paraphrased. This is the one
  sentence the view exists to say correctly: a co-change edge is a statistical
  co-occurrence, and drawing it as anything edge-like would be the exact misreading the
  engine's `EdgeKind::StatisticalCoChange` exists to prevent.
* the **edges** as pairs (`a ⇔ b`) with the joint-commit count as the headline, support /
  confidence / lift in parts per million (the exact integer counts stay authoritative, per
  the schema's own `ppm` definition), and the per-edge coverage pill.
* the **accounting**: commits seen vs considered, pairs below threshold, paths with
  changes, paths never coupled, digest, coverage pill — and the **exclusions named with
  their reasons** (merge commits, mass commits), because a population someone is not part
  of must be visible to them.
* the **truncation notice**: when `pairs_truncated` is true, the view says the pair set is
  a *stated lower bound* and will not pretend the remainder is empty.

## 2. The rendering decision: a list, not a graph

The blast-radius view draws a radial graph because the catalog *states* references, and a
stated reference is an edge. A co-change pair is not an edge in that sense — it is an
association the engine refuses to call one. So the coupling view is a **list with the claim
tag on every row**, and no graph is drawn. That absence is the honesty requirement, not a
missing feature: a graph would invite the reader to see dependency where the engine says
there is only co-occurrence.

## 3. Empty is never empty pass

Three different "no edges" situations get three different statements, because they are
three different facts:

1. `pairs_below_threshold > 0` — pairs were seen and fell below the threshold; they are
   counted, not hidden.
2. `pairs_below_threshold == 0` — there was nothing to couple in this window.
3. `coverage: unknown` — the engine refused to answer; the view says an empty list here is
   a *refusal, not a finding* of "nothing is coupled".

## 4. Classification and refusal

`classifyArtifact` gains `co_change` (`window` + `edges` array + `coverage`), checked
before the verify/blast-radius shapes and after the bare-fact-array case. The refusal
message for an unrecognized artifact now names all four renderable artifacts, and a test
asserts it mentions `cochange`.

## 5. Verification

| Gate | Result |
|---|---|
| `cd apps/web-inspector && npm test` | **15 passed / 0 failed** (baseline 11 + 4) |
| pre-commit (12 hooks, incl. the inspector render gate) | 12/12 Passed |

The new tests assert the honesty rules, not the markup: the banner carries "never
causation" and the verbatim claim tag, a truncated pair set is a lower bound, the three
empty situations render their three statements, exclusions are named with reasons, the
coverage pill and digest card appear, and the page's read-only/time-free source guard still
holds (it is part of the existing suite and ran unchanged).
