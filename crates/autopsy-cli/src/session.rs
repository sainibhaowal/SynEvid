//! Session receipts & Task-Completion Forensic Trajectory Ledger (Tier 2.5 + 3.7):
//! a deterministic, agent- and developer-facing record of the repair loop —
//! impact → edit → verify → repair → re-verify.
//!
//! Design constraints (see AGENTS.md invariants):
//! - No wall-clock timestamps: identical inputs produce byte-identical store
//!   contents and forensic run artifacts. Ordering comes from a stable,
//!   monotonically assigned step index plus the BLAKE3 digest chain of the record.
//! - The close verdict derives ONLY from recorded steps: the last verify-like
//!   step's exit code, `unknown` when none was recorded. Honest — never
//!   `pass` without evidence.
//! - Store JSON uses `BTreeMap` maps for canonical key order.
//! - Every `session close` (or `session report`) writes a self-contained
//!   Forensic Trajectory Report to `.autopsy/runs/<session_id>.{json,md}` and
//!   `.autopsy/runs/latest.{json,md}` so both human developers and subsequent
//!   AI models can read what happened on the prior run without repeating mistakes.
//!
//! Store location: `<repo_root>/.autopsy/sessions.json`. Writes replace the
//! file atomically (write + rename), mirroring the engine's storage write.

use crate::{Cli, CliError, EXIT_ANALYSIS_ERROR, EXIT_PASS, OutputFormat, get_storage};
use blake3::Hasher;
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf};

pub const SESSION_STORE_FORMAT: &str = "0.0.1";

/// One recorded evidence step: the command the agent ran and its engine answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStep {
    /// Zero-based, assigned in call order; stable across re-reads.
    pub index: usize,
    /// The full subcommand argv as invoked (args only; the engine name is the prefix).
    pub invoked: Vec<String>,
    /// The engine's exit code for that step.
    pub exit_code: i32,
    /// Optional pinned snapshot id belonging to the step's evidence.
    pub snapshot_id: Option<String>,
    /// Optional agent note (why the repair happened, what was changed).
    pub note: Option<String>,
}

/// The close-time receipt derived from the recorded steps alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionReceipt {
    /// `pass` when the last recorded verify-like step exited 0; `fail` when it
    /// exited nonzero; `unknown` when no such step was ever recorded.
    pub verdict: String,
    /// The exit code the verdict is based on (absent for the unknown case).
    pub evidence_exit_code: Option<i32>,
    /// Which kind of step the verdict is based on (baseline/verify/diff), or
    /// `none` when no evidence step was recorded at all.
    pub evidence_step: String,
    /// Total steps recorded over the session.
    pub steps: usize,
    /// The session id, restated for self-contained receipts.
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub session_id: String,
    pub status: String,
    /// The snapshot id captured when the session opened, when a stored
    /// snapshot was available at that time (deterministic at record time).
    pub started_snapshot: Option<String>,
    pub steps: Vec<SessionStep>,
    pub receipt: Option<SessionReceipt>,
}

/// Deterministic Forensic Trajectory Report persisted under `.autopsy/runs/`
/// on task completion so developers and next-turn AI agents have exact memory
/// of prior trajectory, snapshots, repair notes, and remaining obligations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskForensicReport {
    pub format_version: String,
    pub session_id: String,
    pub status: String,
    pub verdict: String,
    pub evidence_exit_code: Option<i32>,
    pub evidence_step: String,
    pub started_snapshot: Option<String>,
    pub completed_snapshot: Option<String>,
    pub total_steps: usize,
    pub failed_steps: usize,
    pub steps: Vec<SessionStep>,
    pub next_agent_guidance: String,
    pub report_digest: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionStore {
    pub format_version: String,
    pub sessions: BTreeMap<String, SessionRecord>,
}

