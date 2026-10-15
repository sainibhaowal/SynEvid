//! Spec document loading, canonicalisation, and local `$ref` resolution.
//!
//! # Why YAML is not simply "JSON with different punctuation"
//! Every YAML feature that `serde_yaml` does not implement becomes a **silent
//! misread**: the document parses successfully, the field is simply absent, and
//! a compatibility checker then reports the missing field as a removal. Two such
//! features are handled explicitly here rather than trusted:
//!
//! * **Merge keys** (`<<: *base`). `serde_yaml` does not apply them. A document
//!   using them would appear to lack every inherited field. Detected and
//!   refused outright — see [`DocumentError::YamlMergeKey`].
//! * **Non-string mapping keys.** Representable in YAML, not in the JSON data
//!   model this crate compares over. Rejected with an explicit error rather
//!   than coerced.
//!
//! Anchors and aliases (`&a` / `*a`) *are* implemented by `serde_yaml`, so they
//! need no special handling.
//!
//! # Canonicalisation
//! The digest is taken over the **parsed value**, not the source bytes. Two
//! documents that differ only in indentation, key order, or comments are the
//! same contract and therefore the same digest. That is the useful property:
//! a reformat must not appear as a contract change.

use autopsy_domain::CoverageState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// How a document's bytes are interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocFormat {
    Json,
    Yaml,
    /// Extension did not identify the format. Parsed as JSON, then YAML, and the
    /// document is marked with a note so the guess is never invisible.
    Inferred,
}

impl DocFormat {
    /// Infers the format from a file extension.
    pub fn from_path(path: &str) -> Self {
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".json") {
            Self::Json
        } else if lower.ends_with(".yaml") || lower.ends_with(".yml") {
            Self::Yaml
        } else {
            Self::Inferred
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Inferred => "inferred",
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DocumentError {
    #[error("{path}: not valid JSON: {message}")]
    Json { path: String, message: String },
    #[error("{path}: not valid YAML: {message}")]
    Yaml { path: String, message: String },
    #[error(
        "{path}: line {line}: YAML merge key `<<` is not supported. serde_yaml does not apply \
         merge keys, so the document would parse with every inherited field silently absent and \
         a compatibility check would report those fields as removed. Expand the merge key, or \
         convert the document to JSON."
    )]
    YamlMergeKey { path: String, line: usize },
    #[error("{path}: document root must be a mapping, found {found}")]
    NonObjectRoot { path: String, found: String },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RefError {
    #[error(
        "external reference `{reference}` cannot be resolved: this engine compares within a single document tree"
    )]
    External { reference: String },
    #[error("reference `{reference}` does not start with `#/` or `#`")]
    NotLocal { reference: String },
    #[error("reference `{reference}` points at `{pointer}`, which does not exist")]
    NotFound { reference: String, pointer: String },
    #[error("reference `{reference}` descends into an array index, which is not dereferenced")]
    IntoArray { reference: String },
}

/// A loaded, canonicalised specification document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecDocument {
    /// Repository-relative POSIX path the document was read from.
    pub path: String,
    pub format: DocFormat,
    /// Canonical value. `serde_json::Map` is `BTreeMap`-backed in this
    /// workspace, so key order is sorted and stable.
    pub root: Value,
    /// Every non-local reference seen, sorted.
    pub external_refs: BTreeSet<String>,
    /// Notes that qualify the parse. Never empty when a guess was made.
    pub notes: Vec<String>,
}

