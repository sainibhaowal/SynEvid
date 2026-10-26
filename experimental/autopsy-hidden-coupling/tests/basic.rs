use autopsy_hidden_coupling::scan;
use std::fs;
use tempfile::tempdir;

#[test]
fn finds_shared_env_var() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.ts"), "const x = process.env.API_KEY;").unwrap();
    fs::write(dir.path().join("b.ts"), "if (process.env.API_KEY) {}").unwrap();
    let report = scan(dir.path()).unwrap();
    assert_eq!(report.files_scanned, 2);
    assert!(!report.edges.is_empty());
    assert_eq!(report.coverage, autopsy_domain::CoverageState::Partial);
}

#[test]
fn empty_root_is_unknown() {
    let dir = tempdir().unwrap();
    let report = scan(dir.path()).unwrap();
    assert_eq!(report.coverage, autopsy_domain::CoverageState::Unknown);
}

#[test]
fn deterministic_digest() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.ts"), "SELECT * FROM users;").unwrap();
    fs::write(dir.path().join("b.ts"), "SELECT * FROM users;").unwrap();
    let r1 = scan(dir.path()).unwrap();
    let r2 = scan(dir.path()).unwrap();
    assert_eq!(r1.digest, r2.digest);
}
