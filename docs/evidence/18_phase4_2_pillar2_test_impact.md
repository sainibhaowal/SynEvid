# Evidence 18 — Phase 4.2 Pillar 2: Deterministic Test Impact (Steps 33–36)

**Date:** 2026-10-16
**Scope:** Steps 33–36 of `Resource/4.2/04_4.2_IMPLEMENTATION_SEQUENCE.md`.
**New crate:** `crates/autopsy-test-impact`
**Predecessor:** `docs/evidence/17_phase4_2_pillar1_temporal.md` (Steps 28–32)

---

## 1. What the pillar had to prove

The acceptance gates from the implementation sequence, verbatim:

| Step | Gate |
|---|---|
| 33 | Discovers Vitest, Jest, and Mocha test files; maps test suites to declared source imports. |
| 34 | Ingests LCOV; distinguishes observed execution from absent instrumentation. |
| 35 | **0% false negatives on the regression fixture**; force `RunAllTests` on dynamic dispatch / `Unknown` coverage. |
| 36 | Ranking is advisory-only and cannot block merges. |

Step 35 is the only gate in the phase where a *silent wrong answer* is the
failure mode rather than a loud one. A false negative — a test that would have
caught the regression but was not selected — is indistinguishable from a
correct narrow selection by looking at the plan alone. Everything below is
organised around making that failure impossible by construction rather than
unlikely by calibration.

---

## 2. The three load-bearing design decisions

### 2.1 Non-`Verified` suites cannot be ruled out (the core asymmetry)

`select_affected_tests` selects a suite when *positive* evidence connects it to
the change, but it never *excludes* on the absence of evidence. Concretely:

```
selected = { suites mapping to an affected source }          // static imports
         ∪ { suites observed executing an affected source }  // LCOV
         ∪ { suites whose mapping is not Verified }          // the asymmetry
```

The third term is the one that makes the gate hold. If discovery could not
resolve a suite's imports, could not determine its framework, or could not
parse it at all, then the engine does not know what that suite covers — and
"does not know" must not be spent in the direction of running fewer tests. Such
suites are recorded in `unrulable_out_suites` and run.

The deliberate cost: a helper file under `test/` that contains neither a
framework import nor a test declaration is classified `Unknown` and therefore
always runs. This is noisy, and it is the correct trade. The alternative —
treating "I did not understand this file" as "this file is irrelevant" — is
exactly the defect class the gate exists to forbid.

### 2.2 Absence of a positive mapping forces a full run

If no suite can be positively connected to the change set, the plan returns
`RunAllTests`. The reasoning is that two very different situations produce the
same observable state:

* genuinely nothing tests this code, and
* our mapping is broken.

From inside the engine these are indistinguishable. The honest reading of an
ambiguity you cannot resolve is the conservative one, so the fallback records
the ambiguity in `SelectionFallback::RunAllTests { reason }` with the reason
`"no test suite could be positively mapped to the change set; 'no test covers
this' cannot be distinguished from a mapping failure"`.

When static imports *do* narrow but no coverage was ingested, the fallback is
`RunPackageTests` instead — narrower than running everything, but explicitly
labelled as resting on static imports alone, and `coverage` degrades to
`partial`.

### 2.3 Step 36 is structurally incapable of gating a merge

Requirement from the pillar plan: predictive ranking is simulation and
presentation only and **must never block a merge**.

Enforced by the type system, not by documentation:

* `select_affected_tests` accepts no ranking input. There is no code path by
  which a probabilistic score reaches `SelectionPlan`.
* `PredictiveRanking` carries `advisory_only: bool`, always `true`, so the
  artifact is self-describing to a consumer that has never read this document.
* The scoring function is a directly measurable ratio
  (`observed_affected_sources / total_affected_sources`) — no fitted model, so
  nothing can drift between runs or claim predictive power the engine has not
  demonstrated.
* When there is no coverage data, the ranking emits **no entries** and reports
  `coverage: unknown`. It refuses to guess.

`step36_gate_ranking_cannot_change_the_plan` re-derives the plan *after*
computing a ranking and asserts byte-identical equality of both the plan and
its digest.

---

## 3. Honest-coverage map (`FR-014`, `FR-030`)

Every degradation path, and what it produces. Nothing here converts an
`UNKNOWN` into a `PASS`.

