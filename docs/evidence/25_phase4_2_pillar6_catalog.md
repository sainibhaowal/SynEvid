# Evidence 25 — Phase 4.2 Pillar 6 (part 1): Catalog & Ownership Ingestion (Step 43)

**Date:** 2026-10-16
**Scope:** Step 43 of `Resource/4.2/04_4.2_IMPLEMENTATION_SEQUENCE.md`.
**Crate:** `crates/autopsy-catalog` (new)
**Predecessor:** `docs/evidence/24_phase4_2_pillar5_mining.md` (Step 42)

---

## 1. What the step had to prove

> **Step 43** — *Catalog & Ownership* — `autopsy-catalog` — Ingests Backstage `catalog-info.yaml`
> `R28` and GitHub dependency manifests `R29``R37`; attaches team ownership metadata.

and the boundary the master plan puts around it:

> **Do Not Become a Generic Software Catalog:** Do not replicate Backstage or ServiceNow;
> ingest their IDs to enrich semantic evidence.

Two failure modes, opposite in direction, and both structural here:

* **claiming an ownership or dependency fact no document states** — inventing a team for a
  component, or reading `^1.2.3` as the installed version; and
* **silently losing a document** — a refused entity, a pruned directory, an oversize file, a
  second declaration of the same reference that a map quietly overwrote.

---

## 2. The join to code is where the design puts it, and no further

`spec.owner` gives a team per **entity**. Nothing in a Backstage catalog says which *files* a
component owns, and nothing in this crate will infer it — a wrong ownership claim is worse
than an absent one, because it routes an incident to the wrong team.

So the join is split, and both halves are named:

| Half | Resolution | Evidence |
|---|---|---|
| Which entity is this repository? | `components_declared_at(dir)` — a `catalog-info.yaml` **inside** the directory; `components_for_github_project(slug)` — the `github.com/project-slug` annotation `R37` | a document, not a directory name |
| What does that entity touch organisationally? | `blast_radius(component)` — co-owned components, siblings in the system, domain, provided/consumed APIs, dependents | the catalog's own relationships |

The symbol-level join stays where the symbol table is, exactly as Step 41 left
`DeclaredSurface` to its caller.

`OrgBlastRadius` is a *value*, not a stored graph: the master plan's prohibition is honoured
by producing something other engines join against, not a second Backstage.

### 2.1 Siblings are components, not entities

The first version put every entity in the system into `siblings` — so an `API` and a
`Domain` appeared in a list of services an incident would spread to. Caught by a fixture
whose expectation was right. Siblings are now filtered to `Component`.

### 2.2 A `subcomponentOf` chain, bounded

A subcomponent inherits the system of the component it is part of, followed up to 32 levels
with a visited set. A loop in a hand-maintained catalog is a real possibility, and following
it forever would be a hang rather than an answer: the chain is abandoned and **reported**.

---

## 3. Unresolved references have three outcomes, not two

| Outcome | Meaning | Effect |
|---|---|---|
| resolved | the entity is in the catalog | — |
| kind not modelled | the reference names a `Group`, `User`, … | counted by kind |
| dangling | the reference names one of the five **and no such entity exists** | degrades coverage |

The middle row is the whole point. Every `spec.owner` in a normal catalog points at a
`Group`, and `Group` is outside the five kinds the design names. A naive
"referenced but absent" check would report **every entity in every catalog** as broken,
burying the real dangling reference in expected output. A test asserts the two are counted
in different fields, and the schema says which is which.

An unknown component is **not** an error: *"I have never heard of that component"* is an
answer, and it is the more useful one, because a caller who expected the component to be
present has learned that their catalog scan is incomplete rather than that their code is
uncoupled. Coverage is `Unknown`, never a zero-impact claim.

---

## 4. The population is accounted for again

Same identity as Step 42, in a different currency:

```text
  candidate files == files read + Σ (skipped_oversize + skipped_beyond_limit + skipped_symlinks)
```

A test asserts it. A file dropped without a record makes a report look complete while a
manifest is missing from it, which is the failure mode that turns a supply-chain answer into
a wrong one.

