//! Native pure-Rust Git CLI Subcommands.
//!
//! Complies with INV-SANDBOX-003 (Zero external git CLI sub-processes)
//! and FR-P3-001..010 (Complete native Git intelligence & guardrails).

use crate::{Cli, CliError, EXIT_PASS, OutputFormat};
use autopsy_git_engine::{
    BisectRequest, GitDiffEngine, GitGalaxyReader, GitMutationManager, GitStatusScanner,
    MergeRequest, TagRequest,
};
use clap::Subcommand;

#[derive(Subcommand, Debug, Clone)]
pub enum GitSubcommands {
    /// Scan working tree status (staged, modified, untracked, deleted, ignored)
    Status,
    /// Inspect commit DAG evolutionary topology from git init to HEAD
    Galaxy {
        /// Maximum commits to read in topological order (default 50)
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Any-to-Any semantic commit diff and contract drift analysis
    Diff {
        /// Base commit OID (starting state)
        #[arg(long)]
        base: String,
        /// Head commit OID (ending state)
        #[arg(long)]
        head: String,
    },
    /// Line-level unified diff of a specific working tree file
    DiffFile {
        /// Relative path of file to diff against HEAD
        path: String,
    },
    /// Stage specific files for verified commit
    Stage {
        /// Relative file paths to stage
        paths: Vec<String>,
    },
    /// Unstage specific files from pre-commit index
    Unstage {
        /// Relative file paths to unstage (omit to unstage all)
        paths: Vec<String>,
    },
    /// Pre-flight verify and create commit with Synevid trailers
    Commit {
        /// Commit message summary
        #[arg(short, long)]
        message: String,
        /// Optional release tag to attach to new commit
        #[arg(long)]
        tag: Option<String>,
        /// Optional author name
        #[arg(long)]
        author: Option<String>,
        /// Optional author email
        #[arg(long)]
        email: Option<String>,
    },
    /// Manage branches (create, switch, delete)
    Branch {
        /// Branch name
        name: Option<String>,
        /// Action: switch, create, or delete
        #[arg(long, default_value = "switch")]
        action: String,
    },
    /// Manage tags (create or delete)
    Tag {
        /// Tag name
        name: String,
        /// Target commit OID (defaults to HEAD)
        #[arg(long)]
        target: Option<String>,
        /// Delete existing tag
        #[arg(long)]
        delete: bool,
    },
    /// Guarded merge of source branch into active branch
    Merge {
        /// Source branch to merge into current branch
        source: String,
    },
    /// Automated autopsy visual bisect to pinpoint contract-breaking commit
    Bisect {
        /// Good commit OID (passing state)
        #[arg(long)]
        good: String,
        /// Bad commit OID (failing state)
        #[arg(long)]
        bad: String,
    },
    /// 1-click atomic undo to restore pre-mutation state from refs/autopsy/undo/
    Undo,
}

pub fn handle_git(cli: &Cli, cmd: &GitSubcommands) -> Result<i32, CliError> {
    match cmd {
        GitSubcommands::Status => {
            let scanner = GitStatusScanner::new(&cli.repo_root);
            let report = scanner
                .scan_status()
                .map_err(|e| CliError::Analysis(format!("Git status scan failed: {e}")))?;

            if cli.format == OutputFormat::Json {
                let json = serde_json::to_string_pretty(&report)
                    .map_err(|e| CliError::Analysis(e.to_string()))?;
                println!("{json}");
            } else {
                println!(
                    "On branch {} (HEAD: {})",
                    report.branch,
                    report.head_oid.as_deref().unwrap_or("none")
                );
                if !report.staged.is_empty() {
                    println!("\nChanges to be committed:");
                    for f in &report.staged {
                        println!(
                            "  \x1b[32mstaged:   {}\x1b[0m (+{} -{})",
                            f.path, f.added_lines, f.deleted_lines
                        );
                    }
                }
                if !report.unstaged.is_empty() {
                    println!("\nChanges not staged for commit:");
                    for f in &report.unstaged {
                        println!(
                            "  \x1b[33mmodified: {}\x1b[0m (+{} -{})",
                            f.path, f.added_lines, f.deleted_lines
                        );
                    }
                }
                if !report.untracked.is_empty() {
                    println!("\nUntracked files:");
                    for f in &report.untracked {
                        println!("  \x1b[36muntracked: {}\x1b[0m", f.path);
                    }
                }
                if !report.deleted.is_empty() {
                    println!("\nDeleted files:");
                    for f in &report.deleted {
                        println!("  \x1b[31mdeleted:  {}\x1b[0m", f.path);
                    }
                }
                println!("\nTotal uncommitted: {} files", report.total_uncommitted);
            }
            Ok(EXIT_PASS)
        }

        GitSubcommands::Galaxy { limit } => {
            let reader = GitGalaxyReader::new(&cli.repo_root);
            let galaxy = reader
                .read_galaxy(*limit)
                .map_err(|e| CliError::Analysis(format!("Git galaxy read failed: {e}")))?;

            if cli.format == OutputFormat::Json {
                let json = serde_json::to_string_pretty(&galaxy)
                    .map_err(|e| CliError::Analysis(e.to_string()))?;
                println!("{json}");
            } else {
                println!(
                    "Git Time-Travel Galaxy (Branch: {}, Total Commits: {})",
                    galaxy.active_branch.as_deref().unwrap_or("main"),
                    galaxy.commits.len()
                );
                for c in &galaxy.commits {
                    let head_marker = if c.is_head { " (HEAD)" } else { "" };
                    let branches = if !c.branches.is_empty() {
                        format!(" [{}]", c.branches.join(", "))
                    } else {
                        String::new()
                    };
                    let tags = if !c.tags.is_empty() {
                        format!(" <tag: {}>", c.tags.join(", "))
                    } else {
                        String::new()
                    };
                    println!(
                        "* {} - {}{}{}{} (Verdict: {})",
                        c.short_oid, c.summary, head_marker, branches, tags, c.verdict
                    );
                }
            }
            Ok(EXIT_PASS)
        }

        GitSubcommands::Diff { base, head } => {
            let differ = GitDiffEngine::new(&cli.repo_root);
            let report = differ
                .diff_commits(base, head)
                .map_err(|e| CliError::Analysis(format!("Git range diff failed: {e}")))?;

            if cli.format == OutputFormat::Json {
                let json = serde_json::to_string_pretty(&report)
                    .map_err(|e| CliError::Analysis(e.to_string()))?;
                println!("{json}");
            } else {
                println!(
                    "Commit Range Diff: {} ↔ {}",
                    &base[..7.min(base.len())],
                    &head[..7.min(head.len())]
                );
                println!("Total Changed Files: {}", report.total_changed_files);
                println!("Blast Radius Score: {}", report.blast_radius_score);
                println!("Verdict: {}", report.verdict);
                if !report.added_files.is_empty() {
                    println!("Added Files: {}", report.added_files.join(", "));
                }
                if !report.modified_files.is_empty() {
                    println!("Modified Files: {}", report.modified_files.join(", "));
                }
                if !report.deleted_files.is_empty() {
                    println!("Deleted Files: {}", report.deleted_files.join(", "));
                }
                if !report.drifted_contracts.is_empty() {
                    println!("Drifted Contracts:");
                    for c in &report.drifted_contracts {
                        println!("  - {c}");
                    }
                }
            }
            Ok(EXIT_PASS)
        }

        GitSubcommands::DiffFile { path } => {
            let scanner = GitStatusScanner::new(&cli.repo_root);
            let diff = scanner
                .diff_file(path)
                .map_err(|e| CliError::Analysis(format!("File diff failed: {e}")))?;

            if cli.format == OutputFormat::Json {
                let json = serde_json::to_string_pretty(&diff)
                    .map_err(|e| CliError::Analysis(e.to_string()))?;
                println!("{json}");
            } else {
                println!("--- a/{}", diff.path);
                println!("+++ b/{}", diff.path);
                for line in &diff.diff_lines {
                    let old_n = line
                        .old_line
                        .map(|n| format!("{n:3}"))
                        .unwrap_or_else(|| "   ".to_string());
                    let new_n = line
                        .new_line
                        .map(|n| format!("{n:3}"))
                        .unwrap_or_else(|| "   ".to_string());
                    let prefix = if line.kind == "add" {
                        "+"
                    } else if line.kind == "del" {
                        "-"
                    } else {
                        " "
                    };
                    println!("{old_n} {new_n} {prefix} {}", line.text);
                }
                println!(
                    "\n+{} -{} lines",
                    diff.total_additions, diff.total_deletions
                );
            }
            Ok(EXIT_PASS)
        }

        GitSubcommands::Stage { paths } => {
            let manager = GitMutationManager::new(&cli.repo_root);
            let res = manager
                .stage(paths)
                .map_err(|e| CliError::Analysis(format!("Stage failed: {e}")))?;

            if cli.format == OutputFormat::Json {
                println!("{}", serde_json::to_string_pretty(&res).unwrap_or_default());
            } else {
                println!("{}", res.message);
            }
            Ok(EXIT_PASS)
        }

        GitSubcommands::Unstage { paths } => {
            let manager = GitMutationManager::new(&cli.repo_root);
            let res = if paths.is_empty() {
                manager.unstage_all()
            } else {
                manager.unstage(paths)
            }
            .map_err(|e| CliError::Analysis(format!("Unstage failed: {e}")))?;

            if cli.format == OutputFormat::Json {
                println!("{}", serde_json::to_string_pretty(&res).unwrap_or_default());
            } else {
                println!("{}", res.message);
            }
            Ok(EXIT_PASS)
        }

        GitSubcommands::Commit {
            message,
            tag,
            author,
            email,
        } => {
            let manager = GitMutationManager::new(&cli.repo_root);
            let res = manager
                .commit(message, author.as_deref(), email.as_deref())
                .map_err(|e| CliError::Analysis(format!("Commit failed: {e}")))?;

            if let (Some(tag_name), Some(new_oid)) = (tag, &res.new_commit_oid) {
                let _ = manager.tag(&TagRequest {
                    tag_name: tag_name.clone(),
                    target_oid: Some(new_oid.clone()),
                    message: None,
                    action: Some("create".to_string()),
                });
            }

            if cli.format == OutputFormat::Json {
                println!("{}", serde_json::to_string_pretty(&res).unwrap_or_default());
            } else {
                println!("{}", res.message);
                if let Some(ref oid) = res.new_commit_oid {
                    println!("Commit OID: {oid}");
                }
                if let Some(ref undo_id) = res.undo_checkpoint_id {
                    println!("Undo checkpoint saved: refs/autopsy/undo/{undo_id}");
                }
            }
            Ok(EXIT_PASS)
        }

        GitSubcommands::Branch { name, action } => {
            let manager = GitMutationManager::new(&cli.repo_root);
            let target_name = name.as_deref().unwrap_or("main");
            let res = manager
                .branch(target_name, action)
                .map_err(|e| CliError::Analysis(format!("Branch action failed: {e}")))?;

            if cli.format == OutputFormat::Json {
                println!("{}", serde_json::to_string_pretty(&res).unwrap_or_default());
            } else {
                println!("{}", res.message);
            }
            Ok(EXIT_PASS)
        }

        GitSubcommands::Tag {
            name,
            target,
            delete,
        } => {
            let manager = GitMutationManager::new(&cli.repo_root);
            let action = if *delete { "delete" } else { "create" };
            let res = manager
                .tag(&TagRequest {
                    tag_name: name.clone(),
                    target_oid: target.clone(),
                    message: None,
                    action: Some(action.to_string()),
                })
                .map_err(|e| CliError::Analysis(format!("Tag action failed: {e}")))?;

            if cli.format == OutputFormat::Json {
                println!("{}", serde_json::to_string_pretty(&res).unwrap_or_default());
            } else {
                println!("{}", res.message);
            }
            Ok(EXIT_PASS)
        }

        GitSubcommands::Merge { source } => {
            let manager = GitMutationManager::new(&cli.repo_root);
            let res = manager
                .merge(&MergeRequest {
                    source_branch: source.clone(),
                    strategy: None,
                })
                .map_err(|e| CliError::Analysis(format!("Merge failed: {e}")))?;

            if cli.format == OutputFormat::Json {
                println!("{}", serde_json::to_string_pretty(&res).unwrap_or_default());
            } else {
                println!("{}", res.message);
            }
            Ok(EXIT_PASS)
        }

        GitSubcommands::Bisect { good, bad } => {
            let manager = GitMutationManager::new(&cli.repo_root);
            let report = manager
                .bisect(&BisectRequest {
                    start_oid: good.clone(),
                    end_oid: bad.clone(),
                    rule_id: None,
                })
                .map_err(|e| CliError::Analysis(format!("Bisect failed: {e}")))?;

            if cli.format == OutputFormat::Json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).unwrap_or_default()
                );
            } else {
                println!("Autopsy Visual Bisect Report:");
                println!(
                    "Evaluated {} commits across range",
                    report.commits_evaluated
                );
                if let Some(ref breaking) = report.breaking_commit_oid {
                    println!("\x1b[31mFirst Breaking Commit:\x1b[0m {breaking}");
                    println!("Summary: {}", report.breaking_summary);
                    println!("Author:  {}", report.breaking_author);
                    if !report.drifted_symbols.is_empty() {
                        println!("Drifted Symbols: {}", report.drifted_symbols.join(", "));
                    }
                } else {
                    println!(
                        "\x1b[32mNo breaking commit identified. All evaluated commits passed contracts.\x1b[0m"
                    );
                }
            }
            Ok(EXIT_PASS)
        }

        GitSubcommands::Undo => {
            let manager = GitMutationManager::new(&cli.repo_root);
            let res = manager
                .undo()
                .map_err(|e| CliError::Analysis(format!("Undo failed: {e}")))?;

            if cli.format == OutputFormat::Json {
                println!("{}", serde_json::to_string_pretty(&res).unwrap_or_default());
            } else {
                println!("{}", res.message);
            }
            Ok(EXIT_PASS)
        }
    }
}
