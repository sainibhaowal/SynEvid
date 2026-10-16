//! The sanitized trace record (Step 40).
//!
//! # Two identities, because they answer different questions
//!
//! [`SpanRecord::digest`] covers the whole observation, timestamps included. It
//! answers *"is this the same observation?"* and is what `NFR-001` compares
//! across runs.
//!
//! [`SpanRecord::occupancy_key`] deliberately excludes every timestamp. It
//! answers *"was this code path executed in this window?"*, which is the question
//! Step 41's dead-code detection asks. A span that ran yesterday and the same
//! span that ran today are two different observations and **one** occupancy. If
//! the two were the same value, dead-code detection would count executions
//! instead of presence, and a busy path would look alive while a path that ran
//! once would be indistinguishable from one that ran a thousand times — which is
//! not the stated rule (§4.2 is "zero runtime invocations over *N* observation
//! windows", not "few invocations").
//!
//! # The salt is required, and that is not configurable
//!
//! Span and trace ids are pseudonymised with **BLAKE3 in keyed mode**, keyed by a
//! caller-supplied 32-byte salt.
//!
//! A salt is not optional. An OTLP span id is 64 bits: `BLAKE3(raw_id)` without a
//! key is a public function of a 2^64 space, so the mapping is invertible by
//! exhaustive search in seconds. Publishing such a hash would be obfuscation a
//! reader would reasonably mistake for protection — the exact "honest coverage"
//! failure this codebase forbids. Keyed BLAKE3 is a pseudorandom function, so
//! without the salt the mapping is not computable at all.
//!
//! Consequences the reader of a report must know, and which are therefore stated
//! rather than left implicit:
//!
//! * hashes are comparable **only within one salt**;
//! * the salt is an input to the engine, so `NFR-001` determinism is "identical
//!   inputs" where the salt is one of them;
//! * the output records **no** salt information. A fingerprint of the salt would
//!   let an attacker verify a guessed salt offline, so the caller is responsible
//!   for keeping the salt alongside the report.
//!
//! # Identifiers are normalised, and an un-normalisable one is an error
//!
//! OTLP defines trace ids as 32 lowercase-hex characters and span ids as 16, but
//! exporters differ on case. Hashing the raw text would give `EEE19B7EC3C1B174`
//! and `eee19b7ec3c1b174` two different pseudonyms for what the specification says
//! is one span — a silent split in the topology Step 41 draws. So identifiers are
//! lowercased, **and anything that is not the right count of hex characters is
//! rejected**. Degrading instead would leave Step 41 correlating on an identifier
//! it cannot trust while reporting nothing wrong.

use crate::otlp::{AnyValue, ResourceSpans, SpanKind, StatusCode, TraceRequest};
use crate::redact::{RedactionKind, Redactor, SanitizedEntry, SanitizedValue};
use autopsy_domain::CoverageState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// Inputs the caller must supply. The salt has no default; see the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestOptions {
    salt: [u8; 32],
}

impl IngestOptions {
    pub fn with_salt(salt: [u8; 32]) -> Self {
        Self { salt }
    }

    /// A fixed, **publicly known** salt. For tests, and for reproducing a report
    /// whose salt is already known to the reader.
    ///
    /// The name is the warning. Used on real telemetry it makes every pseudonym
    /// recomputable, because the key is right here in the source.
    pub const fn test_salt() -> Self {
        Self { salt: [0x51; 32] }
    }

    fn hash(&self, role: &str, raw: &str) -> String {
        let mut hasher = blake3::Hasher::new_keyed(&self.salt);
        hasher.update(b"autopsy-telemetry/");
        hasher.update(role.as_bytes());
        hasher.update(b"/");
        hasher.update(raw.as_bytes());
        hasher.finalize().to_hex().to_string()
    }
}

// There is deliberately no `Default`. A default salt would be a *public* key, and
// every pseudonym in every report produced with it would be recomputable by anyone
// holding an 8-byte span id — obfuscation presented as protection. Requiring the
// caller to name a salt, or to call the explicitly-named `test_salt`, makes that
// choice visible at the call site instead of hidden in a derive.

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IngestError {
    #[error(
        "`{pointer}` declares `{value}` as a {kind}; the specification requires {length} \
         lowercase-hex characters"
    )]
    BadIdentifier {
        pointer: String,
        value: String,
        kind: &'static str,
        length: usize,
    },
    #[error("`{pointer}` is absent; every span needs a {kind}")]
    MissingIdentifier { pointer: String, kind: &'static str },
}

