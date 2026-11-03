# Phase 5 Electron Packaging, Analytics, and Runtime Closure

Date: 2026-11-01  
Release line: `v0.6.0`

## Scope and canvas safety

This closure covers the Electron shell, packaged runtime resources, daemon security/watchdog, analytics reports, workspace presets, and release target configuration. It does not modify `apps/web/src/components/CodeUniverse3D.tsx`, the 2D/3D renderer, animation loop, camera behavior, WebGL implementation, or canvas styling. The new analytics and preset UI is additive and lives outside the canvas component.

## Implemented runtime

- `npm run dev` runs Electron from source with the current Rust daemon through Cargo and the current Next.js dev server; no electron-builder build is required for source iteration.
- `npm run build:runtime` builds the optimized Rust daemon, Next.js standalone server, and MCP bundle, then assembles them under `build/runtime`.
- electron-builder copies `cockpit`, `deps`, `sidecars`, and `mcp-server` into the packaged resources. Dependencies are kept in `deps` because electron-builder prunes arbitrary `node_modules` folders from extra resources; the packaged Next server receives that directory through `NODE_PATH`.
- `npm run dist:dir` produced `dist/win-unpacked` with `SynEvid.exe`, `resources/cockpit/server.js`, `resources/deps/next`, `resources/sidecars/synevid-ui.exe`, and `resources/mcp-server/index.bundle.js`.
- `npm run dist:win` produced distinct Windows artifacts: `SynEvid-0.6.0-win-x64-setup.exe` (NSIS), `SynEvid-0.6.0-win-x64-portable.exe`, and `SynEvid-0.6.0-win-x64.msi`.
- The packaged smoke launcher uses isolated ports and user data, starts the real packaged executable, waits for both services, and reported `PACKAGED_SMOKE_PASS`.

## Security and recovery

- Electron creates an ephemeral 256-bit loopback token per process and exposes it only through the synchronous preload bridge needed to attach request headers.
- The Rust daemon accepts only the matching Bearer token or token query parameter when launched by Electron and returns `ERR-AUTH-401` for unauthenticated requests.
- The main process checks the authenticated daemon health endpoint, restarts an exited sidecar automatically, and injects an additive `ERR-DAEMON-301` recovery banner while retrying.
- Production CSP excludes `unsafe-eval`; source development enables the narrowly scoped React debugging exception needed by Next development mode.

## Analytics and reports

The daemon report is built from persisted SQLite graph rows, Git history, and AI ledgers. The canonical JSON report is BLAKE3-digested before presentation formatting. JSON, Markdown, HTML, and SARIF envelopes share the same digest and contain no wall-clock fields. The web Analytics & Reports center queries the real endpoint and downloads the returned content; it has no fabricated offline metrics.

## Verification executed

- Electron package tests: 12/12 passed.
- Web contract and browser behavior tests: 19/19 passed; `npx tsc --noEmit` passed. Browser E2E uses an isolated per-port Next dist directory so it can run while the live source runtime is open.
- MCP typecheck, lint, bundle, and integration suite: 49/49 passed.
- Rust analytics determinism test: passed in both `synevid-ui` library and binary test targets.
- Rust package build: `cargo build --release -p synevid-ui` passed.
- Next production build with `output: standalone`: passed.
- MCP bundle build: passed.
- Electron unpacked Windows packaging: passed.
- Windows NSIS, portable, and MSI packaging: passed with the distinct artifacts listed above.
- Packaged Electron smoke: daemon and cockpit health both passed; `PACKAGED_SMOKE_PASS`.
- Web TypeScript check and Next build: passed during runtime assembly.

Platform signing/notarization and publishing still require release credentials and are intentionally CI-owned. The post-merge workflow now builds the Electron target matrix on native Windows, macOS, and Linux runners (NSIS/portable/MSI, DMG/ZIP, and AppImage/DEB/RPM) and includes the packaged Windows smoke test; local verification of non-Windows artifacts remains unavailable on this host.
