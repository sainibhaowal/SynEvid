# Evidence 36 — Tier-2 agent-quality edge-to-edge (Go/Rust adapters, repair advice, workspace, pinned browse, session receipts)

Implements all five Tier-2 items from `Resource/Agent_Advancement_Roadmap.md` end-to-end,
production-grade, with the full gate stack green (`pre-commit run --all-files` All Passed,
898 workspace tests, 35/35 MCP node tests, 100/100 determinism runs).

---

## T2.2 — Go and Rust language adapters (`autopsy-adapter-go`, `autopsy-adapter-rust`)

Two new core crates behind `autopsy-adapter-api::LanguageAdapter`, same two-phase
architecture as the TypeScript/Python adapters: `parse` → `extract_symbols` → global
qualified-name index → `extract_edges` with the complete index visible. Zero-LLM,
read-only, tree-sitter only (`tree-sitter 0.24`, `tree-sitter-go 0.23.4`,
`tree-sitter-rust 0.23.3`; each verified latest on crates.io).

### Go (`crates/autopsy-adapter-go`, ~1,300 loc)

- **Honest coverage:** code-inclusion → `Unknown` (cgo `import "C"`, `plugin.Open`,
  `reflect`), run-time behavior beyond the analyzed bytes → `Partial` (`unsafe.Pointer`,
  `//go:generate`, generated-file tag, `DO NOT EDIT`). Never converts Unknown into PASS.
- **Symbols:** functions, methods (receiver-typed scope, pointer/value receivers
  resolved through `parameter_declaration`'s `type` field), structs → `class`,
  interfaces → `interface` (explicit `type_alias` arm for `type A = B`), package-level
  `var`/`const` → `variable`; export visibility from the first rune's case.
- **Edges:** `Contains` per symbol; imports resolved against the filesystem: the
  `go_import` path anchors to the package directory (lexicographically-first `.go`
  file as provenance `go_import:`); unresolved imports emit honest name-level
  `Partial` edges (`go_import_unresolved:<pkg>`) — never dropped. Struct/Interface
  embedding → `Inherits`. Call edges: bare calls resolve on unique index hits;
  package-qualified calls (`pkg.F()`) resolve through the import-alias map built
  once from in-memory content (no disk re-reads) — ambiguity degrades to name-level
  `Partial` with `call_expression:name-level`, never a guess.
- **Contracts:** pointer receivers → `mutates-receiver` effect; result types as
  output; parameter types as inputs.

### Rust (`crates/autopsy-adapter-rust`, ~1,900 loc)

- **Honest coverage:** `include!` → `Unknown`/`DynamicImport` (code exists outside the
  analyzed bytes); `unsafe`, `dyn Trait` dispatch, `env!`/`option_env!`,
  proc-macro sites, `DO NOT EDIT` → `Partial`. Confirmed by 7 unit tests +
  conformance tests: valid → Verified, dynamic → Partial/Unknown per construct.
- **Module bridge:** `crate::` → `src/` probes (`<path>.rs` preferred over
  `<path>/mod.rs` — `use crate::billing::ledger::Ledger` binds the *module* file
  `ledger.rs`, not the parent package), `super::`/`self::` anchored on the importing
  file, `use` flattened per leaf (`a::{b, c}` and `a as x` and `a::*` synthetic keys)
  with a per-file alias bindings map; unresolved uses emit
  `use_declaration_unresolved:<name>` `Partial` edges.
- **Symbols:** structs → `class` (fields inside the scope), enums → `enum`, traits →
  `interface`, type aliases → `type`, impl functions → `method` (scope = impl target,
  whitespace-stripped), module-level `const`/`static` → `variable`; visibility from
  `pub`. Trait default methods scope under the trait (`method:Charge.charge`);
  impl methods under the target (`method:Vault.settle`).
- **Trait edges:** `impl Trait for Type` → `Implements` (target resolved when unique,
  else name-level), supertrait bounds (`trait A: Send`) → `Inherits`
  (`trait_supertrait` provenance); `impl Vault` inherent impls emit nothing false.
- **Call edges:** bare calls resolve when the two-segment qname (`file::name`) is a
  unique index hit; `module::name(...)` resolves through use bindings (module dir or
  binding file scope); `x.method(...)` (field expressions) stay honest name-level
  `Partial`.
- **Contracts:** `unsafe`/`async` modifier effects, `&self`/`&mut self` receivers,
  `mut` inputs + effect ordering all covered in
  `test_rust_contracts_effects`.

### Wiring, conformance, determinism

- `autopsy-cli` instantiates all four adapters per analysis with
  `with_root_dir(repo_root)`; incremental, diff-staging, and doctor paths updated.
- `classify_language` + `is_text_language` cover `go` (LF-normalization applies);
  `is_analyzed_language` covers both.
- Fixture-backed unit suites: 8 Go + 7 Rust adapter tests (conformance, symbols,
  edges, contracts, honest-coverage disclosure, byte-identical re-extraction) plus
  6 e2e integration tests in `tests/tests/e2e_adapter_go_rust_conformance.rs`
  covering a real Go package layout (`checkout.simple` import x-index binding), a
  real Rust module tree (`crate::billing::ledger` → `file::src/billing/ledger.rs`
  Verified import), and a mixed Go+Rust+TS engine run asserting cross-language
  symbols plus snapshot determinism.

---

## T2.1 — Next-best-action on every error

- **Rust CLI:** `CliError::repair()` derives a deterministic
  `ErrorRepair { what_failed, why, next_action }` triple from the variant plus the
  message's own patterns (`config_repo_root_missing`, `config_field_missing`,
  `analysis_snapshot_missing`, `analysis_symbol_missing`, `storage_busy`, and so on).
  `run_cli` prints it on stderr for every execution failure AND for clap
  argument-parse failures (`what_failed: argument_parse` + the `--help` pointer), so
  an agent can self-repair without human interpretation.
