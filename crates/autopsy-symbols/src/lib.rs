//! autopsy-symbols: stable symbol identity authority (FR-005).
//!
//! # Identity / shape split (forensic correctness)
//!
//! A symbol's **identity** (`SymbolId`) is derived ONLY from stable,
//! edit-resilient coordinates: canonical file path + symbol kind +
//! lexical scope + declared name. It MUST NOT embed the normalized
//! signature hash.
//!
//! Rationale: if the identity contained the signature, every parameter
//! or return-type edit would look like a delete+create pair, and the
//! `SignatureModified` / breaking-contract paths in `autopsy-diff` and
//! `autopsy-invariants` could never fire on the primary forensic event
//! (a signature change). Shape lives in `Symbol.normalized_signature`
//! and in `Contract.inputs/output`, where diff and compatibility
//! engines compare it explicitly.
//!
//! Identity format: `{file}#fn:{name}` for functions,
//! `{file}#method:{parent}.{name}` for methods,
//! `{file}#{kind}:{name}` for class/interface/type/enum/variable.
//! Overload collisions (same name+kind+scope, e.g. TS overloads) receive
//! a deterministic `:overload:N` suffix (N >= 2, ordered by
//! normalized signature bytes).
//!
//! Hard boundaries: no line numbers in identities, no `HashMap`
//! iteration, deterministic on all platforms.

use autopsy_domain::SymbolId;

/// Symbol kinds with their canonical identity tags.
///
/// Tags preserve the historical `fn` / `method` spellings so existing
/// qualified tooling keeps working; identity strings contain no hashes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Interface,
    Type,
    Enum,
    Variable,
}

impl SymbolKind {
    /// Canonical identity tag used inside `SymbolId` strings.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Function => "fn",
            Self::Method => "method",
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Type => "type",
            Self::Enum => "enum",
            Self::Variable => "variable",
        }
    }

    /// Human `Symbol.kind` string stored on the symbol record.
    pub fn kind_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Type => "type",
            Self::Enum => "enum",
            Self::Variable => "variable",
        }
    }

    /// Parses a canonical identity tag back into a kind.
    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "fn" | "function" => Some(Self::Function),
            "method" => Some(Self::Method),
            "class" => Some(Self::Class),
            "interface" => Some(Self::Interface),
            "type" => Some(Self::Type),
            "enum" => Some(Self::Enum),
            "variable" => Some(Self::Variable),
            _ => None,
        }
    }
}

/// Builds the canonical qualified name `file::[parent::]name`.
pub fn qualified_name(file_path: &str, parent_scope: &str, name: &str) -> String {
    if parent_scope.is_empty() {
        format!("{}::{}", file_path, name)
    } else {
        format!("{}::{}::{}", file_path, parent_scope, name)
    }
}

/// Extracts the short (declared) name from a qualified name.
pub fn short_name(qualified: &str) -> &str {
    qualified.rsplit("::").next().unwrap_or(qualified)
}

/// Computes the stable identity for a symbol.
///
/// `parent_scope` is required for methods (`ClassName`) and empty
/// otherwise. The returned id contains no signature material.
pub fn stable_symbol_id(
    file_path: &str,
    kind: SymbolKind,
    parent_scope: &str,
    name: &str,
) -> SymbolId {
    match kind {
        SymbolKind::Method => {
            SymbolId::new(format!("{}#method:{}.{}", file_path, parent_scope, name))
        }
        _ => SymbolId::new(format!("{}#{}:{}", file_path, kind.tag(), name)),
    }
}

/// Applies the deterministic overload suffix to a base identity.
///
/// First overload keeps the bare id; subsequent overloads (ordered by
/// normalized-signature bytes by the caller) become `id:overload:N`.
pub fn overload_id(base: &SymbolId, index: usize) -> SymbolId {
    if index == 0 {
        base.clone()
    } else {
        SymbolId::new(format!("{}:overload:{}", base.as_str(), index + 1))
    }
}

