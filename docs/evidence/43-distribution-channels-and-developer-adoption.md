# Production Evidence 43: Distribution Channels & Developer Adoption Ecosystem

**Date:** 2026-10-26  
**Status:** VERIFIED & PRODUCTION READY  
**Traceability:** `FR-026`, `INV-SANDBOX-001..004`, `AGENT-GATE-1`, `DIST-CHANNELS-001`

---

## 1. Overview & Objectives

Deliver enterprise-grade distribution channels, package manager recipes, workflow automation, and IDE integrations to unlock frictionless developer adoption across all major operating systems and deployment environments:

1. **Package Managers:**
   - **Homebrew (`brew install synevid`):** Multi-arch formula in [`Formula/synevid.rb`](file:///Formula/synevid.rb) targeting macOS (`arm64`, `x86_64`) and Linux (`arm64`, `x86_64`) with executable symlinks (`autopsy`, `synevid`, `synevid-ui`).
   - **Cargo Binstall (`cargo binstall autopsy-cli`):** Configured `[package.metadata.binstall]` metadata in [`crates/autopsy-cli/Cargo.toml`](file:///crates/autopsy-cli/Cargo.toml) fetching pre-compiled GitHub release tarballs/zips directly without compiling from source.
   - **Zero-Install MCP (`npx @synevid/mcp`):** Scoped package [`apps/mcp-server/package.json`](file:///apps/mcp-server/package.json) declared as `@synevid/mcp` with executables `synevid-mcp` and `@synevid/mcp`.
   - **Python / `uvx` Wrapper (`pip install synevid`, `uvx synevid`):** Declared in [`packaging/python/pyproject.toml`](file:///packaging/python/pyproject.toml) with native release binary auto-bootstrap via `synevid_cli`.
   - **Docker Multi-Arch Container (`ghcr.io/sainibhaowal/synevid`):** Multi-stage non-root container in [`Dockerfile`](file:///Dockerfile) with automated GitHub Actions build and push in [`.github/workflows/docker.yml`](file:///.github/workflows/docker.yml).

2. **Marketplace GitHub Action (`uses: sainibhaowal/synevid-action@v1`):**
   - Enriched [`action.yml`](file:///action.yml) with outputs (`verdict`, `exit-code`, `snapshot-id`, `sarif-path`), Markdown step summaries via `$GITHUB_STEP_SUMMARY`, and SARIF 2.1.0 output for GitHub Code Scanning.

3. **VS Code & Cursor Inline Blast Radius CodeLens:**
   - Enriched [`apps/vscode-extension/extension.js`](file:///apps/vscode-extension/extension.js) with live CodeLens (`$(pulse) 12 callers · 3 tests`), caller and test metric parsing, caching, and hover inspection.

4. **Zero-Config Architecture Layer Detection (`autopsy init`):**
   - Broadened heuristic detection in [`crates/autopsy-cli/src/init.rs`](file:///crates/autopsy-cli/src/init.rs) to detect presentation, application, domain, and infrastructure patterns across root and monorepo folders (`pkg`, `packages`, `crates`, `internal`, `domain`, `infra`, `services`).

5. **Pre-Commit Hook Repository:**
   - Manifest [`.pre-commit-hooks.yaml`](file:///.pre-commit-hooks.yaml) exposing hooks `synevid-verify`, `synevid-doctor`, and `synevid-baseline` for single-line `.pre-commit-config.yaml` integration.

---

## 2. Test & Verification Telemetry

- **VS Code Extension Tests (`apps/vscode-extension`):**
  - `node --test test/extension.test.js`: 6 passed, 0 failed (CodeLens formatting, blast radius metrics parsing, binary resolution, declaration extraction).
- **MCP Server Tests (`apps/mcp-server`):**
  - `node --test --test-concurrency=1`: 44 passed, 0 failed (stdio transport, toolspec discovery, snapshot pinning, 3-call agent loop).
- **Web Inspector Tests (`apps/web-inspector`):**
  - `node --test test/inspector.test.js`: 15 passed, 0 failed.
- **Rust Init Unit Tests:**
  - `cargo test -p autopsy-cli -- init::`: 1 passed, 0 failed.
- **Rust Formatting & Architectural Boundaries:**
  - `cargo fmt --all -- --check`: PASSED.
  - `./scripts/verify-arch-boundaries.sh`: PASSED (zero sub-process execution, zero outbound network sockets in core crates).