fn normalise_identifier(
    raw: &str,
    pointer: &str,
    kind: &'static str,
    length: usize,
) -> Result<String, IngestError> {
    if raw.is_empty() {
        return Err(IngestError::MissingIdentifier {
            pointer: pointer.to_string(),
            kind,
        });
    }
    if raw.len() != length || !raw.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(IngestError::BadIdentifier {
            pointer: pointer.to_string(),
            value: raw.to_string(),
            kind,
            length,
        });
    }
    Ok(raw.to_ascii_lowercase())
}

/// The set of spans that represent the same code path, independent of when.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OccupancySubject {
    /// An `http.route` template, exactly as the instrumented code reported it.
    ///
    /// Not canonicalised here. Turning `/users/2` into `/users/{}` and matching
    /// that against a declared endpoint is Step 41's job, and doing it twice in
    /// two places is how the two definitions drift apart.
    Route(String),
    /// A `code.function`.
    Function(String),
    /// An `rpc.service` / `rpc.method` pair.
    Rpc(String),
    /// Nothing on this span says where in the code it came from. Recorded rather
    /// than dropped: a span that cannot be placed is a gap in the fusion, and
    /// dropping it would make the remaining coverage look complete.
    Uncorrelated,
}

/// The correlation key of an observation, with every timestamp excluded.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OccupancyKey {
    pub service: Option<String>,
    pub subject: OccupancySubject,
    pub http_method: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRecord {
    pub name: SanitizedValue,
    pub time_unix_nano: u64,
    pub attributes: Vec<SanitizedEntry>,
    pub coverage: CoverageState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkRecord {
    pub trace_id_hash: String,
    pub span_id_hash: String,
    pub attributes: Vec<SanitizedEntry>,
    pub coverage: CoverageState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusRecord {
    pub code: StatusCode,
    pub message: Option<SanitizedValue>,
}

/// One sanitized span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpanRecord {
    pub pointer: String,
    /// Keyed BLAKE3 of the normalised trace id.
    pub trace_id_hash: String,
    /// Keyed BLAKE3 of the normalised span id. The raw id appears nowhere.
    pub span_id_hash: String,
    pub parent_span_id_hash: Option<String>,
    pub name: SanitizedValue,
    pub kind: SpanKind,
    pub start_unix_nano: u64,
    pub duration_nanos: u64,
    pub attributes: Vec<SanitizedEntry>,
    pub events: Vec<EventRecord>,
    pub links: Vec<LinkRecord>,
    pub status: Option<StatusRecord>,
    pub service: Option<String>,
    pub route: Option<String>,
    pub url_path: Option<String>,
    pub code_function: Option<String>,
    pub code_filepath: Option<String>,
    pub http_method: Option<String>,
    pub http_status_code: Option<u16>,
    pub rpc_service: Option<String>,
    pub rpc_method: Option<String>,
    /// How many values of each reason were removed. Counts only — a
    /// per-value record would have to name the value.
    pub redactions: BTreeMap<RedactionKind, usize>,
    pub coverage: CoverageState,
    pub notes: Vec<String>,
    pub digest: String,
}

impl SpanRecord {
    /// The correlation key: everything that identifies *which code path ran*,
    /// and no timestamp. See the module docs for why the two identities differ.
    pub fn occupancy_key(&self) -> OccupancyKey {
        let subject = if let Some(route) = &self.route {
            OccupancySubject::Route(route.clone())
        } else if let Some(function) = &self.code_function {
            OccupancySubject::Function(function.clone())
        } else if let (Some(service), Some(method)) = (&self.rpc_service, &self.rpc_method) {
            OccupancySubject::Rpc(format!("{service}/{method}"))
        } else {
            OccupancySubject::Uncorrelated
        };
        OccupancyKey {
            service: self.service.clone(),
            subject,
            http_method: self.http_method.clone(),
        }
    }

    /// Whether this span can be placed in the static graph at all.
    pub fn is_correlated(&self) -> bool {
        self.occupancy_key().subject != OccupancySubject::Uncorrelated
    }

    /// The full observation digest, timestamps included.
    pub fn compute_digest(&self) -> String {
        digest_of(self)
    }
}

/// A whole ingested export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceRecord {
    pub document_path: String,
    pub span_count: usize,
    /// `service.name` values seen at resource level, sorted.
    pub services: BTreeSet<String>,
    /// Sorted by `(occupancy_key, digest)` so the record does not depend on the
    /// arbitrary order a collector happened to export its spans in.
    pub spans: Vec<SpanRecord>,
    pub redaction_totals: BTreeMap<RedactionKind, usize>,
    pub coverage: CoverageState,
    pub notes: Vec<String>,
    pub digest: String,
}

