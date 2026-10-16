# Evidence 20 — Phase 4.2 Pillar 3 (part 2): AsyncAPI 3.1.0 Contract Engine (Step 38)

**Date:** 2026-10-16
**Scope:** Step 38 of `Resource/4.2/04_4.2_IMPLEMENTATION_SEQUENCE.md`.
**Crate:** `crates/autopsy-contracts-distributed` (extended; no new crate)
**Predecessors:** `docs/evidence/19_phase4_2_pillar3_openapi.md` (Step 37)

---

## 1. What the step had to prove

Verbatim from the implementation sequence:

> **Step 38** — AsyncAPI 3.1.0 Contract Engine — Parses AsyncAPI 3.1.0 channels/messages;
> detects payload schema incompatibilities across producers and consumers.

Note the second clause carefully. Step 37 asked *"did this contract change?"*. Step 38 asks
*"does this producer satisfy this consumer?"* Those are different relations, and building one
on top of the other produces confident nonsense in both directions.

---

## 2. The design decision that shapes this step

### 2.1 An HTTP operation has one arrow. A broker channel has two.

Which one you are looking at is **not** in the schema. It is in the operation's `action`:

| `action` | the application is | the payload schema describes |
|---|---|---|
| `send` | the producer | what it **emits** |
| `receive` | the consumer | what it **accepts** |

So the same schema edit has *opposite* severities depending on which side made it:

| Change | `Accepted` (we consume) | `Emitted` (we produce) |
|---|---|---|
| property removed | breaking — senders lost a capability | breaking — consumers stop receiving it |
| optional property added | non-breaking | **unknown** — a strict consumer rejects unknown fields |
| requiredness gained | breaking — senders that omit it break | non-breaking — the producer guarantees more |
| requiredness lost | non-breaking | breaking — the producer guarantees less |
| type narrowed | breaking — accepts fewer values | non-breaking — emits a subset |
| type widened | non-breaking | breaking — may emit values no consumer has seen |
| enum value removed | breaking | non-breaking |
| enum value added | non-breaking | breaking |
| `additionalProperties: false` introduced | breaking | non-breaking |
| `additionalProperties` loosened | non-breaking | **unknown** |

A message referenced by both a `send` and a `receive` operation in the same document is
`Both`, and a change must be safe in **both** readings at once. That takes the worse of the
two, which is a genuinely strong requirement.

`Both` is not a corner case to be papered over. It is what a merged or aggregated
specification looks like, and it is the case where a change that looks harmless from either
side alone is not.

**Why the `Emitted` additions are `Unknown` and not non-breaking.** "Old consumers ignore
unknown fields" is a *policy* some ecosystems adopt; it is not a property of the schema. Avro
and Protobuf decoders reject unknown fields by default, and JSON deserialisers frequently do
too. This engine has not observed the consumer, so it says so. When the consumer *is* in the
document, `verify_flows` settles the question from evidence rather than assuming an answer —
which is the next point.

### 2.2 Two relations, not one

| function | question |
|---|---|
| `diff_asyncapi(from, to)` | did revision A stop being compatible with revision B? |
| `verify_flows(spec)` | does the producer's declared payload satisfy the consumer's declared expectations? |

`verify_flows` checks **containment**: everything the producer *guarantees* must be
*acceptable* to the consumer.

1. Every field in the consumer's `required` must be declared by the producer.
   `additionalProperties` never guarantees a specific field, so being *permitted* to send it
   is not enough.
2. The producer's offered type set must be a subset of the consumer's accepted set. A producer
   with **no** `type` accepts every JSON type, so it over-delivers and is reported — that is
   what the schema says, not a heuristic.
3. Enum containment: if the consumer declares an `enum` and the producer declares none, the
   producer may emit any value and the pair is reported.
4. If the consumer has `additionalProperties: false`, the producer must not declare extra
   fields *and* must not permit them.
5. All of the above recurse, so a violation inside `a.b.c` is reported at its own JSON pointer.

### 2.3 Flow findings fold into the revision delta — but only the *new* ones

A `diff_asyncapi` that reported `Compatible` while the candidate revision's own producers
cannot satisfy its own consumers would be wrong. So `verify_flows` runs on the candidate and
its findings become `ChangeKind::ProducerConsumerIncompatible` changes.

The first implementation folded in the **absolute** flow report, and that was wrong in a way
worth recording. Any document that describes only one side of a channel has an unverifiable
flow: `verify_flows` correctly reports `Unknown`. But folding that into the *delta* made
`Compatible` unreachable for every one-sided document, and — worse — reported an **unchanged**
gap as a regression. A signal that fires on unchanged input is a signal users learn to ignore.

