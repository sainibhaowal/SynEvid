//! autopsy-invariants: Invariant policy loader and deterministic evaluators (FR-011, FR-012, FR-014).
//!
//! Evaluates repository invariants deterministically:
//! - forbidden-dep: Prevents forbidden architectural imports and dependencies.
//! - layers: Enforces strict layered architecture constraints.
//! - no-new-cycle: Asserts no circular dependency cycles introduced beyond baseline.
//! - api-compat: Evaluates contract backward compatibility and breaking changes.
//!
//! Hard boundary: UNKNOWN != PASS. Unknown or partial coverage must never collapse into Pass.

use autopsy_contracts::{ContractCompatibilityChecker, NormalizedContract};
use autopsy_diff::SemanticDiff;
use autopsy_domain::{CoverageState, Finding, FindingStatus, Severity, SnapshotId, SymbolId};
use autopsy_evidence::{EvidenceReceipt, EvidenceReceiptBuilder};
use autopsy_graph::DependencyGraph;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InvariantError {
    #[error("Failed to parse invariants YAML: {0}")]
    YamlParse(#[from] serde_yaml::Error),
    #[error("Failed to read invariants file {path}: {reason}")]
    Io { path: String, reason: String },
    #[error("Evaluation error: {0}")]
    Evaluation(String),
}

/// Invariant specification loaded from YAML configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvariantRule {
    pub id: String,
    pub description: String,
    pub kind: String,
    pub severity: String,
    #[serde(default)]
    pub scope: Option<InvariantScope>,
    #[serde(default)]
    pub rule: Option<RuleParameters>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvariantScope {
    pub source: Option<String>,
    pub target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleParameters {
    #[serde(default)]
    pub layers: Vec<String>,
    #[serde(default)]
    pub allowed_cycles: Vec<Vec<String>>,
}

/// Invariants file configuration containing version and list of invariant rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvariantsConfig {
    pub version: String,
    pub invariants: Vec<InvariantRule>,
}

impl InvariantsConfig {
    pub fn from_yaml_str(yaml: &str) -> Result<Self, InvariantError> {
        let config: Self = serde_yaml::from_str(yaml)?;
        Ok(config)
    }

    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, InvariantError> {
        let path_ref = path.as_ref();
        let content = std::fs::read_to_string(path_ref).map_err(|e| InvariantError::Io {
            path: path_ref.display().to_string(),
            reason: e.to_string(),
        })?;
        Self::from_yaml_str(&content)
    }
}

/// Context provided to the invariant evaluation engine.
pub struct EvaluationContext<'a> {
    pub snapshot_id: SnapshotId,
    pub analyzer_version: &'a str,
    pub graph: &'a DependencyGraph,
    pub baseline_graph: Option<&'a DependencyGraph>,
    pub diff: Option<&'a SemanticDiff>,
    pub contracts_before: &'a BTreeMap<SymbolId, NormalizedContract>,
    pub contracts_after: &'a BTreeMap<SymbolId, NormalizedContract>,
    pub coverage_state: CoverageState,
}

/// Deterministic invariant evaluator.
#[derive(Debug, Default)]
pub struct InvariantEvaluator;

impl InvariantEvaluator {
    pub fn new() -> Self {
        Self
    }

    /// Evaluates all configured invariant rules against the provided context.
    pub fn evaluate(
        &self,
        config: &InvariantsConfig,
        ctx: &EvaluationContext<'_>,
    ) -> Result<Vec<(Finding, EvidenceReceipt)>, InvariantError> {
        let mut results = Vec::new();

        for rule in &config.invariants {
            let pair = match rule.kind.as_str() {
                "forbidden_dependency" | "forbidden-dep" => {
                    self.evaluate_forbidden_dep(rule, ctx)?
                }
                "layers" => self.evaluate_layers(rule, ctx)?,
                "no_new_cycle" | "no-new-cycle" => self.evaluate_no_new_cycles(rule, ctx)?,
                "api_compatibility" | "api-compat" => self.evaluate_api_compat(rule, ctx)?,
                unknown => {
                    return Err(InvariantError::Evaluation(format!(
                        "Unknown invariant kind: '{unknown}' in rule '{}'",
                        rule.id
                    )));
                }
            };
            results.push(pair);
        }

        // Sort findings deterministically by rule ID
        results.sort_by(|(f1, _), (f2, _)| f1.id.cmp(&f2.id));
        Ok(results)
    }

