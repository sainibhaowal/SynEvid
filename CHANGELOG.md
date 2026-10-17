# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - Tier 2 agent quality (2026-10-17)

Implements all five Tier-2 items from `Resource/Agent_Advancement_Roadmap.md`; see
`docs/evidence/36-tier2-agent-quality.md`.

### Added
- **Go adapter (`AGENT-T2.2`):** `autopsy-adapter-go` — full symbol graph
  (functions, receiver-typed methods, structs/interfaces/type aliases, package
  vars/consts, uppercase-rune export visibility), struct + interface embedding
  inheritance edges, filesystem-anchored import resolution (`go_import:` provenance)
  with honest name-level `Partial` edges for unresolvable imports, package-qualified
  call resolution through an import-alias map that degrades ambiguities to name-level partials, and honest coverage
  (`Unknown` for cgo/plugin/reflect, `Partial` for `unsafe`/`//go:generate`).
- **Rust adapter (`AGENT-T2.2`):** `autopsy-adapter-rust` — module bridge
  (`crate::`/`super::`/`self::` file probes preferring `<path>.rs`, nested
  `use` flattening with alias bindings), struct/enum/trait/alias symbols with impl
  method scopes, `Implements`/`Inherits` trait edges, bare + qualified call
  resolution with honest name-level method calls, `unsafe`/`async` contract
  effects, and `Unknown` for `include!` inlining.
- **Repair advice (`AGENT-T2.1`):** every CLI error (execution and argument-parse
  alike) prints a deterministic `what_failed` / `why` / `next_action` triple; the MCP
  envelope carries it as `repair` exactly when the engine emitted its markers.
- **Session receipts (`AGENT-T2.5`):** `autopsy session start/record/show/close/list`
  — deterministic BLAKE3 session ids, no timestamps, honest close verdicts
  (`pass`/`fail`/`unknown`) derived only from recorded steps; `autopsy_session`
  MCP tool; opt-in `agents/hooks/autopsy-session.sh` pre-commit wiring.
- **Workspace registry CLI (`AGENT-T2.3`):** `autopsy workspace register/list/remove`
  over `.autopsy/workspace.json` with live honest status; `autopsy://workspace`
  resource.
- **Pinned browse (`AGENT-T2.4`):** `--snapshot-id` on `symbols`, `edges`, `find`,
  and `map`; MCP pinning extends to the whole browse family, `find` gains a
  `snapshot_id` field, and the graph resources accept `snapshot` + `repo_root`.

### Changed
- MCP tools with file arguments (`contract-dist` documents, blast-radius
  catalog/trace/surface, test-selection LCOV) now resolve against the tool's
  `repo_root` in one deterministic place; `repo_root` is consistent on every
  path-taking tool and resource.

---

## [Unreleased] - Tier 1 Agent Experience (2026-10-17)

Implements all five Tier-1 gaps from `Resource/Agent_Advancement_Roadmap.md`; see
`docs/evidence/35-tier1-agent-experience.md`.

### Added
- **`autopsy find` (AGENT-T1.1):** case-insensitive symbol resolution over qualified
  names, declaring paths, and stable IDs, with `--kind`, `--name-only`, `--limit`
  (ceiling 1000), `--offset`. Deterministic stable-id order and honest `total`; an empty
  result is an answer, an empty pattern refuses with exit 3 before any repository read.
  `autopsy_find` MCP tool added (10 tools).
- **`autopsy map` + `autopsy://graph/map` (AGENT-T1.3):** bounded whole-graph projection —
  symbol-kind, package (first two path segments), and edge-kind counts in `BTreeMap`
  serialization order, plus top-N nodes by degree (in+out), degree-descending with
  stable-id tie-break, `--under` prefix scope that narrows every count and rank and is
  stated in the report. Never a full graph dump.
- **Snapshot pinning (AGENT-T1.4):** `AUTOPSY_SNAPSHOT` session pin +
  per-call `snapshotId` on the MCP `baseline/impact/diff/verify` tools, mapped to
  `--baseline-id`/`--snapshot-id`/`--before`; explicit arguments always win over the pin.
  `AUTOPSY_TIMEOUT_MS` raises the kill-switch ceiling for large repositories (garbage and
  zero values are ignored).
- **`--compact` / `--top-k` token budgets (AGENT-T1.5):** on `temporal`, `lineage`,
  `contract-dist`, `telemetry`, `cochange`, `catalog`, `blast-radius` and the MCP temporal /
  contract-diff / blast-radius tools. Slice respects each engine's canonical order,
  names `_omitted` residuals, flags `"compact": true`, keeps counts, coverage, digests,
  and exit codes exact, never alters the text summary, and is applied after persistence
  so storage always keeps the full report.

### Changed
- **One envelope for answering (AGENT-T1.2):** Phase 4.1 MCP tools return
  `{command, exit_code, actionable, report}` like the Phase 4.2 tools; bare `{error}`
  JSON is gone (`refusedEnvelope` covers missing-target and missing-salt refusals with
  unchanged exit-code 3 semantics).

## [Unreleased] - Phase 4.2 (Steps 28–45, all six pillars) (2026-10-16)

Steps 28–45 of the 4.2 implementation sequence — the sequence's last step is
done, so all six pillars are complete end-to-end: engine, CLI, MCP tools and
Web Inspector. See
`docs/evidence/17-temporal-semantic-graph.md`,
`docs/evidence/18-predictive-test-selection.md`,
`docs/evidence/19-openapi-contract-diff.md`,
`docs/evidence/20-asyncapi-contract-diff.md`,
`docs/evidence/21-pact-contract-diff.md`, and
`docs/evidence/22-opentelemetry-trace-ingestion.md`, and
`docs/evidence/23-runtime-static-topology-fusion.md`,
`docs/evidence/24-git-cochange-mining.md`,
`docs/evidence/25-enterprise-service-catalog.md`,
`docs/evidence/26-unified-cli-orchestration.md`, and
`docs/evidence/27-blast-radius-visual-views.md`.

### Added
- **Global entity identity — RFC-GLOBAL-001 (`autopsy-domain::global_id`, Step 28):**
  `urn:autopsy:repo:<provider>:<org>:<repo>[#<path>[:<symbol-path>]]` as a total,
  canonicalizing parser. Provider host lowercased; org/repo case-preserved.
  Paths POSIX-normalized (leading `./` and trailing `/` stripped, `//`
  collapsed, `..` rejected). Round-trip is total: `parse(display(id)) == id`.
  Backslashes are **rejected rather than rewritten**, so one entity cannot hold
  two identities across platforms.
- **SQLite migration v2 (`autopsy-storage`, Step 29):** tables
  `temporal_facts`, `test_mappings`, `distributed_contracts`, `co_change_matrix`
  plus 8 indexes. Verified to upgrade a hand-built v1 database in place and to
  be idempotent across reopens. `schema_version()` accessor added.
  `created_at`/`window_*` are bookkeeping only and never reach a digest.
- **Read-only pure-Rust git object store (`autopsy-temporal::git`):** refs
  (`HEAD`, loose, `packed-refs`, symbolic chains, tag deref), loose objects with
  a 256 MiB inflate cap and declared-length cross-check, and packfile v2 with
  fanout-bounded index search plus `ofs-delta` **and** `ref-delta` resolution.
  This exists because `INV-SANDBOX-001..004` forbid sub-process execution: the
  engine cannot shell out to `git log`. Unsupported features (`.idx` v1,
  SHA-256 object format, alternates, linked worktrees) are explicit
  `GitError::Unsupported`, never silently misread.
- **Temporal fact journal (`autopsy-temporal::fact`, `::timeline`, Step 30):**
  append-only `TemporalFact` records with half-open `[valid_from, valid_to)`
  intervals. `fact_id = BLAKE3(subject ‖ predicate ‖ object ‖ valid_from)`,
  NUL-separated and deliberately independent of `valid_to`. Commit ordering is
  topological with object-id tie-breaking, never chronological.
- **Symbol lineage (`autopsy-temporal::lineage`, Step 31):** content-digest
  equality with a unique counterpart is the *only* path to a `Verified`
  rename/move; identical-multiplicity and similarity matches are
  `CoverageState::Partial` with the full candidate set disclosed. Similarity
  alone can never reach `Verified`, which is what makes the
  zero-false-positive-rename gate hold.
