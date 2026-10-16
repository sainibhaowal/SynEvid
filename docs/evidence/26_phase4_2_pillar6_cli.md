# Evidence 26 — Phase 4.2 Pillar 6 (part 2): CLI Subcommands & Exit-Code Integration (Step 44)

**Date:** 2026-10-16
**Scope:** Step 44 of `Resource/4.2/04_4.2_IMPLEMENTATION_SEQUENCE.md`.
**Crate:** `crates/autopsy-cli` (`src/extensions.rs` — new module)
**Predecessor:** `docs/evidence/25_phase4_2_pillar6_catalog.md` (Step 43)

---

## 1. What the step had to prove

> **Step 44** — *CLI Subcommands & Exit Code Integration* — `autopsy-cli` — Adds `temporal`,
> `test-select`, `contract-dist`, `telemetry`; honors deterministic exit codes (0, 2, 3, 4).

Six subcommands were added, not four: the two extra ones (`cochange`, `catalog`) are the
engines of Steps 42 and 43, which until now had **no** caller outside their own tests. The
table in §9 of Evidence 24 and §11.8 of Evidence 25 recorded that gap; this step closes it,
because a Pillar with no CLI reach is a Pillar a user cannot run.

---

## 2. The exit-code rule, in one function

`coverage_exit(strict, coverage)` is the only place the rule lives:

| Code | Meaning here |
|---|---|
| `0` | the command produced an answer it can stand behind |
| `2` | a policy gate failed: a `Breaking` **contract** verdict, or `--fail-if-all-tests-required` when the selection could not narrow |
| `3` | the analysis could not be performed: bad arguments, unreadable input, a malformed document, a missing salt |
| `4` | the answer is **not actionable**: coverage `Unsupported`/`Unknown`, or `Partial` under `--strict` |

Three decisions inside that table are the ones worth reviewing:

1. **`Unknown` exits `4` without `--strict`.** The spec's phrasing is "unknown coverage in
   strict mode", and the narrower reading is wrong in the direction that matters: an engine
   that says *"I cannot answer this"* must not hand a CI script a zero and let it proceed.
   `--strict` extends the same refusal to `Partial`, where the answer is incomplete rather
   than absent. This is the CLI half of `FR-014`/`FR-030` — never convert `UNKNOWN` into
   `PASS` — and it is asserted for every coverage state in both strictness modes.
2. **`Breaking` exits `2` without being asked.** A positive finding that a consumer can break
   is a policy failure at any strictness; exiting `0` would bury it in a pipeline that checks
   only the status.
3. **`4` is not an error.** "No commits to replay", "the history does not fit the bound",
   "the tree was empty" and the journal's own unresolved-fact counts are all *refusals to
   answer*, not failures of analysis. `3` would claim the run went wrong, which is a
   different statement and a different remediation.

---

## 3. Every command is a read of an engine, and nothing else

The module calls public APIs and nothing more. No engine gained a CLI-shaped back door, no
output is written to disk, `.autopsy/` storage is untouched, and no command reads a clock:

* The mining window anchors on the newest commit's timestamp **from the repository**
  (`with_reference_timestamp` is not used), exactly as Step 42 decided.
* A source guard test asserts that `SystemTime`, `Instant::`, `now()`, `chrono` and `Utc`
  appear **nowhere** in the extension layer. The engines keep clocks out of their digests; the
  layer that calls them must not put one back in.
* JSON output is `serde_json` over `BTreeMap`-backed values, so field order is a function of
  the field name. Two runs of the same command produce identical bytes, asserted for the
  mining report across 100 runs and for the catalog report across 10.

`SARIF` is **refused** rather than approximated: SARIF describes findings, and none of these
reports is one. Silently emitting an empty SARIF document would be the worst option available.

---

## 4. The honesty of each command is the engine's own state, not a summary

| Command | What it must not overstate | How the CLI handles it |
|---|---|---|
| `temporal` | `TreeExtractor` records file presence, not symbols | the note is in the **text and the JSON**; coverage is derived from the journal's own `partial`/`unknown` fact counts, not a blanket `verified` |
| `test-select` | the change set is file-level; symbols were not resolved | `ChangeSetSummary::coverage = Partial` and the plan inherits it; `NoReachability` is the engine's identity implementation, so the selection cannot be widened beyond what was named |
| `contract-dist` | a verdict is not a recommendation | `Breaking` → `2`; `Unknown` → `4` under `--strict`; `Compatible` is only reachable with `verified` coverage and that equality is asserted |
| `telemetry` | a pseudonym is not a name | the salt is **required**, 64 hex characters or a 32-byte file, never defaulted, never echoed, and the text says the salt is not recorded |
| `cochange` | co-occurrence is not causality | every edge's `claim` is asserted to be `temporal_co_occurrence_only` and its `kind` `statistical_co_change`; the text says "not a dependency graph" |
| `catalog` | a requirement is not a version | the report's own note reaches the text output; a workspace `path` dependency is asserted to be reported as `path`, not as a registry edge |

