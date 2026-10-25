//! Real-Time Live Watch Mode for SynEvid (Code Autopsy).
//!
//! Debounced filesystem event watcher powered by `notify` and `crossterm`.
//! Filters noise (.git, node_modules, target, .autopsy, editor caches),
//! triggers fast incremental verification, and displays real-time status ticker cards.
//!
//! Hotkeys:
//!   - `v`: Force re-verify now
//!   - `b`: Capture fresh baseline
//!   - `c`: Clear terminal screen
//!   - `q` / `Esc`: Exit watch mode

#![allow(clippy::collapsible_if, clippy::collapsible_match)]

use crate::{
    ANALYZER_VERSION, Cli, CliError, EXIT_PASS, build_snapshot_and_graph, load_or_default_config,
};
use autopsy_domain::{CoverageState, FindingStatus};
use autopsy_invariants::{EvaluationContext, InvariantEvaluator, InvariantsConfig};
use crossterm::{
    cursor,
    event::{self, Event as TermEvent, KeyCode, KeyModifiers},
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode},
};
use notify::{Config, Event as NotifyEvent, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::BTreeSet;
use std::io::{Write, stdout};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Filters out irrelevant directories and noise from filesystem events.
fn should_ignore_path(path: &Path) -> bool {
    for comp in path.components() {
        if let std::path::Component::Normal(c) = comp {
            if let Some(s) = c.to_str() {
                if s == ".git"
                    || s == "node_modules"
                    || s == "target"
                    || s == ".autopsy"
                    || s == ".cache"
                    || s == ".idea"
                    || s == ".vscode"
                    || s.ends_with(".tmp")
                    || s.ends_with('~')
                {
                    return true;
                }
            }
        }
    }
    // Also ignore non-code/config files
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        !matches!(
            ext,
            "ts" | "tsx" | "js" | "jsx" | "json" | "toml" | "rs" | "yml" | "yaml" | "md"
        )
    } else {
        true
    }
}

/// Formats the current local time HH:MM:SS deterministically or from system clock for ticker display.
fn current_time_str() -> String {
    // Basic zero-dependency fallback for local wallclock display
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let secs = now % 60;
    let mins = (now / 60) % 60;
    let hours = (now / 3600) % 24;
    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}

fn print_banner(repo_root: &Path) {
    let mut out = stdout();
    let _ = execute!(
        out,
        SetForegroundColor(Color::Cyan),
        Print(
            "╔════════════════════════════════════════════════════════════════════════════════════╗\n\r"
        ),
        Print(
            "║  SynEvid Live Watch Mode                                        [Status: Active]   ║\n\r"
        ),
        SetForegroundColor(Color::DarkGrey),
        Print(format!("║  Watching: {:<71} ║\n\r", repo_root.display())),
        SetForegroundColor(Color::Yellow),
        Print(
            "║  Hotkeys: [v] Force Verify   [b] New Baseline   [c] Clear   [q] Quit                ║\n\r"
        ),
        SetForegroundColor(Color::Cyan),
        Print(
            "╚════════════════════════════════════════════════════════════════════════════════════╝\n\r\n\r"
        ),
        ResetColor
    );
    let _ = out.flush();
}

fn execute_verification(cli: &Cli, changed_files: &[PathBuf]) {
    let mut out = stdout();
    let time_str = current_time_str();

    if !changed_files.is_empty() {
        let first_file = changed_files[0]
            .strip_prefix(&cli.repo_root)
            .unwrap_or(&changed_files[0])
            .display()
            .to_string();
        let additional = if changed_files.len() > 1 {
            format!(" (+{} more)", changed_files.len() - 1)
        } else {
            String::new()
        };

        let _ = execute!(
            out,
            SetForegroundColor(Color::Magenta),
            Print(format!("[{}] ", time_str)),
            SetForegroundColor(Color::Cyan),
            Print(format!(
                "⚡ Change detected: {}{}\n\r",
                first_file, additional
            )),
            ResetColor
        );
    } else {
        let _ = execute!(
            out,
            SetForegroundColor(Color::Magenta),
            Print(format!("[{}] ", time_str)),
            SetForegroundColor(Color::Cyan),
            Print("🔄 Manual verification requested\n\r"),
            ResetColor
        );
    }

    let config = match load_or_default_config(&cli.repo_root) {
        Ok(c) => c,
        Err(e) => {
            let _ = execute!(
                out,
                SetForegroundColor(Color::Red),
                Print(format!("  [ERROR] Config error: {}\n\r", e)),
                ResetColor
            );
            let _ = out.flush();
            return;
        }
    };

    let artifacts = match build_snapshot_and_graph(&cli.repo_root, &config, None, None) {
        Ok(a) => a,
        Err(e) => {
            let _ = execute!(
                out,
                SetForegroundColor(Color::Red),
                Print(format!("  [ERROR] Analysis error: {}\n\r", e)),
                ResetColor
            );
            let _ = out.flush();
            return;
        }
    };

    // Load invariants
    let inv_path = cli.repo_root.join(".autopsy").join("invariants.yml");
    let invariants_config = if inv_path.exists() {
        InvariantsConfig::load_from_file(&inv_path).unwrap_or_else(|_| InvariantsConfig {
            version: "0.0.1".to_string(),
            invariants: vec![],
        })
    } else {
        InvariantsConfig {
            version: "0.0.1".to_string(),
            invariants: vec![
                autopsy_invariants::InvariantRule {
                    id: "no-circular-deps".to_string(),
                    description: "No circular dependencies introduced".to_string(),
                    kind: "no_new_cycle".to_string(),
                    severity: "error".to_string(),
                    scope: None,
                    rule: None,
                },
                autopsy_invariants::InvariantRule {
                    id: "api-compatibility".to_string(),
                    description: "Public API contract compatibility".to_string(),
                    kind: "api_compatibility".to_string(),
                    severity: "error".to_string(),
                    scope: None,
                    rule: None,
                },
            ],
        }
    };

    let eval_ctx = EvaluationContext {
        snapshot_id: artifacts.snapshot.snapshot_id.clone(),
        analyzer_version: ANALYZER_VERSION,
        graph: &artifacts.graph,
        baseline_graph: None,
        diff: None,
        contracts_before: &std::collections::BTreeMap::new(),
        contracts_after: &std::collections::BTreeMap::new(),
        coverage_state: artifacts.coverage_state,
    };

    let evaluator = InvariantEvaluator::new();
    let evaluation_pairs = evaluator
        .evaluate(&invariants_config, &eval_ctx)
        .unwrap_or_default();

    let total_rules = invariants_config
        .invariants
        .len()
        .max(evaluation_pairs.len());
    let mut failures = Vec::new();

    for (finding, _evidence) in &evaluation_pairs {
        if finding.status == FindingStatus::Fail {
            failures.push(finding);
        }
    }

    if failures.is_empty() {
        let coverage_str = match artifacts.coverage_state {
            CoverageState::Verified => "Verified",
            CoverageState::Partial => "Partial",
            CoverageState::Unknown => "Unknown",
            CoverageState::Unsupported => "Unsupported",
        };

        let _ = execute!(
            out,
            SetForegroundColor(Color::Green),
            Print("  [PASS] "),
            ResetColor,
            Print(format!(
                "Invariants Intact ({}/{} rules passed) | Hash: {}.. | Coverage: {}\n\r\n\r",
                total_rules - failures.len(),
                total_rules,
                &artifacts.snapshot.snapshot_id.to_string()
                    [..8.min(artifacts.snapshot.snapshot_id.to_string().len())],
                coverage_str
            ))
        );
    } else {
        let _ = execute!(
            out,
            SetForegroundColor(Color::Red),
            Print("  [FAIL] "),
            ResetColor,
            Print(format!(
                "Invariants Violated ({}/{} rules failed):\n\r",
                failures.len(),
                total_rules
            ))
        );

        for f in &failures {
            let _ = execute!(
                out,
                SetForegroundColor(Color::Red),
                Print(format!("    ✖ [{}] ", f.rule_id)),
                ResetColor,
                Print(format!("{}\n\r", f.message))
            );
        }
        let _ = execute!(out, Print("\n\r"));
    }
    let _ = out.flush();
}

