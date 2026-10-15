//! Temporal facts: the append-only evolution journal (Step 30).
//!
//! Every semantic assertion about the codebase is recorded as an immutable
//! [`TemporalFact`] with an explicit half-open validity interval
//! `[valid_from, valid_to)`. Nothing is ever mutated in place: a change
//! *closes* one interval and *opens* another.
//!
//! # Digests
//! `fact_id = BLAKE3(subject ‖ predicate ‖ object ‖ valid_from)` per
//! `03_4.2_STANDARDS_SCHEMAS_VERSIONING.md` §3.1. The digest deliberately
//! excludes `valid_to` and every wall-clock field, so closing an interval
//! never changes an identity (`NFR-001`, `FR-028`).

use autopsy_domain::CoverageState;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Canonical predicate vocabulary for temporal facts.
///
/// Mirrors [`autopsy_domain::EdgeKind`] but is a separate type: a fact
/// predicate is a *historical assertion*, whereas an edge is a *present-tense
/// graph relation*. Keeping them distinct prevents accidental conversion of a
/// closed historical fact back into a live edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactPredicate {
    /// A file declares a symbol.
    Declares,
    /// A symbol references another symbol.
    References,
    /// A symbol calls another symbol.
    Calls,
    /// A symbol implements an interface.
    Implements,
    /// A symbol overrides a base member.
    Overrides,
    /// A file or directory contains another path.
    Contains,
    /// A file imports another module.
    Imports,
    /// A symbol inherits from a base type.
    Inherits,
    /// A test suite covers a symbol.
    TestCovers,
    /// A file existed with a given content digest.
    FilePresent,
}

impl FactPredicate {
    /// Stable wire name, used inside the canonical digest input.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Declares => "declares",
            Self::References => "references",
            Self::Calls => "calls",
            Self::Implements => "implements",
            Self::Overrides => "overrides",
            Self::Contains => "contains",
            Self::Imports => "imports",
            Self::Inherits => "inherits",
            Self::TestCovers => "test_covers",
            Self::FilePresent => "file_present",
        }
    }

    /// Every variant, for exhaustive iteration in tests and schema emission.
    pub fn all() -> &'static [FactPredicate] {
        &[
            Self::Declares,
            Self::References,
            Self::Calls,
            Self::Implements,
            Self::Overrides,
            Self::Contains,
            Self::Imports,
            Self::Inherits,
            Self::TestCovers,
            Self::FilePresent,
        ]
    }
}

impl fmt::Display for FactPredicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Content-addressed identifier of a temporal fact.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FactId(pub String);

impl FactId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FactId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An immutable assertion with an explicit validity interval.
///
/// `valid_to == None` means the fact is still active at the analyzed head.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TemporalFact {
    pub fact_id: FactId,
    /// Canonical identity of the asserting entity.
    pub subject: String,
    pub predicate: FactPredicate,
    /// Canonical identity of the asserted target, or an opaque value.
    pub object: String,
    /// Commit introducing the assertion.
    pub valid_from: String,
    /// Commit that retired the assertion; `None` while active.
    pub valid_to: Option<String>,
    /// BLAKE3 of the content that proves this fact (AST slice or blob digest).
    pub evidence_digest: String,
    /// Honest coverage of the extraction that produced this fact.
    pub coverage: CoverageState,
    /// Provenance label, e.g. `git:tree` or `adapter:typescript`.
    pub provenance: String,
}

impl TemporalFact {
    /// Computes the canonical fact digest.
    ///
    /// Input is `subject ‖ predicate ‖ object ‖ valid_from` joined with a
    /// single NUL separator. A NUL cannot appear in any canonical identifier
    /// (see `RFC-GLOBAL-001` character classes), so the encoding is
    /// unambiguous and length-extension-free for our purposes.
    pub fn compute_id(
        subject: &str,
        predicate: FactPredicate,
        object: &str,
        valid_from: &str,
    ) -> FactId {
        let mut hasher = blake3::Hasher::new();
        let mut feed = |part: &str| {
            hasher.update(part.as_bytes());
            hasher.update(&[0u8]);
        };
        feed(subject);
        feed(predicate.as_str());
        feed(object);
        feed(valid_from);
        FactId(hasher.finalize().to_hex().to_string())
    }

    /// Builds a fact, deriving its id from the canonical inputs.
    pub fn new(
        subject: impl Into<String>,
        predicate: FactPredicate,
        object: impl Into<String>,
        valid_from: impl Into<String>,
        evidence_digest: impl Into<String>,
        coverage: CoverageState,
        provenance: impl Into<String>,
    ) -> Self {
        let subject = subject.into();
        let object = object.into();
        let valid_from = valid_from.into();
        let fact_id = Self::compute_id(&subject, predicate, &object, &valid_from);
        Self {
            fact_id,
            subject,
            predicate,
            object,
            valid_from,
            valid_to: None,
            evidence_digest: evidence_digest.into(),
            coverage,
            provenance: provenance.into(),
        }
    }