The fix computes `verify_flows` on **both** revisions and reports only what changed:

* findings in the candidate that were not in the baseline → changes;
* findings the candidate **resolved** → a note (a delta that only ever reports problems is
  indistinguishable from one that misses them);
* operations that could be checked before and cannot be checked now → a note **and** a
  coverage penalty;
* modelling coverage that got *worse* between revisions → a note and a penalty, because the
  same edit can look "compatible" simply because less of the candidate was read.

---

## 3. What is modelled, and what is disclosed instead

| Surface | Modelled | Notes |
|---|---|---|
| `channels`, `address`, `parameters` | ✅ | address placeholders checked against declared parameters **both ways** |
| `operations`, `action`, `channel`, `messages` | ✅ | absent `messages` applies to the whole channel, per spec |
| `components.messages` | ✅ | via `$ref`, keyed by position, not by pointer |
| Message `name`, `contentType` | ✅ | contentType change is breaking |
| Payload and header schemas | ✅ | via the shared `SchemaReader` |
| Producer/consumer payload + header compatibility | ✅ | the step gate |
| **Multi Format Schema Object** (`schemaFormat` + `schema`) | ✅ | read when the format is JSON Schema |
| **Message-level `schemaFormat`** (the 2.x layout) | ⚠️ | honoured, not ignored — see §4.4 |
| **Avro / Protobuf / RAML payloads** | ❌ | recorded by pointer, degraded |
| **`traits`** (message and operation) | ❌ | degraded: a trait may inject `payload` or `headers` |
| **`reply`** (request/reply) | ❌ | degraded |
| **`bindings`, `tags`, `security`, `servers`, `externalDocs`** | n/a | **not** degraded — see below |
| **AsyncAPI 3.0.x** | ⚠️ | parsed under 3.1 semantics, `partial` |
| **AsyncAPI 2.x** | ❌ | hard error |

The `bindings` row is a deliberate asymmetry. Those keywords carry no payload facts, so they
cannot hide an incompatibility. Degrading for them would make every realistic document
permanently `Unknown` and would teach users that the coverage signal is noise. The rule is:
degrade when a keyword **could hide a fact**, never merely because a keyword is unmodelled.
`traits`, by contrast, can inject properties into the payload, so they degrade.

AsyncAPI 2.x is refused rather than parsed leniently. Its channel model is
`publish`/`subscribe` with operations nested *inside* channels, so a lenient 3.x read yields
an empty operation set — and the diff would then report **every operation of the other
revision as removed**. A garbage diff that says "everything broke" is as useless as one that
says "nothing broke".

---

## 4. Defects found and fixed

Five defects, four of them found by a test failing, and two of them severe enough that the
engine would have been actively misleading.

### 4.1 `3.1.0` was read as minor version `1`, so *every* correct document was flagged partial

`version.split('.').nth(1)` returns the **patch** level for a three-component version. Every
`3.1.0` document therefore failed the "is this 3.1?" check and was marked `partial` with a
confusing note. The comparison constant was `"3.1"`; the extracted value was `"1"`.

A shared `major_minor` helper now takes the first two components, with a regression test
asserting `major_minor("3.1.0") == "3.1"` and `major_minor("3.10.4") == "3.10"` (the latter
because `10` must not be truncated by a naive take-two-characters approach).

This is the second time in two steps that a *version-string* assumption silently downgraded
every document. It was caught only because a test asserted `Verified` on a document that
should plainly be `Verified` — a test that asserts the **happy** path. Tests written only
around failure modes would have shipped this.

### 4.2 Unescaping a reference target before splitting it destroyed channel/message identity

A message is keyed `channelId/messageId`, both escaped. The operation parser did:

```rust
target.strip_prefix("/channels/")   // "user~1signedup/messages/M"
  .map(unescape)                    // "user/signedup/messages/M"   <-- merged
```

and produced the key `user/signedup/messages/M` instead of `user~1signedup/M`. So a channel id
containing a `/` — which is common, because some documents use the raw topic as the id —
silently produced message keys that matched nothing, and every flow on such a channel read as
"no common message".

The fix splits **before** unescaping. The escaped form guarantees no literal `/` inside a
segment, so the first literal `/messages/` is the structural separator and the ids are
recovered exactly.

### 4.3 Following `$ref` chains to the end lost the identity the reference was naming

