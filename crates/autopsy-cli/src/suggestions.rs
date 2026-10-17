//! Agent feedback channel (Tier 3.3): correction receipts.
//!
//! An agent that repaired a problem can attach the verbatim fix to the run it
//! belongs to: `autopsy <cmd> --suggestion "the invariants gate was stale; ran
//! verify --strict after re-persisting"`. The receipt is stored deterministically
//! in `.autopsy/suggestions.json` — BLAKE3 id over the normalized root and the
//! store's own counter, no wall-clock time anywhere — so a corpus of real agent
//! corrections accumulates next to the evidence it explains.
//!
//! `--disable-suggestion` suppresses the store write and the failure hint: the
//! opt-out exists so hosts that already collect feedback elsewhere never get
//! double-booked. Like sessions, this store is agent bookkeeping — the analyzer
//! itself stays read-only over source.

use crate::{Cli, CliError};
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf};

pub const SUGGESTION_STORE_FORMAT: &str = "0.0.1";

/// One correction receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    /// Deterministic BLAKE3 id over the normalized workspace root and the
    /// store's own counter (no wall-clock time).
    pub id: String,
    /// The subcommand the note is attached to.
    pub command: String,
    /// The invocation argv, verbatim (the engine name is the prefix).
    pub invoked: Vec<String>,
    /// The agent's verbatim note about what actually repaired the problem.
    pub text: String,
    /// The exit code the run produced with the fix in place, when the receipt
    /// was attached to a finished run.
    pub exit_code: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SuggestionStore {
    pub format_version: String,
    pub suggestions: BTreeMap<String, Suggestion>,
}

impl SuggestionStore {
    pub fn new() -> Self {
        Self {
            format_version: SUGGESTION_STORE_FORMAT.to_string(),
            suggestions: BTreeMap::new(),
        }
    }
}

fn store_path(cli: &Cli) -> PathBuf {
    cli.repo_root.join(".autopsy").join("suggestions.json")
}

fn load_store(cli: &Cli) -> Result<SuggestionStore, CliError> {
    let path = store_path(cli);
    if !path.exists() {
        return Ok(SuggestionStore::new());
    }
    let bytes = fs::read(&path).map_err(CliError::Io)?;
    serde_json::from_slice(&bytes).map_err(|e| {
        CliError::Storage(format!(
            "suggestions store unreadable at {}: {e}",
            path.display()
        ))
    })
}

fn save_store(cli: &Cli, store: &SuggestionStore) -> Result<(), CliError> {
    let path = store_path(cli);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(CliError::Io)?;
    }
    let canonical =
        serde_json::to_string_pretty(store).map_err(|e| CliError::Storage(e.to_string()))?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, canonical.as_bytes()).map_err(CliError::Io)?;
    fs::rename(&temp, &path).map_err(CliError::Io)?;
    Ok(())
}

fn normalized_root(cli: &Cli) -> String {
    // INV-CROSS-001: strict forward-slash normalization across OSs.
    cli.repo_root
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}

fn suggestion_id(cli: &Cli, counter: u64) -> String {
    let mut hasher = Hasher::new();
    hasher.update(b"autopsy-suggestion-v1");
    hasher.update(normalized_root(cli).as_bytes());
    hasher.update(counter.to_le_bytes().as_slice());
    hasher.finalize().to_hex()[..32].to_string()
}

/// The invoked argv of this run (everything after the global flags is already
/// in `cli.command`; the full argv is what the receipt should preserve).
pub fn invoked_argv(args: &[String]) -> Vec<String> {
    args.iter()
        .skip(1) // drop the argv[0] binary name
        .cloned()
        .collect()
}

/// Records the `--suggestion` receipt for a finished run. Returns the receipt
/// id. Storage is the agent's own workspace store — never the source tree.
pub fn record_suggestion(
    cli: &Cli,
    args: &[String],
    exit_code: i32,
) -> Result<Option<String>, CliError> {
    if cli.disable_suggestion {
        return Ok(None);
    }
    let Some(text) = cli
        .suggestion
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    else {
        return Ok(None);
    };
    let mut store = load_store(cli)?;
    let id = suggestion_id(cli, store.suggestions.len() as u64);
    let command = match &cli.command {
        crate::Commands::Baseline { .. } => "baseline",
        crate::Commands::Impact { .. } => "impact",
        crate::Commands::Diff { .. } => "diff",
        crate::Commands::Verify { .. } => "verify",
        crate::Commands::Explain { .. } => "explain",
        crate::Commands::Symbols { .. } => "symbols",
        crate::Commands::Edges { .. } => "edges",
        crate::Commands::Find { .. } => "find",
        crate::Commands::Map { .. } => "map",
        crate::Commands::Session { .. } => "session",
        crate::Commands::Workspace { .. } => "workspace",
        crate::Commands::Temporal { .. } => "temporal",
        crate::Commands::Lineage { .. } => "lineage",
        crate::Commands::TestSelect { .. } => "test-select",
        crate::Commands::ContractDist { .. } => "contract-dist",
        crate::Commands::Telemetry { .. } => "telemetry",
        crate::Commands::Cochange { .. } => "cochange",
        crate::Commands::Catalog { .. } => "catalog",
        crate::Commands::BlastRadius { .. } => "blast-radius",
        crate::Commands::Doctor => "doctor",
        crate::Commands::Tools => "tools",
        crate::Commands::Version => "version",
    };
    let receipt = Suggestion {
        id: id.clone(),
        command: command.to_string(),
        invoked: invoked_argv(args),
        text: text.to_string(),
        exit_code,
    };
    store.suggestions.insert(id.clone(), receipt);
    save_store(cli, &store)?;
    Ok(Some(id))
}

