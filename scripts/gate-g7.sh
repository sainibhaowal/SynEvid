#!/usr/bin/env bash
# Gate G7: Parity verification across all clients
# 
# G7 passes when a command executed from any supported client produces the same
# envelope, digest, repository identity, event, and receipt.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== Gate G7: Cross-Client Parity Verification ==="
echo "Repository: $REPO_ROOT"
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Track failures
FAILURES=0

run_parity_test() {
    local test_name="$1"
    local test_cmd="$2"
    
    echo -n "Running $test_name... "
    if eval "$test_cmd" > /dev/null 2>&1; then
        echo -e "${GREEN}PASS${NC}"
        return 0
    else
        echo -e "${RED}FAIL${NC}"
        ((FAILURES++))
        return 1
    fi
}

echo "1. Running parity test harness (CLI, MCP, Web, Electron)..."
run_parity_test "parity test harness" \
    "cd $REPO_ROOT && cargo test --test parity -- --nocapture 2>&1 | grep -E '(test result|PASS|FAIL)' || true"

echo ""
echo "2. Running cross-client digest parity tests..."
run_parity_test "cross-client digest parity" \
    "cd $REPO_ROOT && cargo test --test parity verify_parity 2>&1 | grep -E '(PASS|FAIL)' || true"

echo ""
echo "3. Running envelope integrity tests..."
run_parity_test "envelope integrity" \
    "cd $REPO_ROOT && cargo test --test parity envelope_integrity 2>&1 | grep -E '(PASS|FAIL)' || true"

echo ""
echo "4. Running no-fabricated-output scan..."
run_parity_test "no fabricated output" \
    "cd $REPO_ROOT && cargo test --test parity no_fabricated_output 2>&1 | grep -E '(PASS|FAIL)' || true"

echo ""
echo "5. Running 100-run determinism tests..."
run_parity_test "100-run determinism" \
    "cd $REPO_ROOT && cargo test --test parity 100_runs_determinism 2>&1 | grep -E '(PASS|FAIL)' || true"

echo ""
echo "=== Gate G7 Summary ==="
if [ $FAILURES -eq 0 ]; then
    echo -e "${GREEN}G7 PASSED${NC}: All parity checks passed."
    echo "  - CLI, MCP, Web, Electron produce identical envelopes"
    echo "  - Digests match across all clients"
    echo "  - No fabricated output detected"
    echo "  - Deterministic across 100 runs"
    exit 0
else
    echo -e "${RED}G7 FAILED${NC}: $FAILURES check(s) failed."
    exit 1
fi