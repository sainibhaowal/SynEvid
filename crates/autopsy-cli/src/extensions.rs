//! Step 44 — CLI subcommands for the Phase 4.2 engines (`autopsy-cli`).
//!
//! Every Phase 4.2 pillar is a library engine with its own crate, its own digest and its own
//! coverage state. This module is the surface that reaches them: `temporal`, `test-select`,
//! `contract-dist`, `telemetry`, `cochange` and `catalog`.
//!
//! # Exit codes, decided in one place
//!
//! The engine-wide contract is `0` pass, `2` policy failure, `3` analysis error, `4`
//! unsupported/unknown. What each code means *here*:
//!
//! | Code | When |
//! |---|---|
//! | `0` | the command produced an answer it can stand behind |
//! | `2` | a policy gate failed: a `Breaking` contract verdict, or `--fail-if-all-tests-required` when the selection could not narrow |
//! | `3` | the analysis could not be performed: bad arguments, unreadable input, a malformed document, no salt |
//! | `4` | the answer is **not actionable**: coverage is `Unsupported` or `Unknown`, or `Partial` under `--strict` |
//!
//! `Unknown` exits `4` **without** `--strict`, and that is deliberate: an engine that says
//! \"I cannot answer this\" must not hand a CI script a zero and let it proceed. `--strict`
//! extends the same refusal to `Partial`, where the answer is *incomplete* rather than
//! absent. This is the CLI half of `FR-014` and `FR-030`: never convert `UNKNOWN` into `PASS`.
//!
//! # Determinism
//!
//! No command reads a clock. The mining window anchors on the newest commit's timestamp
//! **from the repository**; every other engine takes its time from the input document or not
//! at all. JSON output is `serde_json` over `BTreeMap`-backed values, so field order is a
//! function of the field name and two runs produce identical bytes. The digest inside each
//! report is over a canonical projection, so the *rendering* below is not the digest and
//! changing it cannot change one.
//!
//! # Read-only (`FR-026`)
//!
//! Nothing here writes a file, a database row or a git object. Reports go to stdout; the
//! storage path is untouched by every command in this module.

use crate::{Cli, CliError, EXIT_PASS, EXIT_POLICY_FAIL, EXIT_UNSUPPORTED, OutputFormat};
use autopsy_domain::CoverageState;
use std::fs;
use std::path::{Path, PathBuf};

/// What a subcommand produced, before it is printed.
///
/// `Debug` so a caller can `expect` it: an assertion failure has to be able to show what the
/// command produced instead.
///
/// The handlers are pure with respect to the terminal: they return the JSON and the text a
/// caller would print, which is what makes the exit-code rules testable without a pty.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub exit_code: i32,
    pub json: String,
    pub text: String,
}

impl Outcome {
    fn new<T: serde::Serialize>(exit_code: i32, value: &T, text: String) -> Result<Self, CliError> {
        Ok(Self {
            exit_code,
            json: serde_json::to_string_pretty(value).map_err(|error| {
                CliError::Analysis(format!("report is not serializable: {error}"))
            })?,
            text,
        })
    }

    /// Writes the outcome in the requested format. `sarif` is refused rather than
    /// approximated: SARIF describes findings, and none of these reports is one.
    pub fn print(&self, format: OutputFormat) -> Result<(), CliError> {
        match format {
            OutputFormat::Json => println!("{}", self.json),
            OutputFormat::Text => print!("{}", self.text),
            OutputFormat::Sarif => {
                return Err(CliError::Config(
                    "sarif is not defined for this command (it reports findings; these commands \
                     report evidence)"
                        .to_string(),
                ));
            }
        }
        Ok(())
    }
}

/// The coverage rule from the table in the module docs.
fn coverage_exit(strict: bool, coverage: CoverageState) -> i32 {
    match coverage {
        CoverageState::Unsupported | CoverageState::Unknown => EXIT_UNSUPPORTED,
        CoverageState::Partial if strict => EXIT_UNSUPPORTED,
        CoverageState::Verified | CoverageState::Partial => EXIT_PASS,
    }
}

// ---------------------------------------------------------------------------
// Repository plumbing
// ---------------------------------------------------------------------------

