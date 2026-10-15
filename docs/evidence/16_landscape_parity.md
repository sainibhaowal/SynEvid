# Evidence 15 — Landscape Parity: Universal Install, Python Adapter, Agent Pack

Date: 2026-10-10. Triggered by an external landscape audit against
GitHub/package-registry competitors in the same category (ripwire 2.3k★,
codanna, ctxo, CodeCompass, blastcode, plus AI-review incumbents).

## 1. Gap analysis vs the field (all closed or honestly deferred)

| Rival advantage (vendor-reported) | What we shipped | Proof |
|---|---|---|
| 1-line install, prebuilt binaries | `scripts/install.sh`: release-download first, source build fallback (auto-installs Rust), host wiring opt-in, `version` self-verify | Ran end-to-end into a temp prefix: `Synevid (Code Autopsy) v0.0.1` |
| Auto-wiring into agents | `scripts/setup-agent-hosts.py`: 7 hosts (Claude Code, Claude Desktop, Cursor, Windsurf, VS Code, opencode, Gemini CLI), backup-first, idempotent, `--check` dry-run | Fake-home test: cursor updated + existing entry preserved; rerun idempotent; `--hooks` installs 1 entry (no accumulation) |
| Pre-edit safe-edit hook | `agents/hooks/safe-edit.sh` advisory + fail-open (always exit 0), repo-dir discovery, path relativization, digest in output | Live: `{"advisory":true,"top":[...],"digest":"9c40…","command":…}`; non-code and malformed input → empty output, exit 0 |
| Agent skills | `agents/skills/synevid-forensics.md` (blast-first, verify-after, coverage honesty, digest discipline) | committed; referenced in install output |
| Multi-language reach | **Python adapter** (tree-sitter, `autopsy-adapter-python`): functions/methods/classes/module vars, import/absolute/relative resolution incl. sibling-package heuristic, Calls (local, `self.`, `super().`, typed receivers, constructor), Inherits, contracts (self/cls stripped, defaults normalized, `*args`/`**kwargs`), honest coverage (eval/exec/`__import__`/`getattr`/metaclass) | 14 unit tests incl. conformance suite; CLID-level live: sibling import resolved cross-file (`call_expression:imported-resolved`), caller in backward closure |
| Token economy | unchanged: `--profile callers` + `--top-k` is the compact interface | `IMPACT` shortlist = 3 entities on corpus case |

## 2. What we did NOT do (and why)

- **Language breadth beyond Python**: one-deep-first is an explicit project
  rule; further adapters are partner-driven.
- **Blocking pre-edit enforcement**: we ship an *advisory* hook. A blocking
  hook becomes a productivity tax unless tuned per-team — part of the
  design-partner study.
- **Benchmark retuning**: no thresholds touched; only added the new adapter
  and install paths.

## 3. Regression safety record

- `cargo test --workspace --all-targets`: **161 passed / 0 failed** (TS path
  untouched by design; all new coverage is additive).
- `fmt` clean; `clippy -D warnings` zero; determinism 100/100; arch-boundary
  and cross-platform scripts pass; `make check` exit 0.
- MCP: typecheck + lint + 8/8 tests (new tools params are optional-only).

## 4. Public comparison record

`docs/COMPARISON.md` documents the landscape with vendor claims labelled
as such, our honest losses (breadth, default precision, token cost), and
falsifiers that would invalidate the positioning.

---

## 5. Measured competitor bake-off (added 2026-10-11)

Seven direct rivals were cloned into a sandbox (`/home/ravi/rival-sandbox`)
and **built and run** on identical fixtures, not read about. Full method,
per-tool answers and scorecard: [`docs/COMPARISON.md`](../COMPARISON.md).

Ran and measured: ripwire (2.3k★), blastcode, CodeCompass, code-impact-mcp,
codanna. **Not evaluated** (stated, not hidden): ctxo (`ENOSPC` during
`pnpm install`), coderadius (requires `bun`).

Headline findings:

1. **SynEvid is the only tool that blocks.** On the identical breaking
   signature edit, SynEvid `verify` exits **2** with a `FAIL
   api-compatibility` finding and a BLAKE3 receipt; ripwire detects the break
   (`--edit-check … status="contract-change" incompatible="1"`) but exits **0**;
   blastcode reports `"No provable breakage"` and exits **0**.
2. **codanna missed the typed-DI call** on the fixture —
   `retrieve callers createUser` → *"No functions call 'createUser'"*, and
   `retrieve calls handleCreate` → *"'handleCreate' makes no function calls"*,
   i.e. it dropped `this.userService.createUser(...)` (indexing logged
   `23 resolved | 20 skipped`).
3. **CodeCompass embeds a wall-clock `updated_at` in every response**, so its
   output is not byte-reproducible across time even though graph content is.
4. **Determinism is table stakes**, not a differentiator — ripwire, blastcode
   and codanna are all byte-identical on rerun. Our edge is publishing a
   *digest* and *receipts*, not being deterministic.
5. **ripwire has the best per-edge honesty gauges** of anything measured
   (`via="name"`, `counts_floor`, `radius_untested`, `graph_ambiguous`).

### 5.1 Defect this bake-off found in SynEvid, and the fix

Python fixture comparison: ripwire resolved `helper` →
`Calculator().run(n, 2)` at depth 2; **SynEvid did not** — the adapter
handled `self.`, `super().`, typed receivers and `x = C(...)` bindings, but not
the *inline construction chain*, and dropped it silently (a violation of the
honest-coverage boundary, FR-014/FR-030).

Fixed: `call_expression:chained-constructor` resolution in
`crates/autopsy-adapter-python/src/lib.rs`, with regression test
`test_call_edges_chained_constructor_receiver`.

Verification: adapter tests 14 → **15**; workspace **161 → 162 passed / 0
failed**; `cargo fmt --check` clean; `clippy -D warnings` zero;
`verify-determinism.sh` **100/100**; `verify-arch-boundaries.sh` PASSED.
Re-verified live: backward closure on the Python fixture now returns
`add`, `Calculator.run`, **`helper`** — parity with ripwire — with
`result_digest` stable across reruns (`20664b0baf0d23e6…`).

### 5.2 Newly identified missing feature (not yet built)

ripwire ships a **test-coverage axis** (`radius_tested` / `radius_untested`):
"this part of the blast radius no test reaches". SynEvid has `CoverageState`
for *semantic* coverage but **no test-coverage dimension at all**. Logged as the
top parity gap for the next cycle. We are behind ripwire here.
