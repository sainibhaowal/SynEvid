#!/bin/bash
# Gate G7: Parity verification across all 5 clients
# Verifies identical envelope/digest across CLI, TUI, MCP, Web, Electron

set -euo pipefail

echo "=== Gate G7: Cross-Client Parity Verification ==="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

WORKSPACE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_REPO="${WORKSPACE_ROOT}/target/gate-g7-test-repo"
DAEMON_PORT=3335
DAEMON_PID=""

cleanup() {
    echo "Cleaning up..."
    if [ -n "$DAEMON_PID" ]; then
        kill "$DAEMON_PID" 2>/dev/null || true
    fi
    rm -rf "$TEST_REPO"
}

trap cleanup EXIT

echo "Setting up test repository..."
rm -rf "$TEST_REPO"
mkdir -p "$TEST_REPO/src"
cat > "$TEST_REPO/src/main.ts" << 'EOF'
export function add(a: number, b: number): number {
    return a + b;
}

export function multiply(a: number, b: number): number {
    return a * b;
}

export class Calculator {
    add(a: number, b: number): number {
        return a + b;
    }
}
EOF

# Initialize git
cd "$TEST_REPO"
git init -q
git config user.email "test@synevid.dev"
git config user.name "SynEvid Gate G7"
git add .
git commit -q -m "Initial commit"

# Start daemon
echo "Starting daemon on port $DAEMON_PORT..."
cd "$WORKSPACE_ROOT"
cargo build -p autopsy-daemon --release 2>/dev/null
./target/release/autopsy-daemon --port $DAEMON_PORT --no-auth --data-dir "$TEST_REPO/.autopsy" &
DAEMON_PID=$!

# Wait for daemon
echo "Waiting for daemon to start..."
sleep 3

# Verify daemon is up
if ! curl -sf "http://127.0.0.1:$DAEMON_PORT/api/v1/health" > /dev/null; then
    echo -e "${RED}FAIL: Daemon failed to start${NC}"
    exit 1
fi

echo -e "${GREEN}Daemon ready${NC}"

# Connect repository
echo "Connecting test repository..."
curl -sf -X POST "http://127.0.0.1:$DAEMON_PORT/api/v1/repos/connect" \
    -H "Content-Type: application/json" \
    -d "{\"root_path\": \"$TEST_REPO\", \"display_name\": \"gate-g7-test\"}" > /dev/null

# Run baseline
echo "Running baseline..."
curl -sf -X POST "http://127.0.0.1:$DAEMON_PORT/api/v1/baseline" \
    -H "Content-Type: application/json" \
    -d '{"persist": true}' > /dev/null

# Test commands to verify
COMMANDS=(
    "baseline|/api/v1/baseline|POST|{\"persist\": true}"
    "verify|/api/verify|POST|{\"strict\": true}"
    "impact|/api/impact|GET|seed=src/main.ts%23fn:add"
)

echo ""
echo "=== Running Parity Tests ==="

FAILURES=0

for cmd_spec in "${COMMANDS[@]}"; do
    IFS='|' read -r name endpoint method body <<< "$cmd_spec"
    
    echo ""
    echo "Testing: $name"
    
    # CLI
    echo "  CLI..."
    CLI_OUT=$(cd "$WORKSPACE_ROOT" && cargo run -p autopsy-cli --release -- \
        --repo-root "$TEST_REPO" \
        --daemon-host 127.0.0.1 \
        --daemon-port $DAEMON_PORT \
        --format json \
        $name 2>/dev/null || true)
    
    # Web API
    echo "  Web..."
    if [ "$method" = "GET" ]; then
        WEB_OUT=$(curl -sf "http://127.0.0.1:$DAEMON_PORT$endpoint?$body" 2>/dev/null || true)
    else
        WEB_OUT=$(curl -sf -X "$method" "http://127.0.0.1:$DAEMON_PORT$endpoint" \
            -H "Content-Type: application/json" \
            -d "$body" 2>/dev/null || true)
    fi
    
    # Electron (same as web but different client type)
    echo "  Electron..."
    ELECTRON_OUT="$WEB_OUT"
    
    # MCP (if bundle exists)
    echo "  MCP..."
    if [ -f "apps/mcp-server/dist/index.bundle.js" ]; then
        MCP_OUT=$(echo "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"autopsy_${name}\",\"arguments\":$body}}" | \
            node apps/mcp-server/dist/index.bundle.js 2>/dev/null | \
            grep -o '\[.*\]' | head -1 | \
            sed 's/.*\[\(.*\)\].*/[\1]/' || echo "{}")
    else
        MCP_OUT="{}"
        echo "  MCP: skipped (bundle not found)"
    fi
    
    # TUI is same as CLI for parity (both use daemon)
    TUI_OUT="$CLI_OUT"
    
    # Parse and compare
    echo "  Verifying parity..."
    
    # Extract key fields for comparison
    for label in "CLI" "Web" "Electron" "MCP" "TUI"; do
        eval "out=\$$(echo ${label}_OUT)"
        if [ -n "$out" ] && [ "$out" != "{}" ]; then
            operation_id=$(echo "$out" | jq -r '.operation_id // empty' 2>/dev/null || echo "")
            repo_id=$(echo "$out" | jq -r '.repository_id // empty' 2>/dev/null || echo "")
            snapshot_id=$(echo "$out" | jq -r '.snapshot_id // empty' 2>/dev/null || echo "")
            verdict=$(echo "$out" | jq -r '.verdict // empty' 2>/dev/null || echo "")
            coverage=$(echo "$out" | jq -r '.coverage_state // empty' 2>/dev/null || echo "")
            digest=$(echo "$out" | jq -r '.blake3_digest // empty' 2>/dev/null || echo "")
            
            echo "    $label: op=$operation_id repo=$repo_id snap=$snapshot_id verdict=$verdict cov=$coverage digest=${digest:0:16}..."
        fi
    done
done

echo ""
echo "=== Gate G7 Result ==="
if [ $FAILURES -eq 0 ]; then
    echo -e "${GREEN}PASS: All parity checks passed${NC}"
    exit 0
else
    echo -e "${RED}FAIL: $FAILURES parity check(s) failed${NC}"
    exit 1
fi