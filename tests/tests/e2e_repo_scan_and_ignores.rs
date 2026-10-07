use autopsy_repo::{classify_language, is_generated_code, scan_repository};
use autopsy_tests::TestSandbox;
use std::path::Path;

#[test]
fn test_e2e_ignore_patterns_and_exclusions() {
    let sandbox = TestSandbox::new("ignores");

    sandbox.write_file("src/app.ts", "console.log('app');");
    sandbox.write_file("target/binary", "ELF...");
    sandbox.write_file("dist/bundle.js", "bundled code");
    sandbox.write_file("node_modules/pkg/index.js", "npm module");
    sandbox.write_file("docs/manual.md", "# Documentation");
    sandbox.write_file(".hidden_file", "secret");

    let config = sandbox.default_config();
    let snapshot = scan_repository(&sandbox.root, &config, "0.0.1", None).unwrap();

    // Verify exclusions
    assert!(snapshot.files.contains_key("src/app.ts"));
    assert!(snapshot.files.contains_key("docs/manual.md"));
    assert!(
        !snapshot.files.contains_key("target/binary"),
        "target/ must be excluded"
    );
    assert!(
        !snapshot.files.contains_key("dist/bundle.js"),
        "dist/ must be excluded"
    );
    assert!(
        !snapshot.files.contains_key("node_modules/pkg/index.js"),
        "node_modules/ must be excluded"
    );
}

#[test]
fn test_e2e_multiple_roots_scan() {
    let sandbox = TestSandbox::new("multi_roots");

    sandbox.write_file("frontend/src/index.tsx", "export const UI = () => null;");
    sandbox.write_file("backend/src/main.rs", "fn main() {}");
    sandbox.write_file("ignored_folder/file.txt", "skip me");

    let mut config = sandbox.default_config();
    config.repository.roots = vec!["frontend".to_string(), "backend".to_string()];

    let snapshot = scan_repository(&sandbox.root, &config, "0.0.1", None).unwrap();

    assert_eq!(snapshot.files.len(), 2);
    assert!(snapshot.files.contains_key("frontend/src/index.tsx"));
    assert!(snapshot.files.contains_key("backend/src/main.rs"));
    assert!(!snapshot.files.contains_key("ignored_folder/file.txt"));
}

#[test]
fn test_e2e_generated_code_detection() {
    let sandbox = TestSandbox::new("gen_detect");

    let manual = sandbox.write_file("src/normal.ts", "export const x = 1;");
    let gen1 = sandbox.write_file("src/api_gen.ts", "// @generated\nexport const y = 2;");
    let gen2 = sandbox.write_file("src/schema.rs", "// DO NOT EDIT\npub struct S;");

    let content_manual = std::fs::read(&manual).unwrap();
    let content_gen1 = std::fs::read(&gen1).unwrap();
    let content_gen2 = std::fs::read(&gen2).unwrap();

    assert!(!is_generated_code(&content_manual));
    assert!(is_generated_code(&content_gen1));
    assert!(is_generated_code(&content_gen2));

    let config = sandbox.default_config();
    let snapshot = scan_repository(&sandbox.root, &config, "0.0.1", None).unwrap();

    assert!(!snapshot.files["src/normal.ts"].is_generated);
    assert!(snapshot.files["src/api_gen.ts"].is_generated);
    assert!(snapshot.files["src/schema.rs"].is_generated);
}

#[test]
fn test_e2e_language_classification() {
    assert_eq!(classify_language(Path::new("app.ts")), "typescript");
    assert_eq!(classify_language(Path::new("app.tsx")), "typescript");
    assert_eq!(classify_language(Path::new("index.js")), "javascript");
    assert_eq!(classify_language(Path::new("lib.rs")), "rust");
    assert_eq!(classify_language(Path::new("script.py")), "python");
    assert_eq!(classify_language(Path::new("data.json")), "json");
    assert_eq!(classify_language(Path::new("config.toml")), "toml");
    assert_eq!(classify_language(Path::new("spec.yaml")), "yaml");
    assert_eq!(classify_language(Path::new("doc.md")), "markdown");
    assert_eq!(classify_language(Path::new("unknown_ext.xyz")), "xyz");
    assert_eq!(classify_language(Path::new("no_extension")), "unknown");
}
