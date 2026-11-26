# SynEvid CI/CD Integration Guide

**Version**: 1.0
**Date**: 2024-11-24
**Applies To**: GitHub Actions, GitLab CI, Azure Pipelines, Jenkins

## Overview

SynEvid integrates into CI/CD pipelines at multiple levels:
1. **Pre-commit** - Fast local checks
2. **PR Validation** - Impact/verify on changed files
3. **Merge Gate** - Full verification on merge
4. **Release** - Baseline creation + evidence
5. **Scheduled** - Regression detection

## GitHub Actions

### Minimal Setup

```yaml
# .github/workflows/synevid.yml
name: SynEvid Verification

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

jobs:
  verify:
    name: SynEvid Verify
    runs-on: ubuntu-latest
    timeout-minutes: 30
    
    steps:
      - name: Checkout
        uses: actions/checkout@v4
        with:
          fetch-depth: 0  # Full history for git commands

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Cache Cargo
        uses: Swatinem/rust-cache@v2

      - name: Build autopsy-cli
        run: cargo build --release -p autopsy-cli

      - name: Run SynEvid Verify
        env:
          SYNEVID_LOOPBACK_TOKEN: ${{ secrets.SYNEVID_LOOPBACK_TOKEN }}
        run: |
          ./target/release/autopsy baseline --persist --offline
          ./target/release/autopsy verify --strict --offline --format sarif > verify.sarif

      - name: Upload SARIF
        uses: github/codeql-action/upload-sarif@v2
        if: always()
        with:
          sarif_file: verify.sarif
          category: synevid-verify

      - name: Run Impact on Changes
        if: github.event_name == 'pull_request'
        run: |
          CHANGED=$(git diff --name-only origin/${{ github.base_ref }}...HEAD)
          for file in $CHANGED; do
            ./target/release/autopsy impact -s "$file" --offline --format json | jq -e '.verdict != "FAIL"' || exit 1
          done
```

### Advanced: Matrix Testing

```yaml
# .github/workflows/synevid-matrix.yml
name: SynEvid Matrix

on:
  push:
    branches: [main]
  schedule:
    - cron: '0 2 * * *'  # Daily at 2 AM

jobs:
  verify-matrix:
    name: Verify (${{ matrix.os }}, ${{ matrix.rust }})
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
        rust: [stable, beta, nightly]
        include:
          - os: ubuntu-latest
            rust: stable
            features: "full"

    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 0 }
      
      - name: Install Rust
        uses: dtolnay/rust-toolchain@${{ matrix.rust }}
        with:
          components: rustfmt, clippy
      
      - name: Cache Cargo
        uses: Swatinem/rust-cache@v2
        with:
          key: ${{ matrix.os }}-${{ matrix.rust }}
      
      - name: Build
        run: cargo build --release --workspace --all-targets --all-features
      
      - name: Test
        run: cargo test --workspace --all-targets --release -- --nocapture
      
      - name: SynEvid Verify
        if: matrix.features == 'full'
        run: |
          cargo build --release -p autopsy-cli
          ./target/release/autopsy baseline --persist --offline
          ./target/release/autopsy verify --strict --offline --format json > verify.json
          ./target/release/autopsy impact -s "src/main.rs" --offline --format json > impact.json
```

### SARIF Upload

```yaml
# Upload to GitHub Code Scanning
- name: Upload SARIF
  uses: github/codeql-action/upload-sarif@v2
  if: always()
  with:
    sarif_file: verify.sarif
    category: synevid-verify
    wait-for-processing: true
```

## GitLab CI

```yaml
# .gitlab-ci.yml
stages:
  - verify
  - test
  - build

variables:
  CARGO_TERM_COLOR: "always"
  RUST_BACKTRACE: 1

cache:
  key: "${CI_COMMIT_REF_SLUG}"
  paths:
    - .cargo/registry/
    - target/

verify:
  stage: verify
  image: rust:1.90-slim
  before_script:
    - apt-get update && apt-get install -y git sqlite3
    - cargo install cargo-binstall
    - cargo binstall autopsy-cli autopsy-daemon
  script:
    - autopsy baseline --persist --offline
    - autopsy verify --strict --offline --format sarif > verify.sarif
  artifacts:
    reports:
      sast: verify.sarif
    when: always
    expire_in: 1 week

test:
  stage: test
  script:
    - cargo test --workspace --release -- --nocapture

build:
  stage: build
  script:
    - cargo build --release --workspace
  artifacts:
    paths:
      - target/release/autopsy*
    expire_in: 1 week
```

