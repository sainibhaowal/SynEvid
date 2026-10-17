//! Workspace registry (Tier 2.3): a deterministic multi-repo registry stored
//! at `<repo_root>/.autopsy/workspace.json`.
//!
//! The registry records repositories agent sessions operate across; the
//! `list` command reports honest status per entry — `accessible` only when
//! the registered path exists on disk, `analyzed` only when the repo owns an
//! autopsy storage that holds at least one stored snapshot. Never inferred
//! fake states; coverage counts and the federation coverage state come from
//! the `autopsy-workspace-registry` engine, which owns the semantics.

use crate::{Cli, CliError, EXIT_PASS};
use autopsy_workspace_registry::{Registry, RegistryError, RepoEntry, federation_coverage};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

pub const WORKSPACE_STORE_FORMAT: &str = "0.0.1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    pub id: String,
    /// Repository path, normalized to forward slashes. Relative entries are
    /// interpreted against the workspace root's own `.autopsy` registry.
    pub path: String,
    pub accessible: bool,
    /// Honest analysis status: true only when the repo's autopsy storage
    /// opens and holds at least one stored snapshot.
    pub analyzed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceFile {
    pub format_version: String,
    pub entries: std::collections::BTreeMap<String, WorkspaceEntry>,
}

#[derive(Debug, Subcommand)]
pub enum WorkspaceAction {
    /// Register a repository with the workspace.
    Register {
        /// Repository path, absolute or relative to the workspace root.
        path: std::path::PathBuf,
        /// Optional stable id (defaults to the path's final segment).
        #[arg(long)]
        id: Option<String>,
    },
    /// Remove an entry by id or path.
    Remove {
        #[arg(long, value_name = "TARGET")]
        id: String,
    },
    /// List the workspace with honest per-repo status and coverage counts.
    #[command(name = "list")]
    List,
}

fn workspace_path(cli: &Cli) -> PathBuf {
    cli.repo_root.join(".autopsy").join("workspace.json")
}

fn load_workspace(cli: &Cli) -> Result<WorkspaceFile, CliError> {
    let path = workspace_path(cli);
    if !path.exists() {
        return Ok(WorkspaceFile {
            format_version: WORKSPACE_STORE_FORMAT.to_string(),
            entries: std::collections::BTreeMap::new(),
        });
    }
    let bytes = fs::read(&path).map_err(CliError::Io)?;
    serde_json::from_slice(&bytes).map_err(|e| {
        CliError::Storage(format!(
            "workspace store unreadable at {}: {e}",
            path.display()
        ))
    })
}

fn save_workspace(cli: &Cli, workspace: &WorkspaceFile) -> Result<(), CliError> {
    let path = workspace_path(cli);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(CliError::Io)?;
    }
    let canonical =
        serde_json::to_string_pretty(workspace).map_err(|e| CliError::Storage(e.to_string()))?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, canonical.as_bytes()).map_err(CliError::Io)?;
    fs::rename(&temp, &path).map_err(CliError::Io)?;
    Ok(())
}

/// Normalized, forward-slash path string (INV-CROSS-001), absolute form kept
/// so entries stay meaningful from any workspace.
fn normalize_path(cli: &Cli, path: &std::path::Path) -> String {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cli.repo_root.join(path)
    };
    joined.to_string_lossy().replace('\\', "/")
}

fn default_id_for(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("root")
        .to_string()
}

/// Honest analysis detection: a repo is "analyzed" only when its autopsy
/// storage opens and holds at least one stored snapshot. Deterministic.
fn repo_is_analyzed(path: &str) -> bool {
    let root = PathBuf::from(path);
    let db = root.join(".autopsy").join("storage.db");
    if !db.is_file() {
        return false;
    }
    let options = autopsy_storage::StorageOptions {
        use_wal: false,
        cache_dir: Some(root.join(".autopsy").join("cache")),
    };
    match autopsy_storage::StorageEngine::open(&db, options) {
        Ok(storage) => matches!(storage.get_latest_snapshot(), Ok(Some(_))),
        Err(_) => false,
    }
}

