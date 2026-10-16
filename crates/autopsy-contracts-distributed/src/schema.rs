//! Shared JSON-Schema reading layer for the contract engines.
//!
//! Both OpenAPI (Step 37) and AsyncAPI (Step 38) describe message and request
//! bodies with JSON Schema, so the reader lives here rather than in either
//! engine. Duplicating it would mean two independent opinions about what a
//! `$ref` cycle is and two places for the "unmodelled" disclosure to drift.
//!
//! # What this layer refuses to do
//! JSON Schema composition (`allOf`, `oneOf`, `anyOf`, `not`, `if`/`then`/`else`)
//! and several conditional keywords are **recorded but not interpreted**. They
//! are extremely common in real specifications, and pretending to understand
//! them produces exactly the wrong answer: a composed schema whose constraints
//! moved between branches looks unchanged.
//!
//! Instead, every occurrence is named by JSON pointer in
//! [`SchemaFacts::unmodeled`] and degrades that subtree's coverage, which
//! prevents a `compatible` verdict for it. Reporting a gap is recoverable;
//! reporting a false approval is not.

use crate::document::{SpecDocument, worse_of};
use autopsy_domain::CoverageState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Maximum `$ref` chain length before the chain is treated as under-specified.
pub const MAX_REF_DEPTH: usize = 24;

/// Maximum schema nesting depth. Deeper trees are truncated with a coverage
/// penalty rather than walked without bound (`INV-SANDBOX-004`).
pub const MAX_SCHEMA_DEPTH: usize = 64;

/// JSON Schema keywords this engine records but does not interpret.
pub const UNMODELED_KEYWORDS: &[&str] = &[
    "allOf",
    "oneOf",
    "anyOf",
    "not",
    "if",
    "then",
    "else",
    "patternProperties",
    "dependentSchemas",
    "dependentRequired",
    "unevaluatedProperties",
    "unevaluatedItems",
    "discriminator",
    "prefixItems",
    "contains",
    "propertyNames",
];

/// Resolved, recursively-described schema facts.
///
/// Fields mirror only what compatibility classification needs. Everything else
/// lives in [`SchemaFacts::unmodeled`] as a disclosure rather than a guess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaFacts {
    pub pointer: String,
    /// Full declared type set. Empty means unconstrained *or* unresolved; the
    /// two are distinguished by [`SchemaFacts::coverage`], never conflated.
    pub types: BTreeSet<String>,
    /// `None` means no `enum` constraint was declared, which is different from
    /// an empty enum.
    pub enum_values: Option<BTreeSet<String>>,
    pub properties: BTreeMap<String, SchemaFacts>,
    pub required: BTreeSet<String>,
    /// `None` means unspecified (JSON Schema default: allowed).
    pub additional_properties: Option<bool>,
    pub coverage: CoverageState,
    /// JSON pointers of keywords that were recorded but not interpreted.
    pub unmodeled: BTreeSet<String>,
}

impl Default for SchemaFacts {
    /// An unpopulated schema is `Unknown`, not `Verified`. There is no state in
    /// which "nothing was read" may be read as "verified to constrain nothing".
    fn default() -> Self {
        Self {
            pointer: String::new(),
            types: BTreeSet::new(),
            enum_values: None,
            properties: BTreeMap::new(),
            required: BTreeSet::new(),
            additional_properties: None,
            coverage: CoverageState::Unknown,
            unmodeled: BTreeSet::new(),
        }
    }
}

impl SchemaFacts {
    /// A schema that constrains nothing. Coverage is `Unknown` so it can never
    /// be read as "verified to constrain nothing".
    pub fn unknown(pointer: &str) -> Self {
        Self {
            pointer: pointer.to_string(),
            coverage: CoverageState::Unknown,
            ..Default::default()
        }
    }

