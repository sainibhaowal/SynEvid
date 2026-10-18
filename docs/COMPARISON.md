# Measured Comparison — SynEvid vs the Code-Intelligence Field

**Date:** 2026-10-11 · **Method:** every competitor below was `git clone`d and
**built and executed locally** on one identical fixture, with one identical
question. Nothing here is a vendor claim.

**Updated 2026-10-18:** competitor measurements are unchanged; SynEvid-side
evolution since the measurement (Go/Rust adapters, agent surfaces) is annotated
inline and leads each changed section with the date.

- **MEASURED** = we cloned it, built it, ran it, and read its output.
- **VENDOR** = taken from its public README, labelled as unverified.

Fixture A: `benchmarks/corpus/repo-api` (7 TypeScript files, service →
controller → route layering, one typed DI field).
Fixture B: 2 Python files (`src/math.py`, `src/calc.py`) with an inline
construction chain.

Question A: *what breaks if I change `UserService.createUser`?*
Question B: *the same, in Python.*
Then: *I added a required parameter — which tools **block**?*

---

## 1. Execution status (what actually ran)

| Tool | Repo | Build path | Status |
|---|---|---|---|
| **SynEvid** | this repo | `cargo build -p autopsy-cli` | ✅ ran |
| **ripwire** (2.3k★) | `redhat-et/ripwire` | `cmake` + C++23 (GCC 13.3) | ✅ built & ran |
| **blastcode** | `isaim0011/blastcode` | `cargo build --release` | ✅ built & ran |
| **CodeCompass** (v7.2.0) | `mmkumar5401/CodeCompass` | `pip install .` | ✅ ran (as MCP) |
| **code-impact-mcp** | `vk0dev/code-impact-mcp` | `npm ci && tsc` | ✅ built & ran (as MCP) |
| **codanna** | `bartolli/codanna` | `cargo build --release` | ✅ built & ran |
| **ctxo** (66★) | `alperhankendi/ctxo` | `pnpm install -r` | ❌ **not built** — install died with `ENOSPC` (host disk full); not evaluated |
| **coderadius** | `coderadius-ai/coderadius` | requires `bun` | ❌ **not built** — `bun` not installed; not evaluated |

Two rivals are honestly **unevaluated**. No claim is made about them.

---

## 2. Question A — who calls `createUser`? (TypeScript)

| Tool | Answer | Depth / quality |
|---|---|---|
| **SynEvid** | `UserService.createUser` ← `UserController.handleCreate` ← `routeRequest` | 3 entities, backward closure, BLAKE3 digest |
| **ripwire** | `handleCreate` (d=1), `routeRequest` (d=2); importers `userController.ts`, `authGuard.ts` | **Same chain.** Adds `graph_ambiguous=0 graph_unresolved=0 counts_floor=1`, and marks matches `via="name"` (name-only, receiver unproven) |
| **blastcode** | caller `handleCreate` @ `userController.ts:8` + callee `hashPassword` (exact) | 1 caller, labelled `confidence:"probable"`; **did not report `routeRequest`** (1-hop only) |
| **CodeCompass** | `handleCreate` (d=1, receiver `this.userService`, `resolved:true`), `routeRequest` (d=2, `receiver_type: UserController`) | **Same chain**, typed receiver, full depth |
| **code-impact-mcp** | `userController.ts` (direct), `authGuard.ts` (direct), `userRoutes.ts` (transitive) | **File-level only** — no symbol granularity |
| **codanna** | **"No functions call 'createUser'"** | ❌ **Missed it.** Cross-check: `retrieve calls handleCreate` → *"'handleCreate' makes no function calls"* — it also dropped `this.userService.createUser(...)`. Indexing reported `43 relationships | 23 resolved | **20 skipped**` |

**Verdict:** 4 of 5 running tools find the chain. codanna **missed the
typed-DI field call** — the single most common enterprise pattern — and did so
silently apart from the aggregate "20 skipped" count.

---

## 3. The decisive test — does a breaking change **block**?

Edit applied identically to all: `createUser(dto: CreateUserDto)` →
`createUser(dto: CreateUserDto, tenantId: string)`.

| Tool | Detected? | Reported as | **Exit code** |
|---|---|---|---|
| **SynEvid** | ✅ | `is_breaking: true`, `breaking_reasons: ["Inputs changed from [\"dto: CreateUserDto\"] to [\"dto: CreateUserDto\", \"tenantId: string\"]"]`; `verify` emits `[FAIL] api-compatibility` + per-finding BLAKE3 receipt | **2** ✅ blocks CI |
| **ripwire** | ✅ | `--edit-check`: `status="contract-change" change="params,broken-callers" incompatible="1"`, caller `handleCreate sites_l="8"` | **0** ❌ — its exit-2 is reserved for `--arch`/`--scan-skill`/`--quality-delta` |
| **blastcode** | ⚠️ partial | `agent_alert: "No provable breakage, but 1 call site(s) should be reviewed."`, `breaking: 0, review: 1`, `confidence: probable` | **0** ❌ |
| **code-impact-mcp** | ❌ | No signature/contract model (import-graph only) | n/a |
| **codanna** | n/a | Not reached — graph incomplete | n/a |

**This is the real gap and it is not a tie.** Three tools can *tell* you the
contract broke; only SynEvid **fails the process**. `exit 2` is the difference
between "an agent read a warning" and "the merge was blocked, with a receipt."

---

## 4. Determinism & provability

