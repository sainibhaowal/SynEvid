//! autopsy-storage: SQLite and content-addressed `.autopsy/cache` storage engine (FR-015, FR-028).
//!
//! Provides deterministic state persistence, versioned schema migrations,
//! optional WAL mode, and immutable content-addressed object storage.

use autopsy_domain::{AnalysisRun, Finding, RepoSnapshot, SnapshotId};
use autopsy_evidence::EvidenceReceipt;
use rusqlite::{Connection, OptionalExtension, params};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use thiserror::Error;

pub mod rows;
pub use rows::{
    AiSessionLedgerRecord, AiWorkingCacheEntry, CoChangeEdgeRow, ContractRow, EgoNetwork,
    ForensicSessionRecord, ForensicWorkflowRecord, GraphEdgeRow, GraphNodeRow, RepoRecord,
    TemporalFactRow, TestMappingRow, TrajectoryEventRecord, validate_coverage,
};

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("IO error in cache storage: {0}")]
    Io(#[from] std::io::Error),
    #[error("Schema migration error: {0}")]
    Migration(String),
    #[error("Invalid row: {0}")]
    InvalidRow(String),
}

/// Configuration options for storage initialization.
#[derive(Debug, Clone)]
pub struct StorageOptions {
    pub use_wal: bool,
    pub cache_dir: Option<PathBuf>,
}

impl Default for StorageOptions {
    fn default() -> Self {
        Self {
            use_wal: true,
            cache_dir: None,
        }
    }
}

/// Thread-safe SQLite storage engine with content-addressed disk cache.
#[derive(Clone)]
pub struct StorageEngine {
    conn: Arc<Mutex<Connection>>,
    cache_dir: Option<PathBuf>,
}

impl StorageEngine {
    /// Opens or creates an autopsy SQLite database at the specified path.
    pub fn open(db_path: impl AsRef<Path>, options: StorageOptions) -> Result<Self, StorageError> {
        let path = db_path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;
        let engine = Self::init_connection(conn, options)?;
        Ok(engine)
    }

    /// Opens or creates an autopsy SQLite database, automatically self-healing if corrupted (ERR-STORAGE-102, NFR-P1-002).
    pub fn open_or_recover(
        db_path: impl AsRef<Path>,
        options: StorageOptions,
    ) -> Result<Self, StorageError> {
        let path = db_path.as_ref();
        if path.exists() {
            let test_open = Self::open(path, options.clone());
            match test_open {
                Ok(engine) => {
                    let is_ok = {
                        let conn = engine.conn.lock().unwrap();
                        let check: Result<String, _> =
                            conn.query_row("PRAGMA quick_check;", [], |r| r.get(0));
                        check.map(|s| s == "ok").unwrap_or(false)
                    };
                    if is_ok {
                        return Ok(engine);
                    }
                }
                Err(_) => {}
            }
            // Corrupt file detected: back up to .bak and recreate cleanly
            let bak_path = path.with_extension("db.bak");
            let _ = fs::rename(path, &bak_path);
        }
        Self::open(path, options)
    }