    /// True when the fact is still active (no closing commit).
    pub fn is_active(&self) -> bool {
        self.valid_to.is_none()
    }

    /// The interval key used to match facts across commits.
    ///
    /// Excludes the commit that introduced the fact, so the same assertion
    /// re-observed at a later revision is recognized as a continuation rather
    /// than a new fact.
    pub fn identity_key(&self) -> (&str, FactPredicate, &str) {
        (self.subject.as_str(), self.predicate, self.object.as_str())
    }
}

/// Number of facts in a journal, split by coverage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalStats {
    pub total: usize,
    pub active: usize,
    pub retired: usize,
    pub partial: usize,
    pub unknown: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fact_id_is_deterministic() {
        let a = TemporalFact::compute_id("s", FactPredicate::Calls, "o", "c1");
        let b = TemporalFact::compute_id("s", FactPredicate::Calls, "o", "c1");
        assert_eq!(a, b);
        assert_eq!(a.as_str().len(), 64, "blake3 hex digest");
    }

    #[test]
    fn test_fact_id_changes_with_every_input() {
        let base = TemporalFact::compute_id("s", FactPredicate::Calls, "o", "c1");
        assert_ne!(
            base,
            TemporalFact::compute_id("s2", FactPredicate::Calls, "o", "c1")
        );
        assert_ne!(
            base,
            TemporalFact::compute_id("s", FactPredicate::Declares, "o", "c1")
        );
        assert_ne!(
            base,
            TemporalFact::compute_id("s", FactPredicate::Calls, "o2", "c1")
        );
        assert_ne!(
            base,
            TemporalFact::compute_id("s", FactPredicate::Calls, "o", "c2")
        );
    }

    #[test]
    fn test_fact_id_excludes_valid_to() {
        // Closing an interval must not change the identity of the fact.
        let mut fact = TemporalFact::new(
            "s",
            FactPredicate::Calls,
            "o",
            "c1",
            "ev",
            CoverageState::Verified,
            "test",
        );
        let before = fact.fact_id.clone();
        fact.valid_to = Some("c2".to_string());
        assert_eq!(fact.fact_id, before);
    }

    #[test]
    fn test_fact_id_encoding_is_unambiguous() {
        // Without a separator, ("ab","c") and ("a","bc") would collide.
        let a = TemporalFact::compute_id("ab", FactPredicate::Calls, "c", "d");
        let b = TemporalFact::compute_id("a", FactPredicate::Calls, "bc", "d");
        assert_ne!(a, b, "NUL separator must prevent boundary ambiguity");
    }

    #[test]
    fn test_identity_key_ignores_valid_from() {
        let f1 = TemporalFact::new(
            "s",
            FactPredicate::Calls,
            "o",
            "c1",
            "e",
            CoverageState::Verified,
            "p",
        );
        let f2 = TemporalFact::new(
            "s",
            FactPredicate::Calls,
            "o",
            "c2",
            "e",
            CoverageState::Verified,
            "p",
        );
        assert_eq!(f1.identity_key(), f2.identity_key());
        assert_ne!(f1.fact_id, f2.fact_id);
    }

    #[test]
    fn test_predicate_roundtrip_all_variants() {
        for p in FactPredicate::all() {
            let json = serde_json::to_string(p).unwrap();
            let back: FactPredicate = serde_json::from_str(&json).unwrap();
            assert_eq!(*p, back);
            assert!(!p.as_str().is_empty());
            assert_eq!(p.to_string(), p.as_str());
        }
    }

    #[test]
    fn test_fact_serde_roundtrip() {
        let fact = TemporalFact::new(
            "urn:autopsy:repo:github.com:o:r#a.ts",
            FactPredicate::FilePresent,
            "blake3:abc",
            "deadbeef",
            "ev",
            CoverageState::Verified,
            "git:tree",
        );
        let json = serde_json::to_string(&fact).unwrap();
        let back: TemporalFact = serde_json::from_str(&json).unwrap();
        assert_eq!(fact, back);
    }

    #[test]
    fn test_ordering_is_stable_for_btreeset() {
        use std::collections::BTreeSet;
        let mut set = BTreeSet::new();
        let f = || {
            TemporalFact::new(
                "s",
                FactPredicate::Calls,
                "o",
                "c1",
                "e",
                CoverageState::Verified,
                "p",
            )
        };
        set.insert(f());
        set.insert(f());
        assert_eq!(set.len(), 1, "identical facts must deduplicate");
    }

    #[test]
    fn test_is_active() {
        let mut f = TemporalFact::new(
            "s",
            FactPredicate::Calls,
            "o",
            "c1",
            "e",
            CoverageState::Verified,
            "p",
        );
        assert!(f.is_active());
        f.valid_to = Some("c2".into());
        assert!(!f.is_active());
    }
}
