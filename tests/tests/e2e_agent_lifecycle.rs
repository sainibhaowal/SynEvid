//! End-to-end integration test for the In-Session Agent Lifecycle Protocol (CA-TECH-001 Ch.4.2).
//!
//! Verifies the exact protocol loop:
//! Baseline A -> Impact Assessment -> Edit A -> B -> Verify A vs B -> (On FAIL: Repair via Evidence -> Re-verify PASS)

use autopsy_contracts::{
    CallableContract, ContractCompatibilityChecker, NormalizedContract, ParameterModel,
};
use autopsy_diff::{DiffEngine, DiffInput};
use autopsy_domain::{
    CoverageState, Edge, EdgeKind, FindingStatus, SnapshotId, SourceLocation, Symbol, SymbolId,
    Visibility,
};
use autopsy_graph::DependencyGraph;
use autopsy_impact::{ImpactDirection, ImpactEngine, ImpactProfile, ImpactQuery};
use autopsy_invariants::{EvaluationContext, InvariantEvaluator, InvariantsConfig};
use std::collections::BTreeMap;

fn make_symbol(id: &str, path: &str) -> Symbol {
    Symbol {
        stable_id: SymbolId::new(id),
        language_id: "typescript".to_string(),
        kind: "class".to_string(),
        qualified_name: id.to_string(),
        range: Some(SourceLocation {
            path: path.to_string(),
            start_line: 1,
            end_line: 10,
            start_col: Some(0),
            end_col: Some(20),
            symbol_id: Some(SymbolId::new(id)),
        }),
        normalized_signature: Some(format!("class {id}")),
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
fn test_in_session_agent_lifecycle_protocol_pass_flow() {
    // 1. Baseline A: Initial architecture with clean layered dependencies
    let mut graph_a = DependencyGraph::new();

    let controller = make_symbol("UserController", "src/presentation/userController.ts");
    let service = make_symbol("UserService", "src/domain/userService.ts");
    let model = make_symbol("UserModel", "src/infrastructure/userModel.ts");

    graph_a.add_node(controller.clone());
    graph_a.add_node(service.clone());
    graph_a.add_node(model.clone());

    // Valid layers: Controller -> Service -> Model
    graph_a.add_edge(make_edge("UserController", "UserService", EdgeKind::Calls));
    graph_a.add_edge(make_edge("UserService", "UserModel", EdgeKind::Calls));

    // 2. Pre-Edit Impact Assessment: Agent queries blast radius of UserService
    let impact_engine = ImpactEngine::new();
    let query = ImpactQuery {
        seed_symbols: vec![service.stable_id.clone()],
        direction: ImpactDirection::Backward,
        profile: ImpactProfile::default(),
    };
    let impact_result = impact_engine.compute_impact(&graph_a, &query).unwrap();
    assert_eq!(impact_result.impacted_entities.len(), 2);
    assert!(
        impact_result
            .impacted_entities
            .contains(&controller.stable_id)
    );

    // 3. Edit A -> B: Agent adds non-breaking enhancement
    let mut graph_b = graph_a.clone();
    let service_enhanced = Symbol {
        normalized_signature: Some("class UserService (enhanced)".to_string()),
        ..service.clone()
    };
    graph_b.add_node(service_enhanced);

    // 4. Verify A vs B: Semantic Diff + Invariants Check
    let empty_files = BTreeMap::new();
    let diff = DiffEngine::compute_diff(DiffInput {
        before_snapshot_id: SnapshotId::new("snap_a"),
        after_snapshot_id: SnapshotId::new("snap_b"),
        before_files: &empty_files,
        after_files: &empty_files,
        before_graph: &graph_a,
        after_graph: &graph_b,
        before_contracts: &[],
        after_contracts: &[],
    });
    assert_eq!(diff.symbol_deltas.len(), 1);

    let yaml_policy = r#"
version: "0.0.1"
invariants:
  - id: "FORBID_CONTROLLER_DIRECT_MODEL"
    description: "Controllers must not call models directly"
    kind: "forbidden_dependency"
    severity: "error"
    scope:
      source: "src/presentation/*"
      target: "src/infrastructure/*"
"#;
    let config = InvariantsConfig::from_yaml_str(yaml_policy).unwrap();
    let evaluator = InvariantEvaluator::new();
    let contracts = BTreeMap::new();
    let ctx = EvaluationContext {
        snapshot_id: SnapshotId::new("snap_b"),
        analyzer_version: "0.0.1",
        graph: &graph_b,
        baseline_graph: Some(&graph_a),
        diff: Some(&diff),
        contracts_before: &contracts,
        contracts_after: &contracts,
        coverage_state: CoverageState::Verified,
    };
    let findings = evaluator.evaluate(&config, &ctx).unwrap();
    assert_eq!(findings.len(), 1);
    let (finding, receipt) = &findings[0];
    assert_eq!(finding.status, FindingStatus::Pass);
    assert!(receipt.verify_integrity().unwrap());
}

#[test]
fn test_in_session_agent_lifecycle_fail_repair_reverify_flow() {
    let mut graph_a = DependencyGraph::new();

    let controller = make_symbol("CheckoutController", "src/presentation/checkout.ts");
    let payment_db = make_symbol("PaymentDb", "src/infrastructure/paymentDb.ts");
    graph_a.add_node(controller.clone());
    graph_a.add_node(payment_db.clone());

    let yaml_policy = r#"
version: "0.0.1"
invariants:
  - id: "NO_CONTROLLER_TO_DB"
    description: "Controllers must not import DB layer directly"
    kind: "forbidden_dependency"
    severity: "error"
    scope:
      source: "src/presentation/*"
      target: "src/infrastructure/*"
"#;
    let config = InvariantsConfig::from_yaml_str(yaml_policy).unwrap();
    let evaluator = InvariantEvaluator::new();
    let contracts = BTreeMap::new();

    // Step 1: Agent makes bad edit A -> B (adds illegal direct DB call)
    let mut graph_b = graph_a.clone();
    graph_b.add_edge(make_edge(
        "CheckoutController",
        "PaymentDb",
        EdgeKind::Calls,
    ));

    // Step 2: Verify fails with exact violation finding!
    let ctx_b = EvaluationContext {
        snapshot_id: SnapshotId::new("snap_b_violating"),
        analyzer_version: "0.0.1",
        graph: &graph_b,
        baseline_graph: Some(&graph_a),
        diff: None,
        contracts_before: &contracts,
        contracts_after: &contracts,
        coverage_state: CoverageState::Verified,
    };
    let findings_b = evaluator.evaluate(&config, &ctx_b).unwrap();
    assert_eq!(findings_b.len(), 1);
    assert_eq!(findings_b[0].0.status, FindingStatus::Fail);
    assert!(!findings_b[0].0.entities.is_empty());

    // Step 3: Agent REPAIRS the change using evidence: routes through PaymentService
    let mut graph_c = DependencyGraph::new();
    let payment_service = make_symbol("PaymentService", "src/domain/paymentService.ts");
    graph_c.add_node(controller.clone());
    graph_c.add_node(payment_service.clone());
    graph_c.add_node(payment_db.clone());

    // Clean calls: Controller -> Service -> DB (allowed by layers)
    graph_c.add_edge(make_edge(
        "CheckoutController",
        "PaymentService",
        EdgeKind::Calls,
    ));
    graph_c.add_edge(make_edge("PaymentService", "PaymentDb", EdgeKind::Calls));

    // Step 4: Re-verify PASSES!
    let ctx_c = EvaluationContext {
        snapshot_id: SnapshotId::new("snap_c_repaired"),
        analyzer_version: "0.0.1",
        graph: &graph_c,
        baseline_graph: Some(&graph_a),
        diff: None,
        contracts_before: &contracts,
        contracts_after: &contracts,
        coverage_state: CoverageState::Verified,
    };
    let findings_c = evaluator.evaluate(&config, &ctx_c).unwrap();
    assert_eq!(findings_c.len(), 1);
    assert_eq!(findings_c[0].0.status, FindingStatus::Pass);
    assert!(findings_c[0].1.verify_integrity().unwrap());
}

#[test]
fn test_contract_weakening_detection_lifecycle() {
    // Before: getUser(val: number): number
    let before_c = NormalizedContract::Callable(CallableContract {
        name: "getUser".to_string(),
        owner: SymbolId::new("UserService"),
        visibility: Visibility::Public,
        parameters: vec![ParameterModel::new("val").with_type("number")],
        return_type: Some("number".to_string()),
        type_parameters: Vec::new(),
        is_async: false,
    });

    // After: getUser(val: number, mandatory_config: Config): number (Breaking Change!)
    let after_c_breaking = NormalizedContract::Callable(CallableContract {
        name: "getUser".to_string(),
        owner: SymbolId::new("UserService"),
        visibility: Visibility::Public,
        parameters: vec![
            ParameterModel::new("val").with_type("number"),
            ParameterModel::new("mandatory_config").with_type("Config"),
        ],
        return_type: Some("number".to_string()),
        type_parameters: Vec::new(),
        is_async: false,
    });

    let checker = ContractCompatibilityChecker::new();
    let res = checker.check(&before_c, &after_c_breaking).unwrap();
    assert!(!res.is_compatible);
    assert!(res.is_breaking);
    assert_eq!(
        res.reasons,
        vec!["Required parameter 'mandatory_config' added"]
    );
}
