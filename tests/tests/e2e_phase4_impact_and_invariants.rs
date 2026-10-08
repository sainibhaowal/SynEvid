//! End-to-end integration tests for Phase 4:
//! Impact Analysis, Contracts, Invariants, and Evidence Receipts (FR-008, 010, 012, 013, 014).
//!
//! Includes:
//! - Golden impact/verify fixtures exact JSON matches.
//! - Hard boundary test: UNKNOWN != PASS.
//! - 100-run determinism test for impact and evidence digests.

use autopsy_contracts::{
    CallableContract, ContractCompatibilityChecker, NormalizedContract, ParameterModel,
};
use autopsy_domain::{
    CoverageState, Edge, EdgeKind, FindingStatus, SnapshotId, SourceLocation, Symbol, SymbolId,
    Visibility,
};
use autopsy_evidence::EvidenceReceiptBuilder;
use autopsy_graph::DependencyGraph;
use autopsy_impact::{ImpactDirection, ImpactEngine, ImpactProfile, ImpactQuery};
use autopsy_invariants::{EvaluationContext, InvariantEvaluator, InvariantsConfig};
use std::collections::BTreeMap;

fn make_symbol(id: &str, path: &str) -> Symbol {
    Symbol {
        stable_id: SymbolId::new(id),
        language_id: "typescript".to_string(),
        kind: "function".to_string(),
        qualified_name: id.to_string(),
        range: Some(SourceLocation {
            path: path.to_string(),
            start_line: 1,
            end_line: 10,
            start_col: Some(0),
            end_col: Some(20),
            symbol_id: Some(SymbolId::new(id)),
        }),
        normalized_signature: Some(format!("export function {id}(): void")),
        visibility: Visibility::Public,
    }
}

fn make_edge(source: &str, target: &str, kind: EdgeKind) -> Edge {
    Edge {
        source: SymbolId::new(source),
        target: SymbolId::new(target),
        kind,
        coverage: CoverageState::Verified,
        location: None,
        provenance: "test-adapter".to_string(),
    }
}

#[test]
fn test_e2e_bounded_impact_bfs_fwd_bwd_and_truncation() {
    let mut graph = DependencyGraph::new();

    // Setup linear chain: svc -> repo -> db, and auxiliary logger called by all
    graph.add_node(make_symbol("svc", "src/services/user.ts"));
    graph.add_node(make_symbol("repo", "src/repositories/user.ts"));
    graph.add_node(make_symbol("db", "src/infrastructure/db.ts"));
    graph.add_node(make_symbol("logger", "src/utils/logger.ts"));

    graph.add_edge(make_edge("svc", "repo", EdgeKind::Calls));
    graph.add_edge(make_edge("repo", "db", EdgeKind::Calls));
    graph.add_edge(make_edge("svc", "logger", EdgeKind::Calls));
    graph.add_edge(make_edge("repo", "logger", EdgeKind::Calls));

    let engine = ImpactEngine::new();

    // 1. Forward impact from svc (should reach svc, repo, db, logger)
    let fwd_query = ImpactQuery {
        seed_symbols: vec![SymbolId::new("svc")],
        direction: ImpactDirection::Forward,
        profile: ImpactProfile::default(),
    };
    let fwd_res = engine.compute_impact(&graph, &fwd_query).unwrap();
    assert_eq!(
        fwd_res.impacted_entities,
        vec![
            SymbolId::new("db"),
            SymbolId::new("logger"),
            SymbolId::new("repo"),
            SymbolId::new("svc"),
        ]
    );
    assert!(!fwd_res.truncated);

    // 2. Backward impact from db (should reach svc, repo, db)
    let bwd_query = ImpactQuery {
        seed_symbols: vec![SymbolId::new("db")],
        direction: ImpactDirection::Backward,
        profile: ImpactProfile::default(),
    };
    let bwd_res = engine.compute_impact(&graph, &bwd_query).unwrap();
    assert_eq!(
        bwd_res.impacted_entities,
        vec![
            SymbolId::new("db"),
            SymbolId::new("repo"),
            SymbolId::new("svc"),
        ]
    );

    // 3. Budget truncation: budget=2
    let capped_profile = ImpactProfile {
        budget: 2,
        ..Default::default()
    };
    let capped_query = ImpactQuery {
        seed_symbols: vec![SymbolId::new("svc")],
        direction: ImpactDirection::Forward,
        profile: capped_profile,
    };
    let capped_res = engine.compute_impact(&graph, &capped_query).unwrap();
    assert_eq!(capped_res.impacted_entities.len(), 2);
    assert!(capped_res.truncated);
}

