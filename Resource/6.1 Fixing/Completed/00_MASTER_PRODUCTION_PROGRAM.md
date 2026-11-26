# SynEvid Production Program: End-to-End Truthful Forensic System

## 1. Purpose

This program defines the work required to make SynEvid a production-grade, offline-capable, deterministic code-forensics system across the Rust engine, daemon, CLI, TUI, MCP, Electron shell, web UI, Git engine, analytics, reports, and release artifacts.

The target is not “a UI that looks complete.” The target is an auditable system where every displayed conclusion can be traced to:

- repository identity;
- exact snapshot or working-tree state;
- parser and adapter versions;
- algorithm and budget;
- coverage state;
- source locations and provenance;
- deterministic result digest;
- test and execution evidence.

## 2. Current readiness statement

Current state: Phase 0/1 production-program implementation is complete for the
audited Windows source/runtime path, with native Linux/macOS proof delegated to the
configured GitHub Actions matrix.

The earlier blockers in this document were repaired in the current source tree:

- embedded terminal impact uses the indexed graph and returns truthful coverage;
- cockpit and terminal metrics are measured or explicitly unavailable;
- Git decode failures cannot become `PASS`;
- CLI attach/status and MCP authentication use the daemon contract;
- the TUI reads daemon status and replays the shared event cursor;
- daemon responses expose the shared evidence envelope and structured operation logs;
- WAL/SHM recovery, event restart, and transaction atomicity have executable tests.

The remaining release condition is evidence, not an unimplemented feature: native
GitHub Actions runners must execute the cross-platform process/browser matrix before
the release dossier may claim platform-wide verification.

Local closure evidence for this program is recorded in
`docs/evidence/101-phase0-phase1-truth-and-daemon.md`. The Windows run passed the
full Rust workspace with `PYTHON` explicitly pointing at Python 3.12, Rust docs,
clippy, schema validators, determinism, architecture boundaries, MCP, web build,
browser/Electron behavior, and the native daemon restart/replay test. The GitHub
Actions matrix is configured for Ubuntu, macOS, and Windows; those runners must
execute before local Windows evidence is generalized to all supported platforms.

## 3. Production definition

SynEvid is production-ready only when all of the following are true:

- no fabricated or hardcoded operational result remains;
- all analysis results are deterministic and reproducible 100 times;
- unsupported behavior is visible as `UNKNOWN` or `PARTIAL`;
- analysis remains read-only; explicit Git mutations are separate, confirmed, and receipted;
- one authenticated daemon is the source of truth for active repository, session, snapshot, and event sequence;
- CLI, TUI, MCP, Electron, and web UI consume the same public contracts;
- all critical workflows have real repository integration tests and real Electron/browser tests;
- performance budgets are measured on controlled hardware and reported from real samples;
- failure, recovery, corruption, restart, authentication, cancellation, and shutdown paths are tested;
- release artifacts are assembled, launched, smoke-tested, checksummed, and reproducible.

## Non-requirements

- no claim that static analysis proves arbitrary runtime behavior;
- no LLM in correctness crates;
- no fabricated demo values in production views;
- no network dependency for offline correctness;
- no duplicate source of truth for repository or session state;
- no canvas redesign as a side effect of backend work.

## 4. Target architecture

```text
                 ┌───────────────────────────┐
                 │   One per-user SynEvid     │
                 │   authenticated daemon     │
                 │ repository/session/event   │
                 │       source of truth      │
                 └────────────┬──────────────┘
       ┌──────────────────────┼──────────────────────┐
       │                      │                      │
   CLI / CI                TUI / terminal        MCP / AI
       │                      │                      │
       └──────────────────────┼──────────────────────┘
                              │
                       Electron / web UI
                              │
                  canvas and analytical views
```

The Rust correctness crates remain independent of GUI, MCP, and LLM code. The daemon exposes stable public contracts; every client uses those contracts instead of reimplementing analysis.

## 5. Phase gates

| Gate | Scope | Must be proven |
|---|---|---|
| G0 | Truth and requirements | Every output has a source, state, provenance, and digest |
| G1 | Core and daemon | Storage recovery, event replay, watcher, auth, and deterministic scan pass |
| G2 | Graph and blast radius | Real parser-backed impact, truthful terminal, coverage propagation |
| G3 | Git forensics | Packed/delta/tag/merge/dirty history, semantic risk, safe failures |
| G4 | Unified runtime | CLI/TUI/MCP/UI use one daemon, repository identity, and event cursor |
| G5 | Electron | Dev runtime, packaged runtime, security, install, upgrade, smoke tests |
| G6 | Analytics | Dead-code and risk findings are evidence-backed and honestly labeled |
| G7 | UI/UX | Every state is visible without changing the canvas renderer or design system |
| G8 | Verification | Unit, integration, E2E, browser, Electron, chaos, and coverage gates pass |
| G9 | Performance | Budgets are met or explicitly changed by an approved decision |
| G10 | Release | Artifacts, checksums, SBOM, rollback, and release evidence are complete |
| G11 | Data lifecycle | Backup, restore, migration, retention, and safe cleanup pass |
| G12 | Security | Threat boundaries, supply chain, auth, sandbox, and resource limits pass |
| G13 | Compatibility | API, event, schema, report, and migration matrices pass |
| G14 | Autonomous operation | Agent identity, budgets, observation, replay, and policy behavior pass |
| G15 | Product acceptance | Real-repository workflows and support readiness pass |

## 6. Universal output contract

Every operation returns a common envelope containing:

- `operation_id`;
- `repository_id` and canonical repository path;
- `snapshot_id` or working-tree identity;
- `session_id`, `workflow_id`, and `event_seq` when applicable;
- `status`: `PASS`, `FAIL`, `WARN`, `UNKNOWN`, `PARTIAL`, `ERROR`, or `UNAVAILABLE`;
- `coverage_state` and coverage reasons;
- `truncated` and budget information;
- source evidence and provenance;
- algorithm and tool version;
- deterministic BLAKE3 digest;
- actionable next step.

No UI may invent values when a field is missing. It must show “not measured,” “not available,” or the structured error.

## 7. Canvas safety invariant

No phase may modify the 2D/3D renderer, camera, animation, motion physics, node placement, or existing visual styling as an incidental side effect. New data must enter through typed view-model adapters. UI changes require:

- screenshot comparison before and after;
- canvas interaction regression tests;
- frame-time measurement;
- explicit review of `CodeUniverse3D.tsx` and related renderer files.

## 8. Definition of production complete

The program is complete only when [16_IMPLEMENTATION_BACKLOG_AND_DOD.md](16_IMPLEMENTATION_BACKLOG_AND_DOD.md) has no open P0 or P1 item, every requirement in [15_TRACEABILITY_MATRIX.md](15_TRACEABILITY_MATRIX.md) has evidence, and a release dossier links test logs rather than narrative claims.

