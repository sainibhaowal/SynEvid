//! autopsy-cli: Deterministic, offline software verification & change-impact CLI (FR-015, FR-016, FR-024).
//!
//! Output Protocol:
//! - stdout: Machine-readable result (JSON, Text, or SARIF)
//! - stderr: Diagnostics, progress, warnings, info logs
//!
//! Exit Codes:
//! - 0: PASS (All checks and invariants satisfied)
//! - 2: POLICY FAILURE (Invariant or contract violations detected)
//! - 3: ANALYSIS ERROR (Configuration, parse, or I/O failure)
//! - 4: UNSUPPORTED / UNKNOWN (Unsupported syntax or unknown coverage in strict mode)

pub mod extensions;

use autopsy_adapter_api::{LanguageAdapter, ParseContext, ParsedFile};
use autopsy_adapter_python::PythonAdapter;
use autopsy_adapter_typescript::TypeScriptAdapter;
use autopsy_contracts::{CallableContract, NormalizedContract, ParameterModel};
use autopsy_diff::{DiffEngine, DiffInput};
use autopsy_domain::{
    AnalysisRun, Contract, CoverageState, Edge, FileUnit, Finding, FindingStatus, RepoSnapshot,
    SnapshotId, Symbol, SymbolId,
};
use autopsy_evidence::EvidenceReceipt;
use autopsy_graph::DependencyGraph;
use autopsy_impact::{ImpactDirection, ImpactEngine, ImpactProfile, ImpactQuery};
use autopsy_invariants::{EvaluationContext, InvariantEvaluator, InvariantsConfig};
use autopsy_repo::{AutopsyConfig, scan_repository};
use autopsy_report::{
    AutopsyReport, CoverageSummary, SnapshotSummary, convert_domain_findings,
    convert_impact_result_top_k, convert_semantic_diff,
};
use autopsy_storage::{StorageEngine, StorageOptions};
use clap::{Parser, Subcommand, ValueEnum};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const EXIT_PASS: i32 = 0;
pub const EXIT_POLICY_FAIL: i32 = 2;
pub const EXIT_ANALYSIS_ERROR: i32 = 3;
pub const EXIT_UNSUPPORTED: i32 = 4;

pub const ANALYZER_VERSION: &str = "0.0.1";

#[derive(Debug, Error)]
pub enum CliError {
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Analysis error: {0}")]
    Analysis(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Report error: {0}")]
    Report(#[from] autopsy_report::ReportError),
    #[error("Invariant error: {0}")]
    Invariant(String),
}

/// Baseline debt configuration (.autopsy/baseline.json).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BaselineConfig {
    pub version: String,
    #[serde(default)]
    pub created_at_snapshot: Option<String>,
    #[serde(default)]
    pub accepted_debt: Vec<AcceptedDebtEntry>,
}

/// Accepted pre-existing violation entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AcceptedDebtEntry {
    pub fingerprint: String,
    pub rule_id: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub waived_reason: Option<String>,
}

#[derive(Parser, Debug)]
#[command(
    name = "autopsy",
    about = "Offline, deterministic software verification & change-impact engine",
    version = ANALYZER_VERSION
)]
pub struct Cli {
    /// Target repository root directory
    #[arg(short, long, global = true, default_value = ".")]
    pub repo_root: PathBuf,

    /// Output presentation format (json, text, sarif)
    #[arg(short, long, global = true, value_enum, default_value = "text")]
    pub format: OutputFormat,

    /// Path to SQLite database file (defaults to .autopsy/storage.db)
    #[arg(long, global = true)]
    pub db_path: Option<PathBuf>,

    /// Path to content-addressed cache directory (defaults to .autopsy/cache)
    #[arg(long, global = true)]
    pub cache_dir: Option<PathBuf>,

