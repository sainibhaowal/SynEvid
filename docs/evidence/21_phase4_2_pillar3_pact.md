# Evidence 21 — Phase 4.2 Pillar 3 (part 3): Pact Consumer/Provider Verification (Step 39)

**Date:** 2026-10-16
**Scope:** Step 39 of `Resource/4.2/04_4.2_IMPLEMENTATION_SEQUENCE.md`.
**Crate:** `crates/autopsy-contracts-distributed` (extended; no new crate)
**Predecessors:** `docs/evidence/19_phase4_2_pillar3_openapi.md` (Step 37), `docs/evidence/20_phase4_2_pillar3_asyncapi.md` (Step 38)

---

## 1. What the step had to prove

Verbatim from the implementation sequence:

> **Step 39** — *Pact Consumer/Provider Verification* — `autopsy-contracts-distributed` —
> **Ingests Pact [R27] JSON artifacts; cross-references consumer expectations against
> provider AST interfaces.**

The gate names two halves, and they are not the same kind of work:

* **Ingest** — read the artifact into a canonical, deterministic model.
* **Cross-reference** — decide, for each consumer expectation, whether the provider's
  declared interface can satisfy it.

Both halves have a silent-failure mode, and neither is the obvious one. They are
covered in §2 and §4.

---

## 2. The design decisions that shape this step

### 2.1 A Pact is not a schema, and every other artifact in this crate is

Steps 37 and 38 read documents that describe **a shape**. A Pact describes **an
exchange**: *this consumer sent exactly this request and observed exactly this
response*. Three consequences follow, and each one would be a wrong answer if it were
missed.

**A request is concrete, not templated.** The provider declares `GET /alligators/{id}`;
the Pact records `GET /alligators/3`. The question is not equality but *subsumption*,
and comparing the two as strings reports every interaction as unmatched — while still
producing a well-formed delta, so the failure is invisible. `pact_xref::subsumes`
therefore works segment by segment.

**`matchingRules` change what "the same request" means.** A Pact that matches a header
against a regex, or a body key against a type, does not describe one request. It
describes a family. This engine does not evaluate those rules, so their presence is
**recorded and degraded** — never evaluated, and never reported as a match.

**The version changes the field shapes.** Pact 1 and 2 write `providerState` (a string)
and a `query` **string**; Pact 3 and 4 write `providerStates` (an array) and a `query`
**object**. Reading a v2 `query` string with the v3 reader yields *zero* parameters,
which turns every required-parameter check into a vacuous pass. Both shapes are
recognised on their own terms, and `the_v2_query_string_and_the_v3_query_object_produce_the_same_parameters`
asserts they converge. A missing `pactSpecification.version` is common in real artifacts
and is a genuine gap, so it degrades coverage rather than being assumed.

### 2.2 The search for a provider operation must filter by method *before* it ranks

This is the step's load-bearing decision, and the first implementation got it wrong.

Consider two declared paths:

```
/a/health   { get }
/a/{id}     { post }
```

A consumer calls `POST /a/health`. The provider **does** serve it — via the template.
But if the search ranks candidate paths by specificity first and only then looks for the
operation, it names `/a/health`, finds no `post` under it, and reports a method problem
that does not exist. That is a false positive, which is worse than a miss: it fails a
correct consumer.

So `match_path` collects only paths that **both** subsume the concrete path **and**
declare the method, then ranks those. Paths that subsume but do not serve the method are
kept separately, and are the only evidence that justifies `PactMethodNotDeclared`.
Dimensions of the search that are asserted directly:

* an exact path beats a template that also fits;
* fewer template segments beat more (`/a/{id}` beats `/a/{id}/{rest}` for `/a/3`);
* equal specificity on **two distinct paths** is an **ambiguity**, not a coin flip
  (`PactAmbiguousProviderMatch`, coverage degraded) — OpenAPI forbids the pair in one
  document, nothing enforces it, and real documents contain it;
* a `{name}` sharing a segment with literal text (`/files/{name}.json`) is treated as
  literal. That under-matches rather than over-matches: the interaction is reported as
  unmatched and a human looks at it.

