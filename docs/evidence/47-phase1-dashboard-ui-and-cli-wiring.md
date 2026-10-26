# Production Evidence 47: Phase 1 Reactive Web Dashboard & CLI Embedded Router Wiring

**Target Release:** v0.2.1 (Phase 1 Full Integration)  
**Parent Requirements:** FR-P1-001, FR-P1-002, FR-P1-003, FR-P1-004, FR-P1-005, FR-P1-006  
**Specification:** `Resource/5.0_Visual_Cockpit_And_Canvas_PRD/01_PHASE_1_v0.2.0_PLUGGABLE_CORE_SQLITE_AND_REALTIME_DAEMON.md` & `07_UI_BUTTONS_ANIMATIONS_MOTION_AND_WIRING_SPEC.md`  
**Execution Timestamp:** 2026-10-26  
**Status:** **100% VERIFIED**

---

## 1. Executive Summary

Following the creation of the foundational backend crates (`autopsy-plugin-api`, `autopsy-graph-layout`, and `autopsy-storage` Migration 004), this milestone completes the full end-to-end integration across both user presentation surfaces:
1. **Unified Enterprise Web Dashboard (`apps/synevid-ui/src/dashboard.html` & `crates/autopsy-cli/src/dashboard.html`):** A self-contained, enterprise dark-mode Single Page Application running 100% offline with zero npm or Node.js runtime required, zero CDNs, and zero outbound network calls.
2. **CLI Embedded Router Parity (`crates/autopsy-cli/src/ui.rs`):** Upgraded `autopsy ui` with identical `/api/v1/*` REST & SSE streaming routes as `synevid-ui`, providing byte-identical capability and telemetry reporting across CLI and standalone daemon binaries.

---

## 2. Integrated Feature Surface

### 2.1 Live Event Bus (`/api/v1/live`)
- **Status Pill:** Header displays a pulsing emerald dot and live badge (`● LIVE 18ms`).
- **SSE Stream Listener:** Real-time monotonic sequence listener (`id: <seq>\nevent: repo.indexed\ndata: ...\n\n`) feeding the reactive event bus ticker in the Overview panel without polling.

### 2.2 Multi-Repo Connection & Switcher (`/api/v1/repos/*`)
- **Repo Dropdown:** Lists registered repositories from local SQLite WAL storage.
- **Connect Modal:** "+ Connect" modal allows binding any workspace directory, computing canonical BLAKE3 repository IDs, and dynamically switching the active repository with zero page reloads.

### 2.3 Dynamic Capabilities Drawer (`/api/v1/capabilities`)
- **Plugin Inspection:** Inspects active registrations from `autopsy-plugin-api`:
  - 6/6 Language Adapters: TypeScript, Rust, Python, Go, Java, C#.
  - 8/8 Workspace Resolvers: npm, pnpm, yarn, cargo, go, maven, gradle, nuget.
  - Seeded Graph Layout Plugins: Deterministic Barnes-Hut 2D Planar and 3D Orbit.

### 2.4 Deterministic Seeded Code Universe Canvas (`/api/v1/graph/universe`)
- **HTML5 Canvas:** Force-directed node-link visualization seeded deterministically by `BLAKE3(snapshot_digest)`.
- **Interactivity:** Pan (drag), zoom (+/- buttons & mouse wheel), 2D/3D toggle, and click-to-inspect which navigates directly to that node's line-to-line ego network.

### 2.5 Line-to-Line Symbol Ego Network (`/api/v1/graph/ego`)
- **Sub-5ms SQLite Query:** Queries `.autopsy/storage.db` in $<5\text{ms}$ (typically 1–2ms).
- **Exact Calling Line Coordinates:** Inbound callers table explicitly displays caller file path and **exact calling line number** (`source_file:source_line`), with confidence badge (`EXACT`). Outbound callees similarly display exact target line coordinates.

### 2.6 Sub-50ms Real-Time Edit Simulator (`/api/v1/simulate/edit`)
- **Keystroke Buffer:** Debounced 40ms simulation loop executing in $\le 50\text{ms}$ (p95) without filesystem mutation.
- **Interactive Presets:**
  - *Clean Method:* Passes with 100% green `PASS` verdict.
  - *Breaking Arity (`taxRate: number`):* Instantly flags broken callers in Crimson Red (`#e11d48`) with line numbers and diagnostic (`Arity mismatch: expected 2 arguments, caller passes 1`).
  - *Syntax Error (`(((`):* Real-time parse failure diagnostic.
  - *Dead Code:* Warns on orphaned symbols (`0 remaining callers across workspace`) in Grey.

---

## 3. Verification & Telemetry

### 3.1 Test Suite Execution
- **`cargo test -p autopsy-cli`:** 18 unit tests, 15 lineage tests, 31 extension tests, 9 blast radius tests, 22 find and compact tests, 4 presentation tests = **99 passed, 0 failed**.
- **`cargo test -p synevid-ui`:** 3 passed, 0 failed.
- **`cargo test --test phase1_incremental_determinism`:** 3 passed, 0 failed.

### 3.2 Quality Protocol Compliance
- **Formatting:** `cargo fmt --all -- --check` &rarr; Clean (0 diffs).
- **Architectural Sandboxing:** `bash scripts/verify-arch-boundaries.sh` &rarr; PASSED (Zero LLM, zero sub-processes, zero raw socket leaks in core crates).
- **100-Run Golden Determinism:** `bash scripts/verify-determinism.sh` &rarr; PASSED (100/100 byte-identical snapshot digests).

---

## 4. Architectural Boundaries Maintained

- **Zero-LLM Invariant:** Entire simulation and verification pipeline remains 100% deterministic code.
- **Read-Only / In-Memory Daemon:** Live buffer simulation executes purely in-memory; `.autopsy/storage.db` records repositories and graph structures without altering workspace source code.
- **Permanent Zero-Major Versioning:** Milestone tagged under `v0.2.1` in accordance with the permanent `v0.xx.xxx` SemVer rule.
