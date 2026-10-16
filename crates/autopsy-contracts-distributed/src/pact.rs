//! Pact consumer-driven contract model (Step 39).
//!
//! Parses Pact JSON artifacts `R27` into a canonical `BTreeMap`-ordered model.
//!
//! # A Pact is not a schema, and this matters
//!
//! Every other artifact this crate reads describes **a shape**. A Pact describes
//! **an exchange**: *this consumer sent exactly this request and observed exactly
//! this response*. That difference drives three decisions.
//!
//! 1. **A request is concrete, not templated.** An OpenAPI operation is declared
//!    at `/alligators/{id}`; a Pact interaction records `/alligators/3`. Matching
//!    one to the other is a *subsumption* problem, not an equality check, and
//!    [`crate::pact_xref`] does it segment by segment rather than by string
//!    comparison.
//! 2. **`matchingRules` change what "the same request" means.** A Pact that
//!    matches a header against a regex, or a body key against a type, does not
//!    describe one request — it describes a family. This engine does not
//!    interpret those rules, so their presence is **recorded and degraded**. It
//!    never evaluates them and reports a match as if it had.
//! 3. **The version matters in a way that is easy to get wrong.** Pact 1 and 2
//!    write `providerState` (a string) and a `query` **string**; Pact 3 and 4
//!    write `providerStates` (an array) and a `query` **object**. Reading a v2
//!    `query` string as if it were the v3 shape yields zero query parameters,
//!    which would silently turn every required-parameter check into a pass. Both
//!    shapes are therefore recognised on their own terms rather than assumed.
//!
//! # What is deliberately not modelled
//!
//! * **`matchingRules`** — recorded by pointer and degraded, never evaluated.
//! * **`providerStates`** — recorded but not interpreted. They describe the state
//!   a provider must be in, which is not a fact about its interface; they cannot
//!   hide an interface incompatibility, so they do not degrade coverage.
//! * **Message interactions** (`type: "Asynchronous/Messages"`). They are parsed
//!   and their contents fingerprinted, but the cross-reference in this step is
//!   against an HTTP interface, so a message interaction degrades coverage
//!   rather than inventing an HTTP match for it.
//! * **Pact plugins and `Synchronous/Messages`** — an interaction whose `type` is
//!   not one of the two known values is recorded and skipped. Skipping degrades
//!   coverage, so it can never be mistaken for a clean check.

use crate::document::{SpecDocument, canonical_json, type_name, worse_of};
use autopsy_domain::CoverageState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// Pact specification majors this engine reads.
pub const SUPPORTED_PACT_MAJORS: [u64; 4] = [1, 2, 3, 4];

/// Whether an interaction describes an HTTP exchange or a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionKind {
    /// `request` + `response`.
    Http,
    /// `contents` (+ optional `metaData`).
    Message,
}

/// The request half of an HTTP interaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PactRequest {
    /// Upper-cased, because Pact files in the wild write `get`, `Get`, and `GET`.
    pub method: String,
    /// The concrete path, with `{value, matchers}` wrappers unwrapped.
    pub path: String,
    /// Query parameters normalised into name → declared values, regardless of
    /// whether the file used the v2 string form or the v3 object form.
    pub query: BTreeMap<String, Vec<String>>,
    /// Header names lower-cased for matching; the first declared value is kept.
    pub headers: BTreeMap<String, String>,
    /// Whether the interaction declares a body at all. A body of `{}` is still a
    /// body, and conflating it with "no body" turns a missing-body check into a
    /// pass.
    pub has_body: bool,
    /// Top-level keys of the declared body, so an undeclared field can be named
    /// rather than merely counted. Empty when the body is not an object.
    pub body_keys: BTreeSet<String>,
    /// BLAKE3 of the canonical body, `None` when there is no body.
    pub body_digest: Option<String>,
}

/// The response half of an HTTP interaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PactResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub has_body: bool,
    pub body_keys: BTreeSet<String>,
    pub body_digest: Option<String>,
}

/// One interaction. Stored in document order and compared by position, because
/// two interactions may legitimately share a description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interaction {
    pub description: String,
    pub kind: InteractionKind,
    /// Provider states, sorted. Recorded, never interpreted.
    pub provider_states: BTreeSet<String>,
    pub request: Option<PactRequest>,
    pub response: Option<PactResponse>,
    /// Message interactions only: top-level keys of `contents`.
    pub contents_keys: BTreeSet<String>,
    /// Message interactions only: BLAKE3 of the canonical `contents`.
    pub contents_digest: Option<String>,
    /// JSON pointer of this interaction, for reporting.
    pub pointer: String,
    pub coverage: CoverageState,
}

/// A canonical Pact contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PactDocument {
    pub path: String,
    /// `metadata.pactSpecification.version`, when the artifact declares one.
    pub spec_version: Option<String>,
    pub consumer: String,
    pub provider: String,
    pub interactions: Vec<Interaction>,
    pub coverage: CoverageState,
    /// Every JSON pointer whose `matchingRules` wrapper was recorded but not
    /// evaluated, sorted.
    pub unmodeled_pointers: BTreeSet<String>,
    pub notes: Vec<String>,
}

