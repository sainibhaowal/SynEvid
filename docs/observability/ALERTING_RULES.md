# SynEvid Alerting Rules

**Version**: 1.0
**Date**: 2024-11-24
**Applies To**: Prometheus/Alertmanager monitoring stack

## Alert Categories

### Critical (Page Immediately)

| Alert | Expression | Duration | Description |
|-------|------------|----------|-------------|
| `SynvidDaemonDown` | `autopsy_daemon_up == 0` | 1m | Daemon process not responding |
| `SynvidHighErrorRate` | `rate(http_5xx[5m]) / rate(http_total[5m]) > 0.05` | 2m | >5% 5xx errors |
| `SynvidStorageCorruption` | `autopsy_storage_corrupted == 1` | 0m | SQLite integrity check failed |
| `SynvidMemoryExhaustion` | `memory_rss > 8GB` | 5m | RSS > 8GB |
| `SynvidDiskFull` | `disk_free_percent < 5` | 5m | Less than 5% disk free |

### Warning (Notify Within 15min)

| Alert | Expression | Duration | Description |
|-------|------------|----------|-------------|
| `SynvidHighLatencyP99` | `histogram_quantile(0.99, rate(latency_bucket[5m])) > 30s` | 5m | p99 latency > 30s |
| `SynvidHighLatencyP95` | `histogram_quantile(0.95, rate(latency_bucket[5m])) > 10s` | 10m | p95 latency > 10s |
| `SynvidHighMemory` | `memory_rss > 6GB` | 5m | RSS > 6GB |
| `SynvidLeaseExhaustion` | `active_leases > 50` | 5m | >50 concurrent leases |
| `SynvidStorageGrowing` | `storage_growth_24h > 50%` | 1h | Storage grew >50% in 24h |
| `SynvidBaselineSlow` | `baseline_p99 > 60s` | 5m | Baseline p99 > 60s |
| `SynvidImpactSlow` | `impact_p99 > 30s` | 5m | Impact p99 > 30s |
| `SynvidVerifySlow` | `verify_p99 > 60s` | 5m | Verify p99 > 60s |
| `SynvidHighCPU` | `cpu_percent > 90%` | 5m | CPU > 90% |
| `SynvidHighDiskIO` | `disk_io_wait > 50%` | 5m | Disk I/O wait > 50% |

### Info (Log Only)

| Alert | Expression | Description |
|-------|------------|-------------|
| `SynvidNewLease` | `rate(new_leases[5m]) > 0` | New client connected |
| `SynvidBaselineComplete` | `rate(baseline_complete[5m]) > 0` | Baseline finished |
| `SynvidImpactComplete` | `rate(impact_complete[5m]) > 0` | Impact analysis finished |
| `SynvidNewRepository` | `rate(new_repo[5m]) > 0` | New repo registered |

## PrometheusRule Configuration

