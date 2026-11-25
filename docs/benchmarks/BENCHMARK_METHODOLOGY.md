# SynEvid Benchmark Methodology

This document describes the methodology for running and reporting SynEvid benchmarks.

## Test Environment

### Hardware Specification (Reference Machine)

| Component | Specification |
|-----------|---------------|
| CPU | AMD Ryzen 9 7950X / Intel i9-13900K |
| RAM | 64 GB DDR5-5600 |
| Storage | NVMe Gen4 (7000 MB/s read) |
| OS | Ubuntu 24.04 LTS / Windows 11 / macOS 14 |
| Rust | 1.90.0 (MSRV) |
| Rustflags | `-C target-cpu=native -C opt-level=3` |

### Software Versions (Pinned)

| Component | Version |
|-----------|---------|
| Rust | 1.90.0 |
| SQLite | 3.45.1 |
| LLVM | 18.1.0 |
| Tree-sitter | 0.22.x |

## Benchmark Categories

### 1. Correctness Benchmarks (Determinism)

**Goal**: Verify byte-identical output across runs.

**Method**:
```bash
for i in {1..100}; do
  cargo run --release -p autopsy-cli -- baseline --persist
  cargo run --release -p autopsy-cli -- impact -s "main" --format json > run_$i.json
done

# Verify all outputs identical
sha256sum run_*.json | cut -d' ' -f1 | sort -u | wc -l  # Must equal 1
```

**Pass Criteria**: 100/100 runs produce byte-identical BLAKE3 digests.

### 2. Latency Benchmarks (SLA)

**Goal**: Measure p50, p95, p99 latencies for each operation.

**Method**:
- Warm up: 10 runs (discard)
- Measure: 100 runs per operation
- Record: wall-clock time, CPU time, memory delta

**Operations Measured**:
| Operation | SLA Target |
|-----------|------------|
| Baseline (incremental) | p99 < 5s |
| Impact (single symbol) | p99 < 500ms |
| Verify (strict) | p99 < 10s |
| Diff | p99 < 5s |
| Symbols list | p99 < 200ms |
| Find | p99 < 100ms |

### 3. Throughput Benchmarks

**Goal**: Measure sustained throughput under load.

**Method**:
- Concurrent clients: 1, 5, 10, 50
- Duration: 5 minutes per level
- Measure: requests/sec, error rate, p99 latency

### 4. Scalability Benchmarks

**Goal**: Measure performance vs repository size.

**Test Repos**:

| Repo | Files | Lines | Language | Category |
|------|-------|-------|----------|----------|
| react | ~3k | ~300k | TS/JS | Medium |
| typescript | ~8k | ~1M | TS | Large |
| rust-analyzer | ~15k | ~2M | Rust | Large |
| django | ~12k | ~1.5M | Python | Large |
| go | ~50k | ~5M | Go | XLarge |
| nodejs | ~30k | ~3M | JS/C++ | XLarge |

**Metrics Collected**:
- Baseline time vs file count (should be ~O(n))
- Memory peak vs file count
- Impact query time (should be sublinear with good indexing)

### 5. Stress Tests (Reliability)

**Goal**: Verify stability under sustained load.

**Tests**:
1. **100-run determinism**: 100 consecutive runs, verify byte-identical output
2. **Concurrent clients**: 10 clients × 100 requests each
3. **Long-running daemon**: 24h uptime, verify no memory leaks
4. **Large repo**: Full scan of 100k+ file repo
5. **Rapid restart**: 100 daemon restarts, verify state recovery

## Metrics Collection

### Automated Collection

```rust
// Built into stress_test_runner.rs
#[derive(Serialize)]
struct BenchmarkMetrics {
    wall_time_ms: u64,
    cpu_time_ms: u64,
    peak_memory_mb: f64,
    cpu_percent: f64,
    disk_read_mb: f64,
    disk_write_mb: f64,
    network_rx_mb: f64,
    network_tx_mb: f64,
}
```

### System Metrics (via `sysinfo`)

- CPU usage per core
- Memory (RSS, VSZ, swap)
- Disk I/O (read/write bytes, IOPS)
- Network I/O
- Context switches
- Page faults

## Reporting

### Output Formats

1. **JSON** (machine-readable): `results/benchmark_<timestamp>.json`
2. **Markdown** (human-readable): `results/benchmark_<timestamp>.md`
3. **CSV** (for spreadsheet): `results/benchmark_<timestamp>.csv`

