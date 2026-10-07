# Architecture overview
The Rust core owns deterministic analysis. TypeScript MCP and any GUI are adapters. Python is research-only.

Dependency direction: presentation/integration -> core domain interfaces. Core crates must not import MCP/web/benchmark code.
