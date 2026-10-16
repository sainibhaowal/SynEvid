//! OTLP/JSON trace ingestion (Step 40).
//!
//! Reads the `ExportTraceServiceRequest` shape an OTLP/HTTP collector receives on
//! `/v1/traces`, into a lossless model this crate can then sanitize.
//!
//! # Why the reader is lossless and the sanitizer is separate
//!
//! The two jobs have opposite failure modes, so they are kept apart. A reader
//! that quietly dropped the shapes it did not understand would make the sanitizer
//! look complete when it had simply never seen the data — and the sanitizer is
//! the component whose silence is dangerous. So this module preserves **every**
//! value it can represent, keeps the unrepresentable ones as explicit
//! [`AnyValue::Unmodelled`] nodes naming the variants it found, and records a
//! pointer for each. Nothing is discarded here.
//!
//! # `intValue` is a string, and that is not a typo
//!
//! OTLP/JSON writes the 64-bit integer variants as **strings** (`"intValue":
//! "9223372036854775807"`), because JSON numbers are IEEE-754 doubles and a
//! JavaScript collector would silently round them. A reader that only accepts
//! JSON numbers therefore reads every integer attribute as absent. Both spellings
//! are accepted, and the value is canonicalised to decimal text so `"007"` and
//! `7` cannot produce different digests.
//!
//! # `doubleValue` is kept as literal text
//!
//! The other numeric variant *is* a JSON number. Rendering it through `f64` and
//! back is a determinism hazard across platforms and formatting styles, so the
//! literal that `serde_json` produced is kept. `1.0` and `1.00` parse to the same
//! `Number` and render identically, which is exactly the property `NFR-001`
//! needs. Keeping the text also means `Eq` is derivable, which the sanitized model
//! depends on.

use autopsy_domain::CoverageState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use thiserror::Error;

/// Semantic Conventions version this engine reads attribute keys against.
///
/// `R24`. Note that OTLP/JSON carries **no** version of its own: the version in
/// play is a property of the exporter, not of the payload. The engine therefore
/// cannot verify it and says so, rather than claiming a version it did not see.
pub const SEMCONV_VERSION: &str = "1.44.0";

/// Attribute keys this engine reads as correlation surface, per §4.1 of the
/// Pillar 4 design.
pub const MODELLED_KEYS: [&str; 9] = [
    "code.function",
    "code.filepath",
    "http.route",
    "http.request.method",
    "http.response.status_code",
    "rpc.service",
    "rpc.method",
    "url.full",
    "url.path",
];

/// One OTLP `AnyValue`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnyValue {
    Str(String),
    Bool(bool),
    Int(i64),
    /// The JSON literal, kept verbatim (see the module docs).
    Double(String),
    /// The base64 text, as written.
    Bytes(String),
    Array(Vec<AnyValue>),
    KvList(Vec<KeyValue>),
    /// The node declared no variant, or more than one. The variant names present
    /// are recorded, so a disclosure can name what was found instead of saying
    /// "malformed".
    Unmodelled(BTreeSet<String>),
}

/// One OTLP `KeyValue`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyValue {
    pub key: String,
    pub value: AnyValue,
}

/// OTLP `SpanKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanKind {
    Unspecified,
    Internal,
    Server,
    Client,
    Producer,
    Consumer,
}

impl SpanKind {
    /// The OTLP enum value. Anything outside 0–5 is rejected rather than
    /// defaulted, because defaulting would place an unknown span in the
    /// `Unspecified` bucket and Step 41 would then draw topology from it.
    pub fn from_otlp(raw: i64) -> Option<Self> {
        match raw {
            0 => Some(Self::Unspecified),
            1 => Some(Self::Internal),
            2 => Some(Self::Server),
            3 => Some(Self::Client),
            4 => Some(Self::Producer),
            5 => Some(Self::Consumer),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unspecified => "unspecified",
            Self::Internal => "internal",
            Self::Server => "server",
            Self::Client => "client",
            Self::Producer => "producer",
            Self::Consumer => "consumer",
        }
    }
}

/// OTLP `StatusCode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusCode {
    Unset,
    Ok,
    Error,
}

impl StatusCode {
    pub fn from_otlp(raw: i64) -> Option<Self> {
        match raw {
            0 => Some(Self::Unset),
            1 => Some(Self::Ok),
            2 => Some(Self::Error),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unset => "unset",
            Self::Ok => "ok",
            Self::Error => "error",
        }
    }
}

