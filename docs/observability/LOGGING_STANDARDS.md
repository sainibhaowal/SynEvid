# SynEvid Logging Standards

**Version**: 1.0
**Date**: 2024-11-24
**Applies To**: All SynEvid components (daemon, CLI, Web, Electron, MCP)

## Logging Principles

1. **Structured Only**: All logs must be structured JSON
2. **No Secrets**: Never log tokens, paths, or user data
3. **Context Rich**: Include correlation IDs, operation IDs
4. **Consistent Fields**: Standard fields across all components
5. **Sampling**: High-volume logs must be sampled

## Log Format

### Standard Fields (Required)

```json
{
  "timestamp": "2024-11-24T14:30:00.123Z",
  "level": "INFO",
  "service": "autopsy-daemon",
  "version": "0.6.0",
  "trace_id": "abc123def456",
  "span_id": "span789",
  "operation_id": "op_abc123",
  "client_id": "cli:default",
  "client_type": "cli",
  "message": "Baseline completed",
  "fields": {
    "repo_id": "abc123",
    "snapshot_id": "snap_456",
    "duration_ms": 1234,
    "files_scanned": 1500,
    "symbols_found": 45000
  }
}
```

### Field Definitions

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `timestamp` | ISO8601 | ✅ | RFC3339 UTC with milliseconds |
| `level` | Enum | ✅ | TRACE, DEBUG, INFO, WARN, ERROR |
| `service` | String | ✅ | Component name (daemon, cli, web, etc.) |
| `version` | String | ✅ | SynEvid version |
| `trace_id` | String | ✅ | Distributed trace ID (W3C) |
| `span_id` | String | ✅ | Current span ID |
| `operation_id` | String | ✅ | BLAKE3 of canonical request |
| `client_id` | String | ✅ | Client identifier |
| `client_type` | Enum | ✅ | cli, tui, mcp, web, electron, daemon |
| `message` | String | ✅ | Human-readable message |
| `fields` | Object | ✅ | Structured key-value data |

### Log Levels

| Level | Use Case | Sampling |
|-------|----------|----------|
| TRACE | Per-request flow, variable values | 1% |
| DEBUG | Decision points, state changes | 10% |
| INFO | Operations start/end, milestones | 100% |
| WARN | Recoverable issues, fallbacks | 100% |
| ERROR | Failures, data loss, invariants | 100% |

## Component-Specific Standards

### Daemon (autopsy-daemon)

```json
{
  "service": "autopsy-daemon",
  "fields": {
    "repo_id": "abc123",
    "lease_id": "lease_xyz",
    "event_seq": 42
  }
}
```

**Required Fields per Endpoint**:
- `/api/v1/baseline`: `repo_id`, `snapshot_id`, `files_scanned`, `symbols_found`, `duration_ms`
- `/api/v1/impact`: `repo_id`, `seed_symbols`, `coverage`, `verdict`, `duration_ms`
- `/api/v1/verify`: `repo_id`, `strict`, `coverage`, `verdict`, `duration_ms`
- `/api/v1/runtime/connect`: `client_id`, `client_type`, `lease_id`

### CLI (autopsy-cli)

```json
{
  "service": "autopsy-cli",
  "fields": {
    "command": "impact",
    "subcommand": "impact",
    "repo_root": "/path/to/repo",
    "exit_code": 0,
    "duration_ms": 1234
  }
}
```

**Exit Code Logging**: Always log exit code at INFO level with `exit_code` field.

### Web UI (apps/web)

```json
{
  "service": "synevid-web",
  "fields": {
    "route": "/api/impact",
    "method": "POST",
    "status": 200,
    "duration_ms": 45,
    "client_ip": "127.0.0.1"
  }
}
```

**Access Log**: Every HTTP request at INFO level.

### MCP Server (apps/mcp-server)

```json
{
  "service": "autopsy-mcp",
  "fields": {
    "tool": "autopsy_impact",
    "request_id": "req_123",
    "exit_code": 0,
    "duration_ms": 567
  }
}
```

**MCP Protocol**: Log every tool call at INFO with `tool` and `request_id`.

### Electron (apps/desktop-electron)

```json
{
  "service": "synevid-electron",
  "fields": {
    "window": "main",
    "action": "daemon_start",
    "pid": 12345
  }
}
```

**Main Process**: Log at INFO. Renderer process: DEBUG only.

## Structured Logging Implementation

### Rust (tracing)