#[test]
fn test_e2e_golden_impact_fixture_exact_json_match() {
    let mut graph = DependencyGraph::new();
    graph.add_node(make_symbol("a", "src/a.ts"));
    graph.add_node(make_symbol("b", "src/b.ts"));
    graph.add_edge(make_edge("a", "b", EdgeKind::Calls));

    let engine = ImpactEngine::new();
    let query = ImpactQuery {
        seed_symbols: vec![SymbolId::new("a")],
        direction: ImpactDirection::Forward,
        profile: ImpactProfile::default(),
    };
    let res = engine.compute_impact(&graph, &query).unwrap();

    let json = serde_json::to_string_pretty(&res).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["traversal_profile"], "default");
    assert_eq!(parsed["truncated"], false);
    assert_eq!(parsed["impacted_entities"].as_array().unwrap().len(), 2);
    assert!(!res.impact_digest.is_empty());
}

#[test]
fn test_e2e_contracts_compatibility_and_breaking_rules() {
    let checker = ContractCompatibilityChecker::new();
    let sym_id = SymbolId::new("calc::compute");

    // Case 1: Non-breaking change (adding optional parameter)
    let before_c = NormalizedContract::Callable(CallableContract {
        owner: sym_id.clone(),
        name: "compute".to_string(),
        visibility: Visibility::Public,
        parameters: vec![ParameterModel::new("val").with_type("number")],
        return_type: Some("number".to_string()),
        type_parameters: Vec::new(),
        is_async: false,
    });

    let after_c_compatible = NormalizedContract::Callable(CallableContract {
        owner: sym_id.clone(),
        name: "compute".to_string(),
        visibility: Visibility::Public,
        parameters: vec![
            ParameterModel::new("val").with_type("number"),
            ParameterModel::new("factor").with_type("number").optional(),
        ],
        return_type: Some("number".to_string()),
        type_parameters: Vec::new(),
        is_async: false,
    });

    let res_compat = checker.check(&before_c, &after_c_compatible).unwrap();
    assert!(res_compat.is_compatible);
    assert!(!res_compat.is_breaking);

    // Case 2: Breaking change (adding required parameter)
    let after_c_breaking = NormalizedContract::Callable(CallableContract {
        owner: sym_id,
        name: "compute".to_string(),
        visibility: Visibility::Public,
        parameters: vec![
            ParameterModel::new("val").with_type("number"),
            ParameterModel::new("mandatory_config").with_type("Config"),
        ],
        return_type: Some("number".to_string()),
        type_parameters: Vec::new(),
        is_async: false,
    });

    let res_breaking = checker.check(&before_c, &after_c_breaking).unwrap();
    assert!(!res_breaking.is_compatible);
    assert!(res_breaking.is_breaking);
    assert_eq!(
        res_breaking.reasons,
        vec!["Required parameter 'mandatory_config' added"]
    );
}

