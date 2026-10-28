# Production Evidence 66: Removal of Fallback `dashboard.html` & Pure Headless Daemon Architecture

## Context & Problem Statement
Previously, `apps/synevid-ui` and `crates/autopsy-cli` retained an embedded monolithic fallback file `dashboard.html` (1,457 lines of static HTML/JS) baked into Rust binary memory via `include_str!("dashboard.html")`. This created confusion and redundant maintenance:
1. **Redundant Fallback:** SynEvid's canonical user experience is served via the Next.js/Electron visual cockpit (`apps/web`), the TUI, CLI, and MCP servers. A static embedded HTML dashboard is obsolete.
2. **Binary Bloat:** Embedding 1,457 lines of static CSS/JS into the core CLI binary was unnecessary for a headless reactive engine.
3. **Generic Repository Display Label:** When launched with `.` as `repo_root`, `file_name()` returned `None`, falling back to `"repository"`, resulting in the header dropdown showing `repository (fa550c78)` rather than the real project directory name (`SynEvid (fa550c78)`).

---

## Architectural Changes & Solution

### 1. Complete Removal of `dashboard.html`
- Deleted [`apps/synevid-ui/src/dashboard.html`](../../apps/synevid-ui/src/dashboard.html).
- Deleted [`crates/autopsy-cli/src/dashboard.html`](../../crates/autopsy-cli/src/dashboard.html).
- Removed all `include_str!("dashboard.html")` references across both crates.

### 2. Pure Headless Daemon Status Endpoint (`GET /`)
- In both [`apps/synevid-ui/src/server.rs`](../../apps/synevid-ui/src/server.rs) and [`crates/autopsy-cli/src/ui.rs`](../../crates/autopsy-cli/src/ui.rs), the root HTTP route (`GET /` and `GET /index.html`) now returns a structured headless JSON status gateway:
  ```json
  {
    "daemon": "synevid-ui",
    "status": "online",
    "version": "v0.2.6",
    "engine": "offline-deterministic",
    "repo_root": "/home/ravi/Projects Beta/SynEvid",
    "active_repo_id": "fa550c78e87a4d21da9694d35b1472d823d4a2dae1406185c3c30eb2339412a8",
    "cockpit_url": "http://localhost:3000",
    "mcp_bridge": "active",
    "api_base": "/api/v1"
  }
  ```

### 3. Canonical Repository Name Derivation
- Canonicalized `repo_root` prior to extracting `file_name()`, resolving `.` to the real workspace basename:
  ```rust
  let display_name = root_canon
      .file_name()
      .and_then(|n| n.to_str())
      .filter(|n| !n.is_empty() && *n != ".")
      .unwrap_or("SynEvid")
      .to_string();
  ```
- In [`CockpitHeader.tsx`](../../apps/web/src/components/CockpitHeader.tsx), added resilient fallback logic to derive the project folder name from `r.root_path` if an existing SQLite record contains `"repository"`.
- Verified via live endpoint: `GET /api/v1/repos/list` now returns `display_name: "SynEvid"` and the dropdown renders `SynEvid (fa550c78)`.

---

## Verification Telemetry

1. **Rust Workspace Check & Compilation:**
   ```bash
   cargo check -p autopsy-cli -p synevid-ui
   cargo build -p synevid-ui
   # Result: 0 warnings, clean compilation
   ```
2. **Next.js Production Build:**
   ```bash
   cd apps/web && npm run build
   # Result: 0 errors
   ```
3. **Live Daemon Verification:**
   ```bash
   curl -s http://127.0.0.1:3333/
   # Returns: Clean JSON headless daemon response, zero HTML
   curl -s http://127.0.0.1:3333/api/v1/repos/list
   # Returns: [{"repo_id":"fa550c78...","display_name":"SynEvid",...}]
   ```
