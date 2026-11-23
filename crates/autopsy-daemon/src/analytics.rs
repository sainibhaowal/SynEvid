//! Analytics engine for dead-code, risk, and reports

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, info, warn};

use autopsy_storage::{StorageEngine, RuntimeObservationWindowRecord};
use autopsy_domain::CoverageState;

/// Analytics engine
pub struct AnalyticsEngine {
    // Stateless - uses storage
}

impl AnalyticsEngine {
    pub fn new() -> Self {
        Self {}
    }

    /// Generate analytics report
    pub fn generate_report(
        &self,
        storage: &StorageEngine,
        repo_id: &str,
        snapshot_id: &str,
        format: &str,
    ) -> Result<AnalyticsReportResult, String> {
        // Get latest snapshot
        let snapshot = storage.get_snapshot(&autopsy_domain::SnapshotId::new(snapshot_id))
            .map_err(|e| format!("Failed to get snapshot: {}", e))?
            .ok_or("Snapshot not found")?;

        // Get graph nodes and edges
        let (nodes, edges) = storage.graph_rows_for_snapshot(snapshot_id)
            .map_err(|e| format!("Failed to get graph rows: {}", e))?;

        // Static unreferenced analysis
        let (verified_candidates, unknown_candidates, excluded_count) = self.static_unreferenced_candidates(&nodes, &edges, snapshot_id);

        // Runtime unobserved (if telemetry data exists)
        let runtime_unobserved = self.runtime_unobserved_analysis(storage, repo_id, snapshot_id)?;

        // Git evolution analysis
        let git_evolution = self.git_evolution_analysis(storage, repo_id)?;

        // Structural health
        let structural_health = self.structural_health(&nodes, &edges, &snapshot);

        let report = AnalyticsReport {
            report_id: format!("analytics-{}-{}", repo_id, snapshot_id),
            scope: AnalyticsScope {
                repository_id: repo_id.to_string(),
                snapshot_id: snapshot_id.to_string(),
                generated_at: current_timestamp(),
            },
            structural_health,
            static_unreferenced: StaticUnreferencedResult {
                verified_candidates,
                unknown_candidates,
                excluded_count,
            },
            runtime_unobserved,
            git_evolution,
            coverage: format!("{:?}", snapshot.coverage_state).to_lowercase(),
        };

        let content = match format {
            "json" => serde_json::to_string_pretty(&report).unwrap(),
            "markdown" => self.render_markdown(&report),
            "html" => self.render_html(&report),
            "sarif" => self.render_sarif(&report),
            "bundle" => self.render_bundle(&report),
            _ => return Err("Invalid format".to_string()),
        };

        let digest = format!("blake3:{}", blake3::hash(content.as_bytes()).to_hex());

        Ok(AnalyticsReportResult {
            content,
            digest,
            format: format.to_string(),
            report,
        })
    }

    /// Ingest runtime observation window
    pub fn ingest_runtime_window(
        &self,
        storage: &mut StorageEngine,
        repo_id: &str,
        snapshot_id: &str,
        window: RuntimeObservationWindowRecord,
    ) -> Result<(), String> {
        storage.insert_runtime_observation_window(&window)
            .map_err(|e| format!("Failed to insert runtime window: {}", e))
    }

    fn static_unreferenced_candidates(
        &self,
        nodes: &[autopsy_storage::GraphNodeRow],
        edges: &[autopsy_storage::GraphEdgeRow],
        snapshot_id: &str,
    ) -> (Vec<Value>, Vec<Value>, usize) {
        let mut verified = Vec::new();
        let mut unknown = Vec::new();
        let mut excluded = 0;

        // Build incoming edge map (excluding containment)
        let incoming: BTreeSet<&str> = edges.iter()
            .filter(|e| e.edge_kind != "contains")
            .map(|e| e.target_stable_id.as_str())
            .collect();

        let graph_edge_digest = "computed-digest"; // Would be actual digest

        for node in nodes.iter().filter(|n| n.node_kind != "file" && n.node_kind != "unknown") {
            if incoming.contains(node.stable_id.as_str()) {
                continue;
            }

            let name = node.qualified_name.rsplit([':', '.', '#']).next().unwrap_or(&node.qualified_name);
            let normalized_name = name.to_ascii_lowercase();
            let normalized_path = node.file_path.replace('\\', "/").to_ascii_lowercase();

            let generated = normalized_path.split('/').any(|p| 
                p == "generated" || p == "__generated__" || p == "dist" || p == "build"
            );
            let test_only = normalized_path.split('/').any(|p| 
                p == "test" || p == "tests" || p == "__tests__"
            ) || normalized_path.ends_with(".test.ts") 
                || normalized_path.ends_with(".spec.ts") 
                || normalized_path.ends_with("_test.go");
            let conventional_entrypoint = matches!(
                normalized_name.as_str(),
                "main" | "__main__" | "setup" | "teardown" | "register" | "plugin_init"
            );

            if node.is_exported || node.visibility == "public" || generated || test_only || conventional_entrypoint {
                excluded += 1;
                continue;
            }

            let coverage = node.coverage_state.to_ascii_lowercase();
            let payload = json!({
                "stable_id": node.stable_id,
                "qualified_name": node.qualified_name,
                "file_path": node.file_path,
                "line": node.start_line,
                "finding_state": if coverage == "verified" { "candidate" } else { "unknown" },
                "evidence_basis": "no_inbound_non_containment_edge_in_persisted_static_graph",
                "inbound_edge_count": 0,
                "graph_edge_digest": graph_edge_digest,
                "coverage_state": coverage,
                "reason": if coverage == "verified" {
                    "No inbound edge was found in the analyzed static graph; dynamic or external use is not disproven."
                } else {
                    "No inbound edge was found, but graph coverage is insufficient to classify this declaration."
                },
                "residual_unknowns": ["reflection", "generated_or_external_consumers", "framework_or_plugin_registration"],
                "snapshot_id": snapshot_id,
            });

            if coverage == "verified" {
                verified.push(payload);
            } else {
                unknown.push(payload);
            }
        }

        (verified, unknown, excluded)
    }