- **Point-in-time reconstruction (`autopsy-temporal::timeline`, Step 32):**
  `TemporalJournal::reconstruct(revision)` returns a `GraphSnapshot` whose
  digest is byte-identical to a fresh parse at that revision, at every revision.
  `SourceExtractor` is the extension point for adapter-supplied symbol facts.
- **Test inventory discovery (`autopsy-test-impact::inventory`, Step 33):**
  discovers Vitest, Jest, and Mocha suites from `*.test.*`, `*.spec.*`, and
  `test`/`tests`/`__tests__` directories, extracts declared test cases, and
  resolves relative imports to repository paths with extension probing and
  `index.<ext>` fallback. Test-file detection is **segment-based**, so
  `src/latest.ts` is not mistaken for a test. Dynamic `import(...)`,
  `require(...)`, and template-literal specifiers are deliberately not guessed
  at. A computed test name is addressable by position (`@line<N>`) rather than
  an invented name.
- **Observed coverage ingestion (`autopsy-test-impact::lcov`, Step 34):**
  parses LCOV tracefiles and keeps **zero hits** strictly distinct from **no
  record** — a file absent from every tracefile is evidence that nothing was
  recorded, not that it is untested. Unrecognised records (`FN`, `FNDA`,
  `BRDA`, `LF`, `LH`, `TN`) are skipped; anything uninterpretable is an error,
  never a silent drop. Merging takes the worst coverage across inputs.
- **Conservative test selection (`autopsy-test-impact::selection`, Step 35):**
  `AffectedTests = ⋃ ReverseReachability(changed symbols) ∩ inventory`. The
  0%-false-negative gate holds by construction: (a) any changed symbol with
  `Unknown` semantics forces `RunAllTests`; (b) a suite whose mapping is not
  `Verified` **cannot be ruled out**, so it is included rather than excluded;
  (c) a change set with no positively-mapped suite forces `RunAllTests`, because
  "no test covers this" and "we failed to map this" are indistinguishable from
  inside the engine. When coverage data is missing but static imports still
  narrow, the plan falls back to `RunPackageTests` instead.
- **Advisory predictive ranking (`PredictiveRanking`, Step 36):**
  ranks selected tests by `observed_affected_sources / total_affected_sources`.
  The plan is a *separate* artifact and `select_affected_tests` takes no ranking
  input, so no probabilistic score can reach a merge gate — the types enforce
  this rather than a comment. Per the pillar plan, the sidecar is simulation and
  presentation only and must never block a merge.
- **`schemas/test-selection.schema.json`:** closed (`additionalProperties:
  false`) schema for the selection plan, plus a conformance test suite that
  cross-checks the schema against real serialised plans.
- **OpenAPI 3.2.1 contract engine (`autopsy-contracts-distributed`, Step 37):**
  parses `paths`, operations, parameters, request bodies, and responses into a
  canonical `BTreeMap`-ordered model with eager local `$ref` resolution, and
  classifies compatibility between two revisions. The verdict algebra is the
  point: **`breaking` is asserted from a positive finding and therefore does not
  depend on coverage**, while `compatible` is a claim that nothing was missed
  and is therefore asserted only over fully-modelled documents. Everything in
  between is `unknown`, which is never an approval —
  `CompatibilityVerdict::is_approval()` exists so that no consumer writes
  `verdict != Breaking` by hand and silently promotes `unknown` to a pass.
  Direction is respected: request-side narrowing is breaking, and where no
  directional evidence exists (a changed *response* schema) the change is
  reported as unclassified rather than guessed at.
- **AsyncAPI 3.1.0 contract engine (`autopsy-contracts-distributed`, Step 38):**
  parses `channels`, `operations`, and `messages` into a canonical model, and answers
  both event-contract questions — *did this revision change?* (`diff_asyncapi`) and
  *does this producer satisfy this consumer?* (`verify_flows`).

  The design point is that **an HTTP operation has one arrow and a broker channel has
  two**. Which side you are looking at is decided by the operation's `action`, not by
  the schema, so the same edit has opposite severities on each side: a type narrowed on
  an accepted schema breaks senders, a type narrowed on an emitted schema is safe; a
  field becoming required breaks a consumer and strengthens a producer. A message that
  is both sent and received is classified `Both` and must be safe in both readings at
  once.

  An optional property added to an *emitted* schema is classified `Unknown` rather than
  non-breaking. "Old consumers ignore unknown fields" is a policy some ecosystems adopt,
  not a property of the schema — Avro and Protobuf decoders are strict by default — and
  when the consumer is in the same document `verify_flows` settles the question from
  evidence instead of assuming it.

  The producer/consumer check is *containment*: everything the producer guarantees must
  be acceptable to the consumer. A producer with no `type` accepts every JSON type and
  is reported as over-delivering; a producer that merely *permits* a field does not
  satisfy a consumer that *requires* it. All of it recurses, so a violation is reported
  at its own JSON pointer.

- **`schemas/distributed-contract.schema.json`:** closed schema for the delta,
  with a conformance suite that enumerates the Rust enums exhaustively via
  `ChangeKind::ALL` (**53 kinds** after Step 38) and compares them against the
  schema through `serde`, so an added variant or a `#[serde(rename)]` cannot
  drift unnoticed. The suite also guards the reverse direction: it fails if the
  fixtures stop producing a verdict, which would otherwise leave the
  enum-coverage tests passing while checking less than they claim.

- **Pact consumer/provider verification (`autopsy-contracts-distributed::pact`,
  `::pact_xref`, Step 39):** ingests Pact JSON artifacts [R27] — v1–v4, including the
  v2 `providerState` string and `query` *string* forms and the `{value, matchers}`
  wrapper Pact 2 puts on scalar fields — into a canonical, position-identified model,
  then cross-references each consumer expectation against the provider's declared
  OpenAPI interface. A concrete path (`/alligators/3`) is matched to a provider
  template (`/alligators/{id}`) segment by segment. The search filters by method
  **before** ranking by specificity, so a literal path that does not serve the method
  cannot mask a template that does. Eleven new `ChangeKind`s; `Compatible` requires
  verified coverage and no `Unknown`-severity finding. `matchingRules` are recorded by
  JSON pointer and **never evaluated** — their presence degrades coverage, so a
  matcher-based interaction can never be reported as checked. An ambiguous provider
  match (two equally specific templates) is reported rather than resolved by choosing.

- **OpenTelemetry OTLP trace ingest & redaction (`autopsy-telemetry`, Step 40):** a new
  crate that reads `ExportTraceServiceRequest` OTLP/JSON losslessly, then sanitizes it into
  a record whose digests are taken over the **sanitized** value — never the raw one, so a
  digest can never become a fingerprint of the secret it contained. Span and trace ids are
  pseudonymised with **BLAKE3 in keyed mode** under a caller-supplied 32-byte salt with
  role domain separation; there is deliberately no `Default`, because an 8-byte span id
  under an unkeyed hash is invertible by exhaustive search and a default would hand out a
  public key. Redaction recurses through every `AnyValue`, span/event names, event and link
  attributes, and `status.message`, and is driven by two layers: exact **key-table** rules
  and value **shape** rules (JWT, `Authorization` scheme, AWS access key id, PEM material,
  IP/MAC literal, e-mail, Luhn-valid card). Hand-written linear scans only — no regex, so
  ReDoS is structurally impossible. Structural keys (`http.route`, `code.function`,
  `service.name`, …) are exempt from the fuzzy detectors, so `checkout@1.0.0` is a version
  rather than an e-mail address and the fusion surface survives. Two identities per span:
  a full observation digest (timestamps included) and an `OccupancyKey` (timestamps
  excluded) for Step 41's dead-code detection. Free text, unmodelled nodes, SDK-dropped
  data, an uncorrelatable span and an empty export all degrade coverage. No new
  dependencies; no regex engine, URL parser or telemetry SDK.

- **Runtime-to-static topology diff (`autopsy-telemetry`, Step 41):** correlates observed
  spans with the declared static surface — mapping `code.function`/`code.filepath` spans to
  `autopsy_domain::SymbolId` and `http.route` spans to declared route templates — and diffs
  the topology in both directions: **uninvoked** declared routes, and **undocumented**
  observed routes no contract declares, plus dead-code findings for declared symbols with no
  observed invocation. Route templates are compared on their placeholder-name-erased shape,
  so `/orders/{id}` and `/orders/{orderId}` are one route, and a placeholder is erased only
  as a whole segment (under-matching rather than over-matching).