The parser resolved `channel` and `messages` references with the full chain-following
resolver. In a document that factors a message body into `components/`, the chain is:

```text
#/channels/orders/messages/OrderPlaced  ->  #/components/messages/OrderPlaced
```

so the resolver returned `/components/messages/OrderPlaced` — which is *not* under
`/channels/` — and the operation's message set came out **empty**. The channel parsed fine;
only the operations lost their messages. Both the revision digest and the flow check were
wrong, and both in the direction of *saying less is wrong with this document than is true*.

AsyncAPI says a `channel` reference names a **channel**, and a message reference names a
**message position**. Identity is the thing being referenced, not the place the body happens
to live. References are now read **one hop** from the raw `$ref`, via `SpecDocument::ref_at`,
and normalised through the existing `pointer_of` validator (which also refuses external
references and the bare `#`).

Found by `step38_factoring_a_message_into_components_is_not_a_remove_plus_add`, which exists
precisely because a `refactor-stability` test is the only kind that can catch it: the
before and after documents are semantically identical and the bug hides in the difference.

### 4.4 A message-level `schemaFormat` was ignored, so an Avro payload was read as JSON Schema

AsyncAPI 3.x puts `schemaFormat` and `schema` together in a Multi Format Schema Object;
AsyncAPI 2.x put `schemaFormat` on the message and the schema directly in `payload`. The
parser only recognised the 3.x layout, so a document using the 2.x layout had its Avro record
run through the JSON Schema reader — which cheerfully recorded `type: "record"` and reported
the message `Verified`.

That is the exact failure this engine exists to prevent: a confident claim of understanding
about a payload that was never read. The message-level form is now honoured and gated on the
same readability check, so a non-JSON-Schema format is disclosed no matter which layout
declares it. The test asserts **both** layouts, and additionally that the document's coverage
can never reach `Verified`.

### 4.5 Three test fixtures silently did nothing

Three `.replace()` fixtures did not match, so the "candidate" document was byte-identical to
the baseline. In this crate the pattern is particularly nasty: an unchanged document produces
an **empty** delta, so a test could assert "no breaking changes" and pass while checking
nothing. One of the three asserted `Breaking` and so failed loudly; one asserted a channel
address had moved and failed loudly; one asserted the *absence* of a finding and would have
passed for the wrong reason.

All three were rewritten — two with explicit fixtures rather than string surgery, one with the
pattern corrected. `assert_ne!(to, from, "fixture edit was a no-op")` guards are now in place
wherever an assertion would also hold for an unchanged input. This is the third occurrence of
this pattern in Pillar 3, which is why it is recorded rather than quietly fixed.

---

## 5. Verification telemetry

All commands run from the repository root on `linux`.

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | **PASS** |
| `cargo clippy --workspace --all-targets -- -D warnings` | **PASS** (clean) |
| `cargo test --workspace --all-targets` | **PASS — 486 passed, 0 failed** (was 407) |
| `cargo test --workspace --doc` | **PASS** — 19 targets, 0 failed |
| `./scripts/verify-determinism.sh` | **PASS** — 100/100 byte-identical digests |
| `./scripts/verify-arch-boundaries.sh` | **PASS** — clean, offline, zero sub-process |

New tests contributed by this step: **79**

| File | Tests | Covers |
|---|---|---|
| `crates/autopsy-contracts-distributed/src/asyncapi.rs` (unit) | 40 | parsing, identity, disclosure |
| `crates/autopsy-contracts-distributed/src/asyncapi_diff.rs` (unit) | 19 | direction algebra, flow rules, deltas |
| `crates/autopsy-contracts-distributed/tests/step38_asyncapi.rs` | 20 | Step 38 gates through the public API |

Determinism is asserted directly rather than only inferred from the script:

* `step38_the_delta_is_reproducible_across_100_runs` — 100 iterations, byte-identical delta and
  digest.
* `step38_key_reordering_does_not_change_the_parse_or_the_delta` — the same contract with keys
  in a different order parses equal, digests equal, and diffs empty.
* `the_digest_is_reproducible` — 100 parses of the same source, one digest.
* `changes_are_canonically_ordered` — the ordering the delta digest depends on is asserted,
  not assumed.
* `step38_the_delta_digest_distinguishes_a_reclassification_from_a_reform` — a severity change
  moves the digest, so the digest can never read as "nothing to see".

### 5.1 The direction algebra is asserted kind by kind, not sampled