## Azure Pipelines

```yaml
# azure-pipelines.yml
trigger:
  - main
  - develop

pool:
  vmImage: 'ubuntu-latest'

variables:
  RUST_BACKTRACE: 1

stages:
- stage: Verify
  jobs:
  - job: SynEvidVerify
    timeoutInMinutes: 30
    steps:
    - task: RustToolchain@1
      inputs:
        version: 'stable'
        components: 'rustfmt, clippy'
    
    - task: Cache@2
      inputs:
        key: 'cargo | "$(Agent.OS)" | **/Cargo.lock'
        restoreKeys: |
          cargo | "$(Agent.OS)"
        path: ~/.cargo/registry
    
    - script: |
        cargo build --release -p autopsy-cli
      displayName: 'Build autopsy-cli'
    
    - script: |
        ./target/release/autopsy baseline --persist --offline
        ./target/release/autopsy verify --strict --offline --format sarif > verify.sarif
      displayName: 'Run SynEvid Verify'
    
    - task: PublishBuildArtifacts@1
      inputs:
        pathToPublish: 'verify.sarif'
        artifactName: 'synevid-sarif'
```

## Jenkins

```groovy
// Jenkinsfile
pipeline {
    agent any
    
    tools {
        rust 'stable'
    }
    
    options {
        timeout(time: 30, unit: 'MINUTES')
    }
    
    stages {
        stage('Setup') {
            steps {
                sh 'cargo install cargo-binstall'
                sh 'cargo binstall autopsy-cli autopsy-daemon'
            }
        }
        
        stage('Verify') {
            steps {
                sh '''
                    autopsy baseline --persist --offline
                    autopsy verify --strict --offline --format sarif > verify.sarif
                '''
            }
        }
        
        stage('Test') {
            steps {
                sh 'cargo test --workspace --release -- --nocapture'
            }
        }
        
        stage('Archive') {
            steps {
                archiveArtifacts artifacts: 'verify.sarif', fingerprint: true
            }
        }
    }
    
    post {
        always {
            archiveArtifacts artifacts: 'verify.sarif', fingerprint: true
        }
    }
}
```

## Pre-commit Hooks

```yaml
# .pre-commit-config.yaml
repos:
  - repo: local
    hooks:
      - id: autopsy-verify
        name: SynEvid Verify
        entry: autopsy verify --strict --offline --format json
        language: system
        types: [python, javascript, typescript, rust, go, java, csharp]
        pass_filenames: false
        args: ["--strict", "--offline", "--format", "json"]
        require_serial: true
```

```bash
# Install
pip install pre-commit
pre-commit install
pre-commit run --all-files
```

## Custom CI Scripts

### Impact on Changed Files

```bash
#!/bin/bash
# scripts/ci-impact.sh

set -euo pipefail

# Get changed files from base branch
BASE_BRANCH="${1:-main}"
CHANGED_FILES=$(git diff --name-only "origin/${BASE_BRANCH}...HEAD" | grep -E '\.(ts|tsx|js|py|rs|go|java|cs)$' || true)

if [ -z "$CHANGED_FILES" ]; then
    echo "No relevant files changed"
    exit 0
fi

echo "Checking impact for changed files:"
echo "$CHANGED_FILES"

FAILED=0
for file in $CHANGED_FILES; do
    echo "Checking: $file"
    if ! autopsy impact -s "$file" --offline --format json | jq -e '.verdict != "FAIL"' > /dev/null; then
        echo "❌ Impact check failed for $file"
        FAILED=1
    else
        echo "✅ $file"
    fi
done

exit $FAILED
```

### Baseline in CI

```bash
#!/bin/bash
# scripts/ci-baseline.sh

set -euo pipefail

# Create baseline with persistence
autopsy baseline --persist --offline --baseline-id "ci-${GITHUB_SHA:-$(date +%s)}" \
  --format json > baseline.json

# Extract key metrics
SNAPSHOT_ID=$(jq -r '.snapshot_id' baseline.json)
FILES=$(jq -r '.files_scanned' baseline.json)
SYMBOLS=$(jq -r '.symbols_found' baseline.json)
DIGEST=$(jq -r '.blake3_digest' baseline.json)

echo "Baseline created:"
echo "  Snapshot: $SNAPSHOT_ID"
echo "  Files: $FILES"
echo "  Symbols: $SYMBOLS"
echo "  Digest: $DIGEST"

# Store for later comparison
echo "SNAPSHOT_ID=$SNAPSHOT_ID" >> $GITHUB_ENV
echo "BASELINE_DIGEST=$DIGEST" >> $GITHUB_ENV
```

