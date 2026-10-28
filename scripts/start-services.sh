#!/usr/bin/env bash
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Kill existing instances
pkill -f "synevid-ui --port 3333" 2>/dev/null || true
pkill -f "next dev -p 3000" 2>/dev/null || true
sleep 1

# Start synevid-ui
nohup ./target/debug/synevid-ui --port 3333 > /tmp/synevid-ui.log 2>&1 &
echo $! > /tmp/synevid-ui.pid

# Start Next.js Cockpit
cd "$REPO_ROOT/apps/web"
nohup npx next dev -p 3000 > /tmp/next-dev.log 2>&1 &
echo $! > /tmp/next-dev.pid

sleep 2
echo "Services started. Testing health..."
curl -s http://127.0.0.1:3333/api/v1/health && echo ""
curl -s -I http://127.0.0.1:3000 | head -n 1
