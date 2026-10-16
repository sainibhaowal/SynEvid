//! The protocol-agnostic contract-change vocabulary shared by every engine in
//! this crate.
//!
//! OpenAPI (Step 37), AsyncAPI (Step 38), and Pact (Step 39) all need to say
//! "a required property was added" or "this endpoint disappeared", and they all
//! need the same verdict algebra on top. Putting the vocabulary here rather than
//! in one of the engines is what keeps the algebra from being re-derived — and
//! re-derived slightly differently — per protocol.
//!
//! # Two severities, one field
//!
//! [`ChangeKind::default_severity`] is deliberately **not** the last word. A
//! change kind names a *fact* ("a property was removed"); the severity of that
//! fact depends on which side of the wire is talking. Removing a property from a
//! schema a service *accepts* breaks clients that send it; removing it from a
//! schema a service *emits* breaks consumers that read it — but adding a
//! property is safe in the emitted direction and unsafe in the accepted one.
//!
//! So [`ContractChange`] stores an explicit `severity` computed by the engine
//! that knows the direction, and `default_severity` is the fallback for kinds
//! whose severity genuinely does not depend on direction. AsyncAPI's payload
//! kinds default to [`Severity::Unknown`] for exactly this reason: a fixed
//! default would be a guess, and a wrong guess here is a false approval.

use autopsy_domain::CoverageState;
use serde::{Deserialize, Serialize};

/// How a single change affects compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Breaking,
    /// Detected, but this engine has no directional evidence for it. Prevents a
    /// `Compatible` verdict without asserting a break.
    Unknown,
    NonBreaking,
}

/// What kind of contract change was detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    // --- surface -----------------------------------------------------
    EndpointRemoved,
    EndpointAdded,
    MethodRemoved,
    MethodAdded,
    // --- request parameters ------------------------------------------
    RequiredParameterAdded,
    OptionalParameterAdded,
    ParameterRemoved,
    ParameterBecameRequired,
    ParameterBecameOptional,
    ParameterTypeNarrowed,
    ParameterTypeWidened,
    ParameterEnumValueRemoved,
    ParameterEnumValueAdded,
    // --- request bodies ----------------------------------------------
    RequestBodyAdded,
    OptionalRequestBodyAdded,
    RequestBodyBecameRequired,
    RequestBodyBecameOptional,
    RequestBodyRemoved,
    MediaTypeRemoved,
    MediaTypeAdded,
    RequestPropertyRemoved,
    RequiredPropertyAdded,
    OptionalPropertyAdded,
    RequestAdditionalPropertiesDisallowed,
    RequestEnumValueRemoved,
    // --- responses ---------------------------------------------------
    ResponseStatusRemoved,
    ResponseStatusAdded,
    ResponseSchemaChanged,
    // --- disclosure --------------------------------------------------
    /// A subtree could not be modelled completely enough to compare.
    IncomparableSubtree,

    // --- AsyncAPI surface (Step 38) ------------------------------------
    ChannelRemoved,
    ChannelAdded,
    /// The channel's `address` changed. The address is the wire topic, so
    /// nothing that was publishing to or reading from it follows the move.
    ChannelAddressChanged,
    OperationRemoved,
    OperationAdded,
    /// `send` became `receive` or vice versa: the document now says the
    /// application is on the other side of the wire.
    OperationActionChanged,
    OperationChannelChanged,
    MessageRemoved,
    MessageAdded,
    /// The message's `contentType` changed. A payload that used to arrive as
    /// JSON and now arrives as XML is not the same contract at all.
    MessageContentTypeChanged,
    /// The message gained a payload where it had none.
    MessagePayloadAdded,
    /// The message lost the payload it used to declare.
    MessagePayloadRemoved,

    // --- AsyncAPI producer/consumer flows (Step 38) --------------------
    /// A producer's declared payload does not satisfy a consumer's declared
    /// expectations on the same channel. This is a positive finding computed by
    /// construction, so its severity does not depend on coverage.
    ProducerConsumerIncompatible,

    // --- AsyncAPI payload revisions (Step 38) --------------------------
    // These name a *fact* about a schema pair. The severity is computed from
    // the direction the message flows, so the defaults below are `Unknown` on
    // purpose: a fixed default here would be a guess.
    PayloadPropertyRemoved,
    PayloadPropertyAdded,
    PayloadRequirednessGained,
    PayloadRequirednessLost,
    PayloadTypeNarrowed,
    PayloadTypeWidened,
    PayloadEnumValueRemoved,
    PayloadEnumValueAdded,
    PayloadAdditionalPropertiesDisallowed,
    PayloadAdditionalPropertiesAllowed,
    PayloadIncomparable,
}