### Required Report Sections

1. **Executive Summary**: Pass/fail, key numbers
2. **Environment**: Hardware, OS, versions
3. **Per-Operation Latency**: p50/p95/p99 tables
4. **Throughput**: Requests/sec at each concurrency level
5. **Scalability**: Time vs file count plots
6. **Resource Usage**: Memory/CPU/disk over time
7. **Errors**: Any failures with context
8. **Comparison**: vs previous run (regression detection)

## Regression Detection

### Automated

```bash
# In CI pipeline
previous=$(cat benchmarks/results/latest.json)
current=$(cargo run --release -p syn-bench -- --format json)

# Compare key metrics
if current.baseline_p99 > previous.baseline_p99 * 1.10 {
    fail("Baseline latency regression > 10%")
}
if current.memory_peak > previous.memory_peak * 1.20 {
    fail("Memory regression > 20%")
}
```

### Thresholds

| Metric | Warning | Fail |
|--------|---------|------|
| Baseline p99 latency | +10% | +25% |
| Impact p99 latency | +10% | +25% |
| Memory peak | +15% | +30% |
| Disk usage | +20% | +50% |
| CPU utilization | +10% | +25% |

## Running Benchmarks

### Local Development

```bash
# Quick sanity check (3 repos, 10 runs each)
cargo run --release -p syn-bench -- --quick

# Full suite (all repos, 100 runs)
cargo run --release -p syn-bench -- --full

# Specific repo
cargo run --release -p syn-bench -- --repo rust-analyzer --runs 100
```

### CI Pipeline

```yaml
# .github/workflows/benchmarks.yml
name: Benchmarks
on:
  schedule:
    - cron: '0 2 * * *'  # Daily at 2 AM
  workflow_dispatch:

jobs:
  benchmark:
    runs-on: ubuntu-latest
    timeout-minutes: 120
    steps:
      - uses: actions/checkout@v4
      - name: Build release
        run: cargo build --release -p syn-bench -p autopsy-daemon
      - name: Run benchmarks
        run: ./target/release/syn-bench ./benchmarks/repos ./target/release/autopsy-daemon ./benchmarks/results
      - name: Upload results
        uses: actions/upload-artifact@v4
        with:
          name: benchmark-results
          path: benchmarks/results/
      - name: Check regressions
        run: cargo run -p syn-bench -- --compare-previous
```

## Data Retention

- Raw results: 90 days
- Aggregated reports: 365 days
- Regression alerts: Indefinite

## Reproducibility

All benchmarks must be:
1. **Deterministic**: Same input → same output (verified by 100-run test)
2. **Isolated**: No external dependencies (network, clocks, random)
3. **Portable**: Run on Linux/macOS/Windows with same results
4. **Versioned**: Benchmark code versioned with engine

## Reporting Template

```markdown
# SynEvid Benchmark Report - YYYY-MM-DD

## Executive Summary
- **Status**: PASS/FAIL
- **Total Repos Tested**: N
- **Total Duration**: Xh Ym
- **Overall Status**: PASS/FAIL

## Latency Summary (p99)
| Operation | Target | Actual | Status |
|-----------|--------|--------|--------|
| Baseline | <5s | 3.2s | ✅ |
| Impact | <500ms | 180ms | ✅ |
| Verify | <10s | 4.1s | ✅ |

## Throughput
| Concurrency | Req/sec | p99 Latency | Errors |
|-------------|---------|-------------|--------|
| 1 | 12.5 | 3.2s | 0 |
| 10 | 8.2 | 4.1s | 0 |
| 50 | 5.1 | 8.7s | 2 |

## Scalability
![Baseline vs Files](baseline_vs_files.png)
![Memory vs Files](memory_vs_files.png)

## Resource Usage
- Peak Memory: 2.1 GB (target < 4 GB)
- Peak CPU: 85% (target < 90%)
- Disk I/O: 45 GB written

## Regression Analysis
- No regressions detected vs previous run (2024-01-14)
- Baseline improved 5% (3.4s → 3.2s)

## Artifacts
- Raw data: `benchmarks/results/benchmark_20240115_143000.json`
- Full report: `benchmarks/results/benchmark_20240115_143000.md`
- CSV export: `benchmarks/results/benchmark_20240115_143000.csv`
```