| Bound | Recorded as | Degrades coverage |
|---|---|---|
| Symlink (`INV-SANDBOX-002`) | `skipped_symlinks`, counted; never followed | yes |
| File size (`INV-SANDBOX-004`) | `skipped_oversize_files`, named by path | yes |
| File count | `skipped_beyond_file_limit`, named | yes |
| Depth (`INV-SANDBOX-003`) | `depth_truncated`, named | yes |
| Pruned directory | `pruned_directories`, with a count | **no** — it is policy |

Pruning `node_modules`, `target`, `vendor` and the rest is a **policy**, not a truncation:
those never hold a hand-written catalog. It is recorded in the report (and therefore in the
digest) so a reader can see it happened, and `DiscoveryReport::is_complete` deliberately does
not treat it as incompleteness. A test asserts that distinction rather than leaving the field
looking like a bug.

A symlink is the one entry that is *skipped before the name is even considered*: a path is not
a location, and following one is how a manifest outside the workspace is read through a
workspace-relative path.

---

## 5. Refusals are counted, never handled by silence

`refusals` is a map from **reason** to count, not a list of paths: a tree with forty broken
documents is one fact with a number, and the reason names it. Reasons include an unknown
`apiVersion`, a document with no `kind`, a document with no `metadata.name`, a non-mapping
document, and an unparsable manifest.

Two refusals are deliberate:

* **An unknown `apiVersion` is refused.** An entity claiming a schema version this crate has
  never read may use any of its fields for anything, and the safer reading is not available.
* **`spec.definition` on an `API` is not read.** That is the contract payload, and parsing
  OpenAPI belongs to Pillar 3's engines; reading it twice in two crates is how two answers
  come to disagree. Its presence is recorded and degrades coverage.

And one non-refusal: a malformed `package.json` in a monorepo must not remove the ownership
data of every other service, so it degrades *that file* and the scan continues. A test
asserts the other manifests and every entity survive.

---

## 6. Duplicate entity references

An entity is keyed by its reference, so a second document with the same reference either
repeats the first or contradicts it, and the two are indistinguishable without reading both.
The first cut let a `BTreeMap` decide, which means the answer depended on which document was
inserted last.

Now: entities sort by `(reference, source_path)` — a function of **content** — the first is
kept, and every collision is listed in `duplicate_entities` with **both paths**, degrading
coverage. A test declares `component:default/checkout-api` twice with different owners and
asserts both are named and exactly one entity survives.

---

## 7. Dependency manifests: a requirement is not a version

`R29``R37` is GitHub's dependency graph, which is derived from exactly these files, so
reading them is reading the same evidence GitHub uses — without a network call, a token, or
an API dependency (`FR-026`).

| Ecosystem | File | Modelled |
|---|---|---|
| npm | `package.json` | `dependencies`, `devDependencies`, `peerDependencies`, `optionalDependencies`, `bundledDependencies` |
| pip | `requirements.txt` | PEP 508 names, extras, specifiers, environment markers |
| Go | `go.mod` | `require` (block and single-line), `// indirect` |
| Cargo | `Cargo.toml` | `dependencies`, `dev-dependencies`, `build-dependencies`, `workspace.dependencies` |

The field is named `requirement` and the report says why:

> requirements are reported as written. No lockfile was read, so transitive dependencies are
> not in this report and a requirement is not the version that is installed.

The misreading is concrete: someone takes `^18.2.0` for the installed version, and the next
question — *which services ship the vulnerable build* — is answered wrongly. A schema test
asserts the field's description names the lockfile gap and that the prose says the same.

Things that were **not** done, and are recorded rather than guessed at:

* **Directives degrade.** `-r other.txt` points at requirements this crate never opens;
  `--index-url` changes the resolver's inputs; a Go `replace` swaps a module for a local
  directory or a fork; a Cargo `features` key decides whether an optional crate is compiled
  in at all; `[target.'cfg(unix)'.dependencies]` exists on some platforms only. All are
  recorded **by name** and all degrade coverage.
* **Markers are recorded, not evaluated.** A PEP 508 marker answers for the machine that ran
  the scan; the question this data is used for is about every machine.