impl SessionStore {
    pub fn new() -> Self {
        Self {
            format_version: SESSION_STORE_FORMAT.to_string(),
            sessions: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum SessionAction {
    /// Open a session. The id is deterministic: BLAKE3 over the normalized
    /// repo root and the store's own session counter (no wall-clock time).
    Start,
    /// Record one evidence step (the command an agent ran and its exit code).
    Record {
        /// The session id returned by `session start`.
        #[arg(long)]
        session_id: String,
        /// The subcommand argv as invoked, passed after `--`, e.g.
        /// `-- verify --strict`.
        #[arg(
            trailing_var_arg = true,
            allow_hyphen_values = true,
            value_name = "INVOKED"
        )]
        cli_arg: Vec<String>,
        /// The engine exit code the command returned.
        #[arg(long)]
        exit_code: i32,
        /// Optional snapshot id the step's evidence is pinned to.
        #[arg(long)]
        snapshot_id: Option<String>,
        /// What changed and why (repair notes). Free text, stored verbatim.
        #[arg(long)]
        note: Option<String>,
    },
    /// Print the full session record with its deterministic digest.
    Show {
        /// The session id.
        #[arg(long)]
        session_id: String,
    },
    /// Close the session, persist `.autopsy/runs/{<session_id>,latest}.{json,md}`,
    /// and emit the honest repair-loop receipt.
    Close {
        /// The session id.
        #[arg(long)]
        session_id: String,
    },
    /// Generate (or read) the Forensic Trajectory Report for a session (defaults
    /// to the latest session) and write `.autopsy/runs/{<session_id>,latest}.{json,md}`.
    Report {
        /// Optional session id (defaults to the most recent session in the store).
        #[arg(long)]
        session_id: Option<String>,
    },
    /// List every recorded session id with status.
    #[command(name = "list")]
    List,
}

fn store_path(cli: &Cli) -> PathBuf {
    cli.repo_root.join(".autopsy").join("sessions.json")
}

fn runs_dir(cli: &Cli) -> PathBuf {
    cli.repo_root.join(".autopsy").join("runs")
}

fn load_store(cli: &Cli) -> Result<SessionStore, CliError> {
    let path = store_path(cli);
    if !path.exists() {
        return Ok(SessionStore::new());
    }
    let bytes = fs::read(&path).map_err(CliError::Io)?;
    let store: SessionStore = serde_json::from_slice(&bytes).map_err(|e| {
        CliError::Storage(format!(
            "sessions store unreadable at {}: {e}",
            path.display()
        ))
    })?;
    Ok(store)
}

fn save_store(cli: &Cli, store: &SessionStore) -> Result<(), CliError> {
    let path = store_path(cli);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(CliError::Io)?;
    }
    let canonical =
        serde_json::to_string_pretty(store).map_err(|e| CliError::Storage(e.to_string()))?;
    // Atomic replace: temp file next to the store, then rename.
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, canonical.as_bytes()).map_err(CliError::Io)?;
    fs::rename(&temp, &path).map_err(CliError::Io)?;
    Ok(())
}

fn normalized_root(cli: &Cli) -> String {
    // INV-CROSS-001: strict forward-slash normalization, no OS backslashes.
    cli.repo_root
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}

/// The session id: deterministic digest of the workspace and the store's own
/// counter. Unique within one repository, reproducible across runs.
fn session_id_for(cli: &Cli, counter: u64) -> String {
    let mut hasher = Hasher::new();
    hasher.update(b"autopsy-session-v1");
    hasher.update(normalized_root(cli).as_bytes());
    hasher.update(counter.to_le_bytes().as_slice());
    hasher.finalize().to_hex()[..32].to_string()
}

fn next_counter(store: &SessionStore) -> u64 {
    store.sessions.len() as u64
}

/// The step kinds that carry an engine verdict for the receipt.
fn is_evidence_step(invoked: &[String]) -> bool {
    invoked
        .first()
        .map(|head| matches!(head.as_str(), "baseline" | "verify" | "diff" | "impact"))
        .unwrap_or(false)
}

fn close_receipt(session: &SessionRecord, session_id: &str) -> SessionReceipt {
    let last_evidence = session
        .steps
        .iter()
        .rfind(|s| is_evidence_step(&s.invoked))
        .cloned();
    match last_evidence {
        Some(step) => {
            let verdict = match step.exit_code {
                0 => "pass",
                code if code > 0 => "fail",
                _ => "unknown",
            };
            SessionReceipt {
                verdict: verdict.to_string(),
                evidence_exit_code: Some(step.exit_code),
                evidence_step: step.invoked.first().cloned().unwrap_or_default(),
                steps: session.steps.len(),
                session_id: session_id.to_string(),
            }
        }
        None => SessionReceipt {
            verdict: "unknown".to_string(),
            evidence_exit_code: None,
            evidence_step: "none".to_string(),
            steps: session.steps.len(),
            session_id: session_id.to_string(),
        },
    }
}