- **MCP:** `evidenceEnvelope` attaches `repair` to the envelope exactly when the
  engine exited nonzero *and* emitted its advice markers on stderr — parsing the
  engine's own lines rather than re-deriving a second mapping. A policy failure
  carrying a report (and no markers) gets no advice, so `repair` is never invented.
  `refusedEnvelope` now requires an explicit `next_action`, making every
  server-side refusal actionable (explain, session, salt, edges resource).

---

## T2.3 — Multi-repo workspace binding

- **CLI:** `autopsy workspace register [<path>] [--id ID] | remove --id ID | list`.
  Store: `.autopsy/workspace.json` (`format_version: 0.0.1`, `BTreeMap` entries,
  atomic write). `list` re-detects honest status at read time: `accessible` only when
  the path exists, `analyzed` only when the repo's autopsy storage opens and holds a
  stored snapshot. Coverage counts/status come from `autopsy-workspace-registry`
  (`federation_coverage`), never improvised.
- **MCP:** every tool that takes a path takes `repo_root`; `autopsy_contract_diff`
  gains `repo_root` and resolves `from`/`to` against it; blast-radius resolves
  `catalog_root`/`traces`/`surface`; test-selection resolves `lcov` — all absolute-
  POSIX via `resolveAgainstRepoRoot` in one deterministic place.
- **Resources:** `autopsy://graph/map{&repo_root,snapshot}`,
  `autopsy://graph/symbols{&repo_root,snapshot}`, `autopsy://graph/edges{&repo_root,
  snapshot}`, and the new `autopsy://workspace{?repo_root}` resource served by the
  CLI's own `workspace list`.
- **Honest limitation (documented):** temporal views remain inherently
  git/worktree-bound (`timeline`/`co_change`/`lineage` read `.git` in the declared
  root); pinning applies to the analysis-side commands only.

## T2.4 — Pinned browse end-to-end

- **CLI:** `--snapshot-id` on `symbols`, `edges`, `find`, and `map` (`pinned_or_worktree`
  helper: stored snapshot rebuilds byte-identical artifacts from cache; unknown IDs
  fail with exit 3 + repair advice naming `baseline --persist`). Blank values are
  treated as unpinned rather than trusted.
- **MCP:** `pinningFlags` extends to the whole browse family (`--snapshot-id`), so
  `AUTOPSY_SNAPSHOT` pins a session across *all* read side of the engine; `find`
  gains an explicit `snapshot_id` schema field; map/symbols/edges resources accept a
  `snapshot` template variable. Node tests prove a pinned browse re-reads the stored
  snapshot and that a ghost pin refuses with advice.

## T2.5 — Session receipts (repair loop as a tool)

- **CLI:** `autopsy session start | record | show | close | list`
  (`crates/autopsy-cli/src/session.rs`). Store `.autopsy/sessions.json`
  (`format_version: 0.0.1`, `BTreeMap`, atomic rename).
- **Determinism:** session IDs are BLAKE3 of the normalized repo root plus the
  store's own counter — unique within a workspace, reproducible, no wall-clock
  time anywhere in the record. Steps are ordered by assigned index; receipts carry
  `verdict` (`pass`/`fail`/`unknown`), `evidence_step`, `evidence_exit_code`,
  `steps`, and `session_id`, derived only from recorded steps.
- **Honesty:** the close exit code mirrors the verdict's evidence (0 / the failing
  code / 3 for unknown), so a pre-commit hook auto-invoking close inherits the
  evidence verdict. Closing twice is a refusal; the receipt is final.
- **MCP:** `autopsy_session` tool (11 tools total) with `action`
  start/record/close/show/list, `invoked` argv, `exit_code`, `snapshot_id`, `note`.
  Node test drives the full lifecycle through the server's own `runAutopsy`.
- **Hook wiring:** `agents/hooks/autopsy-session.sh` — an opt-in pre-commit hook for
  consumer repos: no-op unless an autopsy session store exists with an open
  session; then closes the open session and mirrors the verdict as the hook exit
  code. This engine's own CI never activates it (it requires `.autopsy/
  sessions.json`).

---

## Verification telemetry

- `cargo clippy --workspace --all-targets -- -D warnings` — 0 diagnostics.
- `cargo test --workspace --all-targets` — 898 passed, 0 failed (72 suites).
- `verify-determinism.sh` — 100/100 byte-identical digest runs.
- `verify-arch-boundaries.sh` — passed (core crates stay offline, LLM-free).
- `pre-commit run --all-files` — All Passed, including MCP typecheck+35 tests.
- New failure surfaces exercised by tests, not by hope: a blank `find` refusal
  (exit 3 + `analysis_empty_input`), ghost snapshot pin (exit 3 +
  `analysis_snapshot_missing`), duplicate workspace registration, double-close
  receipt refusal.
