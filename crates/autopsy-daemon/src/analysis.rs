//! Analysis engine for impact, verification, and graph operations

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use blake3;
use tracing::{debug, info, warn};

use autopsy_cli::{load_or_default_config, build_snapshot_and_graph, AnalysisArtifacts};
use autopsy_impact::{ImpactEngine, ImpactQuery, ImpactDirection, ImpactProfile, callers_profile, EdgeGrade};
use autopsy_invariants::{InvariantEvaluator, InvariantsConfig, EvaluationContext, FindingStatus};
use autopsy_graph::DependencyGraph;
use autopsy_domain::{CoverageState, SymbolId, Finding, FindingStatus as DomainFindingStatus};
use autopsy_report::AutopsyReport;

/// Analysis engine - stateless, uses daemon's repositories and storage
pub struct AnalysisEngine {
    // Configuration
    default_impact_profile: ImpactProfile,
    default_callers_profile: ImpactProfile,
}

impl AnalysisEngine {
    pub fn new() -> Self {
        Self {
            default_impact_profile: ImpactProfile::default(),
            default_callers_profile: callers_profile(64, 10_000),
        }
    }

    /// Build snapshot and graph for a repository
    pub fn build_artifacts(&self, repo_root: &Path, config: &autopsy_repo::AutopsyConfig) -> Result<AnalysisArtifacts, String> {
        build_snapshot_and_graph(repo_root, config, None, None)
            .map_err(|e| format!("Analysis failed: {}", e))
    }

    /// Compute impact analysis
    pub fn compute_impact(
        &self,
        graph: &DependencyGraph,
        seeds: Vec<String>,
        direction: ImpactDirection,
        profile: Option<ImpactProfile>,
        max_depth: Option<usize>,
        budget: Option<usize>,
    ) -> Result<ImpactResult, String> {
        let engine = ImpactEngine::new();
        
        let query = ImpactQuery {
            seed_symbols: seeds.into_iter().map(SymbolId::new).collect(),
            direction,
            profile: profile.unwrap_or_else(|| self.default_impact_profile.clone()),
        };

        let report = engine.compute_impact(graph, &query)
            .map_err(|e| format!("Impact computation failed: {}", e))?;

        // Determine verdict based on evidence
        let has_unknown = report.entity_meta.values().any(|m| m.grade == EdgeGrade::Unrated);
        let has_partial = report.entity_meta.values().any(|m| matches!(m.grade, EdgeGrade::NameLevel | EdgeGrade::Inferred));

        let verdict = if report.truncated || has_unknown {
            "UNKNOWN"
        } else if has_partial {
            "WARN"
        } else {
            "PASS"
        };

        let coverage = if has_unknown { "unknown" } else if has_partial { "partial" } else { "verified" };

        Ok(ImpactResult {
            verdict: verdict.to_string(),
            coverage: coverage.to_string(),
            truncated: report.truncated,
            impact_digest: report.impact_digest,
            direction: format!("{:?}", direction).to_lowercase(),
            profile: report.traversal_profile,
            budget: query.profile.budget,
            likely_affected: report.likely_affected.into_iter().map(|s| s.as_str().to_string()).collect(),
            possibly_affected: report.possibly_affected.into_iter().map(|s| s.as_str().to_string()).collect(),
            entity_meta: report.entity_meta.into_iter().map(|(k, v)| (k.as_str().to_string(), v)).collect(),
        })
    }

