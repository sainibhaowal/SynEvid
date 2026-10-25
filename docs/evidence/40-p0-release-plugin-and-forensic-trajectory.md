# Evidence 40 — P0 Multi-Arch Release, Native Windows Installer, IDE/CI Plugin System & Task-Completion Forensic Trajectory Ledger

Closes the remaining distribution, onboarding, IDE/CI plugin, and multi-turn AI agent memory gaps identified in Evidence 39 ("Honest limits"):

1. **Universal Multi-Platform Release Matrix & System Packages (P0):**
   - Expanded `.github/workflows/post-merge.yml` from 4 targets to a **7-target release matrix** triggered on both `main` pushes and `v*.*.*` git tags:
     - `x86_64-unknown-linux-gnu` (`.tar.gz` + `.deb` + `.rpm`)
     - `x86_64-unknown-linux-musl` (static Linux binary for Alpine and older glibc distributions)
     - `aarch64-unknown-linux-gnu` (`.tar.gz` + `.deb` + `.rpm` for AWS Graviton, Raspberry Pi, ARM CI runners)
     - `aarch64-unknown-linux-musl` (static ARM64 Linux binary)
     - `aarch64-apple-darwin` (Apple Silicon macOS)
     - `x86_64-apple-darwin` (Intel macOS)
     - `x86_64-pc-windows-msvc` (Windows x86_64 `.zip`)
   - Added [`scripts/build-linux-packages.sh`](../../scripts/build-linux-packages.sh) to build native Debian/Ubuntu `.deb` (`dpkg-deb`) and Fedora/RHEL `.rpm` (`rpmbuild`) packages containing `/usr/bin/autopsy`, `/usr/bin/synevid-lsp`, and `/usr/share/synevid/{mcp-server/index.js,setup-agent-hosts.py,hooks/*}`.
   - Added Homebrew ([`packaging/homebrew/synevid.rb`](../../packaging/homebrew/synevid.rb)), Scoop ([`packaging/scoop/synevid.json`](../../packaging/scoop/synevid.json)), and Winget ([`packaging/winget/sainibhaowal.SynEvid.yaml`](../../packaging/winget/sainibhaowal.SynEvid.yaml)) manifests plus deterministic checksum stamper [`scripts/render-package-manifests.py`](../../scripts/render-package-manifests.py).

2. **Cryptographic Provenance & Code Signing (P0):**
   - Integrated GitHub Artifact Attestations (`actions/attest-build-provenance@v2`, SLSA Build L2+) and keyless Sigstore `cosign sign-blob` (`SHA256SUMS.txt.sigstore.json`) with `id-token: write` and `attestations: write` permissions in `.github/workflows/post-merge.yml`.
   - Added macOS `codesign` support in `.github/workflows/post-merge.yml` (ad-hoc signature by default, Developer ID certificate when `APPLE_CODESIGN_IDENTITY` is configured).

3. **Native Windows PowerShell Installer & Linux ARM64/musl Installer Support (P0):**
   - Created [`scripts/install.ps1`](../../scripts/install.ps1): native PowerShell installer (`irm .../install.ps1 | iex`) supporting `-Update`, `-Check`, `-Uninstall`, and `-Help`, fail-closed SHA-256 verification against `SHA256SUMS.txt`, automatic per-user `[Environment]::SetEnvironmentVariable("Path", ..., "User")` configuration (no Administrator privileges or Git Bash required), and agent-host wiring.
   - Updated [`scripts/install.sh`](../../scripts/install.sh) to support `linux-arm64` (`aarch64-unknown-linux-gnu`) and static `musl` builds (`--musl`, `SYNEVID_LIBC=musl`, or automatic detection on Alpine / older glibc).
   - Updated [`scripts/setup-agent-hosts.py`](../../scripts/setup-agent-hosts.py) with OS-aware host configuration paths for Windows (`%APPDATA%`), macOS (`~/Library/Application Support`), and Linux (`~/.config`).

