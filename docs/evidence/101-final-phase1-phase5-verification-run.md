# Final Phase 1–5 Verification Run

**Date:** 2026-11-03  
**Host:** Windows native development environment  
**Scope:** Final cross-stack verification after the Phase 1/3/4/5 targeted fixes.

## Result

The implemented functional paths are wired end to end across the Rust daemon,
storage, CLI/TUI contracts, MCP server, web cockpit, and source Electron
runtime. The 2D/3D canvas renderer, animation loop, camera/orbit behavior,
WebGL implementation, and canvas styling were not changed by this run.

## Passing gates

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace --all-targets -- --nocapture --test-threads=1`
- `cargo test --workspace --doc`
- Phase 1 HTTP corruption/recovery, randomized edit determinism, WebSocket,
  SSE replay, filesystem browse, and live-event latency tests
- Phase 3 real packed repository test covering delta objects, packed refs,
  tags, merge parents, dirty-tree status, mtime-preserving snapshots, and
  guarded mutation recovery
- Latest targeted rerun: Git engine 7/7, real packed-repository integration
  1/1 with first historical frame 7.13 ms, Phase 1 HTTP 2/2, and Phase 4 HTTP
  2/2 with live WebSocket p95 2.81 ms
- Phase 4 real edit/cache loop, MCP-to-daemon persistence, connected
  WebSocket delivery, and restart replay
- Electron tests 12/12, MCP tests 49/49, web tests 19/19, web-inspector
  tests 38/38, and VS Code extension tests 6/6
- Windows packaged smoke test and NSIS, portable, and MSI artifact builds
- Fresh Phase 5 runtime assembly (`npm run build:runtime`), unpacked packaging
  (`npm run dist:dir`), Windows target packaging (`npm run dist:win`), and a
  fresh packaged smoke run all passed; smoke reported both `daemon: true` and
  `cockpit: true` before `PACKAGED_SMOKE_PASS`
- Git-for-Windows Bash determinism gate: 100/100 byte-identical runs
- Git-for-Windows Bash architecture gate: passed against production sources;
  test fixtures are excluded from the subprocess scan
- Windows installer `install.ps1 -Help` and read-only `install.ps1 -Check`:
  passed
- Phase 3 release packed-history measurements: about 140–161 ms in the latest
  runs; latest targeted real-repository historical frame: 7.13 ms
- Phase 4 live WebSocket delivery: 8-sample p95 3.90 ms; the same run delivered
  `mcp.live_activity` below the 25 ms AI-overlay budget
- Legacy `autopsy ui` and TUI truthfulness checks: no fabricated repository
  branch/snapshot/language records, no sample graph/ego/live events, and the
  simulator now runs real overlay analysis or returns `UNKNOWN` with a
  structured error. The selected CLI/TUI UI regression set passes 10/10
  (including 3 new legacy-UI checks); the workspace gate recorded above remains
  the authoritative cross-stack receipt.
- Latest CLI/TUI rerun passed 23/23 unit tests plus integration targets:
  lineage 15/15, extensions 31/31, blast-radius 9/9, compact/find 22/22,
  and UI/tooling 4/4.

The optimized serial daemon/UI timing run passed 23/23: 2,000-file edit
handler p95 14.076 ms, 1,000-directory browse p95 2.863 ms, and live-event
wake-up p95 1.096 ms.

The latency assertions are run with `--test-threads=1`. The default parallel
debug harness is host-scheduling sensitive: two Phase 1 latency assertions can
measure thread starvation while unrelated repository tests run concurrently.
That does not change the serial measurements or the underlying event/repository
behavior, but it means an unqualified parallel `synevid-ui` test command should
not be used as the performance receipt on this host.

## Explicit status limits

- Phase 3 functional behavior and first historical-frame timing are verified.
  The indexed packed 5,000-commit path measures about 140–161 ms and is not
  stable below the literal `<150 ms` budget; the same loose-object cold-start
  fixture measures about 2.5–3.4 s. This remains the open Phase 3 performance gap.
- Phase 4 functional live delivery and its measured wire-level budgets are
  verified; the MCP package tests cover the shared broadcast route used by the
  UI activity signal.
- Phase 2’s separately measured 5,000-node canvas FPS and native theme
  presentation targets remain below their PRD thresholds; no renderer change
  was made to alter those measurements.
- macOS/Linux artifacts and signing/notarization are configured for native CI
  runners. The post-merge workflow now builds those Electron targets natively,
  but those runners were not executed from this Windows host.
- `verify-determinism.sh`, `verify-arch-boundaries.sh`, and
  `tests/tests/test-install.sh` ran through Git-for-Windows Bash. The first two
  gates passed; the installer self-test correctly skipped its Linux/macOS-only
  checks on this Windows host. The Python benchmark harness also executed as
  part of the workspace suite.

This receipt records what passed and what remains unproven; it does not replace
those open performance or non-Windows release gates with a source-text claim.

Follow-up Phase 3 measurement: physical-offset packed prefetch produced
isolated release runs of 145.03 ms, 136.89 ms, and 147.40 ms, with a later
normal-scheduling run at 172.27 ms. The literal `<150 ms` history gate remains
unproven; no canvas implementation was changed.

Follow-up: the packed reader now uses the known indexed pack offset directly.
The strict `<150 ms` acceptance test passed at 63.42 ms, 78.39 ms, and 88.94
ms across three isolated release runs. The loose-object cold-start path remains
above the PRD budget.

## Refreshed Windows package receipt (2026-11-03)

After the indexed packed-reader change, `npm run dist:win` completed and
produced all requested Windows targets: NSIS setup, portable executable, MSI,
and unpacked application directory. `SYNEVID_SMOKE=1 npm run smoke:packaged`
passed with both the packaged daemon and standalone Next.js cockpit detected.
The artifacts were unsigned because no Windows signing certificate is
configured on this host. Smoke mode now avoids racing the health check with
the normal splash-to-cockpit navigation teardown. The refreshed run completed
without that warning and reported `PACKAGED_SMOKE_PASS` with both components
healthy.

## Current-state Rust gate receipt (2026-11-03)

On the restored production tree, `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace --doc`, and
`cargo test --workspace --all-targets -- --nocapture --test-threads=1` all
passed. The full workspace run included Phase 1 HTTP and incremental tests,
the real packed Git repository flow, Phase 4 cache/replay and WebSocket tests,
WASM tests, and both `synevid-ui` targets. The latest receipts included a
10.84 ms first packed historical frame, a 2.71 ms Phase 4 WebSocket p95, and
the serial Phase 1 latency profiles within their configured budgets.

The strict release packed-history acceptance test passed at 82.74 ms. The
separate loose-object diagnostic remains above the PRD budget at approximately
2.5â€“3.4 seconds on this Windows filesystem; its temporary profiling hooks
were removed and are not part of the production tree.

The post-merge release workflow now runs the packaged health smoke test on
Windows, macOS, and Linux. The smoke launcher resolves the native Windows
executable, macOS `.app` executable, and Linux unpacked executable; Windows
execution is verified locally, while the macOS/Linux runs remain CI-host
evidence.

The release job also runs `npm run verify:artifacts` before smoke testing. The
Windows run passed with NSIS, portable, and MSI targets present; the verifier
requires DMG/ZIP on macOS and AppImage/DEB/RPM on Linux.

The Windows release was rebuilt after adding the verifier. The refreshed
`npm run verify:artifacts` result was `ELECTRON_ARTIFACTS_PASS`, followed by a
clean `PACKAGED_SMOKE_PASS` with both the packaged daemon and cockpit healthy.

The local Windows cross-build also exposed two Linux-release prerequisites:
Electron Builder needs Debian/RPM metadata (`homepage`, maintainer email,
and synchronized desktop naming), and the Linux runner needs `fpm` for DEB/RPM
targets. Those metadata fields are now present and the post-merge Linux job
installs `fpm`; AppImage remains a native-Linux build because Windows cannot
execute its `mksquashfs` helper.
