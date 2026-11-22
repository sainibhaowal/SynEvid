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
