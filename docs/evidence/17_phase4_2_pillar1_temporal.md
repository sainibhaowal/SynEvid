# Evidence 17 — Phase 4.2 Pillar 1: Temporal Semantic Graph (Steps 28–32)

Date: 2026-10-16
Scope: `04_4.2_IMPLEMENTATION_SEQUENCE.md` Steps 28–32
Status: **Steps 28–32 VERIFIED. Steps 33–45 NOT STARTED.**

---

## 1. Executive Summary

Phase 4.2's v0.1.x promotion gate requires a temporal semantic graph. Five
sequential steps are complete and independently verified. Steps 33–45 (Pillars
2–6, CLI subcommands, MCP and Web Inspector evolution) have **not been
implemented**.

The single most consequential architectural finding is recorded in §3: Pillar 1
cannot shell out to `git log`, so a read-only pure-Rust git object reader was a
prerequisite, not an optimisation.

---

## 2. Step-by-Step Deliverables

### Step 28 — Global Entity ID parser (`autopsy-domain::global_id`)

RFC-GLOBAL-001 implemented as a total, canonicalizing parser.

* Grammar enforced exactly as specified in `03_4.2_STANDARDS_SCHEMAS_VERSIONING.md` §2.1.
* **Provider** lowercased; **org/repo** case-preserved (owner names are case-sensitive on most forges).
* **Path** POSIX-normalized: `./` stripped, `//` collapsed, trailing `/` stripped, `..` rejected.
* Ambiguity between the optional `path` and `symbol-path` (both allow `:`) is resolved by a fixed documented rule: first `#` separates the fragment; the last `:` whose right-hand side is a valid symbol-path and contains no `/` separates path from symbol.
* Round-trip is total over the accepted grammar: `parse(display(id)) == id`.

**Strictness decision (deliberate).** Backslashes are **rejected**, not rewritten. Accepting both `src\a.ts` and `src/a.ts` as the same identity would mean two spellings produce one ID, which breaks identity uniqueness across Windows/Linux/macOS. Callers holding Windows input must normalize before constructing an ID. Covered by `test_rejects_backslash_path`.

### Step 29 — SQLite migration v2 (`autopsy-storage`)

* Tables created: `temporal_facts`, `test_mappings`, `distributed_contracts`, `co_change_matrix`.
* 8 indexes created for the query patterns each pillar needs.
* `created_at` / `window_start` / `window_end` are bookkeeping columns and are **never** read into canonical structures, so wall-clock time cannot reach a BLAKE3 digest.
* Verified against a **hand-built v1-only database** to prove in-place upgrade, and verified idempotent across three reopens (exactly two `schema_migrations` rows).
* `schema_version()` added for the CLI `doctor` path.

### Step 30 — Temporal facts & journal (`autopsy-temporal::fact`, `::timeline`)

* `fact_id = BLAKE3(subject ‖ predicate ‖ object ‖ valid_from)`, NUL-separated. The separator is load-bearing: without it `("ab","c")` and `("a","bc")` collide. Covered by `test_fact_id_encoding_is_unambiguous`.
* `valid_to` is deliberately **excluded** from the digest, so closing an interval never changes a fact's identity.
* Commit ordering is a **topological order with object-id tie-breaking** — not chronological. Commit timestamps are attacker-controlled and machine-dependent, so they must not influence output.
* 100-run byte-identical journal JSON verified.

**Bug found and fixed during implementation.** The first replay minted a *new* fact on every re-observation, because `valid_from` is part of the digest while a continuing assertion keeps its original `valid_from`. The fixture exposed this as 9 facts where 5 were correct. The replay now short-circuits on an already-open identity key and records a continuation. This is the class of defect that only a multi-commit fixture can catch; a single-revision test would have passed.

### Step 31 — Symbol lineage (`autopsy-temporal::lineage`)

Confidence ladder, engineered for the *"zero false-positive renames"* gate:

| Condition | Outcome |
|---|---|
| Content digest equal, exactly one source and one target | `Renamed`, `Verified` |
| Content digest equal, multiple counterparts | `Partial` + full candidate set |
| Token similarity ≥ threshold with a clear margin | `Partial` + reason |
| Otherwise | `Added` / `Removed` |

`Renamed`/`Moved` with `Verified` therefore *implies* byte-identical content — asserted directly in `test_step31_same_digest_rename_is_the_only_verified_identity_claim` across every adjacent revision pair. Similarity alone can never reach `Verified`, because token overlap is a syntactic proxy, not a parse.

An item-invariant filter was also removed from the similarity pass: `filter(|a| !after_map.contains_key(b.path))` could only ever match all items or none. Replaced by an explicit `continue` when the path still exists on both sides.

