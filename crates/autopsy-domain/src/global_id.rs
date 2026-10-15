//! Global entity identity per **RFC-GLOBAL-001**.
//!
//! Scales entity identity beyond a single repository so cross-repo contracts
//! (Pillar 3) can reference symbols in repositories that are not checked out
//! locally.
//!
//! ```text
//! global-entity-id = "urn:autopsy:repo:" provider ":" org ":" repo
//!                    [ "#" path [ ":" symbol-path ] ]
//! provider         = 1*( ALPHA / DIGIT / "." / "-" )
//! org              = 1*( ALPHA / DIGIT / "." / "-" / "_" )
//! repo             = 1*( ALPHA / DIGIT / "." / "-" / "_" )
//! path             = 1*( VCHAR ) ; POSIX forward-slash normalized
//! symbol-path      = 1*( ALPHA / DIGIT / "." / "#" / "_" / "$" )
//! ```
//!
//! Example: `urn:autopsy:repo:github.com:synevid:billing-service#src/api/invoice.ts:createInvoice`
//!
//! # Normalization rules (RFC-GLOBAL-001 §2.2)
//! 1. **Provider** is lowercased to a canonical host form.
//! 2. **Path** is POSIX (`/`) normalized; leading `./` and trailing `/` are
//!    stripped; `..` segments are rejected.
//! 3. **Display names** (e.g. `"Billing Service"`) are metadata attributes and
//!    are never part of the identity key.
//!
//! # Disambiguation of the optional symbol-path
//! Both `path` and the `#` fragment delimiter permit `:` and `#` in the raw
//! grammar, which would make the fragment ambiguous. This implementation
//! resolves it deterministically:
//!
//! * the **first** `#` separates the authority from the fragment; and
//! * within the fragment, the **last** `:` whose right-hand side is a valid
//!   symbol-path (and contains no `/`) separates path from symbol-path.
//!
//! A left-to-right scan with a fixed rule keeps `parse(display(id)) == id`
//! total over the accepted grammar.
//!
//! # Strictness
//! Backslashes are **rejected** rather than rewritten. Two spellings that
//! denote the same entity must not both be able to produce an identity, or
//! identity uniqueness would be violated across platforms. Callers holding
//! Windows-style input must normalize to `/` *before* constructing an ID.

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// Fixed scheme prefix for every global entity identifier.
pub const URN_PREFIX: &str = "urn:autopsy:repo:";

/// Character class for `org` / `repo` authority segments.
const SEGMENT_ALLOWED: &str = "A-Za-z0-9.-_";

/// Upper bound for accepted input length, guarding unbounded allocation.
const MAX_INPUT_LEN: usize = 4096;

/// Failure modes for RFC-GLOBAL-001 parsing.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GlobalIdError {
    #[error("missing `urn:autopsy:repo:` scheme prefix")]
    MissingScheme,
    #[error("identifier exceeds {MAX_INPUT_LEN} byte limit")]
    TooLong,
    #[error("expected `<provider>:<org>:<repo>`; found {0} segment(s)")]
    MalformedAuthority(usize),
    #[error("`{field}` must be non-empty and match [{allowed}]")]
    InvalidSegment {
        field: &'static str,
        allowed: &'static str,
    },
    #[error("fragment must not be empty when `#` is present")]
    EmptyFragment,
    #[error("path segment `..` is forbidden (path traversal)")]
    PathTraversal,
    #[error("path contains a forbidden character (byte 0x{0:02x})")]
    ForbiddenCharacter(u8),
    #[error("fragment is not a valid symbol path")]
    InvalidSymbolPath,
}

/// A parsed, canonicalized global entity identifier.
///
/// Ordering is derived from the canonical string form, so `BTreeMap`/
/// `BTreeSet` iteration is deterministic without a custom `Ord` impl.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct GlobalEntityId {
    provider: String,
    org: String,
    repo: String,
    path: Option<String>,
    symbol_path: Option<String>,
}