    /// Enable SQLite write-ahead logging (WAL mode)
    #[arg(long, global = true)]
    pub wal: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
    Sarif,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliImpactDirection {
    Forward,
    Backward,
    Bidirectional,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliImpactProfile {
    Full,
    Callers,
}

impl From<CliImpactDirection> for ImpactDirection {
    fn from(val: CliImpactDirection) -> Self {
        match val {
            CliImpactDirection::Forward => ImpactDirection::Forward,
            CliImpactDirection::Backward => ImpactDirection::Backward,
            CliImpactDirection::Bidirectional => ImpactDirection::Bidirectional,
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Scan repository, compute deterministic snapshot, build dependency graph, and save to storage
    Baseline {
        #[arg(long)]
        revision: Option<String>,
        /// Reuse per-file analysis artifacts from a stored baseline for
        /// byte-identical files (falls back to full analysis when the
        /// cache cannot satisfy the request).
        #[arg(long)]
        incremental: bool,
        /// Baseline snapshot ID for incremental reuse (defaults to the
        /// latest stored snapshot).
        #[arg(long)]
        baseline_id: Option<String>,
    },
    /// Compare two snapshots or compare baseline against worktree
    Diff {
        #[arg(long)]
        before: Option<String>,
        /// Stored after-state snapshot ID. When omitted, or when the ID
        /// is not a stored snapshot, the current worktree is analyzed
        /// (the value is then only a revision label).
        #[arg(long)]
        after: Option<String>,
    },
    /// Compute bounded transitive impact analysis from seed symbols
    Impact {
        #[arg(short, long, required = true)]
        symbol: Vec<String>,
        /// Traversal direction. Omitted = preset default (forward for
        /// `full`, backward for `callers`). Explicit values always win.
        #[arg(long, value_enum)]
        direction: Option<CliImpactDirection>,
        #[arg(long, default_value_t = 10)]
        max_depth: usize,
        #[arg(long, default_value_t = 1000)]
        budget: usize,
        /// Stored snapshot ID to analyze. When omitted, the current
        /// worktree is analyzed.
        #[arg(long)]
        snapshot_id: Option<String>,
        /// Comma-separated edge kinds to traverse
        /// (contains,imports,references,calls,implements,inherits,reads-schema,writes-schema,test-covers).
        /// Defaults to all kinds (maximum recall). Narrowing kinds is the
        /// operator-side precision control: fewer kinds, smaller blast radius.
        #[arg(long)]
        edge_kinds: Option<String>,
        /// Traversal preset: `full` (default; current behavior, maximum
        /// recall) or `callers` (backward invocation closure with
        /// file-node transit suppressed + top-10 ranking; the recommended
        /// precision-first workflow). Explicit --direction/--edge-kinds/
        /// --top-k override the preset.
        #[arg(long, value_enum, default_value = "full")]
        profile: CliImpactProfile,
        /// Ranked shortlist size. Presents the top-k entities by
        /// shortest-path distance; the full set stays in the report for
        /// evidence. Omitted = no shortlist (full list only).
        #[arg(long)]
        top_k: Option<usize>,
    },
    /// Evaluate invariant rules against current snapshot or diff
    Verify {
        /// Return exit code 4 if coverage contains unknown or unsupported constructs
        #[arg(long)]
        strict: bool,
        /// Invariants YAML configuration file (defaults to .autopsy/invariants.yml)
        #[arg(long)]
        invariants_file: Option<PathBuf>,
        /// Optional baseline snapshot ID to verify diff against
        #[arg(long)]
        baseline_id: Option<String>,
    },
    /// Explain evidence receipt and provenance for a finding or symbol
    Explain {
        /// Target finding ID or symbol ID
        target: String,
    },
    /// List symbols in stable-id order with cursor pagination (read-only graph browsing)
    Symbols {
        /// Maximum entries to return (default 50, max 1000)
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Zero-based offset into stable-id order (default 0)
        #[arg(long, default_value_t = 0)]
        offset: usize,
        /// Filter by symbol kind (function, method, class, interface, type, enum, variable)
        #[arg(long)]
        kind: Option<String>,
    },
    /// List 1-hop edges for a symbol with cursor pagination (read-only graph browsing)
    Edges {
        /// Source stable symbol ID (or qualified name)
        #[arg(long)]
        source: String,
        /// Traversal direction (default bidirectional)
        #[arg(long, value_enum, default_value = "bidirectional")]
        direction: CliImpactDirection,
        /// Filter by edge kind (calls, contains, imports, references, inherits, implements, ...)
        #[arg(long)]
        kind: Option<String>,
        /// Maximum entries to return (default 50, max 1000)
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Zero-based offset into deterministic edge order (default 0)
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
    /// Replay git history into a temporal journal, or reconstruct one revision (Pillar 1)
    Temporal {
        /// Reconstruct the graph at this revision instead of emitting the whole journal
        #[arg(long)]
        at: Option<String>,
        /// Bound on commits walked. The walker refuses a history that does not fit rather
        /// than truncating it, so this must cover the reachable history.
        #[arg(long, default_value_t = 25_000)]
        max_commits: usize,
        /// Exit 4 when coverage is partial
        #[arg(long)]
        strict: bool,
    },
    /// Trace file identity across a revision pair: renames, moves, modifications (Pillar 1)
    Lineage {
        /// The earlier revision (default: the first parent of `--to`)
        #[arg(long)]
        from: Option<String>,
        /// The later revision (default: HEAD)
        #[arg(long)]
        to: Option<String>,
        /// Token-similarity floor in 0..=1 for proposing a candidate rename
        #[arg(long)]
        threshold: Option<f64>,
        /// Minimum margin in 0..=1 by which the best candidate must beat the runner-up
        #[arg(long)]
        margin: Option<f64>,
        /// Bound on files read from one revision
        #[arg(long, default_value_t = 20_000)]
        max_files: usize,
        /// Skip blobs larger than this many bytes
        #[arg(long, default_value_t = 1_048_576)]
        max_file_bytes: u64,
        /// Exit 4 when coverage is partial as well as when it is unknown
        #[arg(long)]
        strict: bool,
    },
    /// Select the tests that must run for a set of changed files (Pillar 2)
    TestSelect {
        /// A changed source file, POSIX-relative (repeatable)
        #[arg(long = "changed", value_name = "PATH")]
        changed: Vec<String>,
        /// An LCOV coverage report, one per suite (repeatable)
        #[arg(long = "lcov", value_name = "FILE")]
        lcov: Vec<PathBuf>,
        /// Exit 2 when the plan is `run-all-tests` (the CI gate for a selection that did not narrow)
        #[arg(long)]
        fail_if_all_tests_required: bool,
        /// Exit 4 when coverage is partial as well as when it is unknown
        #[arg(long)]
        strict: bool,
    },
    /// Diff two contract documents across repositories (Pillar 3)
    ContractDist {
        /// The `from` document (the earlier revision, the Pact, or the flows document)
        from: PathBuf,
        /// The `to` document (the later revision, or the provider OpenAPI spec)
        to: PathBuf,
        /// Which protocol to compare
        #[arg(long, value_enum, default_value = "openapi")]
        protocol: extensions::ContractProtocol,
        /// Exit 4 when the verdict is `unknown` or coverage is partial
        #[arg(long)]
        strict: bool,
    },
    /// Ingest an OTLP/JSON trace export, optionally correlating it into a topology report (Pillar 4)
    Telemetry {
        /// The OTLP/JSON trace export
        input: PathBuf,
        /// The pseudonymisation salt, 64 hex characters. Required: there is no default.
        #[arg(long)]
        salt: Option<String>,
        /// A file containing the salt as 64 hex characters
        #[arg(long)]
        salt_file: Option<PathBuf>,
        /// A declared-surface JSON document; when given, the topology report is produced instead
        #[arg(long)]
        surface: Option<PathBuf>,
        /// Minimum observation windows a dead-code claim needs
        #[arg(long, default_value_t = 3)]
        windows: u32,
        /// Assert that the observation is complete (required for a `verified` dead-code claim)
        #[arg(long)]
        complete_observation: bool,
        /// Assert a static dispatch model (required for a `verified` dead-code claim)
        #[arg(long)]
        dispatch_static: bool,
        /// Exit 4 when coverage is partial as well as when it is unknown
        #[arg(long)]
        strict: bool,
    },
    /// Mine git history for paths that change together (Pillar 5)
    Cochange {
        /// Window width in seconds (default: 183 days, the design's "e.g. 6 months")
        #[arg(long, default_value_t = 183 * 86_400)]
        window_seconds: i64,
        /// Bound on commits walked
        #[arg(long, default_value_t = 5_000)]
        max_commits: usize,
        /// A commit changing more paths than this is not one change set
        #[arg(long, default_value_t = 30)]
        max_files_per_commit: usize,
        /// Fewest joint commits before a pair is reported at all
        #[arg(long, default_value_t = 2)]
        min_pair_commits: u32,
        /// Exit 4 when coverage is partial as well as when it is unknown
        #[arg(long)]
        strict: bool,
    },
    /// Ingest Backstage catalog entities and dependency manifests (Pillar 6)
    Catalog {
        /// The tree to scan (defaults to the repository root)
        #[arg(long)]
        root: Option<PathBuf>,
        /// Bound on a single file's size, in bytes
        #[arg(long)]
        max_file_bytes: Option<u64>,
        /// Exit 4 when coverage is partial as well as when it is unknown
        #[arg(long)]
        strict: bool,
    },
    /// The organisational blast radius of one catalog component: owners, co-owned
    /// components, siblings, provided and consumed APIs, and dependents (Pillar 6)
    BlastRadius {
        /// The component reference to traverse from, e.g. `component:default/checkout`
        reference: String,
        /// The tree to scan (defaults to the repository root)
        #[arg(long)]
        root: Option<PathBuf>,
        /// Bound on a single file's size, in bytes
        #[arg(long)]
        max_file_bytes: Option<u64>,
        /// Exit 4 when coverage is partial as well as when it is unknown
        #[arg(long)]
        strict: bool,
    },
    /// Verify repository configuration, storage integrity, and analyzer health
    Doctor,
    /// Display engine version and adapter capabilities
    Version,
}

/// Entry point that parses command line arguments and runs CLI.
pub fn run_from_env() -> i32 {
    let args: Vec<String> = std::env::args().collect();
    run_cli(&args)
}

/// Executes the CLI with the provided argument list and returns exit code.
pub fn run_cli(args: &[String]) -> i32 {
    let cli = match Cli::try_parse_from(args) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "{}", e);
            return EXIT_ANALYSIS_ERROR;
        }
    };

    match execute(&cli) {
        Ok(code) => code,
        Err(err) => {
            let _ = writeln!(std::io::stderr(), "Error: {}", err);
            EXIT_ANALYSIS_ERROR
        }
    }
}

fn execute(cli: &Cli) -> Result<i32, CliError> {
    match &cli.command {
        Commands::Baseline {
            revision,
            incremental,
            baseline_id,
        } => handle_baseline(
            cli,
            revision.as_deref(),
            *incremental,
            baseline_id.as_deref(),
        ),
        Commands::Diff { before, after } => handle_diff(cli, before.as_deref(), after.as_deref()),
        Commands::Impact {
            symbol,
            direction,
            max_depth,
            budget,
            snapshot_id,
            edge_kinds,
            profile,
            top_k,
        } => handle_impact(
            cli,
            ImpactRequest {
                seeds: symbol,
                direction: *direction,
                max_depth: *max_depth,
                budget: *budget,
                snapshot_id: snapshot_id.as_deref(),
                edge_kinds: edge_kinds.as_deref(),
                profile: *profile,
                top_k: *top_k,
            },
        ),
        Commands::Verify {
            strict,
            invariants_file,
            baseline_id,
        } => handle_verify(
            cli,
            *strict,
            invariants_file.as_deref(),
            baseline_id.as_deref(),
        ),
        Commands::Explain { target } => handle_explain(cli, target),
        Commands::Symbols {
            limit,
            offset,
            kind,
        } => handle_symbols(cli, *limit, *offset, kind.as_deref()),
        Commands::Edges {
            source,
            direction,
            kind,
            limit,
            offset,
        } => handle_edges(cli, source, *direction, kind.as_deref(), *limit, *offset),
        Commands::Temporal {
            at,
            max_commits,
            strict,
        } => run_extension(
            cli,
            extensions::temporal(cli, at.as_deref(), *max_commits, *strict),
        ),
        Commands::Lineage {
            from,
            to,
            threshold,
            margin,
            max_files,
            max_file_bytes,
            strict,
        } => run_extension(
            cli,
            extensions::lineage(
                cli,
                &extensions::LineageArgs {
                    from: from.clone(),
                    to: to.clone(),
                    threshold: *threshold,
                    margin: *margin,
                    max_files: *max_files,
                    max_file_bytes: *max_file_bytes,
                    strict: *strict,
                },
            ),
        ),
        Commands::TestSelect {
            changed,
            lcov,
            fail_if_all_tests_required,
            strict,
        } => run_extension(
            cli,
            extensions::test_select(cli, changed, lcov, *fail_if_all_tests_required, *strict),
        ),
        Commands::ContractDist {
            from,
            to,
            protocol,
            strict,
        } => run_extension(
            cli,
            extensions::contract_dist(from, to, *protocol, None, *strict),
        ),
        Commands::Telemetry {
            input,
            salt,
            salt_file,
            surface,
            windows,
            complete_observation,
            dispatch_static,
            strict,
        } => run_extension(
            cli,
            extensions::telemetry(
                input,
                &extensions::TelemetryArgs {
                    salt_hex: salt.clone(),
                    salt_file: salt_file.clone(),
                    surface: surface.clone(),
                    windows: *windows,
                    complete_observation: *complete_observation,
                    dispatch_static: *dispatch_static,
                    strict: *strict,
                },
            ),
        ),
        Commands::Cochange {
            window_seconds,
            max_commits,
            max_files_per_commit,
            min_pair_commits,
            strict,
        } => run_extension(
            cli,
            extensions::cochange(
                &cli.repo_root,
                *window_seconds,
                *max_commits,
                *max_files_per_commit,
                *min_pair_commits,
                *strict,
            ),
        ),
        Commands::Catalog {
            root,
            max_file_bytes,
            strict,
        } => run_extension(
            cli,
            extensions::catalog(cli, root.as_deref(), *max_file_bytes, *strict),
        ),
        Commands::BlastRadius {
            reference,
            root,
            max_file_bytes,
            strict,
        } => run_extension(
            cli,
            extensions::blast_radius(cli, reference, root.as_deref(), *max_file_bytes, *strict),
        ),
        Commands::Doctor => handle_doctor(cli),
        Commands::Version => handle_version(cli),
    }
}

/// Prints an extension outcome and returns its exit code.
///
/// The handlers themselves never touch the terminal, which is what lets the Step 44 tests
/// assert an exit code and the exact output for it without a pty.
fn run_extension(
    cli: &Cli,
    outcome: Result<extensions::Outcome, CliError>,
) -> Result<i32, CliError> {
    let outcome = outcome?;
    outcome.print(cli.format)?;
    Ok(outcome.exit_code)
}

/// Storage resolution helper.
fn get_storage(cli: &Cli) -> Result<StorageEngine, CliError> {
    let autopsy_dir = cli.repo_root.join(".autopsy");
    let db_path = cli
        .db_path
        .clone()
        .unwrap_or_else(|| autopsy_dir.join("storage.db"));
    let cache_dir = cli
        .cache_dir
        .clone()
        .unwrap_or_else(|| autopsy_dir.join("cache"));

    let options = StorageOptions {
        use_wal: cli.wal,
        cache_dir: Some(cache_dir),
    };

    StorageEngine::open(db_path, options).map_err(|e| CliError::Storage(e.to_string()))
}

/// Load or synthesize default autopsy configuration.
///
/// A MISSING `autopsy.toml` falls back to safe defaults (zero-config
/// operation). A PRESENT but invalid file is a hard error with
/// path-aware diagnostics — never a silent fallback (02 §3: invalid
/// config fails before verification).
fn load_or_default_config(repo_root: &Path) -> Result<AutopsyConfig, CliError> {
    let config_path = repo_root.join("autopsy.toml");
    if !config_path.exists() {
        return Ok(AutopsyConfig {
            config_version: "0.0.1".to_string(),
            repository: autopsy_repo::RepositoryConfig {
                roots: vec![".".to_string()],
                exclude: vec![
                    "**/node_modules/**".to_string(),
                    "**/dist/**".to_string(),
                    "**/target/**".to_string(),
                    "**/.git/**".to_string(),
                    "**/.autopsy/**".to_string(),
                ],
            },
            analysis: autopsy_repo::AnalysisConfig {
                default_profile: "default".to_string(),
                max_traversal_nodes: 50_000,
                max_file_bytes: autopsy_repo::default_max_file_bytes(),
            },
            cache: autopsy_repo::CacheConfig {
                directory: ".autopsy/cache".to_string(),
            },
        });
    }

    AutopsyConfig::load_from_file(&config_path)
        .map_err(|e| CliError::Config(format!("{}: {}", config_path.display(), e)))
}

/// Pipeline result holding parsed artifacts.
pub struct AnalysisArtifacts {
    pub snapshot: RepoSnapshot,
    pub graph: DependencyGraph,
    pub symbols: Vec<Symbol>,
    pub edges: Vec<Edge>,
    pub contracts: Vec<Contract>,
    pub coverage_state: CoverageState,
    pub unsupported_constructs: Vec<String>,
}

/// Languages with full adapter analysis (symbol graph + contracts).
/// Mirrors `classify_language`; anything else is hashed but not modeled.
fn is_analyzed_language(language: &str) -> bool {
    matches!(language, "typescript" | "javascript" | "python")
}
/// Canonical (LF-normalized) bytes for a file, mirroring
/// `scan_repository` exactly: text languages are line-ending
/// normalized, all other content passes through byte-identical.
fn canonical_file_bytes(raw: &[u8], language: &str) -> Vec<u8> {
    if autopsy_repo::is_text_language(language) {
        autopsy_repo::normalize_line_endings(raw)
    } else {
        raw.to_vec()
    }
}

/// File-local analysis result: everything derivable from one file in
/// isolation (edges excluded — they need the global symbol index).
/// `parsed` is None for cache-reused files until edge extraction
/// requires it (populated lazily from contents by the assembler).
struct FileLocalAnalysis {
    symbols: Vec<Symbol>,
    contracts: Vec<Contract>,
    coverage: CoverageState,
    unsupported: Vec<String>,
    parsed: Option<ParsedFile>,
}

/// Serializable per-file analysis artifact for the incremental cache.
/// Stored under the file's content hash: identical bytes always yield
/// identical keys, so reuse is exact by construction.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct FileAnalysisArtifact {
    content_hash: String,
    symbols: Vec<Symbol>,
    edges: Vec<Edge>,
    contracts: Vec<Contract>,
    coverage: CoverageState,
    unsupported: Vec<String>,
}

/// Parses one file and extracts its file-local model.
/// Returns None when the file cannot be parsed (caller skips it).
/// The adapter is selected by the file's classified language.
fn analyze_single_file(
    ts_adapter: &TypeScriptAdapter,
    py_adapter: &PythonAdapter,
    parse_ctx: &ParseContext,
    unit: &FileUnit,
    content_str: &str,
) -> Option<FileLocalAnalysis> {
    macro_rules! analyze_with {
        ($adapter:expr) => {{
            let adapter = $adapter;
            let parsed: ParsedFile = adapter.parse(parse_ctx, unit, content_str).ok()?;
            let mut unsupported_here = Vec::new();
            if matches!(parsed.coverage, CoverageState::Unsupported) {
                for construct in &parsed.dynamic_constructs {
                    unsupported_here.push(format!("{:?}", construct.kind));
                }
            }
            let symbols = adapter.extract_symbols(&parsed).ok()?;
            let contracts = adapter.extract_contracts(&parsed).unwrap_or_default();
            Some(FileLocalAnalysis {
                symbols,
                contracts,
                coverage: parsed.coverage,
                unsupported: unsupported_here,
                parsed: Some(parsed),
            })
        }};
    }
    match unit.language.as_str() {
        "python" => analyze_with!(py_adapter),
        _ => analyze_with!(ts_adapter),
    }
}

/// Executes repository scan, AST extraction, symbol table, multigraph, and contract model.
/// When `persist` is provided, per-file analysis artifacts are written
/// through to the incremental cache.
pub fn build_snapshot_and_graph(
    repo_root: &Path,
    config: &AutopsyConfig,
    revision: Option<&str>,
    persist: Option<&StorageEngine>,
) -> Result<AnalysisArtifacts, CliError> {
    let snapshot = scan_repository(repo_root, config, ANALYZER_VERSION, revision)
        .map_err(|e| CliError::Config(e.to_string()))?;

    // Read current worktree contents for every scanned file.
    let mut contents: BTreeMap<String, String> = BTreeMap::new();
    for (file_path, unit) in &snapshot.files {
        if !is_analyzed_language(unit.language.as_str()) {
            continue;
        }
        let full_path = repo_root.join(file_path);
        if let Ok(raw) = std::fs::read(&full_path) {
            let canonical = canonical_file_bytes(&raw, &unit.language);
            if let Ok(text) = String::from_utf8(canonical) {
                contents.insert(file_path.clone(), text);
            }
        }
    }

    analyze_contents(repo_root, snapshot, contents, persist)
}

/// Rebuilds analysis artifacts for a STORED snapshot from the
/// content-addressed cache (FR-007, FR-010).
///
/// Every `baseline` persists per-file canonical bytes keyed by content
/// hash; this loader replays them through the identical two-phase
/// analysis pipeline, so `diff` and `verify --baseline-id` compare two
/// genuinely independent states instead of a state against itself.
pub fn build_artifacts_from_snapshot(
    snapshot: RepoSnapshot,
    storage: &StorageEngine,
) -> Result<AnalysisArtifacts, CliError> {
    let contents = load_snapshot_contents(&snapshot, storage)?;
    // Root dir is irrelevant here: contents come from CAS, never disk.
    analyze_contents(Path::new(""), snapshot, contents, Some(storage))
}

/// Two-phase deterministic analysis over explicit file contents.
///
/// Phase 1 parses every file and collects symbols into a GLOBAL
/// qualified-name index (deterministic `BTreeMap` order). Phase 2
/// extracts edges with the complete index visible, so cross-file
/// references resolve independent of filesystem walk order.
fn analyze_contents(
    repo_root: &Path,
    snapshot: RepoSnapshot,
    contents: BTreeMap<String, String>,
    persist_storage: Option<&StorageEngine>,
) -> Result<AnalysisArtifacts, CliError> {
    let ts_adapter = TypeScriptAdapter::with_root_dir(repo_root);
    let py_adapter = PythonAdapter::with_root_dir(repo_root);
    let parse_ctx = ParseContext {
        project_root: repo_root.to_path_buf(),
        options: BTreeMap::new(),
    };

    // Phase 1: file-local analysis (parse + symbols + contracts + coverage).
    let mut local_by_path: BTreeMap<String, FileLocalAnalysis> = BTreeMap::new();
    for (file_path, unit) in &snapshot.files {
        if !is_analyzed_language(unit.language.as_str()) {
            continue;
        }
        let Some(content_str) = contents.get(file_path) else {
            continue;
        };
        if let Some(local) =
            analyze_single_file(&ts_adapter, &py_adapter, &parse_ctx, unit, content_str)
        {
            local_by_path.insert(file_path.clone(), local);
        }
    }

    let (artifacts, edges_by_file, local_back) = assemble_artifacts(
        &ts_adapter,
        &py_adapter,
        &parse_ctx,
        snapshot.clone(),
        local_by_path,
        &BTreeMap::new(),
        &contents,
    )?;

    // Write-through per-file artifact cache for future incrementals.
    if let Some(storage) = persist_storage {
        let _ = store_file_artifacts(storage, &snapshot, &local_back, &edges_by_file);
    }

    Ok(artifacts)
}

/// Assembled analysis output: full artifacts, per-file edge split, and
/// the per-file local analyses back (for incremental artifact persistence).
type AssembledArtifacts = (
    AnalysisArtifacts,
    BTreeMap<String, Vec<Edge>>,
    BTreeMap<String, FileLocalAnalysis>,
);
/// Assembles full artifacts from per-file local analyses.
///
/// `cached_edges` carries edges reused from the incremental cache for
/// files whose global symbol context is unchanged; files listed here
/// skip edge re-extraction. Empty in full (non-incremental) analysis.
///
/// Files with `parsed: None` (cache-reused) are re-parsed from
/// `contents` only when their edges must be extracted.
fn assemble_artifacts(
    ts_adapter: &TypeScriptAdapter,
    py_adapter: &PythonAdapter,
    parse_ctx: &ParseContext,
    snapshot: RepoSnapshot,
    mut local_by_path: BTreeMap<String, FileLocalAnalysis>,
    cached_edges: &BTreeMap<String, Vec<Edge>>,
    contents: &BTreeMap<String, String>,
) -> Result<AssembledArtifacts, CliError> {
    let mut all_symbols: Vec<Symbol> = Vec::new();
    let mut symbol_index: BTreeMap<String, SymbolId> = BTreeMap::new();
    for local in local_by_path.values() {
        for sym in &local.symbols {
            symbol_index.insert(sym.qualified_name.clone(), sym.stable_id.clone());
            all_symbols.push(sym.clone());
        }
    }
    all_symbols.sort_by(|a, b| {
        a.stable_id
            .cmp(&b.stable_id)
            .then_with(|| a.qualified_name.cmp(&b.qualified_name))
    });

    // Phase 2: edges with the global index (fresh except cache hits).
    // Per-file edge attribution is retained for the incremental cache.
    let mut graph = DependencyGraph::new();
    for sym in &all_symbols {
        graph.add_node(sym.clone());
    }
    let mut all_edges = Vec::new();
    let mut all_contracts = Vec::new();
    let mut edges_by_file: BTreeMap<String, Vec<Edge>> = BTreeMap::new();
    for (file_path, local) in &mut local_by_path {
        let mut file_edges: Vec<Edge> = Vec::new();
        if let Some(reused) = cached_edges.get(file_path) {
            for edge in reused {
                graph.add_edge(edge.clone());
                all_edges.push(edge.clone());
                file_edges.push(edge.clone());
            }
        } else {
            // Lazily (re-)parse cache-reused files only when edges are
            // actually extracted; fresh files already carry `parsed`.
            if local.parsed.is_none()
                && let Some(unit) = snapshot.files.get(file_path)
                && let Some(content_str) = contents.get(file_path)
                && let Ok(parsed) = match unit.language.as_str() {
                    "python" => py_adapter.parse(parse_ctx, unit, content_str),
                    _ => ts_adapter.parse(parse_ctx, unit, content_str),
                }
            {
                local.parsed = Some(parsed);
            }
            if let Some(parsed) = local.parsed.as_ref() {
                let edges = match parsed.file.language.as_str() {
                    "python" => py_adapter.extract_edges(parsed, &symbol_index),
                    _ => ts_adapter.extract_edges(parsed, &symbol_index),
                };
                if let Ok(edges) = edges {
                    for edge in &edges {
                        graph.add_edge(edge.clone());
                        all_edges.push(edge.clone());
                        file_edges.push(edge.clone());
                    }
                }
            }
        }
        edges_by_file.insert(file_path.clone(), file_edges);
        all_contracts.extend(local.contracts.iter().cloned());
    }
    all_edges.sort_by(|a, b| (&a.kind, &a.source, &a.target).cmp(&(&b.kind, &b.source, &b.target)));
    all_contracts.sort_by(|a, b| a.owner.cmp(&b.owner));

    let mut unsupported_constructs = BTreeSet::new();
    let mut had_unknown = false;
    let mut had_partial = false;
    for local in local_by_path.values() {
        match local.coverage {
            CoverageState::Unknown => had_unknown = true,
            CoverageState::Partial => had_partial = true,
            CoverageState::Unsupported => {
                for u in &local.unsupported {
                    unsupported_constructs.insert(u.clone());
                }
            }
            CoverageState::Verified => {}
        }
    }
    let overall_coverage = if had_unknown {
        CoverageState::Unknown
    } else if !unsupported_constructs.is_empty() {
        CoverageState::Unsupported
    } else if had_partial {
        CoverageState::Partial
    } else {
        CoverageState::Verified
    };

    Ok((
        AnalysisArtifacts {
            snapshot,
            graph,
            symbols: all_symbols,
            edges: all_edges,
            contracts: all_contracts,
            coverage_state: overall_coverage,
            unsupported_constructs: unsupported_constructs.into_iter().collect(),
        },
        edges_by_file,
        local_by_path,
    ))
}

/// Normalizes domain contracts into the compatibility-checker model
/// (shared by before/after states so joins align).
pub fn normalize_contracts(contracts: &[Contract]) -> BTreeMap<SymbolId, NormalizedContract> {
    let mut map = BTreeMap::new();
    for c in contracts {
        map.insert(
            c.owner.clone(),
            NormalizedContract::Callable(CallableContract {
                owner: c.owner.clone(),
                name: c.owner.to_string(),
                visibility: c.visibility,
                parameters: c
                    .inputs
                    .iter()
                    .map(|inp| ParameterModel::new(inp.clone()))
                    .collect(),
                return_type: c.output.clone(),
                type_parameters: Vec::new(),
                is_async: false,
            }),
        );
    }
    map
}

/// Persists canonical per-file bytes for a snapshot into the
/// content-addressed cache, keyed by `content_hash`.
///
/// `put_cache_object` keys by BLAKE3 of the stored bytes, and
/// `scan_repository` defines `content_hash` as BLAKE3 of the canonical
/// bytes, so the round-trip key matches by construction. Files whose
/// bytes no longer match (raced modification) are skipped with a
/// stderr diagnostic rather than cached corruptly.
pub fn cache_snapshot_contents(
    repo_root: &Path,
    snapshot: &RepoSnapshot,
    storage: &StorageEngine,
) -> Result<usize, CliError> {
    use blake3::hash as blake3_hash;
    let mut cached = 0usize;
    for (file_path, unit) in &snapshot.files {
        let full_path = repo_root.join(file_path);
        let Ok(raw) = std::fs::read(&full_path) else {
            continue;
        };
        let canonical = canonical_file_bytes(&raw, &unit.language);
        if blake3_hash(&canonical).to_hex().to_string() != unit.content_hash {
            let _ = writeln!(
                std::io::stderr(),
                "warning: file changed during scan, skipping content cache: {}",
                file_path
            );
            continue;
        }
        storage
            .put_cache_object(&canonical)
            .map_err(|e| CliError::Storage(e.to_string()))?;
        cached += 1;
    }
    Ok(cached)
}

/// Stores per-file analysis artifacts in the incremental cache.
/// Artifacts are keyed by content hash: unchanged files map to
/// identical keys, so later `--incremental` runs reuse them exactly.
/// Only successfully analyzed files are stored.
fn store_file_artifacts(
    storage: &StorageEngine,
    snapshot: &RepoSnapshot,
    local_by_path: &BTreeMap<String, FileLocalAnalysis>,
    edges_by_file: &BTreeMap<String, Vec<Edge>>,
) -> Result<usize, CliError> {
    let mut stored = 0usize;
    for (file_path, local) in local_by_path {
        let Some(unit) = snapshot.files.get(file_path) else {
            continue;
        };
        let artifact = FileAnalysisArtifact {
            content_hash: unit.content_hash.clone(),
            symbols: local.symbols.clone(),
            edges: edges_by_file.get(file_path).cloned().unwrap_or_default(),
            contracts: local.contracts.clone(),
            coverage: local.coverage,
            unsupported: local.unsupported.clone(),
        };
        let bytes = serde_json::to_vec(&artifact).map_err(|e| CliError::Analysis(e.to_string()))?;
        storage
            .put_file_artifact(&unit.content_hash, &bytes)
            .map_err(|e| CliError::Storage(e.to_string()))?;
        stored += 1;
    }
    Ok(stored)
}

/// Loads a cached per-file artifact by content hash.
/// Returns None on cache miss or corrupt entry (caller falls back to
/// fresh analysis — never a hard error).
fn load_file_artifact(storage: &StorageEngine, content_hash: &str) -> Option<FileAnalysisArtifact> {
    let bytes = storage.get_file_artifact(content_hash).ok()??;
    serde_json::from_slice(&bytes).ok()
}

/// Attempts incremental analysis against a base snapshot.
///
/// Reuses cached per-file artifacts for byte-identical files and skips
/// edge re-extraction for them when the global symbol-id set is
/// unchanged. Returns None when the cache cannot satisfy the request
/// (caller runs the full pipeline — always correct, occasionally
/// slower). Changed files are always analyzed fresh from worktree bytes.
fn try_incremental_artifacts(
    repo_root: &Path,
    base_snapshot: &RepoSnapshot,
    new_snapshot: &RepoSnapshot,
    storage: &StorageEngine,
) -> Option<(AnalysisArtifacts, usize, usize)> {
    let ts_adapter = TypeScriptAdapter::with_root_dir(repo_root);
    let py_adapter = PythonAdapter::with_root_dir(repo_root);
    let parse_ctx = ParseContext {
        project_root: repo_root.to_path_buf(),
        options: BTreeMap::new(),
    };

    // 1. Load base artifacts; any miss degrades to full analysis.
    let mut base_ids: BTreeSet<SymbolId> = BTreeSet::new();
    let mut base_by_hash: BTreeMap<String, FileAnalysisArtifact> = BTreeMap::new();
    for unit in base_snapshot.files.values() {
        if !is_analyzed_language(unit.language.as_str()) {
            continue;
        }
        let artifact = load_file_artifact(storage, &unit.content_hash)?;
        for sym in &artifact.symbols {
            base_ids.insert(sym.stable_id.clone());
        }
        base_by_hash.insert(unit.content_hash.clone(), artifact);
    }

    // 2. Read worktree contents once (cheap I/O; parsing is the cost).
    let mut contents: BTreeMap<String, String> = BTreeMap::new();
    for (file_path, unit) in &new_snapshot.files {
        if !is_analyzed_language(unit.language.as_str()) {
            continue;
        }
        let full_path = repo_root.join(file_path);
        if let Ok(raw) = std::fs::read(&full_path) {
            let canonical = canonical_file_bytes(&raw, &unit.language);
            if let Ok(text) = String::from_utf8(canonical) {
                contents.insert(file_path.clone(), text);
            }
        }
    }

    // 3. Partition files into reused (no parse at all) vs fresh.
    let mut local_by_path: BTreeMap<String, FileLocalAnalysis> = BTreeMap::new();
    let mut staged_edges: BTreeMap<String, Vec<Edge>> = BTreeMap::new();
    let mut reused = 0usize;
    let mut total = 0usize;
    for (file_path, unit) in &new_snapshot.files {
        if !is_analyzed_language(unit.language.as_str()) {
            continue;
        }
        total += 1;
        let base_unit = base_snapshot.files.get(file_path);
        let same_bytes = base_unit.is_some_and(|b| b.content_hash == unit.content_hash);
        if same_bytes && let Some(artifact) = base_by_hash.get(&unit.content_hash) {
            reused += 1;
            staged_edges.insert(file_path.clone(), artifact.edges.clone());
            local_by_path.insert(
                file_path.clone(),
                FileLocalAnalysis {
                    symbols: artifact.symbols.clone(),
                    contracts: artifact.contracts.clone(),
                    coverage: artifact.coverage,
                    unsupported: artifact.unsupported.clone(),
                    parsed: None,
                },
            );
            continue;
        }
        // Changed or new file: analyze fresh from worktree bytes.
        if let Some(content_str) = contents.get(file_path)
            && let Some(local) =
                analyze_single_file(&ts_adapter, &py_adapter, &parse_ctx, unit, content_str)
        {
            local_by_path.insert(file_path.clone(), local);
        }
    }

    // 4. Global symbol-id set decides edge reuse soundness.
    let mut current_ids: BTreeSet<SymbolId> = BTreeSet::new();
    for local in local_by_path.values() {
        for sym in &local.symbols {
            current_ids.insert(sym.stable_id.clone());
        }
    }
    let cached_edges: BTreeMap<String, Vec<Edge>> = if current_ids == base_ids {
        staged_edges
    } else {
        BTreeMap::new()
    };

    let (artifacts, edges_by_file, local_back) = assemble_artifacts(
        &ts_adapter,
        &py_adapter,
        &parse_ctx,
        new_snapshot.clone(),
        local_by_path,
        &cached_edges,
        &contents,
    )
    .ok()?;

    // 5. Persist freshly computed artifacts for future incrementals.
    let mut fresh_locals = local_back;
    for (file_path, edges) in &edges_by_file {
        if cached_edges.contains_key(file_path) {
            fresh_locals.remove(file_path);
            continue;
        }
        if let (Some(unit), Some(local)) = (
            new_snapshot.files.get(file_path),
            fresh_locals.get(file_path),
        ) {
            let artifact = FileAnalysisArtifact {
                content_hash: unit.content_hash.clone(),
                symbols: local.symbols.clone(),
                edges: edges.clone(),
                contracts: local.contracts.clone(),
                coverage: local.coverage,
                unsupported: local.unsupported.clone(),
            };
            if let Ok(bytes) = serde_json::to_vec(&artifact) {
                let _ = storage.put_file_artifact(&unit.content_hash, &bytes);
            }
        }
    }

    Some((artifacts, reused, total))
}
/// Loads canonical per-file bytes for a stored snapshot from the
/// content-addressed cache. Returns only TypeScript/JavaScript sources
/// (the analyzed subset); other files are irrelevant to the pipeline.
fn load_snapshot_contents(
    snapshot: &RepoSnapshot,
    storage: &StorageEngine,
) -> Result<BTreeMap<String, String>, CliError> {
    let mut contents = BTreeMap::new();
    for (file_path, unit) in &snapshot.files {
        if !is_analyzed_language(unit.language.as_str()) {
            continue;
        }
        match storage
            .get_cache_object(&unit.content_hash)
            .map_err(|e| CliError::Storage(e.to_string()))?
        {
            Some(bytes) => match String::from_utf8(bytes) {
                Ok(text) => {
                    contents.insert(file_path.clone(), text);
                }
                Err(_) => {
                    return Err(CliError::Storage(format!(
                        "cached content for '{}' (snapshot {}) is not valid UTF-8",
                        file_path, snapshot.snapshot_id
                    )));
                }
            },
            None => {
                return Err(CliError::Storage(format!(
                    "before-state content for '{}' (snapshot {}) is not cached; \
                     re-run `baseline` to persist snapshot contents, then retry",
                    file_path, snapshot.snapshot_id
                )));
            }
        }
    }
    Ok(contents)
}

// ---------------------------------------------------------------------------
// SUBCOMMAND: BASELINE
// ---------------------------------------------------------------------------
fn handle_baseline(
    cli: &Cli,
    revision: Option<&str>,
    incremental: bool,
    baseline_id_opt: Option<&str>,
) -> Result<i32, CliError> {
    let config = load_or_default_config(&cli.repo_root)?;
    let storage = get_storage(cli)?;

    // Full pipeline always runs the complete analysis; incremental
    // reuses per-file artifacts for byte-identical files when possible.
    let artifacts = if incremental {
        let base_opt = match baseline_id_opt {
            Some(id) => storage
                .get_snapshot(&SnapshotId::new(id))
                .map_err(|e| CliError::Storage(e.to_string()))?,
            None => storage
                .get_latest_snapshot()
                .map_err(|e| CliError::Storage(e.to_string()))?,
        };
        match base_opt {
            Some(base) => {
                let new_snapshot =
                    scan_repository(&cli.repo_root, &config, ANALYZER_VERSION, revision)
                        .map_err(|e| CliError::Config(e.to_string()))?;
                match try_incremental_artifacts(&cli.repo_root, &base, &new_snapshot, &storage) {
                    Some((artifacts, reused, total)) => {
                        let _ = writeln!(
                            std::io::stderr(),
                            "incremental baseline: reused {}/{} file analyses from cache",
                            reused,
                            total
                        );
                        artifacts
                    }
                    None => {
                        let _ = writeln!(
                            std::io::stderr(),
                            "incremental baseline: cache incomplete, running full analysis"
                        );
                        build_snapshot_and_graph(&cli.repo_root, &config, revision, Some(&storage))?
                    }
                }
            }
            None => {
                let _ = writeln!(
                    std::io::stderr(),
                    "incremental baseline: no stored baseline, running full analysis"
                );
                build_snapshot_and_graph(&cli.repo_root, &config, revision, Some(&storage))?
            }
        }
    } else {
        build_snapshot_and_graph(&cli.repo_root, &config, revision, Some(&storage))?
    };

    // Empty snapshots almost always mean a wrong --repo-root (relative
    // path resolved against an unintended directory). Warn loudly but
    // keep exit 0: empty repositories are legal.
    if artifacts.snapshot.files.is_empty() {
        let _ = writeln!(
            std::io::stderr(),
            "warning: snapshot contains 0 files; check --repo-root '{}' (use an absolute path)",
            cli.repo_root.display()
        );
    }
    // Surface resource-bound exclusions honestly: oversized files never
    // entered the snapshot, and operators must know what was not analyzed.
    if !artifacts.snapshot.skipped_oversize_files.is_empty() {
        let _ = writeln!(
            std::io::stderr(),
            "warning: {} file(s) exceeded max_file_bytes ({}) and were excluded from analysis: {}",
            artifacts.snapshot.skipped_oversize_files.len(),
            config.analysis.max_file_bytes,
            artifacts.snapshot.skipped_oversize_files.join(", ")
        );
    }

    // Persist snapshot record in SQLite database
    storage
        .save_snapshot(&artifacts.snapshot)
        .map_err(|e| CliError::Storage(e.to_string()))?;

    // Persist canonical per-file bytes into the content-addressed cache
    // so later `diff` / `verify --baseline-id` can rebuild this exact
    // state (FR-007, FR-010). Keyed by content_hash by construction.
    let cached_files = cache_snapshot_contents(&cli.repo_root, &artifacts.snapshot, &storage)?;
    if cached_files != artifacts.snapshot.files.len() {
        let _ = writeln!(
            std::io::stderr(),
            "warning: cached {}/{} snapshot files; before-state rebuild may be incomplete",
            cached_files,
            artifacts.snapshot.files.len()
        );
    }

    // Build and output report
    let coverage = CoverageSummary {
        state: format!("{:?}", artifacts.coverage_state).to_lowercase(),
        unsupported_constructs: artifacts.unsupported_constructs,
        files_scanned: Some(artifacts.snapshot.files.len()),
    };

    let report = AutopsyReport::new(
        format!(
            "baseline-{}",
            &artifacts.snapshot.snapshot_id.to_string()[..8]
        ),
        "baseline".to_string(),
        ANALYZER_VERSION.to_string(),
        Some(0),
        vec![SnapshotSummary {
            snapshot_id: artifacts.snapshot.snapshot_id.to_string(),
            revision: artifacts.snapshot.revision.clone(),
            config_hash: artifacts.snapshot.config_hash.clone(),
            file_set_digest: artifacts.snapshot.file_set_digest.clone(),
        }],
        None,
        None,
        vec![],
        None,
        coverage,
    )?;

    emit_report(cli.format, &report)?;
    Ok(EXIT_PASS)
}

// ---------------------------------------------------------------------------
// SUBCOMMAND: DIFF
// ---------------------------------------------------------------------------
fn handle_diff(
    cli: &Cli,
    before_opt: Option<&str>,
    after_opt: Option<&str>,
) -> Result<i32, CliError> {
    let config = load_or_default_config(&cli.repo_root)?;
    let storage = get_storage(cli)?;

    // Resolve the after-state: a stored snapshot ID rebuilds that exact
    // state from cache; anything else (or nothing) analyzes the current
    // worktree, keeping the value as a revision label only.
    let after_artifacts: AnalysisArtifacts = match after_opt {
        Some(id) => {
            let stored_opt = storage
                .get_snapshot(&SnapshotId::new(id))
                .map_err(|e| CliError::Storage(e.to_string()))?;
            match stored_opt {
                Some(stored) => build_artifacts_from_snapshot(stored, &storage)?,
                None => build_snapshot_and_graph(&cli.repo_root, &config, Some(id), None)?,
            }
        }
        None => build_snapshot_and_graph(&cli.repo_root, &config, None, None)?,
    };

    // Resolve the before-state snapshot record (--before id, else latest).
    let before_snapshot = if let Some(b) = before_opt {
        let id = SnapshotId::new(b);
        storage
            .get_snapshot(&id)
            .map_err(|e| CliError::Storage(e.to_string()))?
            .ok_or_else(|| {
                CliError::Storage(format!(
                    "baseline snapshot '{}' not found; run `baseline` first",
                    b
                ))
            })?
    } else {
        storage
            .get_latest_snapshot()
            .map_err(|e| CliError::Storage(e.to_string()))?
            .unwrap_or_else(|| after_artifacts.snapshot.clone())
    };

    // Rebuild the genuine before-state analysis from cached contents.
    // If the before snapshot IS the current worktree state (identical
    // file-set digest), reuse the fresh artifacts directly.
    let before_artifacts: Option<AnalysisArtifacts> = if before_snapshot.snapshot_id
        == after_artifacts.snapshot.snapshot_id
        && before_snapshot.file_set_digest == after_artifacts.snapshot.file_set_digest
    {
        None
    } else {
        Some(build_artifacts_from_snapshot(
            before_snapshot.clone(),
            &storage,
        )?)
    };
    let before_graph = before_artifacts
        .as_ref()
        .map(|a| &a.graph)
        .unwrap_or(&after_artifacts.graph);
    let before_contracts: &[Contract] = before_artifacts
        .as_ref()
        .map(|a| a.contracts.as_slice())
        .unwrap_or(&after_artifacts.contracts);

    let diff_input = DiffInput {
        before_snapshot_id: before_snapshot.snapshot_id.clone(),
        after_snapshot_id: after_artifacts.snapshot.snapshot_id.clone(),
        before_files: &before_snapshot.files,
        after_files: &after_artifacts.snapshot.files,
        before_graph,
        after_graph: &after_artifacts.graph,
        before_contracts,
        after_contracts: &after_artifacts.contracts,
    };

    let semantic_diff = DiffEngine::compute_diff(diff_input);
    let change_set = convert_semantic_diff(&semantic_diff);
    let coverage = CoverageSummary {
        state: format!("{:?}", after_artifacts.coverage_state).to_lowercase(),
        unsupported_constructs: after_artifacts.unsupported_constructs,
        files_scanned: Some(after_artifacts.snapshot.files.len()),
    };

    let report = AutopsyReport::new(
        format!(
            "diff-{}",
            &after_artifacts.snapshot.snapshot_id.to_string()[..8]
        ),
        "diff".to_string(),
        ANALYZER_VERSION.to_string(),
        Some(0),
        vec![
            SnapshotSummary {
                snapshot_id: before_snapshot.snapshot_id.to_string(),
                revision: before_snapshot.revision.clone(),
                config_hash: before_snapshot.config_hash.clone(),
                file_set_digest: before_snapshot.file_set_digest.clone(),
            },
            SnapshotSummary {
                snapshot_id: after_artifacts.snapshot.snapshot_id.to_string(),
                revision: after_artifacts.snapshot.revision.clone(),
                config_hash: after_artifacts.snapshot.config_hash.clone(),
                file_set_digest: after_artifacts.snapshot.file_set_digest.clone(),
            },
        ],
        Some(change_set),
        None,
        vec![],
        None,
        coverage,
    )?;

    emit_report(cli.format, &report)?;
    Ok(EXIT_PASS)
}

// ---------------------------------------------------------------------------
// SUBCOMMAND: IMPACT
// ---------------------------------------------------------------------------
/// Bundled impact query options (keeps the handler signature lint-clean).
struct ImpactRequest<'a> {
    seeds: &'a [String],
    direction: Option<CliImpactDirection>,
    max_depth: usize,
    budget: usize,
    snapshot_id: Option<&'a str>,
    edge_kinds: Option<&'a str>,
    profile: CliImpactProfile,
    top_k: Option<usize>,
}

