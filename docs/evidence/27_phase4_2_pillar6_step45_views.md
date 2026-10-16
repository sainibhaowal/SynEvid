# Evidence 27 — Phase 4.2 Pillar 6 (part 3): MCP Tools & Web Inspector Evolution (Step 45)

**Date:** 2026-10-16
**Scope:** Step 45 of `Resource/4.2/04_4.2_IMPLEMENTATION_SEQUENCE.md`.
**Targets:** `apps/mcp-server` (4 new read-only tools), `apps/web-inspector` (two views),
`crates/autopsy-cli` (one new subcommand), `crates/autopsy-catalog` (one defect fixed)
**Predecessor:** `docs/evidence/26_phase4_2_pillar6_cli.md` (Step 44)

---

## 1. What the step had to prove

> **Step 45** — *MCP Tools & Web Inspector Evolution* — `apps/mcp-server`,
> `apps/web-inspector` — Exposes 4 new MCP read-only tools; adds Temporal Timeline &
> Distributed Blast Radius views to Web Inspector.

The step does not name the four tools, so the naming is this step's decision and is
recorded in §2. The two views are the design's §6.2.2 *Temporal Evolution Timeline* and
§6.2.3 *Cross-Repo Blast Radius*.

---

## 2. The four tools, and why these four

| Tool | Engines behind it | Why it is the tool a client wants |
|---|---|---|
| `autopsy_temporal` | Pillar 1 (revision journal, `at` reconstruction) and Pillar 5 (mined co-change) | "What changed, and when" and "what changes together" are one question asked two ways; the `view` selector names which engine answered, and each view keeps its own report and digest. |
| `autopsy_test_selection` | Pillar 2 | The action a change set implies: which tests must run. |
| `autopsy_contract_diff` | Pillar 3 (all four protocols) | The action a contract change implies: is this breaking? |
| `autopsy_distributed_blast_radius` | Pillar 6 (catalog ownership) and Pillar 4 (observed topology) | The design's cross-repo blast radius *is* that composition: what the catalog declares (owners, siblings, APIs, dependents) and, when a trace export is supplied, which of it was actually observed. The response names which engine produced each half. |

Two engines are therefore reachable through one tool in two cases. That is deliberate rather
than convenient: the alternative was a tool that returned a blast radius with no ownership
context, or a temporal tool that could answer only half of "what changed and what moves with
it". In both cases the response says which engine produced which section, and the two exit
codes are reported separately when both ran.

**Every tool is read-only.** They construct `autopsy` argv, run the binary, and return what
it said — including its exit code, which is part of the answer rather than transport
metadata:

* `0` → actionable;
* `2` → a policy gate failed (a breaking contract change, `--fail-if-all-tests-required`);
* `4` → **not actionable**: coverage unknown or unsupported (or partial under `--strict`);
* `3` → the analysis could not be performed.

`evidenceEnvelope` computes `actionable` in one place, and `cli.ts` now treats `4` as
*evidence* rather than a transport error. Before this step the MCP layer understood `0` and
`2` only, so every Step 44 refusal would have surfaced as a failed call with the refusal
buried in stderr.

---

## 3. The defect this step found (and why it was found here)

Building the blast-radius view meant loading a real catalog with **bare names** in its
relationship fields — which is how real Backstage catalogs are written:

```yaml
owner: team-x
system: checkout-system
providesApis: [checkout-api]
```

The engine read every bare name as a **component**:

```text
owner: team-x         -> component:default/team-x      (resolves to nothing)
system: checkout-system -> component:default/checkout-system (ditto)
providesApis: [x]     -> component:default/x           (ditto)
```

Nothing failed. The entities parsed, the owner was present, the catalog reported
`coverage: verified` — and every join *across* entities silently did not happen. A
`Group` owner never matched a `Group`, a `System` never matched a `System`, so
co-ownership was always empty and siblings were keyed on a fabricated reference. It could
not be caught by the Step 43 tests because those fixtures used fully qualified references
(`group:default/team-x`) — the spelling that *works*.

