//! autopsy-envelope: Shared Evidence Envelope for SynEvid
//!
//! This crate defines the canonical `EvidenceEnvelope` that ALL clients
//! (CLI, TUI, MCP, Web, Electron) must use for responses. It includes:
//! - Standardized metadata (operation_id, repository_id, snapshot_id, etc.)
//! - Verdict and coverage state with hard boundaries
//! - BLAKE3 digest for deterministic verification
//! - Evidence receipt with provenance
//! - Hard boundary: UNKNOWN/PARTIAL can never become PASS

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Custom serde module for DateTime<Utc> serialized as seconds since epoch
pub mod ts_seconds {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(dt: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(dt.timestamp())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let ts = i64::deserialize(deserializer)?;
        Ok(DateTime::from_timestamp(ts, 0).unwrap_or_else(Utc::now))
    }
}

/// Custom serde module for Option<DateTime<Utc>> serialized as seconds since epoch
pub mod ts_seconds_option {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(dt: &Option<DateTime<Utc>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match dt {
            Some(dt) => serializer.serialize_i64(dt.timestamp()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<DateTime<Utc>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum TimestampOrNull {
            Timestamp(i64),
            Null,
        }

        match TimestampOrNull::deserialize(deserializer)? {
            TimestampOrNull::Timestamp(ts) => Ok(Some(
                DateTime::from_timestamp(ts, 0).unwrap_or_else(Utc::now),
            )),
            TimestampOrNull::Null => Ok(None),
        }
    }
}

/// Canonical evidence envelope - the single source of truth for all responses
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct EvidenceEnvelope {
    /// Unique operation identifier (BLAKE3 over canonical request)
    pub operation_id: String,

    /// Repository identity
    pub repository_id: Option<String>,

    /// Canonical repository path
    pub canonical_path: Option<String>,

    /// Snapshot identity (if applicable)
    pub snapshot_id: Option<String>,

    /// Session identity (for forensic sessions)
    pub session_id: Option<String>,

    /// Workflow identity (for multi-step operations)
    pub workflow_id: Option<String>,

    /// Event sequence number (for live event replay)
    pub event_seq: Option<i64>,

    /// Verdict - MUST be one of the canonical states
    pub verdict: Verdict,

    /// Coverage state - MUST be one of the canonical states
    pub coverage_state: CoverageState,

    /// Warnings (non-fatal issues)
    #[serde(default)]
    pub warnings: Vec<String>,

    /// Request latency in milliseconds
    pub latency_ms: Option<f64>,

    /// BLAKE3 digest of the canonical response body
    pub blake3_digest: String,

    /// Evidence receipt with provenance
    pub evidence_receipt: EvidenceReceipt,

    /// The actual response payload (command-specific)
    #[serde(flatten)]
    pub payload: serde_json::Value,
}

/// Canonical verdict states - hard boundary: UNKNOWN/PARTIAL can never become PASS
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum Verdict {
    /// No violation found within verified scope
    Pass,
    /// Verified violation exists
    Fail,
    /// Possible issue but not conclusive
    Warn,
    /// Some relevant input/behavior not analyzable
    Partial,
    /// Engine cannot prove the claim
    Unknown,
    /// Operation could not complete
    Error,
    /// Required service/artifact not reachable
    Unavailable,
}

impl Verdict {
    /// Check if this verdict represents a successful outcome
    pub fn is_success(&self) -> bool {
        matches!(self, Verdict::Pass)
    }

    /// Check if this verdict is a non-success state
    pub fn is_non_success(&self) -> bool {
        !self.is_success()
    }
}

/// Canonical coverage states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CoverageState {
    /// All analyzed files carried full adapter coverage
    Verified,
    /// A stated construct (dynamic/unsupported) limited the graph
    Partial,
    /// Engine refuses to pretend
    Unknown,
    /// Coverage could not be determined
    Unavailable,
    /// Coverage determination failed
    Error,
}

impl CoverageState {
    /// Check if coverage allows PASS verdict
    pub fn allows_pass(&self) -> bool {
        matches!(self, CoverageState::Verified)
    }
}

/// Evidence receipt with provenance
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EvidenceReceipt {
    /// Hash algorithm used
    pub algorithm: String,

    /// BLAKE3 digest of the canonical response
    pub digest: String,

    /// Source of this envelope
    pub source: String,

    /// Test reference schema
    pub test_reference: String,

    /// Timestamp of envelope creation (operational metadata only)
    #[serde(
        serialize_with = "ts_seconds::serialize",
        deserialize_with = "ts_seconds::deserialize"
    )]
    #[schemars(with = "String")]
    pub timestamp: DateTime<Utc>,

    /// Engine version that produced this envelope
    pub engine_version: String,

    /// Adapter versions used
    #[serde(default)]
    pub adapter_versions: BTreeMap<String, String>,
}