`the_direction_algebra_is_exactly_as_documented` enumerates the 22 `(kind, direction)` pairs
that have a defined severity and asserts each one against the module's own table — the doc
comment and the code are checked against each other. It then asserts, for every payload kind,
that `Both` equals `worst(Emitted, Accepted)`. A new payload kind added without a direction
rule cannot slip through: `severity_for`'s catch-all returns `Unknown`, and
`a_kind_reaching_the_algebra_unhandled_is_unknown_not_a_pass` asserts that the catch-all is
**not** an approval.

### 5.2 The schema drift guard now covers 53 kinds

`schemas/distributed-contract.schema.json` grew from 29 to 53 change kinds as Step 38 added
its vocabulary. The conformance suite caught the drift the moment the enum changed, which is
what it is for — `the_schema_enumerates_every_change_kind_the_engine_can_emit` compares the
schema against `ChangeKind::ALL` **through `serde`**, so an added variant or a
`#[serde(rename)]` cannot pass unnoticed. An AsyncAPI delta was added to the shared fixture
set so every structural check exercises both protocols.

---

## 6. Honest scope statement — what is **not** done

1. **Steps 39–45 are not started.** The Pact consumer/provider engine (Step 39),
   `autopsy-telemetry` (Steps 40–41), `autopsy-mining` (Step 42), `autopsy-catalog`, the CLI
   subcommands, and the MCP / Web Inspector evolution do not exist.
2. **`autopsy-contracts-distributed` is a library API only.** No CLI subcommand; nothing
   reaches it from the command line.
3. **No storage write-back.** `distributed_contracts` exists in migration v2 but nothing
   writes to it.
4. **`verify_flows` sees only what one document declares.** Cross-repository or
   cross-service producer/consumer verification — the case where the consumer's schema is in
   another project — is not possible; the flow check only pairs operations inside a single
   document. This is the single largest limitation of Step 38.
5. **Message identity is positional.** Factoring a message body into `components/` is stable
   (proved by test), but *renaming* a message id reads as a removal plus an addition. Pairing
   by `name` exists for flow checks only; the revision diff has no name-based identity.
   A cross-revision diff therefore over-reports on renames.
6. **Operation `traits` and `reply` are not applied.** A request/reply API reports `partial`
   and cannot reach `Compatible`.
7. **Payload `examples` are not checked against the schema.** A realistic engine would verify
   the examples describe the declared payload; this one records nothing about them.
8. **The flow check does not model `bindings`-level semantics** (Kafka key/partition,
   AMQP routing keys). Bindings are not payload, so they are out of scope here — but a
   binding change is a real wire change that this engine does not report.
9. **`schemas/temporal.schema.json` is still missing** from Pillar 1.
10. **`Cargo.toml` version remains `0.0.1`** while `CHANGELOG.md` has had an `[0.1.0]` section
    since 2026-10-08.

---

## 7. Boundary compliance

* **Zero LLM** — every classification is a deterministic comparison of parsed structures.
* **No sub-process execution** (`INV-SANDBOX-001..004`) — verified by
  `verify-arch-boundaries.sh`.
* **Read-only** (`FR-026`) — documents are taken by value; the crate opens nothing, writes
  nothing, and spawns nothing.
* **No new dependency** — this step added no crate to `Cargo.toml`; `serde_yaml` was already
  present for Step 37.
* **Dependency direction** — `ContractsDistributed → Domain` only.
* **Determinism** (`NFR-001`) — every canonical collection is a `BTreeMap` or `BTreeSet`. The
  delta digest excludes `notes` (prose, allowed to be reworded) and includes `severity`, so a
  reclassification is a different digest even when the detected facts are identical.

---

## 8. Traceability

| Item | Location |
|---|---|
| Sequence with acceptance gates | `Resource/4.2/04_4.2_IMPLEMENTATION_SEQUENCE.md`, Step 38 |
| Standards, schema inventory, exit codes | `Resource/4.2/03_4.2_STANDARDS_SCHEMAS_VERSIONING.md` |
| Requirement table | `docs/requirements/TRACEABILITY.md`, Phase 4.2 |
| Schema | `schemas/distributed-contract.schema.json` (53 change kinds) |
| Shared schema layer | `crates/autopsy-contracts-distributed/src/schema.rs` |
| Shared change vocabulary | `crates/autopsy-contracts-distributed/src/change.rs` |
| Step 38 implementation | `crates/autopsy-contracts-distributed/src/{asyncapi,asyncapi_diff}.rs` |
| Step 38 tests | `crates/autopsy-contracts-distributed/tests/step38_asyncapi.rs` |
