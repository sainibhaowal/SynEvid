# Evidence 19 — Phase 4.2 Pillar 3 (part 1): OpenAPI 3.2.1 Contract Engine (Step 37)

**Date:** 2026-10-16
**Scope:** Step 37 of `Resource/4.2/04_4.2_IMPLEMENTATION_SEQUENCE.md`.
**New crate:** `crates/autopsy-contracts-distributed`
**Predecessors:** `docs/evidence/17-temporal-semantic-graph.md` (Steps 28–32),
`docs/evidence/18-predictive-test-selection.md` (Steps 33–36)

---

## 1. What the step had to prove

Verbatim from the implementation sequence:

> **Step 37** — OpenAPI 3.2.1 Contract Engine — Parses OpenAPI 3.2.1 paths/schemas;
> flags breaking changes (removed endpoint, added required query param).

The two named examples are both **request-side surface** losses: an operation that
no longer exists, and a parameter that existing callers do not send. The gate is
satisfied directly and specifically, but the interesting engineering is in what
happens when the engine *cannot* tell.

---

## 2. The design decision that shapes the whole pillar

A linter that fails to parse a file produces a visible gap. A **compatibility
checker** that fails to parse a file produces the answer **"compatible"** — which
reads as an approval. The failure modes are not symmetric, and treating them as
if they were is how a breaking-change gate becomes decorative.

The verdict is therefore computed as:

```text
breaking   if any change classified `breaking` was found
unknown    else if any change could not be classified, or coverage != verified
compatible otherwise
```

Both halves of that are load-bearing:

1. **A positive finding needs no completeness.** The removed endpoint is breaking
   whether or not the rest of the document was modelled. Degrading coverage must
   never *silence* a real break, so `breaking` does not depend on `coverage`.
2. **A negative finding does.** `compatible` is the only verdict that asserts
   nothing was missed, so it is the only verdict gated on `coverage == verified`
   (`FR-014`, `FR-030` — `UNKNOWN` must never become `PASS`).

`CompatibilityVerdict::is_approval()` exists specifically so no consumer writes
`verdict != Breaking` by hand, which would silently promote `unknown` to a pass.
The schema-conformance suite asserts that `is_approval()` is true for exactly one
variant.

Direction is respected separately. Request-side and response-side schemas have
opposite arrows: narrowing a request parameter's accepted type rejects payloads
that used to be accepted (breaking), whereas a changed **response** schema has no
directional evidence available to this engine at all. That case is reported as
`response_schema_changed` with severity `unknown` rather than guessed at — it
blocks a `compatible` verdict without asserting a break.

---

## 3. What is modelled, and what is disclosed instead

| Surface | Modelled | Notes |
|---|---|---|
| `paths` and HTTP methods | ✅ | all eight methods |
| Path / query / header / cookie parameters | ✅ | keyed on `(name, in)` |
| Parameter requiredness | ✅ | path parameters forced required per spec |
| Parameter types and `enum` | ✅ | direction-aware narrowing/widening |
| Request body requiredness | ✅ | added-optional vs added-required distinguished |
| Request body media types | ✅ | keyed by media type, not collapsed |
| Request body object properties + `required` | ✅ | recursive |
| `additionalProperties: false` | ✅ | introduced = breaking |
| Response status sets | ✅ | removed = breaking |
| **Response body schema changes** | ⚠️ | detected, severity `unknown` |
| **`allOf` / `oneOf` / `anyOf` / `not` / `if`-`then`-`else`** | ❌ | recorded by JSON pointer, subtree degraded |
| **`patternProperties`, `dependentSchemas`, `prefixItems`, `discriminator`, …** | ❌ | recorded by JSON pointer, subtree degraded |
| **External (`./file#/…`) references** | ❌ | reported, whole document degraded |
| **OpenAPI 3.1.x / 3.0.x** | ⚠️ | parsed under 3.2 semantics, `partial` + note |
| **OpenAPI 2.0 (Swagger) or 4.x** | ❌ | hard error, not a lenient parse |

The bottom row matters more than it looks. Parsing a Swagger 2.0 document
leniently would yield an empty path set — and the diff would then report **every
endpoint of the other side as removed**. A garbage diff that says "everything
broke" is as useless as one that says "nothing broke", so an unsupported
specification version is refused outright.

For the composition keywords, every occurrence is named by JSON pointer in
`unmodeled_pointers` and degrades that subtree, which in turn prevents a
`compatible` verdict. This is deliberately pessimistic: almost every real
specification uses composition somewhere, so a realistic run reports `partial`
and `unknown` unless it finds a definite break. That is the honest answer. A
composed schema whose constraints moved between branches is genuinely invisible
to this engine, and claiming otherwise would be the defect the requirement exists
to prevent.

`SpecDocument::parse` also **rejects YAML merge keys (`<<:`)** rather than parsing
around them. `serde_yaml` does not apply merge keys, so a document using them
parses with every inherited field silently absent — and a compatibility check
would then report those fields as *removed*. The detection is a line-based scan
and can therefore refuse a document that merely contains `<<:` inside a block
scalar. That asymmetry is deliberate and stated in the code: refusing to analyse
is recoverable; a confident wrong answer is not.