The match carries the `&Operation` it found rather than letting the caller look it up
again. Looking it up again is exactly where a method-agnostic search turns into a silent
hole (§4.1).

### 2.3 The severity of a body-field finding is decided by the provider's schema

An interaction sends a field the provider's request-body schema does not declare. The
consequence is not a property of the Pact — it is a property of the **schema**:

| Provider schema | Consequence | Severity |
|---|---|---|
| `additionalProperties: false` | the provider rejects the field, so the exchange cannot succeed | `Breaking` |
| anything else | JSON Schema permits additional properties by default | `Unknown` |

`PactUndeclaredBodyProperty` therefore defaults to `Unknown` and the cross-reference
raises it once it has actually seen a closed schema. The same reasoning applies to query
parameters: whether an undeclared parameter is ignored or rejected is provider
behaviour that the interface does not state, so it is disclosed and blocks `Compatible`
rather than being asserted as a break.

### 2.4 "Undeclared" is only a finding when the provider declares *something*

The first version told the provider's story about every parameter it had never
documented. When an operation declares **no** query parameters at all, "undeclared" is
true of everything and therefore says nothing. The engine now degrades coverage and says
once, in prose, that *N* parameters could not be checked — the honest report of a check
that did not happen.

### 2.5 Interaction position is identity; object key order is not

Pact interactions are a sequence. Two interactions may legitimately share a description,
and the array index is then the only thing distinguishing them. Reversing the array is a
**reform**, and `PactDocument::digest` moves. Reordering the *keys* inside an object is
formatting, and the digest does not. Both halves are asserted
(`key_reordering_does_not_change_the_cross_reference`), because the first version of that
test reordered the array and mistook a real difference for a formatting one.

---

## 3. What is modelled, and what is disclosed instead

| Modelled | Disclosed, coverage degraded |
|---|---|
| `consumer` / `provider` (object **and** bare-string form) | `matchingRules` and v2 `{value, matchers}` wrappers — recorded by JSON pointer, never evaluated |
| Interaction `description` and position | Interaction `type` values other than `Synchronous/HTTP` / `Asynchronous/Messages` (for example `Synchronous/Messages` and plugin types) — recorded, skipped, and counted |
| `request.method` (case-insensitive → upper-cased), `path`, `query`, `headers`, `body` | Message `contents` — parsed and fingerprinted, but a *value*, not a schema |
| `response.status`, `headers`, `body` | Any Pact major outside 1–4 |
| `providerStates` **and** v1/v2 `providerState` | A missing `pactSpecification.version` |
| Query in both the v2 string and v3 object form | Response bodies — fingerprinted, never compared to the provider's schema (§6) |

`bindings`-style metadata does not exist in Pact, but the Step 38 rule still governs:
degrade when a keyword could **hide a fact**, never merely because it is unmodelled.
`providerStates` are recorded and named in the notes but do **not** degrade coverage —
they describe the state a provider must be in, which is not a fact about its interface
and cannot hide an interface incompatibility. Degrading for them would make every real
Pact permanently `Partial` for no gain.

---

## 4. Defects found and fixed

Five defects were found before this step was committed. Two were found by the Step 39
integration tests, and one by a test that was itself wrong.

### 4.1 `PactMethodNotDeclared` was unreachable, and a broken consumer produced no finding

The first `match_path` ranked candidate paths **without regard to the method**, then the
caller looked the operation up under the chosen path. When the path existed but the
method did not, the caller fell into a defensive "this should not happen" branch that
added a *note*, degraded coverage, and `continue`d — emitting **no change at all**.

Consequences:

* `PactMethodNotDeclared` was dead code. The `any_method` branch that was supposed to
  emit it could never fire, because a path that subsumes the request was always matched
  by `match_path` first.
* A consumer calling an operation the provider does not offer produced an **empty change
  list**. It was still reported as `Unknown` rather than `Compatible`, so it could not
  approve anything — but it also could not tell the consumer what was wrong.

Found by `schema_conformance`'s new Pact fixture, which asserts a breaking finding and
got `[]`. Fixed by making the search method-aware and returning the operation (§2.2).

### 4.2 A matcher wrapper degraded the *record* but not the *state*

