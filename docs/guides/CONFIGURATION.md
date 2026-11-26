# SynEvid Configuration Reference

**Version**: 1.0
**Date**: 2024-11-24
**Applies To**: autopsy-daemon, autopsy-cli, synevid-web, synevid-electron

## Configuration Hierarchy

Configuration is loaded in priority order (highest first):

1. **CLI flags** (highest priority)
2. **Environment variables** (`SYNEVID_*`)
3. **Config file** (`autopsy.toml` in repo root or `--config`)
4. **Defaults** (lowest priority)

## Daemon Configuration (`autopsy.toml`)

```toml
# autopsy.toml - Daemon configuration
# All sections optional; defaults shown

[daemon]
# Network
host = "127.0.0.1"           # Bind address
port = 3333                  # Listen port
enable_websocket = true      # Enable WebSocket support
max_ws_frame_size = 1048576  # Max WebSocket frame (1MB)

# Timeouts
read_timeout_secs = 30       # Request read timeout
write_timeout_secs = 30      # Response write timeout
idle_timeout_secs = 60       # Keep-alive timeout
request_timeout_secs = 60    # Total request timeout

# Limits
max_body_size = 10485760     # Max request body (10MB)
max_connections = 1000       # Max concurrent connections
max_ws_connections = 100     # Max WebSocket connections

# Storage
data_dir = "~/.autopsy"      # Daemon data directory
repo_root = "/path/to/repo"  # Default repository (optional)
managed_runtime = false      # Daemon-managed repos only

# Authentication
no_auth = false              # Disable loopback auth (DEV ONLY)
loopback_token = ""          # Override token (env takes precedence)

# Logging
log_level = "info"           # trace, debug, info, warn, error
json_logs = false            # JSON vs text format

# Compression
compression = true           # Enable gzip compression
compression_level = 6        # gzip level (1-9)

# Trusted proxies (for X-Forwarded-For)
trusted_proxies = []         # CIDR ranges, e.g. ["10.0.0.0/8"]
```

### Environment Variable Mapping

| Config Key | Environment Variable | Example |
|------------|---------------------|---------|
| `daemon.port` | `SYNEVID_PORT` | `3333` |
| `daemon.host` | `SYNEVID_HOST` | `0.0.0.0` |
| `daemon.data_dir` | `SYNEVID_DATA_DIR` | `/var/lib/synevid` |
| `daemon.managed_runtime` | `SYNEVID_MANAGED_RUNTIME` | `true` |
| `daemon.no_auth` | `SYNEVID_NO_AUTH` | `true` |
| `daemon.log_level` | `SYNEVID_LOG_LEVEL` | `debug` |
| `daemon.loopback_token` | `SYNEVID_LOOPBACK_TOKEN` | `abc123...` |

## CLI Configuration (`autopsy.toml`)

```toml
# autopsy.toml - CLI configuration (repo root)

[cli]
# Default format for all commands
format = "json"              # json, text, sarif

# Default repository root (relative to config file)
repo_root = "."              # Path to repository

# Default output
color = true                 # Color output
progress = true              # Progress bars

# Performance
parallel_jobs = 4            # Parallel analysis jobs
cache_dir = ".autopsy/cache" # Cache directory

# Persistence
persist = false              # Persist results by default

# Offline mode
offline = false              # Run without daemon

# Output
output_dir = "."             # Output directory for reports
report_format = "json"       # json, markdown, html, sarif, bundle
```

### CLI Environment Variables

| Config Key | Environment Variable | Example |
|------------|---------------------|---------|
| `cli.repo_root` | `AUTOPSY_REPO_ROOT` | `/path/to/repo` |
| `cli.format` | `AUTOPSY_FORMAT` | `json` |
| `cli.offline` | `AUTOPSY_OFFLINE` | `true` |
| `cli.parallel_jobs` | `AUTOPSY_JOBS` | `8` |

## Repository Configuration (`.autopsy/invariants.yml`)

