# Evidence 30 — One version number, and the ones that are deliberately not it

**Date:** 2026-10-16
**Scope:** item 5 of the six outstanding closure items: the CHANGELOG carried a dated
`[0.1.0]` release while every artifact said `0.0.1`. A version that the release notes
document but the binaries deny is a false claim about the artifact — the same class of
falsehood as the MSRV was (Evidence 28 §6), just a quieter one.

---

## 1. The decision

The artifacts are bumped to **`0.1.0`**: the CHANGELOG's `[0.1.0] - 2026-10-08` section
documents a release that happened, so the code must carry the number its own release notes
give it. Work after that section lives under `[Unreleased]`, which is where the Phase 4.2
record already is.

## 2. What changed (the app version, everywhere it is the app)

| Place | Was | Now |
|---|---|---|
| `Cargo.toml` `[workspace.package].version` | `0.0.1` | `0.1.0` |
| `crates/autopsy-cli/src/lib.rs` `ANALYZER_VERSION` | `"0.0.1"` | `"0.1.0"` |
| `crates/autopsy-cli/tests/step44_cli_extensions.rs` (the pinning test) | `"0.0.1"` | `"0.1.0"` |
| `apps/mcp-server/package.json` (+ lockfile, + `server.ts` server info) | `0.0.1` | `0.1.0` |
| `apps/web-inspector/package.json`, `index.html` header label | `0.0.1` | `0.1.0` |

`autopsy version` now prints `Synevid (Code Autopsy) v0.1.0` — and, deliberately,
`Schema Version: 0.0.1` on the next line. Those are two different numbers about two
different things, and the output saying both is the point.

## 3. What did *not* change, and why (this list is the substance)

Every remaining `0.0.1` in the repository is a **format version**, not the app version.
Changing one silently invalidates persisted artifacts, stored baselines, or computed
digests, so each is pinned on purpose:

* `autopsy.toml` `config_version` (and the default in `load_or_default_config`, and the
  test fixtures in `tests/src/lib.rs`) — the **config file format** version.
* `InvariantsConfig::version` (default suite + fixtures) — the **invariants config format**
  version.
* `.autopsy/baseline.json` `version` (+ schema + the e2e fixture that writes one) — the
  **accepted-debt file format** version.
* `crates/autopsy-report` `schema_version` **and the digest prefix**
  `b"autopsy-result-v0.0.1:"` — the **report schema** version and the string hashed into
  every result digest. Bumping it would change every digest ever computed against the same
  input, which is a compatibility break, not a rename.
* `analyzer_version: "0.0.1"` literals inside unit/e2e test fixtures — those are *inputs*
  a test constructs and digests itself; they are self-consistent at any value and are not
  assertions about the shipped constant. (The one place that *asserts* the shipped
  constant — `step44`'s pinning test — was updated, so a future bump is conscious.)
* `docs/START_HERE.md` and `docs/3.1/*` — historical plan text ("Must v0.0.1") describing
  the requirements document's own milestones, not the artifact.

The distinction is now written down here so the next bump does not have to rediscover it:
**app version = what ships; format version = what the shipped thing writes.** Only the
first moved.

## 4. Verification

* `cargo build -p autopsy-cli` → `./target/debug/autopsy version` prints
  `v0.1.0` / `Schema Version: 0.0.1`.
* `cargo metadata` parses the workspace with the new version (every member crate inherits
  it via `version.workspace = true`).
* The full gate stack ran green after the bump: fmt, clippy `-D warnings`, workspace tests
  (829 passed / 0 failed), doc tests, determinism 100/100, arch boundaries, pre-commit
  12/12 hooks, MCP typecheck+lint+tests (16/16), inspector tests (11/11).

FR-002 note: `ANALYZER_VERSION` feeds snapshot identity, so snapshots taken before this
bump have different ids than snapshots taken after at the same input state. That is the
mechanism working as specified (identity includes the analyzer version), not a defect; the
determinism gate (identical inputs → identical digests *within* a run set) still holds
100/100.