- The two claims this step makes are *negative claims about absent evidence*, so each is
  gated on inputs the engine cannot derive from a payload and will not default:
  `complete_observation` (may the export be sampled?) and `dispatch` (`Static`/`Dynamic`),
  both conservative, with `required_windows` a `NonZeroU32`. A finding reaches `verified`
  coverage only when the caller has asserted both. Findings the evidence base cannot
  support are **withheld into a separate list** (`insufficient_evidence`) naming their
  subject and reason, never reported as weak findings — so a count of findings cannot
  include a code path the engine said only that it had not looked at long enough. Zero
  observation windows yields `unknown` coverage and no findings at all.
- Skipped when they cannot be justified: an **empty** declared route set makes no route
  undocumented (an empty contract and an absent contract are indistinguishable); an
  **ambiguous** symbol is `ambiguous_symbol`, not a coin-flip match, and marks neither
  candidate alive; two equally specific but structurally different templates are reported
  rather than chosen; a **method-agnostic** declaration matches but degrades coverage
  because the method was not checked.
- `schemas/topology-report.schema.json` — closed and draft 2020-12, with
  `observed_invocations` pinned to `const: 0` on both finding types so the claim is carried
  as a field rather than inferred from an absence.

- **Hidden-coupling discovery (`autopsy-mining`, Step 42):** a new crate mines a repository's
  recent history for paths that change together, and reports Support, Confidence and Lift
  per pair over a sliding window (default 183 days), reading first-parent history through
  `autopsy-temporal`'s pure-Rust object store — no `git` sub-process, no network, no write.
  The design's 6-month window is anchored at the newest commit's `committer` timestamp read
  from the repository, never at a clock reading, so the same repository always yields the
  same window; what enters the digest is the **set of commit ids** that were considered, which
  is content-addressed and timestamp-free, plus the window *width* in seconds. Every ratio is
  reported in integer parts per million beside the four counts it came from, because a digest
  over `f64` inherits float formatting and a percentage that moves between platforms is a
  determinism bug that looks like a data change.
- **The non-causality boundary is structural, not a paragraph.** `EdgeClaim` has exactly one
  variant (`TemporalCoOccurrenceOnly`) and it is a *field of every edge*, so a reader who
  slices one edge out of a report still has the warning; a `bool` could be set wrong, an enum
  with one value cannot be omitted or defaulted into existence. `EdgeKind::StatisticalCoChange`
  joins the domain's graph vocabulary beside `Calls`, making the design's distinction hold at
  the type level — a consumer filtering for `Calls` can never pick up a co-change edge by
  accident.
- Each confidence carries a 95% **Wilson score interval** in integer arithmetic (the normal
  approximation is worst exactly where this engine reports most: small and boundary
  proportions, where it renders `Confidence = 1.0` over three commits as a zero-width
  interval). The interval answers how much of the proportion could be the accident of which
  commits fell in the window, and the report and crate docs state that it says nothing about
  causality.
- **The mined population is accounted for exhaustively**, with `commits_seen ==
  commits_considered + Σ excluded[*]` asserted by a test: merge commits, root imports, mass
  commits, out-of-window commits, timestamp-less commits, undecodable trees and empty change
  sets each land in their own bucket. Dropping a commit from the denominator *raises* every
  support figure while looking like nothing happened. Exclusions that remove real edits
  (mass commits, missing timestamps, undecodable trees) degrade coverage to `partial` with the
  counts labelled a lower bound; an empty repository is `unknown`, never a report of zero
  couplings; and per-edge coverage distinguishes "the measurement covered the population" from
  "this number rests on two commits", which are different statements.
- Pure renames (a path disappears and another appears carrying the identical blob) are
  collapsed and counted, so a directory move does not manufacture an N² clique. The unpaired
  surplus is restored as real changes — one blob copied to three paths is one rename and two
  additions — and the identical-blob limit is disclosed in the report's notes.
- `schemas/cochange-report.schema.json` — closed and draft 2020-12, with `claim` and `kind`
  pinned to their single values, proportions bounded through a shared `$ref`, `lift_ppm`
  deliberately unbounded, and a test asserting no property anywhere in the document names a
  clock reading.

- **Catalog and ownership ingestion (`autopsy-catalog`, Step 43):** a new crate reads Backstage
  `catalog-info.yaml` entities (`Component`, `API`, `Resource`, `System`, `Domain`) and
  dependency manifests (npm `package.json`, pip `requirements.txt`, Go `go.mod`, Cargo
  `Cargo.toml`) from a repository tree, and derives a team ownership index plus an
  organisational blast radius per component. GitHub's dependency graph is derived from exactly
  those manifest files, so this is the same evidence GitHub uses — read offline, with no token
  and no network call. `spec.owner` gives a team per **entity**; nothing in a catalog says
  which files a component owns, so the symbol-level join is left where the symbol table is and
  the repository's own component is resolved from evidence instead (a `catalog-info.yaml`
  inside the directory, or the `github.com/project-slug` annotation).
- The scan is bounded and every bound is **recorded**, not merely applied: symlinks are skipped
  and counted (`INV-SANDBOX-002` — a path is not a location), oversize files are named
  (`INV-SANDBOX-004`), the file-count and depth bounds name what they stopped at, and pruned
  dependency directories are listed. Pruning `node_modules` or `target` is policy rather than
  truncation and is recorded as such; a truncated scan that reported itself as complete would
  be worse than no scan. Refusals are counted **by reason** — an unknown `apiVersion`, a
  document with no `kind` or no `metadata.name`, an unparsable manifest — and a malformed
  `package.json` in one directory degrades that file instead of removing every other service's
  ownership data from the report.
- **A requirement is not a version.** The field is named `requirement` and the report says
  that no lockfile was read, so transitive dependencies are absent and `^18.2.0` is not the
  installed build — the misreading has a concrete consequence (which services ship the
  vulnerable version) and it is stated where the field is, not in a footnote. Directives,
  environment markers, Go `replace` targets, Cargo feature flags and target-specific sections
  all change which packages are installed; each is recorded by name and degrades coverage
  rather than being silently dropped. A URL dependency is classified as its own source, not as
  a registry lookup.
- **Unresolved references have three outcomes, not two.** A `Group` owner is counted as a
  reference to an unmodelled kind; only a reference naming one of the five modelled kinds and
  resolving to nothing is `dangling`. Merging the two would report every entity in every
  catalog as broken and bury the real dangling reference in expected output. An unknown
  component answers `unknown`, never zero impact.
- An entity declared by more than one document is **reported with both paths** rather than
  resolved by map insertion order; `spec.definition` on an `API` is recorded as an unread
  contract (Pillar 3 reads it, not this crate); `subcomponentOf` chains inherit a system up to
  a bounded depth and a loop is reported instead of followed.
- `schemas/catalog-report.schema.json` — closed and draft 2020-12 at every level, pinning the
  five entity kinds, four ecosystems, dependency scopes and sources against their Rust
  `all()` lists through serde, requiring the whole discovery accounting, refusing absolute
  paths by pattern, and asserting that no property anywhere names a clock reading.

- **CLI subcommands for every Phase 4.2 engine (Step 44):** `autopsy temporal`, `test-select`,
  `contract-dist`, `telemetry`, `cochange` and `catalog` reach the six engines through their
  public APIs. Six rather than the four the step names: Pillar 5's mining engine and this
  step's catalog engine had no caller outside their own tests, and a pillar a user cannot run
  is not finished.
- **The exit-code rule lives in one function.** `0` pass; `2` a policy gate failed (a
  `Breaking` contract verdict, or `--fail-if-all-tests-required` when a selection could not
  narrow); `3` the analysis could not be performed; `4` the answer is not actionable —
  coverage `unsupported`/`unknown`, or `partial` under `--strict`. `unknown` exits `4`
  **without** `--strict`: an engine that says "I cannot answer this" must not hand a CI script
  a zero and let it proceed. Every one of those codes is asserted end-to-end, and two of them
  are deliberate non-errors: a repository with no commits, and a history that does not fit the
  walker's bound, are refusals to answer rather than failures of analysis.
