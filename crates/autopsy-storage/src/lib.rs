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

        Ok(())
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
                // Atomic write via temporary file
                let tmp_path = dir.join(format!(".tmp-{}", rest));
                fs::write(&tmp_path, content)?;
                fs::rename(tmp_path, target_path)?;
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
}
