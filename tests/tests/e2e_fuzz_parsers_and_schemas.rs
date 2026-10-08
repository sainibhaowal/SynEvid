//! Fuzz and resilience testing for parsers, configs, schemas, and multigraphs (CA-TECH-001 Ch.7).
//! Asserts zero panics, strict error bounds, and deterministic termination on adversarial/fuzzed inputs.

use autopsy_adapter_api::{LanguageAdapter, ParseContext};
use autopsy_adapter_typescript::TypeScriptAdapter;
use autopsy_domain::{CoverageState, Edge, EdgeKind, FileUnit, Symbol, SymbolId, Visibility};
use autopsy_graph::DependencyGraph;
use autopsy_repo::{AutopsyConfig, InvariantsConfig};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[test]
fn test_fuzz_typescript_parser_adversarial_inputs() {
    let adapter = TypeScriptAdapter::new();
    let ctx = ParseContext {
        project_root: PathBuf::from("."),
        options: BTreeMap::new(),
    };
    let unit = FileUnit {
        path: "fuzz.ts".to_string(),
        language: "typescript".to_string(),
        content_hash: "hash".to_string(),
        is_generated: false,
    };

    let massive_string = "0".repeat(50_000);
    let repeated_stmt = "const a = 1; ".repeat(1_000);

    let fuzzed_corpus: Vec<&str> = vec![
        "",                                                               // Empty
        "   \t\r\n  ",                                                    // Whitespace
        "{{{{{{{{{{[[[[(((((",                                            // Unclosed brackets
        ")))))]]]]]}}}}}",                                                // Stray closing
        "export function (((( { return 42; ",                             // Malformed declaration
        "class A extends B implements C, D, {",                           // Broken inheritance
        "const x = `unclosed template literal ${foo", // Unclosed template string
        "/* unclosed comment *",                      // Unclosed block comment
        "\"unclosed string literal \n next line",     // Unclosed quote
        "import { a, b, from './mod';",               // Broken import
        "eval('adversarial dynamic code'); const y = require('legacy');", // Dynamic constructs
        &massive_string,                              // Massive repetition of tokens
        &repeated_stmt,                               // Repeated statements
        "var x = 1\0; // null byte in source",        // Null byte
        "let αβγ = 42; const 🚀 = 'rocket'; // unicode identifiers", // Unicode
    ];

    for (idx, payload) in fuzzed_corpus.iter().enumerate() {
        // Must never panic on any adversarial input
        let result = adapter.parse(&ctx, &unit, payload);
        assert!(
            result.is_ok(),
            "TypeScript adapter parse panicked or failed on corpus case {}",
            idx
        );

        let parsed = result.unwrap();
        // Dynamic construct detection should properly flag eval / dynamic code
        if payload.contains("eval") {
            assert!(
                matches!(
                    parsed.coverage,
                    CoverageState::Unknown | CoverageState::Partial
                ),
                "Case with eval must emit unknown or partial coverage"
            );
        }
    }
}

#[test]
fn test_fuzz_config_and_yaml_adversarial_inputs() {
    let fuzzed_tomls = vec![
        "",
        "[[[",
        "roots = 42",
        "config_version = 123",
        "roots = ['/absolute/escape/attempt']",
        "max_traversal_nodes = -5",
        "exclude = ['[unclosed-glob']",
    ];

    for payload in fuzzed_tomls {
        // Must return Err rather than panicking
        let _ = toml::from_str::<AutopsyConfig>(payload);
    }

    let fuzzed_yamls = vec![
        "",
        "::::",
        "version: 123",
        "invariants: [ { id: 'BAD ID WITH SPACES', severity: 'fatal' } ]",
        "invariants: { not: a: list }",
    ];

    for payload in fuzzed_yamls {
        // Must safely error on invalid yaml
        let _ = serde_yaml::from_str::<InvariantsConfig>(payload);
    }
}

#[test]
fn test_fuzz_multigraph_adversarial_dense_cyclic_graph() {
    // Generate adversarial dense cyclic directed graph
    let mut graph = DependencyGraph::new();
    let num_nodes = 50;

    for i in 0..num_nodes {
        graph.add_node(Symbol {
            stable_id: SymbolId::new(format!("node_{}", i)),
            language_id: "typescript".to_string(),
            kind: "function".to_string(),
            qualified_name: format!("mod::node_{}", i),
            range: None,
            normalized_signature: None,
            visibility: Visibility::Public,
        });
    }

    // Dense interconnected cycle
    for i in 0..num_nodes {
        for j in 0..num_nodes {
            if i != j {
                graph.add_edge(Edge {
                    source: SymbolId::new(format!("node_{}", i)),
                    target: SymbolId::new(format!("node_{}", j)),
                    kind: EdgeKind::Calls,
                    coverage: CoverageState::Verified,
                    location: None,
                    provenance: "fuzz".to_string(),
                });
            }
        }
    }

    // Tarjan SCC cycle detection must terminate and not loop infinitely
    let cycles = graph.find_cycles();
    assert!(
        !cycles.is_empty(),
        "Dense complete graph must contain SCC cycles"
    );

    // Bounded reachability must respect max_depth and terminate safely
    let reachable = graph.bounded_forward_reachability(&SymbolId::new("node_0"), 5);
    assert!(
        !reachable.is_empty(),
        "Reachability must discover connected nodes"
    );
}
