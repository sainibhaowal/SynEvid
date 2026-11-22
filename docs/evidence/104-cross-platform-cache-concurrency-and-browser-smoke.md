# Cross-platform cache concurrency and browser smoke follow-up

## Status

**Phase 5 G5 remains PARTIAL.** This evidence records targeted fixes and local
verification; it does not claim that the latest Linux/macOS native artifact
matrix passed.

## CI evidence that triggered this work

- Commit `aa811d176b389c4b05b5e442a9783bb6fe595664`, [GitHub Actions run
  44983679105](https://github.com/sainibhaowal/SynEvid/actions/runs/44983679105),
  completed with failures.
- Windows workspace test `test_e2e_storage_concurrent_atomic_write_resilience`
  failed because concurrent cache publishers shared a PID-only temporary file.
- Linux browser workflow failed during teardown with `ENOTEMPTY` while Next.js
  was still writing its per-test development output.
- macOS Electron browser workflow reached the live-cursor assertion but then
  displayed the Electron load-error page; the test runner forced SwiftShader and
  offscreen rendering on a native macOS runner. These test-only flags are now
  removed; a native rerun is required to validate the change.

## Targeted changes

- Content-addressed cache writes now create per-writer temporary files with
  `create_new`, write bytes before rename, and clean up temporary files on write
  or rename errors. This prevents one thread from renaming or removing another
  thread's in-progress file.
- The concurrent storage integration test verifies object bytes, absence of
  leftover `.tmp-*` files, and closes storage before removing its test directory.
- The Phase 3 browser test now awaits the Next.js process close event before
  deleting its isolated development output and retries transient filesystem
  contention.
- The macOS Phase 1 Electron test no longer forces SwiftShader/offscreen mode;
  hardware acceleration remains disabled for this hidden test window. Production
  canvas and renderer code were not changed.

## Local verification

- `cargo test -p autopsy-tests --test e2e_cross_platform_and_multi_arch
  -- --nocapture` — **4 passed, 0 failed** on Windows after the cache-writer fix.
- `cd apps/web && npm test` — **21 passed, 0 failed** on Windows, including the
  real Electron/daemon replay workflow and browser slider/range API workflow.
- Full pre-commit, pre-push, and exact-revision native Actions verification are
  still required.

## Next proof gate

Push these changes and require green workspace, browser, packaged-artifact,
installer, contents-manifest, SBOM, and pre-merge jobs on Windows, Linux, and
macOS for the exact resulting commit. A failed or missing native gate keeps G5
PARTIAL. Unsigned artifacts remain unsigned; no OS trust or notarization claim
is made.

## Follow-up from the latest native run

Actions run `44988175938` on commit `8ac1052ce6f2bc3252967ab425b710f4bcdcdfe3`
completed with three relevant failures:

- Windows: the packaged Electron install/connect/analyze/receipt smoke passed;
  the Windows loose-object performance job measured 168.37 ms against its
  `<150 ms` gate.
- Linux: packaging, artifact verification, and SBOM generation passed. The
  installed `.deb` smoke selected `/usr/share/doc/synevid`, a documentation
  directory, rather than the executable file.
- macOS: the Electron browser smoke failed after the live event update, with
  renderer load/GPU initialization diagnostics. The macOS packaging smoke did
  not complete on this run.

The Linux smoke now checks `statSync(...).isFile()` before choosing a path and
has a test that rejects the documentation directory. The macOS browser runner
now disables GPU compositing and WebGL only for its hidden CI window and includes
renderer crash/load diagnostics. Neither change modifies production UI or the
2D/3D canvas implementation.

Local verification after these targeted changes:

- `apps/web`: `npm test` — **21 passed, 0 failed**, including the real
  Electron/daemon replay workflow.
- `apps/desktop-electron`: `npm test` — **20 passed, 0 failed**, including the
  Linux executable-selection regression test.
- `apps/desktop-electron`: `npm run build:runtime` — current-source Rust daemon,
  Next.js production cockpit, and MCP bridge assembled successfully.
- Strict loose-object benchmark on the temporary Windows working tree measured
  **2.86 s**, failing `<150 ms`. The exploratory Rust optimization was reverted
  because it did not meet the target; no performance claim is made from it.

The Linux and macOS changes are not verified until a new native Actions run
finishes on the exact commit containing them. G5 remains **PARTIAL** until every
required job is green; no cross-platform production-ready claim is made.

## macOS runner correction (run 44993378733)

Run `44993378733` on `0ba93f5ca0ace120f467e4082bd7b69e6efdc528` passed the
strict 5,000-loose-commit performance test on Windows, Linux, and macOS. The
macOS Electron browser smoke nevertheless failed. Its log showed SwANGLE/Vulkan
`EGL_NOT_INITIALIZED` errors and a browser-expression timeout. Inspection found
that the behavior test's parent launcher still passed
`--use-gl=swiftshader`, `--use-angle=swiftshader`, and
`--enable-unsafe-swiftshader`; changing only the child runner had not removed
those switches. The macOS-only command-line switches have now been removed from
`apps/web/test/phase1_daemon_browser.behavior.test.mjs`. A follow-up native run
on the new exact commit is required. G5 remains **PARTIAL**.

The first local web-suite rerun after that diagnosis also exposed a test
isolation race: all three Next.js browser E2E files were starting dev servers
concurrently, and each could rewrite the shared `apps/web/tsconfig.json`. The
parallel run failed 3/21; its temporary config had lost strict mode and the
`@/*` alias while Next generated per-test types. `apps/web/package.json` now
sets `--test-concurrency=1`; the full serialized suite passed **21/21** on
Windows. This serializes test execution only and does not change application
runtime or rendering behavior.
