//! OpenAPI 3.2.1 contract model (Step 37).
//!
//! Parses `paths`, operations, parameters, request bodies, and responses into a
//! canonical `BTreeMap`-ordered model, eagerly resolving local `$ref`s so the
//! diff never has to guess at a reference.
//!
//! # What is deliberately not modelled
//! JSON Schema composition (`allOf`, `oneOf`, `anyOf`, `not`, `if`/`then`/`else`)
//! and several conditional keywords are recorded but **not interpreted**. They
//! are extremely common in real specs, and pretending to understand them would
//! produce exactly the wrong answer: a composed schema whose constraints moved
//! between branches would look unchanged.
//!
//! Instead, every occurrence is recorded by JSON pointer in
//! [`SchemaFacts::unmodeled`] and degrades that subtree's coverage. The diff then
//! refuses to conclude "compatible" for it. Reporting a gap is recoverable;
//! reporting a false approval is not.

use crate::document::{SpecDocument, type_name, worse_of};
use autopsy_domain::CoverageState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// Version of OpenAPI this engine reads natively.
pub const TARGET_OPENAPI_MINOR: &str = "3.2";

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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OpenApiError {
    #[error("{path}: missing `openapi` version field")]
    MissingVersion { path: String },
    #[error(
        "{path}: unsupported OpenAPI version `{found}`. This engine reads 3.1.x and 3.2.x. Refusing rather than emitting a diff in which every endpoint appears removed."
    )]
    UnsupportedVersion { path: String, found: String },
    #[error("{path}: `paths` must be a mapping, found {found}")]
    BadPaths { path: String, found: &'static str },
    #[error("{path}: unresolvable reference `{reference}` ({reason})")]
    BadRef {
        path: String,
        reference: String,
        reason: String,
    },
}

impl OpenApiError {
    pub fn path(&self) -> &str {
        match self {
            Self::MissingVersion { path }
            | Self::UnsupportedVersion { path, .. }
            | Self::BadPaths { path, .. }
            | Self::BadRef { path, .. } => path,
        }
    }
}

/// HTTP methods recognised in a path item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HttpMethod {
    Get,
    Put,
    Post,
    Delete,
    Options,
    Head,
    Patch,
    Trace,
}

impl HttpMethod {
    pub const ALL: [HttpMethod; 8] = [
        Self::Get,
        Self::Put,
        Self::Post,
        Self::Delete,
        Self::Options,
        Self::Head,
        Self::Patch,
        Self::Trace,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::Put => "put",
            Self::Post => "post",
            Self::Delete => "delete",
            Self::Options => "options",
            Self::Head => "head",
            Self::Patch => "patch",
            Self::Trace => "trace",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.as_str() == s)
    }
}

/// Where a parameter is carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamLocation {
    Query,
    Header,
    Path,
    Cookie,
}

impl ParamLocation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Header => "header",
            Self::Path => "path",
            Self::Cookie => "cookie",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "query" => Some(Self::Query),
            "header" => Some(Self::Header),
            "path" => Some(Self::Path),
            "cookie" => Some(Self::Cookie),
            _ => None,
        }
    }
}

/// Identity of a parameter within an operation.
///
/// OpenAPI identifies a parameter by the `(name, in)` pair — the same name in a
/// different location is a different parameter. Keying on the name alone would
/// silently merge a query parameter with a header of the same name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ParamIdentity {
    pub location: ParamLocation,
    pub name: String,
}

impl ParamIdentity {
    pub fn describe(&self) -> String {
        format!("{} `{}`", self.location.as_str(), self.name)
    }
}

/// A request parameter after `$ref` resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parameter {
    pub identity: ParamIdentity,
    pub required: bool,
    pub schema: SchemaFacts,
    pub pointer: String,
}

/// A request body after `$ref` resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestBody {
    pub required: bool,
    /// Media type -> content schema. Keyed by media type because a body that is
    /// `application/json` in one version and `application/xml` in the next is a
    /// different contract, not a reformat.
    pub content: BTreeMap<String, SchemaFacts>,
    pub pointer: String,
}

