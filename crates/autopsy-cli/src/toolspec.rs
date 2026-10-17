//! The machine-readable tool specification (Tier 3.1): `autopsy tools --format json`.
//!
//! One canonical document describing every subcommand — its purpose, flags, the
//! exit-code contract, its snapshot-pinning surface, and the envelope the MCP layer
//! wraps around it. Runtime-discoverable, byte-identical across runs on the same
//! build (`BTreeMap` ordering, no timestamps), so an agent can learn the whole
//! surface from the engine instead of a hand-copied doc drifting out of date.
//!
//! `docs/agents/TOOLS.md` is the human rendering of this same contract.

use crate::{Cli, CliError, EXIT_PASS, OutputFormat};
use serde::Serialize;

pub const TOOLSPEC_FORMAT_VERSION: &str = "0.0.1";

/// One invocable argument of a tool. A `flag` takes no value; `value` takes one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolArg {
    pub flag: String,
    pub kind: &'static str,
    pub description: String,
}

/// One tool of the engine surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolSpec {
    pub name: String,
    pub purpose: String,
    pub args: Vec<ToolArg>,
    /// The flag(s) that pin this tool to a stored snapshot, when supported.
    pub pinning: String,
    /// Whether `--compact`/`--top-k` token budgets are defined for this tool.
    pub compact: bool,
    /// Coverage semantics: what `verified`/`partial`/`unknown` mean here.
    pub coverage_semantics: String,
}

/// The full contract document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Toolspec {
    pub format_version: String,
    pub analyzer_version: String,
    /// The one envelope every answer travels in (CLI reports and MCP alike).
    pub envelope: serde_json::Value,
    /// The engine's exit-code contract; part of the answer, not transport metadata.
    pub exit_codes: std::collections::BTreeMap<String, String>,
    /// Deterministic repair advice summary: every fatal error prints this triple.
    pub error_advice: Vec<&'static str>,
    /// Agent correction receipts (Tier 3.3).
    pub feedback: serde_json::Value,
    pub tools: Vec<ToolSpec>,
}

fn arg(flag: &str, kind: &'static str, description: &str) -> ToolArg {
    ToolArg {
        flag: flag.to_string(),
        kind,
        description: description.to_string(),
    }
}

fn tool(
    name: &str,
    purpose: &str,
    args: Vec<ToolArg>,
    pinning: &str,
    compact: bool,
    coverage: &str,
) -> ToolSpec {
    ToolSpec {
        name: name.to_string(),
        purpose: purpose.to_string(),
        args,
        pinning: pinning.to_string(),
        compact,
        coverage_semantics: coverage.to_string(),
    }
}

