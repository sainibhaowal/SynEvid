# Evidence 24 — Phase 4.2 Pillar 5 (part 1): Historical Git Co-Change Mining (Step 42)

**Date:** 2026-10-16
**Scope:** Step 42 of `Resource/4.2/04_4.2_IMPLEMENTATION_SEQUENCE.md`.
**Crate:** `crates/autopsy-mining` (new)
**Predecessor:** `docs/evidence/23_phase4_2_pillar4_topology.md` (Step 41)

---

## 1. What the step had to prove

Verbatim from the implementation sequence:

> **Step 42** — *Historical Git Co-Change Mining* — `autopsy-mining` — Mines 6-month commit
> history `R31`; calculates Support, Confidence, Lift; tags edges with non-causality warning.

and from §5.1 of the Pillar 5 design:

> **Non-Causality Boundary `R32``R33`:** High co-change lift indicates *temporal
> co-occurrence*, not architectural dependency. The report clearly flags
> `EdgeKind::StatisticalCoChange` as distinct from `EdgeKind::StaticCall`.

The metric is arithmetic. The **step** is a claim about what the arithmetic means, and the
two ways to fail it are opposite:

* read a correlation as a dependency, or
* report a percentage over a population the reader cannot see.

Both are handled structurally rather than by prose.

---

## 2. The metric, and why a float never enters

```text
Support(A, B)     = |A ∧ B| / |Total|
Confidence(A → B) = |A ∧ B| / |A|
Lift(A, B)        = P(A ∧ B) / (P(A) · P(B))
```

Every ratio is reported as **parts per million in integer arithmetic** (`*_ppm: u64`), and
every ratio is accompanied by the four counts it came from (`commits_together`,
`commits_with_a`, `commits_with_b`, `total_commits`).

1. **`NFR-001`.** A digest over `f64` inherits float formatting. A percentage that moves
   between platforms because a decimal rendered differently is a determinism bug that looks
   like a data change. Integers have one spelling.
2. **Auditability.** A reader who wants to check Support divides two numbers that are right
   there, instead of re-deriving the population the engine chose.

The ppm rendering is therefore a *rendering*: the counts are authoritative and the ppm value
is for ordering and thresholding. A schema conformance test asserts every proportion is
bounded by `1_000_000` and that `lift_ppm` is **not** bounded — a rare pair that always
travels together produces a very large number, and clamping it would hide the figure that
most invites the wrong reading.

### 2.1 Confidence is directional; the pair is not

Co-change is symmetric — `A` changed with `B` exactly when `B` changed with `A` — but
Confidence is not: `|A ∧ B| / |A|` and `|A ∧ B| / |B|` answer different questions. Both are
reported, and a pair is stored with `a < b` so the **identity** of an edge does not depend on
which direction a reader happened to ask about.

### 2.2 A Wilson score interval, and what it does *not* say

Each confidence carries a 95% Wilson score interval
(`confidence_a_to_b_lower_ppm` / `_upper_ppm`, and the same for `b → a`). Wilson rather than
the normal approximation because the normal approximation is worst exactly where this engine
reports most: small proportions and proportions at the boundary. `Confidence = 1.0` over
three commits is the case Wald renders as a **zero-width** interval, which is the opposite of
the truth.

The interval answers one question: how much of this proportion could be the accident of
which commits fell inside the window? It says **nothing** about whether `A` depends on `B`.
A tight interval around `1.0` means "every commit that touched `A` also touched `B`, and that
is unlikely to be sampling noise" — and a code generator, one reviewer's habit, and a release
script all produce exactly that. Both the report note and the crate docs state this, and a
test asserts the sentence is present.

Two exact values of the bound are *known* and are snapped rather than left a fraction of a
ppm short of certainty: if every trial succeeded the upper bound is exactly `1_000_000`, and
if none did the lower bound is exactly `0`.

---

## 3. The non-causality boundary is structural, not a paragraph

`EdgeClaim` has **one variant**, `TemporalCoOccurrenceOnly`, and it is a **field of every
edge**:

