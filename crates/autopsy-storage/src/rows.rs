//! Row types and persistence for the Phase 4.2 pillar tables (migration 002/003).
//!
//! The tables existed from Step 29 with no production writer — a schema nobody could reach.
//! This module is the write path, held to the same rules as the rest of the engine:
//!
//! * **Idempotent.** Every save is `INSERT OR REPLACE` on the table's natural key, sorted
//!   before insert, so saving the same report twice leaves the same logical content. A
//!   100-run harness that persists on every run produces the same rows every time.
//! * **Honest.** `coverage` travels with the row wherever the engine produces one, and is
//!   validated against the engine's four-state vocabulary at the boundary — a row written
//!   without its coverage state would read back as an unqualified answer.
//! * **DAG-clean.** Rows are plain structs of primitives. `autopsy-storage` depends on no
//!   engine crate; converting an engine's report into rows happens in the caller
//!   (`autopsy-cli`), which already depends on both.
//!
//! `temporal_facts.created_at` (and every `applied_at`) is bookkeeping: written, never
//! read into any canonical structure, so wall-clock time cannot reach a digest (NFR-001).

use crate::{StorageEngine, StorageError};
use rusqlite::{OptionalExtension, params};
use std::collections::{BTreeMap, BTreeSet};

/// The coverage vocabulary, checked at the write boundary.
///
/// A value outside these four is a bug in the caller, not data: persisting it would create
/// a row that reads back as something the engine can say, and it cannot say this.
pub fn validate_coverage(coverage: &str) -> Result<(), StorageError> {
    match coverage {
        "verified" | "partial" | "unsupported" | "unknown" => Ok(()),
        other => Err(StorageError::InvalidRow(format!(
            "coverage `{other}` is not one of verified/partial/unsupported/unknown"
        ))),
    }
}

/// One temporal fact, as `TemporalFact` is carried over the wire — but decoupled from the
/// engine crate, so storage stays a leaf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalFactRow {
    pub fact_id: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub valid_from: String,
    pub valid_to: Option<String>,
    pub evidence_digest: String,
    pub coverage: String,
}

/// One suite-to-symbol mapping from the test inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestMappingRow {
    pub test_suite_id: String,
    pub target_symbol_id: String,
    pub mapping_source: String,
    pub evidence_digest: String,
}

/// One mined co-change edge, pinned to the window that produced it.
///
/// `window_id` is the engine's `commit_set_digest` — an id over the considered commit set,
/// not a timestamp — so two windows coexist and neither overwrites the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoChangeEdgeRow {
    pub entity_a: String,
    pub entity_b: String,
    pub window_id: String,
    pub commits_together: i64,
    pub support_ppm: i64,
    pub confidence_a_to_b_ppm: i64,
    pub lift_ppm: i64,
    pub claim: String,
    pub coverage: String,
}

/// One endpoint or channel of a parsed contract document.
///
/// A document declares many endpoints; the caller splits it into rows and derives a
/// deterministic `contract_id` (BLAKE3 over protocol, service, endpoint, spec digest), so
/// re-saving an unchanged document replaces its own rows and nothing else's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractRow {
    pub contract_id: String,
    pub protocol: String,
    pub service_id: String,
    pub endpoint_or_channel: String,
    pub spec_digest: String,
    pub parsed_json: String,
}

fn require_non_empty(label: &str, value: &str) -> Result<(), StorageError> {
    if value.trim().is_empty() {
        return Err(StorageError::InvalidRow(format!(
            "{label} must not be empty"
        )));
    }
    Ok(())
}

fn validate_i64(label: &str, value: i64) -> Result<(), StorageError> {
    if value < 0 {
        return Err(StorageError::InvalidRow(format!(
            "{label} must not be negative"
        )));
    }
    Ok(())
}

impl StorageEngine {
    /// Persists temporal facts. Idempotent on `fact_id`; rejects duplicate ids in one call
    /// rather than silently letting sort order pick a winner.
    pub fn save_temporal_facts(&self, rows: &[TemporalFactRow]) -> Result<(), StorageError> {
        {
            let conn = self.conn.lock().unwrap();
            let tx = conn.unchecked_transaction()?;
            let mut seen = std::collections::BTreeSet::new();
            for row in rows {
                require_non_empty("fact_id", &row.fact_id)?;
                require_non_empty("subject", &row.subject)?;
                require_non_empty("predicate", &row.predicate)?;
                require_non_empty("object", &row.object)?;
                require_non_empty("valid_from", &row.valid_from)?;
                require_non_empty("evidence_digest", &row.evidence_digest)?;
                validate_coverage(&row.coverage)?;
                if !seen.insert(row.fact_id.as_str()) {
                    return Err(StorageError::InvalidRow(format!(
                        "duplicate fact_id `{}` in one call: the winner would depend on input order",
                        row.fact_id
                    )));
                }
            }
            let mut sorted: Vec<&TemporalFactRow> = rows.iter().collect();
            sorted.sort_by(|a, b| a.fact_id.cmp(&b.fact_id));
            for row in sorted {
                tx.execute(
                    "INSERT OR REPLACE INTO temporal_facts
                     (fact_id, subject, predicate, object, valid_from, valid_to, evidence_digest, created_at, coverage)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'), ?8);",
                    params![
                        row.fact_id,
                        row.subject,
                        row.predicate,
                        row.object,
                        row.valid_from,
                        row.valid_to,
                        row.evidence_digest,
                        row.coverage,
                    ],
                )?;
            }
            tx.commit()?;
        }
        Ok(())
    }