impl RequestBody {
    /// Worst-case coverage across media types.
    pub fn coverage(&self) -> CoverageState {
        self.content
            .values()
            .fold(CoverageState::Verified, |acc, s| worse_of(acc, s.coverage))
    }
}

/// A response, reduced to what compatibility classification needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    pub status: String,
    pub pointer: String,
    /// Media type -> response schema.
    pub content: BTreeMap<String, SchemaFacts>,
    pub coverage: CoverageState,
}

impl Response {
    pub fn coverage_of(&self) -> CoverageState {
        self.content
            .values()
            .fold(self.coverage, |acc, s| worse_of(acc, s.coverage))
    }
}

/// One operation under a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    pub method: HttpMethod,
    pub path: String,
    pub operation_id: Option<String>,
    pub parameters: BTreeMap<ParamIdentity, Parameter>,
    pub request_body: Option<RequestBody>,
    pub responses: BTreeMap<String, Response>,
    /// Coverage of this operation alone.
    pub coverage: CoverageState,
    pub notes: Vec<String>,
}

impl Operation {
    /// Stable identity, independent of ordering elsewhere in the document.
    pub fn operation_key(&self) -> String {
        format!("{} {}", self.method.as_str().to_uppercase(), self.path)
    }

    /// JSON Pointer prefix for this operation.
    pub fn pointer(&self) -> String {
        format!("/paths/{}/{}", escape(&self.path), self.method.as_str())
    }
}

/// One entry in `paths`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathItem {
    pub path: String,
    pub operations: BTreeMap<HttpMethod, Operation>,
    pub coverage: CoverageState,
}

/// Parsed OpenAPI contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenApiSpec {
    pub document_path: String,
    pub declared_version: String,
    pub info_title: Option<String>,
    pub info_version: Option<String>,
    pub servers: Vec<String>,
    pub paths: BTreeMap<String, PathItem>,
    /// Coverage of the document as a whole.
    pub coverage: CoverageState,
    /// Every JSON pointer whose schema subtree was not fully modelled, sorted.
    pub unmodeled_pointers: BTreeSet<String>,
    /// References that could not be resolved, sorted.
    pub unresolved_refs: BTreeSet<String>,
    pub notes: Vec<String>,
}

impl OpenApiSpec {
    /// Every operation in the document, sorted by `(path, method)`.
    pub fn operations(&self) -> Vec<&Operation> {
        let mut out = Vec::new();
        for item in self.paths.values() {
            for op in item.operations.values() {
                out.push(op);
            }
        }
        out
    }

    /// Looks up a single operation.
    pub fn operation(&self, path: &str, method: HttpMethod) -> Option<&Operation> {
        self.paths.get(path)?.operations.get(&method)
    }

    /// BLAKE3 over the canonical contract surface.
    ///
    /// Built from the resolved model rather than the source document, so two
    /// specs that differ only in `$ref` layout compare equal.
    pub fn digest(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.declared_version.as_bytes());
        hasher.update(b"\n");
        for (path, item) in &self.paths {
            hasher.update(path.as_bytes());
            hasher.update(b"\n");
            for (method, op) in &item.operations {
                hasher.update(method.as_str().as_bytes());
                hasher.update(b"\n");
                for (id, param) in &op.parameters {
                    hasher.update(id.location.as_str().as_bytes());
                    hasher.update(b":");
                    hasher.update(id.name.as_bytes());
                    hasher.update(if param.required { b"=req" } else { b"=opt" });
                    hasher.update(param.schema.fingerprint().as_bytes());
                    hasher.update(b"\n");
                }
                if let Some(rb) = &op.request_body {
                    hasher.update(b"body");
                    hasher.update(if rb.required { b"=req" } else { b"=opt" });
                    for (media, facts) in &rb.content {
                        hasher.update(media.as_bytes());
                        hasher.update(facts.fingerprint().as_bytes());
                    }
                    hasher.update(b"\n");
                }
                for (status, response) in &op.responses {
                    hasher.update(b"resp:");
                    hasher.update(status.as_bytes());
                    for (media, facts) in &response.content {
                        hasher.update(media.as_bytes());
                        hasher.update(facts.fingerprint().as_bytes());
                    }
                    hasher.update(b"\n");
                }
            }
        }
        hasher.finalize().to_hex().to_string()
    }
}

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
struct SchemaReader<'a> {
    doc: &'a SpecDocument,
    unmodeled: BTreeSet<String>,
    unresolved: BTreeSet<String>,
    coverage: CoverageState,
    notes: Vec<String>,
}

