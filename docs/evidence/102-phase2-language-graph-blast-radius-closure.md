# Evidence 102 — Phase 2 Language, Graph, Diff, and Blast-Radius Closure

Date: 2026-11-06  
Host: Windows 11 / PowerShell / Rust MSRV workspace  
Scope: the literal Phase 2 language-analysis and graph contract, not the separate canvas-FPS target retained in the historical UI row.

## Result

The Phase 2 correctness path is verified on Windows for the supported adapters and the daemon-backed presentation path. The result is not a claim of universal runtime understanding: reflection, generated code, dynamic imports, macros, runtime dispatch, and external systems remain explicit `PARTIAL`, `UNKNOWN`, or `ERROR` coverage where the adapters cannot prove them.

The 2D/3D renderer, animation loop, camera/orbit behavior, WebGL buffers, and canvas styling were not changed by this closure.

## Implemented closure

### Deterministic core

- Impact queries canonicalize seed order and remove duplicate seeds before traversal.
- Neighbor expansion is sorted by the complete edge identity and stable node ID, so graph insertion order cannot change the result.
- Equal-length paths use a stable-ID lexical tie-break.
- Bounded traversal preserves `truncated` and never treats a budget boundary as a complete result.
- Complete impact evidence remains separate from summary/top-k presentation.

### Real overlay analysis

- Unsaved edits are parsed and analyzed in memory only.
- The overlay path constructs only the repository-scoped adapter for the edited file. It no longer constructs every language adapter or parses the same buffer twice on each simulation.
- The candidate snapshot, semantic diff, impact traversal, and verification result are derived from the actual baseline and candidate artifacts.
- The source file and Git state remain untouched.

### Client authority and parity

- The embedded terminal parses `autopsy impact` options into a structured query and delegates to the daemon graph engine.
- URL query decoding accepts real stable IDs containing `#`, `/`, and other encoded delimiters.
- The TUI uses the authenticated daemon for connected sessions and labels offline analysis as independent.
- TUI hop counts and weakest-link explanations come from actual `entity_meta`; no fixed impact counts or provenance strings are used.
- Name-level/inferred/unknown impact evidence cannot be promoted to `PASS`. The daemon returns `WARN` or `UNKNOWN` with partial/unknown coverage instead.
- API and terminal responses compare the same snapshot, digest, and complete impacted-entity set in the real repository test.

## Verification evidence

Commands were run from the repository root with `C:\Users\Ravin\.cargo\bin\cargo.exe` because Cargo is not on the host PATH.

| Check | Result |
|---|---:|
| `cargo test -p autopsy-impact --lib -- --nocapture --test-threads=1` | 9/9 passed |
| `cargo test -p autopsy-cli --lib -- --nocapture --test-threads=1` | 25/25 passed |
| `cargo test -p autopsy-tests --test e2e_adapter_go_rust_conformance --test e2e_adapter_typescript_conformance --test e2e_graph_and_diff_conformance --test e2e_phase4_impact_and_invariants -- --nocapture --test-threads=1` | 21/21 passed |
| `cargo test -p synevid-ui terminal_impact -- --nocapture --test-threads=1` | 2/2 matching tests passed in both lib and main targets |
| `cargo test -p synevid-ui --lib -- --nocapture --test-threads=1` | 27/27 daemon, Phase 1, Phase 2, Git, timeline, analytics, and truth-contract tests passed |
| `node --test --test-name-pattern="Temporary repository drives graph selection" test/phase2_cockpit.test.js` | 1/1 real temporary-repository E2E passed on warm release daemon |
| `node --test test/*.test.js` in `apps/web-inspector` | 38/38 inspector tests passed, including the real temporary-repository flow |

The real E2E verified:

1. a temporary repository is registered and indexed;
2. a real graph node is selected;
3. `/api/impact` returns a real digest and impacted set;
4. `/api/v1/terminal/exec` returns the same digest, snapshot, and complete impacted set;
5. a percent-encoded seed is equivalent to the canonical seed;
6. an unsaved edit is analyzed without changing the source file;
7. the simulation computation stays within the test gate of 50 ms on the warm Windows release path;
8. unsupported SQL remains visibly non-PASS.

## Requirement mapping

| Phase 2 contract | Evidence |
|---|---|
| Versioned supported-language adapters | TypeScript, Go, Rust, Java/Kotlin, C# adapter conformance tests |
| Stable symbol and edge identity | adapter golden tests and mixed-language deterministic CLI tests |
| Typed edges and honest ambiguity | adapter conformance fixtures and impact grade tests |
| Forward/backward/bidirectional bounded traversal | bounded impact E2E and terminal structured-query tests |
| Deterministic budgets and truncation | impact unit/E2E tests, including explicit truncation |
| Real before/after semantic diff | graph/diff conformance and simulator E2E |
| Read-only simulation | temporary-repository source/Git immutability assertions |
| Terminal/API parity | synevid-ui parity test and real web-inspector E2E |
| Unsupported behavior never PASS | unknown-coverage hard-boundary E2E plus daemon verdict guard |
| Canvas preservation | Phase 2 implementation diff excludes renderer, animation, camera, WebGL, and styling changes |

## Boundaries that remain explicit

- The separate Phase 2 visual-performance row still records the previously measured 5,000-node FPS/theme-latency limitations; this evidence does not relabel those measurements.
- Windows proof is complete for the commands above. Linux/macOS execution remains a CI responsibility and is not inferred from Windows results.
- Static analysis cannot prove arbitrary runtime behavior. The UI and machine-readable envelopes must continue to display the coverage scope and limitations beside any conclusion.