pub fn handle_workspace_register(
    cli: &Cli,
    path: &std::path::Path,
    explicit_id: Option<&str>,
) -> Result<i32, CliError> {
    let mut workspace = load_workspace(cli)?;
    let normalized = normalize_path(cli, path);
    let id = explicit_id
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_id_for(&normalized));
    if workspace.entries.contains_key(&id) {
        return Err(CliError::Storage(format!(
            "workspace already has a repository with id '{id}'; pick another `--id`"
        )));
    }
    let accessible = std::path::Path::new(&normalized).is_dir();
    let analyzed = if accessible {
        repo_is_analyzed(&normalized)
    } else {
        false
    };
    let entry = WorkspaceEntry {
        id: id.clone(),
        path: normalized.clone(),
        accessible,
        analyzed,
    };
    workspace.entries.insert(id.clone(), entry.clone());
    // The engine's registry semantics guard duplicates and own the digest.
    let mut registry = Registry::new();
    for entry in workspace.entries.values() {
        registry
            .register(RepoEntry {
                id: entry.id.clone(),
                provider: "local".to_string(),
                org: workspace_root_org(entry),
                repo: entry.path.clone(),
                accessible: entry.accessible,
            })
            .map_err(|e: RegistryError| CliError::Storage(e.to_string()))?;
    }
    save_workspace(cli, &workspace)?;
    if cli.format == crate::OutputFormat::Json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "registered": true,
                "id": id,
                "path": normalized,
                "accessible": entry.accessible,
                "analyzed": entry.analyzed,
                "workspace_digest": registry.digest(),
            }))
            .map_err(|e| CliError::Analysis(e.to_string()))?
        );
    } else {
        println!(
            "registered {} -> {} (accessible={}, analyzed={})",
            id, normalized, entry.accessible, entry.analyzed
        );
    }
    Ok(EXIT_PASS)
}

/// Federation "org" placeholder for local provider; the path's parent so an
/// org actually groups sibling repositories.
fn workspace_root_org(entry: &WorkspaceEntry) -> String {
    entry
        .path
        .trim_end_matches('/')
        .rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .unwrap_or_else(|| "/".to_string())
}

pub fn handle_workspace_remove(cli: &Cli, target: &str) -> Result<i32, CliError> {
    let mut workspace = load_workspace(cli)?;
    let target = target.trim();
    let key = if workspace.entries.contains_key(target) {
        Some(target.to_string())
    } else {
        workspace
            .entries
            .iter()
            .find(|(_, entry)| entry.path == target)
            .map(|(id, _)| id.clone())
    };
    let key = key.ok_or_else(|| {
        CliError::Analysis(format!(
            "no workspace entry matches '{target}' (list shows what is registered)"
        ))
    })?;
    workspace.entries.remove(&key);
    save_workspace(cli, &workspace)?;
    if cli.format == crate::OutputFormat::Json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "removed": true,
                "id": key,
            }))
            .map_err(|e| CliError::Analysis(e.to_string()))?
        );
    } else {
        println!("removed {key}");
    }
    Ok(EXIT_PASS)
}

pub fn handle_workspace_list(cli: &Cli) -> Result<i32, CliError> {
    let mut workspace = load_workspace(cli)?;
    // Re-detect honest status at read time (registered entries can move).
    for entry in workspace.entries.values_mut() {
        entry.accessible = std::path::Path::new(&entry.path).is_dir();
        entry.analyzed = if entry.accessible {
            repo_is_analyzed(&entry.path)
        } else {
            false
        };
    }
    let entries = workspace.entries.clone();
    if workspace.entries.is_empty() {
        save_workspace(cli, &workspace)?;
    } else {
        // Persist only when detection changed something; comparing costs nothing.
        let changed = entries != workspace.entries;
        if changed {
            save_workspace(cli, &workspace)?;
        }
    }

    // Engine-owned coverage semantics over the live entries.
    let mut registry = Registry::new();
    for entry in workspace.entries.values() {
        registry
            .register(RepoEntry {
                id: entry.id.clone(),
                provider: "local".to_string(),
                org: workspace_root_org(entry),
                repo: entry.path.clone(),
                accessible: entry.accessible,
            })
            .map_err(|e: RegistryError| CliError::Storage(e.to_string()))?;
    }
    let report = serde_json::json!({
        "format_version": workspace.format_version,
        "workspace_digest": registry.digest(),
        "repos": workspace.entries.values().collect::<Vec<_>>(),
        "coverage": coverage_counts(&registry, &workspace.entries),
    });
    if cli.format == crate::OutputFormat::Json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| CliError::Analysis(e.to_string()))?
        );
    } else {
        let coverage = &report["coverage"];
        println!(
            "workspace repos={} accessible={} analyzed={} excluded={}",
            coverage["known"], coverage["accessible"], coverage["analyzed"], coverage["excluded"]
        );
        for entry in workspace.entries.values() {
            println!(
                "  {} {} accessible={} analyzed={}",
                entry.id, entry.path, entry.accessible, entry.analyzed
            );
        }
        println!("workspace_digest={}", registry.digest());
    }
    Ok(EXIT_PASS)
}