fn latest_stored_snapshot_id(cli: &Cli) -> Option<String> {
    if let Ok(storage) = get_storage(cli)
        && let Ok(Some(snapshot)) = storage.get_latest_snapshot()
    {
        return Some(snapshot.snapshot_id.to_string());
    }
    None
}

fn build_forensic_report(cli: &Cli, session: &SessionRecord) -> TaskForensicReport {
    let receipt = session
        .receipt
        .clone()
        .unwrap_or_else(|| close_receipt(session, &session.session_id));
    let completed_snapshot = session
        .steps
        .iter()
        .rev()
        .find_map(|s| s.snapshot_id.clone())
        .or_else(|| latest_stored_snapshot_id(cli))
        .or_else(|| session.started_snapshot.clone());
    let failed_steps = session.steps.iter().filter(|s| s.exit_code != 0).count();

    let next_agent_guidance = match receipt.verdict.as_str() {
        "pass" => format!(
            "PASS: Prior session {} closed cleanly on step `{}` (exit 0). Safe to start the next task from snapshot {}.",
            session.session_id,
            receipt.evidence_step,
            completed_snapshot.as_deref().unwrap_or("worktree")
        ),
        "fail" => {
            let last_fail = session
                .steps
                .iter()
                .rfind(|s| s.exit_code != 0)
                .map(|s| s.invoked.join(" "))
                .unwrap_or_else(|| receipt.evidence_step.clone());
            format!(
                "REPAIR REQUIRED: Prior session {} failed on `autopsy {}` (exit {}). Do NOT build new features on top of this state until `autopsy verify` passes with exit 0.",
                session.session_id,
                last_fail,
                receipt.evidence_exit_code.unwrap_or(2)
            )
        }
        _ => format!(
            "UNVERIFIED: Session {} recorded no conclusive verification step (`{}`). Run `autopsy baseline --persist` and `autopsy verify` before editing.",
            session.session_id, receipt.evidence_step
        ),
    };

    let mut hasher = Hasher::new();
    hasher.update(b"autopsy-forensic-report-v1");
    hasher.update(session.session_id.as_bytes());
    hasher.update(session.status.as_bytes());
    hasher.update(receipt.verdict.as_bytes());
    if let Some(code) = receipt.evidence_exit_code {
        hasher.update(&code.to_le_bytes());
    }
    hasher.update(receipt.evidence_step.as_bytes());
    if let Some(s) = &session.started_snapshot {
        hasher.update(s.as_bytes());
    }
    if let Some(s) = &completed_snapshot {
        hasher.update(s.as_bytes());
    }
    for step in &session.steps {
        hasher.update(&step.index.to_le_bytes());
        for arg in &step.invoked {
            hasher.update(arg.as_bytes());
        }
        hasher.update(&step.exit_code.to_le_bytes());
        if let Some(snap) = &step.snapshot_id {
            hasher.update(snap.as_bytes());
        }
        if let Some(note) = &step.note {
            hasher.update(note.as_bytes());
        }
    }
    let report_digest = hasher.finalize().to_hex().to_string();

    TaskForensicReport {
        format_version: SESSION_STORE_FORMAT.to_string(),
        session_id: session.session_id.clone(),
        status: session.status.clone(),
        verdict: receipt.verdict,
        evidence_exit_code: receipt.evidence_exit_code,
        evidence_step: receipt.evidence_step,
        started_snapshot: session.started_snapshot.clone(),
        completed_snapshot,
        total_steps: session.steps.len(),
        failed_steps,
        steps: session.steps.clone(),
        next_agent_guidance,
        report_digest,
    }
}

