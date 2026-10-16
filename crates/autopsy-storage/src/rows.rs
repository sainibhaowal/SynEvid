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
use rusqlite::params;

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
