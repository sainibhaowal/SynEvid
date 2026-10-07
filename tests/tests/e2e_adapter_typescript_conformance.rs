//! E2E integration test: TypeScript Language Adapter Conformance, Symbol Extraction,
//! Cross-File Edge Resolution, and Honest Coverage States (FR-003, FR-004, FR-005, FR-009, FR-030).

use autopsy_adapter_api::conformance::verify_adapter_conformance;
use autopsy_adapter_api::{LanguageAdapter, ParseContext};
use autopsy_adapter_typescript::TypeScriptAdapter;
use autopsy_domain::{CoverageState, EdgeKind, FileUnit, Visibility};
use autopsy_tests::TestSandbox;
use std::collections::BTreeMap;
use std::path::Path;

#[test]
fn test_e2e_ts_adapter_conformance_and_capabilities() {
    let adapter = TypeScriptAdapter::new();

    let valid_ts = r#"
        export interface UserService {
            getUser(id: string): Promise<User>;
        }

        export class DefaultUserService implements UserService {
            public async getUser(id: string): Promise<User> {
                return { id, name: "Alice" };
            }
        }
    "#;

    let invalid_ts = r#"
        class Broken { def not_ts_syntax( : ===
    "#;

    let dynamic_ts = r#"
        export function loadPlugin(name: string): any {
            return eval(`require('${name}')`);
        }
    "#;

    verify_adapter_conformance(
        &adapter,
        valid_ts,
        invalid_ts,
        dynamic_ts,
        "src/user_service.ts",
    );
}

#[test]
fn test_e2e_ts_real_repo_parsing_and_symbol_extraction() {
    let sandbox = TestSandbox::new("ts_real_repo");

    let model_code = r#"
        export interface User {
            id: string;
            email: string;
            active: boolean;
        }

        export class Account {
            public owner: string;

            constructor(owner: string) {
                this.owner = owner;
            }

            public deposit(amount: number): boolean {
                return amount > 0;
            }
        }
    "#;

    let service_code = r#"
        import { User, Account } from "../models/user";

        export class AuthService {
            private account: Account;

            constructor(account: Account) {
                this.account = account;
            }

            public authenticate(user: User, token: string): boolean {
                return user.active && token.length > 8;
            }
        }
    "#;

    sandbox.write_file("src/models/user.ts", model_code);
    sandbox.write_file("src/services/auth.ts", service_code);

    let adapter = TypeScriptAdapter::with_root_dir(&sandbox.root);
    let ctx = ParseContext::new(&sandbox.root);

    // 1. Discover
    assert!(adapter.discover(Path::new("src/models/user.ts")));
    assert!(adapter.discover(Path::new("src/services/auth.ts")));

    // 2. Parse model
    let model_file = FileUnit {
        path: "src/models/user.ts".to_string(),
        language: "typescript".to_string(),
        content_hash: "hash_user".to_string(),
        is_generated: false,
    };
    let parsed_model = adapter.parse(&ctx, &model_file, model_code).unwrap();
    assert!(parsed_model.syntax_valid);
    assert_eq!(parsed_model.coverage, CoverageState::Verified);

    let model_symbols = adapter.extract_symbols(&parsed_model).unwrap();
    assert!(
        model_symbols
            .iter()
            .any(|s| s.kind == "interface" && s.qualified_name == "src/models/user.ts::User")
    );
    assert!(
        model_symbols
            .iter()
            .any(|s| s.kind == "class" && s.qualified_name == "src/models/user.ts::Account")
    );
    assert!(
        model_symbols
            .iter()
            .any(|s| s.kind == "method"
                && s.qualified_name == "src/models/user.ts::Account::deposit")
    );

    // 3. Contracts
    let contracts = adapter.extract_contracts(&parsed_model).unwrap();
    let deposit_contract = contracts
        .iter()
        .find(|c| c.owner.as_str().contains("deposit"))
        .expect("Must extract deposit contract");
    assert_eq!(deposit_contract.visibility, Visibility::Public);
    assert_eq!(deposit_contract.inputs, vec!["amount: number"]);
    assert_eq!(deposit_contract.output, Some("boolean".to_string()));

    // 4. Cross-file Import Edges
    let auth_file = FileUnit {
        path: "src/services/auth.ts".to_string(),
        language: "typescript".to_string(),
        content_hash: "hash_auth".to_string(),
        is_generated: false,
    };
    let parsed_auth = adapter.parse(&ctx, &auth_file, service_code).unwrap();
    assert!(parsed_auth.syntax_valid);
    assert_eq!(parsed_auth.coverage, CoverageState::Verified);

    let auth_edges = adapter
        .extract_edges(&parsed_auth, &BTreeMap::new())
        .unwrap();
    let import_edge = auth_edges
        .iter()
        .find(|e| e.kind == EdgeKind::Imports)
        .expect("Must extract cross-file import edge");
    assert_eq!(import_edge.target.as_str(), "file::src/models/user.ts");
}

#[test]
fn test_e2e_ts_honest_coverage_boundary() {
    let sandbox = TestSandbox::new("ts_honest_coverage");

    let eval_code = r#"
        export function dangerousEval(payload: string): void {
            eval(payload);
        }
    "#;

    let dynamic_import_code = r#"
        export async function lazyLoad(route: string): Promise<any> {
            const mod = await import(`./routes/${route}`);
            return mod;
        }
    "#;

    let generated_code = r#"
        // @generated by grpc-tools. DO NOT EDIT.
        export const ServiceDefinition = {};
    "#;

    let adapter = TypeScriptAdapter::new();
    let ctx = ParseContext::new(&sandbox.root);

    // Eval -> Unknown
    let file_eval = FileUnit {
        path: "src/eval.ts".to_string(),
        language: "typescript".to_string(),
        content_hash: "hash_eval".to_string(),
        is_generated: false,
    };
    let parsed_eval = adapter.parse(&ctx, &file_eval, eval_code).unwrap();
    assert_eq!(parsed_eval.coverage, CoverageState::Unknown);

    // Dynamic Import -> Unknown
    let file_dyn = FileUnit {
        path: "src/dyn.ts".to_string(),
        language: "typescript".to_string(),
        content_hash: "hash_dyn".to_string(),
        is_generated: false,
    };
    let parsed_dyn = adapter.parse(&ctx, &file_dyn, dynamic_import_code).unwrap();
    assert_eq!(parsed_dyn.coverage, CoverageState::Unknown);

    // Generated Code -> Partial
    let file_gen = FileUnit {
        path: "src/gen.ts".to_string(),
        language: "typescript".to_string(),
        content_hash: "hash_gen".to_string(),
        is_generated: false,
    };
    let parsed_gen = adapter.parse(&ctx, &file_gen, generated_code).unwrap();
    assert_eq!(parsed_gen.coverage, CoverageState::Partial);

    // Critical Invariant: Neither Unknown nor Partial must ever collapse to Verified
    assert_ne!(parsed_eval.coverage, CoverageState::Verified);
    assert_ne!(parsed_dyn.coverage, CoverageState::Verified);
    assert_ne!(parsed_gen.coverage, CoverageState::Verified);
}

#[test]
fn test_e2e_ts_100_runs_determinism() {
    let adapter = TypeScriptAdapter::new();
    let ctx = ParseContext::new(".");

    let code = r#"
        export interface Repository<T> {
            find(id: string): Promise<T | null>;
            save(entity: T): Promise<void>;
        }

        export class SqlRepository<T> implements Repository<T> {
            public async find(id: string): Promise<T | null> {
                return null;
            }

            public async save(entity: T): Promise<void> {
                // saved
            }
        }
    "#;

    let file = FileUnit {
        path: "src/repo.ts".to_string(),
        language: "typescript".to_string(),
        content_hash: "hash_repo".to_string(),
        is_generated: false,
    };

    let first_parsed = adapter.parse(&ctx, &file, code).unwrap();
    let first_symbols = adapter.extract_symbols(&first_parsed).unwrap();
    let first_contracts = adapter.extract_contracts(&first_parsed).unwrap();
    let first_edges = adapter
        .extract_edges(&first_parsed, &BTreeMap::new())
        .unwrap();

    let golden_symbols_json = serde_json::to_string(&first_symbols).unwrap();
    let golden_contracts_json = serde_json::to_string(&first_contracts).unwrap();
    let golden_edges_json = serde_json::to_string(&first_edges).unwrap();

    for run in 1..=100 {
        let parsed = adapter.parse(&ctx, &file, code).unwrap();
        let symbols = adapter.extract_symbols(&parsed).unwrap();
        let contracts = adapter.extract_contracts(&parsed).unwrap();
        let edges = adapter.extract_edges(&parsed, &BTreeMap::new()).unwrap();

        assert_eq!(
            serde_json::to_string(&symbols).unwrap(),
            golden_symbols_json,
            "Symbols non-deterministic on run {}",
            run
        );
        assert_eq!(
            serde_json::to_string(&contracts).unwrap(),
            golden_contracts_json,
            "Contracts non-deterministic on run {}",
            run
        );
        assert_eq!(
            serde_json::to_string(&edges).unwrap(),
            golden_edges_json,
            "Edges non-deterministic on run {}",
            run
        );
    }
}