```rust
pub struct CoChangeEdge {
    pub a: String,
    pub b: String,
    pub kind: EdgeKind,      // StatisticalCoChange
    pub claim: EdgeClaim,    // TemporalCoOccurrenceOnly
    ...
}
```

A `bool` could be set wrong. A single-variant enum cannot be omitted, misread, or defaulted
into existence, and a reader who slices one edge out of a report is precisely who the warning
is for.

`EdgeKind::StatisticalCoChange` was added to `autopsy-domain` alongside `Calls`, `Imports`,
and the rest, so the distinction the design asks for holds **at the type level**: a consumer
filtering for `Calls` can never pick up a co-change edge by accident, and a consumer that
wants co-change has to name it. The design's illustrative name `EdgeKind::StaticCall` does not
exist in the model; the call edge is `EdgeKind::Calls`, and the test asserts
`StatisticalCoChange != Calls` rather than relying on the prose.

**Stated limitation:** no path → `SymbolId` resolution is attempted. A path is not a symbol,
and `SymbolId`'s documented format is
`namespace::module::parent#kind:name:signature_hash` — fabricating one from a file path would
be a lie in the field the whole graph is keyed on. The same boundary decision as Step 41's
`DeclaredSymbol`: the join belongs where the symbol table is.

---

## 4. The population is accounted for, exhaustively

Support is a ratio, and it is only meaningful if the reader knows what the denominator was.
So the window carries an **exhaustive** account rather than incidental counters:

```text
  commits_seen == commits_considered + Σ excluded[*]
```

A test asserts that identity. The failure it prevents is silent: a commit quietly dropped
from the denominator **raises** every support figure in the report while looking like nothing
happened.

| Excluded | Reason | Biases the population? |
|---|---|---|
| `merge_commit` | The first-parent diff of a merge is a union of a whole branch's work: not one change set, and it manufactures pairs | no |
| `root_commit` | An initial import is not co-change; it is a clique over the entire repository | no |
| `mass_commit` | Above `max_files_per_commit` the same argument applies to a vendoring or reformatting commit, and it bounds the pair count | **yes** |
| `outside_window` | The sliding window is the point of the metric | no |
| `missing_timestamp` | Window membership cannot be decided, so the commit cannot be placed | **yes** |
| `undecodable_tree` | The change set is unknown, and counting it as *empty* would claim the commit changed nothing | **yes** |
| `no_changed_paths` | An empty commit contributes no pair and is not evidence either way | no |

The three that *bias* the population degrade coverage to `Partial` with a note saying the
counts are a lower bound. Removing a merge commit is a correction; removing a real edit that
happened to touch five files is a loss. Conflating the two would be the dishonest direction.

Degradations, in full: a population of fewer than two commits (every ratio is trivially 1 and
meaningless); any population-biasing exclusion; a history walk that stopped at its bound; a
pair set that reached its bound; an edge list truncated at `max_edges`; and — per edge rather
than per report — an edge resting on fewer than three commits, where lift is dominated by
small-count noise (`R32`).

An **empty** window is `Unknown` with no edges, never a report of zero couplings. An empty
repository is not an error: it genuinely has no co-change evidence, and the report says so
rather than implying the code is uncoupled.

### 4.1 Per-edge coverage is not report coverage

`the measurement covered the population completely` and `this particular number rests on two
commits` are different statements, and they get the same field at different scopes. A test
asserts a report that is `Verified` while every edge in it is `Partial`, because that is the
state the fixture is genuinely in.

---

## 5. Determinism without hiding the clock

`NFR-001` forbids timestamps in canonical outputs. The fix is not to avoid time but to keep it
out of the **identity** of the result:

* the window's anchor is the newest commit's `committer` timestamp **read from the
  repository** — never `SystemTime::now()`, so the same repository always yields the same
  window, and a caller who wants a specific point in time passes an explicit integer;