    /// Canonical fingerprint of the modelled facts. Used inside digests.
    ///
    /// Excludes `pointer` deliberately: the same schema behind a `$ref` and
    /// written inline are the same contract, and their pointers necessarily
    /// differ.
    pub fn fingerprint(&self) -> String {
        let mut out = String::new();
        out.push_str("t[");
        for t in &self.types {
            out.push_str(t);
            out.push(',');
        }
        out.push_str("]e[");
        if let Some(values) = &self.enum_values {
            for v in values {
                out.push_str(v);
                out.push(',');
            }
        }
        out.push_str("]r[");
        for r in &self.required {
            out.push_str(r);
            out.push(',');
        }
        out.push_str("]p{");
        for (k, v) in &self.properties {
            out.push_str(k);
            out.push('=');
            out.push_str(&v.fingerprint());
            out.push(';');
        }
        out.push('}');
        match self.additional_properties {
            Some(true) => out.push_str("ap=1"),
            Some(false) => out.push_str("ap=0"),
            None => out.push_str("ap=?"),
        }
        out
    }

    /// True when the modelled facts are complete enough to compare safely.
    pub fn is_comparable(&self) -> bool {
        self.coverage == CoverageState::Verified
    }

    /// Recursively collects every unmodelled pointer in this subtree.
    pub fn unmodeled_into<'a>(&'a self, out: &mut BTreeSet<&'a str>) {
        for p in &self.unmodeled {
            out.insert(p.as_str());
        }
        for child in self.properties.values() {
            child.unmodeled_into(out);
        }
    }
}

/// Reads a schema, resolving local references and disclosing what it cannot model.
pub struct SchemaReader<'a> {
    doc: &'a SpecDocument,
    unmodeled: BTreeSet<String>,
    unresolved: BTreeSet<String>,
    coverage: CoverageState,
    notes: Vec<String>,
}

impl<'a> SchemaReader<'a> {
    pub fn new(doc: &'a SpecDocument) -> Self {
        Self {
            doc,
            unmodeled: BTreeSet::new(),
            unresolved: BTreeSet::new(),
            coverage: CoverageState::Verified,
            notes: Vec::new(),
        }
    }

    pub fn unmodeled(&self) -> &BTreeSet<String> {
        &self.unmodeled
    }

    pub fn unresolved(&self) -> &BTreeSet<String> {
        &self.unresolved
    }

    pub fn coverage(&self) -> CoverageState {
        self.coverage
    }

    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// Records a pointer that could not be modelled, for a caller that has
    /// already determined that and does not need to read a schema.
    pub fn record_unmodeled(&mut self, pointer: &str) {
        self.unmodeled.insert(pointer.to_string());
        self.coverage = worse_of(self.coverage, CoverageState::Partial);
    }

    /// Resolves `value` (which may be a `$ref`) and reports where it ended up.
    ///
    /// Used by the AsyncAPI parser, where the resolved pointer is part of a
    /// message's identity in the catalogue and therefore has to be canonical.
    pub fn resolve_node(&mut self, value: &'a Value, pointer: &str) -> Option<(&'a Value, String)> {
        self.deref(value, pointer)
    }

    /// Degrades reader-wide coverage from outside the schema walk.
    pub fn degrade(&mut self, state: CoverageState) {
        self.coverage = worse_of(self.coverage, state);
    }

    /// Reads the schema at `pointer`, returning facts.
    pub fn read(&mut self, pointer: &str) -> SchemaFacts {
        self.read_at(pointer, 0)
    }

    fn read_at(&mut self, pointer: &str, depth: usize) -> SchemaFacts {
        if depth > MAX_SCHEMA_DEPTH {
            self.coverage = worse_of(self.coverage, CoverageState::Partial);
            self.unmodeled.insert(pointer.to_string());
            self.notes.push(format!(
                "schema at `{pointer}` exceeds the nesting bound of {MAX_SCHEMA_DEPTH}; not descended"
            ));
            return SchemaFacts::unknown(pointer);
        }

        let Some(value) = self.doc.pointer(pointer) else {
            // A pointer that does not exist is a broken document, not an absent
            // constraint.
            self.coverage = worse_of(self.coverage, CoverageState::Unknown);
            self.unresolved.insert(pointer.to_string());
            return SchemaFacts::unknown(pointer);
        };

        // A reference chain that cannot be followed yields an unknown schema,
        // not a partially-read one.
        let Some((value, _)) = self.deref(value, pointer) else {
            return SchemaFacts::unknown(pointer);
        };
        self.read_value(value, pointer, depth)
    }