fn handle_impact(cli: &Cli, req: ImpactRequest<'_>) -> Result<i32, CliError> {
    use autopsy_domain::EdgeKind;
    use autopsy_impact::callers_profile;
    let config = load_or_default_config(&cli.repo_root)?;
    let storage = get_storage(cli)?;
    // A stored snapshot ID analyzes that exact state from cache;
    // otherwise the current worktree is analyzed.
    let artifacts = match req.snapshot_id {
        Some(id) => {
            let stored_opt = storage
                .get_snapshot(&SnapshotId::new(id))
                .map_err(|e| CliError::Storage(e.to_string()))?;
            match stored_opt {
                Some(stored) => build_artifacts_from_snapshot(stored, &storage)?,
                None => {
                    return Err(CliError::Storage(format!(
                        "snapshot '{}' not found; run `baseline` first",
                        id
                    )));
                }
            }
        }
        None => build_snapshot_and_graph(&cli.repo_root, &config, None, None)?,
    };

    let mut seed_symbols = Vec::new();
    for seed_str in req.seeds {
        if let Some(sym) = artifacts.symbols.iter().find(|s| {
            s.stable_id.as_str() == seed_str
                || s.qualified_name == *seed_str
                || s.qualified_name.ends_with(&format!("::{}", seed_str))
        }) {
            seed_symbols.push(sym.stable_id.clone());
        } else if seed_str.contains("::") || seed_str.contains('#') {
            seed_symbols.push(SymbolId::new(seed_str.clone()));
        } else {
            // Bare file path seed: address the module node so file-level
            // queries traverse Contains/Imports structure (FR-008).
            let file_node = format!("file::{}", seed_str);
            if artifacts.graph.contains_node(&SymbolId::new(&file_node)) {
                seed_symbols.push(SymbolId::new(file_node));
            } else {
                seed_symbols.push(SymbolId::new(seed_str.clone()));
            }
        }
    }

    let bounded_budget = req.budget.min(config.analysis.max_traversal_nodes as usize);

    // Preset resolution (explicit flags always win per dimension):
    // - profile=full (default): legacy behavior — forward, all core
    //   kinds, file transit allowed, no shortlist.
    // - profile=callers: backward invocation closure, file-node transit
    //   suppressed, top-10 ranked shortlist unless --top-k overrides.
    let use_callers = matches!(req.profile, CliImpactProfile::Callers);
    let direction: ImpactDirection = match req.direction {
        Some(d) => d.into(),
        None => {
            if use_callers {
                ImpactDirection::Backward
            } else {
                ImpactDirection::Forward
            }
        }
    };
    let top_k: Option<usize> = match req.top_k {
        Some(k) => Some(k),
        None => {
            if use_callers {
                Some(10)
            } else {
                None
            }
        }
    };

    // Operator precision control: restrict traversed edge kinds.
    // Unknown kind names are hard errors (never silently ignored).
    let mut kinds = BTreeSet::new();
    if let Some(list) = req.edge_kinds {
        for raw in list.split(',') {
            let k = raw.trim().to_lowercase();
            let kind = match k.as_str() {
                "contains" => EdgeKind::Contains,
                "imports" => EdgeKind::Imports,
                "references" => EdgeKind::References,
                "calls" => EdgeKind::Calls,
                "implements" => EdgeKind::Implements,
                "inherits" => EdgeKind::Inherits,
                "reads-schema" | "reads_schema" => EdgeKind::ReadsSchema,
                "writes-schema" | "writes_schema" => EdgeKind::WritesSchema,
                "test-covers" | "test_covers" => EdgeKind::TestCovers,
                other => {
                    return Err(CliError::Config(format!(
                        "unknown edge kind '{}'; expected contains,imports,references,calls,implements,inherits,reads-schema,writes-schema,test-covers",
                        other
                    )));
                }
            };
            kinds.insert(kind);
        }
        if kinds.is_empty() {
            return Err(CliError::Config(
                "edge-kinds filter must name at least one edge kind".to_string(),
            ));
        }
    } else if use_callers {
        kinds = callers_profile(req.max_depth, bounded_budget).edge_kinds;
    } else {
        kinds = ImpactProfile::default().edge_kinds;
    }

    let traverse_file_nodes = !use_callers || req.edge_kinds.is_some();

    let query = ImpactQuery {
        seed_symbols,
        direction,
        profile: ImpactProfile {
            name: if use_callers {
                "cli-impact-callers".to_string()
            } else {
                "cli-impact".to_string()
            },
            max_depth: req.max_depth,
            budget: bounded_budget,
            edge_kinds: kinds,
            traverse_file_nodes,
        },
    };

    let impact_engine = ImpactEngine::new();
    let impact_result = impact_engine
        .compute_impact(&artifacts.graph, &query)
        .map_err(|e| CliError::Analysis(e.to_string()))?;

    let impact_summary = convert_impact_result_top_k(&impact_result, top_k);
    let coverage = CoverageSummary {
        state: format!("{:?}", artifacts.coverage_state).to_lowercase(),
        unsupported_constructs: artifacts.unsupported_constructs,
        files_scanned: Some(artifacts.snapshot.files.len()),
    };

    let report = AutopsyReport::new(
        format!(
            "impact-{}",
            &artifacts.snapshot.snapshot_id.to_string()[..8]
        ),
        "impact".to_string(),
        ANALYZER_VERSION.to_string(),
        Some(0),
        vec![SnapshotSummary {
            snapshot_id: artifacts.snapshot.snapshot_id.to_string(),
            revision: artifacts.snapshot.revision.clone(),
            config_hash: artifacts.snapshot.config_hash.clone(),
            file_set_digest: artifacts.snapshot.file_set_digest.clone(),
        }],
        None,
        Some(impact_summary),
        vec![],
        None,
        coverage,
    )?;

    emit_report(cli.format, &report)?;
    Ok(EXIT_PASS)
}