fn render_forensic_markdown(report: &TaskForensicReport) -> String {
    let mut md = String::new();
    md.push_str("# SynEvid Task-Completion Forensic Report\n\n");
    md.push_str(&format!("- **Session ID:** `{}`\n", report.session_id));
    md.push_str(&format!("- **Status:** `{}`\n", report.status));
    md.push_str(&format!(
        "- **Verdict:** `{}` (evidence step: `{}`, exit code: `{}`)\n",
        report.verdict.to_uppercase(),
        report.evidence_step,
        report
            .evidence_exit_code
            .map(|c| c.to_string())
            .unwrap_or_else(|| "none".to_string())
    ));
    md.push_str(&format!(
        "- **Started Snapshot:** `{}`\n",
        report.started_snapshot.as_deref().unwrap_or("none")
    ));
    md.push_str(&format!(
        "- **Completed Snapshot:** `{}`\n",
        report.completed_snapshot.as_deref().unwrap_or("none")
    ));
    md.push_str(&format!(
        "- **Steps Executed:** {} ({} failed)\n",
        report.total_steps, report.failed_steps
    ));
    md.push_str(&format!(
        "- **Report Digest (BLAKE3):** `{}`\n\n",
        report.report_digest
    ));

    md.push_str("## Guidance for Next AI Agent / Developer\n\n");
    md.push_str(&format!("> {}\n\n", report.next_agent_guidance));

    md.push_str("## Recorded Trajectory\n\n");
    if report.steps.is_empty() {
        md.push_str("_No evidence steps were recorded in this session._\n");
    } else {
        md.push_str("| Step | Command | Exit Code | Pinned Snapshot | Repair / Evidence Note |\n");
        md.push_str("| :--- | :--- | :---: | :--- | :--- |\n");
        for step in &report.steps {
            let cmd = format!("`autopsy {}`", step.invoked.join(" "));
            let snap = step.snapshot_id.as_deref().unwrap_or("-");
            let note = step.note.as_deref().unwrap_or("-");
            md.push_str(&format!(
                "| {} | {} | `{}` | `{}` | {} |\n",
                step.index, cmd, step.exit_code, snap, note
            ));
        }
    }
    md
}

fn persist_forensic_report(cli: &Cli, report: &TaskForensicReport) -> Result<(), CliError> {
    let dir = runs_dir(cli);
    fs::create_dir_all(&dir).map_err(CliError::Io)?;
    let json_bytes =
        serde_json::to_string_pretty(report).map_err(|e| CliError::Storage(e.to_string()))?;
    let md_bytes = render_forensic_markdown(report);

    let session_json = dir.join(format!("{}.json", report.session_id));
    let session_md = dir.join(format!("{}.md", report.session_id));
    let latest_json = dir.join("latest.json");
    let latest_md = dir.join("latest.md");

    for (target, content) in [
        (&session_json, json_bytes.as_str()),
        (&session_md, md_bytes.as_str()),
        (&latest_json, json_bytes.as_str()),
        (&latest_md, md_bytes.as_str()),
    ] {
        let tmp = target.with_extension("tmp");
        fs::write(&tmp, content.as_bytes()).map_err(CliError::Io)?;
        fs::rename(&tmp, target).map_err(CliError::Io)?;
    }
    Ok(())
}

fn emit_json<T: Serialize>(report: &T) -> Result<(), CliError> {
    println!(
        "{}",
        serde_json::to_string_pretty(report).map_err(|e| CliError::Analysis(e.to_string()))?
    );
    Ok(())
}

pub fn handle_session_start(cli: &Cli) -> Result<i32, CliError> {
    let mut store = load_store(cli)?;
    let started_snapshot = latest_stored_snapshot_id(cli);
    let session_id = session_id_for(cli, next_counter(&store));
    let record = SessionRecord {
        session_id: session_id.clone(),
        status: "open".to_string(),
        started_snapshot,
        steps: Vec::new(),
        receipt: None,
    };
    store.sessions.insert(session_id.clone(), record);
    save_store(cli, &store)?;
    if cli.format == OutputFormat::Json {
        emit_json(&serde_json::json!({
            "session_id": session_id,
            "status": "open",
        }))?;
    } else {
        println!("session {session_id} open");
    }
    Ok(EXIT_PASS)
}

