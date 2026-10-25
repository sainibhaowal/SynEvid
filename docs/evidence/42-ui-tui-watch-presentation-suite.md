# Production Evidence: Presentation & Real-Time Observation Suite (Web UI, TUI & Live Watch)

**Receipt:** `EVID-PRESENTATION-SUITE-042`  
**Date:** 2026-10-25  
**Version:** `0.1.0`  
**Status:** `VERIFIED`  
**Traceability:** `AGENT-T3.10`, `FR-026`, `INV-CROSS-001`, `INV-SANDBOX-001..004`, `ARCH_NO_CORE_TO_MCP`

---

## 1. Overview & Context

Prior to this deliverable, SynEvid provided complete analytical capabilities via CLI commands and the MCP server, but developer and human inspector user experiences had three operational gaps:
1. **Interactive Terminal Experience:** CLI executions printed static stdout text or JSON. Exploring blast radii, inspecting breaking changes, and traversing forensic trajectories required repeated manual terminal commands.
2. **Local Web Dashboard & Multigraph Exploration:** The web inspector (`apps/web-inspector`) required manual JSON report exports and file drag-and-drop. There was no single-command local web server embedded directly in the binary suite to visualize dependency multigraphs, invariants, and agent execution runs without installing npm/node dependencies.
3. **Continuous Developer Watch Feedback:** Developers editing code had to manually invoke `autopsy verify` or `autopsy impact` repeatedly after edits without real-time file-change triggers.

This deliverable implements the complete **Presentation & Real-Time Observation Suite** across 3 production pillars, adhering strictly to SynEvid's architectural invariants:
- Zero LLM in core correctness crates.
- Strict architectural boundary: presentation and HTTP server sockets reside exclusively in `apps/*` (`apps/synevid-ui`), while core engine crates (`crates/*`) maintain 100% offline determinism and zero network sockets (`INV-SANDBOX-004`).
- Zero external CDNs or outbound internet requests in dashboard presentation.
- 100/100 deterministic repeatability across all operations.

---

## 2. Technical Architecture & Implementation

### 2.1 Pillar 1: Embedded Local Web Dashboard (`apps/synevid-ui` & `autopsy ui`)
- **Embedded Server Architecture:** Developed `apps/synevid-ui` using `tiny_http` to serve the full dashboard directly from executable memory with **zero Node.js or npm runtime dependencies**.
- **Self-Contained SPA (`apps/synevid-ui/src/dashboard.html`):**
  - Enterprise dark-mode design system (`#080c14` canvas, `#0f172a` surfaces, `#334155` borders, glassmorphism paneling).
  - Status tokens: Emerald (`#10b981`) for Verified/Likely, Amber (`#f59e0b`) for Inferred/Partial/Possible, Rose (`#f43f5e`) for Policy Failures/Breaking Changes, Violet (`#8b5cf6`) for Declared.
  - Interactive tabs:
    - **Overview:** Coverage gauge (Verified vs Partial vs Unknown), symbol/edge inventory, active snapshot digest.
    - **Invariants:** Live invariant rule evaluations, failure locations, and repair advice.
    - **Blast Radius:** Interactive symbol search bar, dependency multigraph canvas with real-time physics and node dragging, and side-by-side tier tables (`likely_affected` proven vs `possibly_affected` with weakest-link provenance tags).
    - **Trajectory:** Forensic task history with exit codes, cryptographic digests, step counts, and next-agent guidance.
  - 100% offline compliance: Embedded SVG icons, CSS styling, and vanilla JavaScript without external fonts or CDN dependencies.
- **REST Endpoints:**
  - `GET /` — Serves single-page application dashboard.
  - `GET /api/status` — Repository root, active snapshot hash, coverage state, symbol/edge counts.
  - `GET /api/verify` — Live invariant rules evaluation with rule IDs, statuses, and messages.
  - `GET /api/impact?seed=<symbol>` — Two-tier widest-path blast radius with integer confidence grades.
  - `GET /api/trajectory` — Forensic run records loaded from `.autopsy/runs/*.json`.
  - `GET /api/graph` — Dependency nodes and edges formatted for visual canvas rendering.
- **CLI Guidance (`crates/autopsy-cli/src/ui.rs`):**
  - Subcommand `autopsy ui [--port 3333] [--host 127.0.0.1] [--open]` prints connection telemetry and JSON specifications.
  - To respect `ARCH_NO_CORE_TO_MCP` and `verify-arch-boundaries.sh`, the HTTP socket is hosted in `synevid-ui` rather than inside `crates/autopsy-cli`.

### 2.2 Pillar 2: Interactive Terminal Dashboard (`crates/autopsy-cli/src/tui.rs` & `autopsy tui`)
- **Terminal Engine:** Built using `ratatui 0.29` with the `crossterm 0.28` backend.
- **State Machine (`TuiApp`):**
  - **Tabs:** `[1] Overview`, `[2] Invariants (Verify)`, `[3] Blast Radius`, `[4] Trajectory`, `[5] Help`.
  - **Keyboard Navigation:**
    - `Tab` / `1-5`: Switch active view tab.
    - `↑` / `↓` / `j` / `k`: Scroll findings or list items with visual highlighting.
    - `/`: Focus search input to change the active seed symbol.
    - `Enter`: Submit new seed symbol and re-evaluate blast radius.
    - `r`: Refresh repository state and re-scan `.autopsy/runs`.
    - `q` / `Esc`: Restore alternate screen, re-enable cursor, disable raw mode, and exit cleanly.
- **Subcommand Integration:** Registered `autopsy tui` in clap grammar and `toolspec.rs`.