The implied kind is a property of the **field**, not of the string, and it is now threaded
per field (Backstage's own rule): `owner` → `group`, `system` → `system`, `domain` →
`domain`, `providesApis`/`consumesApis` → `api`, `dependsOn`/`subcomponentOf` → `component`,
and the CLI's entry point → `component`. Two engine tests now pin this, including that an
explicit kind in an owner reference still wins (`user:default/ravi` is a person, and
`owner_is_team` is `false`).

The same investigation produced a second, smaller correction: a **modelled kind that is not
a component** (a `System`, `Domain`, `API` or `Resource`) now reports
`coverage: unsupported` with a note — "an organisational blast radius is defined for
components" — instead of an all-empty radius that reads as *"this system touches nothing"*.

---

## 4. The `blast-radius` subcommand

The traversal needed one implementation, not two: a viewer that walked `entities` in
JavaScript would be a second answer to the same question, and the first time the two
disagreed there would be no way to tell which was right. So the CLI gained
`autopsy blast-radius <component-ref>` and the viewers render its output.

The output document is `{ catalog, blast_radius }` **on every path**, including the one
where nothing is found. Emitting only the radius would leave every answer unverifiable: the
radius is a *view of those entities*, and a reader has to be able to see them. Exit codes
follow Step 44's rule: `0` for a verified radius, `4` for `unknown` (absent component) or
`unsupported` (not a component), `4` under `--strict` for `partial`, `3` for a reference
that is not a reference.

`schemas/blast-radius.schema.json` is closed and declares exactly the fields the engine
emits — asserted as **set equality in both directions** (schema properties vs emitted
fields, and `required` vs emitted fields), because a schema that lags a struct is worse than
no schema.

---

## 5. The Web Inspector views

The page stays what it was: one static HTML file, zero build, zero backend, no network API,
no storage write, no clock. Two tabs were added:

**Temporal Evolution Timeline.** A slider over the journal's revisions, with facts that are
active at each stop, what was added, what was retired, and a badge on any fact that did not
fully resolve. Two honesty decisions:

1. *Revision order is first appearance of `valid_from` in the journal*, and the caption says
   so. The journal carries commit ids, **not timestamps** — the engine deliberately does not
   date a history — and the viewer will not put a clock on it.
2. A bare fact array is accepted, and then the page says that the envelope's `coverage` and
   granularity `notes` *are not in this document*. It does not invent them from the facts.

**Distributed Blast Radius.** A radial multigraph of the stated references — owner,
co-owned, siblings, provided APIs, consumed APIs, dependents — plus the unresolved
references rendered **dashed and named, never as edges**, the catalog-level
"what the scan did not read" (unmodelled kinds, refusals by reason, duplicate references),
and, when the MCP tool correlated a trace export, the observation summary with its
`actionable` flag. A component the catalog does not contain renders **no graph** and says
the omission is a refusal to answer, not an empty answer.

The page refuses any other document with a message naming the three artifact kinds it does
render, rather than showing an empty case.

---

## 6. Honesty boundaries kept

* **No re-derivation in the UI.** Coverage states, notes, unresolved references, owner
  semantics, observation windows and dispatch models are rendered as the engines reported
  them. The two things the page *does* derive — revision order and per-revision activity —
  are stated in the caption, and both are the engine's own definition (`opened_at ≤ k` and
  retired after `k`).
* **Unresolved is not an edge.** Asserted in the headless render tests, because this is the
  exact place a viewer would quietly promote "referenced but unknown" into "connected".
* **No salt default in the MCP tool.** `autopsy_distributed_blast_radius` refuses a trace
  export with no `salt_hex` and says why: a default salt would make pseudonyms comparable
  across repositories and therefore forgeable.
* **Exit codes are not swallowed.** `4` reaches the client as `actionable: false` with the
  report attached.