/// OTLP `Status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpanStatus {
    pub code: StatusCode,
    /// Free text. The specification explicitly says this "MAY contain
    /// sensitive information", so it is flagged rather than trusted.
    pub message: Option<String>,
}

/// OTLP `Event`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpanEvent {
    pub name: String,
    pub time_unix_nano: u64,
    pub attributes: Vec<KeyValue>,
    pub dropped_attributes_count: u64,
}

/// OTLP `Link`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpanLink {
    pub trace_id: String,
    pub span_id: String,
    pub attributes: Vec<KeyValue>,
    pub dropped_attributes_count: u64,
}

/// OTLP `Span`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub name: String,
    pub kind: SpanKind,
    pub start_unix_nano: u64,
    pub end_unix_nano: u64,
    pub attributes: Vec<KeyValue>,
    pub events: Vec<SpanEvent>,
    pub links: Vec<SpanLink>,
    pub status: Option<SpanStatus>,
    pub dropped_attributes_count: u64,
    pub dropped_events_count: u64,
    pub dropped_links_count: u64,
}

/// OTLP `InstrumentationScope`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub name: Option<String>,
    pub version: Option<String>,
}

/// OTLP `ScopeSpans`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeSpans {
    pub scope: Option<Scope>,
    pub spans: Vec<Span>,
}

/// OTLP `Resource`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resource {
    pub attributes: Vec<KeyValue>,
}

/// OTLP `ResourceSpans`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceSpans {
    pub resource: Option<Resource>,
    pub scope_spans: Vec<ScopeSpans>,
}

/// A parsed `ExportTraceServiceRequest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceRequest {
    pub document_path: String,
    pub resource_spans: Vec<ResourceSpans>,
    pub coverage: CoverageState,
    /// Pointers to nodes this reader could not represent faithfully.
    pub unmodelled_pointers: BTreeSet<String>,
    pub notes: Vec<String>,
}

impl TraceRequest {
    /// Every span in document order.
    pub fn spans(&self) -> Vec<&Span> {
        self.resource_spans
            .iter()
            .flat_map(|rs| rs.scope_spans.iter())
            .flat_map(|ss| ss.spans.iter())
            .collect()
    }

    /// The `service.name` attributes declared at resource level.
    pub fn service_names(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for resource_spans in &self.resource_spans {
            let Some(resource) = &resource_spans.resource else {
                continue;
            };
            for attribute in &resource.attributes {
                if attribute.key == "service.name"
                    && let AnyValue::Str(name) = &attribute.value
                {
                    out.insert(name.clone());
                }
            }
        }
        out
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OtlpError {
    #[error("`{path}` is not valid JSON: {detail}")]
    NotJson { path: String, detail: String },
    #[error("`{path}`: `{pointer}` is {found}, expected {expected}")]
    WrongShape {
        path: String,
        pointer: String,
        found: &'static str,
        expected: &'static str,
    },
    #[error("`{path}`: `{pointer}` is `{value}`, which is not a valid {expected}")]
    BadValue {
        path: String,
        pointer: String,
        value: String,
        expected: &'static str,
    },
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

/// The top-level `AnyValue` variant names, in the order the specification lists
/// them. Used to name what a malformed node actually contained.
const ANY_VALUE_VARIANTS: [&str; 7] = [
    "stringValue",
    "boolValue",
    "intValue",
    "doubleValue",
    "arrayValue",
    "kvlistValue",
    "bytesValue",
];

fn read_u64(
    value: &Value,
    path: &str,
    pointer: &str,
    expected: &'static str,
) -> Result<u64, OtlpError> {
    // OTLP/JSON writes 64-bit values as strings, either spelling is accepted.
    let parsed = match value {
        Value::String(text) => text.parse::<u64>().ok(),
        Value::Number(number) => number.as_u64(),
        _ => None,
    };
    parsed.ok_or_else(|| OtlpError::BadValue {
        path: path.to_string(),
        pointer: pointer.to_string(),
        value: value.to_string(),
        expected,
    })
}

fn read_i64(
    value: &Value,
    path: &str,
    pointer: &str,
    expected: &'static str,
) -> Result<i64, OtlpError> {
    let parsed = match value {
        Value::String(text) => text.parse::<i64>().ok(),
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|f| f as i64)),
        _ => None,
    };
    parsed.ok_or_else(|| OtlpError::BadValue {
        path: path.to_string(),
        pointer: pointer.to_string(),
        value: value.to_string(),
        expected,
    })
}

fn read_string(
    value: &Value,
    path: &str,
    pointer: &str,
    expected: &'static str,
) -> Result<String, OtlpError> {
    value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| OtlpError::WrongShape {
            path: path.to_string(),
            pointer: pointer.to_string(),
            found: type_name(value),
            expected,
        })
}