/// Engine-owned coverage semantics: `known`/`accessible`/`excluded` from the
/// registry engine's own counts, `analyzed` our honest storage-backed status,
/// and the federation coverage state. Deterministic.
fn coverage_counts(
    registry: &Registry,
    entries: &std::collections::BTreeMap<String, WorkspaceEntry>,
) -> serde_json::Value {
    let (known, accessible, _, excluded) = registry.coverage_summary();
    let analyzed = entries.values().filter(|e| e.analyzed).count();
    let state = match federation_coverage(registry) {
        autopsy_domain::CoverageState::Verified => "verified",
        autopsy_domain::CoverageState::Partial => "partial",
        autopsy_domain::CoverageState::Unknown => "unknown",
        autopsy_domain::CoverageState::Unsupported => "unsupported",
    };
    serde_json::json!({
        "known": known,
        "accessible": accessible,
        "analyzed": analyzed,
        "excluded": excluded,
        "state": state,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run_cli;

    fn cli_args(root: &std::path::Path, extra: &[&str]) -> Vec<String> {
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

    #[test]
    fn workspace_register_list_remove_is_deterministic_and_honest() {
        let root = std::env::temp_dir().join(format!("synevid-ws-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sibling")).expect("sibling repo dir");
        fs::create_dir_all(&root).unwrap();

        // Register the workspace's own root.
        assert_eq!(
            run_cli(&cli_args(&root, &["workspace", "register", "."])),
            EXIT_PASS
        );
        // Register a sibling dir that exists but was never analyzed.
        assert_eq!(
            run_cli(&cli_args(
                &root,
                &["workspace", "register", "sibling", "--id", "sib"]
            )),
            EXIT_PASS
        );
        // A duplicate id is refused.
        assert_eq!(
            run_cli(&cli_args(&root, &["workspace", "register", "."])),
            crate::EXIT_ANALYSIS_ERROR
        );
        // List shape is deterministic across two reads.
        let first = captured_list(&root, &["workspace", "list"]);
        let second = captured_list(&root, &["workspace", "list"]);
        assert_eq!(first, second);
        let parsed = first;
        assert_eq!(parsed["coverage"]["known"], 2);
        assert_eq!(parsed["coverage"]["accessible"], 2);
        assert_eq!(parsed["coverage"]["analyzed"], 0, "no storage exists");
        assert_eq!(parsed["coverage"]["state"], "verified");
        // Remove then list reflects it.
        assert_eq!(
            run_cli(&cli_args(&root, &["workspace", "remove", "--id", "sib"])),
            EXIT_PASS
        );
        let after = captured_list(&root, &["workspace", "list"]);
        assert_eq!(after["coverage"]["known"], 1);
        fs::remove_dir_all(&root).ok();
    }

    fn captured_list(root: &std::path::Path, extra: &[&str]) -> serde_json::Value {
        let args = cli_args(root, extra);
        let cli = clap::Parser::try_parse_from(&args).unwrap();
        let mut workspace = load_workspace(&cli).unwrap();
        for entry in workspace.entries.values_mut() {
            entry.accessible = std::path::Path::new(&entry.path).is_dir();
            entry.analyzed = entry.accessible && repo_is_analyzed(&entry.path);
        }
        let mut registry = Registry::new();
        for entry in workspace.entries.values() {
            registry
                .register(RepoEntry {
                    id: entry.id.clone(),
                    provider: "local".to_string(),
                    org: workspace_root_org(entry),
                    repo: entry.path.clone(),
                    accessible: entry.accessible,
                })
                .unwrap();
        }
        serde_json::json!({
            "coverage": coverage_counts(&registry, &workspace.entries),
            "repos": workspace.entries.values().collect::<Vec<_>>(),
        })
    }
}
