use autopsy_behavior_research::compare_outputs;

#[test]
fn detects_divergence() {
    let base = vec!["1".into(), "2".into()];
    let head = vec!["1".into(), "3".into()];
    let r = compare_outputs(&base, &head).unwrap();
    assert_eq!(r.divergences.len(), 1);
    assert_eq!(r.coverage, autopsy_domain::CoverageState::Partial);
}

#[test]
fn length_mismatch_errors() {
    let base = vec!["1".into()];
    let head = vec!["1".into(), "2".into()];
    assert!(compare_outputs(&base, &head).is_err());
}

#[test]
fn deterministic_digest() {
    let base = vec!["a".into()];
    let head = vec!["b".into()];
    let r1 = compare_outputs(&base, &head).unwrap();
    let r2 = compare_outputs(&base, &head).unwrap();
    assert_eq!(r1.digest, r2.digest);
}
