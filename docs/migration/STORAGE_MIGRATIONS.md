# SynEvid Storage Migration History

**Version**: 1.0
**Last Updated**: 2024-11-24

## Migration Overview

SynEvid uses an append-only migration system. Each migration:
- Has a unique ID and description
- Runs once per storage instance
- Is recorded in `migrations` table
- Cannot be rolled back (except via backup restore)

## Migration History

### v1 → v2: Initial Schema (Pre-0.1.0)

**Migration ID**: `001_initial_schema`
**Date**: 2024-03-01
**Description**: Create initial storage schema

```sql
-- Repositories
CREATE TABLE repositories (
    repo_id TEXT PRIMARY KEY,
    root_path TEXT NOT NULL,
    display_name TEXT NOT NULL,
    active_branch TEXT,
    head_commit_oid TEXT,
    latest_snap_id TEXT,
    languages_json TEXT NOT NULL DEFAULT '[]',
    monorepo_json TEXT NOT NULL DEFAULT '{}',
    last_indexed_seq INTEGER NOT NULL DEFAULT 0
);

-- Snapshots
CREATE TABLE snapshots (
    snap_id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL REFERENCES repositories(repo_id),
    commit_oid TEXT,
    timestamp INTEGER NOT NULL,
    files_json TEXT NOT NULL,
    symbols_json TEXT NOT NULL,
    edges_json TEXT NOT NULL,
    coverage_state TEXT NOT NULL,
    blake3_digest TEXT NOT NULL
);

-- Symbols
CREATE TABLE symbols (
    stable_id TEXT PRIMARY KEY,
    snap_id TEXT NOT NULL REFERENCES snapshots(snap_id),
    qualified_name TEXT NOT NULL,
    kind TEXT NOT NULL,
    file_path TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    visibility TEXT NOT NULL,
    signature_hash TEXT,
    coverage_state TEXT NOT NULL
);

-- Edges
CREATE TABLE edges (
    edge_id TEXT PRIMARY KEY,
    snap_id TEXT NOT NULL REFERENCES snapshots(snap_id),
    source_id TEXT NOT NULL REFERENCES symbols(stable_id),
    target_id TEXT NOT NULL REFERENCES symbols(stable_id),
    kind TEXT NOT NULL,
    confidence TEXT NOT NULL
);

-- Indexes
CREATE INDEX idx_symbols_snap ON symbols(snap_id);
CREATE INDEX idx_symbols_name ON symbols(qualified_name);
CREATE INDEX idx_edges_snap ON edges(snap_id);
CREATE INDEX idx_edges_source ON edges(source_id);
CREATE INDEX idx_edges_target ON edges(target_id);
```

---

### v2 → v3: Events & Sessions (0.3.0)

**Migration ID**: `002_events_sessions`
**Date**: 2024-07-15
**Description**: Add event store and forensic sessions

```sql
-- Events (append-only)
CREATE TABLE events (
    seq_id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    source_client_id TEXT NOT NULL,
    source_client_type TEXT NOT NULL,
    source_lease_id TEXT,
    repository_id TEXT,
    session_id TEXT,
    workflow_id TEXT,
    payload TEXT NOT NULL,
    evidence_digest TEXT NOT NULL
);

CREATE INDEX idx_events_seq ON events(seq_id);
CREATE INDEX idx_events_repo ON events(repository_id);
CREATE INDEX idx_events_session ON events(session_id);

-- Forensic Sessions
CREATE TABLE forensic_sessions (
    session_id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    actor_kind TEXT NOT NULL,
    actor_name TEXT NOT NULL,
    baseline_snap_id TEXT NOT NULL,
    current_snap_id TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    closed_at INTEGER,
    exit_code INTEGER,
    files_touched_count INTEGER DEFAULT 0,
    total_events_count INTEGER DEFAULT 0
);

-- Workflows
CREATE TABLE forensic_workflows (
    workflow_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES forensic_sessions(session_id),
    name TEXT NOT NULL,
    workflow_type TEXT NOT NULL,
    status TEXT NOT NULL,
    start_event_seq INTEGER NOT NULL,
    end_event_seq INTEGER,
    started_at INTEGER NOT NULL,
    completed_at INTEGER
);

-- Trajectory Events
CREATE TABLE trajectory_events (
    event_seq INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES forensic_sessions(session_id),
    job_id TEXT,
    workflow_id TEXT,
    event_type TEXT NOT NULL,
    target_symbol TEXT,
    target_file TEXT,
    before_snap_id TEXT,
    after_snap_id TEXT,
    verdict TEXT NOT NULL,
    summary TEXT NOT NULL,
    evidence_json TEXT NOT NULL,
    blake3_receipt TEXT NOT NULL,
    tokens_spent INTEGER DEFAULT 0,
    tokens_saved INTEGER DEFAULT 0,
    timestamp INTEGER NOT NULL
);

-- AI Session Ledger
CREATE TABLE ai_session_ledger (
    session_id TEXT PRIMARY KEY REFERENCES forensic_sessions(session_id),
    queries_count INTEGER DEFAULT 0,
    tokens_spent INTEGER DEFAULT 0,
    tokens_saved INTEGER DEFAULT 0,
    cache_hits INTEGER DEFAULT 0,
    cache_misses INTEGER DEFAULT 0,
    last_updated INTEGER NOT NULL
);

-- Runtime Observation Windows
CREATE TABLE runtime_observation_windows (
    window_id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    start_time INTEGER NOT NULL,
    end_time INTEGER,
    spans_count INTEGER DEFAULT 0,
    unique_routes INTEGER DEFAULT 0,
    error_rate REAL DEFAULT 0.0,
    p99_latency_ms INTEGER DEFAULT 0
);

-- OTLP Ingestion Log
CREATE TABLE otlp_ingestion_log (
    log_id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id TEXT NOT NULL,
    spans_received INTEGER NOT NULL,
    spans_redacted INTEGER DEFAULT 0,
    spans_dropped INTEGER DEFAULT 0,
    ingestion_time INTEGER NOT NULL,
    surface_digest TEXT,
    status TEXT NOT NULL
);

-- Indexes
CREATE INDEX idx_events_repo ON events(repository_id);
CREATE INDEX idx_events_session ON events(session_id);
CREATE INDEX idx_trajectory_session ON trajectory_events(session_id);
CREATE INDEX idx_trajectory_job ON trajectory_events(job_id);
CREATE INDEX idx_otlp_repo ON otlp_ingestion_log(repo_id);
```

