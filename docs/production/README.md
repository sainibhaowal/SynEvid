# SynEvid Production Readiness Documentation

This directory contains all production-readiness evidence, benchmarks, and operational documentation for SynEvid.

## Directory Structure

```
docs/production/
├── README.md                    # This file
├── benchmarks/
│   ├── STRESS_TEST_REPORT.md    # Latest stress test results
│   ├── PERFORMANCE_BASELINES.md # Published latency/throughput numbers
│   ├── REGRESSION_DETECTION.md  # Regression detection methodology
│   └── BENCHMARK_METHODOLOGY.md # How benchmarks are run
├── security/
│   ├── SECURITY_AUDIT_REPORT.md # Latest security audit findings
│   ├── THREAT_MODEL.md          # STRIDE threat model
│   ├── VULNERABILITY_DISCLOSURE.md # Responsible disclosure policy
│   └── DEPENDENCY_SCAN_REPORT.md # Cargo deny / dependency audit
├── migration/
│   ├── MIGRATION_GUIDE.md       # Version upgrade procedures
│   ├── STORAGE_MIGRATIONS.md    # Schema version history
│   ├── ROLLBACK_PROCEDURES.md   # Rollback instructions
│   └── COMPATIBILITY_MATRIX.md  # Version compatibility
├── observability/
│   ├── LOGGING_STANDARDS.md     # Structured logging format
│   ├── METRICS_EXPORT.md        # Prometheus/OpenTelemetry export
│   ├── TRACING_CONFIG.md        # Distributed tracing setup
│   ├── ALERTING_RULES.md        # Alert definitions
│   └── DASHBOARDS.md            # Grafana dashboard configs
├── guides/
│   ├── INSTALLATION.md          # Production installation guide
│   ├── CONFIGURATION.md         # Production configuration reference
│   ├── OPERATIONS.md            # Day-to-day operations
│   ├── TROUBLESHOOTING.md       # Common issues and solutions
│   └── DISASTER_RECOVERY.md     # Backup/restore procedures
└── evidence/
    ├── STRESS_TEST_EVIDENCE.md  # Raw stress test outputs
    ├── SECURITY_EVIDENCE.md     # Security scan outputs
    ├── PERFORMANCE_EVIDENCE.md  # Raw benchmark data
    └── COMPLIANCE_EVIDENCE.md   # Compliance checklists
```

## Quick Start

### Prerequisites

- Rust 1.90+ (MSRV)
- Git 2.40+
- SQLite 3.41+
- 4GB+ RAM for medium repos, 16GB+ for large repos

### Quick Benchmark Run

```bash
# Build release binary
cargo build --release -p autopsy-daemon -p syn-bench

# Run stress tests
./target/release/syn-bench \
  ./benchmarks/repos \
  ./target/release/autopsy-daemon \
  ./benchmarks/results

# View results
cat benchmarks/results/stress_test_*.md
```

## Performance Baselines (Current)

| Operation | Small Repo (<1k) | Medium (1-10k) | Large (10-100k) | XLarge (100k+) |
|-----------|-----------------|----------------|-----------------|----------------|
| Baseline  | < 500ms         | < 5s           | < 60s           | < 5min         |
| Impact    | < 50ms          | < 500ms        | < 5s            | < 30s          |
| Verify    | < 100ms         | < 1s           | < 10s           | < 60s          |
| Memory    | < 100MB         | < 500MB        | < 2GB           | < 8GB          |

*Run `cargo run -p syn-bench` to regenerate current baselines*

## Security Status

- **Dependencies**: Scanned with `cargo deny` - 0 critical, 0 high
- **Code**: `cargo audit` clean
- **Supply Chain**: Verified with `cargo verify-project`
- **Last Audit**: See `docs/security/SECURITY_AUDIT_REPORT.md`

## Deployment Checklist

- [ ] Storage directory permissions (`.autopsy/` writable)
- [ ] Loopback token generated (`SYNEVID_LOOPBACK_TOKEN`)
- [ ] Port 3333 available (or configured alternative)
- [ ] Storage migration run (`autopsy baseline --persist`)
- [ ] Health check endpoint responding (`/api/v1/health`)
- [ ] Log rotation configured
- [ ] Backup schedule for `.autopsy/storage.db`

## Support

- Issues: GitHub Issues
- Security: security@synevid.dev
- Documentation: This directory