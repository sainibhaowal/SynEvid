# Evidence 23 — Phase 4.2 Pillar 4 (part 2): AST-to-Span Correlation & Topology Diff (Step 41)

**Date:** 2026-10-16
**Scope:** Step 41 of `Resource/4.2/04_4.2_IMPLEMENTATION_SEQUENCE.md`.
**Crate:** `crates/autopsy-telemetry` (extends the Step 40 crate)
**Predecessor:** `docs/evidence/22_phase4_2_pillar4_otel_ingest.md` (Step 40)

---

## 1. What the step had to prove

Verbatim from the implementation sequence:

> **Step 41** — *AST-to-Span Correlation & Topology Diff* — `autopsy-telemetry` —
> **Maps `code.function` and `http.route` spans to static `SymbolId`; identifies dead code
> and undocumented endpoints.**

and from §4.2 of the Pillar 4 design:

> **Trace Topology Overlay.** Overlays runtime execution frequency onto static graph paths:
> *Dead Code Detection:* Static code paths with zero runtime invocations over N observation
> windows. *Undocumented Endpoints:* Runtime routes observed without corresponding OpenAPI
> static contracts.

Three sentences, of which only the first is a lookup. The other two are **negative claims
about the absence of evidence**, and that is where this step's entire risk lives.

---

## 2. The two claims that can be false, and what stops them

### 2.1 "This code never ran"

A reader hears an assertion. It is false in at least four situations the engine can see
coming, so each is handled before the claim is made:

| Situation | Handling |
|---|---|
| The export was **sampled** | Finding withheld — [`InsufficientEvidence`] — because absence is exactly what a sampled export looks like for a function that ran fine |
| Fewer observation **windows** than the caller required | Same: withheld |
| **Zero** windows | No findings at all, `Unknown` coverage, and a note |
| The language has **dynamic dispatch** | Finding emitted, coverage `Partial`, reason stated in the report's notes |

The two inputs the engine *cannot* derive from a payload are therefore caller assertions
with conservative defaults — `complete_observation: false`, `dispatch: Dynamic`:

```rust
TopologyOptions::new(NonZeroU32::new(3)?)   // three windows
    .with_complete_observation(true)         // the exports are unsampled
    .with_dispatch_model(DispatchModel::Static)  // a call names its callee
```

Only when both are relaxed does a finding reach `Verified`. This mirrors Step 40's salt: a
value that weakens a guarantee is **named at the call site**, never defaulted into. A
`Default` here would mean every report silently claimed the strongest reading.

`required_windows` is a `NonZeroU32`, because "zero windows required" is not a budget.

### 2.2 "This endpoint has no contract"

Unanswerable if no contract was loaded. An **empty** `DeclaredSurface::routes` is
indistinguishable from a service that genuinely declares no routes, so the engine emits
**no undocumented findings at all** and reports `Unknown` coverage with the reason in the
notes. Reporting every observed route as a violation of a document nobody read is the same
error as approving an empty Pact (Step 39) or an unchanged-but-unmodelled document
(Step 38, `Unknown`, §4.1 of the Step 38 evidence).

Symmetrically, the absence of a span is never read as "documented".

### 2.3 The withheld findings are a separate list, not a weak finding

`insufficient_evidence` is its own field on the report, not a `DeadCodeFinding` with a low
coverage. Merging them would let a reader counting findings see a code path called dead
when the engine had said only that it did not look long enough — and the count is the thing
that reaches a summary line or a dashboard.

Each withheld entry names its subject (`EvidenceSubject::Symbol(SymbolId)` or
`EvidenceSubject::Route { .. }`) and its reason. A count of withheld findings with no way
to ask *which* would be unusable.

---

## 3. Design decisions

### 3.1 The static side is supplied, not parsed

`DeclaredSurface { routes, symbols }` is an **input**. This crate does not read an OpenAPI
document and does not walk a language AST. Three reasons:

1. **Two definitions in two places drift.** Canonicalising a route template is a rule
   (`canonical_route_template`); doing it once in the contract engine and again here is how
   the two answers end up disagreeing about `/users/{id}`.
2. **Protocol-agnostic.** A route is a method and a template. An OpenAPI operation, an
   AsyncAPI channel and a gRPC method all reduce to that, and coupling this crate to one
   protocol's parser would mean editing it for the next one.