* what enters the digest is the **set of commit ids that were considered** — content-addressed
  and timestamp-free — plus the window *width* in seconds (a duration, not a date);
* a whole-document schema test asserts no property anywhere in the schema names a clock
  reading, and the integration test asserts no fixture timestamp reaches the serialized
  report.

The window is closed at **both** ends. See §7.1: it was not, and that was a defect.

---

## 6. Renames, and the limit of what is detected

A pure rename — path `A` disappears and path `B` appears carrying the **identical blob** — is
collapsed and counted (`pure_renames_collapsed`). Without it, moving a directory of forty
files produces a 780-pair clique for a commit in which nothing was edited.

Pairing is by blob id, and the **unpaired surplus is restored as a real change**: one blob
copied to three paths is one rename and two additions, not one rename and two silent
omissions. A `zip`-style implementation would drop the surplus, so the test counts it.

**Stated limitation:** only the identical-blob case is detected. A move that also edits
appears as a deletion plus an addition and is mined as two unrelated changes. Disclosed in the
report's notes rather than left for the reader to discover.

Submodule entries (`gitlink`, mode `0o160000`) are skipped: a submodule pointer is a path in
*another* repository, and coupling this repository's files to another one's would be exactly
the false dependency this step must not produce. Tree recursion is bounded at 64 levels.

---

## 7. Defects found and fixed

### 7.1 The window had no upper bound

`an_explicit_reference_timestamp_anchors_the_window_without_a_clock` expected 3 commits and
got 6. It was right: with an anchor in the past, the window `[anchor − width, ∞)` included
every commit *after* the anchor. A window that silently grows to the present while claiming to
be historical is a claim the engine cannot support.

Fixed: the window is the closed interval `[anchor − window_seconds, anchor]`, and a commit
dated after the anchor is counted as `outside_window`. Clock skew and rebases do produce such
commits; they are now visible in the accounting rather than quietly included.

### 7.2 The Wilson half-width used `z` where the algebra requires `z/2`

The half-width is
`z · √(4x(n−x) + z²n) / (2·√n·(n + z²))`; the derived coefficient is `98 = z·scale/(2·√scale)`
and the code carried `196`. Caught by a unit test asserting a *known* bound: with
`x = n = 3`, the Wilson upper bound is exactly 1, and the code returned `0.999999` before the
scaling bugs were found.

### 7.3 Truncating `isqrt(n)` destroyed precision for small samples

The half-width divides by `√n`. Computing `isqrt(n)` separately truncates to `1` for every
`n < 4` — the case this engine reports most, and the case the normal approximation also gets
wrong. Fixed by lifting the radicand by `SQRT_SCALE²` *before* the root and dividing by
`SQRT_SCALE`, so the two roots are one. `wilson(3, 3)` now returns a lower bound of ~0.439
instead of 0.

### 7.4 Test fixtures collided, and the failure looked like a data bug

Twelve tests failed with `commits_considered = 4` instead of 6, and a `write_loose` unwrap
panic. The cause was not the engine: the shared fixture builder keys its scratch directory on
the **process id**, and every test in a binary shares one process — so several tests asking
for the same fixture name deleted each other's git objects mid-run. Fixed with a per-call
counter. Worth recording because the symptom (`wrong commit count`) pointed at the engine.

### 7.5 Two assertions of mine were wrong, and one hid a real gap

* `the_integer_square_root_is_exact_on_perfect_squares` expected `√(2¹²⁸−1)` to be about
  `2⁶³`. It is `2⁶⁴−1`. The function was right.
* `gate_a_mass_commit_is_excluded_and_degrades_coverage` used five changed paths against the
  **default** bound of thirty, so nothing was a mass commit. The bound is the parameter under
  test; it now sets it. The same test then failed for a *correct* reason — excluding the mass
  commit drops the pair to a single observation, below the reporting floor — which is exactly
  the behaviour worth having a test for.
* The bound test checked only one side of the boundary; it now asserts that a commit changing
  *exactly* the bound is mined and one changing a single path more is not.