`unwrap_matcher` recorded the wrapper's JSON pointer in `unmodeled_pointers` but left the
interaction's coverage at `Verified`. A Pact 2 artifact whose `path` carried a regex
matcher therefore reported `Verified`, i.e. the engine claimed it had checked one
concrete request when the artifact described a family of them. Found by
`a_matcher_wrapper_on_a_scalar_field_is_unwrapped`; fixed by having `unwrap_matcher`
report the wrapper and the call sites degrade coverage for it.

### 4.3 A method's `Display` was missing, so every report named the method by `Debug`

Caught by the compiler rather than by a test: `format!("{method}")` does not compile for
a type without `Display`. `HttpMethod` now implements it, upper-casing through `as_str`,
so a report says `GET` and not `Get` — matching the spelling Pact files use and a reader
expects to grep for.

### 4.4 A `format!` mixed positional and named arguments

`"…so the {count} parameter(s) sent by `{}`…"` with four positional arguments is one
short. Caught by rustc, fixed by going positional throughout. Recorded because it is the
same class of mistake as the previous step's version-string bug: a message assembled
from the wrong pieces, which no type checks.

### 4.5 A determinism test reordered the array and read a real difference as a formatting one

`key_reordering_does_not_change_the_cross_reference` was written with the interaction
**array** reversed. The digest moved, the test failed, and the correct reading was that
the digest was right and the fixture was wrong: position is identity (§2.5). The fixture
was corrected to reorder object keys only, and a control assertion was added that moving
an interaction between positions *does* move the digest — so the test now pins the
distinction down instead of leaving it implicit.

---

## 5. Verification telemetry

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS (0 warnings) |
| `cargo test --workspace --all-targets` | **543 passed / 0 failed** (was 486, +57) |
| `cargo test --workspace --doc` | PASS (19 doc-test targets) |
| `./scripts/verify-determinism.sh` | PASS — 100/100 byte-identical |
| `./scripts/verify-arch-boundaries.sh` | PASS — no core→MCP edge, no sub-process, no socket |
| `pre-commit run --all-files` | PASS (all 10 hooks) |

Test breakdown for the crate (176 total):

| File | Tests |
|---|---|
| `src/pact.rs` (unit) | 19 |
| `src/pact_xref.rs` (unit) | 11 |
| `tests/schema_conformance.rs` | 12 |
| `tests/step37_openapi.rs` | 40 |
| `tests/step38_asyncapi.rs` | 20 |
| `tests/step39_pact.rs` | 25 |

Plus the shared `src/document.rs`, `src/openapi.rs`, `src/openapi_diff.rs`,
`src/asyncapi.rs` and `src/asyncapi_diff.rs` unit tests (74) counted in the crate total.

### 5.1 Determinism is asserted, not inferred

* `the_cross_reference_is_reproducible_across_100_runs` — same delta and same digest on
  100 consecutive runs.
* `key_reordering_does_not_change_the_cross_reference` — key order in *both* artifacts
  is reversed; the delta is identical.
* `key_reordering_does_not_change_the_digest` — the parsed model's digest is unchanged
  by key order, asserted on the model rather than only through the delta.
* `the_digest_is_reproducible_across_100_runs`.
* `changes_are_canonically_ordered` — the change list is sorted by
  `(kind, subject, detail)`, and the assertion that `alpha` precedes `zebra` proves the
  tie-break is on the detail rather than on map insertion order.
* `the_delta_digest_distinguishes_a_reclassification_from_a_reword` — two deltas agreeing
  on kind and subject but disagreeing on severity have different digests, so a
  reclassification cannot be read as "nothing changed".

`ContractDelta.digest` excludes `notes` (prose, rewordable) and includes `severity`.
`pact_xref::delta_digest` is deliberately **local** rather than shared with the AsyncAPI
engine: a shared builder would make a Pact delta and an AsyncAPI delta collide whenever
the two protocols happened to agree on everything else. Guarded by
`a_pact_delta_never_collides_with_an_asyncapi_delta_that_agrees_elsewhere`.

### 5.2 The schema drift guard now covers 64 kinds