    /// Follows a `$ref` chain, returning the resolved node **and the JSON
    /// pointer it was found at**.
    ///
    /// The pointer matters as much as the value. A node written as
    /// `{"$ref": "#/components/parameters/Q"}` has no `/schema` child of its
    /// own, so computing child pointers from the *reference site* silently
    /// misses every one of them — which is how this engine previously reported
    /// fully-modelled documents as `Unknown` whenever they used `$ref`.
    ///
    /// `None` means the chain could not be followed at all; the caller must not
    /// treat that as "no constraints".
    fn deref(&mut self, value: &'a Value, pointer: &str) -> Option<(&'a Value, String)> {
        let mut current = value;
        let mut current_pointer = pointer.to_string();
        let mut chain: BTreeSet<String> = BTreeSet::new();
        let mut hops = 0usize;

        while let Some(reference) = current.get("$ref").and_then(Value::as_str) {
            hops += 1;
            if hops > MAX_REF_DEPTH || chain.contains(reference) {
                self.coverage = worse_of(self.coverage, CoverageState::Partial);
                self.unresolved.insert(reference.to_string());
                self.notes.push(format!(
                    "reference `{reference}` at `{current_pointer}` is cyclic or exceeds the depth bound of {MAX_REF_DEPTH}"
                ));
                return None;
            }
            chain.insert(reference.to_string());
            match self.doc.resolve(reference) {
                Ok(target) => {
                    current_pointer = reference.trim_start_matches('#').to_string();
                    current = target;
                }
                Err(e) => {
                    self.coverage = worse_of(self.coverage, CoverageState::Unknown);
                    self.unresolved.insert(reference.to_string());
                    self.notes.push(format!("at `{current_pointer}`: {e}"));
                    return None;
                }
            }
        }
        Some((current, current_pointer))
    }