/// Builds the full spec. Order is engine-canonical (analysis, browse, lifecycle,
/// evidence, bookkeeping) and stable across runs of the same build.
pub fn toolspec(analyzer_version: &str) -> Toolspec {
    let analysis_coverage = "verified = all analyzed files carried full adapter coverage; partial = a stated construct (dynamic/unsupported) limited the graph; unknown = the engine refuses to pretend";
    let evidence_coverage = "coverage states name how much of the evidence engine could verify; strict mode exits 4 on partial as well as unknown";

    Toolspec {
        format_version: TOOLSPEC_FORMAT_VERSION.to_string(),
        analyzer_version: analyzer_version.to_string(),
        envelope: serde_json::json!({
            "fields": {
                "command": "the subcommand that produced the answer",
                "exit_code": "0 pass, 2 policy fail with evidence, 3 analysis could not run, 4 coverage gate",
                "actionable": "true only when exit_code is 0",
                "report": "the engine report itself",
                "repair": "present only on failures: {what_failed, why, next_action} parsed from the engine's own stderr markers",
            }
        }),
        exit_codes: std::collections::BTreeMap::from([
            (
                "0".to_string(),
                "PASS: the answer is actionable".to_string(),
            ),
            (
                "2".to_string(),
                "POLICY FAIL: evidence was produced and the pinned policy rejected it".to_string(),
            ),
            (
                "3".to_string(),
                "ANALYSIS ERROR: the answer could not be produced; stderr carries repair advice"
                    .to_string(),
            ),
            (
                "4".to_string(),
                "COVERAGE GATE: coverage was partial/unknown and --strict was set".to_string(),
            ),
        ]),
        error_advice: vec!["what_failed", "why", "next_action"],
        feedback: serde_json::json!({
            "suggestion": "re-run any command with --suggestion \"<what actually fixed it>\" to store a deterministic correction receipt in .autopsy/suggestions.json",
            "disable": "--disable-suggestion suppresses hints and receipt storage",
        }),
        tools: vec![
            tool(
                "baseline",
                "Scan the repository, compute the deterministic snapshot, build the dependency multigraph, and (with --persist) store it",
                vec![
                    arg(
                        "--revision",
                        "value",
                        "Analyze this revision label instead of the worktree",
                    ),
                    arg(
                        "--incremental",
                        "flag",
                        "Reuse per-file cached artifacts for byte-identical files",
                    ),
                    arg(
                        "--baseline-id",
                        "value",
                        "Incremental reuse source (defaults to the latest stored snapshot)",
                    ),
                    arg(
                        "--persist",
                        "flag",
                        "Store the snapshot in .autopsy/storage.db",
                    ),
                ],
                "--incremental --baseline-id <id>",
                false,
                analysis_coverage,
            ),
            tool(
                "impact",
                "Deterministic bounded transitive impact analysis and blast radius for one symbol",
                vec![
                    arg("-s / --symbol", "value", "Target symbol name or stable id"),
                    arg(
                        "--direction",
                        "value",
                        "forward | backward | bidirectional (default forward)",
                    ),
                    arg("--max-depth", "value", "Traversal depth bound"),
                    arg(
                        "--budget",
                        "value",
                        "Visited-node bound before stated truncation",
                    ),
                    arg(
                        "--profile",
                        "value",
                        "full (default) or callers (precision-first shortlist)",
                    ),
                    arg("--edge-kinds", "value", "Comma-separated edge-kind filter"),
                    arg(
                        "--top-k",
                        "value",
                        "Ranked shortlist size (full evidence stays in the report)",
                    ),
                ],
                "--snapshot-id <id>",
                false,
                analysis_coverage,
            ),
            tool(
                "diff",
                "Semantic AST symbol delta between a stored baseline and a state (stored snapshot or current worktree)",
                vec![
                    arg("--before", "value", "Baseline snapshot ID"),
                    arg(
                        "--after",
                        "value",
                        "Stored after-state snapshot ID (default: current worktree)",
                    ),
                ],
                "--before <id>",
                false,
                analysis_coverage,
            ),
            tool(
                "verify",
                "Evaluate pinned architectural invariants and contract models against a snapshot or diff",
                vec![
                    arg(
                        "--baseline-id",
                        "value",
                        "Verify the diff against this stored baseline",
                    ),
                    arg(
                        "--invariants-file",
                        "value",
                        "Invariants YAML policy (default .autopsy/invariants.yml)",
                    ),
                    arg(
                        "--strict",
                        "flag",
                        "Exit 4 when coverage is partial as well as unknown",
                    ),
                ],
                "--baseline-id <id>",
                false,
                evidence_coverage,
            ),
            tool(
                "explain",
                "Explain evidence receipts, provenance paths, and rule derivations for a finding or symbol",
                vec![arg("<target>", "value", "Finding ID or stable symbol ID")],
                "(uses the stored state)",
                false,
                evidence_coverage,
            ),
            tool(
                "symbols",
                "Paginated symbol index in stable-id order with optional kind filter",
                vec![
                    arg("--limit", "value", "Page size (default 50, max 1000)"),
                    arg("--offset", "value", "Zero-based offset"),
                    arg(
                        "--kind",
                        "value",
                        "function | method | class | interface | type | enum | variable",
                    ),
                ],
                "--snapshot-id <id>",
                false,
                analysis_coverage,
            ),
            tool(
                "edges",
                "Paginated 1-hop neighborhood of one symbol",
                vec![
                    arg("--source", "value", "Stable symbol ID or qualified name"),
                    arg("--direction", "value", "in | out | bidirectional"),
                    arg("--kind", "value", "Edge-kind filter"),
                    arg("--limit", "value", "Page size (default 50, max 1000)"),
                    arg("--offset", "value", "Zero-based offset"),
                ],
                "--snapshot-id <id>",
                false,
                analysis_coverage,
            ),
            tool(
                "find",
                "Case-insensitive substring resolution over qualified names, paths, and stable IDs",
                vec![
                    arg(
                        "<pattern>",
                        "value",
                        "The text to match (blank is refused at exit 3)",
                    ),
                    arg("--kind", "value", "Restrict to one symbol kind"),
                    arg(
                        "--name-only",
                        "flag",
                        "Do not widen the match over stable IDs",
                    ),
                    arg("--limit", "value", "Page size (default 50, max 1000)"),
                    arg("--offset", "value", "Zero-based offset"),
                ],
                "--snapshot-id <id>",
                false,
                analysis_coverage,
            ),
            tool(
                "map",
                "Bounded whole-graph projection: kind/package/edge-kind counts and top-N nodes by degree",
                vec![
                    arg("--top", "value", "Top-N size (default 25, max 500)"),
                    arg(
                        "--under",
                        "value",
                        "Path-prefix scope that narrows every count and rank",
                    ),
                ],
                "--snapshot-id <id>",
                false,
                analysis_coverage,
            ),
            tool(
                "session",
                "Repair-loop receipts: record every evidence step, close with an honest verdict (Tier 2.5)",
                vec![
                    arg(
                        "start",
                        "subcommand",
                        "Open a session; the id is BLAKE3 of root + counter",
                    ),
                    arg(
                        "record",
                        "subcommand",
                        "Record one step: --session-id, --exit-code, optional --snapshot-id/--note, then the invoked argv after --",
                    ),
                    arg("show", "subcommand", "Print one session record"),
                    arg(
                        "close",
                        "subcommand",
                        "Close and emit the receipt; exit mirrors the verdict",
                    ),
                    arg("list", "subcommand", "List sessions with status"),
                ],
                "(steps carry --snapshot-id)",
                false,
                "the receipt verdict is derived only from recorded evidence steps",
            ),
            tool(
                "workspace",
                "Multi-repo registry with honest per-repo accessibility and analysis status (Tier 2.3)",
                vec![
                    arg(
                        "register",
                        "subcommand",
                        "Register a path with an optional --id",
                    ),
                    arg("remove", "subcommand", "Remove by --id"),
                    arg(
                        "list",
                        "subcommand",
                        "List with coverage counts and the workspace digest",
                    ),
                ],
                "(none)",
                false,
                "verified = all registered repos accessible; excluded/inaccessible repos degrade coverage",
            ),
            tool(
                "temporal",
                "Replay history: the revision journal of facts added and removed (Pillar 1)",
                vec![
                    arg("--at", "value", "Reconstruct the graph at one revision"),
                    arg(
                        "--max-commits",
                        "value",
                        "Bound on commits walked (a hit is refused, not truncated)",
                    ),
                    arg(
                        "--compact / --top-k",
                        "flag",
                        "Token budget: counts stay exact, arrays are sliced with a named residual",
                    ),
                ],
                "(worktree/git-bound)",
                true,
                evidence_coverage,
            ),
            tool(
                "lineage",
                "File identity across a revision pair: only byte-identical renames reach verified (Pillar 1)",
                vec![
                    arg(
                        "--from / --to",
                        "value",
                        "Revision pair (to defaults to HEAD)",
                    ),
                    arg(
                        "--max-files",
                        "value",
                        "Bound on files read (a hit is a stated lower bound)",
                    ),
                    arg(
                        "--threshold / --margin",
                        "value",
                        "Similarity floor and winning margin for candidates",
                    ),
                    arg("--compact / --top-k", "flag", "Token budget"),
                ],
                "(worktree/git-bound)",
                true,
                "verified = byte-identical identity; partial = similarity match with stated ambiguity",
            ),
            tool(
                "cochange",
                "Mined pairs of paths that historically changed together; co-change is co-occurrence, never causation (Pillar 5)",
                vec![
                    arg("--window-seconds", "value", "Co-change window width"),
                    arg(
                        "--min-pair-commits",
                        "value",
                        "Fewest joint commits before a pair is reported",
                    ),
                    arg("--max-commits", "value", "Bound on commits walked"),
                    arg("--compact / --top-k", "flag", "Token budget"),
                ],
                "(worktree/git-bound)",
                true,
                evidence_coverage,
            ),
            tool(
                "test-select",
                "Select the tests a changed file set requires, with the engine's own fallback and coverage state (Pillar 2)",
                vec![
                    arg(
                        "--changed",
                        "value",
                        "Changed source path, POSIX-relative (repeatable)",
                    ),
                    arg("--lcov", "value", "LCOV report per suite (repeatable)"),
                    arg(
                        "--fail-if-all-tests-required",
                        "flag",
                        "Exit 2 when the plan is run-all-tests",
                    ),
                    arg(
                        "--strict",
                        "flag",
                        "Exit 4 on partial coverage as well as unknown",
                    ),
                ],
                "(worktree-bound)",
                false,
                "partial = the selection could not narrow; run-all-tests is an honest fallback, not a guess",
            ),
            tool(
                "contract-dist",
                "Diff two revisions of one distributed contract (OpenAPI/AsyncAPI/Pact) with a breaking verdict (Pillar 3)",
                vec![
                    arg("<from> <to>", "value", "The earlier and later documents"),
                    arg(
                        "--protocol",
                        "value",
                        "openapi | asyncapi | asyncapi-flows | pact",
                    ),
                    arg("--compact / --top-k", "flag", "Token budget"),
                    arg(
                        "--strict",
                        "flag",
                        "Exit 4 when the verdict is unknown or coverage is partial",
                    ),
                ],
                "(none: document inputs)",
                true,
                "a compatible verdict is only ever returned with verified coverage",
            ),
            tool(
                "telemetry",
                "Correlate an OTLP/JSON trace export against a declared surface; unresolved routes degrade coverage (Pillar 3)",
                vec![
                    arg("<traces>", "value", "OTLP/JSON trace export path"),
                    arg(
                        "--salt",
                        "value",
                        "Exactly 64 hex characters; never defaulted",
                    ),
                    arg("--surface", "value", "Declared-surface JSON document"),
                    arg(
                        "--windows",
                        "value",
                        "Minimum observation windows a dead-code claim needs",
                    ),
                    arg("--compact / --top-k", "flag", "Token budget"),
                ],
                "(none: file inputs)",
                true,
                evidence_coverage,
            ),
            tool(
                "catalog",
                "Ingest Backstage catalog entities and dependency manifests (Pillar 6)",
                vec![
                    arg(
                        "--root",
                        "value",
                        "The tree to scan (defaults to the repository root)",
                    ),
                    arg("--max-file-bytes", "value", "Single-file size bound"),
                    arg(
                        "--strict / --compact / --top-k",
                        "flag",
                        "Coverage gate and token budget",
                    ),
                ],
                "(worktree-bound)",
                true,
                evidence_coverage,
            ),
            tool(
                "blast-radius",
                "Organisational blast radius of one catalog component: owners, siblings, APIs, dependents (Pillar 6)",
                vec![
                    arg("<component>", "value", "component:default/<name>"),
                    arg("--root", "value", "Catalog scan root"),
                    arg("--compact / --top-k", "flag", "Token budget"),
                    arg("--strict", "flag", "Exit 4 on partial coverage"),
                ],
                "(worktree-bound)",
                true,
                evidence_coverage,
            ),
            tool(
                "doctor",
                "Verify repository configuration, storage integrity, and analyzer health",
                vec![],
                "(none)",
                false,
                "health report, no coverage gate",
            ),
            tool(
                "tools",
                "Emit this specification (Tier 3.1)",
                vec![],
                "(none)",
                false,
                "self-describing; always verified",
            ),
            tool(
                "version",
                "Display the engine version and adapter capabilities",
                vec![],
                "(none)",
                false,
                "self-describing; always verified",
            ),
        ],
    }
}