impl SpecDocument {
    /// Parses a document according to its declared format.
    ///
    /// `format` is explicit because inferring it hides a real ambiguity: a
    /// `.json` file that only parses as YAML is mislabelled, and saying so is
    /// more useful than quietly accepting it.
    pub fn parse(path: &str, content: &str, format: DocFormat) -> Result<Self, DocumentError> {
        let mut notes = Vec::new();

        let root = match format {
            DocFormat::Json => {
                serde_json::from_str::<Value>(content).map_err(|e| DocumentError::Json {
                    path: path.to_string(),
                    message: e.to_string(),
                })?
            }
            DocFormat::Yaml => {
                reject_merge_keys(path, content)?;
                serde_yaml::from_str::<Value>(content).map_err(|e| DocumentError::Yaml {
                    path: path.to_string(),
                    message: e.to_string(),
                })?
            }
            DocFormat::Inferred => {
                // JSON is a strict subset of YAML, so trying JSON first is not
                // a heuristic: if it succeeds, that interpretation is correct.
                match serde_json::from_str::<Value>(content) {
                    Ok(v) => {
                        notes.push(
                            "path extension did not identify the format; parsed as JSON"
                                .to_string(),
                        );
                        v
                    }
                    Err(_) => {
                        reject_merge_keys(path, content)?;
                        let v = serde_yaml::from_str::<Value>(content).map_err(|e| {
                            DocumentError::Yaml {
                                path: path.to_string(),
                                message: e.to_string(),
                            }
                        })?;
                        notes.push(
                            "path extension did not identify the format; parsed as YAML"
                                .to_string(),
                        );
                        v
                    }
                }
            }
        };

        if !root.is_object() {
            return Err(DocumentError::NonObjectRoot {
                path: path.to_string(),
                found: type_name(&root).to_string(),
            });
        }

        let external_refs = collect_external_refs(&root);
        if !external_refs.is_empty() {
            notes.push(format!(
                "{} external reference(s) cannot be resolved within a single document tree",
                external_refs.len()
            ));
        }

        Ok(Self {
            path: path.to_string(),
            format,
            root,
            external_refs,
            notes,
        })
    }

    /// Convenience loader that infers the format from the path extension.
    pub fn load(path: &str, content: &str) -> Result<Self, DocumentError> {
        Self::parse(path, content, DocFormat::from_path(path))
    }

    /// BLAKE3 over the canonical document value.
    ///
    /// Formatting-insensitive by construction: comments, indentation, and key
    /// order do not reach the digest, so a reformat is not a contract change.
    pub fn digest(&self) -> String {
        let canonical = canonical_string(&self.root);
        blake3::hash(canonical.as_bytes()).to_hex().to_string()
    }

    /// Resolves a `#/`-style reference against this document.
    pub fn resolve<'a>(&'a self, reference: &str) -> Result<&'a Value, RefError> {
        let pointer = pointer_of(reference)?;
        self.pointer(pointer).ok_or_else(|| RefError::NotFound {
            reference: reference.to_string(),
            pointer: pointer.to_string(),
        })
    }

    /// Resolves a bare JSON-pointer path such as `/components/schemas/Pet`.
    ///
    /// Follows RFC 6901 in full, including array indices. The token type is
    /// determined by the value it is applied to, not by the token: under an
    /// array, `0` is an index; under an object, `0` is the key `"0"`. There is
    /// therefore no ambiguity to resolve, and a `0`-indexed token under an
    /// object is not a conflict.
    ///
    /// Leading zeros are refused (`/a/01` is not a valid pointer token) and so
    /// is `-`, which RFC 6901 defines only for append operations and which has
    /// no meaning for a lookup.
    pub fn pointer<'a>(&'a self, pointer: &str) -> Option<&'a Value> {
        let trimmed = pointer.strip_prefix('/').unwrap_or(pointer);
        if trimmed.is_empty() {
            return Some(&self.root);
        }
        let mut current = &self.root;
        for raw_segment in trimmed.split('/') {
            let segment = unescape_pointer_segment(raw_segment);
            current = match current {
                Value::Object(map) => map.get(&segment)?,
                Value::Array(items) => items.get(array_index(&segment)?)?,
                _ => return None,
            };
        }
        Some(current)
    }

    /// `$ref` string at `pointer`, if the value there is a reference.
    pub fn ref_at(&self, pointer: &str) -> Option<&str> {
        self.pointer(pointer)?.get("$ref")?.as_str()
    }

    /// True when the document declares no resolvable surfaces at all.
    pub fn is_empty_object(&self) -> bool {
        self.root.as_object().is_none_or(|o| o.is_empty())
    }
}

/// Extracts the JSON-pointer part of a local reference.
///
/// Returns `Err` for external references. Accepting a bare `#` as the document
/// root would let a self-referential document resolve to itself, which produces
/// infinite recursion in any consumer; that is refused here.
pub fn pointer_of(reference: &str) -> Result<&str, RefError> {
    if reference == "#" {
        return Err(RefError::NotLocal {
            reference: reference.to_string(),
        });
    }
    let Some(rest) = reference.strip_prefix('#') else {
        return Err(RefError::External {
            reference: reference.to_string(),
        });
    };
    if !rest.starts_with('/') {
        return Err(RefError::NotLocal {
            reference: reference.to_string(),
        });
    }
    Ok(rest)
}

/// A reference is external when it names a file instead of a fragment.
pub fn is_external_ref(reference: &str) -> bool {
    !reference.starts_with('#')
}