* **Read-only everywhere.** The MCP tools spawn the read-only CLI; the inspector is a static
  file. A source guard asserts the page contains no `fetch(`, `XMLHttpRequest`,
  `localStorage.setItem`, `Date.now(` or `new Date(`.

---

## 7. A latent CI/hook defect, fixed

`apps/mcp-server`'s tests import `../dist/*.js`, `dist/` is gitignored, and the hook ran
`npm run typecheck && npm run lint && npm test` with **no build step**. The hook passed only
on machines that happened to have a stale `dist/` from an earlier manual build; on a fresh
checkout it would have failed for reasons unrelated to the change. `pretest` now builds. The
Web Inspector's new headless tests are wired into pre-commit (`web-inspector-check`), the
`Makefile` (`web-inspector-check`, in `check`) and CI (`mcp-server-check` gained the step and
is now named "TypeScript MCP Server & Web Inspector Checks").

---

## 8. Verification telemetry

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS (0 warnings) |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | PASS |
| `cargo test --workspace --all-targets` | **814 passed / 0 failed** (was 803, +11) |
| `cargo test --workspace --doc` | PASS |
| `./scripts/verify-determinism.sh` | PASS — 100/100 byte-identical |
| `./scripts/verify-arch-boundaries.sh` | PASS |
| `pre-commit run --all-files` | PASS (all 12 hooks, one of them new) |
| `cd apps/mcp-server && npm run typecheck && npm run lint && npm test` | PASS (14 tests, was 11) |
| `cd apps/web-inspector && npm test` | PASS (11 tests, new) |

| Test | Count | What it pins |
|---|---|---|
| `crates/autopsy-cli/tests/step45_blast_radius_cli.rs` | 9 | the joins, the two refusal paths, byte-identical output, schema set-equality, library parity |
| `crates/autopsy-catalog/src/backstage.rs` (new) | 2 | the implied kind per field, and that an explicit kind still wins |
| `apps/mcp-server/test/server.test.js` (new) | 3 new | the envelope's exit-code rule, a real blast radius through the binary, and exit 4 as evidence |
| `apps/web-inspector/test/inspector.test.js` (new) | 11 | both views, both refusal paths, the unresolved-is-not-an-edge rule, read-only/time-free |

The inspector's tests run the page's **own script** — extracted from `index.html` and
executed in a `vm` context with a minimal DOM stub — so the single zero-build file stays the
single source of truth and a test can never pass against a copy of it.

---

## 9. Defects found and fixed

1. **The implied kind of a bare reference was always `component`** (§3) — the defect that
   made ownership, systems and APIs silently unjoinable, invisible in every output because
   each reference was still well-formed.
2. **A non-component entry produced an empty radius** (§3) — now `unsupported` with a note.
3. **The MCP layer treated exit `4` as a transport failure** (§2).
4. **`npm test` needed a `dist/` nobody built** (§7).
5. **The inspector's inline helpers would have been `export`ed inside the server factory** —
   caught by `tsc` immediately; `evidenceEnvelope` and `asText` moved to `src/evidence.ts`,
   which also made them directly testable.
6. **A test asserted a marker in the wrong element** (`NOT IN THE CATALOG` lives in the tab's
   summary line, not in the graph markup) — the test was wrong, not the page, and it now
   asserts both.

---

## 10. Honest scope statement

1. **A co-change report has no view.** The design's timeline is the revision journal;
   `autopsy cochange` output is a statistical artifact and is not rendered. The step's gate
   names the temporal timeline and the distributed blast radius, and both exist.
2. **The temporal envelope carries no digest.** `TemporalJournal` exposes no digest entry
   point (its canonical projection is), so the inspector's digest card says *"not carried by
   this envelope"* rather than hashing the facts in JavaScript with a second implementation.
   Adding a journal digest is Pillar 1 work.
3. **The journal is file-level** (`TreeExtractor`): the CLI note travels into the page, and
   the page shows it. Symbol-level extraction is still unwired.