impl TraceRecord {
    /// Spans whose occupancy key matches, in record order.
    pub fn spans_for(&self, key: &OccupancyKey) -> Vec<&SpanRecord> {
        self.spans
            .iter()
            .filter(|span| &span.occupancy_key() == key)
            .collect()
    }

    /// Every distinct occupancy key observed, sorted.
    pub fn occupancy_keys(&self) -> BTreeSet<OccupancyKey> {
        self.spans.iter().map(SpanRecord::occupancy_key).collect()
    }
}

/// Fields a digest never covers, and the reason for each.
///
/// * `digest` — the field being computed.
/// * `pointer` — where the span sat in the export. A collector is free to export
///   its spans in any order, so a digest that covered this would make the same
///   observations digest differently depending on the order they arrived in. This
///   was a real defect: reversing two spans upstream of a clean ingest changed the
///   record digest, which would have made any downstream "has this changed?" check
///   answer yes for a reformatting.
/// * `notes` — prose, and allowed to be reworded without becoming a different
///   observation. The same rule the contract engines use.
///
/// Only the **top level** is stripped, so an attribute whose key happens to be
/// `notes` or `pointer` still reaches the digest — attributes live under
/// `attributes[].key`, not at the top level of a record.
const DIGEST_EXCLUDED_FIELDS: [&str; 3] = ["digest", "pointer", "notes"];

/// Serialises `value` and hashes it, with [`DIGEST_EXCLUDED_FIELDS`] removed.
///
/// `serde_json::Map` is a `BTreeMap` here, so the serialised object's keys are
/// sorted and the digest cannot depend on field declaration order.
fn digest_of<T: Serialize>(value: &T) -> String {
    let mut json = serde_json::to_value(value).unwrap_or(Value::Null);
    if let Value::Object(map) = &mut json {
        for field in DIGEST_EXCLUDED_FIELDS {
            map.remove(field);
        }
    }
    let text = serde_json::to_string(&json).unwrap_or_default();
    blake3::hash(text.as_bytes()).to_hex().to_string()
}

/// What a [`TraceRecord`] digest covers, spelled out rather than inherited.
///
/// A blanket serialisation of the record would reach into `spans[].pointer` and
/// `spans[].notes`, which are deliberately excluded from a span's own digest. The
/// projection makes the covered surface reviewable: coverage, the service set, the
/// redaction totals, and the span digests — which are already order-independent
/// because `spans` is sorted by occupancy key.
#[derive(Serialize)]
struct TraceDigestInput<'a> {
    document_path: &'a str,
    coverage: CoverageState,
    services: &'a BTreeSet<String>,
    redaction_totals: &'a BTreeMap<RedactionKind, usize>,
    span_digests: Vec<&'a str>,
}

fn trace_digest(record: &TraceRecord) -> String {
    let input = TraceDigestInput {
        document_path: &record.document_path,
        coverage: record.coverage,
        services: &record.services,
        redaction_totals: &record.redaction_totals,
        span_digests: record
            .spans
            .iter()
            .map(|span| span.digest.as_str())
            .collect(),
    };
    let text = serde_json::to_string(&input).unwrap_or_default();
    blake3::hash(text.as_bytes()).to_hex().to_string()
}