/// BLAKE3 hex digest utility for signature material (evidence + digest
/// paths). Identity construction never calls this; it exists so hash
/// usage stays centralized and deterministic.
pub fn signature_hash(normalized_signature: &str) -> String {
    blake3::hash(normalized_signature.as_bytes())
        .to_hex()
        .to_string()
}

/// Short (12-hex) signature fingerprint, matching historical evidence
/// display width.
pub fn signature_fingerprint(normalized_signature: &str) -> String {
    signature_hash(normalized_signature)[..12].to_string()
}

/// Validates that an identity string follows the canonical shape
/// `{file}#tag:{rest}` with a known tag.
pub fn is_canonical_identity(id: &SymbolId) -> bool {
    let s = id.as_str();
    let Some(hash_pos) = s.find('#') else {
        return false;
    };
    let Some(colon_rel) = s[hash_pos + 1..].find(':') else {
        return false;
    };
    let tag = &s[hash_pos + 1..hash_pos + 1 + colon_rel];
    if SymbolKind::from_tag(tag).is_none() {
        return false;
    }
    hash_pos > 0 && !s[hash_pos + 1 + colon_rel + 1..].is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_has_no_signature_material() {
        let before = stable_symbol_id("src/a.ts", SymbolKind::Function, "", "add");
        // A signature edit must NOT change identity: shape is compared,
        // not embedded.
        let after = stable_symbol_id("src/a.ts", SymbolKind::Function, "", "add");
        assert_eq!(before, after);
        assert_eq!(before.as_str(), "src/a.ts#fn:add");
        assert!(!before.as_str().contains("number"));
    }

    #[test]
    fn test_method_identity_includes_parent_scope() {
        let id = stable_symbol_id("src/s.ts", SymbolKind::Method, "Service", "create");
        assert_eq!(id.as_str(), "src/s.ts#method:Service.create");
    }

    #[test]
    fn test_kind_tags_roundtrip() {
        for kind in [
            SymbolKind::Function,
            SymbolKind::Method,
            SymbolKind::Class,
            SymbolKind::Interface,
            SymbolKind::Type,
            SymbolKind::Enum,
            SymbolKind::Variable,
        ] {
            assert_eq!(SymbolKind::from_tag(kind.tag()), Some(kind));
        }
        assert_eq!(SymbolKind::from_tag("fn"), Some(SymbolKind::Function));
        assert_eq!(SymbolKind::from_tag("bogus"), None);
    }

    #[test]
    fn test_qualified_and_short_names() {
        assert_eq!(qualified_name("f.ts", "", "add"), "f.ts::add");
        assert_eq!(qualified_name("f.ts", "Svc", "go"), "f.ts::Svc::go");
        assert_eq!(short_name("f.ts::Svc::go"), "go");
        assert_eq!(short_name("plain"), "plain");
    }

    #[test]
    fn test_overload_suffix_deterministic() {
        let base = stable_symbol_id("f.ts", SymbolKind::Function, "", "get");
        assert_eq!(overload_id(&base, 0), base);
        assert_eq!(overload_id(&base, 1).as_str(), "f.ts#fn:get:overload:2");
        assert_eq!(overload_id(&base, 2).as_str(), "f.ts#fn:get:overload:3");
    }

    #[test]
    fn test_canonical_identity_validation() {
        assert!(is_canonical_identity(&SymbolId::new("src/a.ts#fn:add")));
        assert!(is_canonical_identity(&SymbolId::new(
            "src/s.ts#method:Service.create"
        )));
        assert!(is_canonical_identity(&SymbolId::new(
            "src/a.ts#fn:get:overload:2"
        )));
        assert!(!is_canonical_identity(&SymbolId::new("no-hash-here")));
        assert!(!is_canonical_identity(&SymbolId::new("f.ts#bogus:x")));
        assert!(!is_canonical_identity(&SymbolId::new("f.ts#fn:")));
    }

    #[test]
    fn test_signature_hash_deterministic_100_runs() {
        let first = signature_hash("add(a: number, b: number): number");
        assert_eq!(first.len(), 64);
        for _ in 0..100 {
            assert_eq!(signature_hash("add(a: number, b: number): number"), first);
        }
        assert_eq!(signature_fingerprint("x").len(), 12);
    }
}
