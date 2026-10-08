//! autopsy-evidence: Cryptographic evidence receipts and canonical verification (FR-013, FR-014).
//!
//! Every invariant finding or impact query produces an immutable, verifiable evidence receipt.
//! Enforces canonical ordering of locations, paths, snapshots, and deltas before computing
//! the BLAKE3 evidence digest.

use autopsy_domain::{Evidence, SnapshotId, SourceLocation};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EvidenceError {
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Invalid evidence receipt: digest mismatch")]
    DigestMismatch,
}

/// Cryptographically verifiable evidence receipt connecting a finding to exact source code facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceReceipt {
    pub finding_id: String,
    pub rule_id: String,
    pub snapshots: Vec<SnapshotId>,
    pub analyzer_version: String,
    pub locations: Vec<SourceLocation>,
    pub paths: Vec<Vec<String>>,
    pub deltas: Vec<String>,
    pub evidence_digest: String,
}

impl EvidenceReceipt {
    /// Verifies the cryptographic integrity of this receipt against its canonical contents.
    pub fn verify_integrity(&self) -> Result<bool, EvidenceError> {
        let expected_digest = compute_canonical_digest(
            &self.finding_id,
            &self.rule_id,
            &self.snapshots,
            &self.analyzer_version,
            &self.locations,
            &self.paths,
            &self.deltas,
        )?;
        Ok(self.evidence_digest == expected_digest)
    }

    /// Converts to domain Evidence struct.
    pub fn to_domain_evidence(&self) -> Evidence {
        Evidence {
            finding_id: self.finding_id.clone(),
            rule_id: self.rule_id.clone(),
            locations: self.locations.clone(),
            paths: self.paths.clone(),
            deltas: self.deltas.clone(),
            evidence_digest: self.evidence_digest.clone(),
        }
    }
}

/// Builder for creating EvidenceReceipt with guaranteed canonical ordering before hashing.
#[derive(Debug, Default)]
pub struct EvidenceReceiptBuilder {
    finding_id: String,
    rule_id: String,
    snapshots: Vec<SnapshotId>,
    analyzer_version: String,
    locations: Vec<SourceLocation>,
    paths: Vec<Vec<String>>,
    deltas: Vec<String>,
}

impl EvidenceReceiptBuilder {
    pub fn new(finding_id: impl Into<String>, rule_id: impl Into<String>) -> Self {
        Self {
            finding_id: finding_id.into(),
            rule_id: rule_id.into(),
            snapshots: Vec::new(),
            analyzer_version: "0.0.1".to_string(),
            locations: Vec::new(),
            paths: Vec::new(),
            deltas: Vec::new(),
        }
    }

    pub fn analyzer_version(mut self, version: impl Into<String>) -> Self {
        self.analyzer_version = version.into();
        self
    }

    pub fn add_snapshot(mut self, snapshot: SnapshotId) -> Self {
        self.snapshots.push(snapshot);
        self
    }

    pub fn add_location(mut self, location: SourceLocation) -> Self {
        self.locations.push(location);
        self
    }

    pub fn add_path(mut self, path: Vec<String>) -> Self {
        self.paths.push(path);
        self
    }

    pub fn add_delta(mut self, delta: impl Into<String>) -> Self {
        self.deltas.push(delta.into());
        self
    }

    /// Builds the evidence receipt by strictly sorting all collections canonically before hashing.
    pub fn build(mut self) -> Result<EvidenceReceipt, EvidenceError> {
        // Canonical ordering of snapshots
        self.snapshots.sort();
        self.snapshots.dedup();

        // Canonical ordering of locations
        self.locations.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then_with(|| a.start_line.cmp(&b.start_line))
                .then_with(|| a.start_col.cmp(&b.start_col))
                .then_with(|| a.end_line.cmp(&b.end_line))
                .then_with(|| a.end_col.cmp(&b.end_col))
                .then_with(|| a.symbol_id.cmp(&b.symbol_id))
        });
        self.locations.dedup();

        // Canonical ordering of paths
        self.paths.sort();
        self.paths.dedup();

        // Canonical ordering of deltas
        self.deltas.sort();
        self.deltas.dedup();

        // Compute BLAKE3 digest over canonical JSON representation
        let evidence_digest = compute_canonical_digest(
            &self.finding_id,
            &self.rule_id,
            &self.snapshots,
            &self.analyzer_version,
            &self.locations,
            &self.paths,
            &self.deltas,
        )?;

        Ok(EvidenceReceipt {
            finding_id: self.finding_id,
            rule_id: self.rule_id,
            snapshots: self.snapshots,
            analyzer_version: self.analyzer_version,
            locations: self.locations,
            paths: self.paths,
            deltas: self.deltas,
            evidence_digest,
        })
    }
}

fn compute_canonical_digest(
    finding_id: &str,
    rule_id: &str,
    snapshots: &[SnapshotId],
    analyzer_version: &str,
    locations: &[SourceLocation],
    paths: &[Vec<String>],
    deltas: &[String],
) -> Result<String, EvidenceError> {
    let canonical_tuple = (
        finding_id,
        rule_id,
        snapshots,
        analyzer_version,
        locations,
        paths,
        deltas,
    );
    let bytes = serde_json::to_vec(&canonical_tuple)?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonical_ordering_and_hash_stability() {
        let loc1 = SourceLocation {
            path: "src/a.ts".to_string(),
            start_line: 10,
            end_line: 12,
            start_col: Some(0),
            end_col: Some(5),
            symbol_id: None,
        };
        let loc2 = SourceLocation {
            path: "src/b.ts".to_string(),
            start_line: 5,
            end_line: 6,
            start_col: Some(2),
            end_col: Some(8),
            symbol_id: None,
        };

        // Add in order (loc1, loc2)
        let receipt1 = EvidenceReceiptBuilder::new("FIND-001", "RULE-FORBIDDEN-DEP")
            .add_location(loc1.clone())
            .add_location(loc2.clone())
            .add_path(vec!["A".to_string(), "B".to_string()])
            .add_path(vec!["X".to_string(), "Y".to_string()])
            .build()
            .unwrap();

        // Add in reversed order (loc2, loc1) and reversed paths
        let receipt2 = EvidenceReceiptBuilder::new("FIND-001", "RULE-FORBIDDEN-DEP")
            .add_location(loc2)
            .add_location(loc1)
            .add_path(vec!["X".to_string(), "Y".to_string()])
            .add_path(vec!["A".to_string(), "B".to_string()])
            .build()
            .unwrap();

        // Must produce identical canonical digest
        assert_eq!(receipt1.evidence_digest, receipt2.evidence_digest);
        assert!(receipt1.verify_integrity().unwrap());
        assert!(receipt2.verify_integrity().unwrap());
    }

    #[test]
    fn test_100_runs_determinism() {
        let loc = SourceLocation {
            path: "src/index.ts".to_string(),
            start_line: 1,
            end_line: 1,
            start_col: None,
            end_col: None,
            symbol_id: None,
        };
        let first = EvidenceReceiptBuilder::new("F1", "R1")
            .add_location(loc.clone())
            .build()
            .unwrap();

        for _ in 0..100 {
            let next = EvidenceReceiptBuilder::new("F1", "R1")
                .add_location(loc.clone())
                .build()
                .unwrap();
            assert_eq!(first.evidence_digest, next.evidence_digest);
        }
    }
}