impl PactDocument {
    /// Interactions that describe an HTTP exchange, in document order.
    pub fn http_interactions(&self) -> Vec<&Interaction> {
        self.interactions
            .iter()
            .filter(|i| i.kind == InteractionKind::Http)
            .collect()
    }

    /// BLAKE3 over the canonical contract surface.
    ///
    /// Built from the parsed model, not the source bytes, so key order and
    /// formatting cannot move the digest.
    pub fn digest(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.consumer.as_bytes());
        hasher.update(b"->");
        hasher.update(self.provider.as_bytes());
        hasher.update(b"\n");
        if let Some(version) = &self.spec_version {
            hasher.update(b"spec:");
            hasher.update(version.as_bytes());
            hasher.update(b"\n");
        }
        for interaction in &self.interactions {
            hasher.update(b"interaction:");
            hasher.update(format!("{:?}", interaction.kind).as_bytes());
            hasher.update(b"|");
            hasher.update(interaction.description.as_bytes());
            hasher.update(b"\n");
            for state in &interaction.provider_states {
                hasher.update(b"  state:");
                hasher.update(state.as_bytes());
                hasher.update(b"\n");
            }
            if let Some(request) = &interaction.request {
                hasher.update(b"  req:");
                hasher.update(request.method.as_bytes());
                hasher.update(b" ");
                hasher.update(request.path.as_bytes());
                hasher.update(b"\n");
                for (name, values) in &request.query {
                    hasher.update(b"    q:");
                    hasher.update(name.as_bytes());
                    hasher.update(b"=");
                    hasher.update(values.join("\u{1f}").as_bytes());
                    hasher.update(b"\n");
                }
                for (name, value) in &request.headers {
                    hasher.update(b"    h:");
                    hasher.update(name.as_bytes());
                    hasher.update(b"=");
                    hasher.update(value.as_bytes());
                    hasher.update(b"\n");
                }
                hasher.update(b"    body:");
                hasher.update(request.body_digest.as_deref().unwrap_or("\u{0}").as_bytes());
                hasher.update(b"\n");
            }
            if let Some(response) = &interaction.response {
                hasher.update(b"  resp:");
                hasher.update(response.status.to_string().as_bytes());
                hasher.update(b"\n");
                for (name, value) in &response.headers {
                    hasher.update(b"    h:");
                    hasher.update(name.as_bytes());
                    hasher.update(b"=");
                    hasher.update(value.as_bytes());
                    hasher.update(b"\n");
                }
                hasher.update(b"    body:");
                hasher.update(
                    response
                        .body_digest
                        .as_deref()
                        .unwrap_or("\u{0}")
                        .as_bytes(),
                );
                hasher.update(b"\n");
            }
            hasher.update(b"  contents:");
            hasher.update(
                interaction
                    .contents_digest
                    .as_deref()
                    .unwrap_or("\u{0}")
                    .as_bytes(),
            );
            hasher.update(b"\n");
        }
        hasher.finalize().to_hex().to_string()
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PactError {
    #[error("`{path}` is not a Pact artifact: no `consumer`/`provider` participant objects")]
    MissingParticipants { path: String },
    #[error("`{path}`: `{pointer}` is {found}, expected {expected}")]
    WrongShape {
        path: String,
        pointer: String,
        found: &'static str,
        expected: &'static str,
    },
}

/// Reads a participant name, accepting the string form that Pact 1 uses for
/// `consumer`/`provider` as well as the object form every later version uses.
fn participant_name(value: &Value) -> Option<&str> {
    match value {
        Value::String(name) => Some(name.as_str()),
        other => other.get("name").and_then(Value::as_str),
    }
}

/// Unwraps a Pact v2 `{ value, matchers }` field, reporting whether it found one.
///
/// The wrapper is how Pact 2 attaches a matcher to a *scalar* field such as
/// `path` or `query`. Returning the wrapper itself would make the path the
/// literal string `{"value":"/a","matchers":[]}`, which matches no provider
/// operation at all — a silent, total failure of the cross-reference.
///
/// The second half of the return value matters as much as the first: a wrapper
/// **is** a matching rule, so a call site that unwraps one must degrade coverage.
/// Discarding it would leave an interaction whose path was matched by a pattern
/// reporting `Verified`, claiming one concrete request had been checked when the
/// artifact described a family of them.
fn unwrap_matcher<'v>(
    value: &'v Value,
    pointer: &str,
    unmodeled: &mut BTreeSet<String>,
) -> (&'v Value, bool) {
    match value {
        Value::Object(map) if map.contains_key("matchers") => {
            unmodeled.insert(pointer.to_string());
            (map.get("value").unwrap_or(&Value::Null), true)
        }
        other => (other, false),
    }
}