fn read_optional_string(
    value: Option<&Value>,
    path: &str,
    pointer: &str,
) -> Result<Option<String>, OtlpError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(other) => Err(OtlpError::WrongShape {
            path: path.to_string(),
            pointer: pointer.to_string(),
            found: type_name(other),
            expected: "a string",
        }),
    }
}

fn read_u64_field(node: &Value, key: &str, path: &str, pointer: &str) -> Result<u64, OtlpError> {
    match node.get(key) {
        None | Some(Value::Null) => Ok(0),
        Some(value) => read_u64(
            value,
            path,
            &format!("{pointer}/{key}"),
            "a 64-bit unsigned integer",
        ),
    }
}

fn read_any_value(
    value: &Value,
    path: &str,
    pointer: &str,
    unmodelled: &mut BTreeSet<String>,
    notes: &mut Vec<String>,
) -> Result<AnyValue, OtlpError> {
    let Some(object) = value.as_object() else {
        return Err(OtlpError::WrongShape {
            path: path.to_string(),
            pointer: pointer.to_string(),
            found: type_name(value),
            expected: "an AnyValue object",
        });
    };

    let present: BTreeSet<&str> = ANY_VALUE_VARIANTS
        .iter()
        .copied()
        .filter(|variant| object.get(*variant).is_some_and(|v| !v.is_null()))
        .collect();

    // Exactly one variant is required. Zero or several is a structural defect,
    // and *which* it is matters: several means the producer is using a newer
    // oneof member this engine does not know, zero means the value never made it
    // out of the exporter. Both are recorded by name rather than collapsed into
    // "malformed".
    if present.len() != 1 {
        unmodelled.insert(pointer.to_string());
        notes.push(format!(
            "`{pointer}` declares {} AnyValue variant(s){}; exactly one is required",
            present.len(),
            if present.is_empty() {
                String::new()
            } else {
                format!(
                    " ({})",
                    present.iter().copied().collect::<Vec<_>>().join(", ")
                )
            }
        ));
        let names: BTreeSet<String> = if present.is_empty() {
            object.keys().cloned().collect()
        } else {
            present.iter().map(|v| (*v).to_string()).collect()
        };
        return Ok(AnyValue::Unmodelled(names));
    }
    let variant = present.iter().next().copied().unwrap_or_default();
    let inner = object.get(variant).unwrap_or(&Value::Null);
    let inner_pointer = format!("{pointer}/{variant}");

    Ok(match variant {
        "stringValue" => AnyValue::Str(read_string(inner, path, &inner_pointer, "a string")?),
        "boolValue" => {
            inner
                .as_bool()
                .map(AnyValue::Bool)
                .ok_or_else(|| OtlpError::WrongShape {
                    path: path.to_string(),
                    pointer: inner_pointer.clone(),
                    found: type_name(inner),
                    expected: "a boolean",
                })?
        }
        "intValue" => AnyValue::Int(read_i64(
            inner,
            path,
            &inner_pointer,
            "a 64-bit signed integer",
        )?),
        "doubleValue" => {
            let number = inner.as_f64().ok_or_else(|| OtlpError::WrongShape {
                path: path.to_string(),
                pointer: inner_pointer.clone(),
                found: type_name(inner),
                expected: "a number",
            })?;
            if !number.is_finite() {
                // `NaN` and the infinities have no JSON spelling. Reaching here
                // means the input was not JSON, so it cannot be canonicalised.
                return Err(OtlpError::BadValue {
                    path: path.to_string(),
                    pointer: inner_pointer,
                    value: inner.to_string(),
                    expected: "a finite number",
                });
            }
            // `serde_json` renders the number it parsed, so `1.0` and `1.00`
            // agree and the text does not depend on the input's formatting.
            AnyValue::Double(inner.to_string())
        }
        "bytesValue" => {
            AnyValue::Bytes(read_string(inner, path, &inner_pointer, "a base64 string")?)
        }
        "arrayValue" => {
            let Some(object) = inner.as_object() else {
                return Err(OtlpError::WrongShape {
                    path: path.to_string(),
                    pointer: inner_pointer,
                    found: type_name(inner),
                    expected: "an ArrayValue object",
                });
            };
            let values = read_array_value(
                object.get("values"),
                path,
                &inner_pointer,
                unmodelled,
                notes,
            )?;
            AnyValue::Array(values)
        }
        "kvlistValue" => {
            let Some(object) = inner.as_object() else {
                return Err(OtlpError::WrongShape {
                    path: path.to_string(),
                    pointer: inner_pointer,
                    found: type_name(inner),
                    expected: "a KeyValueList object",
                });
            };
            let values = read_key_values(
                object.get("values"),
                path,
                &inner_pointer,
                unmodelled,
                notes,
            )?;
            AnyValue::KvList(values)
        }
        other => {
            // Unreachable while `ANY_VALUE_VARIANTS` and this match agree. If a
            // variant is added to the list and not here, this is the branch that
            // fires — and it *records* the node rather than panicking, because an
            // engine that aborts on an unexpected contract is worse than one that
            // reports it.
            unmodelled.insert(inner_pointer.clone());
            notes.push(format!(
                "`{inner_pointer}` uses `{other}`, which is not modelled"
            ));
            AnyValue::Unmodelled(BTreeSet::from([other.to_string()]))
        }
    })
}

