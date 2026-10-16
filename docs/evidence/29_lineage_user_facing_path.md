# Evidence 29 — Step 31's lineage engine gets a user

**Date:** 2026-10-16
**Scope:** the first of the six items carried as outstanding after Phase 4.2 reached Step 45.
`autopsy-temporal::lineage` (Step 31) was implemented, digested and unit-tested — and had
**zero callers** outside its own tests (`grep` for `detect_lineage` outside
`crates/autopsy-temporal/src/lineage.rs` returned nothing in `autopsy-cli`,
`autopsy-storage` or the MCP server). The Pillar 1 promise "symbol lineage across moves and
renames" was therefore true of a library and false of the product. This closes that gap along
the same route Step 44 used for the other engines: one CLI subcommand, one MCP view, exit
codes in the one place that decides them.

---

## 1. What the user can now do

```console
$ autopsy lineage --from HEAD~10 --to HEAD          # text or --format json
lineage: 65 events (0 renamed, 0 moved, 22 modified, 43 added, 0 removed; 0 identity-preserving, 0 partial)
coverage: partial
note: similarity is a token signature, not a parse: … every similarity-based match is `partial` …
note: 4 entries were skipped (0 gitlink/submodule, 4 over 1048576-byte bound, both revisions combined): …
```

`--from` defaults to the **first parent of `--to`**, and `--to` defaults to `HEAD`, so the
bare command answers "what identity transitions did the last commit make". Revision
expressions accepted: a fully-qualified ref (`refs/heads/main`), `HEAD`, a bare branch or tag
name, a full 40-character commit id, and any of those followed by `~N` (first-parent walk).
Abbreviated ids and `^N` selectors are **refused with an error**, not guessed: the object
store resolves refs and full ids, and a resolver that silently rounds a prefix to some
packed object is the kind of helper that is right until the one time it matters.

A root commit as `--to` compares against the empty tree; every file is then an addition, and
the notes say so rather than leaving the reader to wonder where the removals went.

## 2. Coverage is the whole point

The engine already decides what identity claims are allowed; the command's job is to carry
that decision out intact:

| Situation | Coverage | Exit (`--strict`) |
|---|---|---|
| enumeration complete, every event confident | `verified` | 0 (0) |
| any event not confident (similarity match, ambiguous digest) | `partial` | 0 (4) |
| any entry skipped — gitlink/submodule, or over `--max-file-bytes` | `partial` | 0 (4) |
| file list hit `--max-files` (sorted order) | `partial` | 0 (4) |
| both revisions contain no files | `unsupported` | 4 (4) |
| object store unreadable, revision unresolvable, bad bounds | — | 3 |

Two of those rows are the load-bearing ones. A **skipped blob is a possible rename**, so the
event set is a stated *lower bound*, not a coincidentally smaller exact answer — the note says
which bound was hit and with what count. And `unsupported` for two empty revisions exists
because "no files compared to no files" is not an answer a CI script may branch on; `unknown`
is never emitted here because every path that cannot read the objects is an analysis *error*
(3), not a silent absence.

The engine's own guarantee is repeated verbatim in the machine-readable `notes` every run:
only a byte-identical pair with a unique counterpart reaches `verified`; every
similarity-based match is `partial` and carries its candidate list (or its reason, when the
best candidate legitimately beat the runner-up). `--strict` extends the refusal to all of it.

## 3. MCP

`autopsy_temporal` gained `view: "lineage"` (with `from`, `to`, `max_files`), so the tool
count stays at 9 and Pillar 1 owns one tool instead of two. The response is the same
`evidenceEnvelope`: exit `4` remains **evidence** — `actionable: false` — not a transport
failure, which is asserted through the MCP client path (`success: true`, `exitCode: 4`).

## 4. The fixture writes git objects by hand, on purpose

The obvious integration fixture is `git init` + `git mv` + `git commit`. It is the wrong one:
the analyzer's sandbox invariant is **zero sub-process execution**, and a test that spawns
git trains the suite to depend on exactly what the engine must never do. So the fixture
builds loose objects itself — `zlib("<kind> <len>\0<payload>")` at
`objects/<first 2 hex>/<rest>`, named by the SHA-1 of the *uncompressed* form — using two
test-only dev-dependencies (`flate2`, `sha1`). It is hermetic (no git binary, no global
config, no clock: fixed author/committer timestamps), and it exercises the same
`GitObjectStore` → `flatten_tree` → `FileEntry` path production uses. Fourteen gate tests in
`crates/autopsy-cli/tests/step31_lineage_cli.rs` run against it and the real workspace.

## 5. Verification telemetry (this change)

| Gate | Result |
|---|---|
| `cargo test -p autopsy-cli --test step31_lineage_cli` | **14 passed / 0 failed** |
| `cargo test --workspace --all-targets` | **829 passed / 0 failed** (baseline 815 + 14) |
| `cargo test --workspace --doc` | 0 failed |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cd apps/mcp-server && npm run typecheck && npm run lint && npm test` | **16 passed / 0 failed** (baseline 14 + 2) |

Determinism: the command reads no clock and no unordered collection; a test asserts two runs
produce byte-identical JSON *and* text, and the workspace golden 100-run harness still holds
(§6 of Evidence 28's method: identical inputs, identical digests).

## 6. What is still honest to call incomplete

* The engine's similarity proxy is a **token signature, not a parse** — the command repeats
  that sentence in every output. This is the engine's design (and why no similarity match is
  ever `verified`), not a gap to be closed by wiring.
* No Web Inspector view for lineage. The six outstanding items did not ask for one; the
  inspector has temporal and blast-radius views, and a third view without a rendering
  requirement would be decoration. Named here so its absence is a decision, not an oversight.
* User-facing `^N` and abbreviated-id selectors are refused (§1). `~N` covers the same
  first-parent walk; `^N` is a merge-parent selector whose semantics nobody asked for yet.
