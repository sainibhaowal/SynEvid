# Production Evidence 84: Universal Cross-Platform Path Normalization & Dynamic Multi-Repo Switching

> Current-state note (2026-11-01): The concrete repository path examples in this historical receipt are fixtures, not defaults. Current path selection preserves the exact user-selected native path and does not append a folder name or substitute a project-specific path.

## 1. Problem Root Cause Analysis
In hybrid environments (e.g., Windows Electron desktop host communicating with WSL Linux daemon, or native Windows / macOS / Linux setups):
1. **Windows UNC Paths in WSL**: When selecting folders via Windows native dialogs, Windows returns `\\wsl.localhost\Ubuntu\...` or `\\wsl$\Ubuntu\...`. The Linux filesystem rejected these paths as `Path does not exist` because Linux does not recognize UNC network prefixes.
2. **Hardcoded Fallbacks on Failure**: Because the connect endpoint failed with `Path does not exist`, the UI remained stuck on the hardcoded initial fallback ("SynEvid Workspace"), and could not load real-time files.

---

## 2. Solution Implemented

### A. Universal Path Normalization Engine (`apps/synevid-ui/src/server.rs`)
Implemented `DashboardServer::normalize_cross_platform_path(raw: &str) -> PathBuf`:
- **WSL UNC Translation**: Automatically transforms `\\wsl.localhost\<distro>\<path>` and `\\wsl$\<distro>\<path>` into native Linux `/path`.
- **Windows Drive Letters**: Automatically translates `C:\...` / `D:\...` to `/mnt/c/...` / `/mnt/d/...` on Linux/WSL.
- **Tilde Expansion**: Resolves `~/...` against `$HOME` / `$USERPROFILE`.
- **Bidirectional Sanitization**: Removes wrapping quotes and standardizes separators.

### B. Electron IPC Translation (`apps/desktop-electron/src/main.js`)
In `dialog:openDirectory`, Windows UNC paths returned by `dialog.showOpenDialog` are translated to native POSIX paths before reaching the frontend.

### C. Frontend Path Defense & Dynamic Switcher (`apps/web/src/lib/api.ts` & `ConnectRepoModal.tsx`)
- Exported `normalizeCrossPlatformPath(rawPath: string)`.
- Normalizes paths before sending `connectRepo` or `browseFilesystem`.
- Dynamic dropdown in `CockpitHeader.tsx` updates repository title and ID in real time.
- Presets list in directory browser dynamically populates all registered repositories.

---

## 3. Verification & Telemetry
1. Tested connecting an arbitrarily selected native/WSL folder:
   - The daemon preserved the selected repository identity after host-appropriate normalization.
   - The repository ID was derived from the canonical selected path, not a default project name.
   - Switching the active repository succeeded and discovered its real source files.
2. Unit tests:
   - `server::tests::test_cross_platform_path_normalization`: **PASS**
   - Full `apps/web-inspector` suite: **30/30 PASS**
