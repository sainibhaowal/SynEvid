# Evidence 101 — Cross-platform CI runtime gates

Date: 2026-11-03

## Scope

The first `v0.6.1` GitHub Actions run showed workspace-test failures on all hosted operating systems and a Linux packaged-smoke failure, while the same functionality passed locally. The hosted failures were runner-profile/desktop-launch conditions, not product assertion failures.

The CI gate now:

- runs the complete workspace test matrix with `cargo test --workspace --all-targets --release -- --nocapture --test-threads=1`, matching the production execution profile used for latency acceptance;
- starts only the disposable Linux packaged-smoke child with `--no-sandbox --disable-gpu` under Xvfb, avoiding hosted Linux sandbox/GPU restrictions;
- leaves the installed application's normal Electron launch flags unchanged.

## Local verification

- Optimized workspace unit/integration gate — PASS, including Phase 1 HTTP/incremental tests, Phase 3 packed history and historical-frame tests, Phase 4 WebSocket/replay tests, and Electron-facing Rust UI tests.
- Phase 1 optimized timing samples: overlay p95 16.332 ms; filesystem browse p95 2.344–3.415 ms; daemon simulation p95 12.321–13.091 ms; SSE wake-up p95 0.521–0.671 ms.
- Phase 3 optimized samples: packed 5,000-commit galaxy 90.60 ms; first historical frame 16.66 ms.

## UI safety

The changes are restricted to CI configuration and the disposable smoke-test launcher. `apps/web/src/components/CodeUniverse3D.tsx` remains untouched and uncommitted; no canvas renderer, animation, camera/orbit, WebGL, or styling code changed.