---

### v3 → v4: Graph & Catalog (0.4.0)

**Migration ID**: `003_graph_catalog`
**Date**: 2024-09-01
**Description**: Add graph layout, catalog, and analytics tables

```sql
-- Graph Layout Cache
CREATE TABLE graph_layouts (
    layout_id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    snap_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    seed INTEGER NOT NULL,
    dimensions INTEGER NOT NULL,
    positions_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

-- Catalog Entities
CREATE TABLE catalog_entities (
    entity_id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    namespace TEXT,
    spec_json TEXT NOT NULL,
    relations_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_catalog_repo ON catalog_entities(repo_id);
CREATE INDEX idx_catalog_kind ON catalog_entities(kind);

-- Catalog Manifests
CREATE TABLE catalog_manifests (
    manifest_id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    parsed_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

-- Catalog Dependencies
CREATE TABLE catalog_dependencies (
    dep_id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    source_entity TEXT NOT NULL,
    target_entity TEXT NOT NULL,
    dep_type TEXT NOT NULL,
    strength REAL DEFAULT 1.0,
    created_at INTEGER NOT NULL
);

-- Analytics Reports
CREATE TABLE analytics_reports (
    report_id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    snap_id TEXT NOT NULL,
    format TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    evidence_digest TEXT NOT NULL,
    generated_at INTEGER NOT NULL,
    compact BOOLEAN DEFAULT 0
);

-- Report History
CREATE TABLE report_history (
    history_id INTEGER PRIMARY KEY AUTOINCREMENT,
    report_id TEXT NOT NULL,
    repo_id TEXT NOT NULL,
    snap_id TEXT NOT NULL,
    format TEXT NOT NULL,
    evidence_digest TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

-- Runtime Observation Windows
CREATE TABLE runtime_observation_windows (
    window_id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    start_time INTEGER NOT NULL,
    end_time INTEGER,
    spans_count INTEGER DEFAULT 0,
    unique_routes INTEGER DEFAULT 0,
    error_rate REAL DEFAULT 0.0,
    p99_latency_ms INTEGER DEFAULT 0
);

-- AI Session Ledger
CREATE TABLE ai_session_ledger (
    session_id TEXT PRIMARY KEY REFERENCES forensic_sessions(session_id),
    queries_count INTEGER DEFAULT 0,
    tokens_spent INTEGER DEFAULT 0,
    tokens_saved INTEGER DEFAULT 0,
    cache_hits INTEGER DEFAULT 0,
    cache_misses INTEGER DEFAULT 0,
    last_updated INTEGER NOT NULL
);

-- OTLP Ingestion Log
CREATE TABLE otlp_ingestion_log (
    log_id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id TEXT NOT NULL,
    spans_received INTEGER NOT NULL,
    spans_redacted INTEGER DEFAULT 0,
    spans_dropped INTEGER DEFAULT 0,
    ingestion_time INTEGER NOT NULL,
    surface_digest TEXT,
    status TEXT NOT NULL
);

-- Indexes
CREATE INDEX idx_catalog_repo ON catalog_entities(repo_id);
CREATE INDEX idx_catalog_kind ON catalog_entities(kind);
CREATE INDEX idx_analytics_repo ON analytics_reports(repo_id);
CREATE INDEX idx_report_history_repo ON report_history(repo_id);
```

---

### v4 → v5: Runtime & Envelope (0.6.0)

**Migration ID**: `004_runtime_envelope`
**Date**: 2024-11-24
**Description**: Runtime lease management, unified envelope

