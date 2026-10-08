#!/usr/bin/env bash
set -euo pipefail

export TMPDIR="${TMPDIR:-/home/ravi/.cache/tmp}"
mkdir -p "${TMPDIR}"

echo "================================================================="
echo "Executing Synevid Pillar A: Universal Hardware & Multi-Arch Gate"
echo "================================================================="

echo "1. Checking cross-platform CRLF vs LF line ending determinism..."
cargo test -p autopsy-repo -- test_cross_platform_crlf_lf_determinism -- --nocapture

echo "2. Checking Windows path separator normalization (INV-CROSS-001)..."
cargo test -p autopsy-repo -- test_cross_platform_path_separator_normalization -- --nocapture

echo "3. Checking concurrent atomic cache write resilience..."
cargo test -p autopsy-storage -- test_content_addressed_cache -- --nocapture

echo "4. Running E2E multi-architecture & cross-platform suite..."
cargo test -p autopsy-tests --test e2e_cross_platform_and_multi_arch -- --nocapture

echo "5. Verifying BLAKE3 100-run snapshot determinism invariant..."
./scripts/verify-determinism.sh

echo "================================================================="
echo "SUCCESS: Pillar A Universal Hardware & Determinism Gate PASSED."
echo "================================================================="