/// Envelope builder for constructing valid envelopes
pub struct EnvelopeBuilder {
    operation_id: String,
    repository_id: Option<String>,
    canonical_path: Option<String>,
    snapshot_id: Option<String>,
    session_id: Option<String>,
    workflow_id: Option<String>,
    event_seq: Option<i64>,
    verdict: Verdict,
    coverage_state: CoverageState,
    warnings: Vec<String>,
    latency_ms: Option<f64>,
    payload: serde_json::Value,
    engine_version: String,
    adapter_versions: BTreeMap<String, String>,
}

impl EnvelopeBuilder {
    pub fn new(operation_id: String, engine_version: String) -> Self {
        Self {
            operation_id,
            repository_id: None,
            canonical_path: None,
            snapshot_id: None,
            session_id: None,
            workflow_id: None,
            event_seq: None,
            verdict: Verdict::Unknown,
            coverage_state: CoverageState::Unknown,
            warnings: Vec::new(),
            latency_ms: None,
            payload: serde_json::Value::Null,
            engine_version,
            adapter_versions: BTreeMap::new(),
        }
    }

    pub fn repository_id(mut self, id: String) -> Self {
        self.repository_id = Some(id);
        self
    }

    pub fn canonical_path(mut self, path: String) -> Self {
        self.canonical_path = Some(path);
        self
    }

    pub fn snapshot_id(mut self, id: String) -> Self {
        self.snapshot_id = Some(id);
        self
    }

    pub fn session_id(mut self, id: String) -> Self {
        self.session_id = Some(id);
        self
    }

    pub fn workflow_id(mut self, id: String) -> Self {
        self.workflow_id = Some(id);
        self
    }

    pub fn event_seq(mut self, seq: i64) -> Self {
        self.event_seq = Some(seq);
        self
    }

    pub fn verdict(mut self, verdict: Verdict) -> Self {
        self.verdict = verdict;
        self
    }

    pub fn coverage_state(mut self, state: CoverageState) -> Self {
        self.coverage_state = state;
        self
    }

    pub fn warning(mut self, warning: String) -> Self {
        self.warnings.push(warning);
        self
    }

    pub fn latency_ms(mut self, ms: f64) -> Self {
        self.latency_ms = Some(ms);
        self
    }

