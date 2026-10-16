# Evidence 22 — Phase 4.2 Pillar 4 (part 1): OpenTelemetry Ingest & Redaction (Step 40)

**Date:** 2026-10-16
**Scope:** Step 40 of `Resource/4.2/04_4.2_IMPLEMENTATION_SEQUENCE.md`.
**Crate:** `crates/autopsy-telemetry` (new)
**Predecessor:** `docs/evidence/21-pact-contract-diff.md` (Step 39)

---

## 1. What the step had to prove

Verbatim from the implementation sequence:

> **Step 40** — *OpenTelemetry 1.44.0 Ingestion & Redaction* — `autopsy-telemetry` —
> **Sanitizes OTLP trace spans [R24]; strips PII, auth tokens, and raw query strings;
> hashes span IDs with BLAKE3.**

and from §4.2 of the Pillar 4 design:

> **Data Boundary:** All query parameters, authorization headers, request payloads, and
> IP addresses are stripped before hashing.

The gate reads as three tasks. It is really one, plus two consequences:

1. **Redact.**
2. Therefore the digest must be taken over the *sanitized* value, never the raw one.
3. Therefore span identity must not move when a secret changes — which is a property
   that can be asserted directly, and is.

---

## 2. The design decisions that shape this step

### 2.1 Redaction happens before hashing, and it is asserted, not assumed

A digest computed over raw input is a **fingerprint of the secret it contained**. Two
observations carrying the same token would correlate; anyone holding the digest could
confirm a guessed token by recomputing it. So the digest is over the sanitized value.

The property that follows is the one worth testing, because it fails loudly if the
ordering is ever reversed by a later edit:

```
GET /orders?token=A   ─┐
                       ├─▶  same route, same digest, same occupancy key
GET /orders?token=B   ─┘
```

`span_identity_does_not_depend_on_the_secret_it_carried` asserts exactly this, and it is
guarded on both sides so it cannot pass vacuously:

* a **guard** that the fixture really contained a query to strip
  (`redaction_totals[query_string] == 1` for both records), so the equality is not the
  equality of two fixtures that never had a secret; and
* a **control** that the digest *is* sensitive to attribute content that survives
  redaction, so the equality is about the redaction and not about the digest ignoring
  attributes altogether.

### 2.2 Two identities, because dead-code detection and change detection ask different questions

`SpanRecord::digest` covers the whole observation, **timestamps included**. It answers
*"is this the same observation?"* and is what `NFR-001` compares across runs.

`SpanRecord::occupancy_key` excludes every timestamp. It answers *"was this code path
executed in this window?"* — which is Step 41's question. §4.2 states the dead-code rule
as *zero runtime invocations over N observation windows*, so the key must be a
**presence** and not a **count**: a span that ran yesterday and the same span that ran
today are two observations and one occupancy.

`the_occupancy_key_ignores_time_and_the_digest_does_not` asserts both halves together,
which is the only way the distinction cannot silently collapse.

### 2.3 The salt is required, and there is deliberately no default

Span and trace ids are pseudonymised with **BLAKE3 in keyed mode** under a caller-supplied
32-byte salt.

The reason is not belt-and-braces. An OTLP span id is 64 bits. `BLAKE3(raw_id)` without a
key is a *public function of a 2⁶⁴ space*, so the mapping is invertible by exhaustive
search in seconds. Publishing such a hash would be obfuscation that a reader would
reasonably mistake for protection — the "honest coverage" failure this codebase forbids.
Keyed BLAKE3 is a pseudorandom function, so without the salt the mapping is not computable
at all.

Three consequences are stated rather than left implicit:

* hashes are comparable **only within one salt**;
* the salt is an input to the engine, so `NFR-001` determinism is "identical inputs",
  where the salt is one of them;
* the output records **no** salt information. A fingerprint of the salt would let an
  attacker verify a guessed salt offline, so keeping the salt with the report is the
  caller's responsibility. `the_salt_changes_the_pseudonyms_and_nothing_else` asserts
  that the correlation surface does **not** move with the salt, and that no byte-run of
  the salt reaches the output.

Roles are domain-separated (`trace`, `span`, `parent-span`, `link-trace`, `link-span`), so
a span id cannot be recognised by presenting it in another role.