4. **Zero-Config Onboarding (`autopsy init`) & End-to-End Plugin System:**
   - Implemented [`crates/autopsy-cli/src/init.rs`](../../crates/autopsy-cli/src/init.rs) (`autopsy init [--force]`): deterministically scans repository file extensions and directory layouts (`domain`, `application`, `infrastructure`, `api`, `ui`), generates starter `autopsy.toml` and `.autopsy/invariants.yml` without overwriting existing user configurations unless `--force` is supplied, and returns a BLAKE3-signed `InitReport` in the standard envelope.
   - Created the VS Code / Cursor / Windsurf IDE extension in [`apps/vscode-extension/`](../../apps/vscode-extension/) (`package.json`, `extension.js`, `test/extension.test.js`):
     - Inline **Blast Radius CodeLens** above TypeScript/JS, Python, Go, and Rust declarations.
     - **Hover Provider** displaying caller counts, coverage states, and BLAKE3 digests.
     - **Status Bar** health/verification indicator (`$(shield) SynEvid`).
     - **Task Forensic Trajectory Sidebar** (`synevidTrajectory` tree view in the Activity Bar) rendering `.autopsy/runs/latest.json` so developers and IDE agents can inspect prior session verdicts, snapshots, and repair guidance directly in the editor.
   - Created root composite GitHub Action [`action.yml`](../../action.yml) so any repository can run SynEvid as a 1-step CI plugin (`uses: sainibhaowal/SynEvid@v0.1.0`).

5. **Agentic Task-Completion Forensic Trajectory Ledger (`.autopsy/runs/`):**
   - Extended [`crates/autopsy-cli/src/session.rs`](../../crates/autopsy-cli/src/session.rs) with `TaskForensicReport` and `autopsy session report [--session-id <id>]`.
   - Whenever `autopsy session close` or `autopsy session report` executes, SynEvid deterministically writes both machine-readable JSON and human/LLM-readable Markdown reports to:
     - `.autopsy/runs/<session_id>.json` and `.autopsy/runs/<session_id>.md`
     - `.autopsy/runs/latest.json` and `.autopsy/runs/latest.md`
   - Each report carries `started_snapshot`, `completed_snapshot`, `verdict`, `evidence_exit_code`, ordered `steps`, `report_digest` (BLAKE3, zero timestamps), and explicit `next_agent_guidance` instructing the next AI agent turn whether the prior turn passed, failed (and which command must be repaired first), or closed incomplete.
   - Created [`agents/hooks/autopsy-task-complete.sh`](../../agents/hooks/autopsy-task-complete.sh) and wired it into `scripts/setup-agent-hosts.py --hooks` as a Claude Code `Stop` hook (alongside `PreToolUse` `safe-edit.sh`), with unconditional cleanup on `--unwire` / `--uninstall`.
   - Exposed `action: "report"` on the `autopsy_session` MCP tool and registered MCP resource `autopsy://runs/latest{?repo_root}` in [`apps/mcp-server/src/server.ts`](../../apps/mcp-server/src/server.ts).

---

## Verification Telemetry

| Check | Command | Result |
| :--- | :--- | :--- |
| Rust Format Check | `cargo fmt --all -- --check` | Passed (0 diffs) |
| Rust Clippy Lints | `cargo clippy --workspace --all-targets -- -D warnings` | Passed (0 warnings) |
| CLI Unit & Integration Tests | `cargo test -p autopsy-cli` | Passed (`init`, `session`, `toolspec`, `suggestions`, `step31`, `step44`, `step46`) |
| VS Code Extension Tests | `cd apps/vscode-extension && npm test` | 4/4 passed |
| Web Inspector Tests | `cd apps/web-inspector && npm test` | 15/15 passed |
| MCP Server Build, Typecheck, Lint & Tests | `cd apps/mcp-server && npm run build && npm run build:bundle && npm run typecheck && npm run lint && npm test` | 45/45 passed |
| Offline Installer & Hook Wiring Self-Test | `bash scripts/tests/test-install.sh` | 44/44 passed |
| Windows PowerShell Installer Smoke Check | `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/install.ps1 -Help` | Passed (exit 0) |
| Architectural Boundary Gate | `./scripts/verify-arch-boundaries.sh` | Passed (zero LLM/network/subprocess in `crates/*`) |