impl GlobalEntityId {
    /// Parses and canonicalizes an RFC-GLOBAL-001 identifier.
    pub fn parse(input: &str) -> Result<Self, GlobalIdError> {
        if input.len() > MAX_INPUT_LEN {
            return Err(GlobalIdError::TooLong);
        }

        let rest = input
            .strip_prefix(URN_PREFIX)
            .ok_or(GlobalIdError::MissingScheme)?;

        // First `#` divides authority from fragment (URI convention).
        let (authority, fragment) = match rest.split_once('#') {
            Some((a, f)) => (a, Some(f)),
            None => (rest, None),
        };

        let parts: Vec<&str> = authority.split(':').collect();
        if parts.len() != 3 {
            return Err(GlobalIdError::MalformedAuthority(parts.len()));
        }
        let provider = canonical_provider(parts[0])?;
        let org = validate_segment(parts[1], "org")?;
        let repo = validate_segment(parts[2], "repo")?;

        let (path, symbol_path) = match fragment {
            None => (None, None),
            Some(f) => {
                if f.is_empty() {
                    return Err(GlobalIdError::EmptyFragment);
                }
                let (p, s) = split_fragment(f)?;
                (Some(p), s)
            }
        };

        Ok(Self {
            provider,
            org,
            repo,
            path,
            symbol_path,
        })
    }

    /// Builds an identifier without a path or symbol component.
    pub fn repository(provider: &str, org: &str, repo: &str) -> Result<Self, GlobalIdError> {
        Ok(Self {
            provider: canonical_provider(provider)?,
            org: validate_segment(org, "org")?,
            repo: validate_segment(repo, "repo")?,
            path: None,
            symbol_path: None,
        })
    }

    /// Attaches a POSIX-normalized file path (and optional symbol path).
    pub fn with_path(
        mut self,
        path: &str,
        symbol_path: Option<&str>,
    ) -> Result<Self, GlobalIdError> {
        let normalized = normalize_path(path)?;
        if let Some(sym) = symbol_path {
            validate_symbol_path(sym)?;
            self.symbol_path = Some(sym.to_string());
        }
        self.path = Some(normalized);
        Ok(self)
    }

    /// Canonical provider host, lowercased.
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Organization / owner segment.
    pub fn org(&self) -> &str {
        &self.org
    }

    /// Repository segment.
    pub fn repo(&self) -> &str {
        &self.repo
    }

    /// POSIX-normalized file path, when present.
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    /// Symbol path within the file, when present.
    pub fn symbol_path(&self) -> Option<&str> {
        self.symbol_path.as_deref()
    }

    /// Repository-level prefix (`urn:autopsy:repo:p:o:r`), ignoring path/symbol.
    ///
    /// Used by federated impact to group identifiers by owning repository.
    pub fn repository_urn(&self) -> String {
        format!("{URN_PREFIX}{}:{}:{}", self.provider, self.org, self.repo)
    }

    /// Canonical string form. `Display` delegates here.
    pub fn canonical(&self) -> String {
        let mut out = self.repository_urn();
        if let Some(path) = &self.path {
            out.push('#');
            out.push_str(path);
            if let Some(sym) = &self.symbol_path {
                out.push(':');
                out.push_str(sym);
            }
        }
        out
    }

    /// Derives a local `SymbolId` string for use in a single-repo graph.
    ///
    /// Returns `None` when the identifier carries no path: a repository-level
    /// URN does not denote a symbol.
    pub fn to_local_symbol_id(&self) -> Option<String> {
        let path = self.path.as_ref()?;
        Some(match &self.symbol_path {
            Some(sym) => format!("{path}:{sym}"),
            None => path.clone(),
        })
    }
}

impl fmt::Display for GlobalEntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical())
    }
}

impl std::str::FromStr for GlobalEntityId {
    type Err = GlobalIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<String> for GlobalEntityId {
    type Error = GlobalIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<GlobalEntityId> for String {
    fn from(value: GlobalEntityId) -> Self {
        value.canonical()
    }
}

/// Lowercases a provider host and validates its character class.
fn canonical_provider(raw: &str) -> Result<String, GlobalIdError> {
    if raw.is_empty() || !raw.bytes().all(is_provider_byte) {
        return Err(GlobalIdError::InvalidSegment {
            field: "provider",
            allowed: "A-Za-z0-9.-",
        });
    }
    Ok(raw.to_ascii_lowercase())
}

/// Validates an `org` / `repo` authority segment.
fn validate_segment(raw: &str, field: &'static str) -> Result<String, GlobalIdError> {
    if raw.is_empty() || !raw.bytes().all(is_segment_byte) {
        return Err(GlobalIdError::InvalidSegment {
            field,
            allowed: SEGMENT_ALLOWED,
        });
    }
    Ok(raw.to_string())
}

fn is_provider_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'.' || b == b'-'
}

fn is_segment_byte(b: u8) -> bool {
    is_provider_byte(b) || b == b'_'
}

fn is_symbol_path_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'#' | b'_' | b'$')
}