### Step 32 — Point-in-time reconstruction (`autopsy-temporal::timeline`)

The reconstruction invariant is guaranteed **by construction**, not by testing alone: at each replayed revision the open-fact set is replaced by exactly the set that revision observes. Therefore active-at-`C_k` ≡ extracted-from-`C_k`.

Verified by `test_step32_reconstruction_matches_fresh_parse_at_every_revision`, which compares a `GraphSnapshot` digest from the journal against an independently computed fresh parse at **all four** fixture revisions.

---

## 3. Architectural Constraint: Why a Git Reader Exists

`INV-SANDBOX-001..004` require **zero sub-process execution** and `FR-026`
requires the engine to be read-only. Shelling out to `git log` is therefore
architecturally forbidden, not merely discouraged.

Consequence: `crates/autopsy-temporal/src/git.rs` implements read-only object
access directly.

* Ref resolution: `HEAD`, loose refs, `packed-refs`, symbolic-ref chains (depth-capped); loose refs correctly shadow `packed-refs`.
* Loose objects: zlib inflate with a **256 MiB decompressed cap** (zlib-bomb guard) and a declared-length cross-check that reports a mismatch rather than trusting the header.
* Packfiles: `.idx` **v2** with fanout-bounded binary search, plus `ofs-delta` **and** `ref-delta` resolution, chain depth capped at 64.
* Auto-dereferences annotated tags to commits.
* Uses `flate2` with `rust_backend` (miniz_oxide) so decompression bytes are identical on every platform — a C zlib would not guarantee that.

**Explicitly unsupported, never silently misread** (`FR-014`, `FR-030`):
`.idx` v1, SHA-256 object format, object alternates, linked worktrees
(`commondir`), pack index extensions, multi-pack indexes. Each returns
`GitError::Unsupported`.

`deny.toml` impact: `flate2` and `sha1` are both MIT/Apache-2.0, satisfying the
existing allow-list. `cargo deny check` reports `licenses ok, bans ok, sources
ok`; the `advisories FAILED` result is the pre-existing `serde_yaml 0.9.34`
→ `unsafe-libyaml` RUSTSEC-2024-0449 advisory, unrelated to this change.

---

## 4. Test Telemetry

```
cargo fmt --all -- --check              PASS
cargo clippy --workspace --all-targets -- -D warnings   PASS (clean)
cargo test --workspace --all-targets    273 passed / 0 failed
cargo test --workspace --doc            17 doc-test targets, 0 failed
./scripts/verify-determinism.sh         PASS — 100/100 byte-identical digests
./scripts/verify-arch-boundaries.sh     PASS — core clean, offline, deterministic
```

New coverage added by this change:

| Area | Tests |
|---|---|
| RFC-GLOBAL-001 parser | 21 |
| Storage migration v2 | 7 |
| Git object store (pack/loose/delta/refs/negatives) | 32 |
| Temporal fact model | 9 |
| Lineage | 12 |
| Timeline + reconstruction | 14 |

**Hash correctness is anchored externally.** `test_empty_tree_matches_published_git_constant` asserts `4b825dc642cb6eb9a060e54bf8d69288fbee4904` (git's empty tree) and `test_known_blob_id` asserts `3b18e512dba79e4c8300dd08aeb37f8e728b8dad` (`"hello world\n"`). These are published git constants, so the implementation is checked against git rather than against itself.

Independent end-to-end validation against a real repository (created with
`git gc --aggressive` so every object was packed and deltified) confirmed HEAD
resolution, 15 packed objects, correct tree flattening, byte-exact blob content
through delta chains, and the full first-parent history — cross-checked with
`git cat-file -t/-s`.

---

## 5. Honest Scope Statement

Implemented: **Steps 28, 29, 30, 31, 32**.

Not implemented, and not partially implemented: Steps 33–45 — `autopsy-test-impact`
(Pillar 2), `autopsy-contracts-distributed` (Pillar 3), `autopsy-telemetry`
(Pillar 4), `autopsy-mining` (Pillar 5), `autopsy-catalog` (Pillar 6),
CLI subcommands (`temporal`, `test-select`, `contract-dist`, `telemetry`),
and the MCP/Web Inspector evolution.

Also not yet done: the v0.1.x JSON schema (`schemas/temporal.schema.json`) and
CLI surfacing of the temporal journal. The journal is currently available as a
library API only.

Symbol-level temporal facts are contributed through the `SourceExtractor`
trait. `TreeExtractor` (the built-in) records file-presence facts; the
TypeScript and Python adapters are not yet wired in, so **symbol-level temporal
history is not yet extracted in production paths**. This is stated plainly
rather than implied.

The `Cargo.toml` version remains `0.0.1`; only the Cargo manifest bump is
outstanding for the `v0.1.x` line.