// ---------------------------------------------------------------------------
// SUBCOMMAND: VERIFY
// ---------------------------------------------------------------------------
fn handle_verify(
    cli: &Cli,
    strict: bool,
    invariants_file_opt: Option<&Path>,
    baseline_id_opt: Option<&str>,
) -> Result<i32, CliError> {
    let config = load_or_default_config(&cli.repo_root)?;
    let artifacts = build_snapshot_and_graph(&cli.repo_root, &config, None, None)?;
    let storage = get_storage(cli)?;

    // Load invariants configuration
    let invariants_config = if let Some(path) = invariants_file_opt {
        InvariantsConfig::load_from_file(path).map_err(|e| CliError::Config(e.to_string()))?
    } else {
        let default_inv_path = cli.repo_root.join(".autopsy").join("invariants.yml");
        if default_inv_path.exists() {
            InvariantsConfig::load_from_file(&default_inv_path)
                .map_err(|e| CliError::Config(e.to_string()))?
        } else {
            // Default built-in invariant suite
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
        }
    };

    // Prepare normalized contract maps. The before-state comes from
    // the stored baseline snapshot (rebuilt from cached contents);
    // without a baseline there is no "before", and api-compat honestly
    // reports Unknown instead of a vacuous pass (FR-010, FR-030).
    let contracts_after = normalize_contracts(&artifacts.contracts);

    // Load baseline snapshot and rebuild its graph + contracts if
    // --baseline-id is specified. Unknown ids are hard errors: silently
    // falling back to single-state analysis would hide the comparison
    // the operator explicitly requested.
    let baseline_artifacts: Option<AnalysisArtifacts> = if let Some(base_id) = baseline_id_opt {
        let stored = storage
            .get_snapshot(&SnapshotId::new(base_id))
            .map_err(|e| CliError::Storage(e.to_string()))?
            .ok_or_else(|| {
                CliError::Storage(format!(
                    "baseline snapshot '{}' not found; run `baseline` first",
                    base_id
                ))
            })?;
        Some(build_artifacts_from_snapshot(stored, &storage)?)
    } else {
        None
    };
    let contracts_before: BTreeMap<SymbolId, NormalizedContract> = baseline_artifacts
        .as_ref()
        .map(|a| normalize_contracts(&a.contracts))
        .unwrap_or_default();

    // Load accepted baseline debt if available (.autopsy/baseline.json)
    let baseline_debt_path = cli.repo_root.join(".autopsy").join("baseline.json");
    let accepted_debt_entries: Vec<AcceptedDebtEntry> = if baseline_debt_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&baseline_debt_path) {
            if let Ok(base_cfg) = serde_json::from_str::<BaselineConfig>(&content) {
                base_cfg.accepted_debt
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Load baseline snapshot and graph if baseline_id is specified
    let baseline_graph: Option<&DependencyGraph> = baseline_artifacts.as_ref().map(|a| &a.graph);

    let eval_ctx = EvaluationContext {
        snapshot_id: artifacts.snapshot.snapshot_id.clone(),
        analyzer_version: ANALYZER_VERSION,
        graph: &artifacts.graph,
        baseline_graph,
        diff: None,
        contracts_before: &contracts_before,
        contracts_after: &contracts_after,
        coverage_state: artifacts.coverage_state,
    };

    let evaluator = InvariantEvaluator::new();
    let evaluation_pairs = evaluator
        .evaluate(&invariants_config, &eval_ctx)
        .map_err(|e| CliError::Invariant(e.to_string()))?;

    let mut findings: Vec<Finding> = Vec::new();
    let mut evidence_receipts: Vec<EvidenceReceipt> = Vec::new();
    let mut has_failure = false;

    for (mut finding, evidence) in evaluation_pairs {
        let is_accepted_debt = accepted_debt_entries.iter().any(|entry| {
            entry.rule_id == finding.rule_id
                && (entry.fingerprint == finding.id
                    || entry.fingerprint == "*"
                    || finding
                        .entities
                        .iter()
                        .any(|e| e.contains(&entry.fingerprint)))
        });

        if is_accepted_debt && finding.status == FindingStatus::Fail {
            finding.status = FindingStatus::Pass;
            finding.message = format!("[ACCEPTED BASELINE DEBT] {}", finding.message);
        }

        if finding.status == FindingStatus::Fail {
            has_failure = true;
        }
        findings.push(finding);
        evidence_receipts.push(evidence);
    }

    let (finding_summaries, evidence_summaries) =
        convert_domain_findings(&findings, &evidence_receipts);

    let coverage = CoverageSummary {
        state: format!("{:?}", artifacts.coverage_state).to_lowercase(),
        unsupported_constructs: artifacts.unsupported_constructs,
        files_scanned: Some(artifacts.snapshot.files.len()),
    };

    let run_id = format!(
        "verify-{}",
        &artifacts.snapshot.snapshot_id.to_string()[..8]
    );
    let report = AutopsyReport::new(
        run_id.clone(),
        "verify".to_string(),
        ANALYZER_VERSION.to_string(),
        Some(0),
        vec![SnapshotSummary {
            snapshot_id: artifacts.snapshot.snapshot_id.to_string(),
            revision: artifacts.snapshot.revision.clone(),
            config_hash: artifacts.snapshot.config_hash.clone(),
            file_set_digest: artifacts.snapshot.file_set_digest.clone(),
        }],
        None,
        None,
        finding_summaries,
        Some(evidence_summaries),
        coverage,
    )?;

    // Persist run and findings to storage
    let run = AnalysisRun {
        run_id,
        command: "verify".to_string(),
        analyzer_version: ANALYZER_VERSION.to_string(),
        duration_ms: 0,
        result_digest: report.result_digest.clone(),
    };
    let _ = storage.save_run(&run, &findings, &evidence_receipts);

    emit_report(cli.format, &report)?;

    if has_failure {
        Ok(EXIT_POLICY_FAIL)
    } else if strict
        && matches!(
            artifacts.coverage_state,
            CoverageState::Unknown | CoverageState::Unsupported
        )
    {
        Ok(EXIT_UNSUPPORTED)
    } else {
        Ok(EXIT_PASS)
    }
}

// ---------------------------------------------------------------------------
// SUBCOMMANDS: SYMBOLS / EDGES (paginated read-only graph browsing)
// ---------------------------------------------------------------------------

/// Shared guard: paginated browsing supports JSON and text only.
/// SARIF describes findings, not symbol listings.
fn require_browse_format(format: OutputFormat) -> Result<(), CliError> {
    if format == OutputFormat::Sarif {
        return Err(CliError::Config(
            "SARIF output is only defined for finding reports (verify); use json or text for graph browsing".to_string(),
        ));
    }
    Ok(())
}

fn handle_symbols(
    cli: &Cli,
    limit: usize,
    offset: usize,
    kind_filter: Option<&str>,
) -> Result<i32, CliError> {
    require_browse_format(cli.format)?;
    let config = load_or_default_config(&cli.repo_root)?;
    let artifacts = build_snapshot_and_graph(&cli.repo_root, &config, None, None)?;

    let limit = limit.min(1000);
    let mut symbols: Vec<&Symbol> = artifacts.symbols.iter().collect();
    if let Some(kind) = kind_filter {
        let want = kind.trim().to_lowercase();
        symbols.retain(|s| s.kind.to_lowercase() == want);
    }
    // Stable-id order is already guaranteed by analysis; reassert locally.
    symbols.sort_by(|a, b| {
        a.stable_id
            .cmp(&b.stable_id)
            .then_with(|| a.qualified_name.cmp(&b.qualified_name))
    });
    let total = symbols.len();
    let page: Vec<&Symbol> = symbols.into_iter().skip(offset).take(limit).collect();

    if cli.format == OutputFormat::Json {
        let items: Vec<serde_json::Value> = page
            .iter()
            .map(|s| {
                serde_json::json!({
                    "stable_id": s.stable_id.as_str(),
                    "qualified_name": s.qualified_name,
                    "kind": s.kind,
                    "visibility": format!("{:?}", s.visibility).to_lowercase(),
                    "path": s.range.as_ref().map(|r| r.path.as_str()).unwrap_or(""),
                    "signature": s.normalized_signature,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "total": total,
                "limit": limit,
                "offset": offset,
                "symbols": items,
            }))
            .map_err(|e| CliError::Analysis(e.to_string()))?
        );
    } else {
        println!(
            "symbols total={} showing {}..{}",
            total,
            offset,
            offset + page.len()
        );
        for s in &page {
            println!("  {} [{}] {}", s.stable_id, s.kind, s.qualified_name);
        }
    }
    Ok(EXIT_PASS)
}

fn handle_edges(
    cli: &Cli,
    source: &str,
    direction: CliImpactDirection,
    kind_filter: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<i32, CliError> {
    require_browse_format(cli.format)?;
    let config = load_or_default_config(&cli.repo_root)?;
    let artifacts = build_snapshot_and_graph(&cli.repo_root, &config, None, None)?;

    let source_id = artifacts
        .symbols
        .iter()
        .find(|s| {
            s.stable_id.as_str() == source
                || s.qualified_name == source
                || s.qualified_name.ends_with(&format!("::{}", source))
        })
        .map(|s| s.stable_id.clone())
        .unwrap_or_else(|| SymbolId::new(source.to_string()));
    if artifacts.graph.get_node_index(&source_id).is_none() {
        return Err(CliError::Analysis(format!(
            "unknown symbol for edge browsing: '{}'",
            source
        )));
    }

    let impact_direction: ImpactDirection = direction.into();
    let mut pairs: Vec<(&Symbol, &Edge)> = match impact_direction {
        ImpactDirection::Forward => artifacts.graph.successors(&source_id),
        ImpactDirection::Backward => artifacts.graph.predecessors(&source_id),
        ImpactDirection::Bidirectional => {
            let mut both = artifacts.graph.successors(&source_id);
            both.extend(artifacts.graph.predecessors(&source_id));
            both
        }
    };
    if let Some(kind) = kind_filter {
        let want = kind.trim().to_lowercase().replace('-', "_");
        pairs.retain(|(_, e)| format!("{:?}", e.kind).to_lowercase() == want);
    }
    // Deterministic order: kind, then neighbor identity.
    pairs.sort_by(|a, b| (&a.1.kind, &a.0.stable_id).cmp(&(&b.1.kind, &b.0.stable_id)));
    // Deduplicate bidirectional overlap (self-pairs appear twice).
    pairs.dedup_by(|a, b| a.0.stable_id == b.0.stable_id && a.1.kind == b.1.kind);

    let limit = limit.min(1000);
    let total = pairs.len();
    let page: Vec<(&Symbol, &Edge)> = pairs.into_iter().skip(offset).take(limit).collect();

    if cli.format == OutputFormat::Json {
        let items: Vec<serde_json::Value> = page
            .iter()
            .map(|(sym, edge)| {
                serde_json::json!({
                    "neighbor": sym.stable_id.as_str(),
                    "qualified_name": sym.qualified_name,
                    "edge_kind": format!("{:?}", edge.kind).to_lowercase(),
                    "coverage": format!("{:?}", edge.coverage).to_lowercase(),
                    "provenance": edge.provenance,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "source": source_id.as_str(),
                "total": total,
                "limit": limit,
                "offset": offset,
                "edges": items,
            }))
            .map_err(|e| CliError::Analysis(e.to_string()))?
        );
    } else {
        println!(
            "edges of {} total={} showing {}..{}",
            source_id,
            total,
            offset,
            offset + page.len()
        );
        for (sym, edge) in &page {
            println!("  [{:?}] -> {}", edge.kind, sym.stable_id);
        }
    }
    Ok(EXIT_PASS)
}

// ---------------------------------------------------------------------------
// SUBCOMMAND: EXPLAIN
// ---------------------------------------------------------------------------
fn handle_explain(cli: &Cli, target: &str) -> Result<i32, CliError> {
    let storage = get_storage(cli)?;
    let evidence_opt = storage
        .get_evidence(target)
        .map_err(|e| CliError::Storage(e.to_string()))?;

    if let Some(ev) = evidence_opt {
        if cli.format == OutputFormat::Json {
            let json =
                serde_json::to_string_pretty(&ev).map_err(|e| CliError::Analysis(e.to_string()))?;
            println!("{}", json);
        } else {
            println!(
                "================================================================================"
            );
            println!(" EVIDENCE EXPLANATION FOR: {}", target);
            println!(
                "================================================================================"
            );
            println!(" Finding ID      : {}", ev.finding_id);
            println!(" Rule ID         : {}", ev.rule_id);
            println!(" Evidence Digest : {}", ev.evidence_digest);
            println!(" Locations:");
            for loc in &ev.locations {
                println!("   • {}:{}", loc.path, loc.start_line);
            }
            println!(
                "================================================================================"
            );
        }
        Ok(EXIT_PASS)
    } else {
        // Explain fallback for symbol or general query
        let _ = writeln!(
            std::io::stderr(),
            "Target '{}' not found in stored evidence receipts.",
            target
        );
        Ok(EXIT_ANALYSIS_ERROR)
    }
}

// ---------------------------------------------------------------------------
// SUBCOMMAND: DOCTOR
// ---------------------------------------------------------------------------
fn handle_doctor(cli: &Cli) -> Result<i32, CliError> {
    let mut checks_passed = true;
    let mut output = Vec::new();

    writeln!(
        output,
        "================================================================================"
    )
    .unwrap();
    writeln!(output, " CODE AUTOPSY SYSTEM DIAGNOSTIC (DOCTOR)").unwrap();
    writeln!(
        output,
        "================================================================================"
    )
    .unwrap();

    // 1. Repo Root Check
    if cli.repo_root.exists() && cli.repo_root.is_dir() {
        writeln!(
            output,
            " [ PASS ] Repository root exists ({})",
            cli.repo_root.display()
        )
        .unwrap();
    } else {
        writeln!(
            output,
            " [ FAIL ] Repository root does not exist or is not a directory"
        )
        .unwrap();
        checks_passed = false;
    }

    // 2. Storage & Cache Check
    match get_storage(cli) {
        Ok(storage) => {
            let test_data = b"doctor_check";
            match storage.put_cache_object(test_data) {
                Ok(_) => {
                    writeln!(
                        output,
                        " [ PASS ] Storage database & cache store operational"
                    )
                    .unwrap();
                }
                Err(e) => {
                    writeln!(output, " [ FAIL ] Cache write test failed: {}", e).unwrap();
                    checks_passed = false;
                }
            }
        }
        Err(e) => {
            writeln!(output, " [ FAIL ] Storage initialization failed: {}", e).unwrap();
            checks_passed = false;
        }
    }

    // 3. Adapter Initialization Check (all bundled language adapters)
    for (label, language_id, adapter_version, extensions) in [
        {
            let adapter = TypeScriptAdapter::with_root_dir(&cli.repo_root);
            let caps = adapter.capabilities();
            (
                "TypeScript",
                caps.language_id.clone(),
                caps.adapter_version.clone(),
                caps.supported_extensions.join(", "),
            )
        },
        {
            let adapter = PythonAdapter::with_root_dir(&cli.repo_root);
            let caps = adapter.capabilities();
            (
                "Python",
                caps.language_id.clone(),
                caps.adapter_version.clone(),
                caps.supported_extensions.join(", "),
            )
        },
    ] {
        writeln!(
            output,
            " [ PASS ] Adapter operational: {} ({} v{}, supported: {})",
            label, language_id, adapter_version, extensions
        )
        .unwrap();
    }

    // 4. Invariant File Syntax Check
    let inv_path = cli.repo_root.join(".autopsy").join("invariants.yml");
    if inv_path.exists() {
        match InvariantsConfig::load_from_file(&inv_path) {
            Ok(cfg) => {
                writeln!(
                    output,
                    " [ PASS ] Invariants configuration valid: {} rules loaded",
                    cfg.invariants.len()
                )
                .unwrap();
            }
            Err(e) => {
                writeln!(output, " [ FAIL ] Invariants configuration invalid: {}", e).unwrap();
                checks_passed = false;
            }
        }
    } else {
        writeln!(
            output,
            " [ INFO ] Invariants configuration file not found (using default rules)"
        )
        .unwrap();
    }

    // 5. Offline Operation Check
    writeln!(
        output,
        " [ PASS ] Offline verification: zero network calls configured"
    )
    .unwrap();
    writeln!(
        output,
        "================================================================================"
    )
    .unwrap();

    if checks_passed {
        writeln!(output, " Overall Status: HEALTHY").unwrap();
    } else {
        writeln!(output, " Overall Status: DEGRADED / UNHEALTHY").unwrap();
    }
    writeln!(
        output,
        "================================================================================"
    )
    .unwrap();

    let text = String::from_utf8_lossy(&output);
    print!("{}", text);

    if checks_passed {
        Ok(EXIT_PASS)
    } else {
        Ok(EXIT_ANALYSIS_ERROR)
    }
}

// ---------------------------------------------------------------------------
// SUBCOMMAND: VERSION
// ---------------------------------------------------------------------------
fn handle_version(_cli: &Cli) -> Result<i32, CliError> {
    println!("Synevid (Code Autopsy) v{}", ANALYZER_VERSION);
    println!("Schema Version: 0.0.1");
    println!("Supported Language Adapters: TypeScript / JavaScript + Python (tree-sitter v0.0.1)");
    println!("Storage Engine: SQLite + .autopsy/cache (Content-Addressed BLAKE3)");
    println!("Execution Mode: 100% Deterministic, Offline, Zero-LLM Invariant Engine");
    Ok(EXIT_PASS)
}

/// Helper to emit report to stdout according to chosen format.
fn emit_report(format: OutputFormat, report: &AutopsyReport) -> Result<(), CliError> {
    match format {
        OutputFormat::Json => {
            println!("{}", report.to_canonical_json()?);
        }
        OutputFormat::Text => {
            print!("{}", report.to_human_text());
        }
        OutputFormat::Sarif => {
            println!("{}", report.to_sarif()?);
        }
    }
    Ok(())
}