#[allow(clippy::too_many_arguments)]
pub fn handle_session_record(
    cli: &Cli,
    session_id: &str,
    cli_args: Vec<String>,
    exit_code: i32,
    snapshot_id: Option<&str>,
    note: Option<&str>,
) -> Result<i32, CliError> {
    let mut store = load_store(cli)?;
    let session = store.sessions.get_mut(session_id).ok_or_else(|| {
        CliError::Analysis(format!(
            "session '{session_id}' not found; open one with `autopsy session start`"
        ))
    })?;
    if session.status != "open" {
        return Err(CliError::Analysis(format!(
            "session '{session_id}' is already closed; open a new one to record further steps"
        )));
    }
    let step = SessionStep {
        index: session.steps.len(),
        invoked: cli_args,
        exit_code,
        snapshot_id: snapshot_id
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        note: note.map(|n| n.to_string()),
    };
    let step_index = session.steps.len();
    session.steps.push(step);
    let recorded_index = step_index;
    save_store(cli, &store)?;
    if cli.format == OutputFormat::Json {
        emit_json(&serde_json::json!({
            "session_id": session_id,
            "recorded": true,
            "step_index": recorded_index,
        }))?;
    } else {
        println!("session {session_id} recorded step {recorded_index}");
    }
    Ok(EXIT_PASS)
}

pub fn handle_session_show(cli: &Cli, session_id: &str) -> Result<i32, CliError> {
    let store = load_store(cli)?;
    let session = store.sessions.get(session_id).ok_or_else(|| {
        CliError::Analysis(format!(
            "session '{session_id}' not found; `autopsy session list` shows what exists"
        ))
    })?;
    if cli.format == OutputFormat::Json {
        emit_json(session)?;
    } else {
        println!(
            "session {} status={} steps={}",
            session_id,
            session.status,
            session.steps.len()
        );
        for step in &session.steps {
            println!(
                "  step {} exit={} {}",
                step.index,
                step.exit_code,
                step.invoked.join(" ")
            );
            if let Some(note) = &step.note {
                println!("    note: {note}");
            }
        }
        if let Some(receipt) = &session.receipt {
            println!("  receipt verdict={}", receipt.verdict);
        }
    }
    Ok(EXIT_PASS)
}

pub fn handle_session_close(cli: &Cli, session_id: &str) -> Result<i32, CliError> {
    let mut store = load_store(cli)?;
    let session_id = session_id.trim().to_string();
    let session = store.sessions.get_mut(&session_id).ok_or_else(|| {
        CliError::Analysis(format!(
            "session '{session_id}' not found; `autopsy session list` shows what exists"
        ))
    })?;
    if session.status == "closed" {
        return Err(CliError::Analysis(format!(
            "session '{session_id}' is already closed; its receipt is final"
        )));
    }
    let receipt = close_receipt(session, &session_id);
    session.status = "closed".to_string();
    session.receipt = Some(receipt.clone());
    let session_snapshot = session.clone();
    save_store(cli, &store)?;

    let forensic_report = build_forensic_report(cli, &session_snapshot);
    persist_forensic_report(cli, &forensic_report)?;

    if cli.format == OutputFormat::Json {
        emit_json(&receipt)?;
    } else {
        println!(
            "session {session_id} closed verdict={} evidence_step={} steps={} (forensic report: .autopsy/runs/latest.md)",
            receipt.verdict, receipt.evidence_step, receipt.steps
        );
    }
    match receipt.evidence_exit_code {
        Some(0) => Ok(EXIT_PASS),
        Some(code) if code > 0 => Ok(code),
        _ => Ok(EXIT_ANALYSIS_ERROR),
    }
}

pub fn handle_session_report(cli: &Cli, session_id: Option<&str>) -> Result<i32, CliError> {
    let store = load_store(cli)?;
    let target_session = match session_id {
        Some(id) => store.sessions.get(id.trim()).ok_or_else(|| {
            CliError::Analysis(format!(
                "session '{id}' not found; `autopsy session list` shows what exists"
            ))
        })?,
        None => store
            .sessions
            .values()
            .last()
            .ok_or_else(|| CliError::Analysis("no sessions recorded in .autopsy/sessions.json; start one with `autopsy session start`".to_string()))?,
    };

    let report = build_forensic_report(cli, target_session);
    persist_forensic_report(cli, &report)?;

    if cli.format == OutputFormat::Json {
        emit_json(&report)?;
    } else {
        print!("{}", render_forensic_markdown(&report));
    }
    Ok(EXIT_PASS)
}

