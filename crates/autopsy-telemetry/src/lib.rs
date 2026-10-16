//! # autopsy-telemetry — runtime-static evidence fusion (Pillar 4, Steps 40–41)
//!
//! Ingests sanitized OpenTelemetry OTLP/JSON traces and turns them into evidence
//! that can be joined to the static graph — without ever letting a secret, an
//! identifier or a payload reach a digest.
//!
//! ## Pipeline
//!
//! ```text
//!   OTLP/JSON bytes
//!        │  otlp::parse_otlp_traces        lossless; nothing is discarded
//!        ▼
//!   otlp::TraceRequest
//!        │  span::sanitize_traces         redact, then hash
//!        ▼
//!   span::TraceRecord                      SpanRecord + OccupancyKey
//!        │  topology::correlate_topology  matches against a supplied surface
//!        ▼
//!   topology::TopologyReport               placed spans, dead code, uninvoked and
//!                                          undocumented routes, withheld findings
//! ```
//!
//! The static side is an **input** ([`topology::DeclaredSurface`]), never parsed here:
//! two definitions of "what routes exist" in two places is how they come to disagree.
//!
//! ## The three rules this crate exists to enforce
//!
//! 1. **Redaction precedes hashing.** A digest over raw input is a fingerprint of
//!    the secret it contained. See [`redact`].
//! 2. **A gap is stated, never smoothed over.** Free text, an unmodelled value, an
//!    identifier the export cannot place, and a span the SDK dropped data from all
//!    degrade coverage (FR-014, FR-030).
//! 3. **Pseudonymisation needs a key.** Span and trace ids are hashed with keyed
//!    BLAKE3 under a caller-supplied salt, because an unsalted hash of a 64-bit id
//!    is invertible by exhaustive search. See [`span`].
//!
//! ## Boundaries
//!
//! * `INV-SANDBOX-001..004` — no sub-process, no socket, no filesystem write. This
//!   crate reads bytes it is handed and returns a value.
//! * `INV-CROSS-001` — no timestamp reaches an identifier or a correlation key;
//!   path handling is string-level and separator-free of platform assumptions.
//! * No dependency on a language adapter or on a contract engine. Correlating a
//!   route against a declared endpoint takes the declared routes as input rather
//!   than reaching across the DAG for a parser (Step 41).

pub mod otlp;
pub mod redact;
pub mod span;
pub mod topology;

pub use otlp::{
    AnyValue, KeyValue, MODELLED_KEYS, OtlpError, Resource, ResourceSpans, SEMCONV_VERSION, Scope,
    ScopeSpans, Span, SpanEvent, SpanKind, SpanLink, SpanStatus, StatusCode, TraceRequest,
    parse_otlp_traces,
};
pub use redact::{
    RedactionKind, Redactor, SanitizedEntry, SanitizedValue, UrlSurgery, key_rule, strip_url,
};
pub use span::{
    EventRecord, IngestError, IngestOptions, LinkRecord, OccupancyKey, OccupancySubject,
    SpanRecord, StatusRecord, TraceRecord, sanitize_traces,
};
pub use topology::{
    CorrelationBasis, DeadCodeFinding, DeclaredRoute, DeclaredSurface, DeclaredSymbol,
    DispatchModel, EvidenceSubject, InsufficientEvidence, InsufficientReason, ObservationSummary,
    SpanCorrelation, TopologyOptions, TopologyReport, UndocumentedRoute, UninvokedRoute,
    canonical_route_template, correlate_topology,
};
