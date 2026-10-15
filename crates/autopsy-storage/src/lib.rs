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
        // Enforce foreign keys and normal synchronous mode
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
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
        assert_eq!(engine.schema_version().unwrap(), 2);

        let tables = table_names(&engine);
        for expected in [
            "temporal_facts",
            "test_mappings",
            "distributed_contracts",
            "co_change_matrix",
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
        assert_eq!(engine.schema_version().unwrap(), 2);

        let tables = table_names(&engine);
        assert!(tables.contains(&"temporal_facts".to_string()));
        assert!(tables.contains(&"test_mappings".to_string()));
        assert!(tables.contains(&"distributed_contracts".to_string()));
        assert!(tables.contains(&"co_change_matrix".to_string()));

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
            assert_eq!(engine.schema_version().unwrap(), 2);
            // Re-running migrations explicitly must be a no-op.
            let mut engine = engine;
            engine.run_migrations().unwrap();
            assert_eq!(engine.schema_version().unwrap(), 2);
        }

        // Exactly one row per migration version: no duplicate application.
        let conn = Connection::open(&db_path).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations;", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2, "expected versions 1 and 2 exactly once each");

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
        let engine = StorageEngine::in_memory().unwrap();
        let conn = engine.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO co_change_matrix
             (entity_a, entity_b, commit_count, lift, window_start, window_end)
             VALUES ('a.ts', 'b.ts', 7, 3.25, 'w0', 'w1');",
            [],
        )
        .unwrap();

        let (count, lift): (i64, f64) = conn
            .query_row(
                "SELECT commit_count, lift FROM co_change_matrix
                 WHERE entity_a = 'a.ts' AND entity_b = 'b.ts';",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 7);
        assert!((lift - 3.25).abs() < f64::EPSILON);
    }
}
