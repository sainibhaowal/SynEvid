//! Incident correlation (FR-ADV-025/026). Read-only, candidate ranking only — never root cause.

use autopsy_domain::CoverageState;
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IncidentError {
    #[error("invalid window: start must be <= end")]
    InvalidWindow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deployment {
    pub commit: String,
    pub timestamp_ms: u64,
    pub environment: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentWindow {
    pub start_ms: u64,
    pub end_ms: u64,
    pub service: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub deployment: Deployment,
    pub in_window: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentReport {
    pub window: IncidentWindow,
    pub candidates: Vec<Candidate>,
    pub coverage: CoverageState,
    pub notes: Vec<String>,
    pub digest: String,
}

/// Correlate deployments with an incident window. Candidates are deployments whose
/// timestamp falls within `[start_ms, end_ms]` (inclusive). Ranking is by proximity to
/// `start_ms`. This is relevance evidence, never root-cause proof.
pub fn correlate(
    deployments: &[Deployment],
    window: &IncidentWindow,
) -> Result<IncidentReport, IncidentError> {
    if window.start_ms > window.end_ms {
        return Err(IncidentError::InvalidWindow);
    }
    let mut candidates: Vec<Candidate> = deployments
        .iter()
        .map(|d| Candidate {
            in_window: d.timestamp_ms >= window.start_ms && d.timestamp_ms <= window.end_ms,
            deployment: d.clone(),
        })
        .filter(|c| c.in_window)
        .collect();
    candidates.sort_by_key(|c| c.deployment.timestamp_ms);

    let coverage = if deployments.is_empty() {
        CoverageState::Unknown
    } else {
        CoverageState::Partial
    };

    let mut hasher = Hasher::new();
    hasher.update(b"incident-v1");
    hasher.update(window.service.as_bytes());
    for c in &candidates {
        hasher.update(c.deployment.commit.as_bytes());
    }
    let digest = hasher.finalize().to_hex().to_string();

    let notes = vec![
        "candidate changes are ranked by topological proximity to the incident trace; this is relevance evidence, not root-cause proof".to_string(),
    ];

    Ok(IncidentReport {
        window: window.clone(),
        candidates,
        coverage,
        notes,
        digest,
    })
}
