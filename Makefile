.PHONY: all check test fmt fmt-check determinism cross-platform boundaries schemas pre-commit mcp-check web-inspector-check vscode-extension-check

export TMPDIR ?= /tmp

all: check

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

test:
	cargo test --workspace --all-targets
	cargo test --workspace --doc

determinism:
	./scripts/verify-determinism.sh

cross-platform:
	./scripts/verify-cross-platform.sh

boundaries:
	./scripts/verify-arch-boundaries.sh

schemas:
	python3 -c 'import glob, json, tomllib, yaml; [json.load(open(f)) for f in glob.glob("schemas/*.json")]; tomllib.loads(open("autopsy.toml","rb").read().decode()); yaml.safe_load(open(".autopsy/invariants.yml","r")); json.load(open(".autopsy/baseline.json")); print("All schemas and configs valid.")'

mcp-check:
	cd apps/mcp-server && npm run typecheck && npm run lint && npm test

web-inspector-check:
	cd apps/web-inspector && npm test

vscode-extension-check:
	cd apps/vscode-extension && npm test

check: fmt-check boundaries schemas determinism cross-platform mcp-check web-inspector-check vscode-extension-check
	cargo --list | grep -q clippy && cargo clippy --workspace --all-targets -- -D warnings || true
	cargo test --workspace --all-targets
	cargo test --workspace --doc

pre-commit:
	pre-commit run --all-files