4. **No symbol-level ownership join in the radius** — a `Group` is outside the five modelled
   entity kinds, so ownership rolls up only as far as the reference; the page says this under
   the graph.
5. **The inspector cannot fetch artifacts** from disk or URL (a `file://` page cannot, and a
   read-only page should not); artifacts are loaded by file picker, drag-and-drop or paste.
6. **`autopsy_distributed_blast_radius` reads paths on the server's filesystem** for the
   trace export and the surface document. That is the same contract every existing MCP tool
   has (they run a local CLI), and it is stated in the parameter descriptions.
7. **No storage write-back, before or after this step.** Nothing in Phase 4.2 persists.
8. **The 4-tool count and its two composite tools are this step's decision**, recorded in §2
   because the sequence does not name the tools.

---

## 11. Boundary compliance

* **Zero LLM in correctness crates** — the MCP layer is argv construction and rendering; the
  inspector is a static file with no model call and no network.
* **Read-only analyzer (`FR-026`)** — §5's source guard; no command writes.
* **Determinism (`NFR-001`, `FR-028`)** — engine-side digests unchanged; the radius output is
  byte-identical across ten runs (asserted); the inspector derives no digest at all.
* **Honest coverage (`FR-014`, `FR-030`)** — §6. The three constructs that would have turned
  `UNKNOWN` into a plausible answer (an unresolved edge, an absent component and an
  unstated envelope) are each refused and each asserted.
* **Architectural inversion (`ARCH_NO_CORE_TO_MCP`)** — the new CLI subcommand depends on
  `autopsy-catalog`; the MCP server and the inspector depend on the CLI's output, not on
  core crates. `verify-arch-boundaries.sh` passes unchanged.
* **Supply chain** — no new external dependency: `apps/web-inspector` gained a test script
  and no package; the MCP server gained a source file.

---

## 12. Traceability

| Requirement | Where |
|---|---|
| Step 45 gate — 4 new MCP read-only tools | `apps/mcp-server/src/server.ts` (`autopsy_temporal`, `autopsy_test_selection`, `autopsy_contract_diff`, `autopsy_distributed_blast_radius`); registration asserted as exactly 9 tools |
| Step 45 gate — Temporal Timeline view | `apps/web-inspector/index.html` (`renderTemporal`, `paintTimeline`); `test/inspector.test.js` |
| Step 45 gate — Distributed Blast Radius view | `renderBlastRadius`, `radialGraph`, `unmodelledHtml`, `topologyHtml`; `test/inspector.test.js` |
| Design §6.2.2 (additions and removals across commits) | §5.1; `gate_a_verified_radius_…`'s sibling test `the slider shows what is active at a revision, and moves` |
| Design §6.2.3 (cross-service consumer impacts) | §5.2; `autopsy blast-radius` + `dependents`; `a blast radius draws only stated references, and never an unresolved one as an edge` |
| `FR-014`, `FR-030` (never `UNKNOWN` → `PASS`) | §2 (`actionable`), §5.2 (dashed unresolved), §6 |
| `FR-026` (read-only) | §6 (`the page stays read-only and offline`) |
| `NFR-001`, `FR-028` (determinism) | `gate_the_radius_output_is_byte_identical_across_ten_runs` |
| Pillar 6 defect fix (implied kinds) | `crates/autopsy-catalog/src/backstage.rs` (`the_implied_kind_of_a_bare_reference_comes_from_its_field`, `an_explicit_kind_in_a_bare_owners_reference_still_wins`) |
| Schema currency | `schemas/blast-radius.schema.json`; `gate_the_schema_declares_exactly_what_the_engine_emits` |
| Dynamic-maintenance invariant (AGENTS.md §4) | `.pre-commit-config.yaml` (`web-inspector-check`), `Makefile`, `.github/workflows/ci.yml`, `.agents/rules/agent-engineering-rules.md` |