* **A URL is its own source.** `DependencySource::Url` was split out from `Registry`: a
  tarball or a PEP 508 direct reference is not a registry lookup, and the versions a registry
  would have resolved do not exist for it. The `@` in `name @ https://…` is a separator and
  is not kept as part of the location.
* **`path` and `workspace` are different from a registry edge.** A path dependency is a
  monorepo edge; `workspace.dependencies` is declared for **members** to inherit and is not
  consumed by the manifest it appears in.
* **The allowlist is a list of file names.** `pyproject.toml`, `pom.xml`, `Gemfile`,
  `composer.json`, `Pipfile` and lockfiles are not opened, and the report carries
  `manifest_names_looked_for` so a reader can see the scope instead of inferring it from
  silence.

---

## 8. Determinism

* Paths are POSIX-relative, and `safe_relative` **refuses** an absolute path or any `..`
  component (`INV-CROSS-001`, path traversal).
* The scan **root** is not in the report at all. An absolute local path would make one
  repository hash differently on two machines, and it is a fact the caller already has. A
  test scans two byte-identical trees at two different paths and asserts one digest, and
  asserts the absolute path does not appear in the serialized report.
* 100 scans of one tree produce one digest and one structurally identical report.
* Prose is excluded from the digest (`notes`, `digest`), and both controls are present: a
  changed `description` moves the digest, and appending a note does not.
* No clock is read anywhere, and a schema test asserts **no property anywhere in the schema
  document names a clock reading** — including `duration`.
* Every collection is a `BTreeMap`, `BTreeSet` or a sorted `Vec`; the walk sorts directory
  entries, so the traversal order is a function of the tree.

---

## 9. Verification telemetry

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS (0 warnings) |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | PASS |
| `cargo test --workspace --all-targets` | **771 passed / 0 failed** (was 710, +61) |
| `cargo test --workspace --doc` | PASS |
| `./scripts/verify-determinism.sh` | PASS — 100/100 byte-identical |
| `./scripts/verify-arch-boundaries.sh` | PASS — new crate covered by the `crates/*` glob |
| `pre-commit run --all-files` | PASS (all 11 hooks) |

| File | Tests |
|---|---|
| `src/discover.rs` (unit) | 3 |
| `src/backstage.rs` (unit) | 10 |
| `src/manifests.rs` (unit) | 7 |
| `src/ownership.rs` (unit) | 9 |
| `tests/step43_catalog.rs` | 18 |
| `tests/schema_conformance.rs` | 14 |

Every fixture is a real directory tree on disk: the bounds this step must honour — symlinks,
file size, depth, pruned directories — do not exist off the filesystem.

---

## 10. Defects found and fixed

1. **Entity references were parsed as Backstage parses them, and my test was wrong.** I had
   asserted that `component/checkout` means `component:default/checkout`. Backstage's own
   grammar says the first of two colon-free segments is a **namespace**, so it means
   `component:component/checkout`, and `kind:name` is not a reference at all. The parser now
   implements Backstage's grammar exactly — one segment is a name, two are
   `namespace/name` or `kind:namespace/name`, three is a refusal — and the test documents
   the asymmetry rather than smoothing it over. Both readings produce a *well-formed*
   reference, so a guess here would never look wrong.
2. **`spec.definition` was listed as a modelled key while nothing read it.** A key claimed as
   modelled and never read is the worst of both worlds: no unmodelled-key record and no data.
   It is now absent from the modelled list, so the generic loop reports it.
3. **`domain` was a modelled key on `Component`, and `system` was read off `Domain`.** Both
   populated fields the formats do not define and simultaneously reported the key as unread.
   Reads are now gated by kind: `system` for `Component`/`API`/`Resource`, `domain` for
   `System`, relationships for `Component`/`Resource`.
4. **A Go `replace` was recorded and its target thrown away,** and a loop "reclassified" the
   replaced modules by assigning `Registry` to `Registry`. The directive now records *which
   module* is swapped and *what it is swapped for* (a local directory or another module),
   because a monorepo edit and a fork are different supply-chain facts.
5. **A pip direct reference kept the `@`** (`@ https://…`), producing a "requirement" that
   is not one. It is stripped, and the URL is classified as `DependencySource::Url` rather
   than as a registry version.