**A defect removed during this step:** the first implementation derived `Default` as
`test_salt()`. That is a *public key* handed out silently, and it directly contradicted
the paragraph above it. The `Default` impl was deleted; a caller must now name a salt or
call the explicitly-named `test_salt`, which makes the choice visible at the call site.

### 2.4 The reader is lossless; the sanitizer is where judgement happens

The two have opposite failure modes, so they are separate modules. A reader that quietly
dropped the shapes it did not understand would make the sanitizer look complete when it
had simply never seen the data — and the sanitizer is the component whose silence is
dangerous.

So `parse_otlp_traces` keeps every value it can represent, keeps the unrepresentable ones
as explicit `AnyValue::Unmodelled` nodes **naming the variants they declared**, and records
a JSON pointer for each. An `AnyValue` with two variants present is a producer using a
newer `oneof` member; it is recorded as `{stringValue, intValue}`, not as "malformed".

Two format details are load-bearing and were treated as such:

* **`intValue` is a JSON *string*.** OTLP/JSON writes the 64-bit integer variants as
  strings, because JSON numbers are IEEE-754 doubles and a JavaScript collector would
  silently round them. A reader that only accepted JSON numbers would read *every*
  integer attribute as absent. Both spellings are accepted and canonicalised to decimal
  text, so `"007"` and `7` cannot produce different digests.
* **`doubleValue` is kept as literal text**, because rendering through `f64` and back is a
  determinism hazard across formatting styles. `serde_json` renders the number it parsed,
  so `1.0` and `1.00` agree. Keeping the text also means `Eq` is derivable, which the
  sanitized model depends on.

### 2.5 Redaction is recursive, or it is theatre

A token can sit in a span attribute, inside an `arrayValue`, inside a `kvlistValue`, in a
span **event** attribute, in a **link** attribute, in the span **name**, or in the
`status.message` the specification explicitly says "MAY contain sensitive information".
`exception.message` is the classic leak: the HTTP instrumentation puts the failing request
into it.

`Redactor::value` is therefore a single recursive walk, and **every** call site goes
through it. A sanitizer that walks only the top-level `attributes` array redacts the easy
case, passes the leak through, and looks complete. Three tests assert the reach
separately — event attributes, link attributes, and the status message — because a single
test over one of them would pass with the other two leaking.

Span and event **names** get the URL machinery applied unconditionally. That is not
defensiveness: the OpenTelemetry HTTP instrumentation names a span after the raw request
target until a route is known, so `GET /orders/7?token=…` is the common case.

### 2.6 Two detection layers, and a deliberate limit on each

**Table** rules key on the attribute *name* (`authorization`, `x-api-key`, `url.query`,
`enduser.id`, `db.statement`, …). They are exact and cannot misfire, but they miss a token
smuggled under an innocuous key — `url.full`, `code.filepath`, a span name, an exception
message.

**Shape** rules key on the value's *form* — a JWT (three base64url segments, the first
decoding to `{"`), an `Authorization` scheme prefix followed by a single opaque token, an
AWS access key id, PEM private-key material, an IP or MAC literal, an e-mail address, a
Luhn-valid card number.

Precedence: the key table is consulted **first**, and it removes the whole subtree. A key
that names a credential *is* that credential, whatever its value looks like; reporting the
vaguer reason would lose the one fact the reader can act on. And an `arrayValue` under a
`Cookie` header must not have its elements walked and kept — the leak is inside them.

**Structural keys are exempt from the fuzzy detectors.** The correlation surface of §4.1
(`code.function`, `code.filepath`, `http.route`, `rpc.service`, `rpc.method`,
`service.name`, …) is checked against the four high-confidence rules only. The reason is
that over-redacting a structural key silently destroys the fusion this pillar exists to
perform:

| Value | Key | Without the exemption |
|---|---|---|
| `checkout@1.0.0` | `service.name` | read as an e-mail address and erased |
| `send.1` | `code.function` | read as an IPv4 literal and erased |

A JWT, a `Bearer` prefix, an AWS key id or PEM material cannot plausibly be a service
name, so those four still apply. `a_structural_key_is_immune_to_the_fuzzy_detectors`
asserts both directions.