---

## 4. Defects found and fixed

Four of the five were found by tests, three of them by tests failing — which is
the only reason to write tests that could plausibly fail.

### 4.1 `Document::pointer` refused array-index descent

The public pointer resolver was written to address object members only, with a
comment arguing that "an index is a position, not an identity". That reasoning
belongs to entity identity (RFC-GLOBAL-001), not to JSON Pointer. The practical
effect was that **every legal JSON Pointer addressing a list element failed to
resolve** — including `#/paths/…/parameters/0/schema`, which is on the hot path
for essentially every OpenAPI document with a parameter.

Consequence: `doc.pointer(...)` returned `None`, the schema reader marked the
subtree `Unknown`, and a fully-modelled document reported `coverage: unknown`.
Nine tests failed at once.

Fixed to follow RFC 6901 in full. The token's meaning is decided by the value it
is applied to: under an array, `0` is an index; under an object, `0` is the key
`"0"`. There is no ambiguity to resolve. Leading zeros are refused (`/a/01` is
invalid) and so is `-`, which the RFC defines only for append operations.

### 4.2 Resolved `$ref` nodes kept the reference-site pointer

`resolve_shallow` returned the resolved value but callers continued computing
child pointers from the *original* location. A node written as
`{"$ref": "#/components/parameters/Q"}` has no `/schema` child of its own, so
`/paths/…/parameters/0/schema` always missed. The contract was permanently
`Unknown` — but only in the `$ref` form, which is the form real specifications
use. The inline form worked, which is exactly how such a bug survives review.

Found by `step37_reference_layout_does_not_change_the_contract_digest`, which
asserts that a schema declared inline and the same schema behind a `$ref` are the
same contract.

Fixed by having `resolve_shallow` return the resolved pointer alongside the
resolved value.

### 4.3 A dropped parameter did not degrade coverage

When the parser rejected a parameter — for example a path parameter that does not
appear in its route, which is invalid OpenAPI — it recorded a note and moved on,
**without** touching coverage. A document containing an unreadable parameter could
therefore still claim `Verified`, and a `Verified` document with no detectable
break reports `compatible`.

That is precisely the shape of the requirement's failure mode: a gap in reading
becoming a claim of safety. Dropped parameters now degrade coverage.

### 4.4 Adding an *optional* request body was classified as breaking

`ChangeKind::RequestBodyAdded` carries `Breaking`, and the `(None, Some(rb))` arm
used it unconditionally. An added **optional** body rejects nothing: existing
callers still send no body, which remains valid.

Clippy found this by flagging an `if rb.required { A } else { A }` branch that did
nothing — the symptom of an intent that was written down and then not
implemented. Severity depends on `required`, so the kind must too; a separate
`OptionalRequestBodyAdded` variant now carries `NonBreaking`.

### 4.5 Three test fixtures silently did nothing

Three tests used `.replace()` on a JSON literal to produce a variant document. The
patterns did not match (a comma immediately followed by `"` where the source has
a comma, newline, then indentation), so the "variant" was byte-identical to the
original. Two of them then asserted `Breaking` on a diff of a document with
itself — and failed, which is why they were caught. One asserted `Compatible`,
which a no-op satisfies, and passed for the wrong reason until a guard was added.

Fixed by mutating the parsed model instead of patching source text
(`without_path`), and by adding `assert_ne!(to, BASE, "fixture edit was a no-op")`
guards wherever a test's assertion would also hold for an unchanged input.

---

## 5. Verification telemetry

All commands run from the repository root on `linux`.

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | **PASS** |
| `cargo clippy --workspace --all-targets -- -D warnings` | **PASS** (clean) |
| `cargo test --workspace --all-targets` | **PASS — 407 passed, 0 failed** (was 342) |
| `cargo test --workspace --doc` | **PASS** — 19 targets, 0 failed |
| `./scripts/verify-determinism.sh` | **PASS** — 100/100 byte-identical digests |
| `./scripts/verify-arch-boundaries.sh` | **PASS** — clean, offline, zero sub-process |

New tests contributed by this step: **65**

| File | Tests | Covers |
|---|---|---|
| `crates/autopsy-contracts-distributed/src/document.rs` (unit) | 13 | loading, canonicalisation, RFC 6901 |
| `crates/autopsy-contracts-distributed/tests/step37_openapi.rs` | 40 | Step 37 gates + honesty rules |
| `crates/autopsy-contracts-distributed/tests/schema_conformance.rs` | 12 | schema/struct drift |

Determinism is asserted directly rather than inferred from the script:

* `step37_delta_is_reproducible_across_100_runs` — 100 iterations, delta and
  BLAKE3 digest byte-identical.
* `step37_key_reordering_does_not_change_the_parse_or_the_delta` — the same
  contract written with keys in a different order produces an identical contract
  digest, an identical parsed model, and an empty delta.
