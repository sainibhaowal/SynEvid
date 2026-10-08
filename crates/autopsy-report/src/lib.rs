//! Autopsy Report Engine (FR-015, FR-017, FR-028)
//!
//! Provides deterministic canonical JSON formatting conforming strictly to
//! `schemas/autopsy-result.schema.json`, human-readable terminal summaries,
//! and SARIF 2.1.0 integration for CI and code-scanning systems.

use autopsy_diff::SemanticDiff;
use autopsy_domain::{CoverageState, Finding, Severity};
use autopsy_evidence::EvidenceReceipt;
use autopsy_impact::ImpactResult;
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReportError {
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Validation error: {0}")]
    Validation(String),
}

/// Root report model matching `schemas/autopsy-result.schema.json`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutopsyReport {
    pub schema_version: String,
    pub run: RunMetadata,
    pub snapshots: Vec<SnapshotSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_set: Option<ChangeSetSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impact: Option<ImpactSummary>,
    pub findings: Vec<FindingSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Vec<EvidenceSummary>>,
    pub coverage: CoverageSummary,
    pub result_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunMetadata {
    pub run_id: String,
    pub command: String,
    pub analyzer_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub result_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotSummary {
    pub snapshot_id: String,
    pub revision: String,
    pub config_hash: String,
    pub file_set_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSetSummary {
    pub before_snapshot_id: String,
    pub after_snapshot_id: String,
    pub file_deltas: Vec<serde_json::Value>,
    pub symbol_deltas: Vec<serde_json::Value>,
    pub contract_deltas: Vec<serde_json::Value>,
    pub edge_deltas: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactSummary {
    pub query: serde_json::Value,
    pub impacted_entities: Vec<String>,
    pub paths: Vec<serde_json::Value>,
    pub traversal_profile: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingSummary {
    pub id: String,
    pub rule_id: String,
    pub status: String,
    pub severity: String,
    pub message: String,
    pub entities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceSummary {
    pub finding_id: String,
    pub rule_id: String,
    pub locations: Vec<LocationSummary>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub paths: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub deltas: Vec<serde_json::Value>,
    pub evidence_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationSummary {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageSummary {
    pub state: String,
    pub unsupported_constructs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_scanned: Option<usize>,
}

impl AutopsyReport {
    /// Builds an AutopsyReport and calculates the canonical BLAKE3 result digest.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        run_id: String,
        command: String,
        analyzer_version: String,
        duration_ms: Option<u64>,
        snapshots: Vec<SnapshotSummary>,
        change_set: Option<ChangeSetSummary>,
        impact: Option<ImpactSummary>,
        findings: Vec<FindingSummary>,
        evidence: Option<Vec<EvidenceSummary>>,
        coverage: CoverageSummary,
    ) -> Result<Self, ReportError> {
        let mut report = Self {
            schema_version: "0.0.1".to_string(),
            run: RunMetadata {
                run_id,
                command,
                analyzer_version,
                duration_ms,
                result_digest: String::new(),
            },
            snapshots,
            change_set,
            impact,
            findings,
            evidence,
            coverage,
            result_digest: String::new(),
        };

        report.compute_and_set_digest()?;
        Ok(report)
    }

    /// Computes and sets the BLAKE3 digest of the canonical representation.
    pub fn compute_and_set_digest(&mut self) -> Result<String, ReportError> {
        // Zero out digest fields for repeatable calculation
        self.run.result_digest.clear();
        self.result_digest.clear();

        // Sort findings and snapshots deterministically
        self.snapshots
            .sort_by(|a, b| a.snapshot_id.cmp(&b.snapshot_id));
        self.findings.sort_by(|a, b| a.id.cmp(&b.id));
        if let Some(ref mut evs) = self.evidence {
            evs.sort_by(|a, b| a.finding_id.cmp(&b.finding_id));
        }

        let canonical_bytes = serde_json::to_vec(self)?;
        let mut hasher = Hasher::new();
        hasher.update(b"autopsy-result-v0.0.1:");
        hasher.update(&canonical_bytes);
        let digest = hasher.finalize().to_hex().to_string();

        self.run.result_digest = digest.clone();
        self.result_digest = digest.clone();

        Ok(digest)
    }

    /// Converts the report into canonical indented JSON.
    pub fn to_canonical_json(&self) -> Result<String, ReportError> {
        let val = serde_json::to_value(self)?;
        let sorted_val = sort_json_value(&val);
        Ok(serde_json::to_string_pretty(&sorted_val)?)
    }

    /// Converts the report to a human-readable text presentation for terminal consumption.
    pub fn to_human_text(&self) -> String {
        let mut out = String::new();
        out.push_str(
            "================================================================================\n",
        );
        out.push_str(&format!(
            " CODE AUTOPSY VERIFICATION REPORT — v{}\n",
            self.run.analyzer_version
        ));
        out.push_str(
            "================================================================================\n",
        );
        out.push_str(&format!(" Command         : {}\n", self.run.command));
        out.push_str(&format!(" Run ID          : {}\n", self.run.run_id));
        if let Some(ms) = self.run.duration_ms {
            out.push_str(&format!(" Duration        : {} ms\n", ms));
        }
        out.push_str(&format!(" Result Digest   : {}\n", self.result_digest));
        out.push_str(&format!(
            " Coverage State  : {}\n",
            self.coverage.state.to_uppercase()
        ));
        if let Some(files) = self.coverage.files_scanned {
            out.push_str(&format!(" Files Scanned   : {}\n", files));
        }
        if !self.coverage.unsupported_constructs.is_empty() {
            out.push_str(&format!(
                " Unsupported     : {}\n",
                self.coverage.unsupported_constructs.join(", ")
            ));
        }
        out.push_str(
            "--------------------------------------------------------------------------------\n",
        );

        if !self.snapshots.is_empty() {
            out.push_str(" SNAPSHOTS:\n");
            for s in &self.snapshots {
                out.push_str(&format!(
                    "   • [{}] rev: {} (config: {}, files: {})\n",
                    s.snapshot_id, s.revision, s.config_hash, s.file_set_digest
                ));
            }
            out.push_str("--------------------------------------------------------------------------------\n");
        }

        if let Some(ref cs) = self.change_set {
            out.push_str(" CHANGE SET:\n");
            out.push_str(&format!(
                "   • Files changed   : {}\n",
                cs.file_deltas.len()
            ));
            out.push_str(&format!(
                "   • Symbols changed : {}\n",
                cs.symbol_deltas.len()
            ));
            out.push_str(&format!(
                "   • Contracts diff  : {}\n",
                cs.contract_deltas.len()
            ));
            out.push_str(&format!(
                "   • Edges delta     : {}\n",
                cs.edge_deltas.len()
            ));
            out.push_str("--------------------------------------------------------------------------------\n");
        }

        if let Some(ref imp) = self.impact {
            out.push_str(" IMPACT ASSESSMENT:\n");
            out.push_str(&format!(
                "   • Traversal Profile : {}\n",
                imp.traversal_profile
            ));
            out.push_str(&format!(
                "   • Impacted Entities : {} (truncated: {})\n",
                imp.impacted_entities.len(),
                imp.truncated
            ));
            for entity in imp.impacted_entities.iter().take(10) {
                out.push_str(&format!("       -> {}\n", entity));
            }
            if imp.impacted_entities.len() > 10 {
                out.push_str(&format!(
                    "       ... and {} more entities\n",
                    imp.impacted_entities.len() - 10
                ));
            }
            out.push_str("--------------------------------------------------------------------------------\n");
        }

        out.push_str(&format!(" FINDINGS ({})\n", self.findings.len()));
        if self.findings.is_empty() {
            out.push_str("   No findings recorded. All invariants satisfied.\n");
        } else {
            for f in &self.findings {
                let badge = match f.status.to_lowercase().as_str() {
                    "pass" => "[ PASS ]",
                    "fail" => "[ FAIL ]",
                    _ => "[UNKNOWN]",
                };
                let sev = f.severity.to_uppercase();
                out.push_str(&format!(
                    "   {} [{}] {} — {}\n",
                    badge, sev, f.rule_id, f.message
                ));
                if !f.entities.is_empty() {
                    out.push_str(&format!(
                        "            Entities: {}\n",
                        f.entities.join(", ")
                    ));
                }
            }
        }

        if let Some(evs) = self.evidence.as_ref().filter(|e| !e.is_empty()) {
            out.push_str("--------------------------------------------------------------------------------\n");
            out.push_str(&format!(" EVIDENCE RECEIPTS ({})\n", evs.len()));
            for ev in evs {
                out.push_str(&format!(
                    "   • Finding: {} | Rule: {} | Digest: {}\n",
                    ev.finding_id, ev.rule_id, ev.evidence_digest
                ));
                for loc in &ev.locations {
                    let span = match (loc.start_line, loc.end_line) {
                        (Some(s), Some(e)) => format!(":{}-{}", s, e),
                        (Some(s), None) => format!(":{}", s),
                        _ => String::new(),
                    };
                    out.push_str(&format!("       Location: {}{}\n", loc.path, span));
                }
            }
        }

        out.push_str(
            "================================================================================\n",
        );
        out
    }

    /// Generates a SARIF v2.1.0 document conforming to OASIS SARIF specifications.
    pub fn to_sarif(&self) -> Result<String, ReportError> {
        let mut rules = Vec::new();
        let mut results = Vec::new();

        let mut rule_ids = std::collections::BTreeSet::new();
        for f in &self.findings {
            rule_ids.insert(f.rule_id.clone());
        }

        for rule_id in rule_ids {
            rules.push(serde_json::json!({
                "id": rule_id,
                "shortDescription": {
                    "text": format!("Code Autopsy invariant rule {}", rule_id)
                },
                "defaultConfiguration": {
                    "level": "error"
                }
            }));
        }

        for f in &self.findings {
            let level = match f.severity.to_lowercase().as_str() {
                "error" => "error",
                "warning" => "warning",
                _ => "note",
            };

            let locations = if let Some(ref evs) = self.evidence {
                evs.iter()
                    .filter(|e| e.finding_id == f.id)
                    .flat_map(|e| &e.locations)
                    .map(|loc| {
                        serde_json::json!({
                            "physicalLocation": {
                                "artifactLocation": {
                                    "uri": loc.path
                                },
                                "region": {
                                    "startLine": loc.start_line.unwrap_or(1),
                                    "endLine": loc.end_line.unwrap_or(loc.start_line.unwrap_or(1))
                                }
                            }
                        })
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };

            let mut result_obj = serde_json::json!({
                "ruleId": f.rule_id,
                "level": level,
                "message": {
                    "text": f.message
                }
            });

            if !locations.is_empty() {
                result_obj["locations"] = serde_json::Value::Array(locations);
            }

            results.push(result_obj);
        }

        let sarif = serde_json::json!({
            "version": "2.1.0",
            "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
            "runs": [
                {
                    "tool": {
                        "driver": {
                            "name": "Synevid",
                            "semanticVersion": self.run.analyzer_version,
                            "rules": rules
                        }
                    },
                    "results": results
                }
            ]
        });

        Ok(serde_json::to_string_pretty(&sarif)?)
    }
}

/// Helper to convert domain findings and evidence into report summaries.
pub fn convert_domain_findings(
    findings: &[Finding],
    evidence: &[EvidenceReceipt],
) -> (Vec<FindingSummary>, Vec<EvidenceSummary>) {
    let finding_summaries = findings
        .iter()
        .map(|f| FindingSummary {
            id: f.id.clone(),
            rule_id: f.rule_id.clone(),
            status: match f.status {
                autopsy_domain::FindingStatus::Pass => "pass".to_string(),
                autopsy_domain::FindingStatus::Fail => "fail".to_string(),
                autopsy_domain::FindingStatus::Unknown => "unknown".to_string(),
            },
            severity: match f.severity {
                Severity::Info => "info".to_string(),
                Severity::Warning => "warning".to_string(),
                Severity::Error => "error".to_string(),
            },
            message: f.message.clone(),
            entities: f.entities.clone(),
            coverage: match f.coverage {
                CoverageState::Verified => Some("verified".to_string()),
                CoverageState::Partial => Some("partial".to_string()),
                CoverageState::Unsupported => Some("unsupported".to_string()),
                CoverageState::Unknown => Some("unknown".to_string()),
            },
        })
        .collect();

    let evidence_summaries = evidence
        .iter()
        .map(|e| EvidenceSummary {
            finding_id: e.finding_id.clone(),
            rule_id: e.rule_id.clone(),
            locations: e
                .locations
                .iter()
                .map(|loc| LocationSummary {
                    path: loc.path.clone(),
                    start_line: Some(loc.start_line),
                    end_line: Some(loc.end_line),
                    symbol_id: loc.symbol_id.as_ref().map(|s| s.to_string()),
                })
                .collect(),
            paths: e
                .paths
                .iter()
                .map(|p| serde_json::to_value(p).unwrap_or_default())
                .collect(),
            deltas: e
                .deltas
                .iter()
                .map(|d| serde_json::to_value(d).unwrap_or_default())
                .collect(),
            evidence_digest: e.evidence_digest.clone(),
        })
        .collect();

    (finding_summaries, evidence_summaries)
}

/// Helper to convert semantic diff into ChangeSetSummary
pub fn convert_semantic_diff(diff: &SemanticDiff) -> ChangeSetSummary {
    ChangeSetSummary {
        before_snapshot_id: diff.before_snapshot_id.to_string(),
        after_snapshot_id: diff.after_snapshot_id.to_string(),
        file_deltas: diff
            .file_deltas
            .iter()
            .map(|f| serde_json::to_value(f).unwrap_or_default())
            .collect(),
        symbol_deltas: diff
            .symbol_deltas
            .iter()
            .map(|s| serde_json::to_value(s).unwrap_or_default())
            .collect(),
        contract_deltas: diff
            .contract_deltas
            .iter()
            .map(|c| serde_json::to_value(c).unwrap_or_default())
            .collect(),
        edge_deltas: diff
            .edge_deltas
            .iter()
            .map(|e| serde_json::to_value(e).unwrap_or_default())
            .collect(),
    }
}

/// Helper to convert impact result into ImpactSummary
pub fn convert_impact_result(impact: &ImpactResult) -> ImpactSummary {
    ImpactSummary {
        query: serde_json::json!({
            "seed_symbols": impact.query.seed_symbols.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            "direction": format!("{:?}", impact.query.direction),
        }),
        impacted_entities: impact
            .impacted_entities
            .iter()
            .map(|s| s.to_string())
            .collect(),
        paths: impact
            .paths
            .iter()
            .map(|p| serde_json::to_value(p).unwrap_or_default())
            .collect(),
        traversal_profile: impact.traversal_profile.clone(),
        truncated: impact.truncated,
    }
}

/// Sorts JSON object keys recursively to produce deterministic canonical JSON output.
fn sort_json_value(val: &serde_json::Value) -> serde_json::Value {
    match val {
        serde_json::Value::Object(map) => {
            let mut sorted = BTreeMap::new();
            for (k, v) in map {
                sorted.insert(k.clone(), sort_json_value(v));
            }
            serde_json::to_value(sorted).unwrap()
        }
        serde_json::Value::Array(arr) => {
            let sorted_arr = arr.iter().map(sort_json_value).collect();
            serde_json::Value::Array(sorted_arr)
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_report_canonical_json_and_digest_determinism() {
        let findings = vec![FindingSummary {
            id: "f-1".to_string(),
            rule_id: "no-circular-deps".to_string(),
            status: "fail".to_string(),
            severity: "error".to_string(),
            message: "Detected circular dependency".to_string(),
            entities: vec!["modA".to_string(), "modB".to_string()],
            coverage: Some("verified".to_string()),
        }];

        let coverage = CoverageSummary {
            state: "verified".to_string(),
            unsupported_constructs: vec![],
            files_scanned: Some(42),
        };

        let report1 = AutopsyReport::new(
            "run-1".to_string(),
            "verify".to_string(),
            "0.0.1".to_string(),
            Some(123),
            vec![SnapshotSummary {
                snapshot_id: "snap-1".to_string(),
                revision: "rev-1".to_string(),
                config_hash: "cfg-1".to_string(),
                file_set_digest: "files-1".to_string(),
            }],
            None,
            None,
            findings.clone(),
            None,
            coverage.clone(),
        )
        .unwrap();

        let report2 = AutopsyReport::new(
            "run-1".to_string(),
            "verify".to_string(),
            "0.0.1".to_string(),
            Some(123),
            vec![SnapshotSummary {
                snapshot_id: "snap-1".to_string(),
                revision: "rev-1".to_string(),
                config_hash: "cfg-1".to_string(),
                file_set_digest: "files-1".to_string(),
            }],
            None,
            None,
            findings,
            None,
            coverage,
        )
        .unwrap();

        assert_eq!(report1.result_digest, report2.result_digest);
        let json1 = report1.to_canonical_json().unwrap();
        let json2 = report2.to_canonical_json().unwrap();
        assert_eq!(json1, json2);

        // Verify JSON contains expected root fields
        let val: serde_json::Value = serde_json::from_str(&json1).unwrap();
        assert_eq!(val["schema_version"], "0.0.1");
        assert_eq!(val["run"]["command"], "verify");
        assert_eq!(val["findings"][0]["status"], "fail");
    }

    #[test]
    fn test_report_human_text_and_sarif() {
        let findings = vec![FindingSummary {
            id: "f-1".to_string(),
            rule_id: "layering".to_string(),
            status: "fail".to_string(),
            severity: "error".to_string(),
            message: "UI cannot import DB directly".to_string(),
            entities: vec!["ui".to_string(), "db".to_string()],
            coverage: Some("verified".to_string()),
        }];

        let evidence = vec![EvidenceSummary {
            finding_id: "f-1".to_string(),
            rule_id: "layering".to_string(),
            locations: vec![LocationSummary {
                path: "src/ui.ts".to_string(),
                start_line: Some(10),
                end_line: Some(12),
                symbol_id: Some("sym-1".to_string()),
            }],
            paths: vec![],
            deltas: vec![],
            evidence_digest: "blake3-ev-1".to_string(),
        }];

        let coverage = CoverageSummary {
            state: "verified".to_string(),
            unsupported_constructs: vec![],
            files_scanned: Some(10),
        };

        let report = AutopsyReport::new(
            "run-test".to_string(),
            "verify".to_string(),
            "0.0.1".to_string(),
            Some(50),
            vec![],
            None,
            None,
            findings,
            Some(evidence),
            coverage,
        )
        .unwrap();

        let human = report.to_human_text();
        assert!(human.contains("CODE AUTOPSY VERIFICATION REPORT"));
        assert!(human.contains("[ FAIL ] [ERROR] layering — UI cannot import DB directly"));
        assert!(human.contains("src/ui.ts:10-12"));

        let sarif = report.to_sarif().unwrap();
        let sarif_val: serde_json::Value = serde_json::from_str(&sarif).unwrap();
        assert_eq!(sarif_val["version"], "2.1.0");
        assert_eq!(sarif_val["runs"][0]["tool"]["driver"]["name"], "Synevid");
        assert_eq!(sarif_val["runs"][0]["results"][0]["ruleId"], "layering");
        assert_eq!(
            sarif_val["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["artifactLocation"]
                ["uri"],
            "src/ui.ts"
        );
    }
}