/// The git directory for a work tree, including the two cases `.git` is not a directory.
///
/// * A **worktree or submodule** has `.git` as a *file* containing `gitdir: <path>`.
/// * A worktree's gitdir in turn holds `commondir`, and the objects live in the common
///   directory rather than beside the worktree's own HEAD.
///
/// Both are one `fs` read away, and refusing them would make two engines unavailable in
/// exactly the setup a reviewer checking out a branch would be in.
fn git_dir(root: &Path) -> Result<PathBuf, CliError> {
    let dot_git = root.join(".git");
    let metadata = fs::metadata(&dot_git).map_err(|_| {
        CliError::Config(format!(
            "{} is not a git repository ({} is missing)",
            root.display(),
            dot_git.display()
        ))
    })?;
    let directory = if metadata.is_dir() {
        dot_git
    } else {
        let text = fs::read_to_string(&dot_git)
            .map_err(|error| CliError::Config(format!("{}: {error}", dot_git.display())))?;
        let target = text
            .trim()
            .strip_prefix("gitdir:")
            .map(str::trim)
            .ok_or_else(|| {
                CliError::Config(format!(
                    "{} is neither a directory nor a `gitdir:` pointer",
                    dot_git.display()
                ))
            })?;
        let target = PathBuf::from(target);
        if target.is_absolute() {
            target
        } else {
            root.join(target)
        }
    };
    let common = directory.join("commondir");
    if common.is_file() {
        let text = fs::read_to_string(&common)
            .map_err(|error| CliError::Config(format!("{}: {error}", common.display())))?;
        let target = PathBuf::from(text.trim());
        return Ok(if target.is_absolute() {
            target
        } else {
            directory.join(target)
        });
    }
    Ok(directory)
}

fn read_text(path: &Path) -> Result<String, CliError> {
    fs::read_to_string(path).map_err(|error| {
        CliError::Analysis(format!("{} could not be read: {error}", path.display()))
    })
}

/// The declared format, from the extension. `Inferred` when the extension says nothing, and
/// the parser records that guess rather than hiding it.
fn document_format(path: &Path) -> autopsy_contracts_distributed::DocFormat {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("json") => autopsy_contracts_distributed::DocFormat::Json,
        Some("yaml") | Some("yml") => autopsy_contracts_distributed::DocFormat::Yaml,
        _ => autopsy_contracts_distributed::DocFormat::Inferred,
    }
}

/// Bounds and pruned directories for the CLI's own bounded walk of a repository, used only
/// for test discovery. The engines keep their own bounds; this one exists because the CLI
/// must hand `discover_inventory` file contents and nothing else does that job.
const WALK_PRUNED: &[&str] = &[
    ".git",
    ".autopsy",
    "node_modules",
    "target",
    "dist",
    "build",
    "vendor",
    ".venv",
    "venv",
    "__pycache__",
];
const WALK_MAX_DEPTH: usize = 24;
const WALK_MAX_FILES: usize = 20_000;

/// Reads every file under `root` that the engine considers a **test file**.
///
/// `is_test_file` decides, not this function: whatever the engine means by a test file is
/// what gets discovered, so the CLI cannot drift from it. Symlinks are skipped and the walk
/// is bounded, for the same reasons as `autopsy-catalog`'s discovery.
fn discover_test_files(
    root: &Path,
) -> Result<(Vec<autopsy_test_impact::TestDiscovery>, usize), CliError> {
    let mut files = Vec::new();
    let mut skipped = 0usize;
    let mut queue: Vec<(PathBuf, String, usize)> = vec![(root.to_path_buf(), String::new(), 0)];

    while let Some((directory, prefix, depth)) = queue.pop() {
        if depth > WALK_MAX_DEPTH {
            skipped += 1;
            continue;
        }
        let mut entries: Vec<(String, PathBuf)> = Vec::new();
        for entry in fs::read_dir(&directory)
            .map_err(|error| CliError::Analysis(format!("{}: {error}", directory.display())))?
        {
            let entry = entry
                .map_err(|error| CliError::Analysis(format!("{}: {error}", directory.display())))?;
            entries.push((
                entry.file_name().to_string_lossy().to_string(),
                entry.path(),
            ));
        }
        entries.sort();
        for (name, path) in entries {
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                skipped += 1;
                continue;
            };
            if metadata.file_type().is_symlink() {
                skipped += 1;
                continue;
            }
            let relative = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if metadata.is_dir() {
                if WALK_PRUNED.contains(&name.as_str()) {
                    continue;
                }
                queue.push((path, relative, depth + 1));
                continue;
            }
            if !metadata.is_file() || !autopsy_test_impact::is_test_file(&relative) {
                continue;
            }
            if files.len() >= WALK_MAX_FILES {
                skipped += 1;
                continue;
            }
            let Ok(bytes) = fs::read(&path) else {
                skipped += 1;
                continue;
            };
            let Ok(content) = String::from_utf8(bytes) else {
                skipped += 1;
                continue;
            };
            files.push(autopsy_test_impact::TestDiscovery {
                path: relative,
                content,
            });
        }
    }

    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok((files, skipped))
}