## Artifact Management

### SARIF Upload

```yaml
# GitHub
- uses: github/codeql-action/upload-sarif@v2
  with:
    sarif_file: verify.sarif
    category: synevid-verify

# GitLab
artifacts:
  reports:
    sast: verify.sarif

# Azure
- task: PublishBuildArtifacts@1
  inputs:
    pathToPublish: 'verify.sarif'
    artifactName: 'synevid-sarif'
```

### Evidence Artifacts

```yaml
# Archive evidence
- name: Archive Evidence
  if: always()
  uses: actions/upload-artifact@v4
  with:
    name: synevid-evidence
    path: |
      baseline.json
      impact.json
      verify.sarif
      *.log
    retention-days: 30
```

## Performance Optimization

### Caching

```yaml
# Cargo cache
- uses: Swatinem/rust-cache@v2
  with:
    key: "${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}"
    shared: false

# Autopsy cache
- name: Cache Autopsy
  uses: actions/cache@v4
  with:
    path: |
      ~/.autopsy/cache
      target/
    key: autopsy-${{ runner.os }}-${{ hashFiles('**/Cargo.lock') }}
```

### Parallel Jobs

```yaml
jobs:
  verify:
    strategy:
      matrix:
        command: [verify, impact, diff]
    steps:
      - run: autopsy ${{ matrix.command }} --offline --format json
```

## Secrets Management

### Required Secrets

| Secret | Description | Required |
|----------|-------------|----------|
| `SYNEVID_LOOPBACK_TOKEN` | Daemon auth token | Yes (if daemon) |
| `SYNEVID_DAEMON_HOST` | Daemon hostname | If remote daemon |
| `SYNEVID_DAEMON_PORT` | Daemon port | If remote daemon |

```yaml
# GitHub Secrets
env:
  SYNEVID_LOOPBACK_TOKEN: ${{ secrets.SYNEVID_LOOPBACK_TOKEN }}
  SYNEVID_DAEMON_HOST: ${{ secrets.SYNEVID_DAEMON_HOST }}
  SYNEVID_DAEMON_PORT: ${{ secrets.SYNEVID_DAEMON_PORT }}
```

### Token Generation

```bash
# Generate token for CI
autopsy-daemon --generate-token
# Output: abc123def456...

# Store in GitHub Secrets as SYNEVID_LOOPBACK_TOKEN
```

## Troubleshooting CI

### Common Issues

| Issue | Solution |
|-------|----------|
| `daemon unavailable` | Start daemon in CI: `autopsy-daemon --no-auth &` |
| `permission denied` | `chmod +x ./target/release/autopsy` |
| `snapshot not found` | Run `autopsy baseline --persist` first |
| `offline mode failed` | Ensure `--offline` flag and no daemon needed |
| `out of memory` | Reduce parallel jobs: `--jobs 2` |

### Debug Mode

```yaml
- name: Debug Verify
  run: |
    RUST_LOG=debug autopsy verify --strict --offline --format json 2>&1 | tee verify.log
    cat verify.log
```

## Performance Benchmarks in CI

```yaml
# .github/workflows/benchmark.yml
name: Benchmarks
on:
  schedule:
    - cron: '0 3 * * 0'  # Weekly
  workflow_dispatch:

jobs:
  benchmark:
    runs-on: ubuntu-latest
    timeout-minutes: 60
    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 0 }
      
      - name: Build Release
        run: cargo build --release -p autopsy-cli -p autopsy-daemon
      
      - name: Run Benchmarks
        run: |
          cargo run --release -p syn-bench -- \
            --repo . \
            --binary ./target/release/autopsy-daemon \
            --results-dir ./benchmarks/results
      
      - name: Upload Results
        uses: actions/upload-artifact@v4
        with:
          name: benchmark-results
          path: benchmarks/results/
```

---

*Document Version: 1.0*
*Last Updated: 2024-11-24*
*Next Review: 2025-05-24*