/// Global flags every command accepts; appended to the JSON output rather than
/// duplicated into each tool entry.
pub fn global_flags() -> serde_json::Value {
    serde_json::json!({
        "-r / --repo-root": "Target repository root directory",
        "-f / --format": "text | json | sarif (SARIF is defined for finding reports only)",
        "--db-path / --cache-dir": "Storage and cache locations (defaults under .autopsy/)",
        "--wal": "SQLite write-ahead logging",
        "--persist": "Store the analysis in .autopsy/storage.db",
        "--suggestion": "Attach a correction receipt to this run (Tier 3.3)",
        "--disable-suggestion": "Suppress repair hints and suggestion storage",
    })
}

pub fn handle_tools(cli: &Cli) -> Result<i32, CliError> {
    let spec = toolspec(crate::ANALYZER_VERSION);
    if cli.format == OutputFormat::Json {
        let document = serde_json::json!({
            "format_version": spec.format_version,
            "analyzer_version": spec.analyzer_version,
            "envelope": spec.envelope,
            "exit_codes": spec.exit_codes,
            "error_advice": spec.error_advice,
            "feedback": spec.feedback,
            "global_flags": global_flags(),
            "tools": spec.tools,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&document)
                .map_err(|e| CliError::Analysis(e.to_string()))?
        );
    } else {
        println!(
            "Synevid tools v{} (spec format {}): {} tools",
            spec.analyzer_version,
            spec.format_version,
            spec.tools.len()
        );
        for t in &spec.tools {
            println!("  {:<13} {}", t.name, t.purpose);
        }
        println!("Run with --format json for the full machine-readable contract.");
    }
    Ok(EXIT_PASS)
}

/// Defensive self-check used by tests: the specification names every subcommand
/// the CLI actually accepts, so the spec cannot silently drift from the binary.
#[allow(dead_code)]
pub fn spec_covers_command(spec: &Toolspec, name: &str) -> bool {
    spec.tools.iter().any(|t| t.name == name)
}
