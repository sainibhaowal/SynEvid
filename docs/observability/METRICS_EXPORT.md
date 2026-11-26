# SynEvid Metrics Export

**Version**: 1.0
**Date**: 2024-11-24
**Applies To**: autopsy-daemon, autopsy-cli, synevid-web, synevid-electron

## Overview

SynEvid exports metrics in Prometheus/OpenMetrics format and supports OpenTelemetry tracing for production observability.

## Metrics Endpoint

### Daemon Metrics

**Endpoint**: `GET http://127.0.0.1:3333/metrics`
**Format**: Prometheus/OpenMetrics text format
**Authentication**: Loopback token required (header `Authorization: Bearer <token>`)

### Available Metrics

#### Daemon Health
```prometheus
# HELP autopsy_daemon_up Daemon health status (1=up, 0=down)
# TYPE autopsy_daemon_up gauge
autopsy_daemon_up 1

# HELP autopsy_daemon_version SynEvid version info
# TYPE autopsy_daemon_version gauge
autopsy_daemon_version{version="0.6.0",rustc="1.90.0"} 1

# HELP autopsy_daemon_start_time_seconds Daemon start time (Unix timestamp)
# TYPE autopsy_daemon_start_time_seconds gauge
autopsy_daemon_start_time_seconds 1700836800
```

#### Request Metrics
```prometheus
# HELP autopsy_http_requests_total Total HTTP requests by method, endpoint, status
# TYPE autopsy_http_requests_total counter
autopsy_http_requests_total{method="POST",endpoint="/api/v1/impact",status="200"} 142
autopsy_http_requests_total{method="GET",endpoint="/api/v1/status",status="200"} 89
autopsy_http_requests_total{method="POST",endpoint="/api/v1/verify",status="400"} 3

# HELP autopsy_http_request_duration_seconds HTTP request latency
# TYPE autopsy_http_request_duration_seconds histogram
autopsy_http_request_duration_seconds_bucket{method="POST",endpoint="/api/v1/impact",le="0.1"} 89
autopsy_http_request_duration_seconds_bucket{method="POST",endpoint="/api/v1/impact",le="0.5"} 134
autopsy_http_request_duration_seconds_bucket{method="POST",endpoint="/api/v1/impact",le="1.0"} 140
autopsy_http_request_duration_seconds_bucket{method="POST",endpoint="/api/v1/impact",le="+Inf"} 142
autopsy_http_request_duration_seconds_sum{method="POST",endpoint="/api/v1/impact"} 45.2
autopsy_http_request_duration_seconds_count{method="POST",endpoint="/api/v1/impact"} 142
```

#### Business Metrics
```prometheus
# HELP autopsy_baseline_total Total baseline operations
# TYPE autopsy_baseline_total counter
autopsy_baseline_total{status="success"} 156
autopsy_baseline_total{status="error"} 2

# HELP autopsy_baseline_duration_seconds Baseline duration
# TYPE autopsy_baseline_duration_seconds histogram
autopsy_baseline_duration_seconds_bucket{le="1.0"} 89
autopsy_baseline_duration_seconds_bucket{le="5.0"} 145
autopsy_baseline_duration_seconds_bucket{le="30.0"} 156
autopsy_baseline_duration_seconds_bucket{le="+Inf"} 158

# HELP autopsy_impact_total Total impact analyses
# TYPE autopsy_impact_total counter
autopsy_impact_total{status="success",coverage="verified"} 89
autopsy_impact_total{status="success",coverage="partial"} 12
autopsy_impact_total{status="error"} 3

# HELP autopsy_verify_total Total verification runs
# TYPE autopsy_verify_total counter
autopsy_verify_total{status="pass",coverage="verified"} 67
autopsy_verify_total{status="fail",coverage="verified"} 12
autopsy_verify_total{status="unknown"} 8
```