```yaml
apiVersion: monitoring.coreos.com/v1
kind: PrometheusRule
metadata:
  name: synvid-alerts
  namespace: monitoring
spec:
  groups:
    - name: synvid-critical
      interval: 30s
      rules:
        - alert: SynvidDaemonDown
          expr: autopsy_daemon_up == 0
          for: 1m
          labels:
            severity: critical
            team: platform
          annotations:
            summary: "SynEvid daemon is down"
            description: "Daemon at {{ $labels.instance }} has been down for > 1 minute"
            runbook_url: "https://docs.synevid.dev/runbooks/daemon-down"

        - alert: SynvidHighErrorRate
          expr: |
            sum(rate(autopsy_http_requests_total{status=~"5.."}[5m]))
            /
            sum(rate(autopsy_http_requests_total[5m])) > 0.05
          for: 2m
          labels:
            severity: critical
            team: platform
          annotations:
            summary: "High error rate (>5%)"
            description: "5xx error rate is {{ $value | humanizePercentage }} for {{ $labels.instance }}"
            runbook_url: "https://docs.synevid.dev/runbooks/high-error-rate"

        - alert: SynvidStorageCorruption
          expr: autopsy_storage_corrupted == 1
          for: 0m
          labels:
            severity: critical
            team: platform
          annotations:
            summary: "Storage corruption detected"
            description: "SQLite integrity check failed on {{ $labels.instance }}"
            runbook_url: "https://docs.synevid.dev/runbooks/storage-corruption"

    - name: synvid-warning
      interval: 60s
      rules:
        - alert: SynvidHighLatencyP99
          expr: |
            histogram_quantile(0.99, rate(autopsy_http_request_duration_seconds_bucket[5m])) > 30
          for: 5m
          labels:
            severity: warning
            team: platform
          annotations:
            summary: "High p99 latency (>30s)"
            description: "p99 latency is {{ $value }}s for {{ $labels.endpoint }}"
            runbook_url: "https://docs.synevid.dev/runbooks/high-latency"

        - alert: SynvidHighLatencyP95
          expr: |
            histogram_quantile(0.95, rate(autopsy_http_request_duration_seconds_bucket[5m])) > 10
          for: 10m
          labels:
            severity: warning
            team: platform
          annotations:
            summary: "High p95 latency (>10s)"

        - alert: SynvidHighMemory
          expr: autopsy_memory_bytes{type="rss"} > 6 * 1024 * 1024 * 1024
          for: 5m
          labels:
            severity: warning
            team: platform
          annotations:
            summary: "High memory usage"
            description: "RSS memory is {{ $value | humanize1024 }}"

        - alert: SynvidLeaseExhaustion
          expr: sum(autopsy_active_leases) > 50
          for: 5m
          labels:
            severity: warning
            team: platform
          annotations:
            summary: "High number of active leases"
            description: "{{ $value }} active leases (threshold: 50)"

        - alert: SynvidStorageGrowing
          expr: |
            (autopsy_storage_bytes{type="sqlite"} - 
             autopsy_storage_bytes{type="sqlite"} offset 24h) 
             / autopsy_storage_bytes{type="sqlite"} offset 24h > 0.5
          for: 1h
          labels:
            severity: warning
            team: platform
          annotations:
            summary: "Storage growing >50% in 24h"
            description: "Storage grew {{ $value | humanizePercentage }} in 24h"

        - alert: SynvidBaselineSlow
          expr: |
            histogram_quantile(0.99, rate(autopsy_baseline_duration_seconds_bucket[5m])) > 60
          for: 5m
          labels:
            severity: warning
            team: platform
          annotations:
            summary: "Baseline p99 > 60s"

        - alert: SynvidImpactSlow
          expr: |
            histogram_quantile(0.99, rate(autopsy_impact_duration_seconds_bucket[5m])) > 30
          for: 5m
          labels:
            severity: warning
            team: platform
          annotations:
            summary: "Impact p99 > 30s"

        - alert: SynvidVerifySlow
          expr: |
            histogram_quantile(0.99, rate(autopsy_verify_duration_seconds_bucket[5m])) > 60
          for: 5m
          labels:
            severity: warning
            team: platform
          annotations:
            summary: "Verify p99 > 60s"

        - alert: SynvidHighCPU
          expr: autopsy_cpu_percent > 90
          for: 5m
          labels:
            severity: warning
            team: platform
          annotations:
            summary: "High CPU usage"
            description: "CPU usage is {{ $value }}%"

        - alert: SynvidHighDiskIO
          expr: autopsy_disk_io_wait_percent > 50
          for: 5m
          labels:
            severity: warning
            team: platform
          annotations:
            summary: "High disk I/O wait"
            description: "Disk I/O wait is {{ $value }}%"

    - name: synvid-info
      interval: 300s
      rules:
        - alert: SynvidNewLease
          expr: rate(autopsy_lease_created_total[5m]) > 0
          labels:
            severity: info
          annotations:
            summary: "New client lease created"
            description: "New lease for client {{ $labels.client_id }}"

        - alert: SynvidNewRepository
          expr: rate(autopsy_repo_registered_total[5m]) > 0
          labels:
            severity: info
          annotations:
            summary: "New repository registered"
            description: "Repository {{ $labels.repo_id }} registered"
```

## Alertmanager Configuration