fn read_array_value(
    node: Option<&Value>,
    path: &str,
    pointer: &str,
    unmodelled: &mut BTreeSet<String>,
    notes: &mut Vec<String>,
) -> Result<Vec<AnyValue>, OtlpError> {
    let Some(node) = node else {
        return Ok(Vec::new());
    };
    let Some(items) = node.as_array() else {
        return Err(OtlpError::WrongShape {
            path: path.to_string(),
            pointer: format!("{pointer}/values"),
            found: type_name(node),
            expected: "an array of AnyValue",
        });
    };
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        out.push(read_any_value(
            item,
            path,
            &format!("{pointer}/values/{index}"),
            unmodelled,
            notes,
        )?);
    }
    Ok(out)
}

fn read_key_values(
    node: Option<&Value>,
    path: &str,
    pointer: &str,
    unmodelled: &mut BTreeSet<String>,
    notes: &mut Vec<String>,
) -> Result<Vec<KeyValue>, OtlpError> {
    let Some(node) = node else {
        return Ok(Vec::new());
    };
    let Some(items) = node.as_array() else {
        return Err(OtlpError::WrongShape {
            path: path.to_string(),
            pointer: format!("{pointer}/values"),
            found: type_name(node),
            expected: "an array of KeyValue",
        });
    };
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let item_pointer = format!("{pointer}/values/{index}");
        let key = item
            .get("key")
            .ok_or_else(|| OtlpError::WrongShape {
                path: path.to_string(),
                pointer: format!("{item_pointer}/key"),
                found: "absent",
                expected: "a key",
            })
            .and_then(|k| read_string(k, path, &format!("{item_pointer}/key"), "a string"))?;
        let value = match item.get("value") {
            Some(value) => read_any_value(
                value,
                path,
                &format!("{item_pointer}/value"),
                unmodelled,
                notes,
            )?,
            None => {
                // A `KeyValue` without a `value` is an attribute whose payload the
                // exporter never wrote. Recording it as unmodelled keeps the fact
                // visible; dropping the key entirely would hide that an attribute
                // by that name existed at all.
                unmodelled.insert(format!("{item_pointer}/value"));
                notes.push(format!(
                    "`{item_pointer}` declares key `{key}` with no `value`"
                ));
                AnyValue::Unmodelled(BTreeSet::new())
            }
        };
        out.push(KeyValue { key, value });
    }
    Ok(out)
}

fn read_attributes(
    node: Option<&Value>,
    path: &str,
    pointer: &str,
    unmodelled: &mut BTreeSet<String>,
    notes: &mut Vec<String>,
) -> Result<Vec<KeyValue>, OtlpError> {
    read_key_values(node, path, pointer, unmodelled, notes)
}