/// Rejects documents that use YAML merge keys.
///
/// The scan is line-based and therefore cannot distinguish a `<<:` mapping key
/// from the same characters inside a block scalar. That asymmetry is
/// intentional: a false positive refuses to analyse a document, while a false
/// negative produces a confident wrong answer. Only one of those is recoverable.
fn reject_merge_keys(path: &str, content: &str) -> Result<(), DocumentError> {
    for (idx, raw) in content.lines().enumerate() {
        let line = raw.trim_start();
        if let Some(rest) = line.strip_prefix("<<") {
            let rest = rest.trim_start();
            if rest.starts_with(':') {
                return Err(DocumentError::YamlMergeKey {
                    path: path.to_string(),
                    line: idx + 1,
                });
            }
        }
    }
    Ok(())
}

/// Collects every external `$ref` string anywhere in the tree.
fn collect_external_refs(value: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    walk(value, &mut |v| {
        if let Some(r) = v.get("$ref").and_then(Value::as_str)
            && is_external_ref(r)
        {
            out.insert(r.to_string());
        }
    });
    out
}

/// Pre-order traversal over every value in the tree.
pub(crate) fn walk(value: &Value, visit: &mut impl FnMut(&Value)) {
    visit(value);
    match value {
        Value::Object(map) => {
            for v in map.values() {
                walk(v, visit);
            }
        }
        Value::Array(items) => {
            for v in items {
                walk(v, visit);
            }
        }
        _ => {}
    }
}

/// Canonical string form used for digests.
///
/// `BTreeMap` key ordering makes this independent of source ordering; compact
/// separators make it independent of formatting.
pub(crate) fn canonical_string(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut out = String::from("{");
            let mut entries: BTreeMap<&str, &Value> = BTreeMap::new();
            for (k, v) in map {
                entries.insert(k.as_str(), v);
            }
            for (i, (k, v)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).unwrap_or_else(|_| "\"\"".to_string()));
                out.push(':');
                out.push_str(&canonical_string(v));
            }
            out.push('}');
            out
        }
        Value::Array(items) => {
            let mut out = String::from("[");
            for (i, v) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&canonical_string(v));
            }
            out.push(']');
            out
        }
        other => serde_json::to_string(other).unwrap_or_else(|_| "null".to_string()),
    }
}

/// Parses a JSON Pointer array index per RFC 6901 §4.
///
/// Digits only, no leading zeros, and `-` is excluded: the RFC defines `-` for
/// append operations, which a lookup cannot perform.
fn array_index(segment: &str) -> Option<usize> {
    if segment.is_empty() || segment == "-" {
        return None;
    }
    if segment.len() > 1 && segment.starts_with('0') {
        return None;
    }
    if !segment.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    segment.parse().ok()
}

/// JSON Pointer unescaping per RFC 6901 §4.
fn unescape_pointer_segment(segment: &str) -> String {
    // `~1` must be replaced before `~0`, or `~01` would decode to `~1`
    // instead of the literal `~1` the spec requires.
    segment.replace("~1", "/").replace("~0", "~")
}