### 2.3 Pillar 3: Real-Time Live Watch Mode (`crates/autopsy-cli/src/watch.rs` & `autopsy watch`)
- **Event-Driven File Watcher:** Uses `notify 6` (`RecommendedWatcher`) recursively monitoring the repository root.
- **Noise Filtering:** Automatically filters out noise directories and temporary files (`.git`, `node_modules`, `target`, `.autopsy`, `.cache`, `.idea`, `.vscode`, `*.tmp`, `*~`) and restricts triggers to code and configuration extensions (`.ts`, `.tsx`, `.js`, `.jsx`, `.json`, `.toml`, `.rs`, `.yml`, `.yaml`, `.md`).
- **Debounced Incremental Re-Verification:** Configurable debounce buffer (default 200ms) prevents thrashing during multi-file edits. Upon settling, executes fast incremental graph building and evaluates the invariant suite.
- **Live Ticker Cards:** Formats ANSI color-coded console ticker cards:
  - `[PASS]` Invariants Intact (X/Y rules passed) with snapshot hash and coverage state.
  - `[FAIL]` Invariants Violated (X/Y rules failed) naming rule IDs, violation messages, and target entities.
- **Interactive Hotkeys (Non-Blocking):**
  - `v`: Force immediate re-verification.
  - `b`: Capture a fresh baseline snapshot immediately.
  - `c`: Clear terminal screen and redraw status banner.
  - `q` / `Esc` / `Ctrl+C`: Exit watch mode cleanly.

### 2.4 Self-Describing Tool Specification & Packaging
- **`toolspec.rs`:** Added `ui`, `tui`, and `watch` tools to the machine-readable catalog (`autopsy tools --format json`), raising the total tool count to 26 subcommands.
- **Packaging & Installers:** Updated `scripts/install.sh` and `scripts/install.ps1` to build and package `synevid-ui` alongside `autopsy` and `synevid-lsp`, enabling universal one-line installation across Linux, macOS, and Windows.

---

## 3. Verification & Compliance Telemetry

### 3.1 Architectural Boundary Verification
Ran `./scripts/verify-arch-boundaries.sh` in WSL:
```text
Verifying architectural dependency boundaries across core crates...
Verifying Zero Code Execution invariant (no sub-process execution in core crates)...
Verifying Zero Network Socket Execution invariant...
Architectural boundary verification PASSED: Core crates remain clean, offline, and deterministic.
```
- Core crates (`crates/*`) declare **zero** forbidden dependencies (`openai`, `anthropic`, `reqwest`, `hyper`, `tokio-net`, etc.).
- Core crates contain **zero** sub-process execution (`process::Command`, `libc::exec`, `libc::fork`).
- Core crates contain **zero** network socket execution (`TcpListener`, `TcpStream`).

### 3.2 Presentation Suite Integration Test
Ran `cargo test -p autopsy-cli --test step47_presentation_suite`:
```text
running 3 tests
test toolspec_covers_ui_tui_and_watch_subcommands ... ok
test autopsy_ui_json_output_is_valid_and_actionable ... ok
test autopsy_tools_reports_full_catalog ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

### 3.3 Core CLI Integration & Unit Tests
Ran `cargo test -p autopsy-cli --lib` and `cargo test -p autopsy-cli --tests`:
- Unit tests: 12/12 passed (0 failed).
- Lineage tests (`step31_lineage_cli.rs`): 15/15 passed.
- Extension tests (`step44_cli_extensions.rs`): 31/31 passed.
- Blast radius tests (`step45_blast_radius_cli.rs`): 9/9 passed.
- Find & compact tests (`step46_find_and_compact.rs`): 22/22 passed.
- Presentation suite (`step47_presentation_suite.rs`): 3/3 passed.
- **Total: 92 passed, 0 failed.**

### 3.4 Golden Determinism Verification
Ran 100-run snapshot determinism invariant test:
```text
running 1 test
test tests::test_compute_snapshot_id_golden_100_runs ... ok
test result: ok. 1 passed; 0 failed; finished in 0.00s
```
**100/100 runs produced byte-identical BLAKE3 hashes.**

### 3.5 Installer Self-Test Suite
Ran `bash scripts/tests/test-install.sh`:
```text
installer self-test: 44 passed, 0 failed
```

### 3.6 Linting & Formatting Verification
- `cargo clippy -p autopsy-cli -p synevid-ui --all-targets -- -D warnings` — **0 warnings (clean build)**.
- `cargo fmt --all -- --check` — **0 diffs**.

---

## 4. Summary of Deliverables

| Deliverable | Location | Description |
|---|---|---|
| **Web Dashboard App** | `apps/synevid-ui` | Zero-npm, zero-CDN embedded HTTP dashboard server (`tiny_http`) |
| **Web UI SPA** | `apps/synevid-ui/src/dashboard.html` | Dark-mode interactive UI with canvas multigraph, blast tiers, and trajectory |
| **Terminal TUI** | `crates/autopsy-cli/src/tui.rs` | Interactive terminal UI (`ratatui` + `crossterm`) with 5 tabs and search |
| **Live Watch Mode** | `crates/autopsy-cli/src/watch.rs` | 200ms debounced event watcher (`notify`) with live status ticker cards |
| **CLI Guidance** | `crates/autopsy-cli/src/ui.rs` | `autopsy ui` guidance and configuration launcher |
| **Tool Specification** | `crates/autopsy-cli/src/toolspec.rs` | Registered `ui`, `tui`, and `watch` (26 subcommands total) |
| **Installers** | `scripts/install.sh`, `scripts/install.ps1` | Packages `synevid-ui` alongside `autopsy` and `synevid-lsp` |
| **Test Suite** | `crates/autopsy-cli/tests/step47_presentation_suite.rs` | Automated regression test for the presentation suite |
