#!/usr/bin/env bash
set -euo pipefail

# Invariant: Core crates (crates/*) must NEVER declare LLM or presentation/MCP dependencies.
echo "Verifying architectural boundaries across core crates..."

FORBIDDEN_PATTERNS="openai|anthropic|gemini|llm|mcp-server|langchain|reqwest|tokio-tungstenite"

for cargo_file in crates/*/Cargo.toml; do
    if [ -f "$cargo_file" ]; then
        if grep -E -i "($FORBIDDEN_PATTERNS)" "$cargo_file" >/dev/null; then
            echo "ERROR: Architectural boundary violated in $cargo_file!"
            echo "Core crates must never depend on external LLM services or presentation servers."
            exit 1
        fi
    fi
done

echo "Architectural boundary verification PASSED: Core crates remain clean and deterministic."
