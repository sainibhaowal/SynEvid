use autopsy_domain::CoverageState;
use autopsy_workspace_registry::{Registry, RegistryError, RepoEntry, federation_coverage};

#[test]
fn register_and_summary() {
    let mut r = Registry::new();
    r.register(RepoEntry {
        id: "a".into(),
        provider: "github.com".into(),
        org: "o".into(),
        repo: "a".into(),
        accessible: true,
    })
    .unwrap();
    r.register(RepoEntry {
        id: "b".into(),
        provider: "github.com".into(),
        org: "o".into(),
        repo: "b".into(),
        accessible: false,
    })
    .unwrap();
    assert_eq!(r.coverage_summary(), (2, 1, 1, 1));
    assert_eq!(federation_coverage(&r), CoverageState::Partial);
}

#[test]
fn duplicate_rejected() {
    let mut r = Registry::new();
    r.register(RepoEntry {
        id: "a".into(),
        provider: "github.com".into(),
        org: "o".into(),
        repo: "a".into(),
        accessible: true,
    })
    .unwrap();
    assert!(matches!(
        r.register(RepoEntry {
            id: "a".into(),
            provider: "github.com".into(),
            org: "o".into(),
            repo: "a".into(),
            accessible: true
        }),
        Err(RegistryError::DuplicateRepo(_))
    ));
}

#[test]
fn empty_unknown() {
    let r = Registry::new();
    assert_eq!(federation_coverage(&r), CoverageState::Unknown);
}

#[test]
fn deterministic_digest() {
    let mut r = Registry::new();
    r.register(RepoEntry {
        id: "a".into(),
        provider: "github.com".into(),
        org: "o".into(),
        repo: "a".into(),
        accessible: true,
    })
    .unwrap();
    let d1 = r.digest();
    let d2 = r.digest();
    assert_eq!(d1, d2);
}