    /// Verify invariants
    pub fn verify_invariants(
        &self,
        artifacts: &AnalysisArtifacts,
        invariants_config: &InvariantsConfig,
    ) -> Result<VerifyResult, String> {
        let eval_ctx = EvaluationContext {
            snapshot_id: artifacts.snapshot.snapshot_id.clone(),
            analyzer_version: autopsy_cli::ANALYZER_VERSION,
            graph: &artifacts.graph,
            baseline_graph: None,
            diff: None,
            contracts_before: &std::collections::BTreeMap::new(),
            contracts_after: &std::collections::BTreeMap::new(),
            coverage_state: artifacts.coverage_state,
        };

        let evaluator = InvariantEvaluator::new();
        let pairs = evaluator.evaluate(invariants_config, &eval_ctx)
            .map_err(|e| format!("Invariant evaluation failed: {}", e))?;

        let mut findings = Vec::new();
        let mut fail_count = 0;
        let mut unknown_count = 0;

        for (f, _) in pairs {
            let status = match f.status {
                DomainFindingStatus::Pass => "PASS",
                DomainFindingStatus::Fail => { fail_count += 1; "FAIL" },
                DomainFindingStatus::Unknown => { unknown_count += 1; "UNKNOWN" },
            };

            findings.push(VerifyFinding {
                rule_id: f.rule_id,
                status: status.to_string(),
                severity: format!("{:?}", f.severity).to_uppercase(),
                message: f.message,
                entities: f.entities,
            });
        }

        let verdict = if fail_count > 0 { "FAIL" } else if unknown_count > 0 { "UNKNOWN" } else { "PASS" };

        Ok(VerifyResult {
            verdict: verdict.to_string(),
            coverage: format!("{:?}", artifacts.coverage_state).to_lowercase(),
            findings,
        })
    }

    /// List symbols
    pub fn list_symbols(
        &self,
        graph: &DependencyGraph,
        limit: usize,
        offset: usize,
        kind_filter: Option<&str>,
    ) -> Vec<SymbolInfo> {
        let mut symbols: Vec<_> = graph.all_symbols_sorted()
            .into_iter()
            .filter(|s| kind_filter.map(|k| s.kind == k).unwrap_or(true))
            .skip(offset)
            .take(limit)
            .map(|s| SymbolInfo {
                stable_id: s.stable_id.as_str().to_string(),
                qualified_name: s.qualified_name.clone(),
                kind: s.kind.clone(),
                file_path: s.range.as_ref().map(|r| r.path.clone()).unwrap_or_default(),
                start_line: s.range.as_ref().map(|r| r.start_line as i64).unwrap_or(0),
                end_line: s.range.as_ref().map(|r| r.end_line as i64).unwrap_or(0),
            })
            .collect();

        // Sort by stable_id for deterministic ordering
        symbols.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
        symbols
    }

    /// Find symbols by pattern
    pub fn find_symbols(
        &self,
        graph: &DependencyGraph,
        pattern: &str,
        kind_filter: Option<&str>,
        name_only: bool,
        limit: usize,
        offset: usize,
    ) -> Vec<SymbolInfo> {
        let pattern_lower = pattern.to_lowercase();
        
        let mut symbols: Vec<_> = graph.all_symbols_sorted()
            .into_iter()
            .filter(|s| {
                if let Some(k) = kind_filter {
                    if s.kind != k { return false; }
                }
                
                let qualified_lower = s.qualified_name.to_lowercase();
                let path_lower = s.range.as_ref().map(|r| r.path.to_lowercase()).unwrap_or_default();
                
                if name_only {
                    qualified_lower.contains(&pattern_lower) || path_lower.contains(&pattern_lower)
                } else {
                    qualified_lower.contains(&pattern_lower) || 
                    path_lower.contains(&pattern_lower) ||
                    s.stable_id.as_str().to_lowercase().contains(&pattern_lower)
                }
            })
            .skip(offset)
            .take(limit)
            .map(|s| SymbolInfo {
                stable_id: s.stable_id.as_str().to_string(),
                qualified_name: s.qualified_name.clone(),
                kind: s.kind.clone(),
                file_path: s.range.as_ref().map(|r| r.path.clone()).unwrap_or_default(),
                start_line: s.range.as_ref().map(|r| r.start_line as i64).unwrap_or(0),
                end_line: s.range.as_ref().map(|r| r.end_line as i64).unwrap_or(0),
            })
            .collect();

        symbols.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
        symbols
    }

