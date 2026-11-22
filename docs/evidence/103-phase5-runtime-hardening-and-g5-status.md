# Phase 5 runtime hardening and G5 status

## Current rerun update (2026-11-22; supersedes the historical run notes below)

**Current status: PARTIAL.** Windows source build, installer artifacts, artifact/SBOM validation, and the NSIS plus portable install/launch smoke passed locally. Linux/macOS native jobs and the exact pushed-revision GitHub Actions matrix are still required; therefore Phase 5 is not yet production-ready across the declared platforms.

- Windows current-source runtime build passed: optimized Rust daemon/CLI, Next.js 16.4.6 standalone cockpit, and bundled MCP bridge.
- Exact-SHA native matrix attempt: commit `234ce738c185fe5b91d1064fbd39b12482d8dc96`, [Actions run 44981588658](https://github.com/sainibhaowal/SynEvid/actions/runs/44981588658). Windows and macOS packaged build/artifact/install/connect/analyze/receipt/shutdown jobs passed; Windows, Linux, and macOS Rust workspace suites passed. Linux packaged build, browser workflow, artifact verification, and SBOM generation passed, but its final `.deb` install smoke failed because `dpkg -i` did not resolve the package-declared `libnotify4` and `libsecret-1-0` dependencies.
- Linux smoke remediation: install the local `.deb` through `apt-get install` so dependencies are resolved using the same package-manager path as a user installation, then remove it through `apt-get`. This remediation awaits a new same-SHA-platform CI run and is not yet verified.
- Windows NSIS, portable, and MSI artifacts built unsigned. Artifact verification passed for all three. Three Windows CycloneDX npm SBOMs parsed and passed validation. MSI was built and verified but was not installed by the local smoke.
- Windows `npm run smoke:installers` passed NSIS install, real repository indexing, MCP impact, visible UI/daemon BLAKE3 receipt parity, invalid/missing-token 401 checks, clean exit, asynchronous uninstall, and portable self-extraction/analysis/receipt/exit.
- Latest Zod-patched portable smoke receipt: `blake3:7b3ace2480f8dd1f15bfe10638973ea86fdf18116efdf402b7f74f03496e48da`; 1 file scanned; 1,595 runtime files verified; MCP trajectory persisted; UI receipt matched; missing/invalid token status 401. This is disposable fixture evidence, not a release benchmark.
- Verification: Electron **19/19**, MCP **51/51** after lockfile install and Zod update, web **21/21**, Next production build passed. Production dependency audits passed with zero vulnerabilities for MCP, web, and desktop. Rust fmt, focused clippy, and all `synevid-ui` targets passed before the smoke-only harness edits; the packaged daemon was rebuilt from current source.
- Corrected MCP lock metadata: `concat-map` resolves to the locked tarball version `0.0.1`, not `0.1.0`. Updated Zod to the audited patched version; `npm audit --omit=dev` now reports zero MCP vulnerabilities.
- Linux `.deb` smoke rerun, Rust daemon SBOM, aggregate checksums/provenance, and all-green same-SHA CI remain unverified. Do not mark G5 complete until those gates pass.
- Canvas renderer, camera, animation, and styling were not changed. Windows artifacts remain unsigned; no OS-signing/notarization claim is made.

**Date:** 2026-11-21
**Status:** PARTIAL — implementation updates and desktop unit tests pass; G5 is not yet verified.

## Changes in this receipt

- Electron IPC now checks that the calling frame is on the exact configured loopback cockpit origin before opening a folder picker, returning daemon status/token data, reporting frame metrics, or controlling the window.
- Renderer navigation to other origins and renderer-created windows/webviews are denied. The renderer retains the existing isolated preload contract; no raw Electron API is exposed.
- Daemon and cockpit child processes are stopped on application quit. macOS keeps the runtime alive when its last window closes, matching its dock/tray lifecycle; quitting the app shuts the children down.
- Runtime assembly copies the public JSON schemas into packaged resources beside the daemon, cockpit, dependencies, and MCP bundle. SQLite migration definitions remain compiled into the Rust storage/daemon code; there is no separate migration-file directory to copy.
- Runtime assembly now includes both native executables (`synevid-ui` and the `autopsy` CLI), and its manifest requires and hashes both. The daemon passes MCP runtime paths and loopback configuration through its environment; generated AI-client config can launch the bundled MCP bridge and points at a user-data token file rather than embedding the bearer token. The MCP client reads that credential for each daemon request and validates an explicitly configured daemon URL as loopback-only.
- Runtime assembly writes a sorted SHA-256 manifest for staged files. Packaged smoke verifies required resource presence, canonical in-root paths, file sizes, and hashes before exercising the daemon. Unit tests cover tamper, omission, and traversal failures; installed native verification is still pending.
- Updated the Electron dependency from the unsupported 33 line to 45.4.1. The Electron executable starts in version-probe mode successfully; this is not a substitute for launching the packaged app or validating canvas behavior.
- Updated the documented and enforced developer/build Node.js floor to 22.12.0, matching Electron 45's package requirement.
- Packaged smoke now requires a disposable real repository. It verifies missing and invalid tokens both return HTTP 401, connects and indexes a TypeScript fixture using the process token, requests the persisted analytics report, and checks that repository ID, snapshot ID, and report/evidence BLAKE3 digests are present and consistent.
- Packaged MCP launch configuration now receives the active daemon endpoint, bundled CLI/bridge locations, and a file-based token handoff. This new cross-process path still requires native packaged proof.
- The smoke launcher uses an isolated Electron profile and removes its temporary profile and repository after the child exits.
- Native CI and post-merge release jobs now invoke `smoke:installers`: NSIS install/uninstall and portable launch on Windows; DMG copy and ZIP extraction launches on macOS; DEB install/removal and AppImage launch on Linux. Each uses the authenticated repository/index/report smoke. These are configured tests, not passing evidence for this revision.

## Verification performed

- `apps/desktop-electron`: `npm test` — **19 passed, 0 failed**.
- `apps/mcp-server`: `npm test` (including TypeScript build and bundle build) — **51 passed, 0 failed**.
- `apps/web`: `npm run build` — passed on Next.js **16.4.6**; the route is dynamic and the proxy middleware is emitted.
- `apps/web`: `node --test test/phase5_csp.behavior.test.mjs` — **1 passed**; the production response CSP nonce matched every rendered script.
- `apps/web`: full `npm test` — **20 passed, 1 failed**. The remaining failure is the Phase 1 real Electron browser repository/replay workflow. Browser console evidence says the daemon preflight response does not allow `x-synevid-client-type`. File timestamps confirm the test used a stale daemon: `target/debug/synevid-ui.exe` is from 2026-11-06, while `apps/synevid-ui/src/server.rs` is from 2026-11-20; the current source's OPTIONS and JSON response builders both include the header. The current source has not yet been behaviorally verified by this test because the daemon could not be rebuilt on this host. It is not represented as a pass.
- `$env:PATH = "C:\Users\Ravin\.cargo\bin;$env:PATH"; cargo fmt --all -- --check` — passed.
- `cargo test -p synevid-ui --all-targets` — **not run to completion**: the host has the Windows SDK and Rust's `rust-lld`, but lacks the MSVC C runtime import library (`msvcrt.lib`); linking stopped before crate tests.
- `cargo fmt --all -- --check`, `git diff --check`, and `node --check apps/web/test/phase1_daemon_browser_runner.cjs` — passed on the final working tree.
- No canvas renderer, camera, animation, WebGL, or canvas styling file was edited.

## Evidence not available in this run

- The packaged build and expanded packaged smoke were not executed. Cargo/rustfmt are available, but the required Windows MSVC linker is not, so the Rust daemon sidecar could not be rebuilt for this revision.
- GitHub Actions could not be started from this session: local `gh` is unauthenticated, and the connected GitHub integration rejected Git write access with HTTP 403. CI workflow definitions are not treated as passing evidence.
- The requirement-registry validator was not run; Python is unavailable in this Windows shell, so registry validation is not claimed.
- The native workflows have been changed to smoke installed release artifacts, but these workflow edits have not run yet. Runtime/install behavior remains unverified until native Windows/macOS/Linux jobs pass.
- Packaged smoke now loads the visible cockpit, requests the JSON analytics report through the UI, and compares its repository ID, snapshot ID, and digest with the authenticated daemon response. The new UI-level assertion is implemented but awaits a native packaged run.
- Electron, cockpit, MCP, and Rust daemon CycloneDX SBOM generation and validation are now configured. Per-artifact checksums and keyless signing are produced by the aggregate release workflow, but this updated pipeline has not run and cannot yet count as evidence.
- The manifest is verified during packaged smoke; native Windows/macOS/Linux package runs are still required to prove it matches each installed artifact.
- Next.js now creates a per-request nonce CSP for rendered scripts; the focused HTTP behavior test proves all rendered scripts carry the response nonce. Electron-native CSP console validation is still pending.
- Packaged MCP configuration and token discovery are wired through a private user-data token file, but no installed-app-to-MCP-client end-to-end run has passed yet.
- Electron 45 changes the bundled Chromium renderer. The canvas source files were untouched, but visual/performance regression evidence for this major runtime upgrade is pending on native runners.

## Gate conclusion

Phase 5 is **not 100% complete** on the evidence available here. The runtime security changes and expanded smoke implementation are in place, and the desktop unit suite passes. G5 remains **PARTIAL** until native packaged runs pass on Windows, macOS, and Linux; installer-based launch/connect/analyze/shutdown tests pass for the required artifact formats; the visible UI receipt is asserted; and artifact contents, checksums, and SBOMs are verified from the generated release bundle.
