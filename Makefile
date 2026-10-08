.PHONY: all check test fmt determinism cross-platform boundaries schemas pre-commit

export TMPDIR ?= /home/ravi/.cache/tmp

all: check

fmt:
	cargo fmt --all

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
	cd apps/mcp-server && npm run typecheck && npm test

check: fmt boundaries schemas determinism cross-platform mcp-check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace --all-targets
	cargo test --workspace --doc

pre-commit:
	pre-commit run --all-files