3. **The DAG stays a DAG.** `autopsy-telemetry` depends on `autopsy-domain` only. The
   symbol table arrives through `DeclaredSymbol`, whose `From<&Symbol>` impl is the
   one-line boundary — which is also what makes the gate's *"maps to static `SymbolId`"*
   literal: the report carries `SymbolId`, the domain type.

The same decision as Step 40's, where the crates had to avoid reaching across the DAG for a
contract engine. **Stated limitation:** the CLI that wires this together is Step 44, and
until then nothing in production populates a `DeclaredSurface`.

### 3.2 A placeholder is erased only as a whole segment

```
/orders/{id}          ─┐
                       ├─▶  /orders/{}        one route
/orders/{orderId}     ─┘
/files/{name}.json    ──▶  /files/{name}.json  literal
```

The whole-segment rule mirrors Step 39's concrete-path subsumption, and it is
deliberately the *under-matching* direction: an unmatched observation is visible in the
report, a wrong match is not. That is the same reason Step 39 treats `/files/{name}.json`
as literal.

### 3.3 Ambiguity is a distinct classification, never a coin flip

`CorrelationBasis::AmbiguousSymbol` is split from `CorrelationBasis::Unmatched`. The two
read identically as "not placed" and are fixed in **different places** — a missing binding
versus two symbols the symbol table should have distinguished. Collapsing them would hide
a `SymbolId` collision behind an ordinary gap. (The same reasoning as Step 39's split of
`PactMethodNotDeclared` from `PactInteractionUnmatched`.)

An ambiguous span marks **nothing** alive: it cannot count as evidence that either
candidate ran, so both remain unreported and a finding is made about each.

### 3.4 A path with no `http.route` is matched, and the inference is labelled

When the instrumentation recorded `url.path` but no `http.route`, the concrete path is
matched against declared templates by subsumption (fewest placeholders first, then
lexicographic, so declaration order never decides). This is an **inference** — the concrete
path may be a special case of the template — so `ObservedPathSubsumption` degrades coverage
even when it succeeds, and `is_inferred()` says so.

Two equally specific but structurally *different* shapes (`/a/{id}` versus `/{prefix}/3`
for `/a/3`) are reported rather than chosen. Two declarations of the *same* shape are **not**
an ambiguity — that is one route declared twice, which is a contract smell, so it is noted
and degrades coverage but does not make the route undocumented.

### 3.5 The method is checked, and the check is disclosed when it cannot be

`DELETE /orders/{id}` against a contract that declares `GET` on that shape is an
undeclared endpoint, not a match — the shape is documented, the *endpoint* is not.

A route declared **without** a method matches any observation (so it is not reported as
undocumented) but degrades coverage with a note, because a contract that cannot catch a
wrong method must not read as one that checked it. Likewise, an observation that carries no
method degrades coverage.

### 3.6 Every unplaceable span costs coverage, and the count is stated

A span carrying none of `http.route`, `code.function` or `rpc.service`+`rpc.method` cannot
be placed, and it is precisely the **false-negative source for the dead-code pass**: it
might be the missing evidence that a symbol ran. So it degrades coverage and the report
states how many there were. `Partial`, never a silent pass.

---

## 4. Defects found and fixed

### 4.1 A declared-but-uninvoked route left no trace in the report at all

Found by a **control assertion** — `the_digest_moves_when_the_static_surface_moves` added a
declared route to the surface and asserted the digest must move. It did not, and the digest
was right: the report had nowhere to record an uninvoked declared route.

This is a real gap, not a test artifact. The mirror of *"an observed route no contract
declares"* is *"a declared route nothing calls"* — one of the two halves of the topology
diff — and without it:

* a reader could not ask the obvious question of a contract, and
* the digest did not cover the contract's surface, so two genuinely different contracts
  produced the same report.

Fixed by adding `UninvokedRoute` with the same evidence discipline as dead code: asserted
only when enough windows were observed *and* the export is unsampled; otherwise withheld
with a subject and a reason. `InsufficientEvidence` gained `EvidenceSubject` so it can name
a route as well as a symbol.

The lesson is worth recording separately: **the digest test that caught this was written as
a control for the determinism test above it.** A determinism assertion that holds for
everything passes for the wrong reason; this one was written as "equality is not a
property", and it found a missing feature.

### 4.2 `CorrelationBasis::Rpc` named a capability that did not exist