fn attribute_text(attributes: &[crate::otlp::KeyValue], key: &str) -> Option<String> {
    attributes
        .iter()
        .find(|attribute| attribute.key == key)
        .and_then(|attribute| match &attribute.value {
            AnyValue::Str(text) => Some(text.clone()),
            AnyValue::Int(number) => Some(number.to_string()),
            AnyValue::Double(text) => Some(text.clone()),
            _ => None,
        })
}

/// Reads a `u16` from either an integer or a canonicalised double.
fn as_u16(value: Option<&AnyValue>) -> Option<u16> {
    match value? {
        AnyValue::Int(number) => u16::try_from(*number).ok(),
        AnyValue::Double(text) => text.parse::<f64>().ok().map(|f| f as u16),
        _ => None,
    }
}

/// The path part of a span name or URL, when one is present.
///
/// Only used when `url.path` is absent. A span name is often `GET /users/2`, so
/// the path is the second whitespace-separated token when the first looks like an
/// HTTP method.
fn path_from_name(name: &str) -> Option<String> {
    let candidate = match name.split_once(' ') {
        Some((first, rest))
            if first.chars().all(|c| c.is_ascii_uppercase()) && first.len() <= 8 =>
        {
            rest
        }
        _ => name,
    };
    let trimmed = candidate.split(['?', '#']).next().unwrap_or(candidate);
    if trimmed.starts_with('/') && trimmed.len() > 1 {
        Some(trimmed.to_string())
    } else {
        None
    }
}

fn sanitize_resource_attributes(
    resource_spans: &ResourceSpans,
    redactor: &mut Redactor,
) -> Vec<SanitizedEntry> {
    resource_spans
        .resource
        .as_ref()
        .map(|resource| redactor.attributes(&resource.attributes))
        .unwrap_or_default()
}

fn sanitize_attributes(
    attributes: &[crate::otlp::KeyValue],
    redactor: &mut Redactor,
) -> (Vec<SanitizedEntry>, CoverageState) {
    let sanitized = redactor.attributes(attributes);
    let coverage = if sanitized.iter().any(|entry| {
        matches!(
            entry.value,
            SanitizedValue::FreeText(_) | SanitizedValue::Unmodelled(_)
        )
    }) {
        // A free-text value may carry PII no bounded pattern can find, and an
        // unmodelled node is a value that was never seen. Either one blocks a
        // clean verdict.
        CoverageState::Partial
    } else {
        CoverageState::Verified
    };
    (sanitized, coverage)
}

