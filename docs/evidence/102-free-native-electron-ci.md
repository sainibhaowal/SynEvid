# Native Electron CI and Free Artifact Trust

**Date:** 2026-11-03  
**Scope:** Native macOS/Linux packaging and packaged smoke coverage without paid signing credentials.

## Implemented

- Pull-request CI now builds the unpacked Electron runtime and runs the packaged
  daemon/cockpit health smoke test on native Windows, macOS, and Linux runners.
- Post-merge CI builds the declared native targets: Windows NSIS/portable/MSI,
  macOS DMG/ZIP, and Linux AppImage/DEB/RPM.
- Linux smoke runs under `xvfb-run`, so the Electron health check has a display
  in headless CI. Linux packaging installs `fakeroot`, `fpm`, and RPM tooling.
- `verify:artifacts` now rejects both missing and zero-byte release artifacts.
- CI has a required Ubuntu release-mode gate that preserves all 5,000 loose
  Git commit objects and measures the literal `<150 ms` history budget. The
  normal Windows workspace test remains packed because NTFS loose-object
  filesystem latency is not representative of the native Linux acceptance
  runner.
- Electron-builder signing discovery remains disabled. No certificate, Apple
  Developer account, or paid service is required for the build or smoke gates.
- Release bundles retain SHA-256 checksums, a keyless Sigstore signature for the
  checksum manifest, SBOM output, and build provenance. These provide free,
  independently verifiable artifact integrity; they do not make the installers
  trusted by Windows SmartScreen or macOS Gatekeeper.

## Verification boundary

The Windows packaged artifacts and smoke test were run locally. The macOS and
Linux jobs must execute in GitHub Actions' native runners to produce the final
platform evidence; this Windows host cannot honestly substitute for those
native runtime checks.

The existing 2D/3D canvas renderer, animation, camera/orbit behavior, WebGL
implementation, and canvas styling were not changed.
