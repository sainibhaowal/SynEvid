# Evidence 33 — How to verify this project (end-to-end, no skips)

Run these commands in order. Every one must pass before committing:

1. Format: `cargo fmt --all -- --check`
2. Lint: `cargo clippy --workspace --all-targets -- -D warnings`
3. Unit + integration tests: `cargo test --workspace --all-targets`
4. Doc tests: `cargo test --workspace --doc`
5. Determinism (100/100): `./scripts/verify-determinism.sh`
6. Architecture boundary (zero LLM/core to MCP): `./scripts/verify-arch-boundaries.sh`
7. Pre-commit (12 hooks): `pre-commit run --all-files`
8. MCP server: `cd apps/mcp-server && npm run typecheck && npm run lint && npm test`
9. Web inspector: `cd apps/web-inspector && npm test`
10. Lineage CLI gate: `cargo test -p autopsy-cli --test step31_lineage_cli`
11. Storage layer gate: `cargo test -p autopsy-storage`
12. CLI persistence live: `mkdir -p .autopsy; ./target/debug/autopsy version --persist --repo-root .`
13. Lineage persistence + DB check: `./target/debug/autopsy lineage --from HEAD~1 --to HEAD --persist --repo-root .`
14. Evidence docs present: `ls docs/evidence/29_30_31_32_*.md`
15. Font assets untracked: `echo $(git ls-files web/fonts | wc -l) (expect 0)`
16. Main == origin/main: `test $(git rev-parse HEAD) = $(git rev-parse origin/main)`