/// Decodes one `application/x-www-form-urlencoded` component.
///
/// `+` is a space here, per the form-encoding rules a query string follows. A
/// malformed `%` sequence is kept literally rather than dropped, because
/// dropping it would change the parameter *name* and could turn a missing
/// required parameter into a present one.
fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    None => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Normalises a Pact `query` into name → declared values.
///
/// Pact 1 and 2 wrote a **string** (`"size=3&colour=red"`); Pact 3 and 4 write an
/// **object** whose values are arrays (because a key may repeat). Reading the
/// string form with the object reader yields zero parameters, which would make
/// every required-parameter check pass vacuously — so each form is read on its
/// own terms.
fn normalise_query(
    value: &Value,
    pointer: &str,
    notes: &mut Vec<String>,
    unmodeled: &mut BTreeSet<String>,
) -> (BTreeMap<String, Vec<String>>, bool) {
    let (value, wrapped) = unwrap_matcher(value, pointer, unmodeled);
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    match value {
        Value::String(raw) => {
            for pair in raw.split('&').filter(|p| !p.is_empty()) {
                let (name, val) = match pair.split_once('=') {
                    Some((n, v)) => (n, v),
                    None => (pair, ""),
                };
                out.entry(percent_decode(name))
                    .or_default()
                    .push(percent_decode(val));
            }
        }
        Value::Object(map) => {
            for (name, declared) in map {
                let values = match declared {
                    Value::Array(items) => {
                        let mut out_values = Vec::new();
                        for item in items {
                            match item.as_str() {
                                Some(s) => out_values.push(s.to_string()),
                                None => {
                                    notes.push(format!(
                                        "`{pointer}/{name}` declares a {} query value; only \
                                         string values are understood",
                                        type_name(item)
                                    ));
                                    out_values.push(canonical_json(item));
                                }
                            }
                        }
                        out_values
                    }
                    other => {
                        notes.push(format!(
                            "`{pointer}/{name}` declares a {} query value rather than the array \
                             Pact 3 specifies",
                            type_name(other)
                        ));
                        vec![
                            other
                                .as_str()
                                .map(str::to_string)
                                .unwrap_or_else(|| canonical_json(other)),
                        ]
                    }
                };
                out.entry(name.clone()).or_default().extend(values);
            }
        }
        Value::Null => {}
        other => notes.push(format!(
            "`{pointer}` is {}; no query parameters could be read",
            type_name(other)
        )),
    }
    // A repeated key with the same value carries no extra information, and
    // collapsing it keeps the digest stable across formatting.
    for values in out.values_mut() {
        values.sort();
        values.dedup();
    }
    (out, wrapped)
}

/// Normalises a header map: names lower-cased, first declared value kept.
fn normalise_headers(
    value: Option<&Value>,
    pointer: &str,
    notes: &mut Vec<String>,
) -> BTreeMap<String, String> {
    let Some(value) = value else {
        return BTreeMap::new();
    };
    let Some(map) = value.as_object() else {
        notes.push(format!(
            "`{pointer}` is {}; no headers could be read",
            type_name(value)
        ));
        return BTreeMap::new();
    };
    let mut out = BTreeMap::new();
    for (name, declared) in map {
        match declared {
            Value::String(text) => {
                out.entry(name.to_ascii_lowercase())
                    .or_insert_with(|| text.clone());
            }
            other => {
                notes.push(format!(
                    "`{pointer}/{name}` is {}; only string header values are understood",
                    type_name(other)
                ));
                out.entry(name.to_ascii_lowercase())
                    .or_insert_with(|| canonical_json(other));
            }
        }
    }
    out
}

/// Splits a body into (has_body, top-level keys, digest).
fn describe_body(body: Option<&Value>) -> (bool, BTreeSet<String>, Option<String>) {
    let Some(body) = body else {
        return (false, BTreeSet::new(), None);
    };
    if body.is_null() {
        // `"body": null` declares no body. Treating it as an empty one would
        // make a required-body check pass on an interaction that has none.
        return (false, BTreeSet::new(), None);
    }
    let keys = body
        .as_object()
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default();
    let digest = blake3::hash(canonical_json(body).as_bytes())
        .to_hex()
        .to_string();
    (true, keys, Some(digest))
}

