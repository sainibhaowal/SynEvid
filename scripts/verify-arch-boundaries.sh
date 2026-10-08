#!/usr/bin/env bash
set -euo pipefail

# Invariant 1: Core crates (crates/*) must NEVER declare LLM, networking, or presentation/MCP dependencies.
echo "Verifying architectural dependency boundaries across core crates..."

FORBIDDEN_DEPS="openai|anthropic|gemini|llm|mcp-server|langchain|reqwest|hyper|tokio-net|curl|socket2|tokio-tungstenite|ureq|surf"

for cargo_file in crates/*/Cargo.toml; do
    if [ -f "$cargo_file" ]; then
        if grep -E -i "($FORBIDDEN_DEPS)" "$cargo_file" >/dev/null; then
            echo "ERROR: Architectural dependency boundary violated in $cargo_file!"
            echo "Core crates must never depend on external LLM services, networking, or presentation servers."
            exit 1
        fi
    fi
done

# Invariant 2: Zero Code Execution (FR-026) - Core crates must never spawn sub-processes or run compilers.
echo "Verifying Zero Code Execution invariant (no sub-process execution in core crates)..."
FORBIDDEN_EXEC_CALLS="std::process::Command|process::Command|libc::exec|libc::fork"
if grep -rnE "($FORBIDDEN_EXEC_CALLS)" crates/ >/dev/null; then
    echo "ERROR: Zero Code Execution invariant violated! Detected process execution in core crates:"
    grep -rnE "($FORBIDDEN_EXEC_CALLS)" crates/
    exit 1
fi

# Invariant 3: Zero Network Socket Execution (FR-028) - Core crates must never open network sockets.
echo "Verifying Zero Network Socket Execution invariant..."
FORBIDDEN_NET_CALLS="std::net::TcpStream|std::net::UdpSocket|std::net::TcpListener"
if grep -rnE "($FORBIDDEN_NET_CALLS)" crates/ >/dev/null; then
    echo "ERROR: Zero Network Socket invariant violated! Detected socket usage in core crates:"
    grep -rnE "($FORBIDDEN_NET_CALLS)" crates/
    exit 1
fi

echo "Architectural boundary verification PASSED: Core crates remain clean, offline, and deterministic."