    pub fn payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = payload;
        self
    }

    pub fn adapter_version(mut self, adapter: String, version: String) -> Self {
        self.adapter_versions.insert(adapter, version);
        self
    }

    /// Build the envelope, enforcing hard boundaries
    pub fn build(self) -> Result<EvidenceEnvelope, EnvelopeError> {
        // HARD BOUNDARY: UNKNOWN/PARTIAL coverage can never become PASS
        if self.verdict == Verdict::Pass && !self.coverage_state.allows_pass() {
            return Err(EnvelopeError::CoverageVerdictMismatch {
                verdict: self.verdict,
                coverage: self.coverage_state,
            });
        }

        // HARD BOUNDARY: FAIL/ERROR verdict must have non-verified coverage
        if matches!(self.verdict, Verdict::Fail | Verdict::Error)
            && self.coverage_state == CoverageState::Verified
        {
            // This is allowed - FAIL with verified coverage means verified violation
        }

        // Compute canonical payload for digest
        let canonical_payload = self.canonical_payload()?;
        let blake3_digest = format!("blake3:{}", blake3::hash(&canonical_payload).to_hex());

        // Generate operation_id if not provided (BLAKE3 over canonical request)
        let operation_id = if self.operation_id.is_empty() {
            format!(
                "blake3:{}",
                &blake3::hash(&canonical_payload).to_hex()[..16]
            )
        } else {
            self.operation_id
        };

        let receipt = EvidenceReceipt {
            algorithm: "BLAKE3".to_string(),
            digest: blake3_digest.clone(),
            source: "daemon response".to_string(),
            test_reference: "schemas/evidence-envelope.schema.json".to_string(),
            timestamp: Utc::now(),
            engine_version: self.engine_version,
            adapter_versions: self.adapter_versions,
        };

        Ok(EvidenceEnvelope {
            operation_id,
            repository_id: self.repository_id,
            canonical_path: self.canonical_path,
            snapshot_id: self.snapshot_id,
            session_id: self.session_id,
            workflow_id: self.workflow_id,
            event_seq: self.event_seq,
            verdict: self.verdict,
            coverage_state: self.coverage_state,
            warnings: self.warnings,
            latency_ms: self.latency_ms,
            blake3_digest,
            evidence_receipt: receipt,
            payload: self.payload,
        })
    }

    fn canonical_payload(&self) -> Result<Vec<u8>, EnvelopeError> {
        // Create canonical JSON for digest (sorted keys, no timestamps)
        let payload = self.payload.clone();

        // Ensure deterministic ordering by using BTreeMap internally
        // serde_json with serde::Serialize on BTreeMap guarantees sorted keys

        Ok(serde_json::to_vec(&payload)?)
    }
}

/// Errors that can occur during envelope construction
#[derive(Debug, Error)]
pub enum EnvelopeError {
    #[error(
        "PASS verdict not allowed with coverage state {coverage:?} (only VERIFIED allows PASS)"
    )]
    CoverageVerdictMismatch {
        verdict: Verdict,
        coverage: CoverageState,
    },

    #[error("JSON serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl Verdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            Verdict::Pass => "PASS",
            Verdict::Fail => "FAIL",
            Verdict::Warn => "WARN",
            Verdict::Partial => "PARTIAL",
            Verdict::Unknown => "UNKNOWN",
            Verdict::Error => "ERROR",
            Verdict::Unavailable => "UNAVAILABLE",
        }
    }
}

/// Helper functions for common envelope patterns
pub mod helpers {
    use super::*;

    /// Create a successful envelope
    pub fn success(
        operation_id: String,
        engine_version: String,
        payload: serde_json::Value,
        repository_id: Option<String>,
        snapshot_id: Option<String>,
    ) -> Result<EvidenceEnvelope, EnvelopeError> {
        EnvelopeBuilder::new(operation_id, engine_version)
            .repository_id(repository_id.unwrap_or_default())
            .snapshot_id(snapshot_id.unwrap_or_default())
            .verdict(Verdict::Pass)
            .coverage_state(CoverageState::Verified)
            .payload(payload)
            .build()
    }

    /// Create a failure envelope
    pub fn failure(
        operation_id: String,
        engine_version: String,
        payload: serde_json::Value,
        repository_id: Option<String>,
        snapshot_id: Option<String>,
        error_message: String,
    ) -> Result<EvidenceEnvelope, EnvelopeError> {
        EnvelopeBuilder::new(operation_id, engine_version)
            .repository_id(repository_id.unwrap_or_default())
            .snapshot_id(snapshot_id.unwrap_or_default())
            .verdict(Verdict::Fail)
            .coverage_state(CoverageState::Verified)
            .warning(error_message)
            .payload(payload)
            .build()
    }