// ---------------------------------------------------------------------------
// temporal
// ---------------------------------------------------------------------------

/// Replays commit history into a `TemporalJournal`, or reconstructs one revision of it.
pub fn temporal(
    cli: &Cli,
    at: Option<&str>,
    max_commits: usize,
    strict: bool,
) -> Result<Outcome, CliError> {
    let directory = git_dir(&cli.repo_root)?;
    let mut store = autopsy_temporal::git::GitObjectStore::open(&directory)
        .map_err(|error| CliError::Analysis(format!("{}: {error}", directory.display())))?;
    let head = store
        .resolve_ref("HEAD")
        .map_err(|error| CliError::Analysis(format!("HEAD could not be resolved: {error}")))?;
    // A repository with no commits is not an analysis failure: it has no history to replay,
    // which is `4` rather than `3`.
    let Some(head) = head else {
        return Ok(Outcome {
            exit_code: EXIT_UNSUPPORTED,
            json: "{\n  \"journal\": null\n}".to_string(),
            text: "temporal: no commits (HEAD does not resolve), so there is no history to \
                   replay\n"
                .to_string(),
        });
    };

    let journal = match autopsy_temporal::TemporalJournal::build(
        &store,
        head,
        max_commits,
        &autopsy_temporal::TreeExtractor::default(),
    ) {
        Ok(journal) => journal,
        // The walker **refuses** a history that does not fit rather than truncating it: a
        // truncated timeline is not a timeline, and silently dropping the oldest commits
        // would move every `active at revision` answer. That refusal is `4` — the analysis
        // is not wrong, it is unavailable at this bound — and the message says what to pass
        // instead of leaving a bare error.
        Err(autopsy_temporal::TemporalError::TimelineTooLarge(bound)) => {
            return Ok(Outcome {
                exit_code: EXIT_UNSUPPORTED,
                json: "{\n  \"journal\": null\n}".to_string(),
                text: format!(
                    "temporal: the history does not fit in a bound of {bound} commit(s), and the \\\n                     walker refuses to truncate a timeline silently. Pass --max-commits at least \\\n                     as large as the reachable history.\n"
                ),
            });
        }
        Err(error) => {
            return Err(CliError::Analysis(format!(
                "history could not be replayed: {error}"
            )));
        }
    };

    if let Some(revision) = at {
        let snapshot = journal
            .reconstruct(revision)
            .map_err(|error| CliError::Analysis(format!("{revision}: {error}")))?;
        let exit_code = coverage_exit(strict, CoverageState::Verified);
        let text = format!(
            "temporal at {revision}: {} entities, {} edges, digest {}\n",
            snapshot.nodes.len(),
            snapshot.edges.len(),
            snapshot.digest
        );
        return Outcome::new(exit_code, &snapshot, text);
    }

    let stats = journal.stats();
    // The journal's own signal: a fact it could not fully resolve is `partial`, one it could
    // not resolve at all is `unknown`. That is the coverage state — not a blanket `verified`,
    // which would let `--strict` pass over exactly the facts the engine flagged.
    let journal_coverage = if stats.unknown > 0 {
        CoverageState::Unknown
    } else if stats.partial > 0 {
        CoverageState::Partial
    } else {
        CoverageState::Verified
    };
    let exit_code = coverage_exit(strict, journal_coverage);
    let notes = vec![
        "facts come from `TreeExtractor`, which records file presence per revision; \
         symbol-level extraction is not wired to the language adapters, so this journal says \
         which files existed, not which symbols were defined"
            .to_string(),
    ];
    let text = format!(
        "temporal: {} facts over {} commits ({} active, {} retired, {} partial, {} unknown)\n\
         note: facts come from `TreeExtractor`, which records file presence per revision. \
         Symbol-level extraction is not wired to the language adapters, so this journal says \
         which files existed, not which symbols were defined.\n",
        stats.total,
        journal.timeline().len(),
        stats.active,
        stats.retired,
        stats.partial,
        stats.unknown
    );
    // The journal's own canonical projection is embedded **verbatim** under `journal`, so the
    // engine's digest stays checkable; `stats` and `notes` are the CLI's additions and are
    // named as such rather than merged into the engine's value.
    let output = TemporalOutput {
        journal: journal_to_json(&journal),
        stats,
        coverage: journal_coverage,
        notes,
    };
    Outcome::new(exit_code, &output, text)
}

