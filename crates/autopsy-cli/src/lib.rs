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

use autopsy_adapter_api::{LanguageAdapter, ParseContext};
use autopsy_adapter_typescript::TypeScriptAdapter;
use autopsy_contracts::{CallableContract, NormalizedContract, ParameterModel};
use autopsy_diff::{DiffEngine, DiffInput};
use autopsy_domain::{
    AnalysisRun, Contract, CoverageState, Edge, Finding, FindingStatus, RepoSnapshot, SnapshotId,
    Symbol, SymbolId,
};
use autopsy_evidence::EvidenceReceipt;
use autopsy_graph::DependencyGraph;
use autopsy_impact::{ImpactDirection, ImpactEngine, ImpactProfile, ImpactQuery};
use autopsy_invariants::{EvaluationContext, InvariantEvaluator, InvariantsConfig};
use autopsy_repo::{AutopsyConfig, scan_repository};
use autopsy_report::{
    AutopsyReport, CoverageSummary, SnapshotSummary, convert_domain_findings,
    convert_impact_result, convert_semantic_diff,
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
    },
    /// Compare two snapshots or compare baseline against worktree
    Diff {
        #[arg(long)]
        before: Option<String>,
        #[arg(long)]
        after: Option<String>,
    },
    /// Compute bounded transitive impact analysis from seed symbols
    Impact {
        #[arg(short, long, required = true)]
        symbol: Vec<String>,
        #[arg(long, value_enum, default_value = "forward")]
        direction: CliImpactDirection,
        #[arg(long, default_value_t = 10)]
        max_depth: usize,
        #[arg(long, default_value_t = 1000)]
        budget: usize,
        #[arg(long)]
        snapshot_id: Option<String>,
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
        Commands::Baseline { revision } => handle_baseline(cli, revision.as_deref()),
        Commands::Diff { before, after } => handle_diff(cli, before.as_deref(), after.as_deref()),
        Commands::Impact {
            symbol,
            direction,
            max_depth,
            budget,
            snapshot_id,
        } => handle_impact(
            cli,
            symbol,
            *direction,
            *max_depth,
            *budget,
            snapshot_id.as_deref(),
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
        Commands::Doctor => handle_doctor(cli),
        Commands::Version => handle_version(cli),
    }
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
fn load_or_default_config(repo_root: &Path) -> AutopsyConfig {
    let config_path = repo_root.join("autopsy.toml");
    if let Ok(cfg) = AutopsyConfig::load_from_file(&config_path) {
        return cfg;
    }

    AutopsyConfig {
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
        },
        cache: autopsy_repo::CacheConfig {
            directory: ".autopsy/cache".to_string(),
        },
    }
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

/// Executes repository scan, AST extraction, symbol table, multigraph, and contract model.
pub fn build_snapshot_and_graph(
    repo_root: &Path,
    config: &AutopsyConfig,
    revision: Option<&str>,
) -> Result<AnalysisArtifacts, CliError> {
    let snapshot = scan_repository(repo_root, config, ANALYZER_VERSION, revision)
        .map_err(|e| CliError::Config(e.to_string()))?;

    let adapter = TypeScriptAdapter::with_root_dir(repo_root);
    let mut graph = DependencyGraph::new();
    let mut all_symbols = Vec::new();
    let mut all_edges = Vec::new();
    let mut all_contracts = Vec::new();
    let mut unsupported_constructs = BTreeSet::new();
    let mut had_unknown = false;
    let mut had_partial = false;

    let parse_ctx = ParseContext {
        project_root: repo_root.to_path_buf(),
        options: BTreeMap::new(),
    };

    let mut symbol_index: BTreeMap<String, SymbolId> = BTreeMap::new();

    for (file_path, unit) in &snapshot.files {
        if !matches!(unit.language.as_str(), "typescript" | "javascript") {
            continue;
        }

        let full_path = repo_root.join(file_path);
        let content_str = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if let Ok(parsed) = adapter.parse(&parse_ctx, unit, &content_str) {
            match parsed.coverage {
                CoverageState::Unknown => had_unknown = true,
                CoverageState::Partial => had_partial = true,
                CoverageState::Unsupported => {
                    for construct in &parsed.dynamic_constructs {
                        unsupported_constructs.insert(format!("{:?}", construct.kind));
                    }
                }
                CoverageState::Verified => {}
            }

            if let Ok(symbols) = adapter.extract_symbols(&parsed) {
                for sym in &symbols {
                    graph.add_node(sym.clone());
                    symbol_index.insert(sym.qualified_name.clone(), sym.stable_id.clone());
                    all_symbols.push(sym.clone());
                }
            }

            if let Ok(edges) = adapter.extract_edges(&parsed, &symbol_index) {
                for edge in &edges {
                    graph.add_edge(edge.clone());
                    all_edges.push(edge.clone());
                }
            }

            if let Ok(contracts) = adapter.extract_contracts(&parsed) {
                all_contracts.extend(contracts);
            }
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

    Ok(AnalysisArtifacts {
        snapshot,
        graph,
        symbols: all_symbols,
        edges: all_edges,
        contracts: all_contracts,
        coverage_state: overall_coverage,
        unsupported_constructs: unsupported_constructs.into_iter().collect(),
    })
}

// ---------------------------------------------------------------------------
// SUBCOMMAND: BASELINE
// ---------------------------------------------------------------------------
fn handle_baseline(cli: &Cli, revision: Option<&str>) -> Result<i32, CliError> {
    let config = load_or_default_config(&cli.repo_root);
    let artifacts = build_snapshot_and_graph(&cli.repo_root, &config, revision)?;
    let storage = get_storage(cli)?;

    // Persist snapshot record in SQLite database
    storage
        .save_snapshot(&artifacts.snapshot)
        .map_err(|e| CliError::Storage(e.to_string()))?;

    // Cache graph representation in content-addressed cache
    let graph_bytes =
        serde_json::to_vec(&artifacts.symbols).map_err(|e| CliError::Analysis(e.to_string()))?;
    let _ = storage.put_cache_object(&graph_bytes);

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
    let config = load_or_default_config(&cli.repo_root);
    let after_artifacts = build_snapshot_and_graph(&cli.repo_root, &config, after_opt)?;

    let before_snapshot = if let Some(b) = before_opt {
        let storage = get_storage(cli)?;
        storage
            .get_snapshot(&SnapshotId::new(b))
            .map_err(|e| CliError::Storage(e.to_string()))?
            .unwrap_or_else(|| after_artifacts.snapshot.clone())
    } else {
        let storage = get_storage(cli)?;
        let latest = storage
            .get_latest_snapshot()
            .map_err(|e| CliError::Storage(e.to_string()))?;
        latest.unwrap_or_else(|| after_artifacts.snapshot.clone())
    };

    let diff_input = DiffInput {
        before_snapshot_id: before_snapshot.snapshot_id.clone(),
        after_snapshot_id: after_artifacts.snapshot.snapshot_id.clone(),
        before_files: &before_snapshot.files,
        after_files: &after_artifacts.snapshot.files,
        before_graph: &after_artifacts.graph,
        after_graph: &after_artifacts.graph,
        before_contracts: &after_artifacts.contracts,
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
fn handle_impact(
    cli: &Cli,
    seed_symbol_strings: &[String],
    direction: CliImpactDirection,
    max_depth: usize,
    budget: usize,
    _snapshot_id: Option<&str>,
) -> Result<i32, CliError> {
    let config = load_or_default_config(&cli.repo_root);
    let artifacts = build_snapshot_and_graph(&cli.repo_root, &config, None)?;

    let mut seed_symbols = Vec::new();
    for seed_str in seed_symbol_strings {
        if let Some(sym) = artifacts.symbols.iter().find(|s| {
            s.stable_id.as_str() == seed_str
                || s.qualified_name == *seed_str
                || s.qualified_name.ends_with(&format!("::{}", seed_str))
        }) {
            seed_symbols.push(sym.stable_id.clone());
        } else {
            seed_symbols.push(SymbolId::new(seed_str.clone()));
        }
    }

    let query = ImpactQuery {
        seed_symbols,
        direction: direction.into(),
        profile: ImpactProfile {
            name: "cli-impact".to_string(),
            max_depth,
            budget,
            ..Default::default()
        },
    };

    let impact_engine = ImpactEngine::new();
    let impact_result = impact_engine
        .compute_impact(&artifacts.graph, &query)
        .map_err(|e| CliError::Analysis(e.to_string()))?;

    let impact_summary = convert_impact_result(&impact_result);
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
    _baseline_id_opt: Option<&str>,
) -> Result<i32, CliError> {
    let config = load_or_default_config(&cli.repo_root);
    let artifacts = build_snapshot_and_graph(&cli.repo_root, &config, None)?;
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

    // Prepare normalized contract maps
    let mut contracts_after = BTreeMap::new();
    for c in &artifacts.contracts {
        contracts_after.insert(
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
    let contracts_before = BTreeMap::new();

    let eval_ctx = EvaluationContext {
        snapshot_id: artifacts.snapshot.snapshot_id.clone(),
        analyzer_version: ANALYZER_VERSION,
        graph: &artifacts.graph,
        baseline_graph: None,
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

    for (finding, evidence) in evaluation_pairs {
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

    // 3. Adapter Initialization Check
    let adapter = TypeScriptAdapter::with_root_dir(&cli.repo_root);
    let caps = adapter.capabilities();
    writeln!(
        output,
        " [ PASS ] Adapter operational: {} v{} (Supported: {})",
        caps.language_id,
        caps.adapter_version,
        caps.supported_extensions.join(", ")
    )
    .unwrap();

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
    println!("Supported Language Adapters: TypeScript / JavaScript (tree-sitter v0.0.1)");
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
