# Evidence 33 — How to verify this project (end-to-end, no skips)

Run these commands in order. Every one must pass before committing:

1. Format: `cargo fmt --all -- --check`
2. Lint: `cargo clippy --workspace --all-targets -- -D warnings`
3. Unit + integration tests: `cargo test --workspace --all-targets` (924 tests across the workspace, including the `synevid-lsp` suite)
4. Doc tests: `cargo test --workspace --doc`
5. Determinism (100/100): `./scripts/verify-determinism.sh`
6. Architecture boundary (zero LLM/core to MCP): `./scripts/verify-arch-boundaries.sh`
7. Pre-commit (13 hooks): `pre-commit run --all-files`
8. MCP server typecheck, lint, and tests: `cd apps/mcp-server && npm run typecheck && npm run lint && npm test` (44 tests, including the protocol-level agent-loop suites `e2e_agent_find_and_envelope.test.js` / `e2e_agent_session.test.js` and the shipped-bundle test `bundle.test.js`)
9. IDE surface (LSP): `cargo test -p synevid-lsp` (24 tests; also covered by step 3 as a workspace member)
10. Web inspector: `cd apps/web-inspector && npm test`
11. Installer, update, and host wiring (offline, fake release): `bash scripts/tests/test-install.sh` (39 checks)
12. Lineage CLI gate: `cargo test -p autopsy-cli --test step31_lineage_cli`
13. Storage layer gate: `cargo test -p autopsy-storage`
14. CLI persistence live: `mkdir -p .autopsy; ./target/debug/autopsy version --persist --repo-root .`
15. Lineage persistence + DB check: `./target/debug/autopsy lineage --from HEAD~1 --to HEAD --persist --repo-root .`
16. Agent-surface evidence present: `ls docs/evidence/35-tier1-agent-experience.md docs/evidence/36-tier2-agent-quality.md docs/evidence/37-tier3-polish.md docs/evidence/38-agent-gate-closure.md docs/evidence/39-install-update-lifecycle.md`
17. Font assets untracked: `echo $(git ls-files web/fonts | wc -l) (expect 0)`
18. Main == origin/main: `test $(git rev-parse HEAD) = $(git rev-parse origin/main)`
