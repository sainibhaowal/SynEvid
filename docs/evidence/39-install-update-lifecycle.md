# Evidence 39 — Frictionless install, update, and uninstall lifecycle

Makes the one-time setup real: one command installs the engine, the IDE
server, a **zero-build** single-file MCP server, PATH configuration, and every
detected agent host. The same command updates it; `--check` and `--uninstall`
mirror it. The audit that motivated this receipt found that the published
release path could never succeed, so every user silently fell into a
multi-minute Rust source build — or nothing at all.

An audit of `scripts/install.sh` and `.github/workflows/post-merge.yml` at
commit `3fbe51b` found:

1. The post-merge pipeline built per-platform archives (with per-file `.sha256`
   sidecars) but **no job ever created a GitHub Release**, while the installer
   downloaded only from `releases/latest/download/…` — the "prebuilt binary"
   path was dead on arrival.
2. With `curl … | bash`, `$0` is `bash`, so `dirname "$0"` could never locate
   `setup-agent-hosts.py` — agent-host wiring silently never ran in the
   one-line flow.
3. Release archives shipped no MCP server, so install-only users could not
   connect any agent without a full clone plus `npm ci && npm run build`.
4. Nothing managed PATH, verified checksums, or offered update, check, or
   uninstall modes.

---

## Implementation

### Release pipeline — it now actually publishes

`.github/workflows/post-merge.yml`:

- `cross-platform-builds` sets up Node 22 and runs `npm run build:bundle`
  (esbuild), then packages `mcp-server/index.js`, `setup-agent-hosts.py`, and
  `hooks/{safe-edit,autopsy-session}.sh` into each platform archive.
- New `release-publish` job (after the manifest job, `permissions: contents:
  write`) publishes a rolling GitHub Release for the workspace version
  (`softprops/action-gh-release@v2`, `make_latest`, `overwrite_files`),
  attaching every archive, `SHA256SUMS.txt`, and the CycloneDX SBOM.
  "Latest" therefore always means "latest `main` that passed the full
  post-merge gate" — and re-running the installer is the update path.

### Installer — `scripts/install.sh` (rewritten)

- **Modes:** install/update (default and `--update`), `--check`, `--uninstall`,
  `--help`.
- **Integrity:** downloads the platform archive and verifies its SHA-256
  against the release's `SHA256SUMS.txt`; a mismatch **aborts** (exit 2, no
  silent fallback to a source build); missing sums warn unless
  `SYNEVID_STRICT=1`. Release-absent (404) still falls back to a source build.
- **Layout:** `$PREFIX/bin/{autopsy,synevid-lsp}` (default `~/.local`),
  `$PREFIX/share/synevid/{mcp-server/index.js,setup-agent-hosts.py,hooks/*}`.
  Per-user, system-wide: one installation serves every repository; repos are
  never touched (only the engine's own `.autopsy/` store when a command writes
  one).
- **PATH:** one managed block (`# >>> synevid >>>` … `# <<< synevid <<<`) in
  `~/.bashrc`/`~/.zshrc` (or `~/.profile`), idempotent by byte comparison and
  stripped again by `--uninstall`; `SYNEVID_NO_PATH=1` opts out.
- **Hosts:** wires the installed single-file MCP server with absolute paths
  (editors never depend on PATH); `python3` absence is a warning, not a failure.
- **Uninstall:** unwires hosts (backup-first), removes the PATH block and the
  installed files, tolerant of repeated runs.
- A sourcing guard (`BASH_SOURCE[0]` check) keeps the file safe when piped and
  testable when sourced.

### Host wiring — `scripts/setup-agent-hosts.py`

- `--unwire` (inverse mode; dry-runnable with `--check`; backup-first).
- Layout-aware MCP resolution (installed `share/synevid/mcp-server/index.js`,
  repo `dist/index.js` or the esbuild bundle, then
  `~/.local/share/synevid/…`) and hook resolution next to the installed script.
- Auto-detects `~/.local/bin/autopsy` when `--autopsy-bin` is absent.

### Zero-build MCP artifact

- `apps/mcp-server`: esbuild dev dependency; `npm run build:bundle` emits
  `dist/index.bundle.js` (single file, Node built-ins external, shebang kept);
  `pretest` builds it; `test/bundle.test.js` spawns the bundle over the real
  stdio protocol and asserts the 11-tool surface plus a digest-bearing verify.
- `test/mcp_stdio.js` gained an `entry` override for that test, and
  `freshWorkspaceCopy` now excludes `.autopsy/` — fixing an intermittent
  `ENOENT` race when the corpus store was being written during a parallel copy.

### Offline self-test — `scripts/tests/test-install.sh`

Fake GitHub release + fake `curl` shim + throwaway HOME; 39 assertions:
install, checksum verification and mismatch abort, update idempotency (exactly
one PATH block), `--check` read-only behavior, `--uninstall` (binaries, share
dir, PATH block, Cursor/VS Code entries removed, unrelated config preserved).
Wired into `.pre-commit-config.yaml` (now 13 hooks) and a new CI job
`installer-and-wiring-check`, which feeds the pre-merge gate.

---

## Verification telemetry

| Check | Command | Result |
| :--- | :--- | :--- |
| Installer self-test (offline) | `bash scripts/tests/test-install.sh` | 39/39 checks |
| MCP suite (incl. shipped bundle) | `cd apps/mcp-server && npm test` | 44/44 |
| Gate | `pre-commit run --all-files` | 13/13 hooks |
| Shell syntax | `bash -n scripts/install.sh` | clean |

## Honest limits

- No Windows PowerShell installer; Git Bash installs work, but PATH is managed
  manually there (the installer says so).
- No silent auto-update by design — SynEvid never phones home; update =
  re-run the installer.
- The rolling release refreshes the same version tag in place; release
  channels (prerelease, LTS) are future work.
- `linux-arm64` has no prebuilt target yet and falls back to a source build.
- The self-test covers the Unix flow; the Windows `.zip` extraction branch is
  exercised only by CI release packaging, not by this test.