#### Resource Metrics
```prometheus
# HELP autopsy_memory_bytes Current memory usage
# TYPE autopsy_memory_bytes gauge
autopsy_memory_bytes{type="rss"} 524288000
autopsy_memory_bytes{type="heap"} 209715200

# HELP autopsy_cpu_seconds_total Total CPU time
# TYPE autopsy_cpu_seconds_total counter
autopsy_cpu_seconds_total 1234.56

# HELP autopsy_storage_bytes Storage size
# TYPE autopsy_storage_bytes gauge
autopsy_storage_bytes{type="sqlite"} 104857600
autopsy_storage_bytes{type="objects"} 52428800
```

#### Lease Metrics
```prometheus
# HELP autopsy_active_leases Current active leases
# TYPE autopsy_active_leases gauge
autopsy_active_leases{client_type="cli"} 2
autopsy_active_leases{client_type="web"} 1
autopsy_active_leases{client_type="mcp"} 1

# HELP autopsy_lease_duration_seconds Lease duration
# TYPE autopsy_lease_duration_seconds histogram
autopsy_lease_duration_seconds_bucket{le="60"} 2
autopsy_lease_duration_seconds_bucket{le="3600"} 4
```

#### Storage Metrics
```prometheus
# HELP autopsy_storage_operations_total Storage operations
# TYPE autopsy_storage_operations_total counter
autopsy_storage_operations_total{operation="read",status="success"} 4567
autopsy_storage_operations_total{operation="write",status="success"} 1234
autopsy_storage_operations_total{operation="read",status="error"} 2

# HELP autopsy_storage_latency_seconds Storage operation latency
# TYPE autopsy_storage_latency_seconds histogram
autopsy_storage_latency_seconds_bucket{operation="read",le="0.01"} 4500
autopsy_storage_latency_seconds_bucket{operation="write",le="0.01"} 1100
```

## OpenTelemetry Export

### Configuration

```toml
# autopsy.toml
[observability]
# Enable OpenTelemetry export
otel_enabled = true

# OTLP HTTP endpoint (e.g., collector)
otel_endpoint = "http://localhost:4318/v1/metrics"

# Export interval (seconds)
otel_export_interval = 30

# Resource attributes
otel_resource_attributes = [
    "service.name=synevid",
    "service.version=0.6.0",
    "deployment.environment=production",
    "host.name=myserver"
]

# Headers (e.g., for authentication)
otel_headers = { "Authorization" = "Bearer <token>" }
```

### Programmatic Export (Rust)

```rust
use opentelemetry::metrics::{Counter, Histogram, Meter};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::metrics::{SdkMeterProvider, PeriodicReader};

fn init_metrics() -> Result<SdkMeterProvider, Box<dyn std::error::Error>> {
    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint("http://localhost:4317")
        .build_metrics_exporter()?;
    
    let reader = PeriodicReader::builder(exporter)
        .with_interval(std::time::Duration::from_secs(30))
        .build();
    
    let provider = SdkMeterProvider::builder()
        .with_reader(reader)
        .build();
    
    Ok(provider)
}

// Usage in daemon
lazy_static::lazy_static! {
    static ref METER: Meter = opentelemetry::global::meter("autopsy-daemon");
    static ref BASELINE_COUNTER: Counter<u64> = METER.u64_counter("autopsy.baseline.total").build();
    static ref BASELINE_DURATION: Histogram<f64> = METER.f64_histogram("autopsy.baseline.duration").build();
    static ref IMPACT_COUNTER: Counter<u64> = METER.u64_counter("autopsy.impact.total").build();
}

pub async fn handle_baseline(req: Request) -> Result<Response, Error> {
    let start = Instant::now();
    let result = run_baseline(req).await?;
    
    BASELINE_COUNTER.add(1, &[KeyValue::new("status", "success")]);
    BASELINE_DURATION.record(start.elapsed().as_secs_f64(), &[]);
    
    Ok(Response::json(result))
}
```

## Grafana Dashboards

### Dashboard: SynEvid Overview