6. **`API` and `Resource` entities appeared in a list of sibling *services*.** Filtered.
7. **A duplicate entity reference was resolved by insertion order.** Now recorded with both
   paths and degrading, decisive by content.
8. **`[`crate::discover`]` is both a module and a function**, which the `-D warnings`
   rustdoc gate (added in Step 41) refuses as an ambiguous link. Two occurrences
   disambiguated as `mod@…`. The gate caught it before the commit, which is the third step
   in a row where it earned its place.

---

## 11. Honest scope statement

1. **No symbol-level ownership.** §2. Pygments aside, no document states which files a
   component owns, and this crate will not infer it.
2. **No lockfiles, therefore no transitive dependencies.** The largest gap between this data
   and GitHub's dependency graph, stated in the report itself.
3. **Four ecosystems.** npm, pip, Go and Cargo. Everything else is out of scope by
   allowlist, and the allowlist is reported.
4. **No version resolution, no ranges, no advisories.** A requirement is a string.
5. **`spec.profile`, `spec.members`, team hierarchies and `Location` entities are not read.**
   `Group` is counted, and the note says what that costs: ownership rolls up only as far as
   the reference.
6. **Duplicate YAML keys are invisible.** YAML permits a key to appear twice and `serde_yaml`
   resolves it before a value is reachable; for a field like `spec.owner` two values mean two
   teams. Only the input size bound mitigates it, and this is stated rather than assumed.
7. **The catalog is not validated against Backstage's own schemas.** Structural rules (a
   `System` whose `domain` is a `Component`, say) surface as dangling references, not as
   schema violations.
8. **No storage write-back,** and no CLI subcommand yet — Step 44 wires it. Until then this
   engine is reachable from tests and library callers.

---

## 12. Boundary compliance

* **Zero LLM in correctness crates** — YAML/JSON/TOML parsing and graph traversal.
* **Read-only analyzer (`FR-026`)** — nothing is written; no network; no sub-process.
* **Determinism (`NFR-001`, `FR-028`)** — §8.
* **Honest coverage (`FR-014`, `FR-030`)** — §4, §5, §11; an empty root is `Unknown`.
* **Sandboxing (`INV-SANDBOX-001..004`)** — §4.
* **Architectural inversion (`ARCH_NO_CORE_TO_MCP`)** — `autopsy-catalog` depends on
  `autopsy-domain` plus parsers (`serde`, `serde_json`, `serde_yaml`, `toml`, `blake3`,
  `thiserror`). No adapter, no CLI, no MCP, and no contract engine.
* **Supply chain** — no new production dependency: `serde_yaml` and `toml` are already
  production dependencies of `autopsy-repo` and `autopsy-invariants`, so this adds no
  supply-chain edge to the workspace.

---

## 13. Traceability

| Requirement | Where |
|---|---|
| Step 43 gate (catalog ingestion `R28`; manifests `R29``R37`; team ownership) | `tests/step43_catalog.rs` — `gate_a_repository_yields_entities_ownership_and_manifests_in_one_report`, `gate_the_repositorys_own_component_is_resolved_from_evidence_not_from_a_name`, `gate_unknown_component_is_unknown_rather_than_zero_impact` |
| §6.1 entity kinds and `spec.owner` | `backstage::EntityKind`, `CatalogEntity::owner`, §2 |
| §6.1 map to team ownership for organisational blast radius | `ownership::OwnershipIndex::blast_radius`, `OrgBlastRadius`, §2 |
| Master-plan boundary: not a generic catalog | §2, §11, `docs/evidence/25` |
| `R37` GitHub linkage | `github_project` (`metadata.annotations["github.com/project-slug"]`), `components_for_github_project` |
| `FR-026` read-only | §12; no `fs::write`, no sub-process |
| `FR-014`, `FR-030` honest coverage | §4, §5, §11 |
| `NFR-001` / `FR-028` determinism | §8 |
| `INV-SANDBOX-001..004` | `discover`, §4 |
| `INV-CROSS-001` (POSIX paths, CRLF/LF) | `safe_relative`; POSIX-relative paths throughout; no line-ending handling needed |
| Schema contract | `schemas/catalog-report.schema.json`, `tests/schema_conformance.rs` |
