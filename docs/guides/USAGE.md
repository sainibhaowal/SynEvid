# SynEvid Usage Guide

**Version**: 1.0
**Date**: 2024-11-24
**Applies To**: All SynEvid users

## Quick Start

### 1. Initialize Repository

```bash
cd /path/to/your/repo
autopsy baseline --persist
# Creates .autopsy/ and computes initial baseline
```

### 2. Run Analysis

```bash
# Impact analysis (what breaks if I change this?)
autopsy impact -s "src/users/UserService.ts#fn:calculateTotal"

# Verify invariants
autopsy verify --strict

# Diff two states
autopsy diff --before baseline_1 --after baseline_2
```

### 3. View Results

```bash
# JSON (machine-readable)
autopsy impact -s "main" --format json

# Human-readable
autopsy impact -s "main" --format text

# SARIF (for CI)
autopsy verify --format sarif > results.sarif
```

## Core Commands

### Baseline (`autopsy baseline`)

Creates a deterministic snapshot of the repository.

```bash
# Full baseline
autopsy baseline --persist

# Incremental (faster)
autopsy baseline --incremental --persist

# Specific revision
autopsy baseline --revision HEAD~5 --persist

# With baseline ID
autopsy baseline --baseline-id my-release-1.0 --persist
```

**Output**: Creates `.autopsy/snapshots/<snap_id>/` with:
- `symbols.json` - All symbols
- `edges.json` - Dependency graph
- `manifest.json` - Metadata + BLAKE3 digest

### Impact Analysis (`autopsy impact`)

Computes bounded transitive impact from seed symbols.

```bash
# Basic impact
autopsy impact -s "src/users/UserService.ts#fn:calculateTotal"

# Multiple seeds
autopsy impact -s "UserService" -s "PaymentService"

# Options
autopsy impact -s "UserService" \
  --direction forward \           # forward, backward, bidirectional
  --max-depth 3 \                 # Traversal depth
  --budget 1000 \                 # Max nodes
  --profile callers \             # full, callers
  --top-k 50 \                    # Limit results
  --detail full \                 # summary, full
  --snapshot-id snap_abc123 \     # Specific baseline
  --edge-kinds "calls,imports"    # Filter edge types
```

**Output**:
```json
{
  "verdict": "PARTIAL",
  "coverage": "partial",
  "damaged_callers": [
    {
      "stable_id": "sym_abc123",
      "file_path": "src/orders/OrderService.ts",
      "calling_line": 42,
      "calling_col": 15,
      "diagnostic": "Calls changed signature of calculateTotal",
      "severity": "HIGH"
    }
  ],
  "newly_dead_symbols": [],
  "broken_imports": [],
  "syntax_errors": [],
  "computation_latency_ms": 45
}
```

### Verify (`autopsy verify`)

Evaluates architectural invariants.

```bash
# Verify all invariants
autopsy verify --strict

# Specific invariants file
autopsy verify --invariants-file .autopsy/invariants.yml

# With baseline
autopsy verify --baseline-id snap_abc123

# Output formats
autopsy verify --format json
autopsy verify --format sarif > results.sarif
```

**Output**:
```json
{
  "verdict": "FAIL",
  "coverage": "verified",
  "findings": [
    {
      "rule_id": "CLEAN_ARCHITECTURE_LAYERS",
      "status": "fail",
      "severity": "error",
      "message": "Layer violation: presentation depends on infrastructure",
      "entities": ["PresentationController", "DatabaseRepository"],
      "locations": [
        {"path": "src/controllers/UserController.ts", "line": 15}
      ]
    }
  ],
  "coverage_summary": {
    "verified": 45,
    "partial": 3,
    "unknown": 2,
    "total": 50
  }
}
```

### Diff (`autopsy diff`)

Compares two snapshots or baseline vs worktree.

```bash
# Two snapshots
autopsy diff --before snap_abc --after snap_def

# Baseline vs worktree
autopsy diff --before snap_abc --after worktree

# Specific files
autopsy diff --before snap_abc --after snap_def --paths "src/users/*"

# Output
autopsy diff --format json
autopsy diff --format sarif
```

### Symbols (`autopsy symbols`)

Browse and search symbols.

```bash
# List all
autopsy symbols --limit 50

# Search
autopsy find "UserService"
autopsy find "user" --kind class
autopsy find "calculate" --name-only

# With pagination
autopsy symbols --limit 20 --offset 40

# Filter
autopsy symbols --kind function --snapshot-id snap_abc
```

### Edges (`autopsy edges`)

Explore dependency graph.

```bash
# Outgoing edges
autopsy edges "UserService" --direction forward

# Incoming edges
autopsy edges "UserService" --direction backward

# Both
autopsy edges "UserService" --direction bidirectional

# Filter
autopsy edges "UserService" --kind calls --limit 100 --offset 0
```

### Map (`autopsy map`)

Whole-graph projections.

```bash
# Full map (top 100)
autopsy map --top 100

# Subgraph
autopsy map --under "src/users" --top 50

# Output
autopsy map --format json
```

## Session Commands (`autopsy session`)

Forensic session management.

```bash
# Start session
autopsy session start --actor "CI Bot" --kind AI_AGENT

# Record event
autopsy session record \
  --session-id sess_abc \
  --event-type EDIT \
  --target-symbol "UserService" \
  --verdict PASS \
  --summary "Fixed null check" \
  --evidence '{"files": ["UserService.ts"]}'

# Show session
autopsy session show --session-id sess_abc

# Close session
autopsy session close --session-id sess_abc --exit-code 0

# Report
autopsy session report --session-id sess_abc
```