| Situation | Coverage | Consequence |
|---|---|---|
| Suite imports a framework and all relative imports resolve | `verified` | Eligible for exclusion |
| Relative import does not resolve against the file set | `partial` + specifier named in `unresolved_specifiers` | Cannot be ruled out → runs |
| Test declarations present but no framework import (ambient globals) | `partial` | Cannot be ruled out → runs |
| Test file with neither framework import nor any declaration | `unknown` | Cannot be ruled out → runs |
| `import(...)` / `require(...)` / template-literal specifier | not guessed at | Left unbound rather than misbound |
| Computed test name | `name: None` | Addressed as `…::@line<N>`, never an invented name |
| LCOV `SF` with no `DA` | `partial` + note | "Instrumented but nothing recorded" is not "ran clean" |
| LCOV tracefile with no `SF` at all | `unknown` + note | Distinguished from "never ingested" |
| LCOV `DA` record with zero hits | `verified` | Zero hits is a *record*; kept as `hits: 0` |
| Change set contains an `Unknown`-semantics symbol | `unknown` | `RunAllTests` |
| Change set itself is `unknown` | `unknown` | `RunAllTests` |
| No coverage ingested but static imports narrow | `partial` | `RunPackageTests` |
| Empty inventory | `partial` | Selects nothing; does **not** claim `verified` |

Malformed LCOV is an **error**, never a skipped record. Skipping would let the
parser report `Verified` over data it did not read.

---

## 4. Defects found and fixed

### 4.1 A forced full run omitted the mandatory plan summary

`full_run_plan` built its `reasons` vector with only the causal reason, while
the narrowed path appended a `plan_summary` entry. Both are correct Rust; the
inconsistency is only visible when a consumer reads the plan.

Impact: a report driven by `plan_summary` would render nothing on exactly the
runs where "we narrowed 0 of N" matters most — a forced full run. The
schema-conformance suite caught it via
`reason_objects_match_their_declared_contract_and_codes`, because
`schemas/test-selection.schema.json` declares `plan_summary` mandatory.

Fix: extracted `plan_summary_reason(...)` and called it from both paths.

### 4.2 Two clippy findings inside the new code

* `question_mark` — an `if x.is_none() { return None }` in the import resolver.
* `if_same_then_else` — the coverage-state ladder had two branches producing
  `Partial`. Collapsing them also simplified the predicate: because an empty
  `CoverageMatrix` carries `coverage == Unknown`, "no data" and "degraded data"
  correctly collapse into the same answer.

Both fixed; `clippy --workspace --all-targets -- -D warnings` is clean.

### 4.3 A test fixture bug that turned out to be correct behaviour

`step33_crosses_directory_boundaries_when_resolving` initially asserted that
`test/legacy.spec.js` importing `../src/other` resolves to `src/other.ts`, but
the fixture omitted `src/other.ts` from the file set. The resolver correctly
refused to invent a target. Rather than only fixing the fixture, the case was
split into two tests, the second of which
(`step33_resolution_requires_the_target_to_exist`) pins the refusal as intended
behaviour.

---

## 5. Verification telemetry

All commands run from the repository root on `linux`.

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | **PASS** |
| `cargo clippy --workspace --all-targets -- -D warnings` | **PASS** (clean) |
| `cargo test --workspace --all-targets` | **PASS — 342 passed, 0 failed** (was 273) |
| `cargo test --workspace --doc` | **PASS** — 18 targets, 0 failed |
| `./scripts/verify-determinism.sh` | **PASS** — 100/100 byte-identical digests |
| `./scripts/verify-arch-boundaries.sh` | **PASS** — clean, offline, zero sub-process |

New tests contributed by this pillar: **69**

| File | Tests | Covers |
|---|---|---|
| `crates/autopsy-test-impact/tests/inventory.rs` | 20 | Step 33 |
| `crates/autopsy-test-impact/tests/coverage.rs` | 17 | Step 34 |
| `crates/autopsy-test-impact/tests/pillar2_test_impact.rs` | 23 | Steps 35–36 gates |
| `crates/autopsy-test-impact/tests/schema_conformance.rs` | 9 | Schema/struct drift |
| `crates/autopsy-test-impact/tests/common/mod.rs` | — | Shared fixtures |

Determinism is asserted directly, not inferred from the script:

* `step35_plan_is_reproducible_across_100_runs` — 100 iterations, plan and
  BLAKE3 digest byte-identical.
* `step36_ranking_is_reproducible` — 100 iterations.
* `step34_parsing_is_byte_stable_across_repetitions` — 100 iterations.
* `step33_inventory_digest_is_deterministic_and_input_order_independent` —
  shuffled input order, identical digest.
* `step35_plan_does_not_depend_on_change_set_insertion_order`.