    /// Evaluates forbidden-dep rule.
    fn evaluate_forbidden_dep(
        &self,
        rule: &InvariantRule,
        ctx: &EvaluationContext<'_>,
    ) -> Result<(Finding, EvidenceReceipt), InvariantError> {
        let finding_id = format!("{}-FINDING", rule.id);
        let mut builder = EvidenceReceiptBuilder::new(&finding_id, &rule.id)
            .analyzer_version(ctx.analyzer_version)
            .add_snapshot(ctx.snapshot_id.clone());

        let scope = rule.scope.as_ref();
        let src_pattern = scope.and_then(|s| s.source.as_deref()).unwrap_or("*");
        let tgt_pattern = scope.and_then(|s| s.target.as_deref()).unwrap_or("*");

        let mut violations = Vec::new();
        let mut violating_entities = BTreeSet::new();

        for edge in ctx.graph.all_edges_sorted() {
            let src_sym = ctx.graph.get_node(&edge.source);
            let tgt_sym = ctx.graph.get_node(&edge.target);

            let src_path = src_sym
                .and_then(|s| s.range.as_ref())
                .map(|r| r.path.as_str())
                .unwrap_or(edge.source.as_str());
            let tgt_path = tgt_sym
                .and_then(|s| s.range.as_ref())
                .map(|r| r.path.as_str())
                .unwrap_or(edge.target.as_str());

            if matches_pattern(src_path, src_pattern) && matches_pattern(tgt_path, tgt_pattern) {
                violating_entities.insert(edge.source.to_string());
                violating_entities.insert(edge.target.to_string());

                if let Some(loc) = &edge.location {
                    builder = builder.add_location(loc.clone());
                }
                builder = builder.add_path(vec![edge.source.to_string(), edge.target.to_string()]);
                violations.push(format!(
                    "Forbidden edge: {} -> {}",
                    edge.source, edge.target
                ));
            }
        }

        let severity = parse_severity(&rule.severity);

        if !violations.is_empty() {
            for v in violations {
                builder = builder.add_delta(v);
            }
            let receipt = builder
                .build()
                .map_err(|e| InvariantError::Evaluation(e.to_string()))?;
            let finding = Finding {
                id: finding_id,
                rule_id: rule.id.clone(),
                status: FindingStatus::Fail,
                severity,
                message: format!(
                    "Forbidden dependency violation: found {} illegal edges from '{}' to '{}'",
                    violating_entities.len(),
                    src_pattern,
                    tgt_pattern
                ),
                entities: violating_entities.into_iter().collect(),
                coverage: ctx.coverage_state,
            };
            Ok((finding, receipt))
        } else if ctx.coverage_state == CoverageState::Unknown
            || ctx.coverage_state == CoverageState::Partial
        {
            // Hard boundary: UNKNOWN != PASS
            let receipt = builder
                .build()
                .map_err(|e| InvariantError::Evaluation(e.to_string()))?;
            let finding = Finding {
                id: finding_id,
                rule_id: rule.id.clone(),
                status: FindingStatus::Unknown,
                severity,
                message: "Cannot verify absence of forbidden dependencies due to dynamic or unresolvable code constructs".to_string(),
                entities: Vec::new(),
                coverage: ctx.coverage_state,
            };
            Ok((finding, receipt))
        } else {
            let receipt = builder
                .build()
                .map_err(|e| InvariantError::Evaluation(e.to_string()))?;
            let finding = Finding {
                id: finding_id,
                rule_id: rule.id.clone(),
                status: FindingStatus::Pass,
                severity,
                message: format!(
                    "No forbidden dependencies found from '{}' to '{}'",
                    src_pattern, tgt_pattern
                ),
                entities: Vec::new(),
                coverage: ctx.coverage_state,
            };
            Ok((finding, receipt))
        }
    }

