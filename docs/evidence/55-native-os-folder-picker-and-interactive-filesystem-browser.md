# Production Receipt 55: Native OS Folder Picker & Interactive Filesystem Browser

## Executive Summary
Resolves the repository onboarding deficiency where users were required to manually enter file paths without an OS directory picker or explorer dialog. Introduces a comprehensive, dual-mode filesystem onboarding architecture:
1. **Native OS Directory Picker**: Integrates the standard HTML5 File System Access API (`window.showDirectoryPicker()`) with fallback to directory input (`webkitdirectory`), opening the native Windows Explorer / macOS Finder / Linux file chooser.
2. **Deterministic Host Filesystem Navigator**: Implements `/api/v1/fs/browse` in `synevid-ui` and `autopsy-cli`, enabling interactive, low-latency folder traversal with parent navigation, breadcrumb path display, and real-time Git repository detection.
3. **Live Repository File Discovery**: Dynamically resolves real codebase files upon connection, eliminating hardcoded dummy nodes and rendering authentic project source files inside the 3D Universe and File Explorer.
4. **Squircle Design Integration**: Aligns the onboarding modal with the SynEvid Rounded Square (`rounded-2xl`, `rounded-xl`, `rounded-lg`) design system in both Light and Dark modes.

---

## Architecture & Implementation Details

### 1. Host Filesystem REST Endpoint (`/api/v1/fs/browse`)
- **Location**: [`apps/synevid-ui/src/server.rs`](file:///home/ravi/Projects%20Beta/SynEvid/apps/synevid-ui/src/server.rs), [`crates/autopsy-cli/src/ui.rs`](file:///home/ravi/Projects%20Beta/SynEvid/crates/autopsy-cli/src/ui.rs)
- **Behavior**:
  - Accepts `?path=<target>` query parameter with URL decoding (`%20`, `%2F`, `+`).
  - Expands `~` to user home directory (`HOME` / `USERPROFILE`).
  - Safely canonicalizes directory paths.
  - Returns:
    - `current_path`: String
    - `parent_path`: String | null
    - `is_git_repo`: Boolean (checks for `.git`)
    - `directories`: Alphabetically sorted list of subdirectories (`NFR-001` determinism), filtering out internal caches (`node_modules`, `target`, `.next`, `.git`).
    - `quick_presets`: Workspace root, User Home, Root filesystem.

### 2. Client API & Types
- **Location**: [`apps/web/src/lib/api.ts`](file:///home/ravi/Projects%20Beta/SynEvid/apps/web/src/lib/api.ts)
- **Interface**:
  ```typescript
  export interface FsDirectoryEntry {
    name: string;
    path: string;
    is_dir: boolean;
    is_git_repo: boolean;
  }

  export interface FsBrowseResponse {
    current_path: string;
    parent_path: string | null;
    is_git_repo: boolean;
    directories: FsDirectoryEntry[];
    quick_presets: FsPreset[];
  }
  ```

### 3. Native & Visual Directory Picker UI
- **Location**: [`apps/web/src/components/ConnectRepoModal.tsx`](file:///home/ravi/Projects%20Beta/SynEvid/apps/web/src/components/ConnectRepoModal.tsx)
- **Components**:
  - **Open OS Explorer Picker Button**: Calls `window.showDirectoryPicker()` to launch the operating system's native directory chooser dialog, with automatic fallback to `<input type="file" webkitdirectory />`.
  - **Quick Shortcuts**: One-click pills for Workspace root, Home, and Root directories.
  - **Interactive Directory Tree**: In-modal visual folder browser with parent (`..`) drill-up, subfolder listing, and `[GIT REPO]` status badge.
  - **Squircle Styling**: `rounded-2xl` modal container, `rounded-xl` inputs/buttons, `rounded-lg` preset badges.

---

## Verification & Telemetry

### Unit Tests
```
running 4 tests
test server::tests::test_phase1_sub_50ms_simulate_edit ... ok
test server::tests::test_phase1_capabilities_reporting ... ok
test server::tests::test_phase1_fs_browse_navigation ... ok
test server::tests::test_phase1_ego_network_line_to_line_edges ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```

### Next.js Production Build
```
✓ Compiled successfully in 8.3s
✓ Running TypeScript passed (0 errors)
✓ Generating static pages using 5 workers (4/4) in 2.1s
```

### Live Endpoint Telemetry
- `GET /api/v1/fs/browse`: Verified with `HTTP 200 OK`, returning directory entries for `/home/ravi/Projects Beta/SynEvid`.
- `POST /api/v1/repos/connect`: Verified returning `{"status":"connected","repo_id":"fa550c78...","root_path":"/home/ravi/Projects Beta/SynEvid"}`.
- `GET /api/v1/graph/universe`: Verified discovering real repo files (`AGENTS.md`, `CHANGELOG.md`, `Cargo.toml`, `tests/src/lib.rs`, etc.) and computing deterministic 3D/2D layouts.