Two detector details that exist to prevent false positives, each with its own test:

* an e-mail address must have an **alphabetic final label**, so `checkout@1.0.0` is a
  version and not an address;
* an IPv6 literal either compresses (`::`) or spells all eight groups out, so `12:34:56`
  is a clock time and not an address.

**No regular expressions.** Every detector is a hand-written linear scan. That keeps the
crate free of a regex engine and, more importantly, makes catastrophic backtracking
structurally impossible: a redactor that can hang on attacker-controlled input is a
denial-of-service boundary, and trace attributes are attacker-controlled.

### 2.7 What is *not* attempted, and why the gap is visible rather than claimed

Free-text PII **inside prose** is not scrubbed. `exception.message` may contain a
customer's name; no bounded pattern finds that. Rather than claim otherwise, a free-text
value is marked `SanitizedValue::FreeText` — **kept**, because it is the single most useful
attribute for understanding a failure — and the span's coverage drops to `Partial`.

The alternative, reporting a sanitized trace while arbitrary prose went through
unexamined, is the `UNKNOWN`-as-`PASS` failure this codebase forbids.

### 2.8 An un-normalisable identifier is an error, not a degradation

OTLP defines trace ids as 32 lowercase-hex characters and span ids as 16, but exporters
differ on case. Hashing raw text would give `EEE19B7EC3C1B174` and `eee19b7ec3c1b174`
two pseudonyms for what the specification says is one span — a **silent split** in the
topology Step 41 draws.

So identifiers are lowercased *and* validated. Anything that is not the right count of hex
characters is a hard error. Degrading instead would leave Step 41 correlating on an
identifier it cannot trust while reporting nothing wrong, which is precisely the failure
mode worth refusing. An out-of-range span `kind` is rejected for the same reason:
defaulting it to `Unspecified` would place an unknown span into the topology.

---

## 3. What is modelled, and what is disclosed instead

| Modelled | Disclosed, coverage degraded |
|---|---|
| Every `AnyValue` variant, recursively | A node with zero or several variants — recorded with the variant names it declared |
| `resource` / `scope` / `scopeSpans` / `spans`, in full | A `KeyValue` with no `value` — recorded as key-present/value-absent |
| `span.kind`, `status` (`code` + `message`), `events`, `links` | `bytesValue` — removed as `OpaqueBinaryValue`; binary cannot be classified as text |
| `droppedAttributes/Events/LinksCount` | Non-zero counts — the SDK dropped data the engine never saw |
| `http.route`, `http.request.method`, `http.response.status_code`, `code.function`, `code.filepath`, `rpc.service`, `rpc.method`, `url.full`, `url.path`, `service.name` | A span carrying none of `http.route`, `code.function`, `rpc.service` — recorded as `Uncorrelated`, never dropped |
| Both `intValue` spellings, `doubleValue`, `boolValue` | Free text under a `message`/`stacktrace`/`description`/`detail` segment |
| Query, fragment and userinfo in any URL-valued key | The `Semantic Conventions` version: OTLP/JSON carries none, so the exporter's version is **not verifiable from the payload** — stated in the notes rather than assumed |

An export with **zero spans** is `Unknown`, never `Verified`. Approving a fusion that never
observed anything is the same error as approving an empty Pact (Step 39) or an unchanged
but unmodelled document (Step 38).

---

## 4. Defects found and fixed

### 4.1 The span pointer reached the digest, so export order changed the record

`pointer` recorded *where in the export* a span sat (`/resourceSpans/0/…/spans/1`) and was
part of `SpanRecord`'s serialization, hence of its digest. Reversing two spans in the
source export therefore changed the record digest — reported by
`the_export_order_does_not_change_the_record`.

This is a real defect, not a test artifact: a collector is free to export its spans in any
order, so **a reformatting would have been reported as a change**. Fixed by excluding
`pointer` and `notes` from every digest and by replacing the blanket
`digest_of(&record)` with an explicit `TraceDigestInput` projection that names the covered
surface — coverage, the service set, the redaction totals, and the span digests — instead
of inheriting whatever happens to be in the struct. The exclusion is top-level only, so an
attribute whose key happens to be `notes` still reaches the digest.