    /// Evaluates layers rule.
    fn evaluate_layers(
        &self,
        rule: &InvariantRule,
        ctx: &EvaluationContext<'_>,
    ) -> Result<(Finding, EvidenceReceipt), InvariantError> {
        let finding_id = format!("{}-FINDING", rule.id);
        let mut builder = EvidenceReceiptBuilder::new(&finding_id, &rule.id)
            .analyzer_version(ctx.analyzer_version)
            .add_snapshot(ctx.snapshot_id.clone());

        let layers = rule
            .rule
            .as_ref()
            .map(|r| r.layers.as_slice())
            .unwrap_or(&[]);

        let mut layer_map: BTreeMap<&str, usize> = BTreeMap::new();
        for (idx, layer) in layers.iter().enumerate() {
            layer_map.insert(layer.as_str(), idx);
        }

        let mut violations = Vec::new();
        let mut violating_entities = BTreeSet::new();

        for edge in ctx.graph.all_edges_sorted() {
            let src_sym = ctx.graph.get_node(&edge.source);
            let tgt_sym = ctx.graph.get_node(&edge.target);

            let src_path = src_sym
                .and_then(|s| s.range.as_ref())
                .map(|r| r.path.as_str())
                .unwrap_or(edge.source.as_str());
            let tgt_path = tgt_sym
                .and_then(|s| s.range.as_ref())
                .map(|r| r.path.as_str())
                .unwrap_or(edge.target.as_str());

            let src_layer = find_layer(src_path, &layer_map);
            let tgt_layer = find_layer(tgt_path, &layer_map);

            if let (Some(s_idx), Some(t_idx)) = (src_layer, tgt_layer) {
                // Architectural rule: Layer i may only depend on Layer j where j >= i.
                // Depending on a lower layer index (e.g. domain -> presentation) is forbidden.
                if t_idx < s_idx {
                    violating_entities.insert(edge.source.to_string());
                    violating_entities.insert(edge.target.to_string());
                    if let Some(loc) = &edge.location {
                        builder = builder.add_location(loc.clone());
                    }
                    builder =
                        builder.add_path(vec![edge.source.to_string(), edge.target.to_string()]);
                    violations.push(format!(
                        "Layer violation: '{}' (layer {}) cannot depend on '{}' (layer {})",
                        src_path, s_idx, tgt_path, t_idx
                    ));
                }
            }
        }

        let severity = parse_severity(&rule.severity);

        if !violations.is_empty() {
            for v in violations {
                builder = builder.add_delta(v);
            }
            let receipt = builder
                .build()
                .map_err(|e| InvariantError::Evaluation(e.to_string()))?;
            let finding = Finding {
                id: finding_id,
                rule_id: rule.id.clone(),
                status: FindingStatus::Fail,
                severity,
                message: "Layer architecture violation: detected upward cross-layer imports"
                    .to_string(),
                entities: violating_entities.into_iter().collect(),
                coverage: ctx.coverage_state,
            };
            Ok((finding, receipt))
        } else if ctx.coverage_state == CoverageState::Unknown
            || ctx.coverage_state == CoverageState::Partial
        {
            let receipt = builder
                .build()
                .map_err(|e| InvariantError::Evaluation(e.to_string()))?;
            let finding = Finding {
                id: finding_id,
                rule_id: rule.id.clone(),
                status: FindingStatus::Unknown,
                severity,
                message: "Layer verification incomplete due to dynamic or unresolvable modules"
                    .to_string(),
                entities: Vec::new(),
                coverage: ctx.coverage_state,
            };
            Ok((finding, receipt))
        } else {
            let receipt = builder
                .build()
                .map_err(|e| InvariantError::Evaluation(e.to_string()))?;
            let finding = Finding {
                id: finding_id,
                rule_id: rule.id.clone(),
                status: FindingStatus::Pass,
                severity,
                message: "Layer architecture cleanly preserved across all components".to_string(),
                entities: Vec::new(),
                coverage: ctx.coverage_state,
            };
            Ok((finding, receipt))
        }
    }