    /// Get graph map (bounded projection)
    pub fn get_graph_map(&self, graph: &DependencyGraph, top: usize, under: Option<&str>) -> GraphMapResult {
        let mut nodes = graph.all_symbols_sorted();
        
        if let Some(prefix) = under {
            nodes.retain(|s| {
                s.range.as_ref().map(|r| r.path.starts_with(prefix)).unwrap_or(false)
            });
        }

        let total_nodes = nodes.len();
        let total_edges = graph.edge_count();

        // Calculate degrees
        let mut degree_map: BTreeMap<String, usize> = BTreeMap::new();
        for edge in graph.all_edges_sorted() {
            *degree_map.entry(edge.source.as_str().to_string()).or_default() += 1;
            *degree_map.entry(edge.target.as_str().to_string()).or_default() += 1;
        }

        // Sort by degree, then stable_id
        nodes.sort_by(|a, b| {
            let deg_a = degree_map.get(a.stable_id.as_str()).copied().unwrap_or(0);
            let deg_b = degree_map.get(b.stable_id.as_str()).copied().unwrap_or(0);
            deg_b.cmp(&deg_a).then_with(|| a.stable_id.as_str().cmp(b.stable_id.as_str()))
        });

        let top_nodes: Vec<_> = nodes.into_iter().take(top).map(|s| GraphMapNode {
            stable_id: s.stable_id.as_str().to_string(),
            qualified_name: s.qualified_name.clone(),
            kind: s.kind.clone(),
            degree: degree_map.get(s.stable_id.as_str()).copied().unwrap_or(0),
        }).collect();

        // Kind counts
        let mut kind_counts: BTreeMap<String, usize> = BTreeMap::new();
        for s in graph.all_symbols_sorted() {
            *kind_counts.entry(s.kind.clone()).or_default() += 1;
        }

        // Package counts (simplified - by first path component)
        let mut package_counts: BTreeMap<String, usize> = BTreeMap::new();
        for s in graph.all_symbols_sorted() {
            if let Some(path) = s.range.as_ref().map(|r| &r.path) {
                let pkg = path.split('/').next().unwrap_or("root").to_string();
                *package_counts.entry(pkg).or_default() += 1;
            }
        }

        GraphMapResult {
            total_nodes,
            total_edges,
            top_nodes,
            kind_counts,
            package_counts,
        }
    }

    /// List edges for a symbol
    pub fn list_edges(
        &self,
        graph: &DependencyGraph,
        source: &str,
        direction: ImpactDirection,
        kind_filter: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Vec<EdgeInfo> {
        let source_id = SymbolId::new(source);
        let mut edges = Vec::new();

        for edge in graph.all_edges_sorted() {
            let matches = match direction {
                ImpactDirection::Forward => edge.source == source_id,
                ImpactDirection::Backward => edge.target == source_id,
                ImpactDirection::Bidirectional => edge.source == source_id || edge.target == source_id,
            };

            if !matches { continue; }
            if let Some(k) = kind_filter {
                if format!("{:?}", edge.kind).to_lowercase() != k.to_lowercase() { continue; }
            }

            edges.push(EdgeInfo {
                source: edge.source.as_str().to_string(),
                target: edge.target.as_str().to_string(),
                kind: format!("{:?}", edge.kind),
                file_path: edge.location.as_ref().map(|l| l.path.clone()).unwrap_or_default(),
                start_line: edge.location.as_ref().map(|l| l.start_line as i64).unwrap_or(0),
            });
        }

        edges.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.target.cmp(&b.target)));
        edges.into_iter().skip(offset).take(limit).collect()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImpactResult {
    pub verdict: String,
    pub coverage: String,
    pub truncated: bool,
    pub impact_digest: String,
    pub direction: String,
    pub profile: String,
    pub budget: usize,
    pub likely_affected: Vec<String>,
    pub possibly_affected: Vec<String>,
    pub entity_meta: BTreeMap<String, autopsy_impact::EntityMeta>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VerifyResult {
    pub verdict: String,
    pub coverage: String,
    pub findings: Vec<VerifyFinding>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VerifyFinding {
    pub rule_id: String,
    pub status: String,
    pub severity: String,
    pub message: String,
    pub entities: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SymbolInfo {
    pub stable_id: String,
    pub qualified_name: String,
    pub kind: String,
    pub file_path: String,
    pub start_line: i64,
    pub end_line: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EdgeInfo {
    pub source: String,
    pub target: String,
    pub kind: String,
    pub file_path: String,
    pub start_line: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GraphMapResult {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub top_nodes: Vec<GraphMapNode>,
    pub kind_counts: BTreeMap<String, usize>,
    pub package_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GraphMapNode {
    pub stable_id: String,
    pub qualified_name: String,
    pub kind: String,
    pub degree: usize,
}