The variant was declared and never produced by any code path. Nothing in this crate maps
`rpc.service`/`rpc.method` to a symbol, because no adapter emits `SymbolId`s for gRPC
handlers yet. A variant naming a capability the engine does not have is a claim that reads
as a supported protocol, so it was **removed** and an RPC span is instead disclosed: the
report notes that no static surface is modelled for RPC handlers, so such spans are not
correlated and cannot mark a symbol invoked. The gap is visible and named rather than
implied to be handled.

### 4.3 A second pass recomputed the same decision

`correlate_common` walked every span a second time to rebuild the observed-symbol set that
`correlate_span` had already built, and its doc comment claimed it "records the ambiguity
count in the notes surface" while recording nothing. It was deleted: two places making the
same decision is how they come to disagree.

### 4.4 An ambiguity that was not one, and one that was

The first version of `subsumption_prefers_the_most_specific_template_and_reports_ties`
asserted that `/a/{id}` and `/a/{name}` are a tie. Erasing placeholder names is the entire
purpose of `shape()`, so they are one route declared twice — the assertion was wrong and
the engine was right. The test now asserts the correct behaviour for same-shape templates
(deterministic choice, no tie) **and** a genuine tie between two structurally different
equally-specific shapes.

### 4.5 Ten stale intra-doc links, and a gate that now catches them

Running `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` while checking the new
module's links found four broken links — `[R24]` and `[R27]` (requirement ids used as link
targets, from Steps 40 and 39) and two `[CoverageState::…]` links in `autopsy-test-impact`
where the type was not in scope. Rustdoc warnings are not errors by default, so nothing was
failing. All four were fixed and **a new pre-commit hook enforces it** rather than leaving
the check to whoever thinks to run it:

```
- id: cargo-rustdoc-links
  name: Rustdoc Intra-Doc Link Check (-D warnings)
  entry: bash -c '… RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps'
```

---

## 5. Verification telemetry

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS (0 warnings) |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | PASS (new gate) |
| `cargo test --workspace --all-targets` | **650 passed / 0 failed** (was 602, +48) |
| `cargo test --workspace --doc` | PASS (20 doc-test targets) |
| `./scripts/verify-determinism.sh` | PASS — 100/100 byte-identical |
| `./scripts/verify-arch-boundaries.sh` | PASS — no core→MCP edge, no sub-process, no socket |
| `pre-commit run --all-files` | PASS (**all 11 hooks**) |

Test breakdown for `autopsy-telemetry` (107 total, was 59):

| File | Tests | |
|---|---|---|
| `src/topology.rs` (unit) | 5 | + the 21 already in `redact`/`span` |
| `src/redact.rs` + `src/span.rs` (unit) | 21 | |
| `tests/step41_topology.rs` | 31 | new |
| `tests/topology_schema_conformance.rs` | 12 | new |
| `tests/step40_otlp.rs` | 27 | unchanged |
| `tests/schema_conformance.rs` | 11 | unchanged |

### 5.1 Determinism is asserted, not inferred

* `the_report_is_reproducible_across_100_runs` — 100 runs, same digest and same structure.
* `the_order_the_windows_are_handed_over_does_not_change_the_report` — two records swapped,
  identical digest.
* `the_digest_does_not_depend_on_the_notes` — notes are prose derived from facts already in
  the digest, so rewording one must not move it.
* **`the_digest_moves_when_the_static_surface_moves`** — the control, and the test that
  found §4.1.
* Every list is sorted before the digest: correlations by `(span_id_hash, …)`, findings by
  symbol, routes by `(template, method)`, withheld entries by subject.

### 5.2 The schema drift guard covers the whole report vocabulary

`schemas/topology-report.schema.json` is closed at every level (including across the
`oneOf` variants of `evidenceSubject` and `occupancySubject`) and draft 2020-12. The
conformance test compares it against the Rust side **through serde**:

* every `CorrelationBasis` variant (5), against the exhaustively listed array;
* every `DispatchModel` and `InsufficientReason` variant;
* every `CoverageState` variant, and both `EvidenceSubject` tags read off real
  serializations;
* every field the report and each nested record emits is declared, and every required
  field is emitted;
* `observed_invocations` is `const: 0` on both finding types — the claim is carried as a
  field rather than inferred from an absence;
* every digest is lowercase BLAKE3 hex;
* **no raw span or trace identifier reaches the report**, asserted on real output.

The fixture is built to reach every finding list, with `the_fixture_reaches_every_finding_list`
as a **guard on the guard** — if the fixture stopped reaching a branch, the tests above
would keep passing while checking less and less.