```yaml
# alertmanager.yml
global:
  resolve_timeout: 5m
  smtp_smarthost: 'smtp.gmail.com:587'
  smtp_from: 'alerts@synevid.dev'
  smtp_auth_username: 'alerts@synevid.dev'
  smtp_auth_password: '${SMTP_PASSWORD}'

route:
  group_by: ['alertname', 'severity', 'instance']
  group_wait: 30s
  group_interval: 5m
  repeat_interval: 4h
  receiver: 'default'
  routes:
    - match:
        severity: critical
      receiver: 'critical-pager'
      continue: true
    - match:
        severity: warning
      receiver: 'warning-slack'
    - match:
        severity: info
      receiver: 'info-slack'

receivers:
  - name: 'default'
    email_configs:
      - to: 'team@synevid.dev'
        send_resolved: true
    slack_configs:
      - api_url: '${SLACK_WEBHOOK_URL}'
        channel: '#synevid-alerts'
        send_resolved: true
        title: 'SynEvid Alert: {{ .GroupLabels.alertname }}'
        text: |
          {{ range .Alerts }}
          *{{ .Labels.severity | upper }}*: {{ .Annotations.summary }}
          {{ .Annotations.description }}
          {{ end }}

  - name: 'critical-pager'
    pagerduty_configs:
      - service_key: '${PAGERDUTY_KEY}'
        severity: critical
        description: '{{ .GroupLabels.alertname }}: {{ .Annotations.summary }}'

    - name: 'warning-slack'
      slack_configs:
        - api_url: '${SLACK_WEBHOOK_URL}'
          channel: '#synevid-warnings'
          send_resolved: true

    - name: 'info-slack'
      slack_configs:
        - api_url: '${SLACK_WEBHOOK_URL}'
          channel: '#synevid-info'
          send_resolved: true

inhibit_rules:
  - source_match:
      severity: 'critical'
    target_match:
      severity: 'warning'
    equal: ['alertname', 'instance']
  - source_match:
      severity: 'critical'
    target_match:
      severity: 'info'
    equal: ['alertname', 'instance']
```

## Runbooks

### Daemon Down

**Symptoms**: `autopsy_daemon_up == 0`

**Investigation**:
1. Check process: `ps aux | grep autopsy-daemon`
2. Check logs: `journalctl -u autopsy-daemon -f`
3. Check port: `ss -tlnp | grep 3333`
4. Check disk: `df -h /path/to/data`

**Resolution**:
- Restart: `systemctl restart autopsy-daemon` or `autopsy-daemon --repo-root ...`
- Check config: `autopsy doctor`
- Check storage: `sqlite3 .autopsy/storage.db "PRAGMA integrity_check;"`

### High Error Rate

**Symptoms**: `rate(http_5xx[5m]) / rate(http_total[5m]) > 0.05`

**Investigation**:
1. Check error logs: `grep ERROR /var/log/synevid/daemon.log`
2. Check specific endpoints: `rate(http_requests_total{status="500"}[5m]) by (endpoint)`
3. Check recent deployments

**Resolution**:
- Rollback if recent deploy
- Fix bug if code issue
- Scale if resource exhaustion

### High Latency

**Symptoms**: `histogram_quantile(0.99, rate(latency_bucket[5m])) > 30`

**Investigation**:
1. Check which endpoint: `histogram_quantile(0.99, rate(latency_bucket[5m])) by (endpoint)`
2. Check resource usage: CPU, memory, disk I/O
3. Check GC pauses (if applicable)
4. Check lock contention

**Resolution**:
- Optimize slow queries
- Add caching
- Scale resources
- Fix lock contention

### Storage Corruption

**Symptoms**: `autopsy_storage_corrupted == 1`

**Investigation**:
1. Run integrity check: `sqlite3 .autopsy/storage.db "PRAGMA integrity_check;"`
2. Check WAL file: `ls -la .autopsy/*.wal`
3. Check disk health: `smartctl -a /dev/sdX`

**Resolution**:
1. Stop daemon
2. Restore from backup: `cp storage.db.backup storage.db`
3. Run `PRAGMA integrity_check` again
4. If still corrupted, full restore from backup

### High Memory

**Symptoms**: `memory_rss > 6GB`

**Investigation**:
1. Check heap profile: `pprof --heap`
2. Check for memory leaks: `valgrind --leak-check=full`
3. Check cache sizes

**Resolution**:
- Restart daemon (clears caches)
- Reduce cache sizes in config
- Fix memory leak if found

### High CPU

**Symptoms**: `cpu_percent > 90%`

**Investigation**:
1. Profile CPU: `perf top -p <pid>`
2. Check for infinite loops
3. Check GC pressure

**Resolution**:
- Fix infinite loops
- Reduce GC pressure
- Add caching

---

*Document Version: 1.0*
*Last Updated: 2024-11-24*
*Next Review: 2025-05-24*