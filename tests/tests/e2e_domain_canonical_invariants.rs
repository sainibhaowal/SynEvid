use autopsy_domain::{
    Contract, CoverageState, Edge, EdgeKind, Evidence, FileUnit, Finding, FindingStatus,
    RepoSnapshot, Severity, SnapshotId, SourceLocation, Symbol, SymbolId, Visibility,
};
use std::collections::BTreeMap;

#[test]
fn test_e2e_coverage_state_hard_boundary() {
    // Hard boundary: Unknown and unsupported must not collapse into Verified/Pass
    let states = vec![
        CoverageState::Verified,
        CoverageState::Partial,
        CoverageState::Unsupported,
        CoverageState::Unknown,
    ];

    for state in states {
        let serialized = serde_json::to_string(&state).unwrap();
        let deserialized: CoverageState = serde_json::from_str(&serialized).unwrap();
        assert_eq!(state, deserialized);
        if state == CoverageState::Unknown {
            assert_ne!(state, CoverageState::Verified);
        }
    }
}

#[test]
fn test_e2e_canonical_json_schema_shape_compatibility() {
    let mut files = BTreeMap::new();
    files.insert(
        "src/core.rs".to_string(),
        FileUnit {
            path: "src/core.rs".to_string(),
            language: "rust".to_string(),
            content_hash: "hash123".to_string(),
            is_generated: false,
        },
    );

    let snapshot = RepoSnapshot {
        snapshot_id: SnapshotId::new("snap_abc_123"),
        revision: "git_sha_xyz".to_string(),
        config_hash: "cfg_hash_789".to_string(),
        file_set_digest: "digest_456".to_string(),
        files,
    };

    let snapshot_json = serde_json::to_string_pretty(&snapshot).unwrap();
    assert!(snapshot_json.contains("\"snapshot_id\": \"snap_abc_123\""));
    assert!(snapshot_json.contains("\"revision\": \"git_sha_xyz\""));

    let finding = Finding {
        id: "find_001".to_string(),
        rule_id: "NO_DIRECT_DB_IN_UI".to_string(),
        status: FindingStatus::Fail,
        severity: Severity::Error,
        message: "Direct SQL query invoked inside UI component".to_string(),
        entities: vec!["UiComponent".to_string(), "DbConnection".to_string()],
        coverage: CoverageState::Verified,
    };

    let evidence = Evidence {
        finding_id: "find_001".to_string(),
        rule_id: "NO_DIRECT_DB_IN_UI".to_string(),
        locations: vec![SourceLocation {
            path: "ui/view.tsx".to_string(),
            start_line: 42,
            end_line: 45,
            start_col: Some(5),
            end_col: Some(30),
            symbol_id: Some(SymbolId::new("ui::view#fn:render:sig")),
        }],
        paths: vec![vec![
            "ui::view#fn:render:sig".to_string(),
            "db::conn#fn:query:sig".to_string(),
        ]],
        deltas: vec!["Added call edge to db::conn".to_string()],
        evidence_digest: "ev_digest_blake3_hex".to_string(),
    };

    let edge = Edge {
        source: SymbolId::new("ui::view#fn:render:sig"),
        target: SymbolId::new("db::conn#fn:query:sig"),
        kind: EdgeKind::Calls,
        coverage: CoverageState::Verified,
        location: Some(SourceLocation {
            path: "ui/view.tsx".to_string(),
            start_line: 43,
            end_line: 43,
            start_col: Some(10),
            end_col: Some(25),
            symbol_id: None,
        }),
        provenance: "ast_visitor_call_expr".to_string(),
    };

    let contract = Contract {
        owner: SymbolId::new("db::conn#fn:query:sig"),
        visibility: Visibility::Public,
        inputs: vec!["sql: string".to_string()],
        output: Some("Promise<QueryResult>".to_string()),
        effects: vec!["io:network".to_string()],
    };

    let symbol = Symbol {
        stable_id: SymbolId::new("ui::view#fn:render:sig"),
        language_id: "typescript".to_string(),
        kind: "function".to_string(),
        qualified_name: "ui.view.render".to_string(),
        range: None,
        normalized_signature: Some("() => JSX.Element".to_string()),
        visibility: Visibility::Public,
    };

    // Serialize and verify roundtrip integrity
    let f_json = serde_json::to_string(&finding).unwrap();
    let ev_json = serde_json::to_string(&evidence).unwrap();
    let ed_json = serde_json::to_string(&edge).unwrap();
    let c_json = serde_json::to_string(&contract).unwrap();
    let s_json = serde_json::to_string(&symbol).unwrap();

    let f_back: Finding = serde_json::from_str(&f_json).unwrap();
    let ev_back: Evidence = serde_json::from_str(&ev_json).unwrap();
    let ed_back: Edge = serde_json::from_str(&ed_json).unwrap();
    let c_back: Contract = serde_json::from_str(&c_json).unwrap();
    let s_back: Symbol = serde_json::from_str(&s_json).unwrap();

    assert_eq!(finding, f_back);
    assert_eq!(evidence, ev_back);
    assert_eq!(edge, ed_back);
    assert_eq!(contract, c_back);
    assert_eq!(symbol, s_back);
}
