# Production Evidence Receipt: 90

**Topic:** Electron startup recovery, cockpit actions, and timeline API contract  
**Date:** 2026-10-29  
**Traceability:** FR-P5-001-EXT, INV-SANDBOX-004

---

## 1. Root Cause

The Electron main process retried cockpit readiness 60 times, then attempted to load `http://127.0.0.1:3000/` even when the service was still unavailable. That fail-open path produced the Chromium error page shown in the report. Polling through an async interval also allowed health checks to overlap.

Two repository-drawer actions were also incorrectly wired: **Run Verification Gates** only changed workspaces, while **Export Report** opened the capabilities dialog.

The production cockpit also threw `TypeError: t.find is not a function` during forensic timeline rendering. The daemon returns `{ "sessions": [...] }`, but `listForensicSessions()` had declared and returned the entire response as a bare array.

## 2. Changes

- Replaced overlapping bounded polling with a sequential readiness loop. Electron keeps the splash visible while the cockpit starts and retries after load failures rather than showing Chromium's terminal error page.
- When `.next/BUILD_ID` exists, start Next.js in production mode; otherwise retain the development fallback.
- Let Electron own sidecar startup instead of redundantly prestarting WSL services from the Windows launchers. Clear inherited `ELECTRON_RUN_AS_NODE` before launching the shell.
- Keep `sandbox: true` and the restrictive renderer CSP; remove `unsafe-eval`.
- Keep DevTools enabled and add docked F12 / Ctrl-or-Cmd+Shift+I shortcuts because the shell auto-hides its menu.
- Unwrap and validate the forensic sessions response in the web API client so the timeline receives an array matching its UI state contract.
- Wire the drawer action to open Gates & Policy and refresh the currently available findings. Rename it **Open Gates & Policy** because the daemon's `/api/verify` handler currently returns fixed demonstration findings rather than running real verification gates.
- Wire **Export Report** to download a JSON report containing the selected repository, snapshot, coverage, metrics, and current findings.

## 3. Verification Telemetry

| Check | Result |
| :--- | :--- |
| `apps/desktop-electron` tests | 7/7 passed |
| `apps/mcp-server` typecheck and lint | Passed |
| `apps/mcp-server` tests | 47/47 passed |
| `apps/web-inspector` tests | 37/37 passed |
| `apps/vscode-extension` tests | 6/6 passed |
| `apps/web` production build | Passed, including TypeScript |
| Live Electron runtime | Production cockpit rendered without the prior `.find()` exception; **Trajectory & Jobs** loaded one session and three steps, and **Open Gates & Policy** displayed two findings |
| DevTools shortcuts | F12 opened docked DevTools; Ctrl+Shift+I toggled it closed/open; console showed 0 messages after the API fix |
| Rust workspace all-target tests and doc tests | Passed |
| Determinism check | 100/100 identical digests |
| Architecture-boundary check | Passed |
| Installer self-test | 44 passed, 0 failed |
| Windows Electron production launch | Opened the rebuilt cockpit at `http://127.0.0.1:3000/`; GET `/` returned HTTP 200 |
| Rendered UI inspection | Accessibility tree showed the app, navigation, **Open Gates & Policy**, and **Export Report** controls |

The launch reused an already-running `synevid-ui` daemon and its current repository selection; no repository or source state was changed.

`cargo fmt --all -- --check` reports a formatting difference in the pre-existing user edit in `apps/synevid-ui/src/server.rs`; that unrelated file was left untouched. `cargo clippy` could not run because the installed Rust toolchain does not provide the `clippy` subcommand.

## 4. Remaining Limitation

The Gates & Policy action opens the panel and refreshes the daemon's current response, but it does not execute core verification. The existing `/api/verify` handler is a fixed demonstration response. The UI wording now reflects that limitation; implementing real deterministic gate execution is separate work.
