#!/usr/bin/env bash
set -e

# Resolve repository root
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

echo "=== SynEvid Desktop Application Launcher ==="

# If running inside WSL, delegate to Windows Desktop host
if grep -qi microsoft /proc/version 2>/dev/null; then
    echo "WSL environment detected: launching native Windows Desktop host..."
    cmd.exe /c "scripts\\run-desktop.bat"
    exit 0
fi

# 1. Ensure Rust Daemon binary is built
if [ ! -f "target/debug/synevid-ui" ]; then
    echo "Building synevid-ui daemon..."
    cargo build -p synevid-ui
fi

# 2. Launch Next.js Cockpit if not already running on port 3000
if ! curl -s http://localhost:3000 >/dev/null 2>&1; then
    echo "Starting Web Cockpit on http://localhost:3000..."
    (cd apps/web && npx next dev -p 3000) &
    COCKPIT_PID=$!
    sleep 3
fi

# 3. Launch Electron Desktop Shell
echo "Launching SynEvid Native Desktop Shell..."
cd "$REPO_ROOT/apps/desktop-electron"
npx electron src/main.js