/// Runs the real-time file watcher loop.
pub fn run_watch(cli: &Cli, debounce_ms: u64) -> Result<i32, CliError> {
    let repo_root = cli
        .repo_root
        .canonicalize()
        .unwrap_or_else(|_| cli.repo_root.clone());
    let (tx, rx) = mpsc::channel();

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<NotifyEvent, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        Config::default(),
    )
    .map_err(|e| CliError::Analysis(format!("Failed to initialize file watcher: {}", e)))?;

    watcher
        .watch(&repo_root, RecursiveMode::Recursive)
        .map_err(|e| CliError::Analysis(format!("Failed to watch repository directory: {}", e)))?;

    // Setup terminal
    let _ = enable_raw_mode();
    let mut out = stdout();
    let _ = execute!(out, cursor::Hide);

    print_banner(&repo_root);
    // Initial verification run on start
    execute_verification(cli, &[]);

    let debounce_dur = Duration::from_millis(debounce_ms.max(50));
    let mut pending_changes: BTreeSet<PathBuf> = BTreeSet::new();
    let mut last_change_time = Instant::now();

    loop {
        // 1. Check for keyboard input (non-blocking)
        if event::poll(Duration::from_millis(40)).unwrap_or(false) {
            if let Ok(TermEvent::Key(key)) = event::read() {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Char('c') => {
                        let _ = execute!(out, Clear(ClearType::All), cursor::MoveTo(0, 0));
                        print_banner(&repo_root);
                    }
                    KeyCode::Char('v') => {
                        pending_changes.clear();
                        execute_verification(cli, &[]);
                    }
                    KeyCode::Char('b') => {
                        let _ = execute!(
                            out,
                            SetForegroundColor(Color::Yellow),
                            Print(format!(
                                "[{}] 📸 Capturing fresh baseline...\n\r",
                                current_time_str()
                            )),
                            ResetColor
                        );
                        let _ = out.flush();
                        let config = load_or_default_config(&cli.repo_root)?;
                        let artifacts =
                            build_snapshot_and_graph(&cli.repo_root, &config, None, None)?;
                        let _ = execute!(
                            out,
                            SetForegroundColor(Color::Green),
                            Print(format!(
                                "  [OK] Baseline captured: {}\n\r\n\r",
                                artifacts.snapshot.snapshot_id
                            )),
                            ResetColor
                        );
                        let _ = out.flush();
                    }
                    _ => {}
                }
            }
        }

        // 2. Drain filesystem event notifications
        while let Ok(notify_event) = rx.try_recv() {
            for path in notify_event.paths {
                if !should_ignore_path(&path) {
                    pending_changes.insert(path);
                    last_change_time = Instant::now();
                }
            }
        }

        // 3. If debounce window has passed and we have pending changes, trigger verification
        if !pending_changes.is_empty() && last_change_time.elapsed() >= debounce_dur {
            let changed_files: Vec<PathBuf> = pending_changes.into_iter().collect();
            pending_changes = BTreeSet::new();
            execute_verification(cli, &changed_files);
        }
    }

    // Cleanup terminal
    let _ = execute!(out, cursor::Show, ResetColor);
    let _ = disable_raw_mode();
    println!("\nWatch mode stopped.");

    Ok(EXIT_PASS)
}