impl ChangeKind {
    /// Every variant, so a schema-completeness check can be exhaustive rather
    /// than a hand-maintained list that drifts.
    pub const ALL: [ChangeKind; 53] = [
        ChangeKind::EndpointRemoved,
        ChangeKind::EndpointAdded,
        ChangeKind::MethodRemoved,
        ChangeKind::MethodAdded,
        ChangeKind::RequiredParameterAdded,
        ChangeKind::OptionalParameterAdded,
        ChangeKind::ParameterRemoved,
        ChangeKind::ParameterBecameRequired,
        ChangeKind::ParameterBecameOptional,
        ChangeKind::ParameterTypeNarrowed,
        ChangeKind::ParameterTypeWidened,
        ChangeKind::ParameterEnumValueRemoved,
        ChangeKind::ParameterEnumValueAdded,
        ChangeKind::RequestBodyAdded,
        ChangeKind::OptionalRequestBodyAdded,
        ChangeKind::RequestBodyBecameRequired,
        ChangeKind::RequestBodyBecameOptional,
        ChangeKind::RequestBodyRemoved,
        ChangeKind::MediaTypeRemoved,
        ChangeKind::MediaTypeAdded,
        ChangeKind::RequestPropertyRemoved,
        ChangeKind::RequiredPropertyAdded,
        ChangeKind::OptionalPropertyAdded,
        ChangeKind::RequestAdditionalPropertiesDisallowed,
        ChangeKind::RequestEnumValueRemoved,
        ChangeKind::ResponseStatusRemoved,
        ChangeKind::ResponseStatusAdded,
        ChangeKind::ResponseSchemaChanged,
        ChangeKind::IncomparableSubtree,
        ChangeKind::ChannelRemoved,
        ChangeKind::ChannelAdded,
        ChangeKind::ChannelAddressChanged,
        ChangeKind::OperationRemoved,
        ChangeKind::OperationAdded,
        ChangeKind::OperationActionChanged,
        ChangeKind::OperationChannelChanged,
        ChangeKind::MessageRemoved,
        ChangeKind::MessageAdded,
        ChangeKind::MessageContentTypeChanged,
        ChangeKind::MessagePayloadAdded,
        ChangeKind::MessagePayloadRemoved,
        ChangeKind::ProducerConsumerIncompatible,
        ChangeKind::PayloadPropertyRemoved,
        ChangeKind::PayloadPropertyAdded,
        ChangeKind::PayloadRequirednessGained,
        ChangeKind::PayloadRequirednessLost,
        ChangeKind::PayloadTypeNarrowed,
        ChangeKind::PayloadTypeWidened,
        ChangeKind::PayloadEnumValueRemoved,
        ChangeKind::PayloadEnumValueAdded,
        ChangeKind::PayloadAdditionalPropertiesDisallowed,
        ChangeKind::PayloadAdditionalPropertiesAllowed,
        ChangeKind::PayloadIncomparable,
    ];