/// The CLI's temporal document: the engine's projection plus the two facts the CLI adds.
#[derive(Debug, serde::Serialize)]
struct TemporalOutput {
    /// `TemporalJournal::to_canonical_json`, re-parsed so it can be embedded as data.
    journal: serde_json::Value,
    stats: autopsy_temporal::JournalStats,
    /// Derived from the fact counts: `unknown` facts make it `unknown`, `partial` facts make
    /// it `partial`, and only a journal whose facts all resolved is `verified`.
    coverage: CoverageState,
    notes: Vec<String>,
}

/// The journal's own canonical JSON, re-parsed so it can be embedded in an outcome.
fn journal_to_json(journal: &autopsy_temporal::TemporalJournal) -> serde_json::Value {
    journal
        .to_canonical_json()
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(serde_json::Value::Null)
}

// ---------------------------------------------------------------------------
// test-select
// ---------------------------------------------------------------------------

/// Selects the tests that must run for a set of changed files.
///
/// The change set is **file-level**: the CLI knows which files changed and does not resolve
/// them to symbols, so `ChangeSetSummary::coverage` is `Partial` and the plan inherits it.
/// Claiming `Verified` here would say \"no symbols changed\" when the truth is \"symbols were
/// not looked at\", and those are different statements with different consequences.
pub fn test_select(
    cli: &Cli,
    changed: &[String],
    lcov_paths: &[PathBuf],
    fail_if_all_tests_required: bool,
    strict: bool,
) -> Result<Outcome, CliError> {
    let (files, walk_skipped) = discover_test_files(&cli.repo_root)?;
    let inventory = autopsy_test_impact::discover_inventory(&files);

    let mut matrices = Vec::new();
    for path in lcov_paths {
        let content = read_text(path)?;
        let suite_id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("coverage")
            .to_string();
        matrices.push(
            autopsy_test_impact::parse_lcov(&suite_id, &content)
                .map_err(|error| CliError::Analysis(format!("{}: {error}", path.display())))?,
        );
    }
    let coverage = autopsy_test_impact::merge_matrices(matrices);

    let mut changed_files: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for path in changed {
        // Normalised to the POSIX-relative form the inventory uses, so a path handed in as
        // `./src/a.ts` or `src\\a.ts` is the same file as `src/a.ts`.
        let normalised = path.replace('\\', "/");
        let trimmed = normalised.trim_start_matches("./").to_string();
        changed_files.insert(trimmed);
    }

    let change_set = autopsy_test_impact::ChangeSetSummary {
        changed_files,
        changed_symbols: std::collections::BTreeSet::new(),
        unknown_symbols: std::collections::BTreeSet::new(),
        coverage: CoverageState::Partial,
    };

    // `NoReachability` is the engine's own identity implementation: it cannot widen the seed
    // set beyond what was named, so the selection is conservative by construction.
    let plan = autopsy_test_impact::select_affected_tests(
        &change_set,
        &inventory,
        &coverage,
        &autopsy_test_impact::NoReachability,
    )
    .map_err(|error| CliError::Analysis(error.to_string()))?;

    let all_required = matches!(
        plan.fallback,
        autopsy_test_impact::SelectionFallback::RunAllTests { .. }
    );
    let mut exit_code = if fail_if_all_tests_required && all_required {
        EXIT_POLICY_FAIL
    } else {
        coverage_exit(strict, plan.coverage)
    };
    // A policy gate that fires outranks a coverage warning: the caller asked a policy
    // question and the answer is no.
    if exit_code == EXIT_PASS && strict && plan.coverage == CoverageState::Partial {
        exit_code = EXIT_UNSUPPORTED;
    }

    let fallback = match &plan.fallback {
        autopsy_test_impact::SelectionFallback::None => "none".to_string(),
        autopsy_test_impact::SelectionFallback::RunPackageTests { packages, .. } => {
            format!("run-package-tests({})", packages.join(","))
        }
        autopsy_test_impact::SelectionFallback::RunAllTests { .. } => "run-all-tests".to_string(),
    };
    let mut text = format!(
        "test-select: {} of {} suite(s), {} of {} test(s) selected (fallback: {fallback}, coverage: {:?})\n",
        plan.selected_suites.len(),
        plan.total_suites,
        plan.selected_tests.len(),
        plan.total_tests,
        plan.coverage
    );
    text.push_str(
        "note: the change set is file-level; changed symbols are not resolved by this command, \
         so the plan is `partial` by construction. Reachability is `NoReachability`, which \
         cannot widen the seed set.\n",
    );
    if walk_skipped > 0 {
        text.push_str(&format!(
            "note: {walk_skipped} entr(y/ies) were skipped by the bounded walk (symlinks, \
             non-UTF-8 files, depth or file bounds).\n"
        ));
    }
    for reason in &plan.reasons {
        text.push_str(&format!("  - {reason:?}\n"));
    }
    if all_required {
        text.push_str("note: everything must run; no narrowing could be justified.\n");
    }
    Outcome::new(exit_code, &plan, text)
}

