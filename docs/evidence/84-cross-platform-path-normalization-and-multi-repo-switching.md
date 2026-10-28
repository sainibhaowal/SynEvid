# Production Evidence 84: Universal Cross-Platform Path Normalization & Dynamic Multi-Repo Switching

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
1. Tested connecting `\\wsl.localhost\Ubuntu\home\ravi\Projects Alpha\rea-rea-agents-6.8.0\rea-rea-agents-6.8.0`:
   - Daemon translated to `/home/ravi/Projects Alpha/rea-rea-agents-6.8.0/rea-rea-agents-6.8.0`.
   - Computed BLAKE3 repository ID `6770a2f6c98a9081ca9a61b926cb02048ae70f4ee97fb855b9e6d9becfa98b14`.
   - Successfully switched active repository to `rea-rea-agents-6.8.0`.
   - Discovered all source files and populated real-time file tree.
2. Unit tests:
   - `server::tests::test_cross_platform_path_normalization`: **PASS**
   - Full `apps/web-inspector` suite: **30/30 PASS**