```yaml
# .autopsy/invariants.yml - Project invariants

# Invariant rules
invariants:
  # Architecture layers
  - id: CLEAN_ARCHITECTURE_LAYERS
    description: "Layers must not skip levels"
    severity: error
    rule: |
      no_dependency(presentation, domain)
      no_dependency(presentation, infrastructure)
      no_dependency(domain, infrastructure)

  # Circular dependencies
  - id: NO_CIRCULAR_DEPS
    description: "No circular dependencies allowed"
    severity: error
    rule: |
      no_cycles(imports)

  # API compatibility
  - id: API_COMPATIBILITY
    description: "Public API must not break"
    severity: error
    rule: |
      no_breaking_changes(public_api)

  # Custom invariants
  - id: NO_GOD_CLASSES
    description: "Classes must not exceed 500 lines"
    severity: warn
    rule: |
      max_lines_per_class(500)

  - id: TEST_COVERAGE
    description: "Minimum 80% coverage for domain layer"
    severity: warn
    rule: |
      min_coverage(domain, 0.80)

# Gate configuration
gates:
  # Fail on any error severity
  fail_on: [error]
  # Also fail on warn in strict mode
  strict_fail_on: [error, warn]

# Language adapters
adapters:
  typescript:
    enabled: true
    config:
      tsconfig: "tsconfig.json"
      exclude: ["**/*.test.ts", "**/*.spec.ts", "**/node_modules/**"]
  
  python:
    enabled: true
    config:
      pyproject: "pyproject.toml"
      exclude: ["**/test_*.py", "**/*_test.py", "**/__pycache__/**"]
  
  rust:
    enabled: true
    config:
      cargo: "Cargo.toml"
      exclude: ["**/tests/**", "**/benches/**"]
  
  go:
    enabled: true
    config:
      gomod: "go.mod"
      exclude: ["**/*_test.go"]
  
  java:
    enabled: true
    config:
      build_gradle: "build.gradle"
      exclude: ["**/test/**", "**/*Test.java"]
  
  csharp:
    enabled: true
    config:
      csproj: "*.csproj"
      exclude: ["**/bin/**", "**/obj/**", "**/*Tests.cs"]

# Analysis settings
analysis:
  # Parallelism
  parallel_files: 4
  parallel_symbols: 8
  
  # Limits
  max_file_bytes: 1048576      # 1MB
  max_symbols_per_file: 10000
  max_edges_per_symbol: 1000
  
  # Coverage
  unknown_as_failure: false    # UNKNOWN -> FAIL in strict
  partial_as_failure: false    # PARTIAL -> FAIL in strict
  
  # Increments
  incremental: true
  increment_threshold: 0.1     # Re-analyze if >10% changed

# Gate thresholds
thresholds:
  max_file_lines: 1000
  max_function_lines: 100
  max_class_lines: 500
  max_cyclomatic_complexity: 20
  max_nesting_depth: 5
  max_parameters: 7

# Output
output:
  # Report formats
  formats: [json, markdown, html]
  
  # Evidence
  include_evidence: true
  evidence_detail: full        # full, summary, none
  
  # Digests
  include_digests: true
  
  # Reports
  reports_dir: ".autopsy/reports"
  max_reports: 100
```

## Daemon Runtime Configuration

### Runtime Flags

```bash
autopsy-daemon [OPTIONS]

Options:
  --port <PORT>              Listen port (default: 3333)
  --host <HOST>              Bind host (default: 127.0.0.1)
  --data-dir <DIR>           Data directory (default: ~/.autopsy)
  --repo-root <DIR>          Repository root (managed mode)
  --managed-runtime          Enable managed runtime mode
  --no-auth                  Disable authentication (DEV ONLY)
  --log-level <LEVEL>        trace|debug|info|warn|error
  --json-logs                JSON structured logs
  --config <FILE>            Config file path
  --generate-token           Generate loopback token and exit
  --no-websocket             Disable WebSocket
  --max-connections <N>      Max connections (default: 1000)
```

### Environment Variables

```bash
# Core
export SYNEVID_PORT=3333
export SYNEVID_HOST=127.0.0.1
export SYNEVID_DATA_DIR=/var/lib/synevid
export SYNEVID_MANAGED_RUNTIME=true
export SYNEVID_NO_AUTH=false

# Auth
export SYNEVID_LOOPBACK_TOKEN=your-token-here

# Logging
export SYNEVID_LOG_LEVEL=debug
export SYNEVID_JSON_LOGS=true

# Daemon
export SYNEVID_DAEMON_HOST=127.0.0.1
export SYNEVID_DAEMON_PORT=3333
export SYNEVID_LOOPBACK_TOKEN=your-token
```

## Client Configuration

### Web UI (apps/web)

