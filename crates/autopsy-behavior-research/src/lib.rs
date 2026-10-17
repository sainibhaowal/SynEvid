//! Behavioral research analyzers (FR-ADV-023/024). Research-only, confidence-scored.
//! Never emits a hard PASS without a formal proof.

use autopsy_domain::CoverageState;
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BehaviorError {
    #[error("input length mismatch: base={base}, head={head}")]
    LengthMismatch { base: usize, head: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Divergence {
    pub index: usize,
    pub base: String,
    pub head: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BehaviorReport {
    pub total: usize,
    pub divergences: Vec<Divergence>,
    pub coverage: CoverageState,
    pub notes: Vec<String>,
    pub digest: String,
}

/// Deterministic differential comparison of paired outputs. No code is executed here;
/// callers supply already-captured outputs. Coverage is always `Partial` (research).
pub fn compare_outputs(base: &[String], head: &[String]) -> Result<BehaviorReport, BehaviorError> {
    if base.len() != head.len() {
        return Err(BehaviorError::LengthMismatch {
            base: base.len(),
            head: head.len(),
        });
    }
    let mut divergences = Vec::new();
    for (i, (b, h)) in base.iter().zip(head.iter()).enumerate() {
        if b != h {
            divergences.push(Divergence {
                index: i,
                base: b.clone(),
                head: h.clone(),
            });
        }
    }
    let mut hasher = Hasher::new();
    hasher.update(b"behavior-v1");
    for d in &divergences {
        hasher.update(d.index.to_string().as_bytes());
    }
    let digest = hasher.finalize().to_hex().to_string();
    let notes = vec![
        "behavioral analysis is research-only and confidence-scored; it never emits a hard PASS"
            .to_string(),
    ];
    Ok(BehaviorReport {
        total: base.len(),
        divergences,
        coverage: CoverageState::Partial,
        notes,
        digest,
    })
}