* `step37_reference_layout_does_not_change_the_contract_digest` — inline and
  `$ref`-ed forms of the same schema agree.
* `changes_are_sorted_and_unique` / `notes_are_sorted_and_unique` — the canonical
  ordering the delta digest depends on is asserted, not assumed.

The document digest is taken over the **parsed value**, not the source bytes, so
comments, indentation, and key order cannot reach it: a reformat is not a contract
change. `serde_json`'s map is `BTreeMap`-backed in this workspace (no
`preserve_order`), so no document order can leak into a digest.

### 5.1 The schema conformance suite enumerates the Rust enums exhaustively

The repository's pre-commit hook only runs `json.load` over `schemas/*.json`. That
does not catch the failure mode that hurts: a schema stays valid JSON forever
while the Rust enum it enumerates gains a variant, and every consumer then rejects
a document the engine considers valid.

`ChangeKind::ALL` was added to the source purely so the check can be **exhaustive**
rather than a hand-maintained list that drifts. The suite compares the schema's
`enum` against the serialised Rust values **through `serde`**, not against a
duplicated string list, so a `#[serde(rename)]` cannot drift unnoticed.

It also guards the reverse direction: `the_fixtures_actually_produce_all_three_verdicts`
fails if the fixtures stop producing a verdict, which would otherwise leave the
enum-coverage tests passing while checking less than they claim.

---

## 6. Honest scope statement — what is **not** done

1. **Steps 38–45 are not started.** The AsyncAPI 3.1.0 engine (Step 38), the Pact
   consumer/provider engine (Step 39), `autopsy-telemetry` (Steps 40–41),
   `autopsy-mining` (Step 42), `autopsy-catalog`, the CLI subcommands, and the
   MCP / Web Inspector evolution do not exist.
2. **`autopsy-contracts-distributed` is a library API only.** It is referenced by
   nothing, has no CLI subcommand, and is not reachable from the command line.
3. **No storage write-back.** `distributed_contracts` exists in migration v2 but
   nothing writes to it; parsed contracts are in-memory values.
4. **Only local (`#/…`) references resolve.** External file references are
   detected, counted, and reported in `SpecDocument::external_refs` and in the
   delta notes, but never followed. Multi-file specifications are therefore
   `unknown`, not silently partial.
5. **JSON Schema composition is not interpreted.** See §3. This is the single
   largest limitation, and it means most real-world specification pairs report
   `unknown` rather than `compatible`.
6. **Response-side direction is not modelled.** A changed response schema is
   reported as unclassified. This is honest but not useful; a real response
   compatibility model would need consumer-side evidence this engine does not
   have.
7. **YAML support is `serde_yaml` 0.9**, which carries the pre-existing
   `RUSTSEC-2024-0449` advisory via `unsafe-libyaml`. It was already a production
   dependency of `autopsy-invariants` and `autopsy-repo`; this step adds no new
   supply-chain edge and deliberately does not touch the advisory.
8. **`schemas/temporal.schema.json` is still missing** from Pillar 1, tracked in
   `docs/requirements/TRACEABILITY.md` as `NOT DELIVERED`.
9. **`Cargo.toml` version remains `0.0.1`** while `CHANGELOG.md` has had an
   `[0.1.0]` section since 2026-10-08.
10. **`rust-version = "1.85"` is inaccurate** — the workspace already uses
    let-chains, which require Rust 1.88 in the 2024 edition. Pre-existing.

---

## 7. Boundary compliance

* **Zero LLM** — classification is a deterministic comparison of parsed
  structures. No model, no score, no threshold.
* **No sub-process execution** (`INV-SANDBOX-001..004`) — verified by
  `verify-arch-boundaries.sh`, which greps for `process::Command` across
  `crates/`.
* **Read-only** (`FR-026`) — documents are taken by value and values returned.
  The crate opens no files, writes nothing, and spawns nothing.
* **No new external dependency** — `autopsy-domain`, `blake3`, `serde`,
  `serde_json`, `serde_yaml`, `thiserror`, all already in the `deny.toml`
  allow-list.
* **Dependency direction** — `ContractsDistributed → Domain` only. Nothing
  imports this crate, and it imports no presentation layer.

---

## 8. Traceability

| Item | Location |
|---|---|
| Sequence with acceptance gates | `Resource/4.2/04_4.2_IMPLEMENTATION_SEQUENCE.md`, Step 37 |
| Crate map, `distributed_contracts` DDL | `Resource/4.2/02_4.2_REPO_BUILD_CRATES_INTEGRATION.md` |
| Schema inventory, exit codes | `Resource/4.2/03_4.2_STANDARDS_SCHEMAS_VERSIONING.md` |
| Requirement table | `docs/requirements/TRACEABILITY.md`, Phase 4.2 |
| Schema | `schemas/distributed-contract.schema.json` |
| Implementation | `crates/autopsy-contracts-distributed/src/{document,openapi,openapi_diff}.rs` |
| Tests | `crates/autopsy-contracts-distributed/tests/{step37_openapi,schema_conformance}.rs` |
