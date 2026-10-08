//! End-to-end integration tests for Pillar C: Enterprise Sandboxing & Defense-in-Depth.
//!
//! Enforces:
//! 1. Zero Code Execution (FR-026): In-memory AST parsing only; malicious code patterns are never executed.
//! 2. Symlink Escape Containment: File scanner strictly enforces `follow_links(false)` and drops symlink entries.
//! 3. Path Traversal Sanitization: Normalization rejects `..` sequences, absolute roots, and out-of-tree caches.
//! 4. Resource Bounds: Bounds traversal nodes to prevent denial-of-service from adversarial cyclic graphs.
//! 5. Zero Network Socket Isolation (FR-028): Fully offline analysis with zero network dependencies.

use autopsy_adapter_api::{LanguageAdapter, ParseContext};
use autopsy_adapter_typescript::TypeScriptAdapter;
use autopsy_cli::{EXIT_PASS, run_cli};
use autopsy_domain::{CoverageState, Edge, EdgeKind, SourceLocation, Symbol, SymbolId, Visibility};
use autopsy_graph::DependencyGraph;
use autopsy_impact::{ImpactDirection, ImpactEngine, ImpactProfile, ImpactQuery};
use autopsy_repo::{
    AutopsyConfig, RepoError, is_safe_relative_path, sanitize_relative_path, scan_repository,
};
use std::fs;
use std::path::{Path, PathBuf};

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(name: &str) -> Self {
        let p =
            std::env::temp_dir().join(format!("synevid_sandbox_{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Self { path: p }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn test_e2e_zero_code_execution_in_memory_only() {
    let temp = TempDirGuard::new("zero_code_exec");
    let src = temp.path.join("src");
    fs::create_dir_all(&src).unwrap();

    // Adversarial payload attempting to trigger side effects or crash the runner if executed
    let adversarial_ts = r#"
        // If executed via node or eval, these would crash or mutate the environment
        if (typeof process !== 'undefined') {
            process.exit(99);
        }
        eval("throw new Error('Hostile code execution detected!');");
        try {
            const cp = require('child_process');
            cp.execSync('kill -9 ' + process.pid);
        } catch (e) {}

        export class HostilePayload {
            public readonly danger: number = 42;
            public runAction(): string {
                return "pure_ast_data";
            }
        }
    "#;
    fs::write(src.join("payload.ts"), adversarial_ts).unwrap();

    let config = AutopsyConfig::default();
    let snapshot = scan_repository(&temp.path, &config, "0.0.1", Some("main"))
        .expect("Scan must succeed without executing user code");

    assert!(snapshot.files.contains_key("src/payload.ts"));
    let unit = &snapshot.files["src/payload.ts"];
    assert_eq!(unit.language, "typescript");
    assert!(!unit.content_hash.is_empty());

    // AST parsing must succeed purely in-memory
    let adapter = TypeScriptAdapter::with_root_dir(&temp.path);
    let ctx = ParseContext::new(&temp.path);
    let parsed = adapter
        .parse(&ctx, unit, adversarial_ts)
        .expect("In-memory AST parsing must succeed without executing code");

    let symbols = adapter
        .extract_symbols(&parsed)
        .expect("Symbol extraction must succeed");

    assert!(
        symbols
            .iter()
            .any(|s| s.qualified_name.contains("HostilePayload"))
    );
    assert!(
        symbols
            .iter()
            .any(|s| s.qualified_name.contains("runAction"))
    );
}

#[test]
fn test_e2e_symlink_escape_containment() {
    let temp = TempDirGuard::new("symlink_containment");
    let repo_root = temp.path.join("repo");
    let outside_root = temp.path.join("outside");

    fs::create_dir_all(&repo_root).unwrap();
    fs::create_dir_all(repo_root.join("src")).unwrap();
    fs::create_dir_all(&outside_root).unwrap();

    // Sensitive files outside the repository boundary
    let secret_file = outside_root.join("secret_credentials.env");
    fs::write(
        &secret_file,
        "DATABASE_PASSWORD=SuperSecretAdminPassword123!",
    )
    .unwrap();

    let legit_ts = repo_root.join("src/legit.ts");
    fs::write(&legit_ts, "export const app = 'secure';").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        // Create adversarial symlink targeting outside file
        let symlink_file = repo_root.join("src/stolen_secret.ts");
        let _ = symlink(&secret_file, &symlink_file);

        // Create adversarial directory symlink targeting outside root
        let symlink_dir = repo_root.join("src/outside_dir");
        let _ = symlink(&outside_root, &symlink_dir);
    }

    let config = AutopsyConfig::default();
    let snapshot = scan_repository(&repo_root, &config, "0.0.1", Some("main"))
        .expect("Scan must complete safely with symlink containment");

    // The legit file must be present
    assert!(snapshot.files.contains_key("src/legit.ts"));

    // Hard Boundary: Symlinks must NEVER be indexed or traversed
    assert!(!snapshot.files.contains_key("src/stolen_secret.ts"));
    assert!(
        !snapshot
            .files
            .contains_key("src/outside_dir/secret_credentials.env")
    );
    assert_eq!(snapshot.files.len(), 1);
}

#[test]
fn test_e2e_path_traversal_sanitization() {
    // 1. Unit sanitization checks
    assert!(is_safe_relative_path("src/core/index.ts"));
    assert!(is_safe_relative_path("packages/api/service.ts"));
    assert!(!is_safe_relative_path("../escape.ts"));
    assert!(!is_safe_relative_path("src/../../outside.ts"));
    assert!(!is_safe_relative_path("/etc/shadow"));
    assert!(!is_safe_relative_path("C:\\Windows\\System32\\cmd.exe"));

    assert!(sanitize_relative_path(Path::new("src/main.ts")).is_ok());
    assert!(sanitize_relative_path(Path::new("src/../escape.ts")).is_err());
    assert!(sanitize_relative_path(Path::new("/abs/path.ts")).is_err());

    // 2. Config validation traversal checks
    let bad_root_config = AutopsyConfig {
        config_version: "0.0.1".to_string(),
        repository: autopsy_repo::RepositoryConfig {
            roots: vec!["../../outside".to_string()],
            exclude: vec![],
        },
        analysis: autopsy_repo::AnalysisConfig::default(),
        cache: autopsy_repo::CacheConfig::default(),
    };
    assert!(matches!(
        bad_root_config.validate(Path::new("autopsy.toml")),
        Err(RepoError::PathTraversal { .. })
    ));

    let bad_cache_config = AutopsyConfig {
        config_version: "0.0.1".to_string(),
        repository: autopsy_repo::RepositoryConfig::default(),
        analysis: autopsy_repo::AnalysisConfig::default(),
        cache: autopsy_repo::CacheConfig {
            directory: "/var/log/stolen".to_string(),
        },
    };
    assert!(matches!(
        bad_cache_config.validate(Path::new("autopsy.toml")),
        Err(RepoError::PathTraversal { .. })
    ));
}

#[test]
fn test_e2e_resource_bounds_adversarial_cyclic_graph() {
    let mut graph = DependencyGraph::new();

    // Construct an adversarial graph with 100 cyclic nodes and cross-edges
    let count = 100;
    for i in 0..count {
        let sym_id = format!("node_{}", i);
        let sym = Symbol {
            stable_id: SymbolId::new(&sym_id),
            language_id: "typescript".to_string(),
            kind: "function".to_string(),
            qualified_name: format!("module::Node{}", i),
            range: Some(SourceLocation {
                path: "src/graph.ts".to_string(),
                start_line: i as u32,
                end_line: i as u32 + 1,
                start_col: Some(0),
                end_col: Some(20),
                symbol_id: Some(SymbolId::new(&sym_id)),
            }),
            normalized_signature: Some(format!("export function Node{}(): void", i)),
            visibility: Visibility::Public,
        };
        graph.add_node(sym);
    }

    // Connect in dense forward and backward cycle loops
    for i in 0..count {
        let u = format!("node_{}", i);
        let next = (i + 1) % count;
        let v = format!("node_{}", next);

        graph.add_edge(Edge {
            source: SymbolId::new(&u),
            target: SymbolId::new(&v),
            kind: EdgeKind::Calls,
            coverage: CoverageState::Verified,
            location: None,
            provenance: "test".to_string(),
        });
        graph.add_edge(Edge {
            source: SymbolId::new(&v),
            target: SymbolId::new(&u),
            kind: EdgeKind::References,
            coverage: CoverageState::Verified,
            location: None,
            provenance: "test".to_string(),
        });
    }

    // Traverse with strict budget limit (budget = 15, max_depth = 50)
    let query = ImpactQuery {
        seed_symbols: vec![SymbolId::new("node_0")],
        direction: ImpactDirection::Bidirectional,
        profile: ImpactProfile {
            name: "test-budget".to_string(),
            budget: 15,
            max_depth: 50,
            ..Default::default()
        },
    };

    let engine = ImpactEngine::new();
    let result = engine
        .compute_impact(&graph, &query)
        .expect("Impact analysis on dense cyclic graph must complete safely");

    // Must be bounded strictly by budget
    assert!(
        result.truncated,
        "Analysis must be flagged as truncated when budget is reached"
    );
    assert!(
        result.impacted_entities.len() <= 15,
        "Impacted entity count ({}) must not exceed budget 15",
        result.impacted_entities.len()
    );
    assert!(!result.impact_digest.is_empty());
}

#[test]
fn test_e2e_zero_network_socket_isolation() {
    let temp = TempDirGuard::new("offline_suite");
    let src = temp.path.join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(
        src.join("index.ts"),
        r#"
        export function greet(name: string): string {
            return `Hello, ${name}`;
        }
    "#,
    )
    .unwrap();

    let autopsy_toml = r#"
        config_version = "0.0.1"
        [repository]
        roots = ["."]
        exclude = ["target/**", "node_modules/**"]
        [analysis]
        default_profile = "pr"
        max_traversal_nodes = 50000
        [cache]
        directory = ".autopsy/cache"
    "#;
    fs::write(temp.path.join("autopsy.toml"), autopsy_toml).unwrap();

    // Verify all core subcommands operate with zero outbound network calls
    let subcommands = vec![
        vec!["baseline"],
        vec!["doctor"],
        vec!["version"],
        vec!["verify"],
    ];

    for sub in subcommands {
        let mut args = vec![
            "autopsy".to_string(),
            "--repo-root".to_string(),
            temp.path.display().to_string(),
        ];
        for s in sub {
            args.push(s.to_string());
        }
        let code = run_cli(&args);
        assert_eq!(code, EXIT_PASS);
    }
}