pub fn handle_session_list(cli: &Cli) -> Result<i32, CliError> {
    let store = load_store(cli)?;
    if cli.format == OutputFormat::Json {
        emit_json(&store)?;
    } else {
        println!("sessions total={}", store.sessions.len());
        for (id, session) in &store.sessions {
            println!(
                "  {} status={} steps={}",
                id,
                session.status,
                session.steps.len()
            );
        }
    }
    Ok(EXIT_PASS)
}

#[cfg(test)]
mod tests {
    use super::{SessionStep, SessionStore, TaskForensicReport, load_store};
    use crate::{Cli, EXIT_ANALYSIS_ERROR, EXIT_PASS, OutputFormat, run_cli};
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    fn temp_repo() -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("autopsy-session-test-repo-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cli_for(root: &Path, extra: &[&str]) -> Vec<String> {
        let mut args = vec![
            "autopsy".to_string(),
            "--repo-root".to_string(),
            root.display().to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];
        args.extend(extra.iter().map(|s| s.to_string()));
        args
    }

    fn cli_at_root(root: &Path) -> Cli {
        clap::Parser::try_parse_from(cli_for(root, &["session", "list"])).unwrap()
    }

    #[test]
    fn session_lifecycle_records_closes_and_persists_forensic_report() {
        let root = temp_repo();
        assert_eq!(run_cli(&cli_for(&root, &["session", "start"])), EXIT_PASS);

        let store = load_store(&cli_at_root(&root)).unwrap();
        let session_id = store
            .sessions
            .keys()
            .next()
            .cloned()
            .expect("session start writes a session");

        // A failing `verify` step recorded, then close: verdict fail, exit 2.
        let rec = cli_for(
            &root,
            &[
                "session",
                "record",
                "--session-id",
                &session_id,
                "--exit-code",
                "2",
                "--note",
                "breaking contract change flagged to owner",
                "--",
                "verify",
                "--strict",
            ],
        );
        assert_eq!(run_cli(&rec), EXIT_PASS);
        let close_args = cli_for(&root, &["session", "close", "--session-id", &session_id]);
        assert_eq!(run_cli(&close_args), 2);

        let store = load_store(&cli_at_root(&root)).unwrap();
        let session = store.sessions.get(&session_id).unwrap();
        assert_eq!(session.status, "closed");
        let receipt = session.receipt.as_ref().unwrap();
        assert_eq!(receipt.verdict, "fail");
        assert_eq!(receipt.evidence_exit_code, Some(2));
        assert_eq!(receipt.evidence_step, "verify");
        assert_eq!(receipt.steps, 1);

        // Forensic run files written automatically on close:
        let latest_json = root.join(".autopsy/runs/latest.json");
        let latest_md = root.join(".autopsy/runs/latest.md");
        assert!(latest_json.is_file(), "latest.json persisted");
        assert!(latest_md.is_file(), "latest.md persisted");
        let report: TaskForensicReport =
            serde_json::from_slice(&fs::read(&latest_json).unwrap()).unwrap();
        assert_eq!(report.session_id, session_id);
        assert_eq!(report.verdict, "fail");
        assert!(report.next_agent_guidance.contains("REPAIR REQUIRED"));
        assert!(!report.report_digest.is_empty());

        // `session report` reproduces the exact report deterministically:
        assert_eq!(run_cli(&cli_for(&root, &["session", "report"])), EXIT_PASS);

        // Closing twice is an error; the receipt stays final.
        let close_again = cli_for(&root, &["session", "close", "--session-id", &session_id]);
        assert_eq!(run_cli(&close_again), EXIT_ANALYSIS_ERROR);
        let _ = std::marker::PhantomData::<SessionStore>;
        let _ = std::marker::PhantomData::<SessionStep>;
        fs::remove_dir_all(&root).ok();
        let _ = OutputFormat::Json;
    }
}