    /// Creates an in-memory SQLite database instance (ideal for tests and dry-runs).
    pub fn in_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()?;
        let options = StorageOptions {
            use_wal: false,
            cache_dir: None,
        };
        let engine = Self::init_connection(conn, options)?;
        Ok(engine)
    }

    fn init_connection(conn: Connection, options: StorageOptions) -> Result<Self, StorageError> {
        // Enforce busy timeout, foreign keys and normal synchronous mode
        conn.execute_batch(
            "PRAGMA busy_timeout = 5000;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;",
        )?;

        if options.use_wal {
            let _ = conn.query_row("PRAGMA journal_mode = WAL;", [], |_| Ok(()));
        }

        let mut engine = Self {
            conn: Arc::new(Mutex::new(conn)),
            cache_dir: options.cache_dir,
        };

        engine.run_migrations()?;

        if let Some(cache) = &engine.cache_dir {
            fs::create_dir_all(cache.join("objects"))?;
        }

        Ok(engine)
    }

    /// Executes versioned migrations to ensure database schema is current.
    pub fn run_migrations(&mut self) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );",
        )?;

        let current_version: i64 = conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations;",
            [],
            |row| row.get(0),
        )?;

        if current_version < 1 {
            conn.execute_batch(
                "BEGIN;
                CREATE TABLE snapshots (
                    snapshot_id TEXT PRIMARY KEY,
                    revision TEXT NOT NULL,
                    config_hash TEXT NOT NULL,
                    file_set_digest TEXT NOT NULL,
                    snapshot_json TEXT NOT NULL
                );

                CREATE TABLE runs (
                    run_id TEXT PRIMARY KEY,
                    command TEXT NOT NULL,
                    analyzer_version TEXT NOT NULL,
                    duration_ms INTEGER NOT NULL,
                    result_digest TEXT NOT NULL
                );

                CREATE TABLE findings (
                    id TEXT PRIMARY KEY,
                    run_id TEXT NOT NULL,
                    rule_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    severity TEXT NOT NULL,
                    message TEXT NOT NULL,
                    coverage TEXT NOT NULL,
                    entities_json TEXT NOT NULL,
                    FOREIGN KEY(run_id) REFERENCES runs(run_id) ON DELETE CASCADE
                );

                CREATE TABLE evidence (
                    finding_id TEXT PRIMARY KEY,
                    rule_id TEXT NOT NULL,
                    evidence_digest TEXT NOT NULL,
                    evidence_json TEXT NOT NULL
                );

                INSERT INTO schema_migrations (version, applied_at)
                VALUES (1, datetime('now'));
                COMMIT;",
            )?;
        }

        // --- Migration 002: Phase 4.2 temporal & extended tables ---
        //
        // `created_at` / `window_start` / `window_end` are bookkeeping columns.
        // They are never read into canonical result structures, so wall-clock
        // time cannot leak into BLAKE3 digests (NFR-001, FR-028).
        if current_version < 2 {
            conn.execute_batch(
                "BEGIN;
                CREATE TABLE IF NOT EXISTS temporal_facts (
                    fact_id TEXT PRIMARY KEY,
                    subject TEXT NOT NULL,
                    predicate TEXT NOT NULL,
                    object TEXT NOT NULL,
                    valid_from TEXT NOT NULL,
                    valid_to TEXT,
                    evidence_digest TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_temporal_facts_subject
                    ON temporal_facts (subject, valid_from);
                CREATE INDEX IF NOT EXISTS idx_temporal_facts_span
                    ON temporal_facts (valid_from, valid_to);
                CREATE INDEX IF NOT EXISTS idx_temporal_facts_predicate
                    ON temporal_facts (predicate);

                CREATE TABLE IF NOT EXISTS test_mappings (
                    test_suite_id TEXT NOT NULL,
                    target_symbol_id TEXT NOT NULL,
                    mapping_source TEXT NOT NULL,
                    evidence_digest TEXT NOT NULL,
                    PRIMARY KEY (test_suite_id, target_symbol_id, mapping_source)
                );

                CREATE INDEX IF NOT EXISTS idx_test_mappings_target
                    ON test_mappings (target_symbol_id, mapping_source);

                CREATE TABLE IF NOT EXISTS distributed_contracts (
                    contract_id TEXT PRIMARY KEY,
                    protocol TEXT NOT NULL,
                    service_id TEXT NOT NULL,
                    endpoint_or_channel TEXT NOT NULL,
                    spec_digest TEXT NOT NULL,
                    parsed_json TEXT NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_distributed_contracts_service
                    ON distributed_contracts (service_id, protocol);
                CREATE INDEX IF NOT EXISTS idx_distributed_contracts_endpoint
                    ON distributed_contracts (endpoint_or_channel);

                CREATE TABLE IF NOT EXISTS co_change_matrix (
                    entity_a TEXT NOT NULL,
                    entity_b TEXT NOT NULL,
                    commit_count INTEGER NOT NULL,
                    lift REAL NOT NULL,
                    window_start TEXT NOT NULL,
                    window_end TEXT NOT NULL,
                    PRIMARY KEY (entity_a, entity_b)
                );

                CREATE INDEX IF NOT EXISTS idx_co_change_matrix_a
                    ON co_change_matrix (entity_a, lift);
                CREATE INDEX IF NOT EXISTS idx_co_change_matrix_b
                    ON co_change_matrix (entity_b, lift);

                INSERT INTO schema_migrations (version, applied_at)
                VALUES (2, datetime('now'));
                COMMIT;",
            )?;
        }

        // Migration 003 — the pillar tables get their production write path (Evidence 32).
        //
        // Two changes, both safe because no production path ever wrote these tables:
        //
        // 1. `temporal_facts.coverage` — a fact row without its coverage state would read
        //    back as an unqualified answer. The `DEFAULT 'unknown'` exists only because
        //    SQLite requires one for NOT NULL on ALTER TABLE; the save API always writes
        //    the real state, and the table has never had a row.
        // 2. `co_change_matrix` is recreated in the shape the engine actually produces:
        //    window-scoped primary key (`window_id` is the commit-set digest, an id, not a
        //    timestamp, so two windows coexist), ppm integers for support/confidence/lift,
        //    and the `claim` column that keeps association from ever reading as causation.
        //    The v2 shape stored a wall-clock window and no claim, which no writer ever
        //    produced; any rows dropped here were test rows in development databases.
        if current_version < 3 {
            conn.execute_batch(
                "BEGIN;
                DROP INDEX IF EXISTS idx_co_change_matrix_a;
                DROP INDEX IF EXISTS idx_co_change_matrix_b;
                DROP TABLE IF EXISTS co_change_matrix;
                CREATE TABLE co_change_matrix (
                    entity_a TEXT NOT NULL,
                    entity_b TEXT NOT NULL,
                    window_id TEXT NOT NULL,
                    commits_together INTEGER NOT NULL,
                    support_ppm INTEGER NOT NULL,
                    confidence_a_to_b_ppm INTEGER NOT NULL,
                    lift_ppm INTEGER NOT NULL,
                    claim TEXT NOT NULL,
                    coverage TEXT NOT NULL,
                    PRIMARY KEY (entity_a, entity_b, window_id)
                );
                CREATE INDEX IF NOT EXISTS idx_co_change_matrix_a
                    ON co_change_matrix (entity_a, lift_ppm);
                CREATE INDEX IF NOT EXISTS idx_co_change_matrix_b
                    ON co_change_matrix (entity_b, lift_ppm);
                ALTER TABLE temporal_facts ADD COLUMN coverage TEXT NOT NULL DEFAULT 'unknown';
                INSERT INTO schema_migrations (version, applied_at)
                VALUES (3, datetime('now'));
                COMMIT;",
            )?;
        }

        // --- Migration 004: Phase 1 (v0.2.0) Interactive Visual Cockpit tables ---
        if current_version < 4 {
            conn.execute_batch(
                "BEGIN;
                CREATE TABLE IF NOT EXISTS repositories (
                    repo_id TEXT PRIMARY KEY,
                    root_path TEXT NOT NULL UNIQUE,
                    display_name TEXT NOT NULL,
                    active_branch TEXT,
                    head_commit_oid TEXT,
                    latest_snap_id TEXT,
                    languages_json TEXT NOT NULL,
                    monorepo_json TEXT NOT NULL,
                    last_indexed_seq INTEGER NOT NULL DEFAULT 0
                );

                CREATE TABLE IF NOT EXISTS graph_nodes (
                    snapshot_id TEXT NOT NULL REFERENCES snapshots(snapshot_id) ON DELETE CASCADE,
                    stable_id TEXT NOT NULL,
                    node_kind TEXT NOT NULL,
                    qualified_name TEXT NOT NULL,
                    file_path TEXT NOT NULL,
                    start_line INTEGER NOT NULL,
                    end_line INTEGER NOT NULL,
                    signature_hash TEXT NOT NULL,
                    visibility TEXT NOT NULL,
                    is_exported INTEGER NOT NULL,
                    is_dead_code INTEGER NOT NULL DEFAULT 0,
                    coverage_state TEXT NOT NULL,
                    PRIMARY KEY (snapshot_id, stable_id)
                );
                CREATE INDEX IF NOT EXISTS idx_graph_nodes_file ON graph_nodes(snapshot_id, file_path);

                CREATE TABLE IF NOT EXISTS graph_edges (
                    snapshot_id TEXT NOT NULL REFERENCES snapshots(snapshot_id) ON DELETE CASCADE,
                    edge_id TEXT NOT NULL,
                    source_stable_id TEXT NOT NULL,
                    target_stable_id TEXT NOT NULL,
                    edge_kind TEXT NOT NULL,
                    source_file TEXT NOT NULL,
                    source_line INTEGER NOT NULL,
                    source_col INTEGER NOT NULL,
                    target_file TEXT NOT NULL,
                    target_line INTEGER NOT NULL,
                    confidence TEXT NOT NULL,
                    PRIMARY KEY (snapshot_id, edge_id)
                );
                CREATE INDEX IF NOT EXISTS idx_graph_edges_src ON graph_edges(snapshot_id, source_stable_id);
                CREATE INDEX IF NOT EXISTS idx_graph_edges_tgt ON graph_edges(snapshot_id, target_stable_id);
                CREATE INDEX IF NOT EXISTS idx_graph_edges_file ON graph_edges(snapshot_id, source_file, target_file);

                CREATE TABLE IF NOT EXISTS forensic_sessions (
                    session_id TEXT PRIMARY KEY,
                    repo_id TEXT NOT NULL,
                    actor_kind TEXT NOT NULL,
                    actor_name TEXT NOT NULL,
                    baseline_snap_id TEXT NOT NULL,
                    current_snap_id TEXT NOT NULL,
                    status TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS trajectory_events (
                    event_seq INTEGER PRIMARY KEY AUTOINCREMENT,
                    session_id TEXT NOT NULL,
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
                    tokens_spent INTEGER NOT NULL DEFAULT 0,
                    tokens_saved INTEGER NOT NULL DEFAULT 0
                );
                CREATE INDEX IF NOT EXISTS idx_trajectory_session ON trajectory_events(session_id, event_seq);

                CREATE TABLE IF NOT EXISTS ai_working_cache (
                    cache_key TEXT PRIMARY KEY,
                    repo_id TEXT NOT NULL,
                    snapshot_id TEXT NOT NULL,
                    query_kind TEXT NOT NULL,
                    target_id TEXT NOT NULL,
                    compact_payload TEXT NOT NULL,
                    token_estimate INTEGER NOT NULL,
                    hit_count INTEGER NOT NULL DEFAULT 0,
                    last_receipt_token TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_ai_cache_snap ON ai_working_cache(repo_id, snapshot_id);

                INSERT INTO schema_migrations (version, applied_at)
                VALUES (4, datetime('now'));
                COMMIT;",
            )?;
        }

        // --- Migration 005: Phase 4 (v0.5.0) Real-Time Forensic Trajectory Timeline & AI Working Cache ---
        if current_version < 5 {
            conn.execute_batch(
                "BEGIN;
                CREATE TABLE IF NOT EXISTS forensic_workflows (
                    workflow_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    status TEXT NOT NULL,
                    start_seq INTEGER NOT NULL,
                    end_seq INTEGER,
                    duration_ms INTEGER NOT NULL DEFAULT 0,
                    FOREIGN KEY(session_id) REFERENCES forensic_sessions(session_id) ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_workflow_session ON forensic_workflows(session_id);

                CREATE TABLE IF NOT EXISTS ai_session_ledger (
                    session_id TEXT PRIMARY KEY,
                    total_queries INTEGER NOT NULL DEFAULT 0,
                    cache_hits INTEGER NOT NULL DEFAULT 0,
                    tokens_spent INTEGER NOT NULL DEFAULT 0,
                    tokens_saved INTEGER NOT NULL DEFAULT 0,
                    cost_saved_microcents INTEGER NOT NULL DEFAULT 0,
                    FOREIGN KEY(session_id) REFERENCES forensic_sessions(session_id) ON DELETE CASCADE
                );

                INSERT INTO schema_migrations (version, applied_at)
                VALUES (5, datetime('now'));
                COMMIT;",
            )?;

            // Resilient column checks for pre-existing databases
            let has_col = |table: &str, col: &str| -> bool {
                if let Ok(mut stmt) = conn.prepare(&format!("PRAGMA table_info({});", table)) {
                    if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(1)) {
                        for name in rows.flatten() {
                            if name == col {
                                return true;
                            }
                        }
                    }
                }
                false
            };

            if !has_col("trajectory_events", "tokens_spent") {
                let _ = conn.execute("ALTER TABLE trajectory_events ADD COLUMN tokens_spent INTEGER NOT NULL DEFAULT 0;", []);
            }
            if !has_col("trajectory_events", "tokens_saved") {
                let _ = conn.execute("ALTER TABLE trajectory_events ADD COLUMN tokens_saved INTEGER NOT NULL DEFAULT 0;", []);
            }
            if !has_col("ai_working_cache", "last_receipt_token") {
                let _ = conn.execute(
                    "ALTER TABLE ai_working_cache ADD COLUMN last_receipt_token TEXT;",
                    [],
                );
            }
        }

        Ok(())
    }

    /// Highest applied schema migration version.
    ///
    /// Used by the CLI `doctor` command and by migration tests.
    pub fn schema_version(&self) -> Result<i64, StorageError> {
        let conn = self.conn.lock().unwrap();
        let version = conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations;",
            [],
            |row| row.get(0),
        )?;
        Ok(version)
    }

    /// Returns a reference-counted clone of the database connection handle.
    pub fn raw_connection(&self) -> Arc<Mutex<Connection>> {
        self.conn.clone()
    }

    /// Persists an immutable repository snapshot into storage.
    pub fn save_snapshot(&self, snapshot: &RepoSnapshot) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        let json = serde_json::to_string(snapshot)?;

        conn.execute(
            "INSERT OR REPLACE INTO snapshots (snapshot_id, revision, config_hash, file_set_digest, snapshot_json)
             VALUES (?1, ?2, ?3, ?4, ?5);",
            params![
                snapshot.snapshot_id.as_str(),
                snapshot.revision,
                snapshot.config_hash,
                snapshot.file_set_digest,
                json,
            ],
        )?;

        Ok(())
    }

    /// Retrieves a snapshot by its SnapshotId.
    pub fn get_snapshot(&self, id: &SnapshotId) -> Result<Option<RepoSnapshot>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let json_opt: Option<String> = conn
            .query_row(
                "SELECT snapshot_json FROM snapshots WHERE snapshot_id = ?1;",
                params![id.as_str()],
                |row| row.get(0),
            )
            .optional()?;

        match json_opt {
            Some(json) => {
                let snap = serde_json::from_str(&json)?;
                Ok(Some(snap))
            }
            None => Ok(None),
        }
    }

    /// Retrieves the most recent snapshot stored in the database.
    pub fn get_latest_snapshot(&self) -> Result<Option<RepoSnapshot>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let json_opt: Option<String> = conn
            .query_row(
                "SELECT snapshot_json FROM snapshots ORDER BY rowid DESC LIMIT 1;",
                [],
                |row| row.get(0),
            )
            .optional()?;

        match json_opt {
            Some(json) => {
                let snap = serde_json::from_str(&json)?;
                Ok(Some(snap))
            }
            None => Ok(None),
        }
    }

    /// Persists an analysis run with its associated findings and evidence receipts.
    pub fn save_run(
        &self,
        run: &AnalysisRun,
        findings: &[Finding],
        evidence: &[EvidenceReceipt],
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT OR REPLACE INTO runs (run_id, command, analyzer_version, duration_ms, result_digest)
             VALUES (?1, ?2, ?3, ?4, ?5);",
            params![
                run.run_id,
                run.command,
                run.analyzer_version,
                run.duration_ms,
                run.result_digest,
            ],
        )?;

        for f in findings {
            let entities_json = serde_json::to_string(&f.entities)?;
            let status_str = serde_json::to_value(f.status)?
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            let sev_str = serde_json::to_value(f.severity)?
                .as_str()
                .unwrap_or("info")
                .to_string();
            let cov_str = serde_json::to_value(f.coverage)?
                .as_str()
                .unwrap_or("unknown")
                .to_string();

            tx.execute(
                "INSERT OR REPLACE INTO findings (id, run_id, rule_id, status, severity, message, coverage, entities_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8);",
                params![
                    f.id,
                    run.run_id,
                    f.rule_id,
                    status_str,
                    sev_str,
                    f.message,
                    cov_str,
                    entities_json,
                ],
            )?;
        }

        for ev in evidence {
            let ev_json = serde_json::to_string(ev)?;
            tx.execute(
                "INSERT OR REPLACE INTO evidence (finding_id, rule_id, evidence_digest, evidence_json)
                 VALUES (?1, ?2, ?3, ?4);",
                params![ev.finding_id, ev.rule_id, ev.evidence_digest, ev_json],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Retrieves an analysis run by run ID.
    pub fn get_run(&self, run_id: &str) -> Result<Option<AnalysisRun>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let run_opt = conn
            .query_row(
                "SELECT run_id, command, analyzer_version, duration_ms, result_digest FROM runs WHERE run_id = ?1;",
                params![run_id],
                |row| {
                    Ok(AnalysisRun {
                        run_id: row.get(0)?,
                        command: row.get(1)?,
                        analyzer_version: row.get(2)?,
                        duration_ms: row.get(3)?,
                        result_digest: row.get(4)?,
                    })
                },
            )
            .optional()?;

        Ok(run_opt)
    }

    /// Retrieves all findings associated with a run ID.
    pub fn get_findings_by_run(&self, run_id: &str) -> Result<Vec<Finding>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, rule_id, status, severity, message, coverage, entities_json FROM findings WHERE run_id = ?1 ORDER BY id ASC;",
        )?;

        let rows = stmt.query_map(params![run_id], |row| {
            let id: String = row.get(0)?;
            let rule_id: String = row.get(1)?;
            let status_str: String = row.get(2)?;
            let sev_str: String = row.get(3)?;
            let message: String = row.get(4)?;
            let cov_str: String = row.get(5)?;
            let entities_json: String = row.get(6)?;

            let status = serde_json::from_value(serde_json::Value::String(status_str))
                .unwrap_or(autopsy_domain::FindingStatus::Unknown);
            let severity = serde_json::from_value(serde_json::Value::String(sev_str))
                .unwrap_or(autopsy_domain::Severity::Info);
            let coverage = serde_json::from_value(serde_json::Value::String(cov_str))
                .unwrap_or(autopsy_domain::CoverageState::Unknown);
            let entities = serde_json::from_str(&entities_json).unwrap_or_default();

            Ok(Finding {
                id,
                rule_id,
                status,
                severity,
                message,
                entities,
                coverage,
            })
        })?;

        let mut findings = Vec::new();
        for r in rows {
            findings.push(r?);
        }
        Ok(findings)
    }

    /// Retrieves evidence by finding ID.
    pub fn get_evidence(&self, finding_id: &str) -> Result<Option<EvidenceReceipt>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let json_opt: Option<String> = conn
            .query_row(
                "SELECT evidence_json FROM evidence WHERE finding_id = ?1;",
                params![finding_id],
                |row| row.get(0),
            )
            .optional()?;

        match json_opt {
            Some(json) => {
                let ev = serde_json::from_str(&json)?;
                Ok(Some(ev))
            }
            None => Ok(None),
        }
    }

    /// Stores a binary blob in the content-addressed object store (`.autopsy/cache/objects/xx/yy...`).
    /// Returns the BLAKE3 hex hash of the content.
    pub fn put_cache_object(&self, content: &[u8]) -> Result<String, StorageError> {
        let hash = blake3::hash(content).to_hex().to_string();

        if let Some(cache_root) = &self.cache_dir {
            let prefix = &hash[0..2];
            let rest = &hash[2..];
            let dir = cache_root.join("objects").join(prefix);
            fs::create_dir_all(&dir)?;

            let target_path = dir.join(rest);
            if !target_path.exists() {
                // Atomic write via PID-isolated temporary file
                let tmp_path = dir.join(format!(".tmp-{}-{}", std::process::id(), rest));
                fs::write(&tmp_path, content)?;
                match fs::rename(&tmp_path, &target_path) {
                    Ok(()) => {}
                    Err(_) if target_path.exists() => {
                        // Concurrent writer finished first; safely clean up temporary file
                        let _ = fs::remove_file(&tmp_path);
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        }

        Ok(hash)
    }

    /// Retrieves a binary blob from the content-addressed object store by its BLAKE3 hash.
    pub fn get_cache_object(&self, hash: &str) -> Result<Option<Vec<u8>>, StorageError> {
        if let Some(cache_root) = &self.cache_dir {
            if hash.len() < 3 {
                return Ok(None);
            }
            let prefix = &hash[0..2];
            let rest = &hash[2..];
            let target_path = cache_root.join("objects").join(prefix).join(rest);

            if target_path.exists() {
                let bytes = fs::read(target_path)?;
                return Ok(Some(bytes));
            }
        }
        Ok(None)
    }

    /// Validates a content hash for safe use as a cache shard path.
    /// Only full-length lowercase BLAKE3 hex digests are accepted,
    /// closing path-injection via crafted snapshot records.
    fn validate_content_hash(hash: &str) -> Result<(), StorageError> {
        if hash.len() == 64
            && hash
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            Ok(())
        } else {
            Err(StorageError::Migration(format!(
                "invalid content hash for artifact cache: '{}'",
                hash.chars().take(32).collect::<String>()
            )))
        }
    }

    /// Stores a per-file analysis artifact under its content hash
    /// (`.autopsy/cache/artifacts/xx/rest`). Idempotent: identical bytes
    /// map to identical keys, so unchanged files need no rewrite.
    pub fn put_file_artifact(
        &self,
        content_hash: &str,
        artifact_json: &[u8],
    ) -> Result<(), StorageError> {
        Self::validate_content_hash(content_hash)?;
        if let Some(cache_root) = &self.cache_dir {
            let dir = cache_root.join("artifacts").join(&content_hash[0..2]);
            fs::create_dir_all(&dir)?;
            let target_path = dir.join(&content_hash[2..]);
            if !target_path.exists() {
                let tmp_path = dir.join(format!(
                    ".tmp-{}-{}",
                    std::process::id(),
                    &content_hash[2..8]
                ));
                fs::write(&tmp_path, artifact_json)?;
                match fs::rename(&tmp_path, &target_path) {
                    Ok(()) => {}
                    Err(_) if target_path.exists() => {
                        let _ = fs::remove_file(&tmp_path);
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        }
        Ok(())
    }

    /// Retrieves a per-file analysis artifact by content hash.
    pub fn get_file_artifact(&self, content_hash: &str) -> Result<Option<Vec<u8>>, StorageError> {
        Self::validate_content_hash(content_hash)?;
        if let Some(cache_root) = &self.cache_dir {
            let target_path = cache_root
                .join("artifacts")
                .join(&content_hash[0..2])
                .join(&content_hash[2..]);
            if target_path.exists() {
                let bytes = fs::read(target_path)?;
                return Ok(Some(bytes));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autopsy_domain::{CoverageState, FindingStatus, Severity, SourceLocation};
    use std::collections::BTreeMap;

    #[test]
    fn test_storage_migrations_and_roundtrip() {
        let storage = StorageEngine::in_memory().unwrap();

        // 1. Snapshot save & get
        let snapshot = RepoSnapshot {
            snapshot_id: SnapshotId::new("snap_test_123"),
            revision: "git_sha_abc".to_string(),
            config_hash: "cfg_hash_1".to_string(),
            file_set_digest: "files_digest_1".to_string(),
            files: BTreeMap::new(),
            skipped_oversize_files: Vec::new(),
        };
        storage.save_snapshot(&snapshot).unwrap();

        let retrieved = storage
            .get_snapshot(&snapshot.snapshot_id)
            .unwrap()
            .unwrap();
        assert_eq!(retrieved, snapshot);

        // 2. Run & Finding & Evidence save & get
        let run = AnalysisRun {
            run_id: "run-001".to_string(),
            command: "verify".to_string(),
            analyzer_version: "0.0.1".to_string(),
            duration_ms: 42,
            result_digest: "digest-abc".to_string(),
        };

        let finding = Finding {
            id: "F-001".to_string(),
            rule_id: "RULE-1".to_string(),
            status: FindingStatus::Fail,
            severity: Severity::Error,
            message: "Violation detected".to_string(),
            entities: vec!["a::b".to_string()],
            coverage: CoverageState::Verified,
        };

        let evidence = EvidenceReceipt {
            finding_id: "F-001".to_string(),
            rule_id: "RULE-1".to_string(),
            snapshots: vec![snapshot.snapshot_id.clone()],
            analyzer_version: "0.0.1".to_string(),
            locations: vec![SourceLocation {
                path: "src/a.ts".to_string(),
                start_line: 1,
                end_line: 5,
                start_col: None,
                end_col: None,
                symbol_id: None,
            }],
            paths: vec![vec!["src/a.ts".to_string()]],
            deltas: vec!["delta-1".to_string()],
            evidence_digest: "ev-digest-1".to_string(),
        };

        storage
            .save_run(
                &run,
                std::slice::from_ref(&finding),
                std::slice::from_ref(&evidence),
            )
            .unwrap();

        let fetched_run = storage.get_run("run-001").unwrap().unwrap();
        assert_eq!(fetched_run.run_id, "run-001");

        let fetched_findings = storage.get_findings_by_run("run-001").unwrap();
        assert_eq!(fetched_findings.len(), 1);
        assert_eq!(fetched_findings[0], finding);

        let fetched_evidence = storage.get_evidence("F-001").unwrap().unwrap();
        assert_eq!(fetched_evidence, evidence);
    }

    #[test]
    fn test_content_addressed_cache() {
        let temp_dir =
            std::env::temp_dir().join(format!("autopsy-test-cache-{}", std::process::id()));
        let storage = StorageEngine::open(
            temp_dir.join("autopsy.db"),
            StorageOptions {
                use_wal: true,
                cache_dir: Some(temp_dir.join("cache")),
            },
        )
        .unwrap();

        let sample_data = b"export const answer = 42;";
        let hash = storage.put_cache_object(sample_data).unwrap();

        let retrieved = storage.get_cache_object(&hash).unwrap().unwrap();
        assert_eq!(retrieved, sample_data);

        // Clean up
        let _ = fs::remove_dir_all(temp_dir);
    }

    /// Creates a database containing *only* the v1 schema, to prove that
    /// migration 002 upgrades a pre-existing v1 database in place.
    fn create_v1_only_database(path: &Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );
            CREATE TABLE snapshots (
                snapshot_id TEXT PRIMARY KEY,
                revision TEXT NOT NULL,
                config_hash TEXT NOT NULL,
                file_set_digest TEXT NOT NULL,
                snapshot_json TEXT NOT NULL
            );
            INSERT INTO schema_migrations (version, applied_at)
            VALUES (1, datetime('now'));",
        )
        .unwrap();
    }

    fn table_names(engine: &StorageEngine) -> Vec<String> {
        let conn = engine.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name;")
            .unwrap();
        stmt.query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    fn index_names(engine: &StorageEngine) -> Vec<String> {
        let conn = engine.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'index' ORDER BY name;")
            .unwrap();
        stmt.query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    #[test]
    fn test_migration_002_creates_pillar_tables() {
        let engine = StorageEngine::in_memory().unwrap();
        // 002 creates the tables; 003 reshapes two of them; 004 adds Phase 1 visual tables; 005 adds Phase 4 forensics.
        assert_eq!(engine.schema_version().unwrap(), 5);

        let tables = table_names(&engine);
        for expected in [
            "temporal_facts",
            "test_mappings",
            "distributed_contracts",
            "co_change_matrix",
            "repositories",
            "graph_nodes",
            "graph_edges",
            "forensic_sessions",
            "trajectory_events",
            "ai_working_cache",
            "forensic_workflows",
            "ai_session_ledger",
        ] {
            assert!(
                tables.contains(&expected.to_string()),
                "missing table {expected}; found {tables:?}"
            );
        }
    }

    #[test]
    fn test_migration_002_creates_indexes() {
        let engine = StorageEngine::in_memory().unwrap();
        let indexes = index_names(&engine);
        for expected in [
            "idx_temporal_facts_subject",
            "idx_temporal_facts_span",
            "idx_temporal_facts_predicate",
            "idx_test_mappings_target",
            "idx_distributed_contracts_service",
            "idx_distributed_contracts_endpoint",
            "idx_co_change_matrix_a",
            "idx_co_change_matrix_b",
            "idx_workflow_session",
        ] {
            assert!(
                indexes.contains(&expected.to_string()),
                "missing index {expected}; found {indexes:?}"
            );
        }
    }

    #[test]
    fn test_migration_002_applies_cleanly_on_v1_database() {
        let temp_dir =
            std::env::temp_dir().join(format!("autopsy-test-mig-v1-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("autopsy.db");

        create_v1_only_database(&db_path);

        // Sanity: the fixture really is v1-only.
        {
            let conn = Connection::open(&db_path).unwrap();
            let v: i64 = conn
                .query_row("SELECT MAX(version) FROM schema_migrations;", [], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(v, 1);
        }

        // Opening through the engine must upgrade in place.
        let engine = StorageEngine::open(
            &db_path,
            StorageOptions {
                use_wal: false,
                cache_dir: None,
            },
        )
        .unwrap();
        // The upgrade runs through 002, 003, 004 *and* 005 in place.
        assert_eq!(engine.schema_version().unwrap(), 5);

        let tables = table_names(&engine);
        assert!(tables.contains(&"temporal_facts".to_string()));
        assert!(tables.contains(&"test_mappings".to_string()));
        assert!(tables.contains(&"distributed_contracts".to_string()));
        assert!(tables.contains(&"co_change_matrix".to_string()));
        assert!(tables.contains(&"forensic_workflows".to_string()));
        assert!(tables.contains(&"ai_session_ledger".to_string()));

        // v1 data survives the upgrade.
        assert!(tables.contains(&"snapshots".to_string()));

        drop(engine);
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_migration_is_idempotent_across_reopens() {
        let temp_dir =
            std::env::temp_dir().join(format!("autopsy-test-mig-idem-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("autopsy.db");
        let opts = || StorageOptions {
            use_wal: false,
            cache_dir: None,
        };

        for _ in 0..3 {
            let engine = StorageEngine::open(&db_path, opts()).unwrap();
            assert_eq!(engine.schema_version().unwrap(), 5);
            // Re-running migrations explicitly must be a no-op.
            let mut engine = engine;
            engine.run_migrations().unwrap();
            assert_eq!(engine.schema_version().unwrap(), 5);
        }

        // Exactly one row per migration version: no duplicate application.
        let conn = Connection::open(&db_path).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations;", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            count, 5,
            "expected versions 1, 2, 3, 4 and 5 exactly once each"
        );

        let _ = fs::remove_dir_all(temp_dir);
    }

    /// Creates a database containing *only* the v2 pillar schema, to prove migration 003
    /// upgrades a pre-existing v2 database in place. Two documented consequences are
    /// asserted below: the v2 co-change shape is replaced (it had no writer, so the rows
    /// it held were development rows), and a v2 fact row survives with its coverage
    /// defaulted — the `DEFAULT 'unknown'` that SQLite's ALTER TABLE requires.
    fn create_v2_only_database(path: &Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );
            CREATE TABLE temporal_facts (
                fact_id TEXT PRIMARY KEY,
                subject TEXT NOT NULL,
                predicate TEXT NOT NULL,
                object TEXT NOT NULL,
                valid_from TEXT NOT NULL,
                valid_to TEXT,
                evidence_digest TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE co_change_matrix (
                entity_a TEXT NOT NULL,
                entity_b TEXT NOT NULL,
                commit_count INTEGER NOT NULL,
                lift REAL NOT NULL,
                window_start TEXT NOT NULL,
                window_end TEXT NOT NULL,
                PRIMARY KEY (entity_a, entity_b)
            );
            INSERT INTO schema_migrations (version, applied_at)
            VALUES (2, datetime('now'));
            INSERT INTO temporal_facts
                (fact_id, subject, predicate, object, valid_from, valid_to, evidence_digest, created_at)
            VALUES ('f-1', 'src/a.ts', 'file_present', 'blob:1', 'r1', NULL, 'd', 0);
            INSERT INTO co_change_matrix
                (entity_a, entity_b, commit_count, lift, window_start, window_end)
            VALUES ('old.ts', 'older.ts', 3, 1.5, 'w0', 'w1');",
        )
        .unwrap();
    }

    #[test]
    fn test_migration_003_upgrades_v2_database_in_place() {
        let temp_dir =
            std::env::temp_dir().join(format!("autopsy-test-mig-v3-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("autopsy.db");

        create_v2_only_database(&db_path);

        let mut engine = StorageEngine::open(
            &db_path,
            StorageOptions {
                use_wal: false,
                cache_dir: None,
            },
        )
        .unwrap();
        assert_eq!(engine.schema_version().unwrap(), 5);

        // The v2 fact survived, and its coverage is the documented ALTER default. A row
        // that read back as `verified` would be a lie; `unknown` says "no coverage stated".
        let facts = engine.load_temporal_facts().unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].fact_id, "f-1");
        assert_eq!(facts[0].coverage, "unknown");

        // The engine can now write an honest coverage for it.
        let updated = TemporalFactRow {
            coverage: "verified".to_string(),
            ..facts[0].clone()
        };
        engine
            .save_temporal_facts(std::slice::from_ref(&updated))
            .unwrap();
        assert_eq!(engine.load_temporal_facts().unwrap(), vec![updated]);

        // The v2 edge row is gone with its table, and the new shape accepts rows.
        assert!(engine.load_co_change_edges(None).unwrap().is_empty());
        let edge = CoChangeEdgeRow {
            entity_a: "a.ts".to_string(),
            entity_b: "b.ts".to_string(),
            window_id: "w".repeat(64),
            commits_together: 4,
            support_ppm: 100_000,
            confidence_a_to_b_ppm: 200_000,
            lift_ppm: 1_000_000,
            claim: "temporal_co_occurrence_only".to_string(),
            coverage: "partial".to_string(),
        };
        engine
            .save_co_change_edges(std::slice::from_ref(&edge))
            .unwrap();
        assert_eq!(engine.load_co_change_edges(None).unwrap(), vec![edge]);

        // Re-running migrations on the upgraded database is a no-op.
        engine.run_migrations().unwrap();
        assert_eq!(engine.schema_version().unwrap(), 5);

        drop(engine);
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_temporal_facts_accepts_valid_row_and_allows_open_interval() {
        let engine = StorageEngine::in_memory().unwrap();
        let conn = engine.conn.lock().unwrap();

        // valid_to NULL means "currently active".
        conn.execute(
            "INSERT INTO temporal_facts
             (fact_id, subject, predicate, object, valid_from, valid_to, evidence_digest, created_at)
             VALUES ('fact-1', 'urn:a', 'calls', 'urn:b', 'abc123', NULL, 'digest-1', 0);",
            [],
        )
        .unwrap();

        let open_interval: Option<String> = conn
            .query_row(
                "SELECT valid_to FROM temporal_facts WHERE fact_id = 'fact-1';",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(open_interval, None);

        // fact_id is the primary key: duplicates are rejected.
        let dup = conn.execute(
            "INSERT INTO temporal_facts
             (fact_id, subject, predicate, object, valid_from, valid_to, evidence_digest, created_at)
             VALUES ('fact-1', 'urn:x', 'calls', 'urn:y', 'def456', NULL, 'digest-2', 0);",
            [],
        );
        assert!(dup.is_err(), "fact_id must be unique");
    }

    #[test]
    fn test_test_mappings_composite_primary_key() {
        let engine = StorageEngine::in_memory().unwrap();
        let conn = engine.conn.lock().unwrap();
        let insert = |source: &str| {
            conn.execute(
                "INSERT INTO test_mappings
                 (test_suite_id, target_symbol_id, mapping_source, evidence_digest)
                 VALUES ('suite-1', 'sym-1', ?1, 'd');",
                params![source],
            )
        };

        insert("ast_reference").unwrap();
        // Same (suite, symbol) but a different provenance is a distinct row.
        insert("observed_coverage").unwrap();
        // Exact duplicate of the composite key is rejected.
        assert!(insert("ast_reference").is_err());

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM test_mappings;", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_distributed_contracts_protocol_roundtrip() {
        let engine = StorageEngine::in_memory().unwrap();
        let conn = engine.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO distributed_contracts
             (contract_id, protocol, service_id, endpoint_or_channel, spec_digest, parsed_json)
             VALUES ('c-1', 'openapi_3_2_1', 'svc-a', 'GET /invoices', 'sd', '{}');",
            [],
        )
        .unwrap();

        let protocol: String = conn
            .query_row(
                "SELECT protocol FROM distributed_contracts WHERE contract_id = 'c-1';",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(protocol, "openapi_3_2_1");
    }

    #[test]
    fn test_co_change_matrix_composite_key_and_lift_storage() {
        // Migration 003 reshaped this table to what the engine actually produces: the
        // window is an id (the commit-set digest), the ratios are the engine's ppm
        // integers, and the claim column keeps association from reading as causation.
        // The v2 shape (wall-clock window, float lift, no claim) had no writer, so the
        // test documents the v3 shape through the public write path.
        let engine = StorageEngine::in_memory().unwrap();
        let row = CoChangeEdgeRow {
            entity_a: "a.ts".to_string(),
            entity_b: "b.ts".to_string(),
            window_id: "d".repeat(64),
            commits_together: 7,
            support_ppm: 250_000,
            confidence_a_to_b_ppm: 350_000,
            lift_ppm: 3_250_000,
            claim: "temporal_co_occurrence_only".to_string(),
            coverage: "verified".to_string(),
        };
        engine
            .save_co_change_edges(std::slice::from_ref(&row))
            .unwrap();

        let loaded = engine.load_co_change_edges(None).unwrap();
        assert_eq!(loaded, vec![row.clone()]);
        // A second window coexists; neither overwrites the other.
        let other = CoChangeEdgeRow {
            window_id: "e".repeat(64),
            ..row.clone()
        };
        engine
            .save_co_change_edges(std::slice::from_ref(&other))
            .unwrap();
        let both = engine.load_co_change_edges(None).unwrap();
        assert_eq!(both.len(), 2);
        assert_eq!(
            engine.load_co_change_edges(Some(&row.window_id)).unwrap(),
            vec![row.clone()]
        );

        // The natural key is (a, b, window): re-saving replaces its own edge only.
        let replaced = CoChangeEdgeRow {
            commits_together: 9,
            ..row.clone()
        };
        engine
            .save_co_change_edges(std::slice::from_ref(&replaced))
            .unwrap();
        let edges = engine.load_co_change_edges(Some(&row.window_id)).unwrap();
        assert_eq!(edges, vec![replaced]);
    }

    #[test]
    fn test_phase1_sqlite_wal_roundtrip_and_line_edges() {
        let engine = StorageEngine::in_memory().unwrap();

        // 1. Register repo
        let repo = RepoRecord {
            repo_id: "repo_test_1".to_string(),
            root_path: "/workspace/test-repo".to_string(),
            display_name: "Test Repo".to_string(),
            active_branch: Some("main".to_string()),
            head_commit_oid: Some("a1b2c3d4e5".to_string()),
            latest_snap_id: Some("snap_001".to_string()),
            languages_json: "[\"typescript\",\"rust\"]".to_string(),
            monorepo_json: "{\"type\":\"pnpm\"}".to_string(),
            last_indexed_seq: 1,
        };
        engine.register_repository(&repo).unwrap();
        let repos = engine.list_repositories().unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].repo_id, "repo_test_1");

        // 2. Insert valid baseline snapshot record
        {
            let conn = engine.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO snapshots (snapshot_id, revision, config_hash, file_set_digest, snapshot_json)
                 VALUES ('snap_001', 'rev_1', 'cfg_1', 'digest_1', '{}');",
                [],
            )
            .unwrap();
        }

        // 3. Save graph nodes & line-to-line edges
        let nodes = vec![
            GraphNodeRow {
                snapshot_id: "snap_001".to_string(),
                stable_id: "OrderController.createOrder".to_string(),
                node_kind: "function".to_string(),
                qualified_name: "OrderController.createOrder".to_string(),
                file_path: "src/OrderController.ts".to_string(),
                start_line: 40,
                end_line: 65,
                signature_hash: "sig_abc".to_string(),
                visibility: "public".to_string(),
                is_exported: true,
                is_dead_code: false,
                coverage_state: "verified".to_string(),
            },
            GraphNodeRow {
                snapshot_id: "snap_001".to_string(),
                stable_id: "PaymentService.chargeCard".to_string(),
                node_kind: "function".to_string(),
                qualified_name: "PaymentService.chargeCard".to_string(),
                file_path: "src/PaymentService.ts".to_string(),
                start_line: 30,
                end_line: 55,
                signature_hash: "sig_xyz".to_string(),
                visibility: "public".to_string(),
                is_exported: true,
                is_dead_code: false,
                coverage_state: "verified".to_string(),
            },
        ];

        let edges = vec![GraphEdgeRow {
            snapshot_id: "snap_001".to_string(),
            edge_id: "edge_01".to_string(),
            source_stable_id: "OrderController.createOrder".to_string(),
            target_stable_id: "PaymentService.chargeCard".to_string(),
            edge_kind: "calls".to_string(),
            source_file: "src/OrderController.ts".to_string(),
            source_line: 48, // Exact calling line!
            source_col: 12,
            target_file: "src/PaymentService.ts".to_string(),
            target_line: 30, // Exact definition line!
            confidence: "exact".to_string(),
        }];

        engine.save_graph_nodes_and_edges(&nodes, &edges).unwrap();

        // 3. Query Ego Network in <5ms
        let ego = engine
            .query_ego_network("snap_001", "PaymentService.chargeCard")
            .unwrap();
        assert!(ego.target_node.is_some());
        assert_eq!(ego.incoming_callers.len(), 1);
        assert_eq!(ego.incoming_callers[0].source_line, 48);
        assert_eq!(ego.incoming_callers[0].target_line, 30);
        assert_eq!(
            ego.incoming_callers[0].source_stable_id,
            "OrderController.createOrder"
        );
        assert_eq!(ego.outgoing_callees.len(), 0);

        // 4. Trajectory timeline event
        let event = TrajectoryEventRecord {
            event_seq: 1,
            session_id: "sess_001".to_string(),
            job_id: Some("job_verify".to_string()),
            workflow_id: Some("wf_commit".to_string()),
            event_type: "verify_run".to_string(),
            target_symbol: Some("PaymentService.chargeCard".to_string()),
            target_file: Some("src/PaymentService.ts".to_string()),
            before_snap_id: Some("snap_000".to_string()),
            after_snap_id: Some("snap_001".to_string()),
            verdict: "PASS".to_string(),
            summary: "All contracts intact".to_string(),
            evidence_json: "{}".to_string(),
            blake3_receipt: "blake3:abc123456789".to_string(),
            tokens_spent: 120,
            tokens_saved: 450,
        };
        let seq = engine.record_trajectory_event(&event).unwrap();
        assert_eq!(seq, 1);
        let timeline = engine.list_trajectory_events("sess_001").unwrap();
        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline[0].verdict, "PASS");
        assert_eq!(timeline[0].tokens_saved, 450);

        // 5. AI Working Cache
        let cache_entry = AiWorkingCacheEntry {
            cache_key: "ai_key_chargeCard".to_string(),
            repo_id: "repo_test_1".to_string(),
            snapshot_id: "snap_001".to_string(),
            query_kind: "callers".to_string(),
            target_id: "PaymentService.chargeCard".to_string(),
            compact_payload: "{\"callers\":[\"OrderController.createOrder\"]}".to_string(),
            token_estimate: 45,
            hit_count: 0,
            last_receipt_token: Some("rcpt_01".to_string()),
        };
        engine.set_ai_working_cache(&cache_entry).unwrap();
        let hit = engine
            .get_ai_working_cache("ai_key_chargeCard")
            .unwrap()
            .unwrap();
        assert_eq!(hit.hit_count, 0);
        // Second read should increment hit count
        let hit2 = engine
            .get_ai_working_cache("ai_key_chargeCard")
            .unwrap()
            .unwrap();
        assert_eq!(hit2.hit_count, 1);
    }

    #[test]
    fn test_phase1_sqlite_corruption_self_healing() {
        let temp_dir =
            std::env::temp_dir().join(format!("synevid_heal_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::create_dir_all(&temp_dir);
        let db_file = temp_dir.join("storage.db");

        // 1. Initialize valid DB
        {
            let engine = StorageEngine::open(&db_file, StorageOptions::default()).unwrap();
            let repo = RepoRecord {
                repo_id: "test".to_string(),
                root_path: "/test".to_string(),
                display_name: "Test".to_string(),
                active_branch: None,
                head_commit_oid: None,
                latest_snap_id: None,
                languages_json: "[]".to_string(),
                monorepo_json: "{}".to_string(),
                last_indexed_seq: 0,
            };
            engine.register_repository(&repo).unwrap();
        }

        // 2. Corrupt file by overwriting with random junk bytes
        std::fs::write(&db_file, b"CORRUPTED_GARBAGE_BYTES_THAT_ARE_NOT_SQLITE").unwrap();

        // 3. Self-healing open_or_recover must detect corruption, back up to .bak, and recreate cleanly
        let recovered = StorageEngine::open_or_recover(&db_file, StorageOptions::default())
            .expect("open_or_recover should self-heal");
        let repos = recovered.list_repositories().unwrap();
        assert_eq!(repos.len(), 0); // Recreated clean
        assert!(temp_dir.join("storage.db.bak").exists()); // .bak was preserved!

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_session_workflow_job_timeline_lifecycle() {
        let engine = StorageEngine::in_memory().unwrap();

        // 1. Forensic Session (Tier 1)
        let session = ForensicSessionRecord {
            session_id: "sess_p4_001".to_string(),
            repo_id: "repo_billing".to_string(),
            actor_kind: "AI_AGENT".to_string(),
            actor_name: "Claude-Code".to_string(),
            baseline_snap_id: "blake3:base000".to_string(),
            current_snap_id: "blake3:base000".to_string(),
            status: "ACTIVE".to_string(),
        };
        engine.create_forensic_session(&session).unwrap();

        let loaded_sess = engine.get_forensic_session("sess_p4_001").unwrap().unwrap();
        assert_eq!(loaded_sess.actor_name, "Claude-Code");
        assert_eq!(loaded_sess.status, "ACTIVE");

        // 2. Forensic Workflow (Tier 2)
        let workflow = ForensicWorkflowRecord {
            workflow_id: "wf_p4_refactor_auth".to_string(),
            session_id: "sess_p4_001".to_string(),
            name: "Pre-Edit Guardrail -> Multi-File Edit -> Verification Loop".to_string(),
            status: "RUNNING".to_string(),
            start_seq: 1,
            end_seq: None,
            duration_ms: 0,
        };
        engine.create_forensic_workflow(&workflow).unwrap();

        let workflows = engine.list_forensic_workflows("sess_p4_001").unwrap();
        assert_eq!(workflows.len(), 1);
        assert_eq!(
            workflows[0].name,
            "Pre-Edit Guardrail -> Multi-File Edit -> Verification Loop"
        );

        // 3. 5 Sequential Operations (Tier 3)
        let ops = vec![
            (
                "baseline_created",
                "PaymentService",
                "src/PaymentService.ts",
                "INFO",
                "Baseline snapshot pinned",
                "blake3:a111",
                50,
                0,
            ),
            (
                "impact_queried",
                "chargeCard",
                "src/PaymentService.ts",
                "INFO",
                "10 callers mapped",
                "blake3:b222",
                180,
                25000,
            ),
            (
                "simulate_edit",
                "chargeCard",
                "src/PaymentService.ts",
                "FAIL",
                "2 broken callers detected",
                "blake3:c333",
                90,
                8000,
            ),
            (
                "simulate_edit",
                "chargeCard",
                "src/PaymentService.ts",
                "PASS",
                "Contract repaired",
                "blake3:d444",
                60,
                8000,
            ),
            (
                "verify_run",
                "PaymentService",
                "src/PaymentService.ts",
                "PASS",
                "Verified invariant pass",
                "blake3:e555",
                120,
                15000,
            ),
        ];

        let mut seqs = Vec::new();
        for (event_type, sym, file, verdict, summary, receipt, spent, saved) in ops {
            let evt = TrajectoryEventRecord {
                event_seq: 0,
                session_id: "sess_p4_001".to_string(),
                job_id: Some(format!("job_{}", event_type)),
                workflow_id: Some("wf_p4_refactor_auth".to_string()),
                event_type: event_type.to_string(),
                target_symbol: Some(sym.to_string()),
                target_file: Some(file.to_string()),
                before_snap_id: Some("snap_A".to_string()),
                after_snap_id: Some("snap_B".to_string()),
                verdict: verdict.to_string(),
                summary: summary.to_string(),
                evidence_json: "{}".to_string(),
                blake3_receipt: receipt.to_string(),
                tokens_spent: spent,
                tokens_saved: saved,
            };
            let seq = engine.record_trajectory_event(&evt).unwrap();
            seqs.push(seq);
        }

        assert_eq!(seqs, vec![1, 2, 3, 4, 5]);

        // Verify sequence order and non-empty receipts
        let timeline = engine.list_trajectory_events("sess_p4_001").unwrap();
        assert_eq!(timeline.len(), 5);
        for (i, evt) in timeline.iter().enumerate() {
            assert_eq!(evt.event_seq, (i + 1) as i64);
            assert!(evt.blake3_receipt.starts_with("blake3:"));
        }

        // Test range query (e.g., Step #2 to #4)
        let slice = engine
            .list_trajectory_events_range("sess_p4_001", 2, 4)
            .unwrap();
        assert_eq!(slice.len(), 3);
        assert_eq!(slice[0].event_seq, 2);
        assert_eq!(slice[2].event_seq, 4);

        // 4. AI Session Ledger
        for evt in &timeline {
            let hit = evt.tokens_saved > 0;
            engine
                .record_ai_query("sess_p4_001", hit, evt.tokens_spent, evt.tokens_saved)
                .unwrap();
        }

        let ledger = engine.get_or_create_ai_ledger("sess_p4_001").unwrap();
        assert_eq!(ledger.total_queries, 5);
        assert_eq!(ledger.cache_hits, 4);
        assert_eq!(ledger.tokens_spent, 500);
        assert_eq!(ledger.tokens_saved, 56000);
        assert!(ledger.cost_saved_microcents > 0);
    }
}