```rust
use tracing::{info, warn, error, instrument};
use serde_json::json;

#[instrument(skip_all, fields(
    repo_id = %repo_id,
    operation_id = %op_id,
))]
async fn handle_baseline(repo_id: &str) -> Result<(), Error> {
    let op_id = generate_operation_id();
    let start = Instant::now();
    
    info!(message = "Baseline started", repo_id = %repo_id);
    
    let result = run_baseline(repo_id).await;
    let duration = start.elapsed().as_millis() as u64;
    
    match result {
        Ok(snapshot) => {
            info!(
                message = "Baseline completed",
                repo_id = %repo_id,
                operation_id = %op_id,
                duration_ms = duration,
                snapshot_id = %snapshot.id,
                files_scanned = snapshot.files_count,
                symbols_found = snapshot.symbols_count,
            );
            Ok(snapshot)
        }
        Err(e) => {
            error!(
                message = "Baseline failed",
                repo_id = %repo_id,
                operation_id = %op_id,
                duration_ms = duration,
                error = %e,
                error_code = %e.code()
            );
            Err(e)
        }
    }
}
```

### TypeScript (pino)

```typescript
import pino from 'pino';

const logger = pino({
  level: process.env.LOG_LEVEL || 'info',
  formatters: {
    level: (label) => ({ level: label }),
  },
  timestamp: () => `,"timestamp":"${new Date().toISOString()}"`,
  base: {
    service: 'synevid-web',
    version: process.env.npm_package_version,
  },
});

// Usage
logger.info({
  repo_id,
  operation_id: opId,
  duration_ms: duration,
  snapshot_id: snapshot.id,
  files_scanned: snapshot.filesCount,
  symbols_found: snapshot.symbolsCount,
}, 'Baseline completed');
```

### Log Rotation

```bash
# logrotate config (/etc/logrotate.d/synevid)
/var/log/synevid/*.log {
    daily
    rotate 30
    compress
    delaycompress
    missingok
    notifempty
    create 0640 synvid synvid
    sharedscripts
    postrotate
        systemctl reload synvid-daemon > /dev/null 2>&1 || true
    endscript
}
```

### Log Filtering

```bash
# Filter by trace_id
grep '"trace_id":"abc123"' /var/log/synevid/daemon.log | jq .

# Filter by operation
jq 'select(.fields.operation_id=="op_abc123")' daemon.log

# Filter errors
jq 'select(.level=="ERROR")' daemon.log | jq '.message'

# Aggregate by operation
jq -r '.fields.operation_id' daemon.log | sort | uniq -c | sort -rn
```

## Security Logging

### Never Log

- Loopback tokens
- File contents
- User credentials
- API keys
- PII (emails, names from git)
- Salt values
- Private keys

### Always Redact

```rust
fn sanitize_for_logging(value: &str) -> String {
    value
        .replace(|c: char| c.is_ascii_digit(), "*")
        .replace(|c: char| c.is_ascii_alphabetic(), "x")
        // Keep structure: paths, keys, structure
}
```

### Example Safe Log

```json
{
  "level": "INFO",
  "message": "Baseline completed",
  "fields": {
    "repo_id": "abc123...",
    "snapshot_id": "snap_456...",
    "duration_ms": 1234,
    "files_scanned": 1500,
    "symbols_found": 45000
  }
}
```

## Log Aggregation

### Recommended Stack

| Component | Tool | Purpose |
|-----------|------|---------|
| Collection | Vector / Fluent Bit | Ship logs |
| Storage | Loki / Elasticsearch | Store & query |
| Visualization | Grafana | Dashboards |
| Alerting | Alertmanager | Alerts |

### Grafana Dashboard Panels

1. **Request Rate**: Requests/sec by endpoint
2. **Latency**: p50/p95/p99 by operation
3. **Error Rate**: Errors/sec by type
3. **Resource Usage**: CPU, Memory, Disk, Network
4. **Business Metrics**: Files scanned, symbols, impact queries

## Compliance

| Requirement | Implementation |
|-------------|----------------|
| GDPR Art. 32 | No PII in logs, encryption at rest |
| SOC 2 Type II | Audit trail via event store |
| ISO 27001 | Structured logging, access control |
| NIST 800-53 | AU-2, AU-3, AU-6, AU-12 |

---

*Document Version: 1.0*
*Last Updated: 2024-11-24*
*Next Review: 2025-05-24*