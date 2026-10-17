use autopsy_schema_registry::{CompatibilityMode, Schema, SchemaField, compare};

fn s(name: &str, fields: &[(&str, bool)]) -> Schema {
    Schema {
        subject: name.into(),
        fields: fields
            .iter()
            .map(|(n, r)| SchemaField {
                name: n.to_string(),
                required: *r,
            })
            .collect(),
    }
}

#[test]
fn backward_allows_addition() {
    let old = s("x", &[("a", true)]);
    let new = s("x", &[("a", true), ("b", false)]);
    let r = compare(&old, &new, CompatibilityMode::Backward);
    assert!(r.compatible);
    assert!(r.added_fields.contains(&"b".to_string()));
}

#[test]
fn backward_rejects_removal() {
    let old = s("x", &[("a", true), ("b", false)]);
    let new = s("x", &[("a", true)]);
    let r = compare(&old, &new, CompatibilityMode::Backward);
    assert!(!r.compatible);
    assert!(r.removed_fields.contains(&"b".to_string()));
}

#[test]
fn strict_requires_exact() {
    let old = s("x", &[("a", true)]);
    let new = s("x", &[("a", true), ("b", false)]);
    let r = compare(&old, &new, CompatibilityMode::Strict);
    assert!(!r.compatible);
}

#[test]
fn deterministic_digest() {
    let old = s("x", &[("a", true)]);
    let new = s("x", &[("a", true)]);
    let r1 = compare(&old, &new, CompatibilityMode::Full);
    let r2 = compare(&old, &new, CompatibilityMode::Full);
    assert_eq!(r1.digest, r2.digest);
    assert!(r1.compatible);
}