| Tool | Byte-identical on rerun? | Emits a verifiable digest? | Wall-clock in output? |
|---|---|---|---|
| **SynEvid** | ✅ 100/100 (`verify-determinism.sh`) | ✅ `result_digest` (BLAKE3) on every result + per-finding receipts | ❌ none |
| **ripwire** | ✅ | ❌ no digest field (0 occurrences) | ❌ none |
| **blastcode** | ✅ | ❌ | ❌ none |
| **codanna** | ✅ | ❌ | ❌ none |
| **CodeCompass** | ⚠️ graph content yes, **envelope no** — every response embeds `updated_at: "2026-10-11T19:40:12.410841+00:00"` | ❌ | ⚠️ **yes** |

Determinism is **table stakes — everyone here has it except the wall-clock leak
in CodeCompass.** The differentiator is not determinism; it is that SynEvid
publishes a *digest you can pin in CI*, and ripwire publishes the most granular
per-edge honesty gauges of anyone measured (`via="name"`, `counts_floor`,
`graph_ambiguous`, `radius_untested`).

---

## 5. Question B — Python (both tools support it)

| Tool | Answer for `add` |
|---|---|
| **ripwire** | `run` (d=1), `helper` (d=2, `via="name"`), importer `calc.py` |
| **CodeCompass** | n/a (not re-run) |
| **SynEvid** *(before fix)* | `add`, `Calculator.run` — ❌ **`helper` missing** |
| **SynEvid** *(after fix)* | `add`, `Calculator.run`, **`helper`** ✅ — parity |

**We lost this one, and the comparison caught it.** `helper` calls
`Calculator().run(n, 2)` — an inline construction chain. ripwire resolved it
(flagged `via="name"`, i.e. heuristically); our adapter handled `self.`,
`super().`, typed receivers and `x = C(...)` bindings, but not the chained form
— and **dropped it silently**.

Fixed in `crates/autopsy-adapter-python` with a regression test
(`test_call_edges_chained_constructor_receiver`, provenance
`call_expression:chained-constructor`). Suite 161 → **162**, determinism
re-verified.

---

## 6. Breadth (VENDOR for the rest, MEASURED where noted)

| Tool | Languages |
|---|---|
| **ripwire** | ~19 (C/C++/ObjC/Metal/CUDA/TS/JS/Python/Java/Ruby/PHP/Lua/Elixir/Dart/Kotlin/Bash/Go/Rust/Swift/C#/GDScript) — read from `--help` |
| **codanna** | 15 (VENDOR) |
| **blastcode** | multi (VENDOR) |
| **SynEvid** | **4 deep** (TypeScript/JavaScript, Python, Go, Rust) — *updated 2026-10-17:* Go and Rust adapters landed with conformance suites behind the same `adapter-api` trait, honest per-language coverage states, and benchmark corpus unchanged; see `docs/evidence/36-tier2-agent-quality.md` |

**We are behind on breadth and it shows in the scoring.** ripwire's list is
impressive and cheap for them to have; ours is deliberately deep-per-language
(conformance suites, identity/shape split, honest per-edge coverage) because
our rule is one-deep-first. The 2026-10-17 adapter additions follow that rule:
each new language ships with conformance tests and explicit `Unknown`/`Partial`
states rather than silent approximation.

---

## 7. Honest scorecard

| Dimension | Winner | Why |
|---|---|---|
| **Blocks a breaking change (exit code)** | 🏆 **SynEvid** | only tool that exits 2 by default |
| **Evidence you can pin** | 🏆 **SynEvid** | BLAKE3 `result_digest` + per-finding receipts |
| **Per-edge honesty detail** | 🏆 **ripwire** | `via="name"`, `counts_floor`, `radius_untested`, resolver gauges |
| **Language breadth** | 🏆 **ripwire** | ~19 vs 2 |
| **TS caller-graph completeness** | tie | ripwire, CodeCompass, SynEvid all find the chain; codanna misses it |
| **Symbol granularity** | 🏆 **SynEvid/ripwire/CodeCompass** | code-impact-mcp is file-level only |
| **Token economy** | 🏆 **ripwire** | `--max-tokens`, `--token-budget` binary-search shaping; blastcode publishes 92% reduction claims |
| **Determinism** | tie (SynEvid ships the receipt) | CodeCompass leaks `updated_at` |

---

## 8. Where SynEvid is genuinely worse

1. **Breadth.** 4 vs ~19 (updated 2026-10-17; was 2 at measurement time). If you need
   C++ or Java tomorrow, ripwire is there and we are not.
2. **Token discipline.** ripwire's `--token-budget` *withholds* output before
   emitting; we emit verbose JSON receipts and rely on `--top-k` after the fact.
3. **Graceful degradation.** ripwire ships `counts_floor` and
   `radius_untested` — an explicit "this number is a floor, and this part of
   the blast radius has no test". We ship `CoverageState` but not a
   *test-coverage* axis at all. **This is a real missing feature.**
4. **Two rivals went unevaluated** (ctxo, coderadius) — build environment
   limits, stated above rather than papered over.
5. **Default precision.** Still ~6% on our own harness; rivals report curated
   shortlists. Our mitigation is structural (`--profile callers`, `--top-k`),
   not tuned.

## 9. Falsifiers — what would make our position untenable

- A rival shipping **digest + blocking exit code + branch-capable baseline
  debt** at ripwire's language breadth. ripwire is the only one close, and it
  stops one flag short of blocking on the common case.
- Frontier agents resolving transitive impact natively → our kill criteria in
  `docs/benchmark/PROTOCOL.md` apply.