---

## 6. Honest scope statement — what is **not** done

1. **Nothing populates a `DeclaredSurface` in production.** The CLI subcommand is Step 44;
   until then this engine is reachable only from tests and library callers.
2. **No RPC correlation.** `rpc.service`/`rpc.method` are read and disclosed as
   uncorrelated (§4.2); a protobuf-aware static surface does not exist.
3. **No call-graph reachability.** A function whose only caller is itself dead is reported
   as invoked-if-observed and otherwise as a finding — the engine overlays observations onto
   declared *symbols*, not onto the dependency graph, so it does not compute
   "unreachable from an entrypoint". That is a different and stronger claim.
4. **Dead code is per-symbol, never per-line or per-branch.** A partially executed function
   is invoked; coverage inside it is not measured.
5. **Observation windows are not deduplicated or weighted.** One `TraceRecord` is one
   window by contract; three exports of the same hour count as three windows. Callers
   holding overlapping slices must decide the window boundary themselves.
6. **Dynamic dispatch is a caller assertion, not a detection.** The engine cannot see
   reflection, DI containers, or string-built method names, and defaulting to `Dynamic`
   only means the finding is `Partial` — it does not make it safe.
7. **A route's contract is matched by shape and method only.** Content-type, versioning,
   host, and security are not compared; a route can be "documented" by an operation served
   under a different media type.
8. **`example_path` is one observed path, not a histogram.** Which concrete paths hit an
   undeclared route is reported as one example plus an invocation count.
9. **The report has no storage write-back.** No `telemetry_*` table is written; the report
   is a value.

---

## 7. Boundary compliance

* **Zero LLM in correctness crates** — no LLM call, import, or network path.
* **Read-only analyzer (FR-026)** — spans in, a report out. No filesystem write, no git
  mutation.
* **100/100 determinism (NFR-001, FR-028)** — every collection is a `BTreeMap`/`BTreeSet`
  or sorted before serialization; the digest is taken over an explicitly named projection
  with `notes` and `digest` removed, and never over source bytes.
* **Honest coverage (FR-014, FR-030)** — an unplaceable span, an inferred placement, a
  method-agnostic declaration, a duplicate route shape, an unmodelled RPC span, a
  never-called route and a maybe-sampled export all degrade coverage; a withheld finding is
  a separate list from a finding; zero windows is `Unknown` and never a pass.
* **INV-CROSS-001** — paths are normalised POSIX-style (`\` → `/`, leading `./` stripped) so
  a symbol table and an instrumented file agree on every platform; no timestamp reaches the
  digest.
* **Architectural inversion (`ARCH_NO_CORE_TO_MCP`)** — `autopsy-telemetry` depends on
  `autopsy-domain` and the Step 40 crate's own modules only. No contract engine, no language
  adapter, no CLI, no MCP.
* **No new dependencies** — `serde`, `serde_json`, `blake3`, `thiserror`, `autopsy-domain`.
  No graph library, no glob, no URL parser.

---

## 8. Traceability

| Requirement | Where |
|---|---|
| Step 41 gate (map `code.function`/`http.route` to `SymbolId`) | `src/topology.rs`; `tests/step41_topology.rs` — `gate_a_span_naming_a_function_maps_to_the_declared_symbol`, `gate_a_span_naming_an_http_route_maps_to_the_declared_route` |
| §4.2 Dead Code Detection (zero invocations over N windows) | `DeadCodeFinding`, `InsufficientEvidence`, `TopologyOptions::required_windows` |
| §4.2 Undocumented Endpoints (runtime route without a static contract) | `UndocumentedRoute`; `gate_an_undocumented_endpoint_is_an_observed_route_no_contract_declares` |
| §4.2 Trace Topology Overlay (obs. frequency onto static paths) | `SpanCorrelation`, `UninvokedRoute`, `ObservationSummary` |
| FR-014, FR-030 (honest coverage) | §2, §3.3–3.6, §6 |
| FR-026 (read-only) | `correlate_topology` reads only |
| FR-028 / NFR-001 (determinism) | §5.1 |
| Schema contract | `schemas/topology-report.schema.json`, `tests/topology_schema_conformance.rs` |
| INV-CROSS-001 (path normalisation) | `normalise_path`; asserted in `a_symbol_converts_from_the_domain_type_at_the_boundary` |
