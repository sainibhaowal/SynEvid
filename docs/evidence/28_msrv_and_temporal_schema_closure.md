# Evidence 28 — Closure: Pillar 1's schema, and an MSRV that was not true

**Date:** 2026-10-16
**Scope:** two items carried as outstanding after Phase 4.2 reached Step 45 — a missing
public schema (`schemas/temporal.schema.json`, recorded in
`docs/requirements/TRACEABILITY.md` as **NOT DELIVERED**) and a declared MSRV that the
code contradicts.

---

## 1. The Pillar 1 schema

`schemas/temporal.schema.json` now exists, closed, describing the `autopsy temporal`
envelope: the journal's own canonical projection embedded verbatim under `journal`, plus
`stats`, `coverage` and `notes` — the three fields the CLI adds, named as additions rather
than merged into the engine's value.

Two decisions are in it that are worth naming:

* **`coverage` is derived, and the schema says how.** `unknown` facts make it `unknown`,
  `partial` facts make it `partial`, and only a journal whose facts all resolved is
  `verified`. If it were a constant, `--strict` would pass over precisely the facts the
  engine flagged (Evidence 26 §4).
* **`predicate` is an enum of the engine's own vocabulary** — the ten `FactPredicate`
  variants — because a schema that rejects the engine's own output is worse than no schema.
  The conformance test asserts the enum's *length* against `FactPredicate::all()`, so a new
  predicate fails the test rather than the next user.

The conformance test does not sample one fact: it checks **every** fact of a real journal
against the declared field set, because a field only some facts carry is exactly what a
one-sample test misses. It also pins both structures as set equality in both directions
(schema properties vs emitted fields, `required` vs emitted fields), the same standard
`schemas/blast-radius.schema.json` is held to.

## 2. The MSRV

`Cargo.toml` declared `rust-version = "1.85"`, and `AGENTS.md`, `CONTRIBUTING.md` and
`.agents/rules/agent-engineering-rules.md` repeated "MSRV 1.85+".

The code uses `let`-chains throughout the engines and the adapters:

```rust
if node.kind() == "call_expression"
    && let Some(func) = node.child_by_field_name("function")
```

`let`-chains stabilised in **Rust 1.88**. So the declaration told a 1.85 toolchain that this
workspace is buildable at its floor, and that toolchain would fail inside an engine — a false
claim about the artifact, which is the kind of claim this project's invariants exist to
prevent.

Three things were done, in the order that matters:

1. The number was corrected to `1.88` in all four places it was stated, with the reason
   (`let`-chains) written where the number lives.
2. A CI job now **measures** it: `msrv-check` installs the declared toolchain
   (`dtolnay/rust-toolchain@1.88.0`) and runs `cargo check --workspace --all-targets`. It is
   wired into the pre-merge gate's `needs`, summary table and failure condition, so a floor
   that is too low turns the gate red instead of living on in a comment.
3. The limitation of this verification is stated here: the 1.88 toolchain could not be
   installed in the development environment used for this change (the download from
   `static.rust-lang.org` was reset twice), so the claim is verified by CI on the next push
   rather than locally. The local evidence for the *direction* of the change is the
   `let`-chain syntax above, whose stabilisation version is 1.88 — a fact about the language,
   not about this repository.

If `msrv-check` turns red on its first run, the number is still wrong and the correct
response is to raise it to whatever the compiler reports: the job exists to make the claim
testable, and a red job is the measurement succeeding.

## 3. What this does not change

* No engine, CLI, MCP or inspector behaviour: the schema is additive, and MSRV is metadata.
* The workspace version stays `0.0.1`. `CHANGELOG.md` carries a dated `[0.1.0]` section from
  2026-10-08 while `Cargo.toml`, `autopsy.toml` and the MCP/inspector labels all say
  `0.0.1`; deciding whether to bump the artifacts to `0.1.0` or to demote that section is a
  release decision, not a correctness fix, so it is left to the maintainer (and naming it
  here is the honest alternative to silently changing a released-looking version).

## 4. Verification

| Gate | Result |
|---|---|
| `cargo test -p autopsy-cli --test step44_cli_extensions` | 31 passed (was 30: the new schema test) |
| `cargo fmt --all -- --check`, `clippy -D warnings`, rustdoc `-D warnings` | PASS |
| `cargo test --workspace --all-targets` | 815 passed / 0 failed (was 814) |
| `./scripts/verify-determinism.sh`, `./scripts/verify-arch-boundaries.sh` | PASS |
| `pre-commit run --all-files` (schema hook now covers the new file) | PASS |
| `python3 -c "import yaml; yaml.safe_load(ci.yml)"` + job list | 8 jobs, `msrv-check` present |

## 5. Traceability

| Row | Before | After |
|---|---|---|
| `docs/requirements/TRACEABILITY.md` — `schemas/temporal.schema.json` | **NOT DELIVERED** ("journal is a library API only") | **VERIFIED**, pointing at the schema and the conformance test |
| `Cargo.toml` `rust-version` | `1.85` | `1.88`, enforced by `msrv-check` |
| `AGENTS.md` §3, `CONTRIBUTING.md`, `.agents/rules/agent-engineering-rules.md` | MSRV `1.85+` | MSRV `1.88+` with the reason |

## 6. Correction, same day: the floor is 1.90, and it is now *measured*

§2 declared the corrected floor to be 1.88 and said the toolchain could not be installed
locally. The download succeeded on a later retry, and the claim was then tested instead of
left to CI:

- `rustup toolchain install 1.88.0` → `cargo +1.88.0 check --workspace --all-targets`
  **failed**: `tree-sitter-language@0.1.8 requires rustc 1.90`. The *language* floor is 1.88
  (`let`-chains), but the *dependency graph* floor is 1.90.
- `rustup toolchain install 1.90.0` → `cargo +1.90.0 check --workspace --all-targets`
  **passes cleanly**, so 1.90 is the measured floor of this workspace.

Superseding §2 and its traceability row: `Cargo.toml` now declares `rust-version = "1.90"`,
`msrv-check` pins `dtolnay/rust-toolchain@1.90.0`, and `AGENTS.md`, `CONTRIBUTING.md` and the
agent rules say 1.90+ — each with the reason. The methodological point stands and sharpened:
the first correction (1.85 → 1.88) was *better* but still **unverified**, and testing it
caught a second, dependency-level falsehood that reasoning alone would have missed. The floor
is now a fact two toolchains were run against (§2's 1.88 claim is retained above as the record
of what was believed before the measurement).