```typescript
// apps/web/.env.local
NEXT_PUBLIC_API_URL=http://127.0.0.1:3333
NEXT_PUBLIC_WS_URL=ws://127.0.0.1:3333
NEXT_PUBLIC_OTEL_ENDPOINT=http://localhost:4318
NEXT_PUBLIC_APP_VERSION=0.6.0
```

### Electron (apps/desktop-electron)

```javascript
// apps/desktop-electron/.env
SYNEVID_DAEMON_URL=http://127.0.0.1:3333
SYNEVID_LOOPBACK_TOKEN=your-token
AUTOPSY_DAEMON_PATH=/usr/local/bin/autopsy-daemon
```

### MCP Server (apps/mcp-server)

```typescript
// apps/mcp-server/.env
SYNEVID_DAEMON_URL=http://127.0.0.1:3333
SYNEVID_LOOPBACK_TOKEN=your-token
MCP_PORT=3000
```

## Configuration Validation

### Validate Config

```bash
# Validate all configs
autopsy doctor --check-config

# Validate specific file
autopsy doctor --config autopsy.toml
autopsy doctor --config .autopsy/invariants.yml
```

### Schema Validation

```bash
# Validate against JSON schemas
python3 scripts/validate-schemas.py \
  --config autopsy.toml \
  --invariants .autopsy/invariants.yml \
  --schemas schemas/
```

## Configuration Examples

### Minimal (Development)

```toml
# autopsy.toml
[daemon]
port = 3333
no_auth = true
log_level = "debug"
```

### Production

```toml
# autopsy.toml
[daemon]
port = 3333
host = "127.0.0.1"
data_dir = "/var/lib/synevid"
managed_runtime = false
no_auth = false
log_level = "info"
json_logs = true
compression = true
max_body_size = 10485760
max_connections = 1000
read_timeout_secs = 30
write_timeout_secs = 30
idle_timeout_secs = 60
request_timeout_secs = 60
compression = true
```

### CI/CD

```toml
# autopsy.toml (CI)
[daemon]
port = 3333
host = "127.0.0.1"
data_dir = "/tmp/.autopsy"
managed_runtime = true
no_auth = true
log_level = "warn"
json_logs = true

[cli]
format = "json"
offline = true
parallel_jobs = 2
persist = false
```

### Multi-Repo Workspace

```toml
# autopsy.toml (workspace root)
[workspace]
members = [
  "packages/*",
  "services/*",
]

[daemon]
managed_runtime = true
data_dir = "/workspace/.autopsy"

[workspace.registry]
backend = "sqlite"
path = "/workspace/.autopsy/workspace.db"
```

## Configuration Validation

### Startup Validation

The daemon validates config on startup:

```rust
// Checks performed:
1. Port availability
2. Data directory writable
3. Config syntax valid
4. Schema version compatible
5. Required fields present
6. Path traversal checks
7. Symlink safety
8. Resource limits reasonable
```

### Validation Errors

| Error | Cause | Fix |
|-------|-------|-----|
| `port already in use` | Port 3333 busy | Change port or stop conflicting process |
| `data_dir not writable` | Permissions | `chmod 700 .autopsy` |
| `config field unknown` | Typo in config | Check field name |
| `config field invalid` | Wrong type | Match expected type |
| `path traversal detected` | `..` in path | Use absolute paths |

## Configuration Best Practices

1. **Use environment variables for secrets** - Never commit tokens
2. **Version control config** - Track `autopsy.toml` and `invariants.yml`
3. **Separate environments** - Use different configs for dev/staging/prod
4. **Document custom invariants** - Comment why rules exist
5. **Test config changes** - Run `autopsy doctor` after changes
6. **Monitor resource limits** - Adjust `max_*` based on repo size
6. **Audit config regularly** - Check for drift

## Configuration Migration

### 0.5.x → 0.6.0

| Old Key | New Key | Action |
|---------|---------|--------|
| `daemon.ui_port` | Removed | Use `--web-port` flag |
| `daemon.synevid_ui_port` | Removed | Use `--web-port` flag |
| `ui.port` | Removed | Use `autopsy-daemon --web-port` |
| `synevid_ui.port` | Removed | Use `autopsy-daemon --web-port` |

Run `autopsy doctor --migrate-config` for automatic migration.

---

*Document Version: 1.0*
*Last Updated: 2024-11-24*
*Next Review: 2025-05-24*