// ---------------------------------------------------------------------------
// contract-dist
// ---------------------------------------------------------------------------

/// Diffs two contract documents, or verifies one AsyncAPI document's internal flows, or
/// cross-references a Pact against a provider.
pub fn contract_dist(
    from: &Path,
    to: &Path,
    protocol: ContractProtocol,
    pact: Option<&Path>,
    strict: bool,
) -> Result<Outcome, CliError> {
    use autopsy_contracts_distributed::{CompatibilityVerdict, SpecDocument};

    let parse = |path: &Path| -> Result<SpecDocument, CliError> {
        let content = read_text(path)?;
        let name = path.display().to_string();
        SpecDocument::parse(&name, &content, document_format(path))
            .map_err(|error| CliError::Analysis(format!("{name}: {error}")))
    };

    let delta = match protocol {
        ContractProtocol::Openapi => {
            let before = autopsy_contracts_distributed::parse_openapi(&parse(from)?)
                .map_err(|error| CliError::Analysis(format!("{}: {error}", from.display())))?;
            let after = autopsy_contracts_distributed::parse_openapi(&parse(to)?)
                .map_err(|error| CliError::Analysis(format!("{}: {error}", to.display())))?;
            autopsy_contracts_distributed::diff_openapi(&before, &after)
        }
        ContractProtocol::Asyncapi => {
            let before = autopsy_contracts_distributed::parse_asyncapi(&parse(from)?)
                .map_err(|error| CliError::Analysis(format!("{}: {error}", from.display())))?;
            let after = autopsy_contracts_distributed::parse_asyncapi(&parse(to)?)
                .map_err(|error| CliError::Analysis(format!("{}: {error}", to.display())))?;
            autopsy_contracts_distributed::diff_asyncapi(&before, &after)
        }
        ContractProtocol::Pact => {
            let pact_path = pact.unwrap_or(from);
            let document = autopsy_contracts_distributed::parse_pact(&parse(pact_path)?)
                .map_err(|error| CliError::Analysis(format!("{}: {error}", pact_path.display())))?;
            let provider = autopsy_contracts_distributed::parse_openapi(&parse(to)?)
                .map_err(|error| CliError::Analysis(format!("{}: {error}", to.display())))?;
            autopsy_contracts_distributed::xref_pact_against_provider(&document, &provider)
        }
        ContractProtocol::AsyncapiFlows => {
            let document = autopsy_contracts_distributed::parse_asyncapi(&parse(from)?)
                .map_err(|error| CliError::Analysis(format!("{}: {error}", from.display())))?;
            let report = autopsy_contracts_distributed::verify_flows(&document);
            let exit_code = if strict && report.coverage == CoverageState::Partial {
                EXIT_UNSUPPORTED
            } else {
                coverage_exit(strict, report.coverage)
            };
            let mut text = format!(
                "contract-dist (asyncapi flows): {} checked, {} unchecked, coverage {:?}\n",
                report.checked.len(),
                report.unchecked.len(),
                report.coverage
            );
            for note in &report.notes {
                text.push_str(&format!("  - {note}\n"));
            }
            return Outcome::new(exit_code, &report, text);
        }
    };

    let exit_code = match delta.verdict {
        // `Breaking` is a policy failure even without `--strict`: the engine has positive
        // evidence that a consumer can break, and exiting `0` would bury it in a pipeline.
        CompatibilityVerdict::Breaking => EXIT_POLICY_FAIL,
        CompatibilityVerdict::Unknown if strict => EXIT_UNSUPPORTED,
        CompatibilityVerdict::Unknown => coverage_exit(strict, delta.coverage),
        CompatibilityVerdict::Compatible => coverage_exit(strict, delta.coverage),
    };

    let breaking = delta
        .changes
        .iter()
        .filter(|change| change.severity == autopsy_contracts_distributed::Severity::Breaking)
        .count();
    let mut text = format!(
        "contract-dist ({}): {:?} — {} change(s), {breaking} breaking, coverage {:?}\n\
         from: {} ({})\nto:   {} ({})\ndigest: {}\n",
        delta.protocol,
        delta.verdict,
        delta.changes.len(),
        delta.coverage,
        delta.from_path,
        delta.from_digest,
        delta.to_path,
        delta.to_digest,
        delta.digest
    );
    for change in delta.changes.iter().take(50) {
        text.push_str(&format!(
            "  [{}] {} — {}\n",
            match change.severity {
                autopsy_contracts_distributed::Severity::Breaking => "BREAKING",
                autopsy_contracts_distributed::Severity::Unknown => "UNKNOWN",
                autopsy_contracts_distributed::Severity::NonBreaking => "ok",
            },
            change.subject,
            change.detail
        ));
    }
    if delta.changes.len() > 50 {
        text.push_str(&format!(
            "  … {} more change(s) in the JSON report\n",
            delta.changes.len() - 50
        ));
    }
    for note in &delta.notes {
        text.push_str(&format!("  note: {note}\n"));
    }
    Outcome::new(exit_code, &delta, text)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum ContractProtocol {
    Openapi,
    Asyncapi,
    AsyncapiFlows,
    Pact,
}

// ---------------------------------------------------------------------------
// telemetry
// ---------------------------------------------------------------------------

/// Everything `telemetry` can be asked for, as one value.
///
/// Eight positional booleans and paths is a call nobody can read at the call site, and two
/// adjacent `bool`s are a transposition waiting to happen: `--complete-observation` and
/// `--dispatch-static` look alike and are not.
#[derive(Debug, Clone)]
pub struct TelemetryArgs {
    /// The pseudonymisation salt as 64 hex characters.
    pub salt_hex: Option<String>,
    /// A file containing the salt as 64 hex characters.
    pub salt_file: Option<PathBuf>,
    /// A declared-surface JSON document; when present, the topology report is produced.
    pub surface: Option<PathBuf>,
    /// Minimum observation windows a dead-code claim needs.
    pub windows: u32,
    pub complete_observation: bool,
    pub dispatch_static: bool,
    pub strict: bool,
}

impl Default for TelemetryArgs {
    /// The conservative reading: three windows, an observation that may be sampled, and
    /// dispatch that may be dynamic. No salt — a default salt is the one thing this struct
    /// must never have.
    fn default() -> Self {
        Self {
            salt_hex: None,
            salt_file: None,
            surface: None,
            windows: 3,
            complete_observation: false,
            dispatch_static: false,
            strict: false,
        }
    }
}

/// Ingests an OTLP/JSON trace export and, optionally, correlates it against a declared
/// surface to produce a topology report.
///
/// The salt is **required** and has no default. A default salt would make every pseudonym in
/// every report recomputable from the source, which is the one thing the pseudonymisation
/// exists to prevent.
pub fn telemetry(input: &Path, args: &TelemetryArgs) -> Result<Outcome, CliError> {
    use autopsy_telemetry::{DispatchModel, IngestOptions, TopologyOptions};

    let salt = match (args.salt_hex.as_deref(), args.salt_file.as_deref()) {
        (Some(hex), None) => decode_salt(hex)?,
        (None, Some(path)) => decode_salt(read_text(path)?.trim())?,
        (Some(_), Some(_)) => {
            return Err(CliError::Config(
                "pass either --salt or --salt-file, not both".to_string(),
            ));
        }
        (None, None) => {
            return Err(CliError::Config(
                "a 32-byte salt is required (--salt <64 hex chars> or --salt-file <path>). There \
                 is no default: a built-in salt would make every pseudonym in every report \
                 recomputable from the source."
                    .to_string(),
            ));
        }
    };

    let content = read_text(input)?;
    let request = autopsy_telemetry::parse_otlp_traces(&input.display().to_string(), &content)
        .map_err(|error| CliError::Analysis(error.to_string()))?;
    let options = IngestOptions::with_salt(salt);
    let record = autopsy_telemetry::sanitize_traces(&request, &options)
        .map_err(|error| CliError::Analysis(error.to_string()))?;

    let Some(surface_path) = args.surface.as_deref() else {
        let exit_code = coverage_exit(args.strict, record.coverage);
        let text = format!(
            "telemetry: {} span(s), {} redaction(s), coverage {:?}\ndigest: {}\n\
             note: every identifier in the report is a keyed pseudonym; the salt is not \
             recorded, so the mapping cannot be recovered from the report alone.\n",
            record.span_count,
            record.redaction_totals.values().sum::<usize>(),
            record.coverage,
            record.digest
        );
        return Outcome::new(exit_code, &record, text);
    };

    let surface_text = read_text(surface_path)?;
    let declared: autopsy_telemetry::DeclaredSurface = serde_json::from_str(&surface_text)
        .map_err(|error| {
            CliError::Analysis(format!(
                "{}: declared surface is not valid JSON: {error}",
                surface_path.display()
            ))
        })?;
    let Some(required_windows) = std::num::NonZeroU32::new(args.windows) else {
        return Err(CliError::Config("--windows must be at least 1".to_string()));
    };
    // Built through the constructor rather than a struct literal: the fields are private, so
    // the conservative default (sampled exports, dynamic dispatch) is opt-*out* of explicitly.
    let topology_options = TopologyOptions::new(required_windows)
        .with_complete_observation(args.complete_observation)
        .with_dispatch_model(if args.dispatch_static {
            DispatchModel::Static
        } else {
            DispatchModel::Dynamic
        });
    let report = autopsy_telemetry::correlate_topology(&[record], &declared, &topology_options);

    let exit_code = if args.strict && report.coverage == CoverageState::Partial {
        EXIT_UNSUPPORTED
    } else {
        coverage_exit(args.strict, report.coverage)
    };
    let mut text = format!(
        "telemetry (topology): {} span(s) over {} window(s) ({} required), {} dead-code \
         claim(s), {} undocumented route(s), {} uninvoked route(s), coverage {:?}\n\
         note: a dead-code finding is a claim about a *bounded* observation. It is `verified` \
         only when the caller asserted a complete observation and a static dispatch model; \
         `insufficient_evidence` lists what the observation cannot establish.\ndigest: {}\n",
        report.observation.span_count,
        report.observation.windows_observed,
        report.observation.required_windows,
        report.dead_code.len(),
        report.undocumented_routes.len(),
        report.uninvoked_routes.len(),
        report.coverage,
        report.digest
    );
    for finding in &report.insufficient_evidence {
        text.push_str(&format!("  insufficient evidence: {finding:?}\n"));
    }
    for note in &report.notes {
        text.push_str(&format!("  note: {note}\n"));
    }
    Outcome::new(exit_code, &report, text)
}

/// 64 hex characters, or a 32-byte file. Anything else is refused rather than padded: a salt
/// that is not the length the caller meant is a salt that silently weakens every pseudonym.
fn decode_salt(text: &str) -> Result<[u8; 32], CliError> {
    let trimmed = text.trim();
    if trimmed.len() != 64 {
        return Err(CliError::Config(format!(
            "a salt must be exactly 64 hex characters (32 bytes); got {} character(s)",
            trimmed.len()
        )));
    }
    let mut salt = [0u8; 32];
    for (index, byte) in salt.iter_mut().enumerate() {
        let pair = &trimmed[index * 2..index * 2 + 2];
        *byte = u8::from_str_radix(pair, 16).map_err(|_| {
            CliError::Config(format!(
                "a salt must be hex; `{pair}` at offset {} is not",
                index * 2
            ))
        })?;
    }
    Ok(salt)
}

// ---------------------------------------------------------------------------
// cochange
// ---------------------------------------------------------------------------

/// Mines the repository's recent history for paths that change together.
pub fn cochange(
    repo_root: &Path,
    window_seconds: i64,
    max_commits: usize,
    max_files_per_commit: usize,
    min_pair_commits: u32,
    strict: bool,
) -> Result<Outcome, CliError> {
    use autopsy_mining::{CoChangeOptions, HistoryOptions};

    let directory = git_dir(repo_root)?;
    let mut store = autopsy_temporal::git::GitObjectStore::open(&directory)
        .map_err(|error| CliError::Analysis(format!("{}: {error}", directory.display())))?;
    // A repository with no commits cannot be mined. That is `4` — there is no population to
    // measure — and not `3`, which would say the analysis failed.
    if store
        .resolve_ref("HEAD")
        .map_err(|error| CliError::Analysis(format!("HEAD could not be resolved: {error}")))?
        .is_none()
    {
        return Ok(Outcome {
            exit_code: EXIT_UNSUPPORTED,
            json: "{\n  \"cochange\": null\n}".to_string(),
            text: "cochange: no commits (HEAD does not resolve), so there is no window to \n\
                   mine\n"
                .to_string(),
        });
    }

    let history = HistoryOptions::new()
        .with_window_seconds(window_seconds)
        .with_max_commits(max_commits)
        .with_max_files_per_commit(max_files_per_commit);
    let options = CoChangeOptions::new().with_min_pair_commits(min_pair_commits);

    let report = autopsy_mining::mine_repository(&mut store, &history, &options)
        .map_err(|error| CliError::Analysis(format!("history could not be mined: {error}")))?;

    let exit_code = if strict && report.coverage == CoverageState::Partial {
        EXIT_UNSUPPORTED
    } else {
        coverage_exit(strict, report.coverage)
    };
    let mut text = format!(
        "cochange: {} edge(s) over {} considered commit(s) of {} seen, coverage {:?}\n",
        report.edges.len(),
        report.window.commits_considered,
        report.window.commits_seen,
        report.coverage
    );
    for (reason, count) in &report.window.excluded {
        text.push_str(&format!("  excluded {count} × {reason:?}\n"));
    }
    text.push_str(
        "note: every edge is tagged `temporal_co_occurrence_only`. This is not a dependency \
         graph and there is no threshold at which it becomes one.\n",
    );
    for note in &report.notes {
        text.push_str(&format!("  note: {note}\n"));
    }
    if report.edges.len() > 20 {
        text.push_str(&format!(
            "  ({} more edge(s) in the JSON report)\n",
            report.edges.len() - 20
        ));
    }
    for edge in report.edges.iter().take(20) {
        text.push_str(&format!(
            "  {} .. {} together={} confidence a→b={} ppm lift={} ppm [{:?}]\n",
            edge.a,
            edge.b,
            edge.commits_together,
            edge.confidence_a_to_b_ppm,
            edge.lift_ppm,
            edge.coverage
        ));
    }
    Outcome::new(exit_code, &report, text)
}

// ---------------------------------------------------------------------------
// catalog
// ---------------------------------------------------------------------------

/// Scans a repository tree for catalog entities, ownership and dependency manifests.
pub fn catalog(
    cli: &Cli,
    root: Option<&Path>,
    max_file_bytes: Option<u64>,
    strict: bool,
) -> Result<Outcome, CliError> {
    use autopsy_catalog::DiscoveryOptions;

    let root = root.unwrap_or(&cli.repo_root);
    let mut options = DiscoveryOptions::new();
    if let Some(max) = max_file_bytes {
        options = options.with_max_file_bytes(max);
    }
    let report = autopsy_catalog::scan(root, &options)
        .map_err(|error| CliError::Analysis(format!("catalog scan failed: {error}")))?;

    let exit_code = if strict && report.coverage == CoverageState::Partial {
        EXIT_UNSUPPORTED
    } else {
        coverage_exit(strict, report.coverage)
    };
    let mut text = format!(
        "catalog: {} file(s) scanned, {} entit(y/ies), {} manifest(s), {} dependenc(y/ies), \
         coverage {:?}\n",
        report.files_scanned,
        report.entities.len(),
        report.manifests.len(),
        report.dependencies.len(),
        report.coverage
    );
    for (owner, owned) in &report.ownership.by_owner {
        text.push_str(&format!("  {owner} owns {}\n", owned.join(", ")));
    }
    for note in &report.notes {
        text.push_str(&format!("  note: {note}\n"));
    }
    Outcome::new(exit_code, &report, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_salt_is_exactly_thirty_two_bytes() {
        assert!(decode_salt(&"ab".repeat(32)).is_ok());
        assert!(decode_salt(&"ab".repeat(31)).is_err(), "31 bytes");
        assert!(decode_salt(&"ab".repeat(33)).is_err(), "33 bytes");
        assert!(decode_salt(&"zz".repeat(32)).is_err(), "not hex");
        // The error must say what was wrong, not merely that something was.
        let error = decode_salt("abcd").expect_err("short").to_string();
        assert!(error.contains("exactly 64 hex characters"), "{error}");
    }

    #[test]
    fn the_coverage_rule_never_turns_unknown_into_pass() {
        for strict in [false, true] {
            assert_eq!(
                coverage_exit(strict, CoverageState::Unknown),
                EXIT_UNSUPPORTED
            );
            assert_eq!(
                coverage_exit(strict, CoverageState::Unsupported),
                EXIT_UNSUPPORTED
            );
            assert_eq!(coverage_exit(strict, CoverageState::Verified), EXIT_PASS);
            assert_eq!(
                coverage_exit(strict, CoverageState::Partial),
                if strict { EXIT_UNSUPPORTED } else { EXIT_PASS }
            );
        }
    }
}