```json
{
  "title": "SynEvid Overview",
  "panels": [
    {
      "title": "Request Rate",
      "type": "graph",
      "targets": [
        {
          "expr": "rate(autopsy_http_requests_total[5m])",
          "legendFormat": "{{method}} {{endpoint}}"
        }
      ]
    },
    {
      "title": "Latency p99",
      "type": "graph",
      "targets": [
        {
          "expr": "histogram_quantile(0.99, rate(autopsy_http_request_duration_seconds_bucket[5m]))",
          "legendFormat": "{{endpoint}}"
        }
      ]
    },
    {
      "title": "Active Leases",
      "type": "stat",
      "targets": [
        {
          "expr": "sum(autopsy_active_leases)",
          "legendFormat": "Active Leases"
        }
      ]
    },
    {
      "title": "Memory Usage",
      "type": "graph",
      "targets": [
        {
          "expr": "autopsy_memory_bytes{type=\"rss\"} / 1024 / 1024",
          "legendFormat": "RSS (MB)"
        },
        {
          "expr": "autopsy_memory_bytes{type=\"heap\"} / 1024 / 1024",
          "legendFormat": "Heap (MB)"
        }
      ]
    },
    {
      "title": "Error Rate",
      "type": "graph",
      "targets": [
        {
          "expr": "rate(autopsy_http_requests_total{status=~\"5..\"}[5m])",
          "legendFormat": "5xx Errors"
        },
        {
          "expr": "rate(autopsy_http_requests_total{status=~\"4..\"}[5m])",
          "legendFormat": "4xx Errors"
        }
      ]
    }
  ]
}
```

### Dashboard: Business Metrics

```json
{
  "title": "SynEvid Business Metrics",
  "panels": [
    {
      "title": "Baseline Operations",
      "type": "stat",
      "targets": [
        {
          "expr": "increase(autopsy_baseline_total[1h])",
          "legendFormat": "Baselines/hour"
        }
      ]
    },
    {
      "title": "Impact Coverage Distribution",
      "type": "piechart",
      "targets": [
        {
          "expr": "sum by (coverage) (autopsy_impact_total{status=\"success\"})",
          "legendFormat": "{{coverage}}"
        }
      ]
    },
    {
      "title": "Verify Verdicts",
      "type": "piechart",
      "targets": [
        {
          "expr": "sum by (status) (autopsy_verify_total)",
          "legendFormat": "{{status}}"
        }
      ]
    }
  ]
}
```

## Alerting Rules

### Critical Alerts

```yaml
groups:
  - name: synvid-critical
    interval: 30s
    rules:
      - alert: SynvidDaemonDown
        expr: autopsy_daemon_up == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "SynEvid daemon is down"
          description: "Daemon has been down for > 1 minute"

      - alert: SynvidHighErrorRate
        expr: |
          rate(autopsy_http_requests_total{status=~"5.."}[5m]) 
          / rate(autopsy_http_requests_total[5m]) > 0.05
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "High error rate (>5%)"
          description: "Error rate is {{ $value | humanizePercentage }}"

      - alert: SynvidHighMemory
        expr: |
          autopsy_memory_bytes{type="rss"} > 8 * 1024 * 1024 * 1024
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High memory usage"
          description: "RSS memory is {{ $value | humanize1024 }}"

      - alert: SynvidHighLatency
        expr: |
          histogram_quantile(0.99, rate(autopsy_http_request_duration_seconds_bucket[5m])) > 30
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High p99 latency"
          description: "p99 latency is {{ $value }}s"

      - alert: SynvidStorageGrowing
        expr: |
          (autopsy_storage_bytes{type="sqlite"} - 
           autopsy_storage_bytes{type="sqlite"} offset 24h) 
           / autopsy_storage_bytes{type="sqlite"} offset 24h > 0.5
        for: 1h
        labels:
          severity: warning
        annotations:
          summary: "Storage growing >50% in 24h"
```