`ChangeKind` grew 53 → **64**: eleven Pact-prefixed variants, each with a documented
default severity and an entry in `schemas/distributed-contract.schema.json`. The
exhaustive comparison in `tests/schema_conformance.rs` compares the Rust enum against the
schema *through serde*, so a `#[serde(rename)]` cannot drift unnoticed, and a fifth delta
fixture (a Pact cross-reference) now exercises the Step 39 kinds against every structural
check in that file.

`ChangeKind::default_severity()` remains a **default**, not the last word: it names what
the kind is, and `PactUndeclaredBodyProperty` is raised to `Breaking` by the
cross-reference once it has seen a closed schema (§2.3).

---

## 6. Honest scope statement — what is **not** done

1. **`matchingRules` are never evaluated.** The engine reports that an interaction
   describes a family and that it did not check the family. Since matching rules are
   ubiquitous in real Pacts, most real artifacts will report `Unknown`. This is the
   largest limitation of the step and it is deliberate: reporting `Compatible` on
   unevaluated matchers would be a claim the engine has not earned.
2. **The provider interface is the parsed OpenAPI model.** The sequence says "provider
   AST interfaces". This crate sits *below* the language adapters in the DAG, so no
   symbol graph is reachable from here, and the declared interface is the honest
   provider-side artifact. Cross-referencing a Pact against a language-level AST surface
   would require the adapters to re-enter this crate's dependency direction.
3. **Message interactions are not cross-referenced.** They are parsed, their contents
   fingerprinted, and they are counted and degraded — never silently skipped — but the
   cross-reference in this step is against an HTTP interface. `verify_flows` (Step 38)
   is the message-side relation.
4. **Response bodies are fingerprinted, not compared.** The provider's response schema
   describes a family and the interaction records one instance, so the meaningful
   comparison is schema ⊇ instance, which needs a JSON Schema validator this engine does
   not carry.
5. **`providerStates` are recorded but not interpreted.** They are what a Pact *verifier*
   needs; they are not interface facts.
6. **Pact plugins and `Synchronous/Messages` are not modelled.** An interaction with an
   unrecognised `type` is recorded and skipped, which degrades coverage, so it can never
   be mistaken for a clean check.
7. **Path templates are matched per segment.** OpenAPI permits a placeholder sharing a
   segment with literal text (`/files/{name}.json`); such a path is treated as literal and
   under-matches.
8. **The engine is library-API only.** No CLI subcommand and no storage write-back to
   `distributed_contracts` yet; both are covered by later steps in the sequence.

---

## 7. Boundary compliance

* **Zero LLM in correctness crates** — no LLM call, import, or network path. All
  classification is a deterministic match over parsed structures.
* **Read-only analyzer (FR-026)** — both artifacts are read through `SpecDocument`; the
  engine never writes to the source, the Pact, or git.
* **100/100 determinism (NFR-001, FR-028)** — every collection is a `BTreeMap` or
  `BTreeSet`; candidate ranking is a total order with an explicit
  specificity-then-lexicographic tie-break; digests are built from the parsed model, never
  from source bytes.
* **Honest coverage (FR-014, FR-030)** — no unsupported construct is converted into a
  pass. Every unmodelled construct degrades coverage, and `Compatible` additionally
  requires the absence of any `Unknown`-severity finding.
* **Architectural inversion** — `autopsy-contracts-distributed` depends on
  `autopsy-domain` only; no new dependency was added for this step.
* **No new dependencies** — the crates' existing `serde`, `serde_json`, `blake3`,
  `thiserror` set covers everything used here.

---

## 8. Traceability

| Requirement | Where |
|---|---|
| FR-014, FR-030 (honest coverage) | `pact.rs` degradation of `matchingRules`, missing version, unknown `type`, unmatched-method; §3 |
| FR-026 (read-only) | `SpecDocument` reads only; no write path exists |
| FR-028 / NFR-001 (determinism) | §5.1 |
| R27 (Pact artifacts) | `src/pact.rs`, `src/pact_xref.rs` |
| Step 39 gate (ingest + cross-reference) | `tests/step39_pact.rs` — 25 tests |
| Schema contract | `schemas/distributed-contract.schema.json`, `tests/schema_conformance.rs` |