    /// Create an unavailable envelope (daemon down, etc.)
    pub fn unavailable(
        operation_id: String,
        engine_version: String,
        reason: String,
    ) -> Result<EvidenceEnvelope, EnvelopeError> {
        EnvelopeBuilder::new(operation_id, engine_version)
            .verdict(Verdict::Unavailable)
            .coverage_state(CoverageState::Unavailable)
            .warning(reason)
            .payload(serde_json::json!({}))
            .build()
    }

    /// Create an unknown coverage envelope
    pub fn unknown(
        operation_id: String,
        engine_version: String,
        payload: serde_json::Value,
        reason: String,
    ) -> Result<EvidenceEnvelope, EnvelopeError> {
        EnvelopeBuilder::new(operation_id, engine_version)
            .verdict(Verdict::Unknown)
            .coverage_state(CoverageState::Unknown)
            .warning(reason)
            .payload(payload)
            .build()
    }

    /// Create a partial coverage envelope
    pub fn partial(
        operation_id: String,
        engine_version: String,
        payload: serde_json::Value,
        reason: String,
    ) -> Result<EvidenceEnvelope, EnvelopeError> {
        EnvelopeBuilder::new(operation_id, engine_version)
            .verdict(Verdict::Partial)
            .coverage_state(CoverageState::Partial)
            .warning(reason)
            .payload(payload)
            .build()
    }
}

/// Verify an envelope's digest and integrity
pub fn verify_envelope(envelope: &EvidenceEnvelope) -> Result<(), EnvelopeError> {
    // Recompute digest from payload
    let canonical = serde_json::to_vec(&envelope.payload)?;
    let expected_digest = format!("blake3:{}", blake3::hash(&canonical).to_hex());

    if envelope.blake3_digest != expected_digest {
        return Err(EnvelopeError::Serialization(serde_json::Error::io(
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Digest mismatch"),
        )));
    }

    // Verify verdict/coverage consistency
    if envelope.verdict == Verdict::Pass && !envelope.coverage_state.allows_pass() {
        return Err(EnvelopeError::CoverageVerdictMismatch {
            verdict: envelope.verdict,
            coverage: envelope.coverage_state,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pass_with_verified_coverage() {
        let envelope = helpers::success(
            "test-op".to_string(),
            "0.6.0".to_string(),
            serde_json::json!({ "result": "ok" }),
            Some("repo-123".to_string()),
            Some("snap-456".to_string()),
        )
        .unwrap();

        assert_eq!(envelope.verdict, Verdict::Pass);
        assert_eq!(envelope.coverage_state, CoverageState::Verified);
        verify_envelope(&envelope).unwrap();
    }

    #[test]
    fn test_pass_with_partial_coverage_fails() {
        let result = EnvelopeBuilder::new("test-op".to_string(), "0.6.0".to_string())
            .verdict(Verdict::Pass)
            .coverage_state(CoverageState::Partial)
            .payload(serde_json::json!({}))
            .build();

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            EnvelopeError::CoverageVerdictMismatch { .. }
        ));
    }

    #[test]
    fn test_unknown_coverage_with_unknown_verdict() {
        let envelope = helpers::unknown(
            "test-op".to_string(),
            "0.6.0".to_string(),
            serde_json::json!({ "result": "unknown" }),
            "Dynamic import not supported".to_string(),
        )
        .unwrap();

        assert_eq!(envelope.verdict, Verdict::Unknown);
        assert_eq!(envelope.coverage_state, CoverageState::Unknown);
        verify_envelope(&envelope).unwrap();
    }

    #[test]
    fn test_digest_verification() {
        let envelope = helpers::success(
            "test-op".to_string(),
            "0.6.0".to_string(),
            serde_json::json!({ "data": "test" }),
            None,
            None,
        )
        .unwrap();

        // Verify digest matches
        verify_envelope(&envelope).unwrap();

        // Digest should be deterministic
        let envelope2 = helpers::success(
            "test-op".to_string(),
            "0.6.0".to_string(),
            serde_json::json!({ "data": "test" }),
            None,
            None,
        )
        .unwrap();

        assert_eq!(envelope.blake3_digest, envelope2.blake3_digest);
    }
}

/// TypeScript type generation for client consumption
pub mod ts_types;
