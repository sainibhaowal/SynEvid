use autopsy_repo::{AutopsyConfig, InvariantsConfig, RepoError};
use autopsy_tests::TestSandbox;
use std::path::Path;

#[test]
fn test_e2e_valid_autopsy_toml_loading() {
    let sandbox = TestSandbox::new("valid_cfg");
    let valid_toml = r#"
        config_version = "0.0.1"
        [repository]
        roots = ["src", "crates"]
        exclude = ["target/**", "node_modules/**"]
        [analysis]
        default_profile = "full"
        max_traversal_nodes = 25000
        [cache]
        directory = ".autopsy/cache"
    "#;
    let path = sandbox.write_autopsy_toml(valid_toml);

    let config = AutopsyConfig::load_from_file(&path).expect("Valid config must load successfully");
    assert_eq!(config.config_version, "0.0.1");
    assert_eq!(config.repository.roots, vec!["src", "crates"]);
    assert_eq!(config.analysis.max_traversal_nodes, 25000);
}

#[test]
fn test_e2e_missing_autopsy_toml() {
    let missing_path = Path::new("/path/that/does/not/exist/autopsy.toml");
    let err = AutopsyConfig::load_from_file(missing_path).unwrap_err();
    match err {
        RepoError::ConfigNotFound(p) => assert_eq!(p, missing_path),
        other => panic!("Expected ConfigNotFound, got {:?}", other),
    }
}

#[test]
fn test_e2e_invalid_toml_syntax() {
    let sandbox = TestSandbox::new("invalid_syntax");
    let bad_toml = "this is not valid toml = [[";
    let path = sandbox.write_autopsy_toml(bad_toml);

    let err = AutopsyConfig::load_from_file(&path).unwrap_err();
    match err {
        RepoError::ConfigParseError { path: p, reason } => {
            assert_eq!(p, path);
            assert!(!reason.is_empty());
        }
        other => panic!("Expected ConfigParseError, got {:?}", other),
    }
}

#[test]
fn test_e2e_invalid_config_semantics() {
    let sandbox = TestSandbox::new("invalid_semantics");

    // Case 1: Empty roots
    let no_roots_toml = r#"
        config_version = "0.0.1"
        [repository]
        roots = []
        exclude = []
        [analysis]
        default_profile = "pr"
        max_traversal_nodes = 1000
        [cache]
        directory = ".autopsy/cache"
    "#;
    let path1 = sandbox.write_autopsy_toml(no_roots_toml);
    let err1 = AutopsyConfig::load_from_file(&path1).unwrap_err();
    assert!(matches!(err1, RepoError::InvalidConfig { .. }));

    // Case 2: Zero max_traversal_nodes
    let zero_nodes_toml = r#"
        config_version = "0.0.1"
        [repository]
        roots = ["."]
        exclude = []
        [analysis]
        default_profile = "pr"
        max_traversal_nodes = 0
        [cache]
        directory = ".autopsy/cache"
    "#;
    let path2 = sandbox.write_autopsy_toml(zero_nodes_toml);
    let err2 = AutopsyConfig::load_from_file(&path2).unwrap_err();
    assert!(matches!(err2, RepoError::InvalidConfig { .. }));

    // Case 3: Malformed glob pattern
    let bad_glob_toml = r#"
        config_version = "0.0.1"
        [repository]
        roots = ["."]
        exclude = ["[invalid-glob"]
        [analysis]
        default_profile = "pr"
        max_traversal_nodes = 1000
        [cache]
        directory = ".autopsy/cache"
    "#;
    let path3 = sandbox.write_autopsy_toml(bad_glob_toml);
    let err3 = AutopsyConfig::load_from_file(&path3).unwrap_err();
    assert!(matches!(err3, RepoError::InvalidGlobPattern { .. }));
}

#[test]
fn test_e2e_valid_invariants_loading() {
    let sandbox = TestSandbox::new("valid_inv");
    let valid_yaml = r#"
        version: "0.0.1"
        invariants:
          - id: RULE_CLEAN_ARCH
            description: Clean architecture rule
            kind: forbidden_dependency
            severity: error
            scope:
              source: "crates/autopsy-domain/**"
              target: "crates/autopsy-cli/**"
          - id: RULE_WARN_LONG_METHOD
            description: Method length warning
            kind: threshold
            severity: warning
    "#;
    let path = sandbox.write_invariants_yml(valid_yaml);

    let config =
        InvariantsConfig::load_from_file(&path).expect("Valid invariants config must load");
    assert_eq!(config.version, "0.0.1");
    assert_eq!(config.invariants.len(), 2);
    assert_eq!(config.invariants[0].id, "RULE_CLEAN_ARCH");
    assert_eq!(config.invariants[0].severity, "error");
    assert_eq!(config.invariants[1].severity, "warning");
}

#[test]
fn test_e2e_invalid_invariants_diagnostics() {
    let sandbox = TestSandbox::new("invalid_inv");

    // Case 1: Invalid severity
    let bad_severity_yaml = r#"
        version: "0.0.1"
        invariants:
          - id: RULE_1
            description: Test
            kind: rule
            severity: catastrophic
    "#;
    let path1 = sandbox.write_invariants_yml(bad_severity_yaml);
    let err1 = InvariantsConfig::load_from_file(&path1).unwrap_err();
    match err1 {
        RepoError::InvalidConfig { reason, .. } => {
            assert!(reason.contains("unknown severity 'catastrophic'"));
        }
        other => panic!("Expected InvalidConfig, got {:?}", other),
    }

    // Case 2: Illegal characters in invariant ID
    let bad_id_yaml = r#"
        version: "0.0.1"
        invariants:
          - id: "RULE WITH SPACES & SYMBOLS!"
            description: Test
            kind: rule
            severity: error
    "#;
    let path2 = sandbox.write_invariants_yml(bad_id_yaml);
    let err2 = InvariantsConfig::load_from_file(&path2).unwrap_err();
    match err2 {
        RepoError::InvalidConfig { reason, .. } => {
            assert!(reason.contains("contains invalid characters"));
        }
        other => panic!("Expected InvalidConfig, got {:?}", other),
    }
}