    fn read_value(&mut self, value: &Value, pointer: &str, depth: usize) -> SchemaFacts {
        let Some(object) = value.as_object() else {
            // `schema: true` / `schema: false` are valid JSON Schema. `true`
            // constrains nothing; either way there is nothing to compare.
            self.coverage = worse_of(self.coverage, CoverageState::Partial);
            self.unmodeled.insert(pointer.to_string());
            return SchemaFacts::unknown(pointer);
        };

        let mut facts = SchemaFacts {
            pointer: pointer.to_string(),
            coverage: CoverageState::Verified,
            ..Default::default()
        };

        // --- types -------------------------------------------------------
        match object.get("type") {
            Some(Value::String(t)) => {
                facts.types.insert(t.clone());
            }
            Some(Value::Array(items)) => {
                for item in items {
                    match item.as_str() {
                        Some(t) => {
                            facts.types.insert(t.to_string());
                        }
                        None => {
                            facts.coverage = worse_of(facts.coverage, CoverageState::Partial);
                            facts.unmodeled.insert(format!("{pointer}/type"));
                        }
                    }
                }
            }
            Some(_) => {
                facts.coverage = worse_of(facts.coverage, CoverageState::Partial);
                facts.unmodeled.insert(format!("{pointer}/type"));
            }
            None => {}
        }

        // OpenAPI 3.0 expresses nullability as `nullable: true`. Reading a 3.0
        // document without this produces a wrong narrowing verdict, because
        // `["string","null"] -> "string"` is breaking and `"string" -> "string"`
        // is not.
        if object.get("nullable").and_then(Value::as_bool) == Some(true) {
            facts.types.insert("null".to_string());
        }

        // --- enum --------------------------------------------------------
        if let Some(enum_value) = object.get("enum") {
            match enum_value.as_array() {
                Some(items) => {
                    let mut values = BTreeSet::new();
                    for item in items {
                        match canonical_scalar(item) {
                            Some(s) => {
                                values.insert(s);
                            }
                            None => {
                                facts.coverage = worse_of(facts.coverage, CoverageState::Partial);
                                facts.unmodeled.insert(format!("{pointer}/enum"));
                            }
                        }
                    }
                    facts.enum_values = Some(values);
                }
                None => {
                    facts.coverage = worse_of(facts.coverage, CoverageState::Partial);
                    facts.unmodeled.insert(format!("{pointer}/enum"));
                }
            }
        }

        // --- required ----------------------------------------------------
        if let Some(Value::Array(items)) = object.get("required") {
            for item in items {
                match item.as_str() {
                    Some(name) => {
                        facts.required.insert(name.to_string());
                    }
                    None => {
                        facts.coverage = worse_of(facts.coverage, CoverageState::Partial);
                        facts.unmodeled.insert(format!("{pointer}/required"));
                    }
                }
            }
        }

        // --- additionalProperties ----------------------------------------
        match object.get("additionalProperties") {
            Some(Value::Bool(b)) => facts.additional_properties = Some(*b),
            Some(_) => {
                // Schema-valued `additionalProperties` is a constraint, not a
                // toggle. Treating it as `true` would hide a real narrowing.
                facts.coverage = worse_of(facts.coverage, CoverageState::Partial);
                facts
                    .unmodeled
                    .insert(format!("{pointer}/additionalProperties"));
            }
            None => {}
        }

        // --- properties --------------------------------------------------
        if let Some(Value::Object(props)) = object.get("properties") {
            for name in props.keys() {
                let child_pointer = format!("{pointer}/properties/{}", escape(name));
                let child = self.read_at(&child_pointer, depth + 1);
                facts.coverage = worse_of(facts.coverage, child.coverage);
                facts.properties.insert(name.clone(), child);
            }
        }

        // --- items (arrays) ----------------------------------------------
        if let Some(items) = object.get("items") {
            match items {
                Value::Object(_) | Value::Bool(_) => {
                    let child_pointer = format!("{pointer}/items");
                    let child = self.read_at(&child_pointer, depth + 1);
                    facts.coverage = worse_of(facts.coverage, child.coverage);
                    // Modelled as a synthetic property so array element
                    // narrowing participates in comparison.
                    facts.properties.insert("[]".to_string(), child);
                }
                _ => {
                    facts.coverage = worse_of(facts.coverage, CoverageState::Partial);
                    facts.unmodeled.insert(format!("{pointer}/items"));
                }
            }
        }

        // --- keywords this engine does not interpret ----------------------
        for keyword in UNMODELED_KEYWORDS {
            if object.contains_key(*keyword) {
                facts.coverage = worse_of(facts.coverage, CoverageState::Partial);
                let p = format!("{pointer}/{keyword}");
                facts.unmodeled.insert(p.clone());
                self.unmodeled.insert(p);
            }
        }
        for p in &facts.unmodeled {
            self.unmodeled.insert(p.clone());
        }
        self.coverage = worse_of(self.coverage, facts.coverage);
        facts
    }
}

/// Canonical string form of a scalar, or `None` if it is not a scalar.
///
/// A non-scalar enum member is not an error, but it cannot participate in a
/// value-set comparison, so the caller degrades rather than coercing it.
pub fn canonical_scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Bool(_) | Value::Number(_) => {
            Some(serde_json::to_string(value).unwrap_or_else(|_| "null".to_string()))
        }
        _ => None,
    }
}

/// Extracts the `major.minor` prefix of a version string.
///
/// `"3.1.0"` yields `"3.1"`, not `"1"`. Taking the second dot-separated
/// component directly reads the *patch* level, which silently classifies every
/// correct document as a version this engine does not target.
pub fn major_minor(version: &str) -> Option<String> {
    let mut parts = version.split('.');
    let major = parts.next()?;
    let minor: String = parts
        .next()?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    if major.is_empty() || minor.is_empty() {
        return None;
    }
    Some(format!("{major}.{minor}"))
}

/// Escapes a JSON Pointer path segment per RFC 6901 §3.
pub fn escape(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

/// Unescapes a JSON Pointer path segment per RFC 6901 §3, or `None` if it uses
/// an escape this RFC does not define.
///
/// The scan is left to right on purpose: `~01` is the two characters `~1`, not
/// the single character `/`. Replacing `~1` first would get that wrong.
pub fn unescape(segment: &str) -> Option<String> {
    let mut out = String::with_capacity(segment.len());
    let mut chars = segment.chars();
    while let Some(c) = chars.next() {
        if c == '~' {
            match chars.next() {
                Some('0') => out.push('~'),
                Some('1') => out.push('/'),
                _ => return None,
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}