### 4.2 A coverage assertion that scanned the wrong field

`a_free_text_or_unmodelled_value_always_costs_coverage` first scanned only
`span.attributes` for a `free_text` or `unmodelled` value. The fixture carried its prose in
`status.message`, so the scan found nothing and the test reported an absent gap — while a
gap was present and the engine had, correctly, set coverage to `Partial`. The test now
scans the whole serialized span.

Recorded because it is the more dangerous direction to be wrong in: a check that looks for
a gap in the wrong place fails *open*, reporting "no gap" when the answer is "not looked
at". The same shape of mistake appeared in Step 39 §4.5, where a determinism test asserted
the wrong property and failed for the right reason.

### 4.3 `IngestOptions::default()` handed out a public key

Described in §2.3. The `Default` impl was deleted after the module was written, because it
contradicted the module's own stated rule. A default that silently weakens a security
property is worse than no default.

### 4.4 A test asserted a contradiction

`span_identity_does_not_depend_on_the_secret_it_carried` first contained
`assert_ne!(a.digest, b.digest, "the fixtures must genuinely differ")` — which is the exact
opposite of the property under test, and it failed for the right reason. Replaced with a
guard on the redaction count plus a control on a *kept* attribute, which is what "the
fixture genuinely differs" was trying to say.

### 4.5 The `const` spelling of a unit enum variant was misread

`OccupancySubject::Uncorrelated` serializes as the bare string `"uncorrelated"`, so a
schema-drift extractor that only read `required[0]` reported the literal name `const` and
failed against the correct schema. The extractor now reads `const` for unit variants; the
schema was right and the test was wrong.

---

## 5. Verification telemetry

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS (0 warnings) |
| `cargo test --workspace --all-targets` | **602 passed / 0 failed** (was 543, +59) |
| `cargo test --workspace --doc` | PASS (20 doc-test targets) |
| `./scripts/verify-determinism.sh` | PASS — 100/100 byte-identical |
| `./scripts/verify-arch-boundaries.sh` | PASS — no core→MCP edge, no sub-process, no socket |
| `pre-commit run --all-files` | PASS (all 10 hooks) |

Test breakdown for the new crate (59 total):

| File | Tests |
|---|---|
| `src/redact.rs` (unit) | 18 |
| `src/span.rs` (unit) | 3 |
| `tests/step40_otlp.rs` | 27 |
| `tests/schema_conformance.rs` | 11 |

### 5.1 Determinism is asserted, not inferred

* `the_record_is_reproducible_across_100_runs` — same record and same digest, 100 runs.
* `the_export_order_does_not_change_the_record` — the span array is reversed in the parsed
  request and the record digest is unchanged.
* `a_map_attribute_is_sorted_so_its_order_cannot_move_a_digest` — a `kvlistValue`'s entry
  order is swapped; the digest does not move.
* `gate_the_same_identifier_in_two_spellings_is_one_span` — upper- and lower-case hex ids
  produce one pseudonym and one digest.
* `an_integer_attribute_reads_the_same_from_either_spelling` — `"200"`, `200` and `"00200"`
  agree.
* `the_occupancy_key_ignores_time_and_the_digest_does_not` — the two identities are pinned
  in opposite directions.

### 5.2 The salt is asserted, not described

`the_salt_changes_the_pseudonyms_and_nothing_else` asserts that a different salt changes
every span pseudonym **and** that the correlation surface (`route`) and the occupancy key
are unmoved — so the salt is shown to be a key on the *identifier* and not on the
evidence. It also asserts that no byte-run of either salt reaches the output.

### 5.3 The schema drift guard covers the whole redaction vocabulary

`schemas/telemetry-fusion.schema.json` is closed (`additionalProperties: false` at every
object level, asserted) and draft 2020-12. The conformance test compares the schema
against the Rust side **through serde**, so a `#[serde(rename)]` cannot drift unnoticed:

* all 18 `RedactionKind` variants, against `RedactionKind::ALL`;
* every `CoverageState`, `SpanKind` and `StatusCode` variant;
* every `SanitizedValue` variant, as the union of the `oneOf` tags — and the fixture is
  built to actually reach six of the eight, so the vocabulary is held together by output
  and not only by the schema;