The journal's coverage deserves its own line. `--strict` on a journal that carries unresolved
facts must fail, otherwise the flag passes over exactly what the engine flagged. So
`unknown` facts ⇒ `unknown`, `partial` facts ⇒ `partial`, and only a journal whose facts all
resolved is `verified` — a three-branch mapping rather than a constant.

---

## 5. Repository plumbing for the two history engines

`.git` is not always a directory, and the two cases are one `fs` read away:

* a **worktree or submodule** has `.git` as a *file* containing `gitdir: <path>`;
* a worktree's gitdir in turn holds `commondir`, and the objects live in the **common**
  directory.

Refusing either would make both history commands unavailable in exactly the setup a reviewer
checking out a branch is in. Both are handled, and a directory that is not a repository is a
`3` with the path named.

---

## 6. The history bound is a refusal, not a truncation

`TemporalJournal::build` returns `TimelineTooLarge` when the reachable history does not fit
the bound. That is Pillar 1's decision: a truncated timeline is not a timeline, and silently
dropping the oldest commits would move every *active at revision* answer.

The CLI maps that variant to `4` with a message naming the flag, rather than letting it fall
through to `3` with a bare engine error — and the default `--max-commits` was raised to
`25_000` after a probe showed a 31-commit repository failing a default of `500` for a reason
the user cannot act on from the message alone.

Mining is the opposite case, and the difference is deliberate: its window walk **truncates**
and records `walk_truncated`, because a window is a window.

---

## 7. Testable exit codes

Handlers return `Outcome { exit_code, json, text }` and **never touch the terminal**;
`run_cli` prints. That is what lets 30 integration tests assert an exact exit code and the
exact bytes for it without a pty. `Outcome::print` is the only place formatting happens.

---

## 8. Verification telemetry

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS (0 warnings) |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | PASS |
| `cargo test --workspace --all-targets` | **803 passed / 0 failed** (was 771, +32) |
| `cargo test --workspace --doc` | PASS |
| `./scripts/verify-determinism.sh` | PASS — 100/100 byte-identical |
| `./scripts/verify-arch-boundaries.sh` | PASS |
| `pre-commit run --all-files` | PASS (all 11 hooks) |

| File | Tests |
|---|---|
| `src/extensions.rs` (unit) | 2 |
| `tests/step44_cli_extensions.rs` | 30 |

Two fixtures are worth naming: **this repository** is the git fixture for `temporal` and
`cochange` (the values asserted are invariants — a digest stable across runs, a population
that is accounted for — rather than constants, because the history grows and a constant would
be a test edited on every commit), and **nothing** — an empty tree, an empty `.git`, a missing
salt, a malformed document — is the fixture for the error paths, where exit-code contracts are
usually wrong.

---

## 9. Defects found and fixed

1. **`TemporalJournal::build` refuses a history larger than the bound**, which the CLI's
   default of `500` turned into an actionable-looking `3` on any real repository. Found by
   running the command rather than only its tests. Now: `4`, naming `--max-commits`, with the
   default raised.
2. **The journal's canonical projection is a JSON array**, not an object, so the CLI's
   `stats` insertion silently did nothing (`if let Some(object) = value.as_object_mut()`).
   The CLI now wraps it in a named `journal` field with `stats`, `coverage` and `notes` as
   declared siblings — self-describing output instead of an anonymous array.
3. **The `compatible` fixture was not hitting the `compatible` branch.** The documents were
   declared OpenAPI `3.1.0`, which the engine parses under 3.2 semantics with a recorded gap,
   so coverage was `partial` and the verdict `unknown` — correctly. The fixture now declares
   `3.2.1` and the test asserts *both* the verdict and that a `compatible` verdict implies
   `verified` coverage, which is the rule that makes the fixture meaningful.
4. **Eight positional arguments to `telemetry`**, two of them adjacent booleans
   (`complete_observation`, `dispatch_static`) that are trivially transposable and have
   opposite meanings. Refactored to `TelemetryArgs` with the conservative default; the salt
   is the one field the default cannot supply.