impl<'a> SchemaReader<'a> {
    fn new(doc: &'a SpecDocument) -> Self {
        Self {
            doc,
            unmodeled: BTreeSet::new(),
            unresolved: BTreeSet::new(),
            coverage: CoverageState::Verified,
            notes: Vec::new(),
        }
    }

    /// Reads the schema at `pointer`, returning facts.
    fn read(&mut self, pointer: &str, depth: usize, seen: &BTreeSet<String>) -> SchemaFacts {
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

        let value = self.deref(value, pointer, seen, depth);
        self.read_value(value, pointer, depth, seen)
    }

    /// Follows a `$ref` chain, detecting cycles.
    fn deref(
        &mut self,
        value: &'a Value,
        pointer: &str,
        seen: &BTreeSet<String>,
        depth: usize,
    ) -> &'a Value {
        let mut current = value;
        let mut current_pointer = pointer.to_string();
        let mut chain = seen.clone();
        let mut hops = 0usize;

        while let Some(reference) = current.get("$ref").and_then(Value::as_str) {
            hops += 1;
            if hops > MAX_REF_DEPTH || chain.contains(reference) {
                self.coverage = worse_of(self.coverage, CoverageState::Partial);
                self.unresolved.insert(reference.to_string());
                self.notes.push(format!(
                    "reference `{reference}` at `{current_pointer}` is cyclic or exceeds the depth bound of {MAX_REF_DEPTH}"
                ));
                return &Value::Null;
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
                    return &Value::Null;
                }
            }
        }
        let _ = depth;
        current
    }

    fn read_value(
        &mut self,
        value: &Value,
        pointer: &str,
        depth: usize,
        seen: &BTreeSet<String>,
    ) -> SchemaFacts {
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
            Some(other) => {
                let _ = other;
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

        // --- additionalProperties ---------------------------------------
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
            for (name, _) in props {
                let child_pointer = format!("{pointer}/properties/{}", escape(name));
                let child = self.read(&child_pointer, depth + 1, seen);
                facts.coverage = worse_of(facts.coverage, child.coverage);
                facts.properties.insert(name.clone(), child);
            }
        }

        // --- items (arrays) ---------------------------------------------
        if let Some(items) = object.get("items") {
            match items {
                Value::Object(_) | Value::Bool(_) => {
                    let child_pointer = format!("{pointer}/items");
                    let child = self.read(&child_pointer, depth + 1, seen);
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

        // --- keywords this engine does not interpret ---------------------
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
fn canonical_scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Bool(_) | Value::Number(_) => Some(canonical_string_scalar(value)),
        _ => None,
    }
}

fn canonical_string_scalar(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

/// Escapes a JSON Pointer path segment per RFC 6901 §3.
pub fn escape(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

/// Parses an OpenAPI document into a canonical contract.
pub fn parse_openapi(doc: &SpecDocument) -> Result<OpenApiSpec, OpenApiError> {
    let path = doc.path.clone();

    let version = doc
        .pointer("openapi")
        .and_then(Value::as_str)
        .ok_or_else(|| OpenApiError::MissingVersion { path: path.clone() })?
        .to_string();

    let mut notes: Vec<String> = Vec::new();
    let mut coverage = CoverageState::Verified;

    let minor = minor_of(&version);
    match minor.as_deref() {
        Some(TARGET_OPENAPI_MINOR) => {}
        Some("3.1") => {
            coverage = worse_of(coverage, CoverageState::Partial);
            notes.push(
                "document declares OpenAPI 3.1.x; parsed under 3.2 semantics and 3.1-to-3.2 \
                 differences are not modelled"
                    .to_string(),
            );
        }
        Some("3.0") => {
            coverage = worse_of(coverage, CoverageState::Partial);
            notes.push(
                "document declares OpenAPI 3.0.x; `nullable` is honoured but other 3.0-to-3.1 \
                 schema differences are not modelled"
                    .to_string(),
            );
        }
        _ => {
            return Err(OpenApiError::UnsupportedVersion {
                path: path.clone(),
                found: version,
            });
        }
    }

    let paths_value = doc.pointer("/paths");
    let mut paths: BTreeMap<String, PathItem> = BTreeMap::new();
    let mut reader = SchemaReader::new(doc);

    match paths_value {
        None | Some(Value::Null) => {
            // A document with no `paths` is legal (webhooks-only). It has no
            // path surface to compare, which is not an error but is also not
            // something to claim completeness about.
            notes.push("document declares no `paths`; path-level comparison is empty".to_string());
            coverage = worse_of(coverage, CoverageState::Partial);
        }
        Some(Value::Object(map)) => {
            for (route, item_value) in map {
                let item = parse_path_item(doc, &mut reader, route, item_value, &mut notes)?;
                coverage = worse_of(coverage, item.coverage);
                paths.insert(route.clone(), item);
            }
        }
        Some(other) => {
            return Err(OpenApiError::BadPaths {
                path,
                found: type_name(other),
            });
        }
    }

    for note in &reader.notes {
        notes.push(note.clone());
    }
    if !reader.unmodeled.is_empty() {
        notes.push(format!(
            "{} JSON pointer(s) use constructs this engine records but does not interpret; a \
             change inside them could not be detected",
            reader.unmodeled.len()
        ));
    }
    if !reader.unresolved.is_empty() {
        notes.push(format!(
            "{} reference(s) or schema pointer(s) could not be resolved",
            reader.unresolved.len()
        ));
    }
    coverage = worse_of(coverage, reader.coverage);

    let servers = doc
        .pointer("/servers")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|s| s.get("url").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    notes.sort();
    notes.dedup();

    Ok(OpenApiSpec {
        document_path: path,
        declared_version: version,
        info_title: doc
            .pointer("/info/title")
            .and_then(Value::as_str)
            .map(str::to_string),
        info_version: doc
            .pointer("/info/version")
            .and_then(Value::as_str)
            .map(str::to_string),
        servers,
        paths,
        coverage,
        unmodeled_pointers: reader.unmodeled,
        unresolved_refs: reader.unresolved,
        notes,
    })
}

/// Extracts the `major.minor` prefix of an OpenAPI version string.
fn minor_of(version: &str) -> Option<String> {
    let mut parts = version.split('.');
    let major = parts.next()?;
    let minor = parts.next()?;
    if major.is_empty() || minor.is_empty() {
        return None;
    }
    if !major.chars().all(|c| c.is_ascii_digit()) || !minor.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(format!("{major}.{minor}"))
}

fn parse_path_item(
    doc: &SpecDocument,
    reader: &mut SchemaReader<'_>,
    route: &str,
    item_value: &Value,
    notes: &mut Vec<String>,
) -> Result<PathItem, OpenApiError> {
    let item_pointer = format!("/paths/{}", escape(route));
    let mut coverage = CoverageState::Verified;

    let Some(item_object) = item_value.as_object() else {
        return Ok(PathItem {
            path: route.to_string(),
            operations: BTreeMap::new(),
            coverage: CoverageState::Unknown,
        });
    };

    // Path-item-level parameters apply to every operation unless overridden.
    let mut shared: BTreeMap<ParamIdentity, Parameter> = BTreeMap::new();
    if let Some(Value::Array(items)) = item_object.get("parameters") {
        for (idx, param_value) in items.iter().enumerate() {
            let pointer = format!("{item_pointer}/parameters/{idx}");
            match parse_parameter(doc, reader, param_value, &pointer, route) {
                Ok(param) => {
                    coverage = worse_of(coverage, param.schema.coverage);
                    shared.insert(param.identity.clone(), param);
                }
                Err(e) => {
                    coverage = worse_of(coverage, CoverageState::Partial);
                    notes.push(format!(
                        "path-level parameter {idx} on `{route}` was dropped: {e}"
                    ));
                }
            }
        }
    }

    let mut operations: BTreeMap<HttpMethod, Operation> = BTreeMap::new();
    for (key, op_value) in item_object {
        let Some(method) = HttpMethod::parse(key) else {
            continue;
        };
        let op_pointer = format!("{item_pointer}/{key}");
        let mut parameters = shared.clone();
        let mut op_coverage = coverage;
        let mut op_notes: Vec<String> = Vec::new();

        if let Some(Value::Array(items)) = op_value.get("parameters") {
            for (idx, param_value) in items.iter().enumerate() {
                let pointer = format!("{op_pointer}/parameters/{idx}");
                match parse_parameter(doc, reader, param_value, &pointer, route) {
                    Ok(param) => {
                        // Operation-level parameters override path-item ones
                        // with the same (name, in) pair, per the specification.
                        parameters.insert(param.identity.clone(), param);
                    }
                    Err(e) => {
                        op_coverage = worse_of(op_coverage, CoverageState::Partial);
                        op_notes.push(format!("parameter {idx} was dropped: {e}"));
                    }
                }
            }
        }

        let request_body = match op_value.get("requestBody") {
            Some(rb) => {
                match parse_request_body(doc, reader, rb, &format!("{op_pointer}/requestBody")) {
                    Ok(rb) => Some(rb),
                    Err(e) => {
                        op_coverage = worse_of(op_coverage, CoverageState::Unknown);
                        op_notes.push(format!("requestBody not resolvable: {e}"));
                        None
                    }
                }
            }
            None => None,
        };

        let mut responses: BTreeMap<String, Response> = BTreeMap::new();
        match op_value.get("responses") {
            Some(Value::Object(map)) => {
                for (status, response_value) in map {
                    let status_pointer = format!("{op_pointer}/responses/{}", escape(status));
                    let content =
                        match read_content_map(doc, reader, response_value, &status_pointer) {
                            Ok(c) => c,
                            Err(e) => {
                                op_coverage = worse_of(op_coverage, CoverageState::Unknown);
                                op_notes.push(format!("response {status} not resolvable: {e}"));
                                BTreeMap::new()
                            }
                        };
                    let mut response = Response {
                        status: status.clone(),
                        pointer: status_pointer,
                        content,
                        coverage: CoverageState::Verified,
                    };
                    response.coverage = response.coverage_of();
                    responses.insert(status.clone(), response);
                }
            }
            Some(_) => {
                op_coverage = worse_of(op_coverage, CoverageState::Partial);
                op_notes.push("`responses` is not a mapping".to_string());
            }
            None => {
                op_coverage = worse_of(op_coverage, CoverageState::Partial);
                op_notes.push("operation declares no `responses`".to_string());
            }
        }

        for param in parameters.values() {
            op_coverage = worse_of(op_coverage, param.schema.coverage);
        }
        if let Some(rb) = &request_body {
            op_coverage = worse_of(op_coverage, rb.coverage());
        }
        for response in responses.values() {
            op_coverage = worse_of(op_coverage, response.coverage);
        }

        op_notes.sort();
        operation_coverage(&mut coverage, &mut op_coverage);
        operations.insert(
            method,
            Operation {
                method,
                path: route.to_string(),
                operation_id: op_value
                    .get("operationId")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                parameters,
                request_body,
                responses,
                coverage: op_coverage,
                notes: op_notes,
            },
        );
    }

    Ok(PathItem {
        path: route.to_string(),
        operations,
        coverage,
    })
}

fn operation_coverage(total: &mut CoverageState, op: &mut CoverageState) {
    *total = worse_of(*total, *op);
    *op = *total;
}

fn parse_parameter(
    doc: &SpecDocument,
    reader: &mut SchemaReader<'_>,
    value: &Value,
    pointer: &str,
    route: &str,
) -> Result<Parameter, String> {
    let (value, base_pointer) = resolve_shallow(doc, value, pointer)?;
    let object = value
        .as_object()
        .ok_or_else(|| "parameter is not an object".to_string())?;

    let name = object
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "parameter has no `name`".to_string())?
        .to_string();
    let location = object
        .get("in")
        .and_then(Value::as_str)
        .and_then(ParamLocation::parse)
        .ok_or_else(|| "parameter has no valid `in`".to_string())?;
    if location == ParamLocation::Path && !route.contains(&format!("{{{name}}}")) {
        return Err(format!(
            "path parameter `{name}` does not appear in route `{route}`"
        ));
    }

    // A path parameter is always required by the specification, regardless of
    // what the document says.
    let required = location == ParamLocation::Path
        || object.get("required").and_then(Value::as_bool) == Some(true);

    // Child pointers are computed from where the node was *found*, not from
    // where it was referenced. A `{"$ref": "..."}` node has no `/schema` child,
    // so computing from the reference site yields a lookup that always misses
    // and a contract that is permanently `Unknown`.
    let schema_pointer = format!("{base_pointer}/schema");
    let schema = if object.contains_key("schema") {
        reader.read(&schema_pointer, 0, &BTreeSet::new())
    } else {
        // `content`-typed parameters carry a media type map instead of a
        // schema. Nothing here can be compared, and saying so is better than an
        // empty schema that reads as "unconstrained".
        let content_pointer = format!("{base_pointer}/content");
        let mut facts = SchemaFacts {
            pointer: content_pointer.clone(),
            coverage: CoverageState::Partial,
            ..Default::default()
        };
        facts.unmodeled.insert(content_pointer);
        facts
    };

    Ok(Parameter {
        identity: ParamIdentity { location, name },
        required,
        schema,
        pointer: pointer.to_string(),
    })
}

fn parse_request_body(
    doc: &SpecDocument,
    reader: &mut SchemaReader<'_>,
    value: &Value,
    pointer: &str,
) -> Result<RequestBody, String> {
    let (value, base_pointer) = resolve_shallow(doc, value, pointer)?;
    let object = value
        .as_object()
        .ok_or_else(|| "requestBody is not an object".to_string())?;
    let required = object.get("required").and_then(Value::as_bool) == Some(true);
    let content = read_content_map(doc, reader, value, &base_pointer)?;
    Ok(RequestBody {
        required,
        content,
        pointer: pointer.to_string(),
    })
}

/// Reads a `content` mapping into media type -> schema facts.
fn read_content_map(
    doc: &SpecDocument,
    reader: &mut SchemaReader<'_>,
    node: &Value,
    pointer: &str,
) -> Result<BTreeMap<String, SchemaFacts>, String> {
    let content_pointer = format!("{pointer}/content");

    let Some(content) = node.get("content") else {
        return Ok(BTreeMap::new());
    };
    let Some(map) = content.as_object() else {
        return Err(format!("`{content_pointer}` is not a mapping"));
    };

    let mut out = BTreeMap::new();
    for media_type in map.keys() {
        let schema_pointer = format!("{content_pointer}/{}/schema", escape(media_type));
        if doc.pointer(&schema_pointer).is_none() {
            // A media type with no schema (for example `*/*: {}`) constrains
            // nothing. Recorded so the omission is visible.
            let facts = SchemaFacts {
                pointer: schema_pointer,
                coverage: CoverageState::Partial,
                ..Default::default()
            };
            out.insert(media_type.clone(), facts);
            continue;
        }
        out.insert(
            media_type.clone(),
            reader.read(&schema_pointer, 0, &BTreeSet::new()),
        );
    }
    Ok(out)
}

/// Resolves a single `$ref` hop on a non-schema node.
///
/// Returns the resolved value **and the JSON pointer it was found at**. The
/// second half matters: child pointers must be computed relative to where the
/// node actually lives. A `{"$ref": "#/components/parameters/Q"}` node has no
/// `/schema` child of its own, so resolving the reference without also moving
/// the pointer produces a lookup that always misses and a contract that is
/// permanently `Unknown`.
fn resolve_shallow<'v>(
    doc: &'v SpecDocument,
    value: &'v Value,
    pointer: &str,
) -> Result<(&'v Value, String), String> {
    match value.get("$ref").and_then(Value::as_str) {
        Some(reference) => {
            let resolved = doc
                .resolve(reference)
                .map_err(|e| format!("`{pointer}` -> {e}"))?;
            let resolved_pointer = crate::document::pointer_of(reference)
                .map(str::to_string)
                .unwrap_or_else(|_| pointer.to_string());
            Ok((resolved, resolved_pointer))
        }
        None => Ok((value, pointer.to_string())),
    }
}