    /// Loads every temporal fact, ordered by `fact_id`.
    pub fn load_temporal_facts(&self) -> Result<Vec<TemporalFactRow>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT fact_id, subject, predicate, object, valid_from, valid_to, evidence_digest, coverage
             FROM temporal_facts ORDER BY fact_id;",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(TemporalFactRow {
                    fact_id: row.get(0)?,
                    subject: row.get(1)?,
                    predicate: row.get(2)?,
                    object: row.get(3)?,
                    valid_from: row.get(4)?,
                    valid_to: row.get(5)?,
                    evidence_digest: row.get(6)?,
                    coverage: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Persists inventory mappings. Idempotent on the natural key
    /// `(test_suite_id, target_symbol_id, mapping_source)`.
    pub fn save_test_mappings(&self, rows: &[TestMappingRow]) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        for row in rows {
            require_non_empty("test_suite_id", &row.test_suite_id)?;
            require_non_empty("target_symbol_id", &row.target_symbol_id)?;
            require_non_empty("mapping_source", &row.mapping_source)?;
            require_non_empty("evidence_digest", &row.evidence_digest)?;
        }
        let mut sorted: Vec<&TestMappingRow> = rows.iter().collect();
        sorted.sort_by(|a, b| {
            (&a.test_suite_id, &a.target_symbol_id, &a.mapping_source).cmp(&(
                &b.test_suite_id,
                &b.target_symbol_id,
                &b.mapping_source,
            ))
        });
        for row in sorted {
            tx.execute(
                "INSERT OR REPLACE INTO test_mappings
                 (test_suite_id, target_symbol_id, mapping_source, evidence_digest)
                 VALUES (?1, ?2, ?3, ?4);",
                params![
                    row.test_suite_id,
                    row.target_symbol_id,
                    row.mapping_source,
                    row.evidence_digest,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Loads every inventory mapping, ordered by the natural key.
    pub fn load_test_mappings(&self) -> Result<Vec<TestMappingRow>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT test_suite_id, target_symbol_id, mapping_source, evidence_digest
             FROM test_mappings
             ORDER BY test_suite_id, target_symbol_id, mapping_source;",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(TestMappingRow {
                    test_suite_id: row.get(0)?,
                    target_symbol_id: row.get(1)?,
                    mapping_source: row.get(2)?,
                    evidence_digest: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Persists mined co-change edges. Idempotent on
    /// `(entity_a, entity_b, window_id)`: a re-mined window replaces its own edges, and a
    /// different window coexists with them.
    pub fn save_co_change_edges(&self, rows: &[CoChangeEdgeRow]) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        let mut seen = std::collections::BTreeSet::new();
        for row in rows {
            require_non_empty("entity_a", &row.entity_a)?;
            require_non_empty("entity_b", &row.entity_b)?;
            require_non_empty("window_id", &row.window_id)?;
            require_non_empty("claim", &row.claim)?;
            validate_i64("commits_together", row.commits_together)?;
            validate_i64("support_ppm", row.support_ppm)?;
            validate_i64("confidence_a_to_b_ppm", row.confidence_a_to_b_ppm)?;
            validate_i64("lift_ppm", row.lift_ppm)?;
            validate_coverage(&row.coverage)?;
            if row.entity_a >= row.entity_b {
                return Err(StorageError::InvalidRow(format!(
                    "entity_a `{}` must sort before entity_b `{}`: the engine guarantees it and the primary key depends on it",
                    row.entity_a, row.entity_b
                )));
            }
            if !seen.insert((
                row.entity_a.as_str(),
                row.entity_b.as_str(),
                row.window_id.as_str(),
            )) {
                return Err(StorageError::InvalidRow(format!(
                    "duplicate edge `{} -> {}` in window `{}` in one call",
                    row.entity_a, row.entity_b, row.window_id
                )));
            }
        }
        let mut sorted: Vec<&CoChangeEdgeRow> = rows.iter().collect();
        sorted.sort_by(|a, b| {
            (&a.entity_a, &a.entity_b, &a.window_id).cmp(&(&b.entity_a, &b.entity_b, &b.window_id))
        });
        for row in sorted {
            tx.execute(
                "INSERT OR REPLACE INTO co_change_matrix
                 (entity_a, entity_b, window_id, commits_together, support_ppm, confidence_a_to_b_ppm, lift_ppm, claim, coverage)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9);",
                params![
                    row.entity_a,
                    row.entity_b,
                    row.window_id,
                    row.commits_together,
                    row.support_ppm,
                    row.confidence_a_to_b_ppm,
                    row.lift_ppm,
                    row.claim,
                    row.coverage,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Loads co-change edges, optionally scoped to one window, ordered by the natural key.
    pub fn load_co_change_edges(
        &self,
        window_id: Option<&str>,
    ) -> Result<Vec<CoChangeEdgeRow>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT entity_a, entity_b, window_id, commits_together, support_ppm, confidence_a_to_b_ppm, lift_ppm, claim, coverage
             FROM co_change_matrix
             WHERE (?1 IS NULL OR window_id = ?1)
             ORDER BY entity_a, entity_b, window_id;",
        )?;
        let rows = stmt
            .query_map(params![window_id], |row| {
                Ok(CoChangeEdgeRow {
                    entity_a: row.get(0)?,
                    entity_b: row.get(1)?,
                    window_id: row.get(2)?,
                    commits_together: row.get(3)?,
                    support_ppm: row.get(4)?,
                    confidence_a_to_b_ppm: row.get(5)?,
                    lift_ppm: row.get(6)?,
                    claim: row.get(7)?,
                    coverage: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Persists parsed contract documents, split by the caller into per-endpoint rows.
    /// Idempotent on `contract_id`.
    pub fn save_contract_rows(&self, rows: &[ContractRow]) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        for row in rows {
            require_non_empty("contract_id", &row.contract_id)?;
            require_non_empty("protocol", &row.protocol)?;
            require_non_empty("service_id", &row.service_id)?;
            require_non_empty("endpoint_or_channel", &row.endpoint_or_channel)?;
            require_non_empty("spec_digest", &row.spec_digest)?;
            // `parsed_json` must parse: a row that cannot be read back is not storage, it
            // is loss with extra steps.
            serde_json::from_str::<serde_json::Value>(&row.parsed_json).map_err(|error| {
                StorageError::InvalidRow(format!(
                    "contract `{}`: parsed_json is not valid JSON: {error}",
                    row.contract_id
                ))
            })?;
        }
        let mut sorted: Vec<&ContractRow> = rows.iter().collect();
        sorted.sort_by(|a, b| a.contract_id.cmp(&b.contract_id));
        for row in sorted {
            tx.execute(
                "INSERT OR REPLACE INTO distributed_contracts
                 (contract_id, protocol, service_id, endpoint_or_channel, spec_digest, parsed_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6);",
                params![
                    row.contract_id,
                    row.protocol,
                    row.service_id,
                    row.endpoint_or_channel,
                    row.spec_digest,
                    row.parsed_json,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Loads contract rows, ordered by `contract_id`.
    pub fn load_contract_rows(&self) -> Result<Vec<ContractRow>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT contract_id, protocol, service_id, endpoint_or_channel, spec_digest, parsed_json
             FROM distributed_contracts ORDER BY contract_id;",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ContractRow {
                    contract_id: row.get(0)?,
                    protocol: row.get(1)?,
                    service_id: row.get(2)?,
                    endpoint_or_channel: row.get(3)?,
                    spec_digest: row.get(4)?,
                    parsed_json: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// The stored rows, as a deterministic text projection.
    ///
    /// Bookkeeping columns (`created_at`, `applied_at`) are excluded on purpose: two
    /// databases written identically at different times must compare equal, and the
    /// wall-clock columns are the only thing that can differ.
    pub fn pillar_rows_projection(&self) -> Result<String, StorageError> {
        let facts = self.load_temporal_facts()?;
        let mappings = self.load_test_mappings()?;
        let edges = self.load_co_change_edges(None)?;
        let contracts = self.load_contract_rows()?;
        Ok(format!(
            "facts={:?}\nmappings={:?}\nedges={:?}\ncontracts={:?}\n",
            facts, mappings, edges, contracts
        ))
    }
}

// =========================================================================
// Phase 1 (v0.2.0) Interactive Visual Cockpit & SQLite Schema Types
// =========================================================================

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoRecord {
    pub repo_id: String,
    pub root_path: String,
    pub display_name: String,
    pub active_branch: Option<String>,
    pub head_commit_oid: Option<String>,
    pub latest_snap_id: Option<String>,
    pub languages_json: String,
    pub monorepo_json: String,
    pub last_indexed_seq: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LiveEventRecord {
    pub seq_id: i64,
    pub event_type: String,
    pub data_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphNodeRow {
    pub snapshot_id: String,
    pub stable_id: String,
    pub node_kind: String,
    pub qualified_name: String,
    pub file_path: String,
    pub start_line: i64,
    pub end_line: i64,
    pub signature_hash: String,
    pub visibility: String,
    pub is_exported: bool,
    pub is_dead_code: bool,
    pub coverage_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphEdgeRow {
    pub snapshot_id: String,
    pub edge_id: String,
    pub source_stable_id: String,
    pub target_stable_id: String,
    pub edge_kind: String,
    pub source_file: String,
    pub source_line: i64,
    pub source_col: i64,
    pub target_file: String,
    pub target_line: i64,
    pub confidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphUniverseRows {
    pub total_node_count: usize,
    pub total_edge_count: usize,
    pub nodes: Vec<GraphNodeRow>,
    pub edges: Vec<GraphEdgeRow>,
    pub inbound_counts: BTreeMap<String, usize>,
    pub outbound_counts: BTreeMap<String, usize>,
    pub coverage_counts: BTreeMap<String, usize>,
    pub is_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EgoNetwork {
    pub target_node: Option<GraphNodeRow>,
    pub incoming_callers: Vec<GraphEdgeRow>,
    pub outgoing_callees: Vec<GraphEdgeRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForensicSessionRecord {
    pub session_id: String,
    pub repo_id: String,
    pub actor_kind: String,
    pub actor_name: String,
    pub baseline_snap_id: String,
    pub current_snap_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForensicWorkflowRecord {
    pub workflow_id: String,
    pub session_id: String,
    pub name: String,
    pub status: String,
    pub start_seq: i64,
    pub end_seq: Option<i64>,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrajectoryEventRecord {
    pub event_seq: i64,
    pub session_id: String,
    pub job_id: Option<String>,
    pub workflow_id: Option<String>,
    pub event_type: String,
    pub target_symbol: Option<String>,
    pub target_file: Option<String>,
    pub before_snap_id: Option<String>,
    pub after_snap_id: Option<String>,
    pub verdict: String,
    pub summary: String,
    pub evidence_json: String,
    pub blake3_receipt: String,
    pub tokens_spent: i64,
    pub tokens_saved: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiWorkingCacheEntry {
    pub cache_key: String,
    pub repo_id: String,
    pub snapshot_id: String,
    pub query_kind: String,
    pub target_id: String,
    pub compact_payload: String,
    pub token_estimate: i64,
    pub hit_count: i64,
    pub last_receipt_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiSessionLedgerRecord {
    pub session_id: String,
    pub total_queries: i64,
    pub cache_hits: i64,
    pub tokens_spent: i64,
    pub tokens_saved: i64,
    pub cost_saved_microcents: i64,
}

impl StorageEngine {
    /// Upserts a repository record.
    pub fn register_repository(&self, repo: &RepoRecord) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO repositories (
                repo_id, root_path, display_name, active_branch, head_commit_oid,
                latest_snap_id, languages_json, monorepo_json, last_indexed_seq
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(repo_id) DO UPDATE SET
                display_name = excluded.display_name,
                active_branch = excluded.active_branch,
                head_commit_oid = excluded.head_commit_oid,
                latest_snap_id = excluded.latest_snap_id,
                languages_json = excluded.languages_json,
                monorepo_json = excluded.monorepo_json,
                last_indexed_seq = excluded.last_indexed_seq;",
            params![
                repo.repo_id,
                repo.root_path,
                repo.display_name,
                repo.active_branch,
                repo.head_commit_oid,
                repo.latest_snap_id,
                repo.languages_json,
                repo.monorepo_json,
                repo.last_indexed_seq,
            ],
        )?;
        Ok(())
    }

    /// Lists all registered repositories.
    pub fn list_repositories(&self) -> Result<Vec<RepoRecord>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT repo_id, root_path, display_name, active_branch, head_commit_oid,
                    latest_snap_id, languages_json, monorepo_json, last_indexed_seq
             FROM repositories ORDER BY display_name;",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(RepoRecord {
                    repo_id: row.get(0)?,
                    root_path: row.get(1)?,
                    display_name: row.get(2)?,
                    active_branch: row.get(3)?,
                    head_commit_oid: row.get(4)?,
                    latest_snap_id: row.get(5)?,
                    languages_json: row.get(6)?,
                    monorepo_json: row.get(7)?,
                    last_indexed_seq: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Retrieves a repository record by repo_id or root_path.
    pub fn get_repository(&self, id_or_path: &str) -> Result<Option<RepoRecord>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT repo_id, root_path, display_name, active_branch, head_commit_oid,
                    latest_snap_id, languages_json, monorepo_json, last_indexed_seq
             FROM repositories WHERE repo_id = ?1 OR root_path = ?1 LIMIT 1;",
        )?;
        let repo = stmt
            .query_row(params![id_or_path], |row| {
                Ok(RepoRecord {
                    repo_id: row.get(0)?,
                    root_path: row.get(1)?,
                    display_name: row.get(2)?,
                    active_branch: row.get(3)?,
                    head_commit_oid: row.get(4)?,
                    latest_snap_id: row.get(5)?,
                    languages_json: row.get(6)?,
                    monorepo_json: row.get(7)?,
                    last_indexed_seq: row.get(8)?,
                })
            })
            .optional()?;
        Ok(repo)
    }

    pub fn remove_repository(&self, repo_id: &str) -> Result<bool, StorageError> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute(
            "DELETE FROM repositories WHERE repo_id = ?1",
            params![repo_id],
        )? > 0)
    }

    pub fn set_daemon_state(&self, key: &str, value: &str) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO daemon_state (state_key, state_value) VALUES (?1, ?2)
             ON CONFLICT(state_key) DO UPDATE SET state_value = excluded.state_value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_daemon_state(&self, key: &str) -> Result<Option<String>, StorageError> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(
                "SELECT state_value FROM daemon_state WHERE state_key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn record_live_event(
        &self,
        event_type: &str,
        data_json: &str,
    ) -> Result<i64, StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO live_events (event_type, data_json) VALUES (?1, ?2)",
            params![event_type, data_json],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn live_events_since(
        &self,
        since_seq: i64,
        limit: i64,
    ) -> Result<Vec<LiveEventRecord>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT seq_id, event_type, data_json FROM live_events
             WHERE seq_id > ?1 ORDER BY seq_id LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![since_seq, limit.max(1)], |row| {
                Ok(LiveEventRecord {
                    seq_id: row.get(0)?,
                    event_type: row.get(1)?,
                    data_json: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn latest_live_event_seq(&self) -> Result<i64, StorageError> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.query_row(
            "SELECT COALESCE(MAX(seq_id), 0) FROM live_events",
            [],
            |row| row.get(0),
        )?)
    }

    pub fn graph_rows_for_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<(Vec<GraphNodeRow>, Vec<GraphEdgeRow>), StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut node_stmt = conn.prepare_cached(
            "SELECT snapshot_id, stable_id, node_kind, qualified_name, file_path,
                    start_line, end_line, signature_hash, visibility, is_exported,
                    is_dead_code, coverage_state
             FROM graph_nodes WHERE snapshot_id = ?1 ORDER BY stable_id",
        )?;
        let nodes = node_stmt
            .query_map(params![snapshot_id], |row| {
                Ok(GraphNodeRow {
                    snapshot_id: row.get(0)?,
                    stable_id: row.get(1)?,
                    node_kind: row.get(2)?,
                    qualified_name: row.get(3)?,
                    file_path: row.get(4)?,
                    start_line: row.get(5)?,
                    end_line: row.get(6)?,
                    signature_hash: row.get(7)?,
                    visibility: row.get(8)?,
                    is_exported: row.get::<_, i64>(9)? != 0,
                    is_dead_code: row.get::<_, i64>(10)? != 0,
                    coverage_state: row.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut edge_stmt = conn.prepare(
            "SELECT snapshot_id, edge_id, source_stable_id, target_stable_id, edge_kind,
                    source_file, source_line, source_col, target_file, target_line, confidence
             FROM graph_edges WHERE snapshot_id = ?1 ORDER BY edge_id",
        )?;
        let edges = edge_stmt
            .query_map(params![snapshot_id], |row| {
                Ok(GraphEdgeRow {
                    snapshot_id: row.get(0)?,
                    edge_id: row.get(1)?,
                    source_stable_id: row.get(2)?,
                    target_stable_id: row.get(3)?,
                    edge_kind: row.get(4)?,
                    source_file: row.get(5)?,
                    source_line: row.get(6)?,
                    source_col: row.get(7)?,
                    target_file: row.get(8)?,
                    target_line: row.get(9)?,
                    confidence: row.get(10)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((nodes, edges))
    }

    pub fn graph_universe_rows_for_snapshot(
        &self,
        snapshot_id: &str,
        node_limit: usize,
        edge_limit: usize,
    ) -> Result<GraphUniverseRows, StorageError> {
        let conn = self.conn.lock().unwrap();
        let total_node_count = conn.query_row(
            "SELECT COUNT(*) FROM graph_nodes WHERE snapshot_id = ?1",
            params![snapshot_id],
            |row| row.get::<_, i64>(0),
        )?;
        let total_edge_count = conn.query_row(
            "SELECT COUNT(*) FROM graph_edges WHERE snapshot_id = ?1",
            params![snapshot_id],
            |row| row.get::<_, i64>(0),
        )?;
        let total_node_count = usize::try_from(total_node_count).unwrap_or(usize::MAX);
        let total_edge_count = usize::try_from(total_edge_count).unwrap_or(usize::MAX);
        let mut coverage_counts = BTreeMap::new();
        {
            let mut statement = conn.prepare_cached(
                "SELECT coverage_state, COUNT(*) FROM graph_nodes
                 WHERE snapshot_id = ?1 GROUP BY coverage_state ORDER BY coverage_state",
            )?;
            let rows = statement.query_map(params![snapshot_id], |row| {
                let coverage: String = row.get(0)?;
                let count = usize::try_from(row.get::<_, i64>(1)?).unwrap_or(usize::MAX);
                Ok((coverage, count))
            })?;
            for row in rows {
                let (coverage, count) = row?;
                coverage_counts.insert(coverage, count);
            }
        }

        let mut selected_ids = BTreeSet::new();
        if node_limit > 0 && total_node_count <= node_limit {
            let mut statement = conn.prepare_cached(
                "SELECT stable_id FROM graph_nodes WHERE snapshot_id = ?1 ORDER BY stable_id",
            )?;
            let rows = statement.query_map(params![snapshot_id], |row| row.get::<_, String>(0))?;
            selected_ids.extend(rows.collect::<Result<Vec<_>, _>>()?);
        } else if node_limit > 0 {
            let hub_limit = node_limit / 2;
            if hub_limit > 0 {
                let mut statement = conn.prepare_cached(
                    "SELECT node.stable_id
                     FROM graph_nodes AS node
                     LEFT JOIN (
                         SELECT source_stable_id, COUNT(*) AS edge_count
                         FROM graph_edges WHERE snapshot_id = ?1 GROUP BY source_stable_id
                     ) AS outgoing ON outgoing.source_stable_id = node.stable_id
                     LEFT JOIN (
                         SELECT target_stable_id, COUNT(*) AS edge_count
                         FROM graph_edges WHERE snapshot_id = ?1 GROUP BY target_stable_id
                     ) AS incoming ON incoming.target_stable_id = node.stable_id
                     WHERE node.snapshot_id = ?1
                     ORDER BY COALESCE(outgoing.edge_count, 0) + COALESCE(incoming.edge_count, 0) DESC,
                              node.stable_id ASC
                     LIMIT ?2",
                )?;
                let rows = statement.query_map(
                    params![snapshot_id, i64::try_from(hub_limit).unwrap_or(i64::MAX)],
                    |row| row.get::<_, String>(0),
                )?;
                selected_ids.extend(rows.collect::<Result<Vec<_>, _>>()?);
            }

            let remaining = node_limit.saturating_sub(selected_ids.len());
            if remaining > 0 {
                let mut statement = conn.prepare_cached(
                    "SELECT stable_id FROM graph_nodes WHERE snapshot_id = ?1 ORDER BY stable_id",
                )?;
                let rows =
                    statement.query_map(params![snapshot_id], |row| row.get::<_, String>(0))?;
                let spread_nodes = rows
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .filter(|stable_id| !selected_ids.contains(stable_id))
                    .collect::<Vec<_>>();
                for index in 0..remaining {
                    let spread_index = index.saturating_mul(spread_nodes.len()) / remaining;
                    if let Some(stable_id) = spread_nodes.get(spread_index) {
                        selected_ids.insert(stable_id.clone());
                    }
                }
            }
        }

        conn.execute_batch(
            "CREATE TEMP TABLE IF NOT EXISTS synevid_graph_universe_selected (
                stable_id TEXT PRIMARY KEY
            ) WITHOUT ROWID;
            DELETE FROM synevid_graph_universe_selected;",
        )?;
        {
            let mut statement = conn.prepare_cached(
                "INSERT INTO synevid_graph_universe_selected (stable_id) VALUES (?1)",
            )?;
            for stable_id in &selected_ids {
                statement.execute(params![stable_id])?;
            }
        }

        let nodes = {
            let mut statement = conn.prepare_cached(
                "SELECT node.snapshot_id, node.stable_id, node.node_kind, node.qualified_name,
                        node.file_path, node.start_line, node.end_line, node.signature_hash,
                        node.visibility, node.is_exported, node.is_dead_code, node.coverage_state
                 FROM graph_nodes AS node
                 JOIN synevid_graph_universe_selected AS selected
                   ON selected.stable_id = node.stable_id
                 WHERE node.snapshot_id = ?1 ORDER BY node.stable_id",
            )?;
            let rows = statement.query_map(params![snapshot_id], |row| {
                Ok(GraphNodeRow {
                    snapshot_id: row.get(0)?,
                    stable_id: row.get(1)?,
                    node_kind: row.get(2)?,
                    qualified_name: row.get(3)?,
                    file_path: row.get(4)?,
                    start_line: row.get(5)?,
                    end_line: row.get(6)?,
                    signature_hash: row.get(7)?,
                    visibility: row.get(8)?,
                    is_exported: row.get::<_, i64>(9)? != 0,
                    is_dead_code: row.get::<_, i64>(10)? != 0,
                    coverage_state: row.get(11)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let edges = {
            let mut statement = conn.prepare_cached(
                "SELECT edge.snapshot_id, edge.edge_id, edge.source_stable_id, edge.target_stable_id,
                        edge.edge_kind, edge.source_file, edge.source_line, edge.source_col,
                        edge.target_file, edge.target_line, edge.confidence
                 FROM graph_edges AS edge
                 JOIN synevid_graph_universe_selected AS source
                   ON source.stable_id = edge.source_stable_id
                 JOIN synevid_graph_universe_selected AS target
                   ON target.stable_id = edge.target_stable_id
                 WHERE edge.snapshot_id = ?1
                 ORDER BY (LOWER(edge.edge_kind) = 'calls') DESC,
                          (LOWER(edge.confidence) = 'verified') DESC,
                          edge.edge_id ASC
                 LIMIT ?2",
            )?;
            let rows = statement.query_map(
                params![snapshot_id, i64::try_from(edge_limit).unwrap_or(i64::MAX)],
                |row| {
                    Ok(GraphEdgeRow {
                        snapshot_id: row.get(0)?,
                        edge_id: row.get(1)?,
                        source_stable_id: row.get(2)?,
                        target_stable_id: row.get(3)?,
                        edge_kind: row.get(4)?,
                        source_file: row.get(5)?,
                        source_line: row.get(6)?,
                        source_col: row.get(7)?,
                        target_file: row.get(8)?,
                        target_line: row.get(9)?,
                        confidence: row.get(10)?,
                    })
                },
            )?;
            let mut edges = rows.collect::<Result<Vec<_>, _>>()?;
            edges.sort_by(|left, right| left.edge_id.cmp(&right.edge_id));
            edges
        };
        let inbound_counts = {
            let mut statement = conn.prepare_cached(
                "SELECT edge.target_stable_id, COUNT(*)
                 FROM graph_edges AS edge
                 JOIN synevid_graph_universe_selected AS selected
                   ON selected.stable_id = edge.target_stable_id
                 WHERE edge.snapshot_id = ?1
                 GROUP BY edge.target_stable_id ORDER BY edge.target_stable_id",
            )?;
            let rows = statement.query_map(params![snapshot_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    usize::try_from(row.get::<_, i64>(1)?).unwrap_or(usize::MAX),
                ))
            })?;
            rows.collect::<Result<BTreeMap<_, _>, _>>()?
        };
        let outbound_counts = {
            let mut statement = conn.prepare_cached(
                "SELECT edge.source_stable_id, COUNT(*)
                 FROM graph_edges AS edge
                 JOIN synevid_graph_universe_selected AS selected
                   ON selected.stable_id = edge.source_stable_id
                 WHERE edge.snapshot_id = ?1
                 GROUP BY edge.source_stable_id ORDER BY edge.source_stable_id",
            )?;
            let rows = statement.query_map(params![snapshot_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    usize::try_from(row.get::<_, i64>(1)?).unwrap_or(usize::MAX),
                ))
            })?;
            rows.collect::<Result<BTreeMap<_, _>, _>>()?
        };

        let is_truncated = nodes.len() < total_node_count || edges.len() < total_edge_count;
        Ok(GraphUniverseRows {
            total_node_count,
            total_edge_count,
            nodes,
            edges,
            inbound_counts,
            outbound_counts,
            coverage_counts,
            is_truncated,
        })
    }

    /// Batch saves graph nodes and directed line-to-line edges for a snapshot.
    pub fn save_graph_nodes_and_edges(
        &self,
        nodes: &[GraphNodeRow],
        edges: &[GraphEdgeRow],
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        {
            let mut stmt = tx.prepare(
                "INSERT INTO graph_nodes (
                    snapshot_id, stable_id, node_kind, qualified_name, file_path,
                    start_line, end_line, signature_hash, visibility, is_exported,
                    is_dead_code, coverage_state
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                ON CONFLICT(snapshot_id, stable_id) DO UPDATE SET
                    qualified_name = excluded.qualified_name,
                    start_line = excluded.start_line,
                    end_line = excluded.end_line,
                    signature_hash = excluded.signature_hash,
                    is_exported = excluded.is_exported,
                    is_dead_code = excluded.is_dead_code,
                    coverage_state = excluded.coverage_state;",
            )?;

            for n in nodes {
                stmt.execute(params![
                    n.snapshot_id,
                    n.stable_id,
                    n.node_kind,
                    n.qualified_name,
                    n.file_path,
                    n.start_line,
                    n.end_line,
                    n.signature_hash,
                    n.visibility,
                    if n.is_exported { 1 } else { 0 },
                    if n.is_dead_code { 1 } else { 0 },
                    n.coverage_state,
                ])?;
            }
        }

        {
            let mut stmt = tx.prepare(
                "INSERT INTO graph_edges (
                    snapshot_id, edge_id, source_stable_id, target_stable_id, edge_kind,
                    source_file, source_line, source_col, target_file, target_line, confidence
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ON CONFLICT(snapshot_id, edge_id) DO UPDATE SET
                    source_line = excluded.source_line,
                    target_line = excluded.target_line,
                    confidence = excluded.confidence;",
            )?;

            for e in edges {
                stmt.execute(params![
                    e.snapshot_id,
                    e.edge_id,
                    e.source_stable_id,
                    e.target_stable_id,
                    e.edge_kind,
                    e.source_file,
                    e.source_line,
                    e.source_col,
                    e.target_file,
                    e.target_line,
                    e.confidence,
                ])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    /// Queries the Ego-Network for a file or symbol in <5ms.
    pub fn query_ego_network(
        &self,
        snapshot_id: &str,
        stable_id: &str,
    ) -> Result<EgoNetwork, StorageError> {
        let conn = self.conn.lock().unwrap();

        // Target node
        let mut node_stmt = conn.prepare_cached(
            "SELECT snapshot_id, stable_id, node_kind, qualified_name, file_path,
                    start_line, end_line, signature_hash, visibility, is_exported,
                    is_dead_code, coverage_state
             FROM graph_nodes WHERE snapshot_id = ?1 AND stable_id = ?2 LIMIT 1;",
        )?;
        let target_node = node_stmt
            .query_row(params![snapshot_id, stable_id], |row| {
                Ok(GraphNodeRow {
                    snapshot_id: row.get(0)?,
                    stable_id: row.get(1)?,
                    node_kind: row.get(2)?,
                    qualified_name: row.get(3)?,
                    file_path: row.get(4)?,
                    start_line: row.get(5)?,
                    end_line: row.get(6)?,
                    signature_hash: row.get(7)?,
                    visibility: row.get(8)?,
                    is_exported: row.get::<_, i64>(9)? != 0,
                    is_dead_code: row.get::<_, i64>(10)? != 0,
                    coverage_state: row.get(11)?,
                })
            })
            .optional()?;

        // Incoming Callers (who calls this target)
        let mut in_stmt = conn.prepare_cached(
            "SELECT snapshot_id, edge_id, source_stable_id, target_stable_id, edge_kind,
                    source_file, source_line, source_col, target_file, target_line, confidence
             FROM graph_edges WHERE snapshot_id = ?1 AND target_stable_id = ?2
               AND edge_kind IN ('calls', 'references', 'imports')
             ORDER BY source_file, source_line;",
        )?;
        let incoming_callers = in_stmt
            .query_map(params![snapshot_id, stable_id], |row| {
                Ok(GraphEdgeRow {
                    snapshot_id: row.get(0)?,
                    edge_id: row.get(1)?,
                    source_stable_id: row.get(2)?,
                    target_stable_id: row.get(3)?,
                    edge_kind: row.get(4)?,
                    source_file: row.get(5)?,
                    source_line: row.get(6)?,
                    source_col: row.get(7)?,
                    target_file: row.get(8)?,
                    target_line: row.get(9)?,
                    confidence: row.get(10)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Outgoing Callees (who this target calls)
        let mut out_stmt = conn.prepare_cached(
            "SELECT snapshot_id, edge_id, source_stable_id, target_stable_id, edge_kind,
                    source_file, source_line, source_col, target_file, target_line, confidence
             FROM graph_edges WHERE snapshot_id = ?1 AND source_stable_id = ?2
               AND edge_kind IN ('calls', 'references', 'imports')
             ORDER BY target_file, target_line;",
        )?;
        let outgoing_callees = out_stmt
            .query_map(params![snapshot_id, stable_id], |row| {
                Ok(GraphEdgeRow {
                    snapshot_id: row.get(0)?,
                    edge_id: row.get(1)?,
                    source_stable_id: row.get(2)?,
                    target_stable_id: row.get(3)?,
                    edge_kind: row.get(4)?,
                    source_file: row.get(5)?,
                    source_line: row.get(6)?,
                    source_col: row.get(7)?,
                    target_file: row.get(8)?,
                    target_line: row.get(9)?,
                    confidence: row.get(10)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(EgoNetwork {
            target_node,
            incoming_callers,
            outgoing_callees,
        })
    }

    /// Creates or registers a forensic session.
    pub fn create_forensic_session(
        &self,
        session: &ForensicSessionRecord,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO forensic_sessions (
                session_id, repo_id, actor_kind, actor_name, baseline_snap_id, current_snap_id, status
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(session_id) DO UPDATE SET
                current_snap_id = excluded.current_snap_id,
                status = excluded.status;",
            params![
                session.session_id,
                session.repo_id,
                session.actor_kind,
                session.actor_name,
                session.baseline_snap_id,
                session.current_snap_id,
                session.status,
            ],
        )?;
        Ok(())
    }

    /// Gets a forensic session by ID.
    pub fn get_forensic_session(
        &self,
        session_id: &str,
    ) -> Result<Option<ForensicSessionRecord>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT session_id, repo_id, actor_kind, actor_name, baseline_snap_id, current_snap_id, status
             FROM forensic_sessions WHERE session_id = ?1 LIMIT 1;",
        )?;
        let record = stmt
            .query_row(params![session_id], |row| {
                Ok(ForensicSessionRecord {
                    session_id: row.get(0)?,
                    repo_id: row.get(1)?,
                    actor_kind: row.get(2)?,
                    actor_name: row.get(3)?,
                    baseline_snap_id: row.get(4)?,
                    current_snap_id: row.get(5)?,
                    status: row.get(6)?,
                })
            })
            .optional()?;
        Ok(record)
    }

    /// Lists forensic sessions for a repository.
    pub fn list_forensic_sessions(
        &self,
        repo_id: &str,
    ) -> Result<Vec<ForensicSessionRecord>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT session_id, repo_id, actor_kind, actor_name, baseline_snap_id, current_snap_id, status
             FROM forensic_sessions WHERE repo_id = ?1 OR ?1 = '' ORDER BY session_id DESC;",
        )?;
        let rows = stmt
            .query_map(params![repo_id], |row| {
                Ok(ForensicSessionRecord {
                    session_id: row.get(0)?,
                    repo_id: row.get(1)?,
                    actor_kind: row.get(2)?,
                    actor_name: row.get(3)?,
                    baseline_snap_id: row.get(4)?,
                    current_snap_id: row.get(5)?,
                    status: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Updates status and current snapshot of a forensic session.
    pub fn update_forensic_session_status(
        &self,
        session_id: &str,
        status: &str,
        current_snap_id: &str,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE forensic_sessions SET status = ?1, current_snap_id = ?2 WHERE session_id = ?3;",
            params![status, current_snap_id, session_id],
        )?;
        Ok(())
    }

    /// Creates a forensic workflow.
    pub fn create_forensic_workflow(
        &self,
        wf: &ForensicWorkflowRecord,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO forensic_workflows (
                workflow_id, session_id, name, status, start_seq, end_seq, duration_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(workflow_id) DO UPDATE SET
                status = excluded.status,
                end_seq = excluded.end_seq,
                duration_ms = excluded.duration_ms;",
            params![
                wf.workflow_id,
                wf.session_id,
                wf.name,
                wf.status,
                wf.start_seq,
                wf.end_seq,
                wf.duration_ms,
            ],
        )?;
        Ok(())
    }

    /// Lists forensic workflows for a session.
    pub fn list_forensic_workflows(
        &self,
        session_id: &str,
    ) -> Result<Vec<ForensicWorkflowRecord>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT workflow_id, session_id, name, status, start_seq, end_seq, duration_ms
             FROM forensic_workflows WHERE session_id = ?1 ORDER BY start_seq ASC;",
        )?;
        let rows = stmt
            .query_map(params![session_id], |row| {
                Ok(ForensicWorkflowRecord {
                    workflow_id: row.get(0)?,
                    session_id: row.get(1)?,
                    name: row.get(2)?,
                    status: row.get(3)?,
                    start_seq: row.get(4)?,
                    end_seq: row.get(5)?,
                    duration_ms: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Records an event in the Forensic Trajectory Timeline.
    pub fn record_trajectory_event(
        &self,
        event: &TrajectoryEventRecord,
    ) -> Result<i64, StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO trajectory_events (
                session_id, job_id, workflow_id, event_type, target_symbol,
                target_file, before_snap_id, after_snap_id, verdict, summary,
                evidence_json, blake3_receipt, tokens_spent, tokens_saved
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14);",
            params![
                event.session_id,
                event.job_id,
                event.workflow_id,
                event.event_type,
                event.target_symbol,
                event.target_file,
                event.before_snap_id,
                event.after_snap_id,
                event.verdict,
                event.summary,
                event.evidence_json,
                event.blake3_receipt,
                event.tokens_spent,
                event.tokens_saved,
            ],
        )?;
        let seq = conn.last_insert_rowid();
        Ok(seq)
    }

    /// Lists trajectory events for a session in chronological sequence.
    pub fn list_trajectory_events(
        &self,
        session_id: &str,
    ) -> Result<Vec<TrajectoryEventRecord>, StorageError> {
        self.list_trajectory_events_range(session_id, 0, i64::MAX)
    }

    /// Lists trajectory events for a session bounded by sequence range [start_seq, end_seq].
    pub fn list_trajectory_events_range(
        &self,
        session_id: &str,
        start_seq: i64,
        end_seq: i64,
    ) -> Result<Vec<TrajectoryEventRecord>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT event_seq, session_id, job_id, workflow_id, event_type, target_symbol,
                    target_file, before_snap_id, after_snap_id, verdict, summary,
                    evidence_json, blake3_receipt,
                    COALESCE(tokens_spent, 0), COALESCE(tokens_saved, 0)
             FROM trajectory_events
             WHERE session_id = ?1 AND event_seq >= ?2 AND event_seq <= ?3
             ORDER BY event_seq ASC;",
        )?;
        let rows = stmt
            .query_map(params![session_id, start_seq, end_seq], |row| {
                Ok(TrajectoryEventRecord {
                    event_seq: row.get(0)?,
                    session_id: row.get(1)?,
                    job_id: row.get(2)?,
                    workflow_id: row.get(3)?,
                    event_type: row.get(4)?,
                    target_symbol: row.get(5)?,
                    target_file: row.get(6)?,
                    before_snap_id: row.get(7)?,
                    after_snap_id: row.get(8)?,
                    verdict: row.get(9)?,
                    summary: row.get(10)?,
                    evidence_json: row.get(11)?,
                    blake3_receipt: row.get(12)?,
                    tokens_spent: row.get(13)?,
                    tokens_saved: row.get(14)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Gets a single trajectory event by sequence ID.
    pub fn get_trajectory_event(
        &self,
        event_seq: i64,
    ) -> Result<Option<TrajectoryEventRecord>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT event_seq, session_id, job_id, workflow_id, event_type, target_symbol,
                    target_file, before_snap_id, after_snap_id, verdict, summary,
                    evidence_json, blake3_receipt,
                    COALESCE(tokens_spent, 0), COALESCE(tokens_saved, 0)
             FROM trajectory_events WHERE event_seq = ?1 LIMIT 1;",
        )?;
        let event = stmt
            .query_row(params![event_seq], |row| {
                Ok(TrajectoryEventRecord {
                    event_seq: row.get(0)?,
                    session_id: row.get(1)?,
                    job_id: row.get(2)?,
                    workflow_id: row.get(3)?,
                    event_type: row.get(4)?,
                    target_symbol: row.get(5)?,
                    target_file: row.get(6)?,
                    before_snap_id: row.get(7)?,
                    after_snap_id: row.get(8)?,
                    verdict: row.get(9)?,
                    summary: row.get(10)?,
                    evidence_json: row.get(11)?,
                    blake3_receipt: row.get(12)?,
                    tokens_spent: row.get(13)?,
                    tokens_saved: row.get(14)?,
                })
            })
            .optional()?;
        Ok(event)
    }

    /// Retrieves an entry from the AI Working Cache.
    pub fn get_ai_working_cache(
        &self,
        cache_key: &str,
    ) -> Result<Option<AiWorkingCacheEntry>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT cache_key, repo_id, snapshot_id, query_kind, target_id,
                    compact_payload, token_estimate, hit_count, last_receipt_token
             FROM ai_working_cache WHERE cache_key = ?1 LIMIT 1;",
        )?;
        let entry = stmt
            .query_row(params![cache_key], |row| {
                Ok(AiWorkingCacheEntry {
                    cache_key: row.get(0)?,
                    repo_id: row.get(1)?,
                    snapshot_id: row.get(2)?,
                    query_kind: row.get(3)?,
                    target_id: row.get(4)?,
                    compact_payload: row.get(5)?,
                    token_estimate: row.get(6)?,
                    hit_count: row.get(7)?,
                    last_receipt_token: row.get(8)?,
                })
            })
            .optional()?;

        if entry.is_some() {
            let _ = conn.execute(
                "UPDATE ai_working_cache SET hit_count = hit_count + 1 WHERE cache_key = ?1;",
                params![cache_key],
            );
        }

        Ok(entry)
    }

    /// Saves an entry into the AI Working Cache.
    pub fn set_ai_working_cache(&self, entry: &AiWorkingCacheEntry) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO ai_working_cache (
                cache_key, repo_id, snapshot_id, query_kind, target_id,
                compact_payload, token_estimate, hit_count, last_receipt_token
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(cache_key) DO UPDATE SET
                compact_payload = excluded.compact_payload,
                token_estimate = excluded.token_estimate,
                hit_count = excluded.hit_count,
                last_receipt_token = excluded.last_receipt_token;",
            params![
                entry.cache_key,
                entry.repo_id,
                entry.snapshot_id,
                entry.query_kind,
                entry.target_id,
                entry.compact_payload,
                entry.token_estimate,
                entry.hit_count,
                entry.last_receipt_token,
            ],
        )?;
        Ok(())
    }

    /// Gets or creates an AI Session Ledger record.
    pub fn get_or_create_ai_ledger(
        &self,
        session_id: &str,
    ) -> Result<AiSessionLedgerRecord, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT session_id, total_queries, cache_hits, tokens_spent, tokens_saved, cost_saved_microcents
             FROM ai_session_ledger WHERE session_id = ?1 LIMIT 1;",
        )?;
        let res = stmt
            .query_row(params![session_id], |row| {
                Ok(AiSessionLedgerRecord {
                    session_id: row.get(0)?,
                    total_queries: row.get(1)?,
                    cache_hits: row.get(2)?,
                    tokens_spent: row.get(3)?,
                    tokens_saved: row.get(4)?,
                    cost_saved_microcents: row.get(5)?,
                })
            })
            .optional()?;

        if let Some(record) = res {
            Ok(record)
        } else {
            let default_record = AiSessionLedgerRecord {
                session_id: session_id.to_string(),
                total_queries: 0,
                cache_hits: 0,
                tokens_spent: 0,
                tokens_saved: 0,
                cost_saved_microcents: 0,
            };
            let _ = conn.execute(
                "INSERT OR IGNORE INTO ai_session_ledger (
                    session_id, total_queries, cache_hits, tokens_spent, tokens_saved, cost_saved_microcents
                ) VALUES (?1, 0, 0, 0, 0, 0);",
                params![session_id],
            );
            Ok(default_record)
        }
    }

    /// Records an AI query event in the session ledger, accumulating token metrics.
    pub fn record_ai_query(
        &self,
        session_id: &str,
        hit: bool,
        tokens_spent: i64,
        tokens_saved: i64,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        let hit_increment = if hit { 1 } else { 0 };
        // Estimate 0.3 cents per 1k input tokens -> 3 microcents per token
        let cost_saved = tokens_saved * 3;
        conn.execute(
            "INSERT INTO ai_session_ledger (
                session_id, total_queries, cache_hits, tokens_spent, tokens_saved, cost_saved_microcents
            ) VALUES (?1, 1, ?2, ?3, ?4, ?5)
            ON CONFLICT(session_id) DO UPDATE SET
                total_queries = total_queries + 1,
                cache_hits = cache_hits + ?2,
                tokens_spent = tokens_spent + ?3,
                tokens_saved = tokens_saved + ?4,
                cost_saved_microcents = cost_saved_microcents + ?5;",
            params![session_id, hit_increment, tokens_spent, tokens_saved, cost_saved],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod graph_universe_tests {
    use super::*;
    use autopsy_domain::{RepoSnapshot, SnapshotId};

    #[test]
    fn graph_universe_projection_is_bounded_deterministic_and_preserves_full_counts() {
        let storage = StorageEngine::in_memory().unwrap();
        let snapshot_id = "snapshot-graph-universe";
        storage
            .save_snapshot(&RepoSnapshot {
                snapshot_id: SnapshotId::new(snapshot_id),
                revision: "revision".to_string(),
                config_hash: "config".to_string(),
                file_set_digest: "files".to_string(),
                files: BTreeMap::new(),
                skipped_oversize_files: Vec::new(),
                traversal_limit_reached: false,
            })
            .unwrap();

        let nodes = (0..15_000)
            .map(|index| {
                let stable_id = format!("node-{index:05}");
                GraphNodeRow {
                    snapshot_id: snapshot_id.to_string(),
                    stable_id: stable_id.clone(),
                    node_kind: "function".to_string(),
                    qualified_name: stable_id.clone(),
                    file_path: format!("src/{stable_id}.ts"),
                    start_line: 1,
                    end_line: 4,
                    signature_hash: String::new(),
                    visibility: "public".to_string(),
                    is_exported: true,
                    is_dead_code: false,
                    coverage_state: if index == 14_999 {
                        "partial".to_string()
                    } else {
                        "verified".to_string()
                    },
                }
            })
            .collect::<Vec<_>>();
        let edges = (0..30_000)
            .map(|index| {
                let source = format!("node-{:05}", index % 2_500);
                let target = format!("node-{:05}", (index * 31 + 1) % 2_500);
                GraphEdgeRow {
                    snapshot_id: snapshot_id.to_string(),
                    edge_id: format!("edge-{index:05}"),
                    source_stable_id: source.clone(),
                    target_stable_id: target.clone(),
                    edge_kind: "calls".to_string(),
                    source_file: format!("src/{source}.ts"),
                    source_line: 2,
                    source_col: 3,
                    target_file: format!("src/{target}.ts"),
                    target_line: 1,
                    confidence: "verified".to_string(),
                }
            })
            .collect::<Vec<_>>();
        storage.save_graph_nodes_and_edges(&nodes, &edges).unwrap();

        let projection_started = std::time::Instant::now();
        let first = storage
            .graph_universe_rows_for_snapshot(snapshot_id, 5_000, 12_000)
            .unwrap();
        eprintln!(
            "SQLite graph-universe projection 15,000 nodes/30,000 edges: {:.3} ms",
            projection_started.elapsed().as_secs_f64() * 1_000.0
        );
        let second = storage
            .graph_universe_rows_for_snapshot(snapshot_id, 5_000, 12_000)
            .unwrap();

        assert_eq!(first.total_node_count, 15_000);
        assert_eq!(first.total_edge_count, 30_000);
        assert_eq!(first.nodes.len(), 5_000);
        assert_eq!(first.edges.len(), 12_000);
        assert!(first.is_truncated);
        assert_eq!(first.nodes, second.nodes);
        assert_eq!(first.edges, second.edges);
        assert_eq!(first.inbound_counts.get("node-00000"), Some(&12));
        assert_eq!(first.outbound_counts.get("node-00000"), Some(&12));
        assert_eq!(first.coverage_counts.get("verified"), Some(&14_999));
        assert_eq!(first.coverage_counts.get("partial"), Some(&1));
        assert!(
            first
                .edges
                .iter()
                .all(|edge| edge.source_line == 2 && edge.target_line == 1)
        );

        let (persisted_nodes, persisted_edges) =
            storage.graph_rows_for_snapshot(snapshot_id).unwrap();
        assert_eq!(persisted_nodes, nodes);
        assert_eq!(persisted_edges, edges);
    }
}