Every collection in a serialised output is a `BTreeMap`/`BTreeSet`.

### 5.1 Schema conformance is a real test, not an assertion of intent

The repository's pre-commit `schema-validation` hook only runs
`json.load` over `schemas/*.json`. That does not catch the failure mode that
actually hurts: a schema stays valid JSON forever while the Rust structure it
describes gains, loses, or renames a field.

`tests/schema_conformance.rs` closes that gap by serialising **real** plans and
checking them against the schema on disk. It checks the contract in both
directions (every required field emitted, every emitted field declared), covers
all three `SelectionFallback` variants so the schema's `oneOf` cannot rot, and
pins the semantic promise that `unrulable_out_suites` is empty exactly when
`coverage` is `verified`.

It is deliberately a set of targeted checks rather than a general JSON Schema
engine. A partial validator that silently skipped the hard keywords would be
worse than no validator at all.

---

## 6. Honest scope statement — what is **not** done

Stated plainly so this document cannot be read as a claim of completeness.

1. **Steps 37–45 are not started.** Pillars 3–6
   (`autopsy-contracts-distributed`, `autopsy-telemetry`, `autopsy-mining`,
   `autopsy-catalog`), their CLI subcommands, and the MCP / Web Inspector
   evolution do not exist.
2. **No CLI subcommand.** `autopsy test-select` is not wired. This pillar is a
   **library API only**, reachable from Rust, not from the command line.
3. **No storage write-back.** `test_mappings` exists in migration v2 but nothing
   writes to it yet; the inventory and coverage matrix are in-memory values.
4. **Test discovery is lexical, not parsed.** `autopsy-test-impact` sits below
   the language adapters in the dependency DAG, so it scans text rather than
   using the TypeScript AST. Consequences: `require()` and computed specifiers
   are unbound; JSX-style or heavily wrapped declarations may be missed.
   Mis-recognition is safe (the suite degrades and runs) but it is a real
   limitation, not a stylistic choice.
5. **Only TypeScript/JavaScript test files are recognised.** Python test
   discovery is not implemented despite `autopsy-adapter-python` existing.
6. **`schemas/temporal.schema.json` is still missing** from Pillar 1, tracked in
   `docs/requirements/TRACEABILITY.md` as `NOT DELIVERED`.
7. **`Cargo.toml` version remains `0.0.1`** while `CHANGELOG.md` has had an
   `[0.1.0]` section since 2026-10-08.
8. **`rust-version = "1.85"` in `[workspace.package]` is inaccurate** — already
   before this change the workspace uses let-chains, which require Rust 1.88 in
   the 2024 edition. Pre-existing, out of scope, and not fixed here.
9. **`cargo deny check` advisories still FAIL** on the pre-existing
   `serde_yaml 0.9.34 → unsafe-libyaml` RUSTSEC-2024-0449 advisory. Unrelated to
   this pillar.

---

## 7. Dependency and boundary compliance

* **Zero LLM** — no LLM, network, or presentation dependency. Selection is
  reverse reachability over an explicit graph.
* **No sub-process execution** (`INV-SANDBOX-001..004`) — verified by
  `verify-arch-boundaries.sh`, which greps for `process::Command` across all of
  `crates/`.
* **Read-only** (`FR-026`) — the crate takes file contents by value and returns
  values; it opens no files, spawns nothing, and writes nothing.
* **No new external dependencies.** `autopsy-test-impact` depends only on
  `autopsy-domain`, `blake3`, `serde`, `serde_json`, and `thiserror`, all
  already in the workspace `deny.toml` allow-list.
* **Dependency DAG** — `TestImpact → Domain` only. The crate is imported by
  nothing yet, and nothing in it depends on a presentation layer.

---

## 8. Traceability

| Item | Location |
|---|---|
| Sequence with acceptance gates | `Resource/4.2/04_4.2_IMPLEMENTATION_SEQUENCE.md`, Steps 33–36 |
| Crate map and SQL DDL | `Resource/4.2/02_4.2_REPO_BUILD_CRATES_INTEGRATION.md` |
| Digest formula, exit codes | `Resource/4.2/03_4.2_STANDARDS_SCHEMAS_VERSIONING.md` |
| Requirement table | `docs/requirements/TRACEABILITY.md`, Phase 4.2 |
| Schema | `schemas/test-selection.schema.json` |
| Implementation | `crates/autopsy-test-impact/src/{inventory,lcov,selection}.rs` |
| Tests | `crates/autopsy-test-impact/tests/*.rs` |