fn read_span(
    node: &Value,
    path: &str,
    pointer: &str,
    unmodelled: &mut BTreeSet<String>,
    notes: &mut Vec<String>,
) -> Result<Span, OtlpError> {
    let required = |key: &str| -> Result<String, OtlpError> {
        node.get(key)
            .ok_or_else(|| OtlpError::WrongShape {
                path: path.to_string(),
                pointer: format!("{pointer}/{key}"),
                found: "absent",
                expected: "a hex identifier",
            })
            .and_then(|v| read_string(v, path, &format!("{pointer}/{key}"), "a hex identifier"))
    };

    let kind_raw = match node.get("kind") {
        None | Some(Value::Null) => 0,
        Some(value) => read_i64(value, path, &format!("{pointer}/kind"), "a span kind")?,
    };
    let Some(kind) = SpanKind::from_otlp(kind_raw) else {
        return Err(OtlpError::BadValue {
            path: path.to_string(),
            pointer: format!("{pointer}/kind"),
            value: kind_raw.to_string(),
            expected: "span kind 0 to 5",
        });
    };

    let status = match node.get("status") {
        None | Some(Value::Null) => None,
        Some(value) => {
            let code_raw = match value.get("code") {
                None | Some(Value::Null) => 0,
                Some(code) => read_i64(
                    code,
                    path,
                    &format!("{pointer}/status/code"),
                    "a status code",
                )?,
            };
            let Some(code) = StatusCode::from_otlp(code_raw) else {
                return Err(OtlpError::BadValue {
                    path: path.to_string(),
                    pointer: format!("{pointer}/status/code"),
                    value: code_raw.to_string(),
                    expected: "status code 0 to 2",
                });
            };
            Some(SpanStatus {
                code,
                message: read_optional_string(
                    value.get("message"),
                    path,
                    &format!("{pointer}/status/message"),
                )?,
            })
        }
    };

    let mut events = Vec::new();
    if let Some(value) = node.get("events") {
        let Some(items) = value.as_array() else {
            return Err(OtlpError::WrongShape {
                path: path.to_string(),
                pointer: format!("{pointer}/events"),
                found: type_name(value),
                expected: "an array of events",
            });
        };
        for (index, item) in items.iter().enumerate() {
            let event_pointer = format!("{pointer}/events/{index}");
            events.push(SpanEvent {
                name: item
                    .get("name")
                    .map(|n| read_string(n, path, &format!("{event_pointer}/name"), "a string"))
                    .transpose()?
                    .unwrap_or_default(),
                time_unix_nano: read_u64_field(item, "timeUnixNano", path, &event_pointer)?,
                attributes: read_attributes(
                    item.get("attributes"),
                    path,
                    &format!("{event_pointer}/attributes"),
                    unmodelled,
                    notes,
                )?,
                dropped_attributes_count: read_u64_field(
                    item,
                    "droppedAttributesCount",
                    path,
                    &event_pointer,
                )?,
            });
        }
    }

    let mut links = Vec::new();
    if let Some(value) = node.get("links") {
        let Some(items) = value.as_array() else {
            return Err(OtlpError::WrongShape {
                path: path.to_string(),
                pointer: format!("{pointer}/links"),
                found: type_name(value),
                expected: "an array of links",
            });
        };
        for (index, item) in items.iter().enumerate() {
            let link_pointer = format!("{pointer}/links/{index}");
            links.push(SpanLink {
                trace_id: item
                    .get("traceId")
                    .map(|v| {
                        read_string(v, path, &format!("{link_pointer}/traceId"), "a hex string")
                    })
                    .transpose()?
                    .unwrap_or_default(),
                span_id: item
                    .get("spanId")
                    .map(|v| {
                        read_string(v, path, &format!("{link_pointer}/spanId"), "a hex string")
                    })
                    .transpose()?
                    .unwrap_or_default(),
                attributes: read_attributes(
                    item.get("attributes"),
                    path,
                    &format!("{link_pointer}/attributes"),
                    unmodelled,
                    notes,
                )?,
                dropped_attributes_count: read_u64_field(
                    item,
                    "droppedAttributesCount",
                    path,
                    &link_pointer,
                )?,
            });
        }
    }

    let span = Span {
        trace_id: required("traceId")?,
        span_id: required("spanId")?,
        parent_span_id: read_optional_string(
            node.get("parentSpanId"),
            path,
            &format!("{pointer}/parentSpanId"),
        )?
        .filter(|id| !id.is_empty()),
        name: required("name")?,
        kind,
        start_unix_nano: read_u64_field(node, "startTimeUnixNano", path, pointer)?,
        end_unix_nano: read_u64_field(node, "endTimeUnixNano", path, pointer)?,
        attributes: read_attributes(
            node.get("attributes"),
            path,
            &format!("{pointer}/attributes"),
            unmodelled,
            notes,
        )?,
        events,
        links,
        status,
        dropped_attributes_count: read_u64_field(node, "droppedAttributesCount", path, pointer)?,
        dropped_events_count: read_u64_field(node, "droppedEventsCount", path, pointer)?,
        dropped_links_count: read_u64_field(node, "droppedLinksCount", path, pointer)?,
    };

    if span.end_unix_nano < span.start_unix_nano {
        notes.push(format!(
            "`{pointer}` ends before it starts ({} < {}); the duration is not usable",
            span.end_unix_nano, span.start_unix_nano
        ));
    }
    Ok(span)
}

