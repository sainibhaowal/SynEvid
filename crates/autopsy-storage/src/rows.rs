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
        let mut node_stmt = conn.prepare(
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
        let mut in_stmt = conn.prepare(
            "SELECT snapshot_id, edge_id, source_stable_id, target_stable_id, edge_kind,
                    source_file, source_line, source_col, target_file, target_line, confidence
             FROM graph_edges WHERE snapshot_id = ?1 AND target_stable_id = ?2
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
        let mut out_stmt = conn.prepare(
            "SELECT snapshot_id, edge_id, source_stable_id, target_stable_id, edge_kind,
                    source_file, source_line, source_col, target_file, target_line, confidence
             FROM graph_edges WHERE snapshot_id = ?1 AND source_stable_id = ?2
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
                evidence_json, blake3_receipt
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12);",
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
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT event_seq, session_id, job_id, workflow_id, event_type, target_symbol,
                    target_file, before_snap_id, after_snap_id, verdict, summary,
                    evidence_json, blake3_receipt
             FROM trajectory_events WHERE session_id = ?1 ORDER BY event_seq ASC;",
        )?;
        let rows = stmt
            .query_map(params![session_id], |row| {
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
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Retrieves an entry from the AI Working Cache.
    pub fn get_ai_working_cache(
        &self,
        cache_key: &str,
    ) -> Result<Option<AiWorkingCacheEntry>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT cache_key, repo_id, snapshot_id, query_kind, target_id,
                    compact_payload, token_estimate, hit_count
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
                compact_payload, token_estimate, hit_count
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(cache_key) DO UPDATE SET
                compact_payload = excluded.compact_payload,
                token_estimate = excluded.token_estimate,
                hit_count = excluded.hit_count;",
            params![
                entry.cache_key,
                entry.repo_id,
                entry.snapshot_id,
                entry.query_kind,
                entry.target_id,
                entry.compact_payload,
                entry.token_estimate,
                entry.hit_count,
            ],
        )?;
        Ok(())
    }
}
