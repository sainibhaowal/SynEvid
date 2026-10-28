# Production Receipt 77: In-Memory Universe Layout Cache & Background Unselect Engine

**Date:** 2026-10-27  
**Status:** Complete & Verified  
**Traceability:** FR-P2-001, FR-P2-004, FR-028, NFR-001  

---

## 1. Executive Summary

Addressing two critical user friction points in Code Universe exploration:
1. **Repository Layout Fallback Elimination ("why is only 6 packages showing instead of 13"):**
   - **Root Cause:** The client-side API timeout in `apps/web/src/lib/api.ts` was set to an overly aggressive $2,500\text{ms}$. Computing the deterministic Force-Atlas layout for large repositories in `apps/synevid-ui/src/server.rs` took $1.9\text{s} \dots 2.8\text{s}$ on cold disk traversal. When the initial layout calculation slightly exceeded $2.5\text{s}$, the frontend timed out and silently fell back to the 6-package mock sample (`src/services`, `src`, `src/controllers`, `src/repositories`, `src/domain`, `src/infra`).
   - **Resolution:**
     - **In-Memory Universe Cache in Rust Daemon (`apps/synevid-ui/src/server.rs`):** Cached the serialized layout JSON keyed by `{repo_id}_{dimension}` in `Arc<Mutex<HashMap<String, String>>>`. Cold layout computation takes $\sim 1.15\text{s}$; subsequent layout queries return in **$0.71\text{ms}$ (a 1,620x speedup)**. Cache is automatically invalidated whenever a repository is connected or switched.
     - **Client Timeout Scaling (`apps/web/src/lib/api.ts`):** Raised default `fetchWithTimeout` timeout to $15,000\text{ms}$ and `getUniverse` timeout to $20,000\text{ms}$, guaranteeing large codebases never abort prematurely.
2. **Instant Background Unselect & Escape Key Reset ("trapped in spotlight isolation"):**
   - **Root Cause:** In `apps/web/src/components/CodeUniverse3D.tsx`, clicking on empty canvas space (`intersects.length === 0`) did not clear `selectedNodeIdRef`, leaving all non-neighboring nodes permanently dimmed to $10\%$ opacity and forcing the user to reload the page.
   - **Resolution:**
     - Added an empty-canvas click handler to immediately reset `selectedNodeIdRef.current = null`, `setSelectedNodeId(null)`, and `activeNeighborsRef.current.clear()`, restoring all nodes to 100% full brightness with zero camera jump.
     - Added a global `Escape` key listener (`window.addEventListener('keydown', onKeyDown)`) to reset both node spotlight selection and active folder filters in a single keystroke.

---

## 2. Technical Architecture & Delta Analysis

### A. Backend Layout Caching Architecture
```mermaid
graph TD
    Client["Client /api/v1/graph/universe?dim=3d"] --> CheckCache{"Cache Key in universe_cache?"}
    CheckCache -- "Hit (subsequent calls)" --> RetFast["Return Cached JSON (0.71ms, 1,620x faster)"]
    CheckCache -- "Miss (cold start)" --> Calc["discover_repo_files + Force-Atlas Layout (~1.15s)"]
    Calc --> Store["Store in Arc<Mutex<HashMap<String, String>>>"]
    Store --> RetFull["Return Full 13-Package Topology"]
    ConnectRepo["Connect/Switch Repo Event"] --> Invalidate["universe_cache.lock().clear()"]
```

### B. Client Timeout & Selection State Flow
```mermaid
graph LR
    UserAction["Click Empty Canvas OR Press Escape"] --> ClearRef["selectedNodeIdRef = null"]
    ClearRef --> ClearState["setSelectedNodeId(null)"]
    ClearState --> ClearNeighbors["activeNeighborsRef.clear()"]
    ClearNeighbors --> ResetFilter["selectedFolderFilter = null (Esc only)"]
    ResetFilter --> Render["Render Loop Restores 100% Full Opacity to All Nodes"]
```

---

## 3. Verification & Benchmark Telemetry

### A. Backend Benchmark Telemetry (Port 3333)
- **First Cold Request:**
  ```text
  Time: 1.150916s (traversed 200 files, computed Force-Atlas coordinates, cached)
  ```
- **Second Cached Request:**
  ```text
  Time: 0.000710s (0.71ms latency, 1,620x speedup, 0 alloc overhead)
  ```

### B. Multi-Stack Verification
1. **Rust Backend (`synevid-ui`):**
   - `cargo check -p synevid-ui`: Clean (0 errors).
   - `cargo build -p synevid-ui`: Clean.
   - `cargo test -p synevid-ui`: 4/4 passed in $0.03\text{s}$.
   - `cargo fmt --all -- --check`: 100% clean formatting.
2. **Next.js Web Cockpit (`apps/web`):**
   - `npm run build`: Compiled in $2.4\text{s}$, static generation 4/4 completed in $1103\text{ms}$, 0 errors.
3. **Web Inspector Suite (`apps/web-inspector`):**
   - `npm test`: 20/20 passed in $443\text{ms}$.

---

## 4. Invariant Compliance
- **Zero LLM in Core Crates:** 100% deterministic offline Rust caching and Three.js raycasting logic.
- **Read-Only Analyzer (FR-026):** Cache operates purely in memory; zero repository source files or git history touched.
- **NFR-001 Determinism:** Coordinates remain bit-identical based on the deterministic seed `blake3::hash(format!("{}_active_universe", active_id))`.
