# Phase 5 evaluation — packaged lifecycle and security

## Evaluation status

**PARTIAL overall.** Local Windows packaged acceptance passes; exact-commit
Linux/macOS proof and release-bundle attestation are pending. This report makes
no claim of universal production readiness.

## Scope and method

The current source-based Electron application was packaged on Windows and
tested through both an NSIS install/uninstall and portable launch. The smoke
connected a real temporary TypeScript repository, indexed it, requested an MCP
impact action, compared the visible UI analytics receipt with the daemon result,
terminated the daemon, verified watchdog recovery and repository identity,
invoked tray Show and Quit callbacks, and required both loopback ports to close.
The package checks also verified renderer isolation, CSP nonce, blocked hostile
navigation/popups, hostile IPC denial, and HTTP 401 responses for missing and
invalid daemon tokens.

## Results

- Rust formatting check: pass.
- Managed-runtime regression: pass on library and binary targets.
- Electron tests: 21/21 pass.
- Current-source production runtime build: pass for Rust daemon, Next standalone
  cockpit, and MCP bridge.
- Windows artifact validation: required NSIS/portable outputs and three npm
  CycloneDX inventories validated. An MSI file existed but was not rebuilt
  after the final Electron-only tray marker change and was not installer-smoked.
- Windows installer smoke: pass for NSIS install/uninstall and portable launch.
- Watchdog recovery: pass; `ERR-DAEMON-301` emitted and the original repository
  was restored by ID in status and report.
- Security/lifecycle smoke: pass on Windows.
- Exact commit Actions run `45040294535` exposed a macOS browser replay timeout
  after daemon restart, despite successful Windows/Linux packaged smoke and all
  three platform workspace suites. A durable catch-up poll was added alongside
  SSE/WebSocket; local Windows end-to-end web tests pass 21/21 and TypeScript
  checking passes. Native CI for this follow-up is still pending.

Detailed output and boundaries are in
[`105-phase5-packaged-security-watchdog-and-supply-chain.md`](../../evidence/105-phase5-packaged-security-watchdog-and-supply-chain.md).

## Not yet evaluated

- Changed-source native packaged lifecycle/security smoke on Linux and macOS.
- Exact-commit aggregated daemon/Electron/cockpit/MCP SBOM release bundle.
- Full checksum, Sigstore keyless signature, and SLSA provenance verification.

These are required before changing Phase 5 traceability to fully verified.