/// Splits the `#` fragment into `(path, Option<symbol-path>)`.
///
/// Scans from the right for the first `:` whose right-hand side is a valid
/// symbol-path and contains no `/`. This keeps paths that legitimately contain
/// `:` (e.g. `src/a:b/c.ts`) intact.
fn split_fragment(fragment: &str) -> Result<(String, Option<String>), GlobalIdError> {
    let mut search_end = fragment.len();
    while let Some(idx) = fragment[..search_end].rfind(':') {
        let candidate = &fragment[idx + 1..];
        if !candidate.is_empty()
            && !candidate.contains('/')
            && candidate.bytes().all(is_symbol_path_byte)
        {
            let path = normalize_path(&fragment[..idx])?;
            return Ok((path, Some(candidate.to_string())));
        }
        search_end = idx;
    }
    let path = normalize_path(fragment)?;
    Ok((path, None))
}

/// Validates a symbol path against its restricted character class.
fn validate_symbol_path(symbol: &str) -> Result<(), GlobalIdError> {
    if symbol.is_empty() || !symbol.bytes().all(is_symbol_path_byte) {
        return Err(GlobalIdError::InvalidSymbolPath);
    }
    Ok(())
}

/// POSIX-normalizes a path and rejects traversal.
///
/// Collapses redundant `/`, strips leading `./` and trailing `/`, and rejects
/// any `..` segment. Rejects backslash and non-printable-ASCII bytes outright
/// so that two spellings can never yield one identity.
pub fn normalize_path(raw: &str) -> Result<String, GlobalIdError> {
    let mut segments = Vec::new();
    for segment in raw.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return Err(GlobalIdError::PathTraversal);
        }
        for &b in segment.as_bytes() {
            // Backslash is rejected rather than rewritten: accepting both
            // spellings would let one entity hold two identities.
            if b == b'\\' || !(0x21..=0x7e).contains(&b) {
                return Err(GlobalIdError::ForbiddenCharacter(b));
            }
        }
        segments.push(segment);
    }

    if segments.is_empty() {
        return Err(GlobalIdError::ForbiddenCharacter(b'/'));
    }

    let mut joined = segments.join("/");
    if joined.is_empty() {
        joined = ".".to_string();
    }
    Ok(joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str =
        "urn:autopsy:repo:github.com:synevid:billing-service#src/api/invoice.ts:createInvoice";

    #[test]
    fn test_parse_spec_example() {
        let id = GlobalEntityId::parse(EXAMPLE).expect("spec example must parse");
        assert_eq!(id.provider(), "github.com");
        assert_eq!(id.org(), "synevid");
        assert_eq!(id.repo(), "billing-service");
        assert_eq!(id.path(), Some("src/api/invoice.ts"));
        assert_eq!(id.symbol_path(), Some("createInvoice"));
    }

    #[test]
    fn test_roundtrip_is_total() {
        let id = GlobalEntityId::parse(EXAMPLE).unwrap();
        let reparsed = GlobalEntityId::parse(&id.canonical()).unwrap();
        assert_eq!(id, reparsed);
        assert_eq!(id.canonical(), EXAMPLE);
    }

    #[test]
    fn test_repository_only_identifiers() {
        let id = GlobalEntityId::parse("urn:autopsy:repo:github.com:synevid:api").unwrap();
        assert_eq!(id.path(), None);
        assert_eq!(id.symbol_path(), None);
        assert_eq!(id.to_string(), "urn:autopsy:repo:github.com:synevid:api");
        assert_eq!(id.to_local_symbol_id(), None);
    }

    #[test]
    fn test_provider_lowercased() {
        let a = GlobalEntityId::parse("urn:autopsy:repo:GitHub.COM:acme:x").unwrap();
        let b = GlobalEntityId::parse("urn:autopsy:repo:github.com:acme:x").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.provider(), "github.com");
    }

    #[test]
    fn test_org_and_repo_case_preserved() {
        // Only the provider is canonicalized; owner and repo are case-sensitive.
        let a = GlobalEntityId::parse("urn:autopsy:repo:github.com:Acme:Repo").unwrap();
        assert_eq!(a.org(), "Acme");
        assert_eq!(a.repo(), "Repo");
    }

    #[test]
    fn test_path_normalization_strips_leading_dot_slash_and_trailing_slash() {
        let id = GlobalEntityId::parse("urn:autopsy:repo:github.com:o:r#./src///a.ts/").unwrap();
        assert_eq!(id.path(), Some("src/a.ts"));
    }

    #[test]
    fn test_rejects_path_traversal() {
        let err = GlobalEntityId::parse("urn:autopsy:repo:github.com:o:r#src/../../etc/passwd")
            .unwrap_err();
        assert_eq!(err, GlobalIdError::PathTraversal);
    }

    #[test]
    fn test_rejects_leading_parent_traversal() {
        assert!(GlobalEntityId::parse("urn:autopsy:repo:github.com:o:r#../secret").is_err());
    }

    #[test]
    fn test_rejects_missing_scheme() {
        assert_eq!(
            GlobalEntityId::parse("github.com:o:r").unwrap_err(),
            GlobalIdError::MissingScheme
        );
    }

    #[test]
    fn test_rejects_malformed_authority() {
        assert_eq!(
            GlobalEntityId::parse("urn:autopsy:repo:github.com:o").unwrap_err(),
            GlobalIdError::MalformedAuthority(2)
        );
        assert_eq!(
            GlobalEntityId::parse("urn:autopsy:repo:a:b:c:d").unwrap_err(),
            GlobalIdError::MalformedAuthority(4)
        );
    }

    #[test]
    fn test_rejects_empty_authority_segments() {
        assert!(GlobalEntityId::parse("urn:autopsy:repo::o:r").is_err());
        assert!(GlobalEntityId::parse("urn:autopsy:repo:github.com::r").is_err());
        assert!(GlobalEntityId::parse("urn:autopsy:repo:github.com:o:").is_err());
    }

    #[test]
    fn test_rejects_forbidden_authority_characters() {
        // Slash is not a legal provider byte.
        assert!(GlobalEntityId::parse("urn:autopsy:repo:github/com:o:r").is_err());
        // Space is not a legal segment byte.
        assert!(GlobalEntityId::parse("urn:autopsy:repo:github.com:o r:x").is_err());
    }

    #[test]
    fn test_rejects_underscore_in_provider() {
        assert!(GlobalEntityId::parse("urn:autopsy:repo:github_com:o:r").is_err());
    }

    #[test]
    fn test_rejects_backslash_path() {
        // Anti-confusion: backslash must be normalized by the caller, never
        // silently accepted here.
        assert!(GlobalEntityId::parse("urn:autopsy:repo:github.com:o:r#src\\a.ts").is_err());
    }

    #[test]
    fn test_rejects_space_and_control_bytes_in_path() {
        assert!(GlobalEntityId::parse("urn:autopsy:repo:github.com:o:r#src/a b.ts").is_err());
        assert!(GlobalEntityId::parse("urn:autopsy:repo:github.com:o:r#src/a\tb.ts").is_err());
        assert!(GlobalEntityId::parse("urn:autopsy:repo:github.com:o:r#src/a\x7fb.ts").is_err());
    }

    #[test]
    fn test_rejects_empty_fragment() {
        assert_eq!(
            GlobalEntityId::parse("urn:autopsy:repo:github.com:o:r#").unwrap_err(),
            GlobalIdError::EmptyFragment
        );
    }

    #[test]
    fn test_rejects_oversize_input() {
        let big = format!(
            "urn:autopsy:repo:github.com:o:r#{}",
            "a".repeat(MAX_INPUT_LEN)
        );
        assert_eq!(
            GlobalEntityId::parse(&big).unwrap_err(),
            GlobalIdError::TooLong
        );
    }

    #[test]
    fn test_path_containing_colon_without_symbol() {
        // Right-hand side of the last `:` contains `/`, so it cannot be a
        // symbol path; the whole fragment stays a path.
        let id = GlobalEntityId::parse("urn:autopsy:repo:github.com:o:r#src/a:b/c.ts").unwrap();
        assert_eq!(id.path(), Some("src/a:b/c.ts"));
        assert_eq!(id.symbol_path(), None);
    }

    #[test]
    fn test_path_with_colon_and_symbol() {
        let id = GlobalEntityId::parse("urn:autopsy:repo:github.com:o:r#src/a:b/c.ts:run").unwrap();
        assert_eq!(id.path(), Some("src/a:b/c.ts"));
        assert_eq!(id.symbol_path(), Some("run"));
    }

    #[test]
    fn test_symbol_path_allows_dollar_and_hash() {
        let id =
            GlobalEntityId::parse("urn:autopsy:repo:github.com:o:r#s.ts:Ns$inner#tag").unwrap();
        assert_eq!(id.symbol_path(), Some("Ns$inner#tag"));
    }

    #[test]
    fn test_rejects_symbol_path_with_slash() {
        // `/` is not in the symbol-path class, so it degrades to a path.
        let id = GlobalEntityId::parse("urn:autopsy:repo:github.com:o:r#a:b/c").unwrap();
        assert_eq!(id.path(), Some("a:b/c"));
        assert_eq!(id.symbol_path(), None);
    }

    #[test]
    fn test_repository_urn_groups_federated_identities() {
        let a = GlobalEntityId::parse("urn:autopsy:repo:github.com:o:r#src/a.ts:one").unwrap();
        let b = GlobalEntityId::parse("urn:autopsy:repo:github.com:o:r#src/b.ts:two").unwrap();
        assert_eq!(a.repository_urn(), b.repository_urn());
        assert_eq!(a.repository_urn(), "urn:autopsy:repo:github.com:o:r");
    }

    #[test]
    fn test_to_local_symbol_id() {
        let with_symbol =
            GlobalEntityId::parse("urn:autopsy:repo:github.com:o:r#src/a.ts:fn").unwrap();
        assert_eq!(
            with_symbol.to_local_symbol_id().as_deref(),
            Some("src/a.ts:fn")
        );

        let file_only = GlobalEntityId::parse("urn:autopsy:repo:github.com:o:r#src/a.ts").unwrap();
        assert_eq!(file_only.to_local_symbol_id().as_deref(), Some("src/a.ts"));
    }

    #[test]
    fn test_builder_matches_parser() {
        let built = GlobalEntityId::repository("github.com", "synevid", "billing-service")
            .unwrap()
            .with_path("src/api/invoice.ts", Some("createInvoice"))
            .unwrap();
        let parsed = GlobalEntityId::parse(EXAMPLE).unwrap();
        assert_eq!(built, parsed);
        assert_eq!(built.canonical(), EXAMPLE);
    }

    #[test]
    fn test_with_path_rejects_traversal() {
        let base = GlobalEntityId::repository("github.com", "o", "r").unwrap();
        assert_eq!(
            base.with_path("../escape", None).unwrap_err(),
            GlobalIdError::PathTraversal
        );
    }

    #[test]
    fn test_serde_roundtrip_is_canonical() {
        let id = GlobalEntityId::parse("urn:autopsy:repo:GitHub.com:o:r#./src/a.ts:fn").unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"urn:autopsy:repo:github.com:o:r#src/a.ts:fn\"");
        let back: GlobalEntityId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn test_serde_rejects_invalid_on_deserialize() {
        let bad = "\"not-a-urn\"";
        assert!(serde_json::from_str::<GlobalEntityId>(bad).is_err());
    }

    #[test]
    fn test_btreeset_dedupes_equivalent_spellings() {
        use std::collections::BTreeSet;
        let mut set = BTreeSet::new();
        set.insert(GlobalEntityId::parse("urn:autopsy:repo:GitHub.com:o:r#src/a.ts").unwrap());
        set.insert(GlobalEntityId::parse("urn:autopsy:repo:github.com:o:r#./src/a.ts/").unwrap());
        assert_eq!(
            set.len(),
            1,
            "equivalent spellings must collapse to one identity"
        );
    }

    #[test]
    fn test_from_str_trait() {
        use std::str::FromStr;
        let id = GlobalEntityId::from_str(EXAMPLE).unwrap();
        assert_eq!(id.repo(), "billing-service");
    }

    #[test]
    fn test_ordering_is_canonical_and_stable() {
        let mut ids = [
            GlobalEntityId::parse("urn:autopsy:repo:github.com:o:r#b.ts").unwrap(),
            GlobalEntityId::parse("urn:autopsy:repo:github.com:o:r#a.ts").unwrap(),
        ];
        ids.sort();
        assert_eq!(ids[0].path(), Some("a.ts"));
        assert_eq!(ids[1].path(), Some("b.ts"));
    }
}