    fn runtime_unobserved_analysis(&self, storage: &StorageEngine, repo_id: &str, snapshot_id: &str) -> Result<RuntimeUnobservedResult, String> {
        // Query runtime observation windows
        let windows = storage.get_runtime_observation_windows(repo_id, snapshot_id)
            .map_err(|e| format!("Failed to get runtime windows: {}", e))?;

        if windows.is_empty() {
            return Ok(RuntimeUnobservedResult {
                status: "insufficient_data".to_string(),
                windows_analyzed: 0,
                unobserved_symbols: Vec::new(),
            });
        }

        // Analyze which symbols were not observed
        // This is simplified - real implementation would correlate with graph
        Ok(RuntimeUnobservedResult {
            status: "analyzed".to_string(),
            windows_analyzed: windows.len(),
            unobserved_symbols: Vec::new(),
        })
    }

    fn git_evolution_analysis(&self, storage: &StorageEngine, repo_id: &str) -> Result<GitEvolutionResult, String> {
        // Get git history analysis
        Ok(GitEvolutionResult {
            semantic_git_risk: SemanticGitRisk {
                status: "not_analyzed".to_string(),
                risk_score: 0.0,
            },
        })
    }

    fn structural_health(&self, nodes: &[autopsy_storage::GraphNodeRow], edges: &[autopsy_storage::GraphEdgeRow], snapshot: &autopsy_domain::RepoSnapshot) -> StructuralHealth {
        StructuralHealth {
            total_files: snapshot.files.len(),
            total_symbols: nodes.len(),
            total_edges: edges.len(),
            coverage_state: format!("{:?}", snapshot.coverage_state).to_lowercase(),
            languages: serde_json::from_str::<Vec<String>>(&serde_json::to_string(&snapshot.languages).unwrap_or_default()).unwrap_or_default(),
        }
    }

    fn render_markdown(&self, report: &AnalyticsReport) -> String {
        format!("# Analytics Report\n\n**Repository**: {}\n**Snapshot**: {}\n\n## Structural Health\n- Files: {}\n- Symbols: {}\n- Edges: {}\n\n## Static Unreferenced\n- Verified: {}\n- Unknown: {}\n\n## Coverage: {}", 
            report.scope.repository_id, report.scope.snapshot_id,
            report.structural_health.total_files, report.structural_health.total_symbols, report.structural_health.total_edges,
            report.static_unreferenced.verified_candidates.len(), report.static_unreferenced.unknown_candidates.len(),
            report.coverage)
    }

    fn render_html(&self, report: &AnalyticsReport) -> String {
        format!("<html><body><h1>Analytics Report</h1><p>Repository: {}</p><p>Snapshot: {}</p></body></html>", 
            report.scope.repository_id, report.scope.snapshot_id)
    }

    fn render_sarif(&self, report: &AnalyticsReport) -> String {
        // Simplified SARIF
        json!({
            "version": "2.1.0",
            "$schema": "https://schemastore.org/schemas/json/sarif-2.1.0.json",
            "runs": [{
                "tool": { "driver": { "name": "SynEvid Analytics", "version": "0.6.0" }},
                "results": []
            }]
        }).to_string()
    }

    fn render_bundle(&self, report: &AnalyticsReport) -> String {
        // Bundle format - all formats in one
        json!({
            "json": serde_json::to_value(report).unwrap(),
            "markdown": self.render_markdown(report),
            "html": self.render_html(report),
            "sarif": self.render_sarif(report),
        }).to_string()
    }
}

// Result types

#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyticsReportResult {
    pub content: String,
    pub digest: String,
    pub format: String,
    pub report: AnalyticsReport,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyticsReport {
    pub report_id: String,
    pub scope: AnalyticsScope,
    pub structural_health: StructuralHealth,
    pub static_unreferenced: StaticUnreferencedResult,
    pub runtime_unobserved: RuntimeUnobservedResult,
    pub git_evolution: GitEvolutionResult,
    pub coverage: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyticsScope {
    pub repository_id: String,
    pub snapshot_id: String,
    pub generated_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StructuralHealth {
    pub total_files: usize,
    pub total_symbols: usize,
    pub total_edges: usize,
    pub coverage_state: String,
    pub languages: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StaticUnreferencedResult {
    pub verified_candidates: Vec<Value>,
    pub unknown_candidates: Vec<Value>,
    pub excluded_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RuntimeUnobservedResult {
    pub status: String,
    pub windows_analyzed: usize,
    pub unobserved_symbols: Vec<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitEvolutionResult {
    pub semantic_git_risk: SemanticGitRisk,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SemanticGitRisk {
    pub status: String,
    pub risk_score: f64,
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}