```sql
-- Runtime Leases
CREATE TABLE runtime_leases (
    lease_id TEXT PRIMARY KEY,
    client_id TEXT NOT NULL,
    client_type TEXT NOT NULL,
    repository_id TEXT,
    session_id TEXT,
    workflow_id TEXT,
    created_at INTEGER NOT NULL,
    last_heartbeat INTEGER NOT NULL,
    last_event_seq INTEGER DEFAULT 0
);

-- Daemon State (key-value)
CREATE TABLE daemon_state (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Event Sequence (for cursor-based replay)
CREATE TABLE event_sequences (
    repo_id TEXT PRIMARY KEY,
    last_seq INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
);

-- Evidence Envelope Receipts (for verification)
CREATE TABLE envelope_receipts (
    receipt_id TEXT PRIMARY KEY,
    operation_id TEXT NOT NULL,
    blake3_digest TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

-- Indexes
CREATE INDEX idx_leases_repo ON runtime_leases(repository_id);
CREATE INDEX idx_leases_client ON runtime_leases(client_id);
CREATE INDEX idx_sequences_repo ON event_sequences(repo_id);
```

---

## Migration Execution

### Automatic Execution

Migrations run automatically on daemon startup:

```rust
// In crates/autopsy-storage/src/migrations.rs
pub async fn run_migrations(conn: &mut Connection) -> Result<(), MigrationError> {
    let applied: Vec<String> = conn.query(
        "SELECT migration_id FROM migrations ORDER BY applied_at",
        [],
        |row| row.get(0)
    )?;
    
    for migration in ALL_MIGRATIONS {
        if !applied.contains(&migration.id) {
            migration.apply(conn)?;
            conn.execute(
                "INSERT INTO migrations (migration_id, applied_at) VALUES (?1, ?2)",
                [&migration.id, &chrono::Utc::now().timestamp()]
            )?;
        }
    }
    Ok(())
}
```

### Migration Table

```sql
CREATE TABLE migrations (
    migration_id TEXT PRIMARY KEY,
    applied_at INTEGER NOT NULL,
    checksum TEXT NOT NULL,  -- BLAKE3 of migration SQL
    success BOOLEAN NOT NULL DEFAULT 1
);
```

---

## Verification Commands

```bash
# Check applied migrations
sqlite3 .autopsy/storage.db "SELECT * FROM migrations;"

# Verify schema integrity
sqlite3 .autopsy/storage.db "PRAGMA integrity_check;"

# Check specific migration
sqlite3 .autopsy/storage.db "SELECT sql FROM sqlite_master WHERE type='table' AND name='runtime_leases';"
```

---

## Rollback Procedure

**Migrations are NOT reversible.** To rollback:

```bash
# 1. Stop daemon
pkill autopsy-daemon

# 2. Restore from backup
cp .autopsy/storage.db.backup .autopsy/storage.db

# 3. Restore config if needed
cp autopsy.toml.backup autopsy.toml

# 4. Restart
autopsy-daemon --repo-root /path/to/repo
```

---

## Schema Version Compatibility

| Storage Version | SynEvid Version | Compatible |
|-----------------|-----------------|------------|
| v1 (initial) | 0.1.x | ❌ |
| v2 (events) | 0.3.x | ❌ |
| v3 (graph) | 0.4.x | ❌ |
| v4 (runtime) | 0.5.x | ✅ |
| v5 (envelope) | 0.6.x | ✅ |

**Current**: v5 (0.6.x)

---

## Adding New Migrations

1. Create migration file: `crates/autopsy-storage/src/migrations/v006_description.rs`
2. Implement `Migration` trait:
```rust
pub struct Migration006Description;
impl Migration for Migration006Description {
    fn id(&self) -> &str { "006_description" }
    fn sql(&self) -> &str { include_str!("006_description.sql") }
    fn checksum(&self) -> &str { "b3:..." }
}
```
3. Register in `ALL_MIGRATIONS` array
4. Add test in `tests/migrations.rs`
5. Update this document

---

## Current Schema (v5)

### Core Tables
- `repositories` - Registered repositories
- `snapshots` - Baseline snapshots
- `symbols` - Extracted symbols
- `edges` - Dependency edges
- `events` - Event log
- `forensic_sessions` - Forensic sessions
- `forensic_workflows` - Workflows
- `trajectory_events` - Event timeline
- `ai_session_ledger` - AI token tracking
- `runtime_observation_windows` - OTLP windows
- `otlp_ingestion_log` - Ingestion audit
- `catalog_entities` - Backstage entities
- `catalog_manifests` - Manifest files
- `catalog_dependencies` - Dependency graph
- `analytics_reports` - Generated reports
- `report_history` - Report history
- `runtime_leases` - Client leases
- `daemon_state` - Daemon key-value
- `event_sequences` - Event cursors
- `envelope_receipts` - Envelope verification

### Indexes (Performance)
All foreign keys indexed. Common query patterns covered:
- `idx_symbols_snap` - Symbols by snapshot
- `idx_edges_source/target` - Graph traversal
- `idx_events_repo/session` - Event replay
- `idx_trajectory_session` - Forensic timeline
- `idx_leases_repo/client` - Lease management