### 7.6 Requirement ids as intra-doc links

`[R31]`, `[R32]`, `[R33]` in doc comments are read by rustdoc as links to items named `R31`,
and the new `-D warnings` document gate (added in Step 41) failed. Ten occurrences across the
domain change and the new crate were changed to inline code.

---

## 8. Verification telemetry

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS (0 warnings) |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | PASS |
| `cargo test --workspace --all-targets` | **710 passed / 0 failed** (was 650, +60) |
| `cargo test --workspace --doc` | PASS |
| `./scripts/verify-determinism.sh` | PASS — 100/100 byte-identical |
| `./scripts/verify-arch-boundaries.sh` | PASS — new crate covered by the `crates/*` glob |
| `pre-commit run --all-files` | PASS (all 11 hooks) |

Test breakdown for `autopsy-mining` (60 total, new crate):

| File | Tests |
|---|---|
| `src/history.rs` (unit) | 4 |
| `src/cochange.rs` (unit) | 8 |
| `tests/step42_mining.rs` | 34 |
| `tests/schema_conformance.rs` | 14 |

### 8.1 The fixtures are real repositories, built by hand

The integration tests build git repositories **byte by byte** — loose blobs, trees and commits
written through the byte-level encoders that `autopsy-temporal`'s test toolkit already owns and
that this crate reuses by path. No `git` sub-process is executed, so the zero-sub-process
invariant holds under test as well as in production, and the suite is hermetic.

Every metric assertion is against a hand-computed value. A test that only checked
`support <= 1_000_000` would pass for an engine returning a constant, and the risk in this step
is a number that is *plausible* and wrong.

### 8.2 Determinism is asserted, not inferred

* `the_same_repository_yields_the_same_window_across_100_runs` — 100 runs, same digest and
  same structure.
* `the_digest_carries_no_timestamp_but_does_carry_the_commit_set` — no fixture timestamp
  reaches the report, the anchor is a 40-hex commit id, and a different window is a different
  digest (**the control**: an equality that holds for everything is not a property).
* `the_record_order_does_not_change_the_report` — the same change sets in the opposite order
  produce one digest.
* `the_commit_set_digest_is_order_independent_and_timestamp_free`.

### 8.3 The schema guard covers the report vocabulary

`schemas/cochange-report.schema.json` is closed at every level and draft 2020-12, with:

* every `ExclusionReason` compared against `ExclusionReason::all()` through serde;
* every `CoverageState` variant;
* `claim` pinned to `const: "temporal_co_occurrence_only"` and `kind` pinned to `const:
  "statistical_co_change"`, both read off real serializations;
* the proportion bound checked **through the `$ref`** rather than by reading an inline
  `maximum` that is not there — a test reading the inline key would pass on a schema whose
  field had quietly lost its bound;
* every `*_ppm` proportion referencing the bounded definition, and `lift_ppm` asserted to have
  no bound;
* an assertion that no property anywhere in the document names a clock reading;
* `the_fixture_reaches_every_branch_it_claims_to` as a **guard on the guard**.

---

## 9. Honest scope statement — what is **not** done

1. **The metric is file-path level.** Symbol-level co-change would need a path → `SymbolId`
   mapping from a language adapter, and Pillar 1's symbol-level temporal extraction is still
   not wired to the adapters (§3).
2. **Rename detection is identical-blob only.** A move that also edits is mined as a deletion
   plus an addition (§6).
3. **No causality, ever, at any confidence.** The interval covers sampling noise in the
   window; it is not a hypothesis test for a dependency, and there is no threshold at which
   an edge becomes a dependency.
4. **No author, message or diff analysis.** Attributes are read from the *commit object*
   (parents, tree, timestamps) and never from a diff hunk, so no line-level co-change exists
   and no commit message is mined.
5. **Window boundaries are the caller's.** One window is one call; there is no sliding
   re-computation, no decay weighting, and no overlap handling.