/// Sanitizes a parsed export into a record.
///
/// Returns an error, rather than a degraded record, when an identifier is
/// malformed. See the module docs: a silently un-normalisable identifier splits
/// the topology Step 41 builds, and reporting nothing wrong while doing so is the
/// failure mode worth refusing.
pub fn sanitize_traces(
    request: &TraceRequest,
    options: &IngestOptions,
) -> Result<TraceRecord, IngestError> {
    let mut notes: Vec<String> = Vec::new();
    let mut coverage = request.coverage;
    let mut spans: Vec<SpanRecord> = Vec::new();
    let mut services: BTreeSet<String> = BTreeSet::new();

    if !request.unmodelled_pointers.is_empty() {
        notes.push(format!(
            "{} node(s) in the export could not be represented faithfully and were recorded \
             rather than interpreted",
            request.unmodelled_pointers.len()
        ));
    }

    for (rs_index, resource_spans) in request.resource_spans.iter().enumerate() {
        let rs_pointer = format!("/resourceSpans/{rs_index}");
        let resource_redactor = &mut Redactor::new();
        let resource_attributes = sanitize_resource_attributes(resource_spans, resource_redactor);

        // The service name is the one resource attribute that reaches every span,
        // so it is read through the same structural path as the correlation
        // attributes rather than being trusted.
        let service = resource_attributes
            .iter()
            .find(|entry| entry.key == "service.name")
            .and_then(|entry| match &entry.value {
                SanitizedValue::Text(text) => Some(text.clone()),
                _ => None,
            });
        if let Some(name) = &service {
            services.insert(name.clone());
        }

        for (ss_index, scope_spans) in resource_spans.scope_spans.iter().enumerate() {
            for (span_index, span) in scope_spans.spans.iter().enumerate() {
                let pointer = format!("{rs_pointer}/scopeSpans/{ss_index}/spans/{span_index}");
                let redactor = &mut Redactor::new();
                let mut span_coverage = CoverageState::Verified;
                let mut span_notes: Vec<String> = Vec::new();

                let trace_id = normalise_identifier(&span.trace_id, &pointer, "trace id", 32)?;
                let span_id = normalise_identifier(&span.span_id, &pointer, "span id", 16)?;
                let parent_span_id_hash = match &span.parent_span_id {
                    Some(parent) => Some(options.hash(
                        "parent-span",
                        &normalise_identifier(parent, &pointer, "parent span id", 16)?,
                    )),
                    None => None,
                };

                if span.end_unix_nano < span.start_unix_nano {
                    span_coverage = worse(span_coverage, CoverageState::Partial);
                    span_notes.push(format!(
                        "`{pointer}` ends before it starts; its duration is not usable"
                    ));
                }

                let (attributes, attributes_coverage) =
                    sanitize_attributes(&span.attributes, redactor);
                span_coverage = worse(span_coverage, attributes_coverage);

                if span.dropped_attributes_count > 0
                    || span.dropped_events_count > 0
                    || span.dropped_links_count > 0
                {
                    // The SDK dropped data before the engine ever saw it. Counting
                    // the span as fully observed would be a claim about data that
                    // does not exist.
                    span_coverage = worse(span_coverage, CoverageState::Partial);
                    span_notes.push(format!(
                        "`{pointer}` reports dropped data ({} attribute(s), {} event(s), {} \
                         link(s)); the export is incomplete",
                        span.dropped_attributes_count,
                        span.dropped_events_count,
                        span.dropped_links_count
                    ));
                }

                let mut events = Vec::with_capacity(span.events.len());
                for (event_index, event) in span.events.iter().enumerate() {
                    let (event_attributes, event_coverage) =
                        sanitize_attributes(&event.attributes, redactor);
                    if event.dropped_attributes_count > 0 {
                        span_coverage = worse(span_coverage, CoverageState::Partial);
                        span_notes.push(format!(
                            "`{pointer}/events/{event_index}` reports {} dropped attribute(s)",
                            event.dropped_attributes_count
                        ));
                    }
                    span_coverage = worse(span_coverage, event_coverage);
                    events.push(EventRecord {
                        // Event names carry the same leak as span names: the
                        // OpenTelemetry HTTP instrumentation emits the raw target
                        // as an event name in some configurations.
                        name: redactor.span_name(&event.name),
                        time_unix_nano: event.time_unix_nano,
                        attributes: event_attributes,
                        coverage: event_coverage,
                    });
                }

                let mut links = Vec::with_capacity(span.links.len());
                for (link_index, link) in span.links.iter().enumerate() {
                    let (link_attributes, link_coverage) =
                        sanitize_attributes(&link.attributes, redactor);
                    span_coverage = worse(span_coverage, link_coverage);
                    let link_pointer = format!("{pointer}/links/{link_index}");
                    // A link with an empty id is what some exporters emit for a
                    // link they could not resolve; it is recorded as such rather
                    // than hashed, because hashing "" would mint a pseudonym for a
                    // span that does not exist.
                    let trace_id_hash = if link.trace_id.is_empty() {
                        span_notes.push(format!("`{link_pointer}` declares no `traceId`"));
                        span_coverage = worse(span_coverage, CoverageState::Partial);
                        String::new()
                    } else {
                        options.hash(
                            "link-trace",
                            &normalise_identifier(&link.trace_id, &link_pointer, "trace id", 32)?,
                        )
                    };
                    let span_id_hash = if link.span_id.is_empty() {
                        span_coverage = worse(span_coverage, CoverageState::Partial);
                        String::new()
                    } else {
                        options.hash(
                            "link-span",
                            &normalise_identifier(&link.span_id, &link_pointer, "span id", 16)?,
                        )
                    };
                    links.push(LinkRecord {
                        trace_id_hash,
                        span_id_hash,
                        attributes: link_attributes,
                        coverage: link_coverage,
                    });
                }

                let status = match &span.status {
                    None => None,
                    Some(status) => {
                        let message = status.message.as_ref().map(|message| {
                            // The specification says this field "MAY contain
                            // sensitive information". A value under a free-text
                            // segment is flagged, which degrades coverage.
                            redactor.value(Some("status.message"), &AnyValue::Str(message.clone()))
                        });
                        if matches!(message, Some(SanitizedValue::FreeText(_))) {
                            span_coverage = worse(span_coverage, CoverageState::Partial);
                        }
                        Some(StatusRecord {
                            code: status.code,
                            message,
                        })
                    }
                };

                // The correlation surface, read through the redactor so a
                // structural key that matched a rule reports `None` rather than a
                // placeholder Step 41 would then correlate against.
                let route = attribute_text(&span.attributes, "http.route");
                let url_path = attribute_text(&span.attributes, "url.path");
                let code_function = attribute_text(&span.attributes, "code.function");
                let code_filepath = attribute_text(&span.attributes, "code.filepath");
                let http_method = attribute_text(&span.attributes, "http.request.method");
                let rpc_service = attribute_text(&span.attributes, "rpc.service");
                let rpc_method = attribute_text(&span.attributes, "rpc.method");
                let status_code = span
                    .attributes
                    .iter()
                    .find(|attribute| attribute.key == "http.response.status_code")
                    .and_then(|attribute| as_u16(Some(&attribute.value)));

                let name = redactor.span_name(&span.name);
                let derived_path = match &url_path {
                    Some(path) => Some(path.clone()),
                    None => match &name {
                        SanitizedValue::Text(text) => path_from_name(text),
                        _ => None,
                    },
                };

                let mut record = SpanRecord {
                    pointer: pointer.clone(),
                    trace_id_hash: options.hash("trace", &trace_id),
                    span_id_hash: options.hash("span", &span_id),
                    parent_span_id_hash,
                    name,
                    kind: span.kind,
                    start_unix_nano: span.start_unix_nano,
                    duration_nanos: span.end_unix_nano.saturating_sub(span.start_unix_nano),
                    attributes,
                    events,
                    links,
                    status,
                    service: service.clone(),
                    route,
                    url_path: derived_path,
                    code_function,
                    code_filepath,
                    http_method,
                    http_status_code: status_code,
                    rpc_service,
                    rpc_method,
                    redactions: redactor.counts().clone(),
                    coverage: span_coverage,
                    notes: Vec::new(),
                    digest: String::new(),
                };

                if !record.is_correlated() {
                    // A span nothing can be placed against is a gap in the fusion.
                    // Reporting it as verified coverage would let Step 41's
                    // dead-code list be read as complete when the export contained
                    // spans it could not attribute at all.
                    record.coverage = worse(record.coverage, CoverageState::Partial);
                    span_notes.push(format!(
                        "`{pointer}` carries none of `http.route`, `code.function` or \
                         `rpc.service`, so it cannot be placed in the static graph"
                    ));
                }
                if redactor.total() > 0 {
                    span_notes.push(format!(
                        "{} value(s) were removed from `{pointer}`",
                        redactor.total()
                    ));
                }
                span_notes.sort();
                span_notes.dedup();
                record.notes = span_notes;

                coverage = worse(coverage, record.coverage);
                spans.push(record);
            }
        }
    }

    if spans.is_empty() {
        // An export with no spans asserts nothing about runtime behaviour.
        // Reaching a clean verdict on it would approve a fusion that never ran.
        coverage = worse(coverage, CoverageState::Unknown);
        notes
            .push("the export contains no spans, so no runtime behaviour was observed".to_string());
    }

    // Sorted by `(occupancy_key, digest)`. A collector is free to export its spans
    // in any order, so the record order must not be the export order: the same
    // observations exported twice in different orders must produce the same
    // record and the same digest.
    spans.sort_by(|a, b| {
        a.occupancy_key()
            .cmp(&b.occupancy_key())
            .then_with(|| a.digest.cmp(&b.digest))
    });
    for span in &mut spans {
        span.digest = span.compute_digest();
    }

    let mut redaction_totals: BTreeMap<RedactionKind, usize> = BTreeMap::new();
    for span in &spans {
        for (kind, count) in &span.redactions {
            *redaction_totals.entry(*kind).or_insert(0) += count;
        }
    }
    if !redaction_totals.is_empty() {
        notes.push(format!(
            "{} value(s) were removed across {kind_count} reason(s): {kinds}",
            redaction_totals.values().sum::<usize>(),
            kind_count = redaction_totals.len(),
            kinds = redaction_totals
                .keys()
                .map(|kind| kind.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    for note in &request.notes {
        notes.push(note.clone());
    }
    notes.push(format!(
        "{span_count} span(s) across {service_count} service(s); {correlated} could be placed",
        span_count = spans.len(),
        service_count = services.len(),
        correlated = spans.iter().filter(|span| span.is_correlated()).count()
    ));
    notes.push(format!(
        "Semantic Conventions {version} is the key set this engine reads; OTLP/JSON carries no \
         version of its own, so the exporter's version is not verifiable from the payload",
        version = crate::otlp::SEMCONV_VERSION
    ));
    notes.sort();
    notes.dedup();

    let mut record = TraceRecord {
        document_path: request.document_path.clone(),
        span_count: spans.len(),
        services,
        spans,
        redaction_totals,
        coverage,
        notes,
        digest: String::new(),
    };
    record.digest = trace_digest(&record);
    Ok(record)
}

/// The worse of two coverage states.
///
/// `Unsupported` outranks `Partial`: a construct the engine does not model at all
/// is a larger gap than one it modelled partially, and collapsing the two would
/// hide the difference in every report that shows a single state.
fn worse(left: CoverageState, right: CoverageState) -> CoverageState {
    use CoverageState::{Partial, Unknown, Unsupported, Verified};
    match (left, right) {
        (Unknown, _) | (_, Unknown) => Unknown,
        (Unsupported, _) | (_, Unsupported) => Unsupported,
        (Partial, _) | (_, Partial) => Partial,
        (Verified, Verified) => Verified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identifier_must_be_the_right_count_of_hex_characters() {
        assert_eq!(
            normalise_identifier("EEE19B7EC3C1B174", "/p", "span id", 16).unwrap(),
            "eee19b7ec3c1b174",
            "normalisation is lowercasing, so two spellings of one id agree"
        );
        assert!(normalise_identifier("eee19b7ec3c1b174", "/p", "span id", 16).is_ok());
        assert!(matches!(
            normalise_identifier("eee19b7ec3c1b17", "/p", "span id", 16),
            Err(IngestError::BadIdentifier { .. })
        ));
        assert!(matches!(
            normalise_identifier("zzz19b7ec3c1b174", "/p", "span id", 16),
            Err(IngestError::BadIdentifier { .. })
        ));
        assert!(matches!(
            normalise_identifier("", "/p", "span id", 16),
            Err(IngestError::MissingIdentifier { .. })
        ));
    }

    #[test]
    fn the_salt_changes_every_pseudonym() {
        let a = IngestOptions::with_salt([1; 32]);
        let b = IngestOptions::with_salt([2; 32]);
        assert_eq!(a.hash("span", "abc"), a.hash("span", "abc"));
        assert_ne!(a.hash("span", "abc"), b.hash("span", "abc"));
        // Role separation: the same raw id in two roles is not the same pseudonym,
        // so a span id cannot be recognised by presenting it as a trace id.
        assert_ne!(a.hash("span", "abc"), a.hash("trace", "abc"));
    }

    #[test]
    fn a_path_is_derived_from_a_span_name_only_when_it_looks_like_one() {
        assert_eq!(path_from_name("GET /users/2").as_deref(), Some("/users/2"));
        assert_eq!(path_from_name("/users/2").as_deref(), Some("/users/2"));
        assert_eq!(
            path_from_name("GET /users/2?a=b").as_deref(),
            Some("/users/2")
        );
        assert_eq!(path_from_name("users/2"), None);
        assert_eq!(path_from_name("/"), None);
        assert_eq!(path_from_name("DatabaseMigrate"), None);
    }
}