/// Parses a Pact artifact into a canonical contract.
pub fn parse_pact(doc: &SpecDocument) -> Result<PactDocument, PactError> {
    let path = doc.path.clone();

    let consumer = doc
        .pointer("consumer")
        .and_then(participant_name)
        .map(str::to_string);
    let provider = doc
        .pointer("provider")
        .and_then(participant_name)
        .map(str::to_string);
    let (Some(consumer), Some(provider)) = (consumer, provider) else {
        return Err(PactError::MissingParticipants { path });
    };

    let mut notes: Vec<String> = Vec::new();
    let mut unmodeled: BTreeSet<String> = BTreeSet::new();
    let mut coverage = CoverageState::Verified;

    let spec_version = doc
        .pointer("/metadata/pactSpecification/version")
        .and_then(Value::as_str)
        .map(str::to_string);
    match &spec_version {
        Some(version) => {
            let major = version
                .split('.')
                .next()
                .and_then(|m| m.parse::<u64>().ok());
            match major {
                Some(m) if SUPPORTED_PACT_MAJORS.contains(&m) => {}
                Some(m) => {
                    coverage = worse_of(coverage, CoverageState::Partial);
                    notes.push(format!(
                        "artifact declares Pact {version} (major {m}); majors 1-4 are understood \
                         and the differences in {m} are not modelled"
                    ));
                }
                None => {
                    coverage = worse_of(coverage, CoverageState::Partial);
                    notes.push(format!(
                        "artifact declares Pact `{version}`, whose major version could not be \
                         read"
                    ));
                }
            }
        }
        None => {
            // Absence is common in hand-written and older artifacts, and it is a
            // real gap: `providerState` vs `providerStates` and the two `query`
            // shapes are chosen by version.
            coverage = worse_of(coverage, CoverageState::Partial);
            notes.push(
                "artifact declares no `metadata.pactSpecification.version`; both the v2 and v3+ \
                 field shapes were accepted, so which one the author intended is unknown"
                    .to_string(),
            );
        }
    }

    let interaction_entries: Vec<Value> = match doc.pointer("interactions") {
        Some(Value::Array(items)) => items.clone(),
        Some(other) => {
            return Err(PactError::WrongShape {
                path,
                pointer: "/interactions".to_string(),
                found: type_name(other),
                expected: "an array of interactions",
            });
        }
        None => {
            coverage = worse_of(coverage, CoverageState::Unknown);
            notes.push(
                "artifact declares no `interactions`; there is nothing to cross-reference"
                    .to_string(),
            );
            Vec::new()
        }
    };

    let mut interactions: Vec<Interaction> = Vec::new();
    for (index, node) in interaction_entries.iter().enumerate() {
        let pointer = format!("/interactions/{index}");
        let mut local = CoverageState::Verified;

        let description = match node.get("description").and_then(Value::as_str) {
            Some(d) => d.to_string(),
            None => {
                local = worse_of(local, CoverageState::Partial);
                notes.push(format!("`{pointer}` has no `description`"));
                format!("<interaction {index}>")
            }
        };

        // `type` is a Pact 4 field. Before it, the shape of the object decided:
        // `request`+`response` meant HTTP, `contents` meant a message.
        let kind = match node.get("type").and_then(Value::as_str) {
            Some(raw) if raw.eq_ignore_ascii_case("Synchronous/HTTP") => {
                Some(InteractionKind::Http)
            }
            Some(raw) if raw.eq_ignore_ascii_case("Asynchronous/Messages") => {
                Some(InteractionKind::Message)
            }
            Some(raw) => {
                // `Synchronous/Messages` and plugin types exist. Guessing at one
                // would produce a cross-reference against the wrong interface.
                local = worse_of(local, CoverageState::Partial);
                notes.push(format!(
                    "`{pointer}` declares interaction type `{raw}`, which this engine does not \
                     model; the interaction was recorded but not cross-referenced"
                ));
                None
            }
            None => {
                if node.get("request").is_some() && node.get("response").is_some() {
                    Some(InteractionKind::Http)
                } else if node.get("contents").is_some() {
                    Some(InteractionKind::Message)
                } else {
                    local = worse_of(local, CoverageState::Partial);
                    notes.push(format!(
                        "`{pointer}` declares neither a request/response pair nor `contents`; its \
                         kind could not be determined"
                    ));
                    None
                }
            }
        };

        // Pact 1 and 2: `providerState` (a string). Pact 3 and 4:
        // `providerStates` (an array of objects with a `name`).
        let mut provider_states: BTreeSet<String> = BTreeSet::new();
        match node.get("providerStates") {
            Some(Value::Array(items)) => {
                for (i, state) in items.iter().enumerate() {
                    match state.get("name").and_then(Value::as_str) {
                        Some(name) => {
                            provider_states.insert(name.to_string());
                        }
                        None => {
                            local = worse_of(local, CoverageState::Partial);
                            notes.push(format!("`{pointer}/providerStates/{i}` has no `name`"));
                        }
                    }
                }
            }
            Some(other) => {
                local = worse_of(local, CoverageState::Partial);
                notes.push(format!(
                    "`{pointer}/providerStates` is {}",
                    type_name(other)
                ));
            }
            None => {}
        }
        match node.get("providerState") {
            Some(Value::String(state)) => {
                provider_states.insert(state.clone());
            }
            Some(other) => {
                local = worse_of(local, CoverageState::Partial);
                notes.push(format!("`{pointer}/providerState` is {}", type_name(other)));
            }
            None => {}
        }

        // `matchingRules` make the interaction describe a family of requests
        // rather than one. They are recorded and degraded, never evaluated.
        for keyword in [
            "matchingRules",
            "requestMatchingRules",
            "responseMatchingRules",
        ] {
            if node.get(keyword).is_some() {
                local = worse_of(local, CoverageState::Partial);
                unmodeled.insert(format!("{pointer}/{keyword}"));
            }
        }

        let request = match kind {
            Some(InteractionKind::Http) => {
                let request_pointer = format!("{pointer}/request");
                match node.get("request") {
                    None => {
                        local = worse_of(local, CoverageState::Partial);
                        notes.push(format!("`{request_pointer}` is absent"));
                        None
                    }
                    Some(raw_request) => {
                        if raw_request.as_object().is_none() {
                            // A `request` that is not an object is a structural
                            // defect, not a coverage gap: there is no
                            // interaction here to reason about at all.
                            return Err(PactError::WrongShape {
                                path: path.clone(),
                                pointer: request_pointer,
                                found: type_name(raw_request),
                                expected: "a request object",
                            });
                        }
                        let method = match raw_request.get("method").and_then(Value::as_str) {
                            Some(m) => m.to_ascii_uppercase(),
                            None => {
                                local = worse_of(local, CoverageState::Partial);
                                notes.push(format!(
                                    "`{request_pointer}/method` is absent; the interaction cannot \
                                     be matched to a provider operation"
                                ));
                                String::new()
                            }
                        };
                        let path = match raw_request.get("path") {
                            Some(value) => {
                                let (unwrapped, wrapped) = unwrap_matcher(
                                    value,
                                    &format!("{request_pointer}/path"),
                                    &mut unmodeled,
                                );
                                if wrapped {
                                    local = worse_of(local, CoverageState::Partial);
                                }
                                match unwrapped.as_str() {
                                    Some(p) => p.to_string(),
                                    None => {
                                        local = worse_of(local, CoverageState::Partial);
                                        notes.push(format!(
                                            "`{request_pointer}/path` is {}",
                                            type_name(unwrapped)
                                        ));
                                        String::new()
                                    }
                                }
                            }
                            None => {
                                local = worse_of(local, CoverageState::Partial);
                                notes.push(format!("`{request_pointer}/path` is absent"));
                                String::new()
                            }
                        };
                        let query = match raw_request.get("query") {
                            Some(value) => {
                                let (query, wrapped) = normalise_query(
                                    value,
                                    &format!("{request_pointer}/query"),
                                    &mut notes,
                                    &mut unmodeled,
                                );
                                if wrapped {
                                    local = worse_of(local, CoverageState::Partial);
                                }
                                query
                            }
                            None => BTreeMap::new(),
                        };
                        let headers = normalise_headers(
                            raw_request.get("headers"),
                            &format!("{request_pointer}/headers"),
                            &mut notes,
                        );
                        if raw_request.get("matchingRules").is_some() {
                            local = worse_of(local, CoverageState::Partial);
                            unmodeled.insert(format!("{request_pointer}/matchingRules"));
                        }
                        let (has_body, body_keys, body_digest) =
                            describe_body(raw_request.get("body"));
                        Some(PactRequest {
                            method,
                            path,
                            query,
                            headers,
                            has_body,
                            body_keys,
                            body_digest,
                        })
                    }
                }
            }
            _ => None,
        };

        let response = match kind {
            Some(InteractionKind::Http) => {
                let response_pointer = format!("{pointer}/response");
                match node.get("response") {
                    None => {
                        local = worse_of(local, CoverageState::Partial);
                        notes.push(format!("`{response_pointer}` is absent"));
                        None
                    }
                    Some(raw_response) => {
                        let status = raw_response.get("status").and_then(Value::as_u64);
                        match status.and_then(|s| u16::try_from(s).ok()) {
                            Some(status) => {
                                let headers = normalise_headers(
                                    raw_response.get("headers"),
                                    &format!("{response_pointer}/headers"),
                                    &mut notes,
                                );
                                if raw_response.get("matchingRules").is_some() {
                                    local = worse_of(local, CoverageState::Partial);
                                    unmodeled.insert(format!("{response_pointer}/matchingRules"));
                                }
                                let (has_body, body_keys, body_digest) =
                                    describe_body(raw_response.get("body"));
                                Some(PactResponse {
                                    status,
                                    headers,
                                    has_body,
                                    body_keys,
                                    body_digest,
                                })
                            }
                            None => {
                                local = worse_of(local, CoverageState::Partial);
                                notes.push(format!(
                                    "`{response_pointer}/status` is absent or is not an integer \
                                     status"
                                ));
                                None
                            }
                        }
                    }
                }
            }
            _ => None,
        };

        // Message contents are a *value*, not a schema, so they are
        // fingerprinted rather than interpreted as one.
        let (contents_keys, contents_digest) = match kind {
            Some(InteractionKind::Message) => {
                for keyword in ["contents", "contentsMatchingRules"] {
                    if node.get(keyword).is_some() {
                        unmodeled.insert(format!("{pointer}/{keyword}"));
                    }
                }
                match node.get("contents") {
                    Some(contents) => {
                        let keys = contents
                            .as_object()
                            .map(|map| map.keys().cloned().collect())
                            .unwrap_or_default();
                        let digest = blake3::hash(canonical_json(contents).as_bytes())
                            .to_hex()
                            .to_string();
                        (keys, Some(digest))
                    }
                    None => {
                        local = worse_of(local, CoverageState::Partial);
                        notes.push(format!("`{pointer}/contents` is absent"));
                        (BTreeSet::new(), None)
                    }
                }
            }
            _ => (BTreeSet::new(), None),
        };

        if kind.is_none() {
            // Recorded so the count is honest, but with no reader attached to
            // it: this interaction contributed nothing to the cross-reference.
            local = worse_of(local, CoverageState::Partial);
        }

        coverage = worse_of(coverage, local);
        interactions.push(Interaction {
            description,
            kind: kind.unwrap_or(InteractionKind::Http),
            provider_states,
            request,
            response,
            contents_keys,
            contents_digest,
            pointer,
            coverage: local,
        });
    }

    if interactions.is_empty() {
        // A Pact with no interactions asserts nothing. Reaching `Compatible` on
        // it would be an approval of an empty claim.
        coverage = worse_of(coverage, CoverageState::Partial);
        notes.push(
            "artifact declares no interactions, so nothing was asserted and nothing was checked"
                .to_string(),
        );
    }

    let states: usize = interactions.iter().map(|i| i.provider_states.len()).sum();
    if states > 0 {
        notes.push(format!(
            "{states} provider state(s) are recorded but not interpreted; they describe the state \
             a provider must be in, which is not a fact about its interface"
        ));
    }

    notes.push(format!(
        "`{}` (consumer) -> `{}` (provider): {} interaction(s)",
        consumer,
        provider,
        interactions.len()
    ));
    if !unmodeled.is_empty() {
        notes.push(format!(
            "{} `matchingRules`/matcher wrapper(s) were recorded but not evaluated; a match \
             against a pattern is a family of requests, not one",
            unmodeled.len()
        ));
    }
    notes.sort();
    notes.dedup();

    Ok(PactDocument {
        path,
        spec_version,
        consumer,
        provider,
        interactions,
        coverage,
        unmodeled_pointers: unmodeled,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::DocFormat;

    fn doc_of(json: &str) -> SpecDocument {
        SpecDocument::parse("pact.json", json, DocFormat::Json).expect("parses as a document")
    }

    fn parse(json: &str) -> PactDocument {
        parse_pact(&doc_of(json)).expect("parses as a pact")
    }

    fn error(json: &str) -> PactError {
        parse_pact(&doc_of(json)).unwrap_err()
    }

    const V3: &str = r###"{
      "consumer": { "name": "ConsumerA" },
      "provider": { "name": "ProviderB" },
      "interactions": [
        {
          "description": "a request for alligators",
          "providerStates": [ { "name": "there are alligators" } ],
          "request": {
            "method": "get",
            "path": "/alligators",
            "query": "size=3&colour=red",
            "headers": { "Accept": "application/json" },
            "body": { "colour": "red" }
          },
          "response": {
            "status": 200,
            "headers": { "Content-Type": "application/json" },
            "body": { "id": 1 }
          }
        }
      ],
      "metadata": { "pactSpecification": { "version": "3.0.0" } }
    }"###;

    #[test]
    fn a_v3_artifact_parses_into_a_canonical_model() {
        let pact = parse(V3);
        assert_eq!(pact.consumer, "ConsumerA");
        assert_eq!(pact.provider, "ProviderB");
        assert_eq!(pact.spec_version.as_deref(), Some("3.0.0"));
        assert_eq!(pact.interactions.len(), 1);
        assert_eq!(pact.coverage, CoverageState::Verified);

        let interaction = &pact.interactions[0];
        assert_eq!(interaction.kind, InteractionKind::Http);
        assert_eq!(interaction.description, "a request for alligators");
        assert_eq!(
            interaction.provider_states,
            BTreeSet::from(["there are alligators".to_string()])
        );

        let request = interaction.request.as_ref().expect("request");
        assert_eq!(
            request.method, "GET",
            "Pact files write `get`; matching is upper-case"
        );
        assert_eq!(request.path, "/alligators");
        assert_eq!(
            request.query,
            BTreeMap::from([
                ("colour".to_string(), vec!["red".to_string()]),
                ("size".to_string(), vec!["3".to_string()]),
            ])
        );
        assert_eq!(
            request.headers.get("accept").map(String::as_str),
            Some("application/json")
        );
        assert!(request.has_body);
        assert_eq!(request.body_keys, BTreeSet::from(["colour".to_string()]));

        let response = interaction.response.as_ref().expect("response");
        assert_eq!(response.status, 200);
        assert_eq!(
            response.headers.get("content-type").map(String::as_str),
            Some("application/json")
        );
    }

    #[test]
    fn the_v2_query_string_and_the_v3_query_object_produce_the_same_parameters() {
        // This is the single most dangerous version difference in the format.
        // Reading a v2 string with the v3 reader yields *zero* parameters, which
        // turns every required-query-parameter check into a vacuous pass.
        let v2 = V3
            .replace(
                "\"query\": \"size=3&colour=red\"",
                "\"query\": { \"size\": [\"3\"], \"colour\": [\"red\"] }",
            )
            .replace(
                "\"providerStates\": [ { \"name\": \"there are alligators\" } ]",
                "\"providerState\": \"there are alligators\"",
            );
        assert_ne!(v2, V3, "fixture edit was a no-op");
        let string_form = parse(V3);
        let object_form = parse(&v2);

        let a = string_form.interactions[0].request.as_ref().unwrap();
        let b = object_form.interactions[0].request.as_ref().unwrap();
        assert!(
            !a.query.is_empty(),
            "the v2 string form must not read as empty"
        );
        assert_eq!(a.query, b.query);
        assert_eq!(
            string_form.interactions[0].provider_states,
            object_form.interactions[0].provider_states,
            "`providerState` and `providerStates` must agree"
        );
        assert_eq!(string_form.digest(), object_form.digest());
    }

    #[test]
    fn percent_decoding_handles_the_form_encoding_a_query_string_uses() {
        assert_eq!(percent_decode("a+b"), "a b");
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("%2Fslash"), "/slash");
        // A malformed escape is kept literally: dropping it would change the
        // parameter *name* and could turn a missing required parameter into a
        // present one.
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }

    #[test]
    fn a_matcher_wrapper_on_a_scalar_field_is_unwrapped() {
        // Pact 2 wraps `path` when it carries a matcher. Returning the wrapper
        // would make the path the literal string `{"value":"/alligators"}`, which
        // matches no provider operation — a total, silent failure.
        let wrapped = V3.replace(
            "\"path\": \"/alligators\"",
            "\"path\": { \"value\": \"/alligators\", \"matchers\": [ { \"match\": \"regex\" } ] }",
        );
        assert_ne!(wrapped, V3, "fixture edit was a no-op");
        let pact = parse(&wrapped);
        assert_eq!(
            pact.interactions[0].request.as_ref().unwrap().path,
            "/alligators"
        );
        assert_eq!(pact.coverage, CoverageState::Partial);
        assert!(!pact.unmodeled_pointers.is_empty());
    }

    #[test]
    fn matching_rules_are_recorded_and_degrade_coverage() {
        let rules = V3.replace(
            "\"method\": \"get\",",
            "\"method\": \"get\",\n            \"matchingRules\": { \"header\": {} },",
        );
        assert_ne!(rules, V3, "fixture edit was a no-op");
        let pact = parse(&rules);
        assert_eq!(pact.coverage, CoverageState::Partial);
        assert!(
            pact.unmodeled_pointers
                .iter()
                .any(|p| p.ends_with("/matchingRules")),
            "{:?}",
            pact.unmodeled_pointers
        );
        assert!(
            pact.notes.iter().any(|n| n.contains("not evaluated")),
            "{:?}",
            pact.notes
        );
    }

    #[test]
    fn response_matching_rules_are_recorded_too() {
        let rules = V3.replace(
            "\"status\": 200,",
            "\"status\": 200,\n            \"matchingRules\": { \"body\": {} },",
        );
        assert_ne!(rules, V3, "fixture edit was a no-op");
        let pact = parse(&rules);
        assert_eq!(pact.coverage, CoverageState::Partial);
    }

    #[test]
    fn a_missing_spec_version_is_disclosed_rather_than_assumed() {
        let versionless = V3.replace(
            ",\n      \"metadata\": { \"pactSpecification\": { \"version\": \"3.0.0\" } }",
            "",
        );
        assert_ne!(versionless, V3, "fixture edit was a no-op");
        let pact = parse(&versionless);
        assert!(pact.spec_version.is_none());
        assert_eq!(pact.coverage, CoverageState::Partial);
        assert!(
            pact.notes
                .iter()
                .any(|n| n.contains("pactSpecification.version")),
            "{:?}",
            pact.notes
        );
    }

    #[test]
    fn a_future_pact_major_is_disclosed_rather_than_parsed_silently() {
        let future = V3.replace("\"version\": \"3.0.0\"", "\"version\": \"9.0.0\"");
        assert_ne!(future, V3, "fixture edit was a no-op");
        let pact = parse(&future);
        assert_eq!(pact.coverage, CoverageState::Partial);
        assert!(
            pact.notes.iter().any(|n| n.contains("major 9")),
            "{:?}",
            pact.notes
        );
    }

    #[test]
    fn participants_may_be_bare_strings_as_pact_1_writes_them() {
        let pact = parse(
            r###"{
              "consumer": "ConsumerA",
              "provider": "ProviderB",
              "interactions": []
            }"###,
        );
        assert_eq!(pact.consumer, "ConsumerA");
        assert_eq!(pact.provider, "ProviderB");
    }

    #[test]
    fn missing_participants_are_an_error() {
        assert!(matches!(
            error(r###"{ "interactions": [] }"###),
            PactError::MissingParticipants { .. }
        ));
    }

    #[test]
    fn interactions_of_the_wrong_shape_are_an_error() {
        assert!(matches!(
            error(r###"{ "consumer": "c", "provider": "p", "interactions": {} }"###),
            PactError::WrongShape { .. }
        ));
    }

    #[test]
    fn a_request_that_is_not_an_object_is_an_error_not_a_coverage_gap() {
        assert!(matches!(
            error(
                r###"{
                  "consumer": "c",
                  "provider": "p",
                  "interactions": [ { "description": "d", "request": "nope", "response": {} } ]
                }"###
            ),
            PactError::WrongShape { .. }
        ));
    }

    #[test]
    fn an_artifact_with_no_interactions_cannot_reach_verified_coverage() {
        let pact = parse(r###"{ "consumer": "c", "provider": "p", "interactions": [] }"###);
        assert!(pact.interactions.is_empty());
        assert_eq!(pact.coverage, CoverageState::Partial);
        assert!(
            pact.notes.iter().any(|n| n.contains("nothing was checked")),
            "{:?}",
            pact.notes
        );
    }

    #[test]
    fn a_message_interaction_records_its_contents() {
        let pact = parse(
            r###"{
              "consumer": "c",
              "provider": "p",
              "interactions": [
                {
                  "type": "Asynchronous/Messages",
                  "description": "an order was placed",
                  "contents": { "id": "1", "total": 3 }
                }
              ]
            }"###,
        );
        let interaction = &pact.interactions[0];
        assert_eq!(interaction.kind, InteractionKind::Message);
        assert_eq!(
            interaction.contents_keys,
            BTreeSet::from(["id".to_string(), "total".to_string()])
        );
        assert!(interaction.contents_digest.is_some());
        assert!(pact.http_interactions().is_empty());
    }

    #[test]
    fn an_unknown_interaction_type_degrades_instead_of_being_guessed_at() {
        let pact = parse(
            r###"{
              "consumer": "c",
              "provider": "p",
              "interactions": [
                { "type": "Synchronous/Messages", "description": "d", "contents": {} }
              ]
            }"###,
        );
        assert_eq!(pact.coverage, CoverageState::Partial);
        assert!(
            pact.notes
                .iter()
                .any(|n| n.contains("does not \n model") || n.contains("does not")),
            "{:?}",
            pact.notes
        );
    }

    #[test]
    fn a_null_body_is_not_an_empty_body() {
        // Conflating the two would make a required-request-body check pass on an
        // interaction that sends nothing.
        let pact = parse(
            r###"{
              "consumer": "c",
              "provider": "p",
              "interactions": [
                {
                  "description": "d",
                  "request": { "method": "POST", "path": "/a", "body": null },
                  "response": { "status": 200 }
                }
              ]
            }"###,
        );
        let request = pact.interactions[0].request.as_ref().unwrap();
        assert!(!request.has_body);
        assert!(request.body_digest.is_none());

        let empty = parse(
            r###"{
              "consumer": "c",
              "provider": "p",
              "interactions": [
                {
                  "description": "d",
                  "request": { "method": "POST", "path": "/a", "body": {} },
                  "response": { "status": 200 }
                }
              ]
            }"###,
        );
        let empty_request = empty.interactions[0].request.as_ref().unwrap();
        assert!(empty_request.has_body);
        assert!(empty_request.body_digest.is_some());
    }

    #[test]
    fn an_interaction_without_a_method_is_degraded_rather_than_matched_loosely() {
        let pact = parse(
            r###"{
              "consumer": "c",
              "provider": "p",
              "interactions": [
                { "description": "d", "request": { "path": "/a" }, "response": { "status": 200 } }
              ]
            }"###,
        );
        assert_eq!(pact.interactions[0].request.as_ref().unwrap().method, "");
        assert_eq!(pact.coverage, CoverageState::Partial);
    }

    #[test]
    fn provider_states_are_recorded_but_do_not_degrade_coverage() {
        // A provider state is not a fact about the interface, so it cannot hide
        // an interface incompatibility. Degrading for it would make every real
        // Pact permanently `partial`.
        let pact = parse(V3);
        assert_eq!(pact.coverage, CoverageState::Verified);
        assert!(
            pact.notes.iter().any(|n| n.contains("provider state")),
            "{:?}",
            pact.notes
        );
    }

    #[test]
    fn key_reordering_does_not_change_the_digest() {
        let reordered = r###"{
          "metadata": { "pactSpecification": { "version": "3.0.0" } },
          "interactions": [
            {
              "response": {
                "body": { "id": 1 },
                "headers": { "Content-Type": "application/json" },
                "status": 200
              },
              "request": {
                "body": { "colour": "red" },
                "headers": { "Accept": "application/json" },
                "query": { "colour": ["red"], "size": ["3"] },
                "path": "/alligators",
                "method": "GET"
              },
              "providerStates": [ { "name": "there are alligators" } ],
              "description": "a request for alligators"
            }
          ],
          "provider": { "name": "ProviderB" },
          "consumer": { "name": "ConsumerA" }
        }"###;
        assert_eq!(parse(V3).digest(), parse(reordered).digest());
        assert_eq!(parse(V3), parse(reordered));
    }

    #[test]
    fn the_digest_is_reproducible_across_100_runs() {
        let once = parse(V3).digest();
        for i in 0..100 {
            assert_eq!(parse(V3).digest(), once, "run {i} drifted");
        }
        assert_eq!(once.len(), 64);
    }
}