    /// Evaluates no-new-cycles rule.
    fn evaluate_no_new_cycles(
        &self,
        rule: &InvariantRule,
        ctx: &EvaluationContext<'_>,
    ) -> Result<(Finding, EvidenceReceipt), InvariantError> {
        let finding_id = format!("{}-FINDING", rule.id);
        let mut builder = EvidenceReceiptBuilder::new(&finding_id, &rule.id)
            .analyzer_version(ctx.analyzer_version)
            .add_snapshot(ctx.snapshot_id.clone());

        let current_cycles = ctx.graph.find_cycles();

        let baseline_cycles = match ctx.baseline_graph {
            Some(bg) => bg.find_cycles(),
            None => Vec::new(),
        };

        let baseline_cycle_set: BTreeSet<BTreeSet<SymbolId>> = baseline_cycles
            .into_iter()
            .map(|cycle| cycle.into_iter().collect())
            .collect();

        let mut new_cycles = Vec::new();
        let mut violating_entities = BTreeSet::new();

        for cycle in current_cycles {
            let cycle_set: BTreeSet<SymbolId> = cycle.iter().cloned().collect();
            if !baseline_cycle_set.contains(&cycle_set) {
                for sym_id in &cycle {
                    violating_entities.insert(sym_id.to_string());
                    if let Some(sym) = ctx.graph.get_node(sym_id)
                        && let Some(range) = &sym.range
                    {
                        builder = builder.add_location(range.clone());
                    }
                }
                let cycle_str = cycle
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(" -> ");
                builder = builder.add_path(cycle.iter().map(|s| s.to_string()).collect());
                builder = builder.add_delta(format!("New cycle: {}", cycle_str));
                new_cycles.push(cycle);
            }
        }

        let severity = parse_severity(&rule.severity);

        if !new_cycles.is_empty() {
            let receipt = builder
                .build()
                .map_err(|e| InvariantError::Evaluation(e.to_string()))?;
            let finding = Finding {
                id: finding_id,
                rule_id: rule.id.clone(),
                status: FindingStatus::Fail,
                severity,
                message: format!(
                    "Introduced {} new cyclic dependency loops",
                    new_cycles.len()
                ),
                entities: violating_entities.into_iter().collect(),
                coverage: ctx.coverage_state,
            };
            Ok((finding, receipt))
        } else {
            let receipt = builder
                .build()
                .map_err(|e| InvariantError::Evaluation(e.to_string()))?;
            let finding = Finding {
                id: finding_id,
                rule_id: rule.id.clone(),
                status: FindingStatus::Pass,
                severity,
                message: "No new circular dependency loops introduced".to_string(),
                entities: Vec::new(),
                coverage: ctx.coverage_state,
            };
            Ok((finding, receipt))
        }
    }

    /// Evaluates api-compatibility rule.
    fn evaluate_api_compat(
        &self,
        rule: &InvariantRule,
        ctx: &EvaluationContext<'_>,
    ) -> Result<(Finding, EvidenceReceipt), InvariantError> {
        let finding_id = format!("{}-FINDING", rule.id);
        let mut builder = EvidenceReceiptBuilder::new(&finding_id, &rule.id)
            .analyzer_version(ctx.analyzer_version)
            .add_snapshot(ctx.snapshot_id.clone());

        let checker = ContractCompatibilityChecker::new();
        let mut breaking_changes = Vec::new();
        let mut violating_entities = BTreeSet::new();

        for (sym_id, before_contract) in ctx.contracts_before {
            if let Some(after_contract) = ctx.contracts_after.get(sym_id) {
                // Only public/exported contracts are constrained by API backward compatibility
                if before_contract.visibility() == autopsy_domain::Visibility::Public {
                    match checker.check(before_contract, after_contract) {
                        Ok(res) => {
                            if res.is_breaking {
                                violating_entities.insert(sym_id.to_string());
                                for r in res.reasons {
                                    let desc = format!("{}: {}", sym_id, r);
                                    builder = builder.add_delta(&desc);
                                    breaking_changes.push(desc);
                                }
                                if let Some(sym) = ctx.graph.get_node(sym_id)
                                    && let Some(loc) = &sym.range
                                {
                                    builder = builder.add_location(loc.clone());
                                }
                            }
                        }
                        Err(e) => {
                            let desc = format!("{}: kind mismatch error ({})", sym_id, e);
                            breaking_changes.push(desc);
                        }
                    }
                }
            }
        }

        let severity = parse_severity(&rule.severity);

        if !breaking_changes.is_empty() {
            let receipt = builder
                .build()
                .map_err(|e| InvariantError::Evaluation(e.to_string()))?;
            let finding = Finding {
                id: finding_id,
                rule_id: rule.id.clone(),
                status: FindingStatus::Fail,
                severity,
                message: format!(
                    "Detected {} breaking public API contract modifications",
                    breaking_changes.len()
                ),
                entities: violating_entities.into_iter().collect(),
                coverage: ctx.coverage_state,
            };
            Ok((finding, receipt))
        } else if ctx.coverage_state == CoverageState::Unknown
            || ctx.coverage_state == CoverageState::Partial
        {
            let receipt = builder
                .build()
                .map_err(|e| InvariantError::Evaluation(e.to_string()))?;
            let finding = Finding {
                id: finding_id,
                rule_id: rule.id.clone(),
                status: FindingStatus::Unknown,
                severity,
                message: "Public API contract compatibility unknown due to unresolvable dynamic AST constructs".to_string(),
                entities: Vec::new(),
                coverage: ctx.coverage_state,
            };
            Ok((finding, receipt))
        } else {
            let receipt = builder
                .build()
                .map_err(|e| InvariantError::Evaluation(e.to_string()))?;
            let finding = Finding {
                id: finding_id,
                rule_id: rule.id.clone(),
                status: FindingStatus::Pass,
                severity,
                message: "All public API contracts remain strictly backward compatible".to_string(),
                entities: Vec::new(),
                coverage: ctx.coverage_state,
            };
            Ok((finding, receipt))
        }
    }
}