6. **The 6-month default is a constant, not a policy.** `DEFAULT_WINDOW_SECONDS = 183 days`;
   the design says "e.g. 6 months" and this crate does not argue for a number.
7. **Cross-repository mining is impossible.** One repository per call.
8. **No storage write-back.** No `cochange_*` table is written; the report is a value.
9. **The CLI does not exist yet.** Step 44 wires a subcommand; until then this engine is
   reachable only from tests and library callers.

---

## 10. Boundary compliance

* **Zero LLM in correctness crates** — counting and integer division, nothing else.
* **Read-only analyzer (`FR-026`)** — history is read through `autopsy-temporal`'s pure-Rust
  object store. No sub-process, no network, no write, and git history is never mutated. Git
  state is read through a store that never runs `git`.
* **100/100 determinism (`NFR-001`, `FR-028`)** — no clock reading; ratios are integers; the
  digest is taken over a canonical projection with `notes` and `digest` removed; every
  collection is a `BTreeMap`/`BTreeSet` or explicitly sorted.
* **Honest coverage (`FR-014`, `FR-030`)** — §4. An empty repository is `Unknown`, a biased
  population is `Partial`, and every degradation has a stated reason.
* **Bounded traversal (`INV-SANDBOX`)** — commit count, tree depth, paths per commit, distinct
  pairs, and reported edges are all capped, and reaching any cap is disclosed.
* **Architectural inversion (`ARCH_NO_CORE_TO_MCP`)** — `autopsy-mining` depends on
  `autopsy-domain` and `autopsy-temporal` (core on core). No contract engine, no adapter, no
  CLI, no MCP.
* **Supply chain** — no new *production* dependency: `autopsy-domain`, `autopsy-temporal`,
  `blake3`, `serde`, `serde_json`, `thiserror`. `flate2` is a **dev**-dependency only, for the
  fixture encoder.
* **One documented DAG edge is realised differently, and deliberately.**
  `02_4.2_REPO_BUILD_CRATES_INTEGRATION.md` draws `Mining --> Repo`. `autopsy-repo` is
  configuration loading and language classification; it holds **no** git object reader, and the
  read-only store this step needs is Pillar 1's `autopsy-temporal`. Depending on
  `autopsy-repo` would therefore have meant either moving the git reader there (a Pillar 1
  change with no benefit to Pillar 1) or re-implementing it here (two readers, two answers).
  The edge is `Mining --> Temporal`, which is core-on-core and keeps the read-only store in
  one place. Recorded because a reader comparing that graph to the manifest would otherwise
  reasonably assume an accident.

---

## 11. Traceability

| Requirement | Where |
|---|---|
| Step 42 gate (mine history; Support/Confidence/Lift; non-causality tag) | `src/cochange.rs`; `tests/step42_mining.rs` — `gate_support_confidence_and_lift_are_the_hand_computed_values`, `gate_every_edge_is_tagged_as_temporal_co_occurrence_and_not_a_dependency`, `gate_the_population_is_fully_accounted_for` |
| §5.1 Support / Confidence / Lift formulas | `ratio_ppm`, `CoChangeEdge`, §2 |
| §5.1 sliding window | `HistoryOptions::window_seconds`, `history::recent_changes`, §5 |
| §5.1 non-causality boundary `R32``R33` | `EdgeClaim`, `EdgeKind::StatisticalCoChange`, `EdgeClaim::WARNING`, §3 |
| Non-causal confidence interval | `wilson_interval_ppm`, `confidence_*_lower_ppm`/`_upper_ppm`, §2.2 |
| `FR-014`, `FR-030` (honest coverage) | §4, §9 |
| `FR-026` (read-only) | `history::recent_changes` reads only; no sub-process |
| `FR-028` / `NFR-001` (determinism) | §5, §8.2 |
| Schema contract | `schemas/cochange-report.schema.json`, `tests/schema_conformance.rs` |
| `INV-SANDBOX` (bounds) | `MAX_TREE_DEPTH`, `max_commits`, `max_files_per_commit`, `max_pairs`, `max_edges` |
