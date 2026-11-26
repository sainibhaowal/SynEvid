# Phase 5 evaluation — packaged lifecycle and security

## Evaluation status

**PASS for the Phase 5 G5 acceptance on the tested Windows, Linux, and macOS
runner targets.** Exact source SHA `21ba6bf99f4b157dfbbb1db2c35315c169de7a4b`
passed the native matrix and release-bundle verification. The artifacts are
unsigned by OS vendors; first-launch trust warnings remain expected and are
documented. This is not a claim for untested OS versions or future revisions.

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
- Exact-SHA native matrix [run `45044033733`](https://github.com/sainibhaowal/SynEvid/actions/runs/45044033733): all workspace tests on Windows/Linux/macOS, all packaged Electron/security/browser smoke jobs, replay after daemon restart, determinism, schemas, MSRV, architecture, dependency policy, and history budget passed.
- Post-merge supply-chain [run `45044046615`](https://github.com/sainibhaowal/SynEvid/actions/runs/45044046615): native Electron artifacts for all three OSes and release binaries built; CycloneDX inventories generated; checksum verification, keyless Sigstore signature verification, and SLSA provenance verification passed. Final bundle: [artifact `14763372530`](https://github.com/sainibhaowal/SynEvid/actions/runs/45044046615/artifacts/14763372530); provenance: [attestation `70795215`](https://github.com/sainibhaowal/SynEvid/attestations/70795215).
- Release publication was deliberately disabled. OS-level signing/notarization was not used; the keyless signature covers the checksum file/provenance, not the executable publisher identity.

Detailed output and boundaries are in
[`105-phase5-packaged-security-watchdog-and-supply-chain.md`](../../evidence/105-phase5-packaged-security-watchdog-and-supply-chain.md).

The prior macOS reconnect failure on `34fe80a` was fixed with sequence-based
durable replay polling. The exact-source macOS browser workflow passed in
`45044033733`; there are no remaining G5 acceptance gaps for the platform
targets tested here. MSI installation and OS-level signing are not claimed.
