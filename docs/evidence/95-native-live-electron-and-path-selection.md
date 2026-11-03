# Native Live Electron Runtime and Repository Path Selection

Date: 2026-11-01

## Findings

- The web client converted every Windows drive path into `/mnt/<drive>/...`, even when the daemon was a native Windows process.
- Browser directory pickers returned only a folder name and the modal appended it to the current location, producing duplicate trailing directory names.
- The daemon emitted fixed `User Home` and `Root Filesystem` entries and restored stale persisted registrations without canonicalizing them.
- The previous desktop launchers depended on a user-specific launcher directory and could inherit `ELECTRON_RUN_AS_NODE=1`.

## Implemented flow

`npm run dev` in `apps/desktop-electron` now checks Node 20.9+ and npm 10+, clears Electron's Node-only environment flag, launches Electron from `node_modules/electron`, sets `SYNEVID_DEV=1`, and lets the Electron main process start Next.js in dev mode. Renderer changes hot-reload; `electron-builder` is not part of this loop.

The selected path now flows as:

`native picker -> native Windows path -> frontend slash normalization -> daemon host normalization -> canonical stored path -> active repository`

The live Electron shell now injects a development-only CSP exception for React's
`eval()` callstack helper. Production and packaged Electron continue to use the
strict CSP without `unsafe-eval`. A live source launch therefore does not report
the React CSP console error or open the Next.js issue overlay.

The web cockpit is pinned to Next.js 16.4.3, matching the workspace dependency,
so the development overlay does not report the previously installed 16.4.2
package as stale.

The daemon strips `//?/` only after host-side canonicalization, converts WSL mount paths only when appropriate for the daemon host, and returns an idempotent `connected` result for an already registered directory. Removing a saved registration removes metadata only; it does not delete source files.

## Verification

- `apps/web`: `npm test` — 14/14 passed.
- `apps/web`: `npm exec -- tsc --noEmit` — passed.
- `apps/desktop-electron`: `npm test` — 8/8 passed.
- Rust focused tests — path normalization, native Windows path preservation, and empty quick presets passed.
- `cargo fmt --all -- --check` — passed.
- `cargo build -p synevid-ui --release` — passed.
- Live native verification — source Electron window loaded `http://127.0.0.1:3000/`; selecting `C:/Users/Ravin/Projects/Projetcs Alpha/SynEvid/apps/desktop-electron/src` produced the exact canonical path, indexed 2 files, and displayed the same path in the native window.
- Invalid native and `/mnt/c` inputs both return the host-native diagnostic `C:/Users/Ravin/Projects/does-not-exist`, rather than fabricating a nested or WSL-only path.
- Reconnecting the same selected directory returns the existing registration and activates it without a SQLite uniqueness error.
- Live Electron CSP verification â€” the source runtime uses the development-only `unsafe-eval` allowance; the production branch remains strict, and the native window no longer reports the React `eval()` console error.
- Live Electron freshness verification â€” the restarted native window has no Next.js issue badge and the web dependency tree resolves `next@16.4.3` and `eslint-config-next@16.4.3`.