## Git Commands (`autopsy git`)

Native Git operations.

```bash
# Status
autopsy git status

# Galaxy (commit graph)
autopsy git galaxy --limit 200

# Time travel
autopsy git time-travel --at "HEAD~10"

# Blame
autopsy git blame "src/users/UserService.ts"

# Diff
autopsy git diff --base HEAD~1 --head HEAD

# Stage
autopsy git stage "src/users/UserService.ts"

# Commit
autopsy git commit -m "Fix null check" --author "Bot <bot@example.com>"

# Branch
autopsy git branch feature/new-feature --create

# Tag
autopsy git tag v1.0.0 -m "Release 1.0"

# Bisect
autopsy git bisect --good v1.0 --bad HEAD --rule "CLEAN_ARCHITECTURE_LAYERS"

# Undo
autopsy git undo
```

## Analytics (`autopsy analytics`)

Generate reports.

```bash
# Dead code report
autopsy analytics report --format json --evidence

# Git risk report
autopsy analytics report --format markdown --evidence --git-risk

# Custom format
autopsy analytics report --format sarif --evidence > report.sarif

# History
autopsy analytics report-history --limit 10
```

## Catalog (`autopsy catalog`)

Backstage catalog integration.

```bash
# Scan
autopsy catalog --root /path/to/catalog

# Blast radius
autopsy blast-radius "component:default/checkout"
```

## Temporal (`autopsy temporal`)

Git history analysis.

```bash
# Replay history
autopsy temporal --at "HEAD~10" --max-commits 100

# Lineage
autopsy lineage --from "v1.0" --to "HEAD"

# Co-change
autopsy cochange --window-seconds 3600 --max-commits 1000
```

## Contracts (`autopsy contracts`)

Distributed contract verification.

```bash
# Diff contracts
autopsy contract-dist --from openapi-v1.yaml --to openapi-v2.yaml --protocol openapi

# AsyncAPI flows
autopsy contract-dist --from events.yaml --to events.yaml --protocol asyncapi-flows
```

## Test Selection (`autopsy test-select`)

```bash
autopsy test-select \
  --changed "src/users/*" \
  --lcov coverage.lcov \
  --strict
```

## Output Formats

| Format | Use Case |
|--------|----------|
| `json` | Machine parsing, CI |
| `text` | Human reading |
| `sarif` | VS Code, GitHub Code Scanning |
| `markdown` | Reports, docs |
| `html` | Browser viewing |
| `bundle` | Offline archive |

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | PASS - All checks passed |
| 2 | POLICY_FAIL - Invariant violated |
| 3 | ANALYSIS_ERROR - Config/input error |
| 4 | UNSUPPORTED - Unknown/partial coverage |

## Offline Mode

Run without daemon (limited commands):

```bash
# Commands supporting --offline:
autopsy baseline --offline --persist
autopsy impact -s "main" --offline
autopsy verify --offline --strict
autopsy diff --offline
autopsy symbols --offline
autopsy git --offline status
```

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `AUTOPSY_REPO_ROOT` | Default repo path |
| `AUTOPSY_FORMAT` | Default output format |
| `AUTOPSY_OFFLINE` | Force offline mode |
| `AUTOPSY_JOBS` | Parallel jobs |
| `SYNEVID_LOOPBACK_TOKEN` | Daemon auth token |
| `SYNEVID_DAEMON_HOST` | Daemon host |
| `SYNEVID_DAEMON_PORT` | Daemon port |
| `SYNEVID_LOOPBACK_TOKEN` | Auth token |
| `AUTOPSY_FORMAT` | Default format |

## Common Workflows

### Pre-Commit Hook

```bash
#!/bin/sh
# .git/hooks/pre-commit
autopsy verify --strict --format sarif > .git/verify.sarif
```

### CI Pipeline

```yaml
# .github/workflows/verify.yml
name: Verify
on: [push, pull_request]
jobs:
  verify:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rs/toolchain@v1
        with: toolchain: stable
      - name: Build
        run: cargo build --release -p autopsy-cli
      - name: Verify
        run: |
          ./target/release/autopsy baseline --persist
          ./target/release/autopsy verify --strict --format sarif > verify.sarif
      - uses: github/codeql-action/upload-sarif@v2
        with:
          sarif_file: verify.sarif
```

### Pre-Push Impact Check

```bash
#!/bin/sh
# .git/hooks/pre-push
changed=$(git diff --name-only @{push}..HEAD)
for file in $changed; do
  autopsy impact -s "$file" --offline --format json | jq -e '.verdict != "FAIL"' || exit 1
done
```

## Tips & Tricks

1. **Use `--offline` in CI** - No daemon needed
2. **Cache baselines** - `--persist` stores for reuse
3. **Use `--strict` in CI** - Fail on partial/unknown
4. **Profile first** - `autopsy doctor` before big changes
6. **Cache dir** - Set `cache_dir` for faster incremental
7. **Profile format** - JSON for CI, text for humans
7. **Exit codes** - Script against exit codes, not text

## Getting Help

```bash
# Command help
autopsy --help
autopsy impact --help
autopsy verify --help

# Version
autopsy --version
autopsy doctor

# Debug
RUST_LOG=debug autopsy impact -s "main"
```