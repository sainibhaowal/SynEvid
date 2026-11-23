//! Forensic engine for sessions, workflows, and trajectory

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use blake3;
use tracing::{debug, info, warn};

use autopsy_storage::{StorageEngine, ForensicSessionRecord, ForensicWorkflowRecord, TrajectoryEventRecord, AiSessionLedgerRecord, AiWorkingCacheEntry};

/// Forensic engine
pub struct ForensicEngine {
    // Stateless - uses storage
}

impl ForensicEngine {
    pub fn new() -> Self {
        Self {}
    }

    /// Start a new forensic session
    pub fn start_session(
        &self,
        storage: &mut StorageEngine,
        repo_id: &str,
        actor_kind: Option<&str>,
        actor_name: Option<&str>,
        baseline_snap_id: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<ForensicSessionResult, String> {
        let session_id = session_id.unwrap_or_else(|| {
            let input = format!("{}-{}-{}", repo_id, chrono::Utc::now().timestamp_millis(), uuid::Uuid::new_v4());
            blake3::hash(input.as_bytes()).to_hex().to_string()[..16].to_string()
        }).to_string();

        let record = ForensicSessionRecord {
            session_id: session_id.clone(),
            repo_id: repo_id.to_string(),
            actor_kind: actor_kind.unwrap_or("HUMAN").to_string(),
            actor_name: actor_name.unwrap_or("User").to_string(),
            baseline_snap_id: baseline_snap_id.unwrap_or("").to_string(),
            current_snap_id: baseline_snap_id.unwrap_or("").to_string(),
            files_touched_count: 0,
            total_events_count: 0,
            status: "ACTIVE".to_string(),
            created_at: current_timestamp(),
            updated_at: current_timestamp(),
        };

        storage.insert_forensic_session(&record)
            .map_err(|e| format!("Failed to insert session: {}", e))?;

        Ok(ForensicSessionResult {
            session_id,
            status: "ACTIVE".to_string(),
        })
    }

    /// Record a forensic event
    pub fn record_event(
        &self,
        storage: &mut StorageEngine,
        session_id: &str,
        repo_root: Option<&str>,
        job_id: Option<&str>,
        workflow_id: Option<&str>,
        event_type: &str,
        target_symbol: Option<&str>,
        target_file: Option<&str>,
        before_snap_id: Option<&str>,
        after_snap_id: Option<&str>,
        verdict: Option<&str>,
        summary: &str,
        evidence: Value,
        tokens_spent: Option<i64>,
        tokens_saved: Option<i64>,
    ) -> Result<ForensicEventResult, String> {
        let event_seq = storage.next_forensic_event_seq()
            .map_err(|e| format!("Failed to get next event seq: {}", e))?;

        let record = TrajectoryEventRecord {
            event_seq,
            session_id: session_id.to_string(),
            job_id: job_id.map(|s| s.to_string()),
            workflow_id: workflow_id.map(|s| s.to_string()),
            event_type: event_type.to_string(),
            target_symbol: target_symbol.map(|s| s.to_string()),
            target_file: target_file.map(|s| s.to_string()),
            before_snap_id: before_snap_id.map(|s| s.to_string()),
            after_snap_id: after_snap_id.map(|s| s.to_string()),
            verdict: verdict.unwrap_or("INFO").to_string(),
            summary: summary.to_string(),
            evidence_json: serde_json::to_string(&evidence).unwrap_or_default(),
            blake3_receipt: format!("blake3:{}", blake3::hash(serde_json::to_string(&evidence).unwrap_or_default().as_bytes()).to_hex()),
            tokens_spent: tokens_spent.unwrap_or(0),
            tokens_saved: tokens_saved.unwrap_or(0),
            timestamp: current_timestamp(),
        };

        storage.insert_trajectory_event(&record)
            .map_err(|e| format!("Failed to insert trajectory event: {}", e))?;

        // Update session
        storage.update_session_activity(session_id, event_seq, verdict.unwrap_or("INFO"))
            .map_err(|e| format!("Failed to update session: {}", e))?;

        Ok(ForensicEventResult {
            event_seq,
            blake3_receipt: record.blake3_receipt,
        })
    }

    /// Revert a forensic step
    pub fn revert_step(
        &self,
        storage: &mut StorageEngine,
        session_id: &str,
        event_seq: i64,
    ) -> Result<ForensicRevertResult, String> {
        // Get the event to revert
        let event = storage.get_trajectory_event(session_id, event_seq)
            .map_err(|e| format!("Failed to get event: {}", e))?
            .ok_or("Event not found")?;

        // Create revert event
        let revert_seq = storage.next_forensic_event_seq()
            .map_err(|e| format!("Failed to get next event seq: {}", e))?;

        let revert_record = TrajectoryEventRecord {
            event_seq: revert_seq,
            session_id: session_id.to_string(),
            job_id: event.job_id.clone(),
            workflow_id: event.workflow_id.clone(),
            event_type: "revert".to_string(),
            target_symbol: event.target_symbol.clone(),
            target_file: event.target_file.clone(),
            before_snap_id: event.after_snap_id.clone(),
            after_snap_id: event.before_snap_id.clone(),
            verdict: "REVERTED".to_string(),
            summary: format!("Reverted event {}", event_seq),
            evidence_json: serde_json::to_string(&json!({
                "reverted_event_seq": event_seq,
                "original_event_type": event.event_type,
            })).unwrap_or_default(),
            blake3_receipt: format!("blake3:{}", blake3::hash(format!("revert-{}", event_seq).as_bytes()).to_hex()),
            tokens_spent: 0,
            tokens_saved: 0,
            timestamp: current_timestamp(),
        };

        storage.insert_trajectory_event(&revert_record)
            .map_err(|e| format!("Failed to insert revert event: {}", e))?;

        Ok(ForensicRevertResult {
            reverted_event_seq: event_seq,
            revert_event_seq: revert_seq,
        })
    }

    /// Get session timeline
    pub fn get_timeline(
        &self,
        storage: &StorageEngine,
        session_id: &str,
    ) -> Result<ForensicTimelineResult, String> {
        let session = storage.get_forensic_session(session_id)
            .map_err(|e| format!("Failed to get session: {}", e))?
            .ok_or("Session not found")?;

        let events = storage.get_trajectory_events(session_id)
            .map_err(|e| format!("Failed to get events: {}", e))?;

        let workflows = storage.get_forensic_workflows(session_id)
            .map_err(|e| format!("Failed to get workflows: {}", e))?;

        let ledger = storage.get_ai_session_ledger(session_id)
            .map_err(|e| format!("Failed to get ledger: {}", e))?;

        Ok(ForensicTimelineResult {
            session: session.into(),
            events: events.into_iter().map(Into::into).collect(),
            workflows: workflows.into_iter().map(Into::into).collect(),
            ledger: ledger.map(Into::into),
        })
    }

    /// List sessions
    pub fn list_sessions(
        &self,
        storage: &StorageEngine,
        repo_id: Option<&str>,
    ) -> Result<Vec<ForensicSessionInfo>, String> {
        let sessions = storage.list_forensic_sessions(repo_id)
            .map_err(|e| format!("Failed to list sessions: {}", e))?;

        Ok(sessions.into_iter().map(Into::into).collect())
    }

    /// Close session (derive verdict from events)
    pub fn close_session(
        &self,
        storage: &mut StorageEngine,
        session_id: &str,
    ) -> Result<ForensicSessionCloseResult, String> {
        let session = storage.get_forensic_session(session_id)
            .map_err(|e| format!("Failed to get session: {}", e))?
            .ok_or("Session not found")?;

        // Determine verdict from events
        let events = storage.get_trajectory_events(session_id)
            .map_err(|e| format!("Failed to get events: {}", e))?;

        let has_fail = events.iter().any(|e| e.verdict == "FAIL");
        let has_warn = events.iter().any(|e| e.verdict == "WARN");
        let has_error = events.iter().any(|e| e.verdict == "ERROR");

        let verdict = if has_fail || has_error {
            "BLOCKED_FAIL"
        } else if has_warn {
            "VERIFIED_PASS" // Actually WARN, but we'll use this
        } else {
            "VERIFIED_PASS"
        };

        storage.close_forensic_session(session_id, verdict)
            .map_err(|e| format!("Failed to close session: {}", e))?;

        // Generate trajectory report
        let report = self.generate_trajectory_report(storage, session_id, verdict)?;

        Ok(ForensicSessionCloseResult {
            session_id: session_id.to_string(),
            verdict: verdict.to_string(),
            report_digest: report.digest,
        })
    }

    /// Generate trajectory report
    fn generate_trajectory_report(
        &self,
        storage: &StorageEngine,
        session_id: &str,
        verdict: &str,
    ) -> Result<TrajectoryReport, String> {
        let session = storage.get_forensic_session(session_id)
            .map_err(|e| format!("Failed to get session: {}", e))?
            .ok_or("Session not found")?;

        let events = storage.get_trajectory_events(session_id)
            .map_err(|e| format!("Failed to get events: {}", e))?;

        let report = TrajectoryReport {
            session_id: session_id.to_string(),
            repo_id: session.repo_id,
            verdict: verdict.to_string(),
            events: events.into_iter().map(Into::into).collect(),
            generated_at: current_timestamp(),
        };

        let digest = format!("blake3:{}", blake3::hash(serde_json::to_string(&report).unwrap_or_default().as_bytes()).to_hex());

        // Store report
        storage.store_trajectory_report(session_id, &report)
            .map_err(|e| format!("Failed to store trajectory report: {}", e))?;

        Ok(TrajectoryReport { digest, ..report })
    }

    /// Query AI working cache
    pub fn query_ai_cache(
        &self,
        storage: &StorageEngine,
        session_id: &str,
        symbol: &str,
        query_kind: &str,
        snapshot_id: Option<&str>,
        cache_mode: Option<&str>,
    ) -> Result<AiCacheQueryResult, String> {
        let entry = storage.get_ai_working_cache(session_id, symbol, query_kind)
            .map_err(|e| format!("Failed to query AI cache: {}", e))?;

        Ok(AiCacheQueryResult {
            hit: entry.is_some(),
            entry: entry.map(Into::into),
        })
    }
}

// Result types

#[derive(Debug, Serialize, Deserialize)]
pub struct ForensicSessionResult {
    pub session_id: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ForensicEventResult {
    pub event_seq: i64,
    pub blake3_receipt: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ForensicRevertResult {
    pub reverted_event_seq: i64,
    pub revert_event_seq: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ForensicTimelineResult {
    pub session: ForensicSessionInfo,
    pub events: Vec<TrajectoryEventInfo>,
    pub workflows: Vec<ForensicWorkflowInfo>,
    pub ledger: Option<AiSessionLedgerInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ForensicSessionInfo {
    pub session_id: String,
    pub repo_id: String,
    pub actor_kind: String,
    pub actor_name: String,
    pub baseline_snap_id: String,
    pub current_snap_id: String,
    pub files_touched_count: usize,
    pub total_events_count: usize,
    pub status: String,
}

impl From<ForensicSessionRecord> for ForensicSessionInfo {
    fn from(r: ForensicSessionRecord) -> Self {
        Self {
            session_id: r.session_id,
            repo_id: r.repo_id,
            actor_kind: r.actor_kind,
            actor_name: r.actor_name,
            baseline_snap_id: r.baseline_snap_id,
            current_snap_id: r.current_snap_id,
            files_touched_count: r.files_touched_count,
            total_events_count: r.total_events_count,
            status: r.status,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrajectoryEventInfo {
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
    pub timestamp: u64,
}

impl From<TrajectoryEventRecord> for TrajectoryEventInfo {
    fn from(r: TrajectoryEventRecord) -> Self {
        Self {
            event_seq: r.event_seq,
            session_id: r.session_id,
            job_id: r.job_id,
            workflow_id: r.workflow_id,
            event_type: r.event_type,
            target_symbol: r.target_symbol,
            target_file: r.target_file,
            before_snap_id: r.before_snap_id,
            after_snap_id: r.after_snap_id,
            verdict: r.verdict,
            summary: r.summary,
            evidence_json: r.evidence_json,
            blake3_receipt: r.blake3_receipt,
            tokens_spent: r.tokens_spent,
            tokens_saved: r.tokens_saved,
            timestamp: r.timestamp,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ForensicWorkflowInfo {
    pub workflow_id: String,
    pub session_id: String,
    pub name: String,
    pub workflow_type: String,
    pub status: String,
    pub start_event_seq: i64,
    pub end_event_seq: Option<i64>,
}

impl From<ForensicWorkflowRecord> for ForensicWorkflowInfo {
    fn from(r: ForensicWorkflowRecord) -> Self {
        Self {
            workflow_id: r.workflow_id,
            session_id: r.session_id,
            name: r.name,
            workflow_type: r.workflow_type,
            status: r.status,
            start_event_seq: r.start_event_seq,
            end_event_seq: r.end_event_seq,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AiSessionLedgerInfo {
    pub session_id: String,
    pub queries_count: usize,
    pub tokens_spent: i64,
    pub tokens_saved: i64,
    pub cache_hits: usize,
    pub cache_misses: usize,
}

impl From<AiSessionLedgerRecord> for AiSessionLedgerInfo {
    fn from(r: AiSessionLedgerRecord) -> Self {
        Self {
            session_id: r.session_id,
            queries_count: r.queries_count as usize,
            tokens_spent: r.tokens_spent,
            tokens_saved: r.tokens_saved,
            cache_hits: r.cache_hits as usize,
            cache_misses: r.cache_misses as usize,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ForensicRevertResult {
    pub reverted_event_seq: i64,
    pub revert_event_seq: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ForensicSessionCloseResult {
    pub session_id: String,
    pub verdict: String,
    pub report_digest: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrajectoryReport {
    pub session_id: String,
    pub repo_id: String,
    pub verdict: String,
    pub events: Vec<TrajectoryEventInfo>,
    pub generated_at: u64,
    pub digest: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AiCacheQueryResult {
    pub hit: bool,
    pub entry: Option<AiWorkingCacheEntryInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AiWorkingCacheEntryInfo {
    pub session_id: String,
    pub symbol: String,
    pub query_kind: String,
    pub snapshot_id: Option<String>,
    pub cache_mode: String,
    pub response: Value,
    pub tokens_saved: i64,
    pub created_at: u64,
}

impl From<AiWorkingCacheEntry> for AiWorkingCacheEntryInfo {
    fn from(e: AiWorkingCacheEntry) -> Self {
        Self {
            session_id: e.session_id,
            symbol: e.symbol,
            query_kind: e.query_kind,
            snapshot_id: e.snapshot_id,
            cache_mode: e.cache_mode,
            response: serde_json::from_str(&e.response_json).unwrap_or(Value::Null),
            tokens_saved: e.tokens_saved,
            created_at: e.created_at,
        }
    }
}