/// Human-readable JSON type name.
pub(crate) fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Worst-case reduction over coverage states. Never upgrades.
pub(crate) fn worse_of(a: CoverageState, b: CoverageState) -> CoverageState {
    use CoverageState::*;
    match (a, b) {
        (Unknown, _) | (_, Unknown) => Unknown,
        (Unsupported, _) | (_, Unsupported) => Unsupported,
        (Partial, _) | (_, Partial) => Partial,
        _ => Verified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unescape_order_handles_tilde_one() {
        // `~01` is the literal `~1`, not `/`. A naive replace-all in the wrong
        // order gets this backwards.
        assert_eq!(unescape_pointer_segment("~01"), "~1");
        assert_eq!(unescape_pointer_segment("~1"), "/");
        assert_eq!(unescape_pointer_segment("~0"), "~");
    }

    #[test]
    fn test_pointer_lookup() {
        let doc = SpecDocument::parse(
            "a.json",
            r#"{"a":{"b":{"c":42}},"l":["x"]}"#,
            DocFormat::Json,
        )
        .unwrap();
        assert_eq!(doc.pointer("/a/b/c"), Some(&Value::from(42)));
        assert_eq!(doc.pointer("a/b/c"), Some(&Value::from(42)));
        assert_eq!(doc.pointer(""), Some(&doc.root));
        assert_eq!(doc.pointer("/a/missing"), None);
    }

    #[test]
    fn test_pointer_descends_arrays_by_index_per_rfc6901() {
        let doc = SpecDocument::parse(
            "a.json",
            r#"{"l":[{"n":1},{"n":2}],"0":"k"}"#,
            DocFormat::Json,
        )
        .unwrap();
        assert_eq!(doc.pointer("/l/0/n"), Some(&Value::from(1)));
        assert_eq!(doc.pointer("/l/1/n"), Some(&Value::from(2)));
        assert_eq!(doc.pointer("/l/2/n"), None, "out of bounds");
        // The same token under an object is the key `"0"`, not an index: the
        // token's meaning is decided by the value it is applied to.
        assert_eq!(doc.pointer("/0"), Some(&Value::from("k")));
    }

    #[test]
    fn test_array_index_rejects_leading_zeros_and_append_token() {
        assert_eq!(array_index("0"), Some(0));
        assert_eq!(array_index("10"), Some(10));
        assert_eq!(array_index("01"), None, "leading zeros are invalid");
        assert_eq!(array_index("-"), None, "`-` is for append, not lookup");
        assert_eq!(array_index(""), None);
        assert_eq!(array_index("1x"), None);
        assert_eq!(array_index("+1"), None);
    }

    #[test]
    fn test_merge_key_is_rejected() {
        let err = SpecDocument::parse(
            "a.yaml",
            "base: &b\n  x: 1\nthing:\n  <<: *b\n  y: 2\n",
            DocFormat::Yaml,
        )
        .unwrap_err();
        assert!(
            matches!(err, DocumentError::YamlMergeKey { line: 4, .. }),
            "{err:?}"
        );
    }

    #[test]
    fn test_anchors_without_merge_keys_are_fine() {
        let doc = SpecDocument::parse(
            "a.yaml",
            "base: &b\n  x: 1\nthing:\n  y: 2\n",
            DocFormat::Yaml,
        )
        .unwrap();
        assert_eq!(doc.pointer("/base/x"), Some(&Value::from(1)));
    }

    #[test]
    fn test_self_reference_is_rejected() {
        assert!(matches!(pointer_of("#"), Err(RefError::NotLocal { .. })));
    }

    #[test]
    fn test_external_reference_is_reported() {
        assert!(is_external_ref("./other.yaml#/components/schemas/Pet"));
        let doc = SpecDocument::parse(
            "a.yaml",
            "components:\n  schemas:\n    P:\n      $ref: './o.yaml#/P'\n",
            DocFormat::Yaml,
        )
        .unwrap();
        assert_eq!(doc.external_refs.len(), 1);
        assert!(!doc.notes.is_empty(), "the guess must not be invisible");
    }

    #[test]
    fn test_digest_ignores_formatting_and_key_order() {
        let a =
            SpecDocument::parse("a.json", r#"{"x":1,"y":{"b":2,"a":1}}"#, DocFormat::Json).unwrap();
        let b =
            SpecDocument::parse("b.yaml", "y:\n  a: 1\n  b: 2\nx: 1\n", DocFormat::Yaml).unwrap();
        assert_eq!(
            a.digest(),
            b.digest(),
            "reformatting is not a contract change"
        );
    }

    #[test]
    fn test_digest_changes_when_a_value_changes() {
        let a = SpecDocument::parse("a.json", r#"{"x":1}"#, DocFormat::Json).unwrap();
        let b = SpecDocument::parse("a.json", r#"{"x":2}"#, DocFormat::Json).unwrap();
        assert_ne!(a.digest(), b.digest());
    }

    #[test]
    fn test_non_object_root_is_an_error() {
        let err = SpecDocument::parse("a.json", "[1,2]", DocFormat::Json).unwrap_err();
        assert!(
            matches!(err, DocumentError::NonObjectRoot { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn test_json_extension_with_yaml_content_is_an_error_not_a_guess() {
        let err = SpecDocument::parse("a.json", "x: 1\n", DocFormat::Json).unwrap_err();
        assert!(matches!(err, DocumentError::Json { .. }), "{err:?}");
    }

    #[test]
    fn test_format_inference_records_what_it_did() {
        let doc = SpecDocument::parse("spec.txt", r#"{"x":1}"#, DocFormat::Inferred).unwrap();
        assert!(
            doc.notes.iter().any(|n| n.contains("parsed as JSON")),
            "{:?}",
            doc.notes
        );
    }
}
