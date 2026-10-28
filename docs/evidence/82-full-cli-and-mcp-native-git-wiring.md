# Evidence Receipt 82: End-to-End CLI & MCP Native Git Surface Expansion

## Executive Summary
This document records the end-to-end alignment and expansion of the terminal command line interface (`crates/autopsy-cli`) and the Model Context Protocol server (`apps/mcp-server`) with the Phase 3 native Git engine.

## Surfaces Synchronized
1. **Command Line Interface (`crates/autopsy-cli`)**:
   - Added `autopsy git` top-level command and subcommands in `git_cmd.rs`:
     - `autopsy git status`: scans working tree with zero subprocesses (`INV-SANDBOX-003`).
     - `autopsy git galaxy [--limit <N>]`: inspects evolutionary commit DAG with branches and tags.
     - `autopsy git diff --base <A> --head <B>`: any-to-any semantic commit diff and contract drift report.
     - `autopsy git diff-file <PATH>`: line-level unified diff of a working file against HEAD.
     - `autopsy git stage [PATHS...]`: stages specific files for verified commit.
     - `autopsy git unstage [PATHS...]`: unstages files from the pre-commit index.
     - `autopsy git commit -m <MSG> [--tag <TAG>]`: verified commit with Synevid trailers.
     - `autopsy git branch [NAME] [--action create|switch|delete]`: manages local branch pointers.
     - `autopsy git tag <NAME> [--target <OID>] [--delete]`: manages semantic release tags.
     - `autopsy git merge <SOURCE_BRANCH>`: guarded branch merge with undo checkpoint.
     - `autopsy git bisect --good <A> --bad <B>`: autopsy visual bisect to find breaking commits.
     - `autopsy git undo`: 1-click atomic undo restoring pre-mutation state from `refs/autopsy/undo/*`.
   - Updated `toolspec.rs` to include `git` in the deterministic toolspec.
2. **Model Context Protocol (`apps/mcp-server`)**:
   - Expanded tool registry from 11 tools to 17 tools:
     - `autopsy_git_status`
     - `autopsy_git_galaxy`
     - `autopsy_git_diff`
     - `autopsy_git_bisect`
     - `autopsy_git_commit`
     - `autopsy_git_undo`
   - Verified that all 17 tools speak the unified `evidenceEnvelope` with deterministic exit codes and zero bare error JSON.
3. **Standalone Desktop Application (`apps/desktop-electron`)**:
   - Added standalone Electron desktop shell with native OS folder dialogs, sidecar daemon watchdog, and local loopback CSP (`INV-SANDBOX-004`).

## Verification Telemetry
- `target/debug/autopsy git --help`: verified all 12 subcommands.
- `target/debug/autopsy git status`: verified working tree scanning without `git` CLI subprocesses.
- `target/debug/autopsy git galaxy`: verified DAG reading and rendering.
- `npm run typecheck && npm run build` in `apps/mcp-server`: 0 errors.
- `npm test` in `apps/desktop-electron`: 3/3 passed.
- `cargo check -p autopsy-cli`: passed.