fn parse_severity(sev: &str) -> Severity {
    match sev.to_lowercase().as_str() {
        "error" => Severity::Error,
        "warning" => Severity::Warning,
        _ => Severity::Info,
    }
}

fn matches_pattern(path: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    match glob::Pattern::new(pattern) {
        Ok(pat) => pat.matches(path),
        Err(_) => path.contains(pattern),
    }
}

fn find_layer(path: &str, layer_map: &BTreeMap<&str, usize>) -> Option<usize> {
    for (&layer_name, &idx) in layer_map {
        if path.contains(layer_name) {
            return Some(idx);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use autopsy_contracts::{CallableContract, ParameterModel};
    use autopsy_domain::{Edge, EdgeKind, SourceLocation, Symbol, Visibility};

    fn make_symbol(id: &str, path: &str) -> Symbol {
        Symbol {
            stable_id: SymbolId::new(id),
            language_id: "typescript".to_string(),
            kind: "function".to_string(),
            qualified_name: id.to_string(),
            range: Some(SourceLocation {
                path: path.to_string(),
                start_line: 1,
                end_line: 5,
                start_col: None,
                end_col: None,
                symbol_id: Some(SymbolId::new(id)),
            }),
            normalized_signature: None,
            visibility: Visibility::Public,
        }
    }

    #[test]
    fn test_yaml_config_loader() {
        let yaml = r#"
version: "0.0.1"
invariants:
  - id: "ARCH_NO_CORE_TO_MCP"
    description: "Core must never import MCP"
    kind: "forbidden_dependency"
    severity: "error"
    scope:
      source: "crates/*"
      target: "apps/mcp*"
"#;
        let config = InvariantsConfig::from_yaml_str(yaml).unwrap();
        assert_eq!(config.version, "0.0.1");
        assert_eq!(config.invariants.len(), 1);
        assert_eq!(config.invariants[0].id, "ARCH_NO_CORE_TO_MCP");
    }

    #[test]
    fn test_forbidden_dep_evaluator_pass_and_fail() {
        let yaml = r#"
version: "0.0.1"
invariants:
  - id: "NO_CORE_TO_GUI"
    description: "Core must not depend on GUI"
    kind: "forbidden_dependency"
    severity: "error"
    scope:
      source: "crates/*"
      target: "apps/gui*"
"#;
        let config = InvariantsConfig::from_yaml_str(yaml).unwrap();
        let evaluator = InvariantEvaluator::new();

        // 1. Clean graph: crates/a -> crates/b (PASS)
        let mut clean_graph = DependencyGraph::new();
        clean_graph.add_node(make_symbol("core::a", "crates/a/src/lib.rs"));
        clean_graph.add_node(make_symbol("core::b", "crates/b/src/lib.rs"));
        clean_graph.add_edge(Edge {
            source: SymbolId::new("core::a"),
            target: SymbolId::new("core::b"),
            kind: EdgeKind::Calls,
            coverage: CoverageState::Verified,
            location: None,
            provenance: "test".to_string(),
        });

        let contracts = BTreeMap::new();
        let clean_ctx = EvaluationContext {
            snapshot_id: SnapshotId::new("snap1"),
            analyzer_version: "0.0.1",
            graph: &clean_graph,
            baseline_graph: None,
            diff: None,
            contracts_before: &contracts,
            contracts_after: &contracts,
            coverage_state: CoverageState::Verified,
        };

        let res = evaluator.evaluate(&config, &clean_ctx).unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].0.status, FindingStatus::Pass);

        // 2. Polluted graph: crates/a -> apps/gui (FAIL)
        let mut polluted_graph = clean_graph.clone();
        polluted_graph.add_node(make_symbol("gui::app", "apps/gui/src/app.tsx"));
        polluted_graph.add_edge(Edge {
            source: SymbolId::new("core::a"),
            target: SymbolId::new("gui::app"),
            kind: EdgeKind::Calls,
            coverage: CoverageState::Verified,
            location: None,
            provenance: "test".to_string(),
        });

        let polluted_ctx = EvaluationContext {
            snapshot_id: SnapshotId::new("snap2"),
            analyzer_version: "0.0.1",
            graph: &polluted_graph,
            baseline_graph: None,
            diff: None,
            contracts_before: &contracts,
            contracts_after: &contracts,
            coverage_state: CoverageState::Verified,
        };

        let res_fail = evaluator.evaluate(&config, &polluted_ctx).unwrap();
        assert_eq!(res_fail.len(), 1);
        assert_eq!(res_fail[0].0.status, FindingStatus::Fail);
        assert!(res_fail[0].1.verify_integrity().unwrap());
    }

    #[test]
    fn test_layers_evaluator_pass_and_fail() {
        let yaml = r#"
version: "0.0.1"
invariants:
  - id: "CLEAN_LAYERS"
    description: "Layered architecture rule"
    kind: "layers"
    severity: "error"
    rule:
      layers:
        - "presentation"
        - "domain"
        - "infrastructure"
"#;
        let config = InvariantsConfig::from_yaml_str(yaml).unwrap();
        let evaluator = InvariantEvaluator::new();
        let contracts = BTreeMap::new();

        // 1. Valid: presentation -> domain (layer 0 -> layer 1)
        let mut valid_graph = DependencyGraph::new();
        valid_graph.add_node(make_symbol("view", "src/presentation/view.ts"));
        valid_graph.add_node(make_symbol("model", "src/domain/model.ts"));
        valid_graph.add_edge(Edge {
            source: SymbolId::new("view"),
            target: SymbolId::new("model"),
            kind: EdgeKind::Calls,
            coverage: CoverageState::Verified,
            location: None,
            provenance: "test".to_string(),
        });

        let valid_ctx = EvaluationContext {
            snapshot_id: SnapshotId::new("snap1"),
            analyzer_version: "0.0.1",
            graph: &valid_graph,
            baseline_graph: None,
            diff: None,
            contracts_before: &contracts,
            contracts_after: &contracts,
            coverage_state: CoverageState::Verified,
        };

        let res = evaluator.evaluate(&config, &valid_ctx).unwrap();
        assert_eq!(res[0].0.status, FindingStatus::Pass);

        // 2. Invalid: infrastructure -> presentation (layer 2 -> layer 0)
        let mut invalid_graph = valid_graph.clone();
        invalid_graph.add_node(make_symbol("db", "src/infrastructure/db.ts"));
        invalid_graph.add_edge(Edge {
            source: SymbolId::new("db"),
            target: SymbolId::new("view"),
            kind: EdgeKind::Calls,
            coverage: CoverageState::Verified,
            location: None,
            provenance: "test".to_string(),
        });

        let invalid_ctx = EvaluationContext {
            snapshot_id: SnapshotId::new("snap2"),
            analyzer_version: "0.0.1",
            graph: &invalid_graph,
            baseline_graph: None,
            diff: None,
            contracts_before: &contracts,
            contracts_after: &contracts,
            coverage_state: CoverageState::Verified,
        };

        let res_fail = evaluator.evaluate(&config, &invalid_ctx).unwrap();
        assert_eq!(res_fail[0].0.status, FindingStatus::Fail);
    }

    #[test]
    fn test_no_new_cycles_evaluator() {
        let yaml = r#"
version: "0.0.1"
invariants:
  - id: "NO_CYCLES"
    description: "Cycle free"
    kind: "no_new_cycle"
    severity: "error"
"#;
        let config = InvariantsConfig::from_yaml_str(yaml).unwrap();
        let evaluator = InvariantEvaluator::new();
        let contracts = BTreeMap::new();

        let mut baseline_graph = DependencyGraph::new();
        baseline_graph.add_node(make_symbol("a", "src/a.ts"));
        baseline_graph.add_node(make_symbol("b", "src/b.ts"));
        baseline_graph.add_edge(Edge {
            source: SymbolId::new("a"),
            target: SymbolId::new("b"),
            kind: EdgeKind::Calls,
            coverage: CoverageState::Verified,
            location: None,
            provenance: "test".to_string(),
        });

        // Current graph introduces a cycle: b -> a
        let mut current_graph = baseline_graph.clone();
        current_graph.add_edge(Edge {
            source: SymbolId::new("b"),
            target: SymbolId::new("a"),
            kind: EdgeKind::Calls,
            coverage: CoverageState::Verified,
            location: None,
            provenance: "test".to_string(),
        });

        let ctx = EvaluationContext {
            snapshot_id: SnapshotId::new("snap_cur"),
            analyzer_version: "0.0.1",
            graph: &current_graph,
            baseline_graph: Some(&baseline_graph),
            diff: None,
            contracts_before: &contracts,
            contracts_after: &contracts,
            coverage_state: CoverageState::Verified,
        };

        let res = evaluator.evaluate(&config, &ctx).unwrap();
        assert_eq!(res[0].0.status, FindingStatus::Fail);
    }

    #[test]
    fn test_api_compatibility_evaluator() {
        let yaml = r#"
version: "0.0.1"
invariants:
  - id: "PUBLIC_API_STABLE"
    description: "No breaking public API changes"
    kind: "api_compatibility"
    severity: "error"
"#;
        let config = InvariantsConfig::from_yaml_str(yaml).unwrap();
        let evaluator = InvariantEvaluator::new();
        let graph = DependencyGraph::new();

        let sym_id = SymbolId::new("calc::add");
        let before_c = NormalizedContract::Callable(CallableContract {
            owner: sym_id.clone(),
            name: "add".to_string(),
            visibility: Visibility::Public,
            parameters: vec![ParameterModel::new("a").with_type("number")],
            return_type: Some("number".to_string()),
            type_parameters: Vec::new(),
            is_async: false,
        });

        // After adds a required parameter: breaking!
        let after_c = NormalizedContract::Callable(CallableContract {
            owner: sym_id.clone(),
            name: "add".to_string(),
            visibility: Visibility::Public,
            parameters: vec![
                ParameterModel::new("a").with_type("number"),
                ParameterModel::new("b").with_type("number"),
            ],
            return_type: Some("number".to_string()),
            type_parameters: Vec::new(),
            is_async: false,
        });

        let mut before_map = BTreeMap::new();
        before_map.insert(sym_id.clone(), before_c);

        let mut after_map = BTreeMap::new();
        after_map.insert(sym_id, after_c);

        let ctx = EvaluationContext {
            snapshot_id: SnapshotId::new("snap_api"),
            analyzer_version: "0.0.1",
            graph: &graph,
            baseline_graph: None,
            diff: None,
            contracts_before: &before_map,
            contracts_after: &after_map,
            coverage_state: CoverageState::Verified,
        };

        let res = evaluator.evaluate(&config, &ctx).unwrap();
        assert_eq!(res[0].0.status, FindingStatus::Fail);
    }

    #[test]
    fn test_unknown_never_collapses_to_pass() {
        let yaml = r#"
version: "0.0.1"
invariants:
  - id: "STRICT_CHECK"
    description: "Strict check"
    kind: "forbidden_dependency"
    severity: "error"
    scope:
      source: "crates/*"
      target: "apps/mcp*"
"#;
        let config = InvariantsConfig::from_yaml_str(yaml).unwrap();
        let evaluator = InvariantEvaluator::new();
        let graph = DependencyGraph::new();
        let contracts = BTreeMap::new();

        // Even though no violations exist, coverage is Unknown
        let ctx = EvaluationContext {
            snapshot_id: SnapshotId::new("snap_unknown"),
            analyzer_version: "0.0.1",
            graph: &graph,
            baseline_graph: None,
            diff: None,
            contracts_before: &contracts,
            contracts_after: &contracts,
            coverage_state: CoverageState::Unknown,
        };

        let res = evaluator.evaluate(&config, &ctx).unwrap();
        assert_eq!(res.len(), 1);
        // CRITICAL INVARIANT: UNKNOWN != PASS
        assert_ne!(res[0].0.status, FindingStatus::Pass);
        assert_eq!(res[0].0.status, FindingStatus::Unknown);
    }
}