5. **A `to_string` on a trimmed string** and an `assert_eq!(x, false, …)` — clippy's `-D
   warnings` gate, again before the commit.

---

## 10. Honest scope statement

1. **`test-select` runs with `NoReachability`.** The CLI has no symbol-level change set, so
   the selection is the conservative one: it cannot widen beyond the affected files, and the
   plan is `partial` by construction. The engine has the trait and the ranking sidecar; a
   future caller with a symbol table will do better, and the CLI must not pretend to now.
2. **No `topology` command of its own.** The topology report is reached through
   `telemetry --surface <file>`; `DeclaredSurface` is supplied, never parsed, exactly as Step
   41 decided. There is no route extraction from contracts on the CLI's behalf.
3. **No symbol-level temporal extraction** — Pillar 1's extractor is `TreeExtractor`
   (file presence). Wiring the language adapters into `SourceExtractor` is unstarted work, and
   both the text and the JSON say so.
4. **`contract-dist` compares one document pair per invocation.** Cross-repository producer
   and consumer verification needs two documents that name each other, which is Pillar 3's
   documented gap, not this step's.
5. **No `--out` flag.** Every command prints; nothing writes. `FR-026` is trivially satisfied
   by having nowhere to write to.
6. **`catalog` scans one tree**, and the symbol-level ownership join remains the caller's, as
   Evidence 25 §2 recorded.

---

## 11. Boundary compliance

* **Zero LLM in correctness crates** — argument parsing and engine calls.
* **Read-only analyzer (`FR-026`)** — no write, no sub-process, no socket.
* **Determinism (`NFR-001`, `FR-028`)** — §3.
* **Honest coverage (`FR-014`, `FR-030`)** — §2, §4.
* **Architectural inversion (`ARCH_NO_CORE_TO_MCP`)** — the CLI depends on engines; no engine
  depends on the CLI. `verify-arch-boundaries.sh` passes unchanged.
* **Supply chain** — no new *external* dependency: the six new edges are workspace crates.

---

## 12. Traceability

| Requirement | Where |
|---|---|
| Step 44 gate (`temporal`, `test-select`, `contract-dist`, `telemetry`) | `Commands::{Temporal, TestSelect, ContractDist, Telemetry}` in `src/lib.rs`; `src/extensions.rs` |
| The two engines the gate did not name (`cochange`, `catalog`) | `Commands::{Cochange, Catalog}`; §1 |
| Deterministic exit codes 0/2/3/4 | §2; `tests/step44_cli_extensions.rs` (`gate_an_unknown_verdict_is_an_answer_without_strict_and_a_refusal_with_it`, `gate_a_breaking_contract_change_exits_two_without_being_asked`, `gate_coverage_state_never_becomes_a_pass_through_the_cli`, `gate_the_exit_code_constants_are_the_documented_ones`) |
| `FR-014`, `FR-030` | §2 decision 1, §4 |
| `FR-026` read-only | §10.5, §11 |
| `NFR-001`, `FR-028` | §3; `gate_a_history_report_is_byte_identical_across_one_hundred_runs`, `gate_a_catalog_scan_is_byte_identical_across_runs`, `gate_no_command_in_this_module_reads_a_clock` |
| Pillar 1 acceptance (`temporal`) | `gate_the_history_engines_work_against_a_real_repository`, `gate_a_history_that_does_not_fit_the_bound_is_unsupported_and_says_what_to_pass`, `gate_the_journal_coverage_comes_from_the_facts_that_did_not_resolve` |
| Pillar 2 acceptance (`test-select`) | `gate_a_pact_document_that_is_not_a_pact_is_an_error_not_an_empty_diff` and the selection assertions in `src/extensions.rs` |
| Pillar 3 acceptance (`contract-dist`) | `gate_a_breaking_contract_change_exits_two_without_being_asked`, `gate_a_compatible_change_exits_zero_and_says_only_that_it_found_nothing_breaking`, `gate_an_openapi_2_document_is_refused_rather_than_read_as_3_x`, `gate_asyncapi_flow_verification_runs_from_one_document` |
| Pillar 4 acceptance (`telemetry`) | `gate_the_telemetry_commands_refuse_to_run_without_a_salt`, `gate_a_salt_that_is_not_thirty_two_bytes_is_refused`, `gate_an_ingested_export_redacts_the_email_and_reports_a_digest`, `gate_a_topology_report_correlates_an_observed_route_against_a_declared_one` |
| Pillar 5 acceptance (`cochange`) | `gate_every_mined_edge_is_tagged_as_temporal_co_occurrence`, `gate_a_repository_with_no_commits_is_unsupported_rather_than_an_error` |
| Pillar 6 acceptance (`catalog`) | `gate_a_catalog_scan_of_this_workspace_finds_the_manifests_and_the_ownership_gaps`, `gate_an_empty_tree_is_unknown_and_exits_four`, `gate_a_catalog_root_that_does_not_exist_is_an_analysis_error` |
