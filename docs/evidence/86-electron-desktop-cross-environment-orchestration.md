# Production Evidence 86: Standalone Electron Desktop Cross-Environment Orchestration

**Date:** 2026-10-28  
**Status:** VERIFIED & RUNNING  
**Invariants Enforced:** INV-CROSS-001, INV-SANDBOX-004, FR-P5-001  

---

## 1. Problem Root-Cause Analysis

When the user attempted to launch the Electron desktop app from Windows PowerShell:
```powershell
cmd /c "cd /d C:\ && npx electron ""\\wsl.localhost\Ubuntu\home\ravi\Projects Beta\SynEvid\apps\desktop-electron\src\main.js"""
```
Two distinct errors occurred:

1. **Sidecar Daemon Spawn Error (`spawn ... synevid-ui ENOENT`):**
   - The path `\\wsl.localhost\Ubuntu\...\target\debug\synevid-ui` pointed to a Linux ELF executable.
   - Windows Node.js `child_process.spawn()` cannot directly execute a Linux binary without delegating to `wsl.exe`.
2. **Cockpit Load Error (`ERR_CONNECTION_REFUSED` on `http://localhost:3000`):**
   - The Next.js web application was not active on port 3000, causing Electron's `loadURL()` to fail immediately and show a blank connection error.

---

## 2. Solutions Implemented

### 2.1 Cross-Environment WSL Daemon & Web Auto-Orchestration (`apps/desktop-electron/src/main.js`)
- **WSL Path Detection (`toLinuxPath`):** Automatically maps Windows WSL UNC paths (`\\wsl.localhost\Ubuntu\...` or `\\wsl$\Ubuntu\...`) and Windows drive paths (`C:\...`) to POSIX Linux paths.
- **Dual Service Watchdog:**
  - `ensureSidecarDaemon()`: Checks `http://127.0.0.1:3333/api/v1/health`. If offline, spawns the Linux binary via `wsl.exe bash -c "cd '<repo>' && ./target/debug/synevid-ui --port 3333"` on Windows hosts, or directly on Linux/macOS.
  - `ensureWebCockpit()`: Checks `http://localhost:3000`. If offline, spawns Next.js via `wsl.exe bash -c "cd '<repo>/apps/web' && npx next dev -p 3000"` on Windows hosts, or via `npx next dev` on Linux/macOS.
- **Immediate Startup Splash Screen:** Displays an inline branded dark splash screen (`#090A0F`) with cyan glowing spinner and status indicator ("Initializing SynEvid Standalone Shell... Connecting Reactive Daemon & Cockpit") while services boot.
- **Dynamic Connection Polling:** Seamlessly transitions from splash screen to `http://localhost:3000` as soon as the service responds.
- **Clean Teardown:** Automatically kills any child processes spawned by Electron when the desktop window is closed (`app.on('window-all-closed')`).

### 2.2 Dedicated Launchers
- **Batch Launcher (`scripts/run-desktop.bat`):** Changes directory to `%USERPROFILE%` to prevent CMD UNC path warnings and invokes `npx --yes electron "\\wsl.localhost\Ubuntu\home\ravi\Projects Beta\SynEvid\apps\desktop-electron\src\main.js"`.
- **PowerShell Launcher (`scripts/run-desktop.ps1`):** Switches to `$env:USERPROFILE` and launches the Electron application cleanly.
- **WSL Launcher (`scripts/run-desktop.sh`):** Detects WSL environments and automatically delegates to the Windows desktop host.

---

## 3. Verification & Live Confirmation

1. **Electron Unit Tests:**
   ```bash
   cd apps/desktop-electron && npm test
   ```
   *Result:* 3/3 passed.
2. **Process Verification:**
   Windows `Get-Process electron` confirmed active Windows processes for PID 30680, 38148, 41892.
3. **Service Health:**
   - Daemon `http://127.0.0.1:3333/api/v1/health` $\to$ `200 OK`
   - Cockpit `http://localhost:3000` $\to$ `200 OK`
