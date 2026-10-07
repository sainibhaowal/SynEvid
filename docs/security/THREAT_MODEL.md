# Threat model - foundation
Repositories are untrusted input.

## Threats
Parser/compiler resource exhaustion; malicious symlinks/path traversal; untrusted build hooks; secret leakage; agent-triggered tool abuse; cache poisoning; dependency compromise.

## Baseline controls
Local/offline execution; no source mutation; no project build scripts by default; file/process/time limits; repository/cache root enforcement; minimal logging of source; narrow read-only MCP tools; locked dependencies; canonical cache keys with version/config identity.
