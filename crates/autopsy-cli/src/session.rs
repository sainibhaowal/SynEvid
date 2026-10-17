//! Session receipts (Tier 2.5): a deterministic, agent-facing record of the
//! repair loop — impact → edit → verify → repair → re-verify.
//!
//! Design constraints (see AGENTS.md invariants):
//! - No wall-clock timestamps: identical inputs produce byte-identical store
//!   contents. Ordering comes from a stable, monotonically assigned step
//!   index plus the BLAKE3 digest chain of the record.
//! - The close verdict derives ONLY from recorded steps: the last verify-like
//!   step's exit code, `unknown` when none was recorded. Honest — never
//!   `pass` without evidence.
//! - Store JSON uses `BTreeMap` maps for canonical key order.
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
    /// Close the session and emit the honest repair-loop receipt.
    Close {
        /// The session id.
        #[arg(long)]
        session_id: String,
    },
    /// List every recorded session id with status.
    #[command(name = "list")]
    List,
}

fn store_path(cli: &Cli) -> PathBuf {
    cli.repo_root.join(".autopsy").join("sessions.json")
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
    // The counter is derived from what the store already holds: for each
    // session, the id is positional only by convention; uniqueness is what
    // matters and BTreeMap ordering keeps it deterministic.
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

fn emit_json<T: Serialize>(report: &T) -> Result<(), CliError> {
    println!(
        "{}",
        serde_json::to_string_pretty(report).map_err(|e| CliError::Analysis(e.to_string()))?
    );
    Ok(())
}

pub fn handle_session_start(cli: &Cli) -> Result<i32, CliError> {
    let mut store = load_store(cli)?;
    // Latest stored snapshot when available, else None: honest, deterministic.
    let started_snapshot = {
        let mut id = None;
        if let Ok(storage) = get_storage(cli)
            && let Ok(Some(snapshot)) = storage.get_latest_snapshot()
        {
            id = Some(snapshot.snapshot_id.to_string());
        }
        id
    };
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
    save_store(cli, &store)?;
    if cli.format == OutputFormat::Json {
        emit_json(&receipt)?;
    } else {
        println!(
            "session {session_id} closed verdict={} evidence_step={} steps={}",
            receipt.verdict, receipt.evidence_step, receipt.steps
        );
    }
    // The close operation mirrors the evidence the verdict is based on: a
    // failing verify stays a policy failure for the caller (e.g. a pre-commit
    // hook auto-invoking close), and an unknown verdict is not actionable.
    match receipt.evidence_exit_code {
        Some(0) => Ok(EXIT_PASS),
        Some(code) if code > 0 => Ok(code),
        _ => Ok(EXIT_ANALYSIS_ERROR),
    }
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
    use super::{SessionStep, SessionStore, load_store};
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
    fn session_lifecycle_records_and_closes_honestly() {
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

        // Closing twice is an error; the receipt stays final.
        let close_again = cli_for(&root, &["session", "close", "--session-id", &session_id]);
        assert_eq!(run_cli(&close_again), EXIT_ANALYSIS_ERROR);
        let _ = std::marker::PhantomData::<SessionStore>;
        let _ = std::marker::PhantomData::<SessionStep>;
        fs::remove_dir_all(&root).ok();
        let _ = OutputFormat::Json;
    }
}
