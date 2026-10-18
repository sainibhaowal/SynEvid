# Recipe: PR Gate (CI Integration)

Run Synevid as a deterministic gate in CI. Every command exits non-zero on
failure, so a shell `set -e` or GitHub Actions `run` step fails the build.

## Minimal gate

```bash
# 1. Baseline + persist
autopsy baseline --persist --format json

# 2. Verify invariants (exit 2 on policy fail, 4 on coverage gate)
autopsy verify --strict --format json

# 3. Test selection (exit 2 if run-all-tests required)
autopsy test-select --changed src/foo.ts --fail-if-all-tests-required --format json
```

## Full gate script

```bash
#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="${1:-.}"
cd "$REPO_ROOT"

# Baseline
autopsy baseline --persist --format json

# Invariant gate
autopsy verify --strict --format json

# Contract diff (if contract documents changed): compare each document's two revisions.
# `contract-dist` takes document files, not git revisions — materialize the base side first.
if git diff --name-only HEAD~1 -- '*.yaml' '*.yml' '*.json' | grep -q .; then
  while IFS= read -r doc; do
    [ -f "$doc" ] || continue
    base_doc="$(mktemp)"
    git show "HEAD~1:$doc" > "$base_doc"
    autopsy contract-dist "$base_doc" "$doc" --protocol openapi --strict --format json
    rm -f "$base_doc"
  done < <(git diff --name-only HEAD~1 -- '*.yaml' '*.yml' '*.json')
fi

# Test selection: `--changed` takes exactly one path per occurrence, so repeat the flag.
CHANGED_ARGS=()
while IFS= read -r file; do
  CHANGED_ARGS+=(--changed "$file")
done < <(git diff --name-only HEAD~1)
autopsy test-select "${CHANGED_ARGS[@]}" --fail-if-all-tests-required --format json

echo "All gates passed."
```

## Exit codes in CI

| Code | CI action |
|---|---|
| `0` | Gate passed — continue |
| `2` | Policy/contract failure — block merge |
| `3` | Analysis error — investigate (not a code problem) |
| `4` | Coverage unknown/partial under `--strict` — block merge |

## Tips

- Use `--compact` on large repos to keep output within token budgets.
- Set `AUTOPSY_TIMEOUT_MS=120000` for large repositories.
- Set `AUTOPSY_MAX_BUFFER_BYTES=52428800` (50 MB) if output exceeds 20 MB.
- Use `--disable-suggestion` to suppress hint lines in CI logs.
- Run `autopsy doctor` first to verify the engine is healthy.