* every `OccupancySubject` variant, including the bare-string `Uncorrelated`;
* every field the record and a span emit is declared, and every required field is emitted;
* `notes` are sorted and unique; every digest matches `^[0-9a-f]{64}$`; every redaction
  count is ≥ 1 and the totals are the sum of the per-span counts.

`no_secret_and_no_raw_identifier_appears_anywhere_in_the_record` is asserted on real
output rather than described, so a field added later that carries a raw value fails there
instead of in production.

---

## 6. Honest scope statement — what is **not** done

1. **Prose is not scrubbed.** Free text is flagged and costs coverage (§2.7). This is the
   largest limitation and it is deliberate.
2. **`http.route` is not canonicalised.** Turning `/orders/7` into `/orders/{}` and matching
   that against a declared endpoint is Step 41's job; doing it in two places is how two
   definitions drift apart.
3. **`url.path` is kept percent-encoded** and raw, because it is an input to route
   correlation and decoding it here would change what Step 41 matches on.
4. **A concrete value is not compared against a declared schema.** An attribute whose value
   should satisfy a contract is not checked against one; that needs a JSON Schema validator
   this crate does not carry.
5. **`bytesValue` is removed rather than classified.** Binary cannot be read as text, so no
   rule can see inside it.
6. **A salt is not derived and is not recorded.** Cross-report comparison requires the
   caller to keep the salt; the engine will not help, because a record of the salt would be
   a verification oracle for a guessed salt.
7. **Span-level `flags` and the newer `span.kind` extensions are not read.** They are not
   in the modelled key set and are not disclosed individually — only their absence from the
   correlation surface is.
8. **The engine is library-API only.** No CLI subcommand and no storage write-back to
   `telemetry_*` tables yet; both are later steps in the sequence.

---

## 7. Boundary compliance

* **Zero LLM in correctness crates** — no LLM call, import, or network path.
* **Read-only analyzer (FR-026)** — bytes in, a value out. No filesystem write, no git
  mutation.
* **100/100 determinism (NFR-001, FR-028)** — every collection is a `BTreeMap`/`BTreeSet`
  or sorted on construction; digests are taken over an explicitly named projection of the
  sanitized model, never over source bytes; the salt is an explicit input.
* **Honest coverage (FR-014, FR-030)** — free text, unmodelled nodes, dropped data,
  uncorrelatable spans and an empty export all degrade coverage; nothing is converted to a
  pass.
* **INV-SANDBOX-001..004** — zero sub-process (`std::process` is not imported), zero
  outbound socket, no `follow_links` traversal because there is no traversal.
* **INV-CROSS-001** — no timestamp reaches an identifier or a correlation key; path handling
  is string-level with no platform separator assumption.
* **Architectural inversion** — `autopsy-telemetry` depends on `autopsy-domain` only. It does
  **not** depend on `autopsy-contracts-distributed`: correlating a route against a declared
  endpoint takes the declared routes as an input rather than reaching across the DAG for a
  parser, and template↔template matching is a different problem from the concrete↔template
  subsumption Step 39 performs. That is Step 41's work and it will be justified there.
* **No new dependencies** — `serde`, `serde_json`, `blake3`, `thiserror` and
  `autopsy-domain` were all already in the workspace. No regex engine, no URL parser, no
  telemetry SDK.

---

## 8. Traceability

| Requirement | Where |
|---|---|
| R24 (OpenTelemetry semantic conventions, sanitized traces) | `src/otlp.rs`, `src/span.rs`; `SEMCONV_VERSION` and `MODELLED_KEYS` name the key set read |
| §4.2 Data Boundary (query, auth, payload, IP stripped before hashing) | `src/redact.rs`; §2.1, §5.2 |
| FR-014, FR-030 (honest coverage) | §2.7, §3, §5.3 |
| FR-026 (read-only) | `parse_otlp_traces` / `sanitize_traces` read only |
| FR-028 / NFR-001 (determinism) | §5.1 |
| Step 40 gate (sanitize, strip, hash) | `tests/step40_otlp.rs` — 27 tests |
| Schema contract | `schemas/telemetry-fusion.schema.json`, `tests/schema_conformance.rs` |
