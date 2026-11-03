# Evidence 100 — Cross-platform release-gate follow-up

Date: 2026-11-03

## Scope

This patch addresses two failures reported by the first public GitHub Actions run for the `v0.6.0` release commit:

1. Cargo Deny rejected the transitive `paste` crate (`RUSTSEC-2024-0436`) through `ratatui 0.29.0`. The workspace now uses `ratatui 0.30.2`, which removes that dependency path.
2. Electron's POSIX runtime packager copied the Rust daemon without preserving its executable mode. The packager now applies mode `0755` to the Linux/macOS sidecar after copying it.

## Local verification

- `cargo test --workspace --all-targets -- --nocapture --test-threads=1` — PASS.
- `cargo test --workspace --doc` — PASS.
- `cargo clippy --workspace --all-targets -- -D warnings` — PASS.
- `cargo-deny 0.20.2 check --hide-inclusion-graph` — PASS for advisories, bans, licenses, and sources. It emits only existing non-failing warnings for unmatched license allowances and duplicate transitive versions.

The existing Windows packaged smoke and artifact gates remain green. The existing `v0.6.0` tag is not moved; this correction is released as the next patch tag after the follow-up commit.

## UI safety

`apps/web/src/components/CodeUniverse3D.tsx` was intentionally excluded from the commit. No 2D/3D canvas renderer, animation, camera/orbit, WebGL, or styling code was changed.