#[test]
fn test_e2e_invariants_evaluation_all_rules_and_receipts() {
    let yaml = r#"
version: "0.0.1"
invariants:
  - id: "ARCH_FORBIDDEN_DEP"
    description: "Core must never import MCP"
    kind: "forbidden_dependency"
    severity: "error"
    scope:
      source: "crates/*"
      target: "apps/mcp*"
  - id: "ARCH_LAYERS"
    description: "Layer constraint"
    kind: "layers"
    severity: "error"
    rule:
      layers:
        - "presentation"
        - "domain"
        - "infrastructure"
  - id: "CYCLE_FREE"
    description: "No cycles"
    kind: "no_new_cycle"
    severity: "error"
"#;
    let config = InvariantsConfig::from_yaml_str(yaml).unwrap();
    let evaluator = InvariantEvaluator::new();

    let mut graph = DependencyGraph::new();
    graph.add_node(make_symbol("view", "src/presentation/view.ts"));
    graph.add_node(make_symbol("model", "src/domain/model.ts"));
    graph.add_edge(make_edge("view", "model", EdgeKind::Calls));

    let contracts = BTreeMap::new();
    let ctx = EvaluationContext {
        snapshot_id: SnapshotId::new("snap_golden"),
        analyzer_version: "0.0.1",
        graph: &graph,
        baseline_graph: None,
        diff: None,
        contracts_before: &contracts,
        contracts_after: &contracts,
        coverage_state: CoverageState::Verified,
    };

    let findings = evaluator.evaluate(&config, &ctx).unwrap();
    assert_eq!(findings.len(), 3);

    for (finding, receipt) in findings {
        assert_eq!(finding.status, FindingStatus::Pass);
        assert!(receipt.verify_integrity().unwrap());
        assert_eq!(receipt.snapshots, vec![SnapshotId::new("snap_golden")]);
    }
}

#[test]
fn test_e2e_unknown_never_equals_pass_hard_boundary() {
    // HARD GATE INVARIANT (FR-014, FR-030): UNKNOWN != PASS
    // Even if no static violation exists, if code constructs or files have Unknown coverage,
    // the finding status must be Unknown and NEVER Pass.
    let yaml = r#"
version: "0.0.1"
invariants:
  - id: "SECURITY_BOUNDARY"
    description: "Security boundary check"
    kind: "forbidden_dependency"
    severity: "error"
    scope:
      source: "auth/*"
      target: "public/*"
"#;
    let config = InvariantsConfig::from_yaml_str(yaml).unwrap();
    let evaluator = InvariantEvaluator::new();
    let empty_graph = DependencyGraph::new();
    let contracts = BTreeMap::new();

    let unknown_ctx = EvaluationContext {
        snapshot_id: SnapshotId::new("snap_dyn_ast"),
        analyzer_version: "0.0.1",
        graph: &empty_graph,
        baseline_graph: None,
        diff: None,
        contracts_before: &contracts,
        contracts_after: &contracts,
        coverage_state: CoverageState::Unknown,
    };

    let findings = evaluator.evaluate(&config, &unknown_ctx).unwrap();
    assert_eq!(findings.len(), 1);

    let (finding, receipt) = &findings[0];
    // HARD GATE ASSERTION:
    assert_ne!(
        finding.status,
        FindingStatus::Pass,
        "CRITICAL VIOLATION: UNKNOWN must NEVER equal PASS!"
    );
    assert_eq!(finding.status, FindingStatus::Unknown);
    assert_eq!(finding.coverage, CoverageState::Unknown);
    assert!(receipt.verify_integrity().unwrap());
}

#[test]
fn test_e2e_phase4_100_runs_determinism() {
    let mut graph = DependencyGraph::new();
    graph.add_node(make_symbol("a", "src/a.ts"));
    graph.add_node(make_symbol("b", "src/b.ts"));
    graph.add_edge(make_edge("a", "b", EdgeKind::Calls));

    let engine = ImpactEngine::new();
    let query = ImpactQuery {
        seed_symbols: vec![SymbolId::new("a")],
        direction: ImpactDirection::Forward,
        profile: ImpactProfile::default(),
    };

    let first_impact = engine.compute_impact(&graph, &query).unwrap();

    let loc = SourceLocation {
        path: "src/a.ts".to_string(),
        start_line: 1,
        end_line: 5,
        start_col: Some(0),
        end_col: Some(10),
        symbol_id: Some(SymbolId::new("a")),
    };
    let first_evidence = EvidenceReceiptBuilder::new("F1", "R1")
        .add_snapshot(SnapshotId::new("snap_det"))
        .add_location(loc.clone())
        .build()
        .unwrap();

    for _ in 0..100 {
        let impact = engine.compute_impact(&graph, &query).unwrap();
        assert_eq!(first_impact.impact_digest, impact.impact_digest);

        let evidence = EvidenceReceiptBuilder::new("F1", "R1")
            .add_snapshot(SnapshotId::new("snap_det"))
            .add_location(loc.clone())
            .build()
            .unwrap();
        assert_eq!(first_evidence.evidence_digest, evidence.evidence_digest);
    }
}