/// The one-line stderr hint that invites a correction receipt after a failure.
/// Suppressed entirely by `--disable-suggestion`.
pub fn failure_hint(cli: &Cli) -> Option<String> {
    if cli.disable_suggestion {
        return None;
    }
    Some("hint: if you repaired this, re-run with --suggestion \"<the fix>\" to leave a correction receipt (or pass --disable-suggestion to silence this hint)".to_string())
}

/// The success-side echo of a stored receipt (stderr, so JSON stdout is unmoved).
pub fn receipt_echo(id: &str) -> String {
    format!("suggestion_recorded: {id}")
}

#[allow(clippy::too_many_arguments)]
pub fn emit_feedback(cli: &Cli, args: &[String], code: i32) -> Result<(), CliError> {
    if let Some(id) = record_suggestion(cli, args, code)? {
        eprintln!("{}", receipt_echo(&id));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cli, EXIT_ANALYSIS_ERROR, EXIT_PASS, OutputFormat, run_cli};
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    fn temp_repo(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("synevid-suggestion-{tag}-{}", std::process::id()));
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

    fn cli_at(root: &Path, extra: &[&str]) -> Cli {
        clap::Parser::try_parse_from(cli_for(root, extra)).unwrap()
    }

    #[test]
    fn suggestion_receipts_are_deterministic_and_opt_outable() {
        let root = temp_repo("det");

        // A failing run (empty find pattern) with a suggestion attached.
        let args = cli_for(
            &root,
            &[
                "find",
                "   ",
                "--suggestion",
                "the pattern was blank; resolved the symbol with find --name-only first",
            ],
        );
        assert_eq!(run_cli(&args), EXIT_ANALYSIS_ERROR);

        let store = load_store(&cli_at(&root, &["tools"])).unwrap();
        assert_eq!(
            store.suggestions.len(),
            1,
            "the failed run recorded the receipt"
        );
        let receipt = store.suggestions.values().next().unwrap();
        assert_eq!(receipt.command, "find");
        assert_eq!(receipt.exit_code, EXIT_ANALYSIS_ERROR);
        assert!(receipt.text.contains("name-only"));
        assert!(receipt.invoked.windows(2).any(|w| w[0] == "--suggestion"));

        // A second receipt gets a distinct deterministic id (counter increments).
        let args2 = cli_for(
            &root,
            &[
                "doctor",
                "--suggestion",
                "config was missing the exclude field",
            ],
        );
        assert_eq!(run_cli(&args2), EXIT_PASS);
        let store = load_store(&cli_at(&root, &["tools"])).unwrap();
        assert_eq!(store.suggestions.len(), 2);

        // Byte-identical store across independent re-derivation: no timestamps.
        let canonical = serde_json::to_string_pretty(&store).unwrap();
        let again = load_store(&cli_at(&root, &["tools"])).unwrap();
        assert_eq!(canonical, serde_json::to_string_pretty(&again).unwrap());

        // --disable-suggestion: storage stays untouched.
        let args3 = cli_for(
            &root,
            &["doctor", "--suggestion", "ignored", "--disable-suggestion"],
        );
        assert_eq!(run_cli(&args3), EXIT_PASS);
        let store_after = load_store(&cli_at(&root, &["tools"])).unwrap();
        assert_eq!(
            store_after.suggestions.len(),
            2,
            "the opt-out suppressed storage"
        );

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn failure_hint_respects_the_opt_out() {
        let root = temp_repo("hint");
        let hint_cli = cli_at(&root, &["find", "x"]);
        assert!(failure_hint(&hint_cli).is_some());
        let quiet_cli = cli_at(&root, &["find", "x", "--disable-suggestion"]);
        assert!(failure_hint(&quiet_cli).is_none());
        let _ = OutputFormat::Json;
        fs::remove_dir_all(&root).ok();
    }
}