- Handlers return `Outcome { exit_code, json, text }` and never touch the terminal, so an exit
  code and the exact bytes for it are testable without a pty. `SARIF` is refused rather than
  approximated — SARIF describes findings, and none of these reports is one. Every command is
  read-only (`FR-026`): nothing is written, no storage path is touched, and a source guard test
  asserts that no clock (`SystemTime`, `Instant::now`, `chrono`, `Utc`) appears anywhere in the
  new layer. Mining anchors its window on the newest commit's timestamp from the repository.
- **Each command's honesty is the engine's own state.** The temporal journal derives its
  coverage from the facts that failed to resolve, so `--strict` fails on precisely what the
  engine flagged rather than passing a blanket `verified`. `test-select` runs with
  `NoReachability` and a file-level change set, so the plan is `partial` by construction and the
  note says why. `contract-dist` reports `compatible` only with `verified` coverage and exits
  `2` on a breaking change without being asked. The salt for `telemetry` is required, never
  defaulted and never echoed. `cochange` tags every edge `temporal_co_occurrence_only` and says
  in prose that it is not a dependency graph. `catalog` reports a workspace `path` dependency
  as a monorepo edge.
- `.git` is read whether it is a directory, a `gitdir:` file (worktree or submodule) or a
  gitdir whose `commondir` file points at the shared object store — otherwise both history
  commands would be unavailable in exactly the setup a reviewer checking out a branch is in.
- `schemas/` and the engines are unchanged: the new layer is arguments, calls and rendering.

