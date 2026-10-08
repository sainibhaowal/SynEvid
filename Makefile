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
	python3 -c 'import glob, json; [json.load(open(f)) for f in glob.glob("schemas/*.json")]; print("All schemas valid.")'

check: fmt boundaries schemas determinism cross-platform
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace --all-targets
	cargo test --workspace --doc

pre-commit:
	pre-commit run --all-files