/// Parses an OTLP/JSON `ExportTraceServiceRequest`.
pub fn parse_otlp_traces(document_path: &str, text: &str) -> Result<TraceRequest, OtlpError> {
    let root: Value = serde_json::from_str(text).map_err(|error| OtlpError::NotJson {
        path: document_path.to_string(),
        detail: error.to_string(),
    })?;

    let mut unmodelled: BTreeSet<String> = BTreeSet::new();
    let mut notes: Vec<String> = Vec::new();

    let resource_entries = match root.get("resourceSpans") {
        Some(Value::Array(items)) => items.clone(),
        Some(other) => {
            return Err(OtlpError::WrongShape {
                path: document_path.to_string(),
                pointer: "/resourceSpans".to_string(),
                found: type_name(other),
                expected: "an array of ResourceSpans",
            });
        }
        None => {
            // Not an error: an empty export is a legitimate thing for a collector
            // to send. It is reported as such rather than as a clean ingest.
            notes.push(
                "the request declares no `resourceSpans`, so it contains no traces to sanitize"
                    .to_string(),
            );
            Vec::new()
        }
    };

    let mut resource_spans = Vec::with_capacity(resource_entries.len());
    for (rs_index, rs_node) in resource_entries.iter().enumerate() {
        let rs_pointer = format!("/resourceSpans/{rs_index}");
        let resource = match rs_node.get("resource") {
            None | Some(Value::Null) => None,
            Some(value) => Some(Resource {
                attributes: read_attributes(
                    value.get("attributes"),
                    document_path,
                    &format!("{rs_pointer}/resource/attributes"),
                    &mut unmodelled,
                    &mut notes,
                )?,
            }),
        };

        let mut scope_spans = Vec::new();
        if let Some(value) = rs_node.get("scopeSpans") {
            let Some(items) = value.as_array() else {
                return Err(OtlpError::WrongShape {
                    path: document_path.to_string(),
                    pointer: format!("{rs_pointer}/scopeSpans"),
                    found: type_name(value),
                    expected: "an array of ScopeSpans",
                });
            };
            for (ss_index, ss_node) in items.iter().enumerate() {
                let ss_pointer = format!("{rs_pointer}/scopeSpans/{ss_index}");
                let scope = match ss_node.get("scope") {
                    None | Some(Value::Null) => None,
                    Some(value) => Some(Scope {
                        name: read_optional_string(
                            value.get("name"),
                            document_path,
                            &format!("{ss_pointer}/scope/name"),
                        )?,
                        version: read_optional_string(
                            value.get("version"),
                            document_path,
                            &format!("{ss_pointer}/scope/version"),
                        )?,
                    }),
                };
                let mut spans = Vec::new();
                if let Some(value) = ss_node.get("spans") {
                    let Some(items) = value.as_array() else {
                        return Err(OtlpError::WrongShape {
                            path: document_path.to_string(),
                            pointer: format!("{ss_pointer}/spans"),
                            found: type_name(value),
                            expected: "an array of spans",
                        });
                    };
                    for (span_index, span_node) in items.iter().enumerate() {
                        spans.push(read_span(
                            span_node,
                            document_path,
                            &format!("{ss_pointer}/spans/{span_index}"),
                            &mut unmodelled,
                            &mut notes,
                        )?);
                    }
                }
                scope_spans.push(ScopeSpans { scope, spans });
            }
        }
        resource_spans.push(ResourceSpans {
            resource,
            scope_spans,
        });
    }

    let span_total: usize = resource_spans
        .iter()
        .flat_map(|rs| rs.scope_spans.iter())
        .map(|ss| ss.spans.len())
        .sum();
    if span_total == 0 {
        notes.push(
            "the request declares no spans, so there is no runtime behaviour to sanitize"
                .to_string(),
        );
    }

    let coverage = if unmodelled.is_empty() && notes.is_empty() {
        CoverageState::Verified
    } else {
        CoverageState::Partial
    };
    notes.sort();
    notes.dedup();

    Ok(TraceRequest {
        document_path: document_path.to_string(),
        resource_spans,
        coverage,
        unmodelled_pointers: unmodelled,
        notes,
    })
}