- **Four new MCP read-only tools (Step 45).** `autopsy_temporal` (Pillar 1 journal and `at`
  reconstruction, plus Pillar 5's mined co-change under an explicit `view` selector),
  `autopsy_test_selection` (Pillar 2), `autopsy_contract_diff` (Pillar 3, all four
  protocols) and `autopsy_distributed_blast_radius` (Pillar 6 ownership composed with
  Pillar 4 observed topology — the design's cross-repo blast radius *is* that composition).
  Every tool returns an envelope carrying the engine's exit code, because the code is part
  of the answer: `0` actionable, `2` a policy gate failed, `4` **not actionable** (coverage
  unknown/unsupported, or partial under `--strict`). The MCP layer previously understood only
  `0` and `2`, so every Step 44 refusal would have surfaced as a failed call with the refusal
  buried in stderr; `cli.ts` now treats `4` as evidence.
- **`autopsy blast-radius <component-ref>`** so the organisational traversal has one
  implementation rather than one in Rust and a second one in the viewer. Its output document
  is `{catalog, blast_radius}` on every path, including "not found": the radius is a view of
  the entities it walked, and a reader has to be able to see them. Closed schema in
  `schemas/blast-radius.schema.json`, asserted as set equality in both directions against
  what the engine emits.
- **Two Web Inspector views.** *Temporal Evolution Timeline*: a slider over the journal's
  revisions with active/added/retired facts and a badge on anything that did not fully
  resolve. Revision order is first appearance of `valid_from` and the caption says so — the
  journal carries commit ids, not timestamps, and the viewer will not put a clock on a history
  the engine deliberately does not date. A bare fact array is accepted, and then the page says
  the envelope's coverage and granularity notes are not in the document rather than inventing
  them. *Distributed Blast Radius*: a radial multigraph of the stated references, with
  unresolved references drawn dashed and named **and never as edges**, the catalog's
  "what the scan did not read" beside the graph, and the observation summary when a trace
  export was correlated. A component the catalog does not contain renders no graph and says
  the omission is a refusal to answer, not an empty answer.
- The page refuses any other document with a message naming the three artifact kinds it
  renders. It stays one static file: zero build, zero backend, no network API, no storage
  write, no clock — asserted by a source guard, not by convention.
- Wiring kept in step with the new layer (AGENTS.md §4): a `web-inspector-check` pre-commit
  hook, the same check in `Makefile`'s `check`, a CI step in `mcp-server-check` (now
  "TypeScript MCP Server & Web Inspector Checks"), and the language rules text corrected —
  the tests are `node:test`, not Vitest, and the inspector has no compilation step.
- **`schemas/temporal.schema.json`: Pillar 1's missing schema.** `docs/requirements/TRACEABILITY.md`
  recorded it as NOT DELIVERED ("journal is a library API only") because the journal had no CLI
  caller when Pillar 1 shipped; Step 44 gave it one, so the envelope's shape is now a closed
  public schema. `coverage` is documented as *derived* from the fact counts (`unknown` facts ⇒
  `unknown`, `partial` ⇒ `partial`, all resolved ⇒ `verified`), so `--strict` cannot pass over
  what the engine flagged, and `predicate` is an enum of the engine's own ten variants —
  asserted against `FactPredicate::all()`, because a schema that rejects the engine's own output
  is worse than no schema. The conformance test checks **every** fact of a real journal, not a
  sample.

- **Step 31's lineage engine now has a user: `autopsy lineage --from <rev> --to <rev>`
  (Pillar 1).** `detect_lineage` had zero callers outside its own tests — the Pillar 1 promise
  "symbol lineage across moves and renames" was true of a library and false of the product.
  The command replays two revisions from the pure-Rust object store (default: first parent of
  `HEAD` → `HEAD`), feeds `FileEntry`s to the engine, and carries the engine's decision out
  intact: a byte-identical pair with a unique counterpart is `verified`; every similarity
  match is `partial` with its candidate list or its reason; a skipped or truncated entry
  (gitlink, oversize blob, `--max-files` bound) makes the event set a **stated lower bound**
  rather than a smaller exact answer; two empty revisions are `unsupported`, never an empty
  pass. `--strict` turns every partial into exit `4`; a revision that cannot be resolved is
  exit `3`, and `^N`/abbreviated ids are refused rather than guessed. MCP gained
  `autopsy_temporal` `view: "lineage"` (same evidence envelope, exit `4` stays evidence). The
  gate tests build their repository by **writing loose git objects by hand** (zlib + SHA-1,
  test-only dev-dependencies) instead of spawning `git init`/`git mv`: the analyzer's sandbox
  invariant is zero sub-process execution, and a fixture that spawns git trains the suite to
  depend on exactly what the engine must never do. 14 new tests in
  `crates/autopsy-cli/tests/step31_lineage_cli.rs`; see
  `docs/evidence/29-symbol-lineage-protocol.md`.
- **Co-change gets its inspector view (Pillar 5, Step 45 closure).** A sixth tab renders
  `autopsy cochange --format json` with the engine's own claim carried through: the banner
  says **association, never causation** and every edge row shows the verbatim
  `temporal_co_occurrence_only` tag. The view is deliberately a list, not a graph — the
  catalog *states* references so a radius may draw them; a co-change pair is an association
  the engine refuses to call an edge, so drawing one would be the exact misreading
  `EdgeKind::StatisticalCoChange` exists to prevent. A truncated pair set is announced as a
  stated lower bound; the three "no edges" situations (below-threshold pairs counted,
  nothing to couple, coverage-unknown refusal) render three different statements; excluded
  commits are named with their reasons. 4 new tests in `apps/web-inspector/test/inspector.test.js`
  (15 total); see `docs/evidence/31-cochange-coupling-inspector.md`.

### Changed

- **Artifacts bumped to `0.1.0`** — the version the CHANGELOG's dated `[0.1.0]` release
  section already documents. `Cargo.toml` (workspace), `ANALYZER_VERSION` (snapshots,
  reports, `autopsy version`), the MCP server and the web inspector all say `0.1.0` now.
  Every remaining `0.0.1` is a **format version** (config, invariants config, baseline
  file, report schema and its digest prefix) and is pinned deliberately: bumping one would
  invalidate persisted artifacts or change every computed digest, which is a compatibility
  break, not a rename. The distinction, and the full list of what stayed and why, is in
  `docs/evidence/30-version-consistency-audit.md`.

### Fixed

- The `temporal` subcommand's default commit bound was `500`, which `TemporalJournal::build`
  refuses on any real repository (a truncated timeline is not a timeline). A 31-commit
  repository failed, and the error looked like an analysis failure rather than a bound that
  needs raising. The refusal is now exit `4` with a message naming `--max-commits`, and the
  default is `25_000`. Found by running the command, not only its tests.
- The journal's canonical projection is a JSON **array**, so the CLI's attempt to insert a
  `stats` key silently did nothing. The CLI now wraps the engine's projection under a named
  `journal` field with `stats`, `coverage` and `notes` as declared siblings, so the output is
  self-describing and the array cannot be mistaken for the whole document.
- The "compatible contract change" fixture declared OpenAPI `3.1.0`, which the engine parses
  under 3.2 semantics with a recorded gap, so coverage was `partial` and the verdict `unknown`
  — correctly. The fixture now declares `3.2.1` and asserts that a `compatible` verdict implies
  `verified` coverage, which is the rule that makes the fixture meaningful.
- `telemetry` took eight positional arguments, two of them adjacent booleans with opposite
  meanings. It now takes `TelemetryArgs`, whose default is conservative except for the salt,
  which has no default by design.
- `crate::discover` and `super::discover` are **both a module and a re-exported function**,
  which the `-D warnings` rustdoc gate refuses as an ambiguous intra-doc link. Two occurrences
  disambiguated as `mod@…` — the third consecutive step in which that gate caught a real
  defect before the commit.
- An entity reference was parsed with a guess. Backstage's grammar says the first of two
  colon-free segments is a **namespace**, so `component/checkout` means
  `component:component/checkout` and `kind:name` is not a reference at all; both readings
  produce a *well-formed* reference, so a guess here would never have looked wrong. The parser
  now implements Backstage's grammar exactly and refuses the rest.
- `spec.definition` was listed as a modelled spec key while nothing read it — the worst of both
  worlds, since no unmodelled-key record was produced either. Reads of `system`, `domain` and
  the relationship fields are now gated by entity kind, so an unread key is always reported.
- A Go `replace` directive was recorded and its target thrown away, and a loop
  \"reclassified\" replaced modules by assigning `Registry` to `Registry`. A monorepo edit and
  a fork are different supply-chain facts, and the directive now names both ends.
- A co-change window had **no upper bound**: a caller anchoring the window in the past
  silently included every commit *after* the anchor, so the window grew to the present while
  claiming to be historical. The window is now the closed interval
  `[anchor − window_seconds, anchor]`, and a commit dated after the anchor (which clock skew
  and rebases produce) is counted under `outside_window` rather than quietly included.
- The Wilson half-width used `z` where the algebra requires `z/2`, and computing `isqrt(n)`
  separately truncated the divisor to `1` for every sample below four commits — the case this
  engine reports most. Caught by a unit test asserting a *known* bound: with `x = n = 3` the
  upper bound is exactly 1, and the code returned 0.999999.
- `[R31]`-style requirement ids in doc comments were read by rustdoc as intra-doc links, and
  the document gate added in the previous commit failed on ten occurrences.
- A declared-but-**uninvoked** route left no trace in the topology report at all, so the
  report digest did not cover the contract's surface and two genuinely different contracts
  produced the same report. Found by a control assertion (a changed contract must move the
  digest) — the digest was right and a whole finding type was missing. Added
  `UninvokedRoute`, and `InsufficientEvidence` gained an `EvidenceSubject` so a withheld
  finding can name a route as well as a symbol.
- `CorrelationBasis::Rpc` named a capability no code path produced: nothing in the crate
  maps `rpc.service`/`rpc.method` to a symbol, because no adapter emits `SymbolId`s for gRPC
  handlers. The variant was removed and RPC spans are now disclosed in the notes as
  uncorrelated, so the gap is visible instead of implied to be handled.
- `[R24]`/`[R27]` and two `[CoverageState::…]` intra-doc links in Steps 39–40 output were
  broken; rustdoc warnings are not errors, so nothing had failed. Fixed, and a new
  pre-commit hook (`Rustdoc Intra-Doc Link Check (-D warnings)`) now enforces it.
- The telemetry span `pointer` — *where in the export* a span sat — reached the span and
  record digests. A collector is free to export spans in any order, so reversing two spans
  upstream changed the record digest and a **reformatting would have been reported as a
  change**. `pointer` and `notes` are now excluded from every digest, and the record digest
  is an explicitly named projection (`TraceDigestInput`) rather than a blanket
  serialization that inherits whatever happens to be in the struct.
- `IngestOptions::default()` returned a **publicly known** salt, silently reducing keyed
  BLAKE3 to the unkeyed hash the module's own documentation calls obfuscation. The `Default`
  impl was deleted; a caller must name a salt or call the explicitly-named `test_salt`.
- A Pact interaction calling an operation the provider **does not offer** produced
  no finding at all. The provider path search ranked candidate paths *without* regard
  to the request method, then the caller looked the operation up under the path it had
  chosen; when the path existed but the method did not, the caller hit a defensive
  branch that added a note and `continue`d, emitting an empty change list.
  `ChangeKind::PactMethodNotDeclared` was unreachable dead code. The search is now
  method-aware and carries the operation it found.
- A Pact 2 `{value, matchers}` wrapper on `path`/`query` degraded the *record* but not
  the *coverage*, so an interaction whose path was matched by a pattern reported
  `Verified` — as if one concrete request had been checked.
- `HttpMethod` had no `Display`, so reports named the method through `Debug` (`Get`
  rather than `GET`). Found by the compiler rather than by a test.
- Temporal replay no longer mints a duplicate fact on re-observation. Because
  `valid_from` is part of the digest while a continuing assertion keeps its
  original `valid_from`, the first implementation created a new fact at every
  subsequent commit; the fixture exposed 9 facts where 5 were correct.
- Removed an item-invariant filter in the lineage similarity pass that could
  only ever match all candidates or none.
- Version parsing read the **patch** level as the minor version, so every correct
  `3.1.0` AsyncAPI document was flagged `partial` with a note saying the minor
  version could not be read. `version.split('.').nth(1)` returns `1` for `3.1.0`;
  a shared `major_minor` helper now takes the first two components (and a test
  pins `3.10.4 -> "3.10"`).
- A channel or message reference was **unescaped before being split**, so the
  channel id `user/signedup` produced the message key
  `user/signedup/messages/M` instead of `user~1signedup/M`. Every flow on a
  channel id containing `/` — common, because some documents use the raw topic as
  the id — read as "no common message".
- Operation `channel` and `messages` references were followed to the **end of the
  `$ref` chain**. In a document that factors a message body into `components/`,
  the chain lands on `/components/messages/X` — not under `/channels/` — so the
  operation's message set came out empty and both the digest and the flow check
  reported less wrong with the document than was true. References name a
  *position*; they are now read one hop from the raw `$ref`.
- A **message-level `schemaFormat`** (the AsyncAPI 2.x layout) was ignored, so an
  Avro payload was run through the JSON Schema reader, which recorded
  `type: "record"` and reported the message `Verified`. Both layouts are now
  honoured and gated on the same readability check.
- `Document::pointer` refused array-index descent, so every legal JSON Pointer
  addressing a list element — including the extremely common
  `#/paths/.../parameters/0/schema` — failed to resolve, which silently made the
  whole document `Unknown`. Now follows RFC 6901 in full: the token's meaning is
  decided by the value it is applied to, so `0` is an index under an array and
  the key `"0"` under an object. Leading zeros and the append token `-` are
  refused. Found by the test suite, not by inspection.
- Resolved `$ref` nodes had their child pointers computed from the *reference
  site* rather than from where the node was found. A
  `{"$ref": "#/components/parameters/Q"}` parameter has no `/schema` child of its
  own, so the lookup always missed and the contract was permanently `Unknown`.
  `resolve_shallow` now returns the resolved pointer alongside the resolved
  value.
- A parameter the parser rejected (for example a path parameter absent from its
  route) was dropped with a note but without degrading coverage, so a document
  containing an unreadable parameter could still claim `Verified`. Dropped
  parameters now degrade coverage, because an unread parameter is exactly the
  kind of gap that must not read as completeness.
- Adding an *optional* request body was classified as breaking. Severity depends
  on `required`, so the kind must too; an added optional body rejects nothing
  that existing callers relied on. Caught by clippy after removing a branch that
  did nothing.
- A forced full run now emits the mandatory `plan_summary` reason. Previously
  only the narrowed path emitted it, so a consumer reporting the narrowing ratio
  got nothing on exactly the runs where "we narrowed nothing" matters most. The
  schema-conformance suite caught this.

- **The implied kind of a bare Backstage reference was always `component`.** `owner: team-x`
  became `component:default/team-x`, `system: checkout-system` became
  `component:default/checkout-system`, an entry of `providesApis` became a component — each a
  well-formed reference that resolves to nothing. Nothing failed: the entities parsed, the
  owner was present, the catalog reported `verified` coverage, and every join *across*
  entities silently did not happen, so co-ownership was empty and siblings were keyed on a
  fabricated reference. The implied kind is a property of the field, not of the string, and is
  now threaded per field (`owner` → group, `system` → system, `domain` → domain,
  `providesApis`/`consumesApis` → api, `dependsOn`/`subcomponentOf` → component, the CLI entry
  point → component). Found by building the blast-radius view on a catalog that used bare
  names — the spelling real catalogs use, and the one the Step 43 fixtures did not.
- **A modelled kind that is not a component** (system, domain, API, resource) reported an
  all-empty blast radius, which reads as "this system touches nothing". It now reports
  coverage `unsupported` with a note saying an organisational blast radius is defined for
  components.
- **`apps/mcp-server`'s tests imported a `dist/` that nothing built.** `dist/` is gitignored
  and the hook ran `typecheck && lint && test` with no build step, so the hook passed only on
  machines with a stale build from an earlier manual run and would have failed on a fresh
  checkout for reasons unrelated to the change. `pretest` now builds.
- The inspector's helpers would have been `export`ed inside the MCP server factory; caught by
  `tsc` and moved to `apps/mcp-server/src/evidence.ts`, which also made the exit-code rule
  directly testable.
- **The declared MSRV was not true.** `Cargo.toml` said `rust-version = "1.85"` (and
  `AGENTS.md`, `CONTRIBUTING.md` and the agent rules repeated it) while the engines and adapters
  use `let`-chains, which stabilised in **1.88** — so a 1.85 toolchain was told this workspace is
  buildable at its floor and would then have failed inside an engine. The number is corrected in
  all four places, with the reason written where the number lives, and a new `msrv-check` CI job
  installs the declared toolchain and runs `cargo check --workspace --all-targets`, wired into
  the pre-merge gate's needs, summary and failure condition. The floor is measured from now on
  instead of asserted. **And the first measurement failed the first correction:** installing
  1.88 showed `tree-sitter-language@0.1.8` requires rustc **1.90**, and
  `cargo +1.90.0 check --workspace --all-targets` then passed cleanly, so the declared and
  CI-verified floor is **1.90** — a number two toolchains were run against rather than one
  inferred from syntax.

### Notes
- New dependencies `flate2` (rust_backend, for cross-platform-identical
  decompression) and `sha1`; both MIT/Apache-2.0 and within the existing
  `deny.toml` allow-list.
- The git hash implementation is anchored to published git constants (empty
  tree `4b825dc6...`, `"hello world\n"` blob `3b18e512...`) and was validated
  end-to-end against a real `git gc --aggressive` repository.
- Symbol-level temporal extraction is **not yet wired to the language
  adapters**; only the built-in `TreeExtractor` (file presence) runs in
  production paths today.
- Shell-visible artifacts (this section's CLI) and `schemas/temporal.schema.json` were
  added after these notes were first written (Steps 44–45); the journal itself remains a
  library API plus the `temporal` subcommand.
- `autopsy-test-impact` performs a **lexical** scan of test files rather than an
  AST parse, because it sits below the language adapters in the dependency DAG.
  The consequence is stated rather than hidden behind a confidence score:
  constructs the scanner cannot interpret degrade the suite's coverage and force
  a fallback. A helper file that contains neither a framework import nor any
  test declaration is classified `Unknown` and therefore always runs; this is
  deliberately noisy in exchange for never silently excluding a suite we did not
  understand.
- `autopsy-contracts-distributed` adds **no new external dependency**:
  `serde_yaml` is already a production dependency of `autopsy-invariants` and
  `autopsy-repo`, so no new supply-chain edge is introduced. The pre-existing
  `serde_yaml` advisory (`RUSTSEC-2024-0449`) is unchanged by this work.
- **YAML merge keys (`<<:`) are rejected outright** rather than parsed.
  `serde_yaml` does not apply them, so a document using them parses with every
  inherited field silently absent — and a compatibility checker would then
  report those fields as *removed*, producing a confident false positive. The
  detection is line-based and can therefore refuse a document that merely
  contains `<<:` inside a block scalar. That asymmetry is deliberate: refusing
  to analyse is recoverable, a confident wrong answer is not.
- JSON Schema composition (`allOf`, `oneOf`, `anyOf`, `not`, `if`/`then`/`else`)
  is recorded but **not interpreted**. Every occurrence is named by JSON pointer
  and degrades that subtree, so a composed schema cannot yield a `compatible`
  verdict. This is deliberately pessimistic: almost every real specification
  uses composition somewhere, and pretending to model it would produce the worst
  possible answer for a compatibility gate.
- Response-body schema changes are reported with severity `unknown`, not
  `breaking` and not `non_breaking`. Request-side narrowing rules do not apply to
  responses, and this engine holds no directional evidence for them, so it says
  so instead of guessing.
- `crates/autopsy-contracts-distributed` splits into a shared **schema layer**
  (`schema.rs`, the JSON Schema reader both protocols use) and a shared **change
  vocabulary** (`change.rs`: `Severity`, `ChangeKind`, `ContractChange`,
  `ContractDelta`, `CompatibilityVerdict`). Putting them in the engines would
  mean two independent opinions about what a `$ref` cycle is and a verdict
  algebra re-derived, slightly differently, per protocol.
- `ChangeKind::default_severity()` is deliberately **not** the last word. A kind
  names a *fact* ("a property was removed"); its severity depends on the
  direction the message travels, so `ContractChange` stores an explicit severity
  and the AsyncAPI payload kinds default to `Unknown`. A fixed default there
  would be a guess, and a wrong guess is a false approval.
- `verify_flows` reasons about **containment**, not revision. It is not built on
  `payload_changes`, because "A changed into B" and "A satisfies B" are
  different relations and deriving one from the other produces confident
  nonsense in both directions.
- `diff_asyncapi` folds in **newly introduced** flow findings only. Folding in
  the absolute flow report made `Compatible` unreachable for every one-sided
  document and, worse, reported an *unchanged* gap as a regression — the surest
  way to teach users to ignore a signal. Findings a revision *resolved*, and
  operations that became uncheckable, are reported separately.
- **`bindings`, `tags`, `security`, `servers`, and `externalDocs` do not degrade
  coverage.** They carry no payload facts, so they cannot hide an
  incompatibility. `traits` and `reply` **do** degrade, because a trait may
  inject `payload` or `headers`. The rule is: degrade when a keyword could hide
  a fact, never merely because a keyword is unmodelled.
- AsyncAPI **2.x is a hard error**. Its `publish`/`subscribe` channel model means
  a lenient 3.x read yields no operations, and the diff would then report every
  operation of the other revision as removed.
- Message identity is **positional** (`channelId/messageId`, both escaped), so
  factoring a message body into `components/` is a no-op. Renaming a message id
  still reads as a removal plus an addition.
- `autopsy-contracts-distributed` is a **library API only**. No CLI subcommand,
  storage write-back to `distributed_contracts`, or MCP surface is wired.
- `autopsy-test-impact` is a **library API only**. No CLI subcommand, storage
  write-back to `test_mappings`, or MCP surface is wired yet.
- `Cargo.toml` version remains `0.0.1`.

## [Unreleased] - Forensic closure (2026-10-09)

### Fixed

- **Real two-state `diff`/`verify` (FR-007, FR-010):** `baseline` now persists
  canonical per-file bytes to the content-addressed cache; `diff --before`
  and `verify --baseline-id` rebuild the genuine before-state graph and
  contracts instead of comparing a state against itself. Unknown baseline
  ids are hard errors; api-compat without a baseline reports `Unknown`,
  never a vacuous pass.
- **Symbol identity/shape split (ADR-0009, FR-005):** `autopsy-symbols`
  implemented as the identity authority; identities no longer embed
  signature hashes, so signature edits surface as `SignatureModified` +
  breaking contract deltas instead of delete+create pairs.
- **TypeScript call graph (FR-008):** `Calls` edges (local, `this.`,
  `new`, import-resolved, typed-receiver DI/annotated params), module-scope
  variable/enum/arrow symbols, overload + interface-member symbols; fixed
  dangling hierarchy edge sources. New `impact --edge-kinds` operator
  precision control and bare file-path seeds.
- **Benchmark honesty (FR-025):** Baseline-C executes the real engine
  binary per task; prior 100%/100% precision figures withdrawn (simulated).
  Re-measured gate: recall +31.73pp PASS; cost/failure-class FAILs disclosed
  (see `docs/evidence/13-forensic-benchmark-closure.md`).
- **Web inspector:** zero hardcoded evidence; renders live artifacts only
  (verified headlessly against real `impact` + `verify` output).
- **Pillar C resource bound (3.2):** new `analysis.max_file_bytes`
  (default 10 MiB): the scanner gates on metadata size *before* reading,
  oversized files are excluded and recorded in
  `RepoSnapshot.skipped_oversize_files` with a CLI diagnostic (never
  silent); old snapshot JSON without the field still loads.
- **Honest stored-state addressing:** `diff --after <snapshot-id>` and
  `impact --snapshot-id <snapshot-id>` analyze the stored state rebuilt
  from cache (previously `--after` was a revision label and
  `--snapshot-id` was ignored); unknown impact snapshots are hard errors.
  MCP `head`/`after` descriptions corrected to snapshot IDs only.
- **Landscape parity:** one-line `scripts/install.sh` (prebuilt release →
  source-build fallback, host wiring opt-in); `scripts/setup-agent-hosts.py`
  auto-wires 7 agent hosts (Claude Code/Desktop, Cursor, Windsurf, VS Code,
  opencode, Gemini CLI) backup-first and idempotently, `--hooks` adds an
  advisory fail-open safe-edit pre-edit prompt; **Python adapter**
  (`autopsy-adapter-python`): symbols/contracts/calls incl. sibling-import,
  `self.`/`super()`/typed-receiver resolution + honest coverage; agent skill
  + settings example; `docs/COMPARISON.md` landscape record with
  vendor-reported claims labelled.
- Fixed doubled-colon rendering in normalized signatures
  (`f(): : T` → `f(): T`).
- **Precision-first impact (FR-008):** ranked top-k shortlist
  (`--top-k`, `ranked_top` + `total_impacted` in reports/schema, full set
  preserved for evidence); `--profile callers|full` presets (caller
  invocation closure, file-node transit suppression; `full` default
  unchanged); `ImpactProfile.traverse_file_nodes` engine flag.
- **Incremental baseline (FR-020, early):** `baseline --incremental
  --baseline-id` reuses content-hashed per-file artifacts for unchanged
  files (no parse), with global symbol-set soundness gate and full-analysis
  fallback; write-through artifact cache on every baseline.
- **PR forensic gate:** `.github/workflows/autopsy-pr-gate.yml` (verify
  gates merge, per-file callers impact, SARIF + JSON artifacts, PR
  comment); `scripts/forensic-case.sh` design-partner case bundles.
- **4.1 gap closure:** invalid `autopsy.toml` now fails before analysis
  (was silent fallback); `symbols`/`edges` paginated graph-browsing CLI
  commands + MCP resource templates (full dumps intentionally
  unavailable); MCP precision params (`profile`, `top-k`, `edge-kinds`),
  dead `profile` param removed; every public schema field documented;
  `cargo-deny` 0.20 policy enforced in CI; ESLint + ruff + pytest smoke
  enforced; nightly perf-tier tripwire; CycloneDX SBOM in releases.

## [0.1.0] - 2026-10-08

### Added
- **In-Session Agent Lifecycle, TypeScript MCP Server & Web Inspector (Phase 4.1):**
  - **Model Context Protocol (MCP) Server (`apps/mcp-server`):**
    - Out-of-process TypeScript server implementing official MCP SDK v2 (`@modelcontextprotocol/server` v2.3.1).
    - 5 mandatory read-only tools: `autopsy_status`, `autopsy_impact`, `autopsy_diff`, `autopsy_verify`, and `autopsy_explain`.
    - Automated Node.js integration tests asserting stdio CLI subprocess execution, tool registration, and typed parameter validation.
    - Verified architectural inversion (`ARCH_NO_CORE_TO_MCP`): core Rust crates maintain 0 dependencies on MCP or web layers.
  - **Web Inspector Evidence Board (`apps/web-inspector`):**
    - Standalone, zero-backend, read-only single-page web app for interactive forensic inspection.
    - Interactive Evidence Board visualizing hypotheses, supporting evidence cards with file/line provenance, and contradicting signal checks.
    - Transitive Blast Radius and dependency multigraph explorer showing caller hierarchy and downstream API risk.
    - Drag-and-drop local `autopsy.json` artifact loader for instant telemetry analysis.
  - **In-Session Agent Lifecycle Protocol (`tests/tests/e2e_agent_lifecycle.rs`):**
    - Integration test suite (3 tests) validating the closed-loop agent workflow: `Baseline A -> Impact Assessment -> Edit A to B -> Verify A vs B -> (On FAIL: Repair via Evidence -> Re-verify PASS)`.
    - Detection of breaking contract parameter weakening and architectural boundary violations.
  - **Baseline Debt Management & Waivers (`.autopsy/baseline.json`, CA-TECH-001 Ch.8):**
    - Schema validation via committed `schemas/baseline.schema.json` and `.autopsy/baseline.json`.
    - Verification engine support for `--baseline-id` and accepted debt fingerprints, distinguishing pre-existing legacy violations from new PR regressions.
    - Integration test `test_cli_verify_baseline_accepted_debt_waiver` verifying waiver handling and exit code conversion.
    - Added TypeScript MCP server typechecking and unit test execution into `.pre-commit-config.yaml` and `.github/workflows/ci.yml`.
- **Multi-Platform Hardening & Enterprise CI/CD (Phase 3.2):**
  - Universal hardware & architecture support: Linux (`x86_64`, `aarch64`), macOS Apple Silicon (`aarch64-apple-darwin`), macOS Intel (`x86_64-apple-darwin`), and Windows (`x86_64-pc-windows-msvc`).
  - Cross-platform path separator normalization (`/`) in `autopsy-repo` scanner, ensuring byte-identical BLAKE3 digests and snapshot IDs across Linux, macOS, and Windows.
  - Cross-platform CRLF vs LF line-ending normalization (`normalize_line_endings`) in `autopsy-repo`, guaranteeing byte-identical content hashes regardless of git checkout line endings.
  - Concurrent atomic cache write resilience in `autopsy-storage` with PID-isolated staging and collision-safe rename handling on Windows NTFS and POSIX filesystems.
  - Integration test suite `e2e_cross_platform_and_multi_arch.rs` (4 tests) asserting CRLF/LF invariance, path normalization, concurrent cache writes, and BLAKE3 boundary vectors.
  - Dedicated verification script `scripts/verify-cross-platform.sh` integrated into `Makefile` and `pre-commit` hooks.
  - Upgraded GitHub Actions CI/CD suite:
    - Pre-merge multi-platform matrix (`.github/workflows/ci.yml`) across Ubuntu, macOS (Apple Silicon), and Windows runners with concurrency cancellation (`cancel-in-progress: true`), 93 tests, and rich GITHUB_STEP_SUMMARY quality gate tables.
    - Post-merge release pipeline (`.github/workflows/post-merge.yml`) executing serially on `main` (`cancel-in-progress: false`), verifying golden baselines, compiling native release binaries across Linux (`x86_64`), macOS (`aarch64`, `x86_64`), and Windows (`x86_64`), packaging archives with licensing metadata, computing `SHA256SUMS.txt`, and uploading consolidated release bundles.
  - **Enterprise Sandboxing & Defense-in-Depth (Pillar C):**
    - Zero Code Execution (FR-026): In-memory AST parsing only, zero sub-process execution (`std::process::Command` = 0, `fork`/`exec` = 0), verified via architecture boundary gate.
    - Symlink Escape Containment (INV-SANDBOX-001): Explicit `follow_links(false)` on `WalkBuilder`, symlink entry filtering (`entry.path_is_symlink()`), and canonical root boundary checks.
    - Path Traversal Sanitization (INV-SANDBOX-003): Function `sanitize_relative_path` and `is_safe_relative_path` rejecting `..` traversal sequences and absolute paths across file scanning, `roots`, and `cache.directory`.
    - Resource Bounds & DOS Prevention (FR-008): Enforced `max_traversal_nodes` and impact budget ceilings guaranteeing linear termination on adversarial dense cyclic graphs.
    - Zero-Network Isolation (FR-028, INV-SANDBOX-002): Verified 100% offline analysis with zero networking crates and zero socket calls.
    - Integration test suite `tests/tests/e2e_enterprise_sandboxing_and_defense.rs` (5 tests) bringing total workspace test count to 100 tests.
  - Phase 3.2 Master Plan and Engineering Blueprints (`Resource/3.2/`).
- **Community & Governance Infrastructure:**
  - `SECURITY.md` establishing vulnerability disclosure protocol, response timelines, and core security invariants (Zero LLM, Read-Only, Offline-first).
  - `NOTICE` adhering to Apache-2.0 copyright and third-party attribution specifications.
  - `CODE_OF_CONDUCT.md` adopting Contributor Covenant v2.1.
  - Comprehensive `CONTRIBUTING.md` defining deterministic verification protocol, non-negotiable hard boundaries, and pull request procedures.
  - GitHub issue templates (`.github/ISSUE_TEMPLATE/`) for bug reports, feature requests, invariant violation alerts, and security guidance.
  - GitHub pull request template (`.github/PULL_REQUEST_TEMPLATE.md`) with determinism and architecture checklist.
- **Language Adapter Architecture (Phase 2):**
  - `autopsy-adapter-api`: Versioned `LanguageAdapter` trait, `AdapterCapabilities`, `ParsedFile`, `DynamicConstruct`, and reusable conformance test suite `verify_adapter_conformance`.
  - `autopsy-adapter-typescript`: Production-grade tree-sitter bootstrap (TS & TSX), TypeScript compiler bridge for relative and package module resolution, stable `SymbolId` calculation resilient to line shifts, typed multigraph edge extraction (`Contains`, `Imports`, `Inherits`, `Implements`), normalized callable and interface contract extraction, and honest coverage state emission (`Verified`, `Partial`, `Unknown`).
  - E2E integration test suite (`tests/tests/e2e_adapter_typescript_conformance.rs`) verifying conformance, real multi-file repository parsing, cross-file imports, and 100-run determinism.
- **Dependency Multigraph & Semantic Diff Engine (Phase 3):**
  - `autopsy-graph`: Petgraph-backed directed multigraph, edge provenance tracking (adapter, location, method), Tarjan SCC cycle detection, bounded BFS reachability (forward and backward), incremental file updates ($O(|V_{file}| + |E_{file}|)$), and BLAKE3 graph digest hashing.
  - `autopsy-diff`: Semantic diff engine with file deltas (add, del, modify, rename), symbol deltas (add, remove, move, rename, body, signature, visibility, kind), contract deltas with breaking change detection, edge deltas with coverage state tracking, changed frontier calculation, and strict Ambiguous Rename Rule enforcement.
  - E2E conformance test suite (`tests/tests/e2e_graph_and_diff_conformance.rs`) covering cycles, reachability, ambiguous rename safety, golden diff fixtures, and 100-run determinism.
- **Impact, Contracts, Invariants & Evidence Engine (Phase 4):**
  - `autopsy-impact`: Bounded forward/backward BFS graph traversal, traversal profiles (`edge_kinds`, `max_depth`, `budget`), deterministic shortest-path ranking, SCC cycle condensation, budget truncation tracking, and BLAKE3 impact digests.
  - `autopsy-contracts`: Normalized callable and interface contract models for TypeScript, formal backward compatibility rules per kind, breaking change detection, and cryptographic contract digests.
  - `autopsy-invariants`: YAML invariant policy loader (`.autopsy/invariants.yml`), deterministic evaluators for forbidden dependencies (`forbidden-dep`), layered architecture constraints (`layers`), cycle introduction prevention (`no-new-cycle`), and API backward compatibility (`api-compat`), strictly enforcing the `UNKNOWN != PASS` boundary.
  - `autopsy-evidence`: Immutable evidence receipts with canonical ordering of locations, paths, snapshots, and deltas before computing BLAKE3 digests, providing complete tamper verification.
  - E2E integration test suite (`tests/tests/e2e_phase4_impact_and_invariants.rs`) asserting golden fixtures, 100-run determinism, and `UNKNOWN != PASS` invariance.
- **CLI, Storage & Reporting Engine (Phase 5):**
  - `autopsy-storage`: Embedded SQLite storage with versioned auto-migrations, thread-safe connection pooling, optional WAL mode, and content-addressed `.autopsy/cache/objects/xx/yy...` with atomic write-and-rename guarantees.
  - `autopsy-report`: Reporting engine with canonical indented JSON strictly matching `schemas/autopsy-result.schema.json v0.0.1`, formatted human-readable terminal text with line-span badges, and OASIS SARIF 2.1.0 output for CI / GitHub code scanning.
  - `autopsy-cli`: Unified binary CLI providing commands `baseline`, `diff`, `impact`, `verify`, `explain`, `doctor`, and `version` with pure machine-readable `stdout`, diagnostic `stderr`, deterministic exit codes (0 pass, 2 policy failure, 3 analysis error, 4 unsupported/unknown), and offline network-off execution guarantee.
  - E2E integration test suite (`tests/tests/e2e_phase5_cli_and_storage.rs`) asserting all 7 CLI commands, exit code mapping, storage roundtrips, offline isolation, and 100-run determinism.
- **Benchmark Harness & Evaluation Suite (Phase 6):**
  - Standardized benchmark corpus with 5 TypeScript repositories (`benchmarks/corpus/`: `repo-api`, `repo-migration`, `repo-relayer`, `repo-cross-package`, `repo-dead-call`).
  - 50 deterministic benchmark tasks (`benchmarks/tasks/`: `task_001.json` - `task_050.json`) spanning 5 categories (`api_change`, `migration`, `relayer`, `cross_package`, `dead_call`) strictly conforming to `schemas/benchmark-task.schema.json`, including partitioned held-out validation tasks.
  - Three comparative baselines: Baseline A (agent + grep text search), Baseline B (agent + LSP 1-hop AST references), and Baseline C (agent + Autopsy deterministic multigraph reachability).
  - Benchmark evaluation engine (`benchmarks/engine/`) calculating recall, precision, F1, tool calls, tokens, latency, and structural failure class discovery.
  - Pre-registered gate verification (01 Ch.12): **PASSED** with +31.73pp recall lift (threshold $\ge +10\text{pp}$), 56.90% cost reduction (threshold $\ge 40\%$), and 16 newly discovered structural failure classes.
  - Automated reproduction report generation (`benchmarks/results/reproduction_report.json` and `reproduction_report.md`).
  - E2E integration test suite (`tests/tests/e2e_phase6_benchmarks.rs`) asserting corpus integrity, task schema conformance, and gate passage.
- **Engine Core Verification Architecture:**
  - 14 Rust crates implementing foundation pipeline (`autopsy-repo`, `autopsy-domain`, `autopsy-symbols`, `autopsy-graph`, `autopsy-diff`, `autopsy-impact`, `autopsy-invariants`, `autopsy-evidence`, `autopsy-storage`, `autopsy-contracts`, `autopsy-report`, `autopsy-cli`).
  - Cryptographic BLAKE3 100-run snapshot determinism invariant test suite.
  - Strict architectural boundary verification script enforcing Zero LLM in correctness crates.

## [0.0.1] - 2026-09-29
- Repository architecture and public schema starters.
- Foundation schema contracts and ADR records.
- **Measured competitor bake-off:** seven rivals cloned, built and run on
  identical fixtures (`docs/COMPARISON.md`). Found and fixed a silent Python
  recall gap — inline construction chains (`Calculator().run(...)`) now
  resolve as `call_expression:chained-constructor`, with regression test
  (161 → 162 tests). Only SynEvid blocks a breaking change by default
  (exit 2); ripwire and blastcode detect but exit 0. Logged the missing
  test-coverage axis (`radius_untested`) as the top parity gap.