### Warning Alerts

```yaml
  - alert: SynvidHighLatencyP95
    expr: |
      histogram_quantile(0.95, rate(autopsy_http_request_duration_seconds_bucket[5m])) > 10
    for: 10m
    labels:
      severity: warning
    annotations:
      summary: "High p95 latency"

  - alert: SynvidLeaseExhaustion
    expr: |
      sum(autopsy_active_leases) > 50
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "High number of active leases"

  - alert: SynvidBaselineSlow
    expr: |
      histogram_quantile(0.99, rate(autopsy_baseline_duration_seconds_bucket[5m])) > 60
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "Baseline taking > 60s (p99)"
```

## Client-Side Metrics

### CLI Metrics (autopsy-cli)

```bash
# Export metrics via --metrics flag
autopsy baseline --persist --metrics-format prometheus --metrics-output /tmp/metrics.prom

# Output:
# autopsy_cli_baseline_duration_seconds 1.234
# autopsy_cli_baseline_files_scanned 1500
# autopsy_cli_baseline_symbols_found 45000
# autopsy_cli_exit_code 0
```

### Web UI Metrics

```javascript
// apps/web/src/lib/metrics.ts
export function recordMetric(name: string, value: number, labels: Record<string, string>) {
  // Send to daemon /metrics endpoint or local collector
  fetch('/api/v1/metrics/record', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name, value, labels, timestamp: Date.now() })
  });
}

// Usage
recordMetric('ui_page_load_ms', performance.now() - navigationStart, {
  page: 'impact',
  repo_id: currentRepoId
});
```

## Metric Naming Conventions

| Pattern | Example |
|---------|---------|
| Counter | `autopsy_<operation>_total` |
| Gauge | `autopsy_<resource>_current` |
| Histogram | `autopsy_<operation>_duration_seconds` |
| Summary | `autopsy_<operation>_latency_seconds` |

### Label Standards

| Label | Values |
|-------|--------|
| `status` | `success`, `error`, `timeout` |
| `coverage` | `verified`, `partial`, `unknown`, `unavailable`, `error` |
| `verdict` | `pass`, `fail`, `warn`, `partial`, `unknown`, `error`, `unavailable` |
| `client_type` | `cli`, `tui`, `mcp`, `web`, `electron`, `daemon` |
| `operation` | `baseline`, `impact`, `verify`, `diff`, `symbols`, `find` |
| `endpoint` | `/api/v1/impact`, `/api/v1/verify`, etc. |

## Scraping Configuration

### Prometheus

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'synevid-daemon'
    static_configs:
      - targets: ['localhost:3333']
    metrics_path: '/metrics'
    scheme: http
    basic_auth:
      username: 'prometheus'
      password: '${SYNEVID_LOOPBACK_TOKEN}'
    scrape_interval: 15s
    scrape_timeout: 10s
```

### Grafana Agent / Alloy

```yaml
prometheus:
  configs:
    - name: synevid
      scrape_configs:
        - job_name: synevid
          static_configs:
            - targets: ['localhost:3333']
          metrics_path: /metrics
          basic_auth:
            username: prometheus
            password: ${SYNEVID_LOOPBACK_TOKEN}
```

## Custom Metrics API

### Recording Custom Metrics

```rust
// In any component
use metrics::{counter, histogram, gauge};

pub fn record_impact_analysis(symbol: &str, duration: Duration, coverage: CoverageState) {
    counter!("autopsy.impact.analysis.total", 1, 
        "symbol" => symbol.to_string(),
        "coverage" => coverage.to_string()
    );
    histogram!("autopsy.impact.analysis.duration_seconds", duration.as_secs_f64(),
        "symbol" => symbol.to_string()
    );
    gauge!("autopsy.impact.active", 1,
        "symbol" => symbol.to_string()
    );
}
```

---

*Document Version: 1.0*
*Last Updated: 2024-11-24*
*Next Review: 2025-05-24*