    /// Severity this kind carries on its own, before direction is considered.
    pub fn default_severity(self) -> Severity {
        use ChangeKind as K;
        use Severity::{Breaking, NonBreaking, Unknown};
        match self {
            // Losing a previously available operation is the definition of
            // breaking: every existing caller of it breaks.
            K::EndpointRemoved | K::MethodRemoved => Breaking,
            K::EndpointAdded | K::MethodAdded => NonBreaking,

            // A new required parameter rejects every existing call, because
            // existing callers do not send it.
            K::RequiredParameterAdded => Breaking,
            K::OptionalParameterAdded => NonBreaking,

            // Removing a parameter removes a documented capability. Some
            // servers ignore unknown parameters, so this might be survivable —
            // but the contract promised it, and this engine does not assume
            // leniency it has not observed.
            K::ParameterRemoved => Breaking,
            K::ParameterBecameRequired => Breaking,
            K::ParameterBecameOptional => NonBreaking,

            // See the module note on direction.
            K::ParameterTypeNarrowed => Breaking,
            K::ParameterTypeWidened => NonBreaking,
            K::ParameterEnumValueRemoved => Breaking,
            K::ParameterEnumValueAdded => NonBreaking,

            // A body that was absent and is now required rejects bodyless
            // requests. A body that was absent and is now *optional* rejects
            // nothing: existing callers still send no body, which remains valid.
            K::RequestBodyAdded | K::RequestBodyBecameRequired => Breaking,
            K::OptionalRequestBodyAdded | K::RequestBodyBecameOptional => NonBreaking,
            K::RequestBodyRemoved => Breaking,

            // Dropping a media type removes a representation callers could
            // request.
            K::MediaTypeRemoved => Breaking,
            K::MediaTypeAdded => NonBreaking,

            // Removing a request property breaks callers that set it. Adding a
            // required property breaks callers that do not.
            K::RequestPropertyRemoved => Breaking,
            K::RequiredPropertyAdded => Breaking,
            K::OptionalPropertyAdded => NonBreaking,
            K::RequestAdditionalPropertiesDisallowed => Breaking,
            K::RequestEnumValueRemoved => Breaking,

            // Clients switch on status codes; a code that disappears has no
            // handling path.
            K::ResponseStatusRemoved => Breaking,
            K::ResponseStatusAdded => NonBreaking,

            // A response schema changed and this engine has no directional
            // evidence for it, so it says so instead of guessing.
            K::ResponseSchemaChanged => Unknown,
            K::IncomparableSubtree => Unknown,

            // --- AsyncAPI surface ------------------------------------------
            // Losing a channel, an operation, or a message removes a surface
            // that something was already publishing to or consuming from.
            K::ChannelRemoved
            | K::ChannelAddressChanged
            | K::OperationRemoved
            | K::OperationActionChanged
            | K::OperationChannelChanged
            | K::MessageRemoved
            | K::MessageContentTypeChanged => Breaking,
            K::ChannelAdded | K::OperationAdded | K::MessageAdded => NonBreaking,

            // Computed by construction: the producer's declared payload does
            // not satisfy the consumer's declared expectations. A positive
            // finding, so it does not depend on coverage.
            K::ProducerConsumerIncompatible => Breaking,

            // --- AsyncAPI payload revisions --------------------------------
            // Direction-dependent. `PayloadDirection::Accepted` and
            // `PayloadDirection::Emitted` assign opposite severities to several
            // of these, so there is no honest fixed default.
            K::PayloadPropertyRemoved
            | K::PayloadPropertyAdded
            | K::PayloadRequirednessGained
            | K::PayloadRequirednessLost
            | K::PayloadTypeNarrowed
            | K::PayloadTypeWidened
            | K::PayloadEnumValueRemoved
            | K::PayloadEnumValueAdded
            | K::PayloadAdditionalPropertiesDisallowed
            | K::PayloadAdditionalPropertiesAllowed
            | K::PayloadIncomparable
            | K::MessagePayloadAdded
            | K::MessagePayloadRemoved => Unknown,
        }
    }
}

/// A single detected change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractChange {
    pub kind: ChangeKind,
    pub severity: Severity,
    /// `OPERATION`-style subject, or a JSON pointer for schema-level changes.
    pub subject: String,
    /// Human-readable statement of what changed.
    pub detail: String,
    pub coverage: CoverageState,
}

/// The overall compatibility verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityVerdict {
    /// No change was found that could break a consumer, over a fully-modelled
    /// document. This is the only verdict that constitutes a claim of safety.
    Compatible,
    Breaking,
    /// A change was detected that this engine cannot classify, or the document
    /// was not modelled completely enough to rule a break out.
    Unknown,
}

impl CompatibilityVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compatible => "compatible",
            Self::Breaking => "breaking",
            Self::Unknown => "unknown",
        }
    }

    /// True when a merge gate may treat this as a pass.
    ///
    /// Provided so that no consumer writes `verdict != Breaking` by hand —
    /// which would silently promote `Unknown` to an approval.
    pub fn is_approval(self) -> bool {
        self == Self::Compatible
    }
}

/// The result of comparing two OpenAPI contracts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractDelta {
    pub protocol: String,
    pub from_path: String,
    pub to_path: String,
    pub from_digest: String,
    pub to_digest: String,
    pub version_change: Option<VersionChange>,
    pub changes: Vec<ContractChange>,
    pub verdict: CompatibilityVerdict,
    /// Worst-case coverage across both documents.
    pub coverage: CoverageState,
    /// Reasons the verdict is not `Compatible`, sorted.
    pub notes: Vec<String>,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionChange {
    pub from: String,
    pub to: String,
}

impl ContractDelta {
    /// Changes that a consumer must act on, in canonical order.
    pub fn breaking(&self) -> Vec<&ContractChange> {
        self.changes
            .iter()
            .filter(|c| c.severity == Severity::Breaking)
            .collect()
    }

    /// Changes that were detected but not classified.
    pub fn unclassified(&self) -> Vec<&ContractChange> {
        self.changes
            .iter()
            .filter(|c| c.severity == Severity::Unknown)
            .collect()
    }
}
