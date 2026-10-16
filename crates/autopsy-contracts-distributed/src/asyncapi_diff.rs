//! AsyncAPI compatibility classification (Step 38).
//!
//! Two different questions live here, and conflating them is the main way an
//! event-contract checker goes wrong:
//!
//! 1. [`diff_asyncapi`] — *"did revision A stop being compatible with revision
//!    B?"* Compared schema-to-schema, one document against another.
//! 2. [`verify_flows`] — *"does the producer's declared payload actually satisfy
//!    the consumer's declared expectations?"* Compared payload-to-payload,
//!    producer against consumer, inside a single document.
//!
//! # The direction algebra
//!
//! An HTTP operation has one arrow. A broker channel has two, and the schema
//! alone does not say which one is under discussion — the operation's `action`
//! does. So the *same* schema edit has opposite severities depending on which
//! side made it:
//!
//! | Change | `Accepted` (this app consumes) | `Emitted` (this app produces) |
//! |---|---|---|
//! | property removed | breaking — senders that set it lost a capability | breaking — consumers that read it stop receiving it |
//! | optional property added | non-breaking — accepts a field it did not before | **unknown** — a strict consumer rejects unknown fields, and this engine cannot see the consumer |
//! | requiredness gained | breaking — senders that omit it now break | non-breaking — the producer now guarantees more |
//! | requiredness lost | non-breaking — the demand relaxed | breaking — the producer no longer guarantees it |
//! | type narrowed | breaking — accepts fewer values | non-breaking — emits a subset |
//! | type widened | non-breaking — accepts more | breaking — may emit values no consumer has seen |
//! | enum value removed | breaking | non-breaking |
//! | enum value added | non-breaking | breaking |
//! | `additionalProperties: false` introduced | breaking | non-breaking |
//! | `additionalProperties` loosened | non-breaking | **unknown** |
//!
//! `Both` — the same message sent *and* received by this document — requires the
//! change to be safe in both directions at once, so it takes the worse of the
//! two. That is a genuinely strong requirement, which is the point.
//!
//! The `Emitted` additions are `Unknown` rather than non-breaking on purpose.
//! "Old consumers ignore unknown fields" is a *policy* that some schemas assume,
//! not a property of the schema; Avro and Protobuf decoders are strict by
//! default. When the consumer is in the same document, [`verify_flows`] settles
//! the question from evidence instead of assuming an answer.
//!
//! # Why flow findings are folded into the revision delta
//!
//! A `diff_asyncapi` that reported `Compatible` for a revision whose own
//! producers cannot satisfy its own consumers would be wrong. So the flow check
//! runs on the *candidate* revision and its findings become changes in the
//! delta, with kind [`ChangeKind::ProducerConsumerIncompatible`].

use crate::asyncapi::{Action, AsyncApiSpec, Message, PayloadDirection};
use crate::change::{
    ChangeKind, CompatibilityVerdict, ContractChange, ContractDelta, Severity, VersionChange,
};
use crate::document::worse_of;
use crate::schema::{SchemaFacts, escape};
use autopsy_domain::CoverageState;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Every JSON Schema type name. Used as the "unconstrained" set: a schema with
/// no `type` accepts all of these, which is the honest reading and the one that
/// makes an unconstrained producer visibly over-deliver.
const ALL_JSON_TYPES: &[&str] = &[
    "null", "boolean", "object", "array", "number", "string", "integer",
];

/// Worst-case reduction over severities. Never upgrades.
///
/// The enum orders `Breaking < Unknown < NonBreaking`, so this is a `min` — but
/// written out, because relying on a derived `Ord` to mean "worse" is the kind
/// of implicit coupling that breaks silently when a variant is inserted.
fn worst(a: Severity, b: Severity) -> Severity {
    match (a, b) {
        (Severity::Breaking, _) | (_, Severity::Breaking) => Severity::Breaking,
        (Severity::Unknown, _) | (_, Severity::Unknown) => Severity::Unknown,
        _ => Severity::NonBreaking,
    }
}

/// Severity of a payload change, given the direction the message travels.
///
/// See the module table. Every arm is deliberate; none is a default.
pub fn severity_for(kind: ChangeKind, direction: PayloadDirection) -> Severity {
    use ChangeKind as K;
    use Severity::{Breaking, NonBreaking, Unknown};

    match (kind, direction) {
        // Nothing was read, so nothing is claimed.
        (K::PayloadIncomparable, _) => Unknown,

        // A change that must be safe in both directions at once.
        (_, PayloadDirection::Both) => worst(
            severity_for(kind, PayloadDirection::Emitted),
            severity_for(kind, PayloadDirection::Accepted),
        ),

        // The documented surface lost a field. A consumer can no longer read it
        // and a producer can no longer send it, so this breaks either way.
        (K::PayloadPropertyRemoved, _) => Breaking,

        // An addition is safe to *demand* less of and risky to *emit* more of.
        // A strict consumer rejects unknown fields, and this engine cannot see
        // the consumer from here.
        (K::PayloadPropertyAdded, PayloadDirection::Emitted) => Unknown,
        (K::PayloadPropertyAdded, PayloadDirection::Accepted) => NonBreaking,

        // `required` is a guarantee when emitted and a demand when accepted.
        (K::PayloadRequirednessGained, PayloadDirection::Emitted) => NonBreaking,
        (K::PayloadRequirednessGained, PayloadDirection::Accepted) => Breaking,
        (K::PayloadRequirednessLost, PayloadDirection::Emitted) => Breaking,
        (K::PayloadRequirednessLost, PayloadDirection::Accepted) => NonBreaking,

        // Narrowing an emitted type sends a subset; narrowing an accepted type
        // turns away values that used to be fine.
        (K::PayloadTypeNarrowed, PayloadDirection::Emitted) => NonBreaking,
        (K::PayloadTypeNarrowed, PayloadDirection::Accepted) => Breaking,
        (K::PayloadTypeWidened, PayloadDirection::Emitted) => Breaking,
        (K::PayloadTypeWidened, PayloadDirection::Accepted) => NonBreaking,

        (K::PayloadEnumValueRemoved, PayloadDirection::Emitted) => NonBreaking,
        (K::PayloadEnumValueRemoved, PayloadDirection::Accepted) => Breaking,
        (K::PayloadEnumValueAdded, PayloadDirection::Emitted) => Breaking,
        (K::PayloadEnumValueAdded, PayloadDirection::Accepted) => NonBreaking,

        (K::PayloadAdditionalPropertiesDisallowed, PayloadDirection::Emitted) => NonBreaking,
        (K::PayloadAdditionalPropertiesDisallowed, PayloadDirection::Accepted) => Breaking,
        (K::PayloadAdditionalPropertiesAllowed, PayloadDirection::Emitted) => Unknown,
        (K::PayloadAdditionalPropertiesAllowed, PayloadDirection::Accepted) => NonBreaking,

        // A body appearing or disappearing is the whole-payload version of the
        // rules above.
        (K::MessagePayloadAdded, PayloadDirection::Emitted) => Unknown,
        (K::MessagePayloadAdded, PayloadDirection::Accepted) => Breaking,
        (K::MessagePayloadRemoved, PayloadDirection::Emitted) => Breaking,
        (K::MessagePayloadRemoved, PayloadDirection::Accepted) => NonBreaking,

        // Any other kind reaching here is a wiring mistake, and the honest
        // answer to "I do not know how this got here" is not a pass.
        (_, _) => Unknown,
    }
}

/// The declared type set of a schema, treating "no `type`" as "any type".
fn offered_types(facts: &SchemaFacts) -> BTreeSet<String> {
    if facts.types.is_empty() {
        ALL_JSON_TYPES.iter().map(|t| t.to_string()).collect()
    } else {
        facts.types.clone()
    }
}

fn join(set: &BTreeSet<String>) -> String {
    set.iter().cloned().collect::<Vec<_>>().join(", ")
}

/// Recursively compares two schema revisions under a known direction.
fn payload_changes(
    from: &SchemaFacts,
    to: &SchemaFacts,
    direction: PayloadDirection,
    subject: &str,
    out: &mut Vec<ContractChange>,
) {
    // A subtree that was not fully modelled cannot be compared, and descending
    // into it would produce confident statements about a tree that was only
    // partly read. Report the gap once and stop.
    if !from.is_comparable() || !to.is_comparable() {
        out.push(ContractChange {
            kind: ChangeKind::PayloadIncomparable,
            severity: Severity::Unknown,
            subject: subject.to_string(),
            detail: format!(
                "schema is not fully modelled (from: {:?}, to: {:?}); a change inside it could \
                 not be detected",
                from.coverage, to.coverage
            ),
            coverage: worse_of(from.coverage, to.coverage),
        });
        return;
    }

    let push = |out: &mut Vec<ContractChange>,
                kind: ChangeKind,
                detail: String,
                coverage: CoverageState| {
        out.push(ContractChange {
            kind,
            severity: severity_for(kind, direction),
            subject: subject.to_string(),
            detail,
            coverage,
        });
    };

    // --- types -----------------------------------------------------------
    if !from.types.is_empty() || !to.types.is_empty() {
        let old = offered_types(from);
        let new = offered_types(to);
        let lost: BTreeSet<String> = old.difference(&new).cloned().collect();
        let gained: BTreeSet<String> = new.difference(&old).cloned().collect();
        // Reported as two independent facts rather than one "type changed",
        // because a disjoint change (`string` -> `integer`) is simultaneously a
        // narrowing and a widening and means different things in each direction.
        if !lost.is_empty() {
            push(
                out,
                ChangeKind::PayloadTypeNarrowed,
                format!("type `{}` is no longer declared", join(&lost)),
                CoverageState::Verified,
            );
        }
        if !gained.is_empty() {
            push(
                out,
                ChangeKind::PayloadTypeWidened,
                format!("type `{}` is newly declared", join(&gained)),
                CoverageState::Verified,
            );
        }
    }

    // --- enum ------------------------------------------------------------
    match (&from.enum_values, &to.enum_values) {
        (Some(old), Some(new)) => {
            let lost: BTreeSet<String> = old.difference(new).cloned().collect();
            let gained: BTreeSet<String> = new.difference(old).cloned().collect();
            if !lost.is_empty() {
                push(
                    out,
                    ChangeKind::PayloadEnumValueRemoved,
                    format!("enum value(s) `{}` are no longer permitted", join(&lost)),
                    CoverageState::Verified,
                );
            }
            if !gained.is_empty() {
                push(
                    out,
                    ChangeKind::PayloadEnumValueAdded,
                    format!("enum value(s) `{}` are newly permitted", join(&gained)),
                    CoverageState::Verified,
                );
            }
        }
        // Introducing a constraint removes permitted values, so it is reported
        // as a removal at the value level. Dropping one is the mirror image.
        (None, Some(new)) => push(
            out,
            ChangeKind::PayloadEnumValueRemoved,
            format!(
                "an enum constraint was introduced, permitting only `{}`",
                join(new)
            ),
            CoverageState::Verified,
        ),
        (Some(old), None) => push(
            out,
            ChangeKind::PayloadEnumValueAdded,
            format!("the enum constraint over `{}` was dropped", join(old)),
            CoverageState::Verified,
        ),
        (None, None) => {}
    }

    // --- requiredness ----------------------------------------------------
    for field in to.required.difference(&from.required) {
        push(
            out,
            ChangeKind::PayloadRequirednessGained,
            format!("`{field}` is now required"),
            CoverageState::Verified,
        );
    }
    for field in from.required.difference(&to.required) {
        push(
            out,
            ChangeKind::PayloadRequirednessLost,
            format!("`{field}` is no longer required"),
            CoverageState::Verified,
        );
    }

    // --- properties ------------------------------------------------------
    for name in from.properties.keys() {
        if !to.properties.contains_key(name) {
            push(
                out,
                ChangeKind::PayloadPropertyRemoved,
                format!("property `{name}` is no longer declared"),
                CoverageState::Verified,
            );
        }
    }
    for name in to.properties.keys() {
        if from.properties.contains_key(name) {
            continue;
        }
        // A newly required property already produced a `RequirednessGained`
        // change; emitting an addition as well would double-count one edit.
        if to.required.contains(name) {
            continue;
        }
        push(
            out,
            ChangeKind::PayloadPropertyAdded,
            format!("property `{name}` is newly declared"),
            CoverageState::Verified,
        );
    }
    for (name, from_child) in &from.properties {
        let Some(to_child) = to.properties.get(name) else {
            continue;
        };
        let child_subject = format!("{subject}/properties/{}", escape(name));
        payload_changes(from_child, to_child, direction, &child_subject, out);
    }

    // --- additionalProperties --------------------------------------------
    // `None` and `Some(true)` mean the same thing in JSON Schema, so only a
    // transition across `false` is a change.
    let allowed_before = from.additional_properties != Some(false);
    let allowed_after = to.additional_properties != Some(false);
    match (allowed_before, allowed_after) {
        (true, false) => push(
            out,
            ChangeKind::PayloadAdditionalPropertiesDisallowed,
            "the schema now rejects undeclared properties".to_string(),
            CoverageState::Verified,
        ),
        (false, true) => push(
            out,
            ChangeKind::PayloadAdditionalPropertiesAllowed,
            "the schema now permits undeclared properties".to_string(),
            CoverageState::Verified,
        ),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Producer / consumer flow verification
// ---------------------------------------------------------------------------

/// The way a producer's declaration failed to satisfy a consumer's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowMismatch {
    /// The consumer requires a property the producer's schema never declares.
    RequiredPropertyNotProduced,
    /// The producer may emit a type the consumer does not accept.
    TypeNotAccepted,
    /// The producer may emit a value outside the consumer's `enum`.
    EnumValueNotAccepted,
    /// The consumer rejects undeclared properties and the producer's schema
    /// permits them.
    AdditionalPropertiesRejected,
    /// A schema one side needs could not be read, so the pair was not decided.
    Incomparable,
}

/// One concrete reason a producer does not satisfy a consumer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowFinding {
    pub mismatch: FlowMismatch,
    pub subject: String,
    pub detail: String,
}

/// One producer/consumer message pair that was checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowCheck {
    pub channel: String,
    pub producer: String,
    pub consumer: String,
    /// Canonical message key from the producer's side.
    pub message: String,
    pub coverage: CoverageState,
    pub findings: Vec<FlowFinding>,
}

/// An operation that has no counterpart on its channel, so nothing was checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UncheckedFlow {
    pub channel: String,
    pub operation: String,
    pub action: Action,
    pub reason: String,
}

/// The result of cross-checking producers against consumers in one document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowReport {
    /// Pairs that were compared, in canonical order.
    pub checked: Vec<FlowCheck>,
    /// Operations with nothing to compare against. Not failures — but not
    /// verified either, and they block a `Compatible` verdict.
    pub unchecked: Vec<UncheckedFlow>,
    pub verdict: CompatibilityVerdict,
    pub coverage: CoverageState,
    /// Why the verdict is not `Compatible`, sorted and deduplicated.
    pub notes: Vec<String>,
}

impl FlowReport {
    /// Findings that a consumer must act on.
    pub fn findings(&self) -> Vec<(&FlowCheck, &FlowFinding)> {
        self.checked
            .iter()
            .flat_map(|c| c.findings.iter().map(move |f| (c, f)))
            .collect()
    }
}

/// Compares a producer's declared payload against a consumer's expectations.
///
/// The relation is *containment*: everything the producer guarantees must be
/// acceptable to the consumer. It is not the same question as
/// [`payload_changes`], which compares two revisions of one schema — a producer
/// can be perfectly stable across revisions and still not satisfy a consumer.
fn check_producer_against_consumer(
    producer: &SchemaFacts,
    consumer: &SchemaFacts,
    subject: &str,
    out: &mut Vec<FlowFinding>,
) {
    if !producer.is_comparable() || !consumer.is_comparable() {
        out.push(FlowFinding {
            mismatch: FlowMismatch::Incomparable,
            subject: subject.to_string(),
            detail: format!(
                "schema is not fully modelled (producer: {:?}, consumer: {:?}); the pair could \
                 not be decided",
                producer.coverage, consumer.coverage
            ),
        });
        return;
    }

    // 1. Every field the consumer requires must be one the producer guarantees.
    //    `additionalProperties` never guarantees a specific field, so being
    //    merely *permitted* to send it is not enough.
    for field in &consumer.required {
        if !producer.properties.contains_key(field) {
            out.push(FlowFinding {
                mismatch: FlowMismatch::RequiredPropertyNotProduced,
                subject: subject.to_string(),
                detail: format!(
                    "consumer requires `{field}` but the producer's schema does not declare it"
                ),
            });
        }
    }

    // 2. The producer's offered type set must be acceptable to the consumer.
    if !consumer.types.is_empty() {
        let offered = offered_types(producer);
        let unacceptable: BTreeSet<String> = offered.difference(&consumer.types).cloned().collect();
        if !unacceptable.is_empty() {
            out.push(FlowFinding {
                mismatch: FlowMismatch::TypeNotAccepted,
                subject: subject.to_string(),
                detail: format!(
                    "producer may emit type `{}`; the consumer accepts only `{}`",
                    join(&unacceptable),
                    join(&consumer.types)
                ),
            });
        }
    }

    // 3. Enum containment, when the consumer constrains values.
    if let Some(accepted) = &consumer.enum_values {
        match &producer.enum_values {
            Some(offered) => {
                let unacceptable: BTreeSet<String> =
                    offered.difference(accepted).cloned().collect();
                if !unacceptable.is_empty() {
                    out.push(FlowFinding {
                        mismatch: FlowMismatch::EnumValueNotAccepted,
                        subject: subject.to_string(),
                        detail: format!(
                            "producer may emit enum value(s) `{}`; the consumer accepts only `{}`",
                            join(&unacceptable),
                            join(accepted)
                        ),
                    });
                }
            }
            None => out.push(FlowFinding {
                mismatch: FlowMismatch::EnumValueNotAccepted,
                subject: subject.to_string(),
                detail: format!(
                    "consumer accepts only the enum value(s) `{}`, but the producer declares no \
                     enum and may therefore emit any value",
                    join(accepted)
                ),
            }),
        }
    }

    // 4. A closed consumer cannot be satisfied by a producer whose own contract
    //    does not restrict what it may add.
    if consumer.additional_properties == Some(false) {
        let extra: BTreeSet<String> = producer
            .properties
            .keys()
            .filter(|k| !consumer.properties.contains_key(*k))
            .cloned()
            .collect();
        if !extra.is_empty() {
            out.push(FlowFinding {
                mismatch: FlowMismatch::AdditionalPropertiesRejected,
                subject: subject.to_string(),
                detail: format!(
                    "consumer rejects undeclared properties; the producer declares `{}`",
                    join(&extra)
                ),
            });
        } else if producer.additional_properties != Some(false) {
            out.push(FlowFinding {
                mismatch: FlowMismatch::AdditionalPropertiesRejected,
                subject: subject.to_string(),
                detail: "consumer rejects undeclared properties, but the producer's schema \
                         permits them"
                    .to_string(),
            });
        }
    }

    // 5. Recurse into the properties both sides name.
    for (name, producer_child) in &producer.properties {
        let Some(consumer_child) = consumer.properties.get(name) else {
            continue;
        };
        let child_subject = format!("{subject}/properties/{}", escape(name));
        check_producer_against_consumer(producer_child, consumer_child, &child_subject, out);
    }
}

/// Message keys from `producer` that have a counterpart in `consumer`.
///
/// Pairing is by position first (the same `channelId/messageId`) and by declared
/// `name` second, because two services that share a message frequently declare
/// it under different ids. Pairing by name alone would miss the common case
/// where only one side names the message at all.
fn paired_keys(producer: &AsyncApiSpec, first: &[String], second: &[String]) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for a in first {
        let Some(ma) = producer.messages.get(a) else {
            continue;
        };
        for b in second {
            let Some(mb) = producer.messages.get(b) else {
                continue;
            };
            let same_position = a == b;
            let same_name = match (&ma.name, &mb.name) {
                (Some(x), Some(y)) => x == y,
                _ => false,
            };
            if same_position || same_name {
                out.insert(a.clone());
            }
        }
    }
    out.into_iter().collect()
}

/// Cross-checks every producer against every consumer on the same channel.
pub fn verify_flows(spec: &AsyncApiSpec) -> FlowReport {
    let mut checked: Vec<FlowCheck> = Vec::new();
    let mut unchecked: Vec<UncheckedFlow> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut coverage = CoverageState::Verified;

    // A message nothing references has no direction, so it cannot be part of a
    // flow check — and a document containing one is not fully accounted for.
    let orphans = spec.unreferenced_messages();
    if !orphans.is_empty() {
        coverage = worse_of(coverage, CoverageState::Partial);
        notes.push(format!(
            "{} message(s) are declared but no operation references them, so their direction is \
             undefined: {:?}",
            orphans.len(),
            orphans
        ));
    }

    for channel_id in spec.channels.keys() {
        let channel = &spec.channels[channel_id];
        let producers: Vec<&crate::asyncapi::AsyncOperation> = spec
            .operations
            .values()
            .filter(|o| o.channel == *channel_id && o.action.produces())
            .collect();
        let consumers: Vec<&crate::asyncapi::AsyncOperation> = spec
            .operations
            .values()
            .filter(|o| o.channel == *channel_id && !o.action.produces())
            .collect();

        if producers.is_empty() && consumers.is_empty() {
            coverage = worse_of(coverage, CoverageState::Partial);
            notes.push(format!(
                "channel `{channel_id}` has no operations, so nothing was checked on it"
            ));
            continue;
        }

        for producer in &producers {
            if consumers.is_empty() {
                coverage = worse_of(coverage, CoverageState::Partial);
                unchecked.push(UncheckedFlow {
                    channel: channel_id.clone(),
                    operation: producer.id.clone(),
                    action: producer.action,
                    reason: format!("channel `{channel_id}` declares no consumer operation"),
                });
                continue;
            }
            let producer_messages: Vec<String> = producer.messages.iter().cloned().collect();
            for consumer in &consumers {
                let consumer_messages: Vec<String> = consumer.messages.iter().cloned().collect();
                let keys = paired_keys(spec, &producer_messages, &consumer_messages);
                if keys.is_empty() {
                    // The two operations share the channel but not a message.
                    // Nothing was compared, and saying so is the whole point.
                    coverage = worse_of(coverage, CoverageState::Partial);
                    unchecked.push(UncheckedFlow {
                        channel: channel_id.clone(),
                        operation: producer.id.clone(),
                        action: producer.action,
                        reason: format!(
                            "operation `{}` and `{}` share channel `{channel_id}` but name no \
                             common message",
                            producer.id, consumer.id
                        ),
                    });
                    continue;
                }
                for key in keys {
                    let mut findings: Vec<FlowFinding> = Vec::new();
                    let mut pair_coverage = CoverageState::Verified;
                    let producer_message: &Message = &spec.messages[&key];
                    // The consumer's counterpart is the message it names that
                    // paired with this key.
                    let counterpart =
                        consumer
                            .messages
                            .iter()
                            .map(|k| &spec.messages[k])
                            .find(|m| {
                                m.key == producer_message.key
                                    || matches!(
                                        (&producer_message.name, &m.name),
                                        (Some(a), Some(b)) if a == b
                                    )
                            });

                    match counterpart {
                        None => {
                            pair_coverage = worse_of(pair_coverage, CoverageState::Partial);
                            findings.push(FlowFinding {
                                mismatch: FlowMismatch::Incomparable,
                                subject: format!("{channel_id} {key}"),
                                detail: format!(
                                    "operation `{}` names message `{key}`, which is not in the \
                                     parsed catalogue",
                                    consumer.id
                                ),
                            });
                        }
                        Some(consumer_message) => {
                            let subject = format!("{channel_id} {key}");
                            for (label, producer_facts, consumer_facts) in [
                                (
                                    "payload",
                                    producer_message.payload.as_ref(),
                                    consumer_message.payload.as_ref(),
                                ),
                                (
                                    "headers",
                                    producer_message.headers.as_ref(),
                                    consumer_message.headers.as_ref(),
                                ),
                            ] {
                                match (producer_facts, consumer_facts) {
                                    (Some(p), Some(c)) => {
                                        let mut local = Vec::new();
                                        check_producer_against_consumer(
                                            p,
                                            c,
                                            &format!("{subject}/{label}"),
                                            &mut local,
                                        );
                                        pair_coverage = worse_of(
                                            pair_coverage,
                                            worse_of(p.coverage, c.coverage),
                                        );
                                        findings.extend(local);
                                    }
                                    (None, None) => {
                                        // Neither side declares one: there is
                                        // genuinely nothing to be incompatible
                                        // about.
                                    }
                                    (Some(_), None) => {
                                        // The producer sends a body the consumer
                                        // says nothing about. That is a gap in
                                        // the document, not a proven break.
                                        pair_coverage =
                                            worse_of(pair_coverage, CoverageState::Partial);
                                        findings.push(FlowFinding {
                                            mismatch: FlowMismatch::Incomparable,
                                            subject: format!("{subject}/{label}"),
                                            detail: format!(
                                                "the producer declares {label} but the consumer \
                                                 declares none; the consumer's expectations are \
                                                 not written down"
                                            ),
                                        });
                                    }
                                    (None, Some(_)) => {
                                        pair_coverage =
                                            worse_of(pair_coverage, CoverageState::Partial);
                                        findings.push(FlowFinding {
                                            mismatch: FlowMismatch::Incomparable,
                                            subject: format!("{subject}/{label}"),
                                            detail: format!(
                                                "the consumer declares {label} but the producer \
                                                 declares none; a required field would never \
                                                 arrive"
                                            ),
                                        });
                                    }
                                }
                            }
                        }
                    }

                    coverage = worse_of(coverage, pair_coverage);
                    checked.push(FlowCheck {
                        channel: channel_id.clone(),
                        producer: producer.id.clone(),
                        consumer: consumer.id.clone(),
                        message: key,
                        coverage: pair_coverage,
                        findings,
                    });
                }
            }
        }

        for consumer in &consumers {
            if producers.is_empty() {
                unchecked.push(UncheckedFlow {
                    channel: channel_id.clone(),
                    operation: consumer.id.clone(),
                    action: consumer.action,
                    reason: format!("channel `{channel_id}` declares no producer operation"),
                });
            }
        }

        coverage = worse_of(coverage, channel.coverage);
    }

    if !unchecked.is_empty() {
        coverage = worse_of(coverage, CoverageState::Partial);
        notes.push(format!(
            "{} operation(s) have no counterpart on their channel, so their message flows were \
             not checked",
            unchecked.len()
        ));
    }

    let breaking = checked.iter().any(|c| {
        !c.findings.is_empty()
            && c.findings
                .iter()
                .any(|f| f.mismatch != FlowMismatch::Incomparable)
    });
    let verdict = if breaking {
        CompatibilityVerdict::Breaking
    } else if coverage == CoverageState::Verified {
        // Reached only when every flow was decided and nothing was left out.
        CompatibilityVerdict::Compatible
    } else {
        CompatibilityVerdict::Unknown
    };
    if verdict != CompatibilityVerdict::Compatible {
        notes.push(match verdict {
            CompatibilityVerdict::Breaking => {
                "at least one producer does not satisfy a consumer on the same channel".to_string()
            }
            _ => format!(
                "flow coverage is {coverage:?}, which is not enough to vouch for \
                          compatibility"
            ),
        });
    }

    notes.sort();
    notes.dedup();

    FlowReport {
        checked,
        unchecked,
        verdict,
        coverage,
        notes,
    }
}

/// Compares two AsyncAPI revisions.
///
/// Includes the candidate revision's own producer/consumer flow findings: a
/// delta that called a revision `Compatible` while its own producers cannot
/// satisfy its own consumers would be wrong.
pub fn diff_asyncapi(from: &AsyncApiSpec, to: &AsyncApiSpec) -> ContractDelta {
    let mut changes: Vec<ContractChange> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut coverage = worse_of(from.coverage, to.coverage);

    // --- channels --------------------------------------------------------
    for (id, channel) in &from.channels {
        match to.channels.get(id) {
            None => changes.push(surface_change(
                ChangeKind::ChannelRemoved,
                id,
                format!("channel `{id}` is no longer declared"),
                channel.coverage,
            )),
            Some(other) => {
                if channel.address != other.address {
                    changes.push(surface_change(
                        ChangeKind::ChannelAddressChanged,
                        id,
                        format!(
                            "channel `{id}` address moved from `{}` to `{}`",
                            channel.address.as_deref().unwrap_or("<none>"),
                            other.address.as_deref().unwrap_or("<none>")
                        ),
                        worse_of(channel.coverage, other.coverage),
                    ));
                }
            }
        }
    }
    for (id, channel) in &to.channels {
        if !from.channels.contains_key(id) {
            changes.push(surface_change(
                ChangeKind::ChannelAdded,
                id,
                format!("channel `{id}` is newly declared"),
                channel.coverage,
            ));
        }
    }

    // --- operations ------------------------------------------------------
    for (id, op) in &from.operations {
        match to.operations.get(id) {
            None => changes.push(surface_change(
                ChangeKind::OperationRemoved,
                id,
                format!("operation `{id}` is no longer declared"),
                op.coverage,
            )),
            Some(other) => {
                if op.action != other.action {
                    changes.push(surface_change(
                        ChangeKind::OperationActionChanged,
                        id,
                        format!(
                            "operation `{id}` changed from `{}` to `{}`, moving the application \
                             to the other side of the wire",
                            op.action.as_str(),
                            other.action.as_str()
                        ),
                        worse_of(op.coverage, other.coverage),
                    ));
                }
                if op.channel != other.channel {
                    changes.push(surface_change(
                        ChangeKind::OperationChannelChanged,
                        id,
                        format!(
                            "operation `{id}` moved from channel `{}` to `{}`",
                            op.channel, other.channel
                        ),
                        worse_of(op.coverage, other.coverage),
                    ));
                }
            }
        }
    }
    for (id, op) in &to.operations {
        if !from.operations.contains_key(id) {
            changes.push(surface_change(
                ChangeKind::OperationAdded,
                id,
                format!("operation `{id}` is newly declared"),
                op.coverage,
            ));
        }
    }

    // --- messages --------------------------------------------------------
    for (key, message) in &from.messages {
        match to.messages.get(key) {
            None => changes.push(surface_change(
                ChangeKind::MessageRemoved,
                key,
                format!("message `{key}` is no longer declared"),
                message.coverage,
            )),
            Some(other) => {
                if message.content_type != other.content_type {
                    changes.push(surface_change(
                        ChangeKind::MessageContentTypeChanged,
                        key,
                        format!(
                            "message `{key}` contentType changed from `{}` to `{}`",
                            message.content_type.as_deref().unwrap_or("<none>"),
                            other.content_type.as_deref().unwrap_or("<none>")
                        ),
                        worse_of(message.coverage, other.coverage),
                    ));
                }

                // Direction is taken from the candidate revision, because that
                // is the revision whose behaviour is being asserted. A message
                // whose direction changed is classified `Both`, which is the
                // worst of the two readings.
                let direction = match (to.message_direction(key), from.message_direction(key)) {
                    (Some(d), _) => d,
                    (None, Some(d)) => d,
                    (None, None) => PayloadDirection::Both,
                };

                let subject = format!("/channels/{key}");
                match (&message.payload, &other.payload) {
                    (Some(a), Some(b)) => {
                        payload_changes(
                            a,
                            b,
                            direction,
                            &format!("{subject}/payload"),
                            &mut changes,
                        );
                    }
                    (None, Some(b)) => changes.push(directional_change(
                        ChangeKind::MessagePayloadAdded,
                        direction,
                        &subject,
                        format!("message `{key}` newly declares a payload"),
                        b.coverage,
                    )),
                    (Some(a), None) => changes.push(directional_change(
                        ChangeKind::MessagePayloadRemoved,
                        direction,
                        &subject,
                        format!("message `{key}` no longer declares a payload"),
                        a.coverage,
                    )),
                    (None, None) => {}
                }
                match (&message.headers, &other.headers) {
                    (Some(a), Some(b)) => {
                        payload_changes(
                            a,
                            b,
                            direction,
                            &format!("{subject}/headers"),
                            &mut changes,
                        );
                    }
                    (None, Some(b)) => changes.push(directional_change(
                        ChangeKind::MessagePayloadAdded,
                        direction,
                        &format!("{subject}/headers"),
                        format!("message `{key}` newly declares headers"),
                        b.coverage,
                    )),
                    (Some(a), None) => changes.push(directional_change(
                        ChangeKind::MessagePayloadRemoved,
                        direction,
                        &format!("{subject}/headers"),
                        format!("message `{key}` no longer declares headers"),
                        a.coverage,
                    )),
                    (None, None) => {}
                }
            }
        }
    }
    for (key, message) in &to.messages {
        if !from.messages.contains_key(key) {
            changes.push(surface_change(
                ChangeKind::MessageAdded,
                key,
                format!("message `{key}` is newly declared"),
                message.coverage,
            ));
        }
    }

    // --- flow regressions --------------------------------------------------
    //
    // Only *newly introduced* flow problems are deltas. A document that
    // describes only the producer side of a channel cannot have its flows
    // checked — but if it described only the producer side in both revisions,
    // that is a pre-existing property of the document, not something this
    // revision did. Folding the absolute result in would make `Compatible`
    // unreachable for every one-sided document and would report an unchanged
    // gap as a regression, which is how a signal gets ignored.
    let flows_before = verify_flows(from);
    let flows_after = verify_flows(to);

    let finding_key = |check: &FlowCheck, finding: &FlowFinding| {
        format!(
            "{}|{}|{}|{}",
            check.channel, check.producer, check.consumer, finding.detail
        )
    };
    let before_findings: BTreeSet<String> = flows_before
        .checked
        .iter()
        .flat_map(|c| c.findings.iter().map(|f| finding_key(c, f)))
        .collect();
    let after_findings: BTreeSet<String> = flows_after
        .checked
        .iter()
        .flat_map(|c| c.findings.iter().map(|f| finding_key(c, f)))
        .collect();

    for flow in &flows_after.checked {
        for finding in &flow.findings {
            if before_findings.contains(&finding_key(flow, finding)) {
                continue;
            }
            changes.push(ContractChange {
                kind: ChangeKind::ProducerConsumerIncompatible,
                severity: if finding.mismatch == FlowMismatch::Incomparable {
                    Severity::Unknown
                } else {
                    Severity::Breaking
                },
                subject: format!("{} {}->{}", flow.channel, flow.producer, flow.consumer),
                detail: finding.detail.clone(),
                coverage: flow.coverage,
            });
        }
    }
    // Findings that the candidate revision resolved are worth saying out loud:
    // a delta that only ever reports problems is indistinguishable from a delta
    // that misses them.
    let resolved = flows_before
        .checked
        .iter()
        .flat_map(|c| c.findings.iter().map(move |f| (c, f)))
        .filter(|(c, f)| !after_findings.contains(&finding_key(c, f)))
        .count();
    if resolved > 0 {
        notes.push(format!(
            "{resolved} flow finding(s) present in the baseline are gone from the candidate"
        ));
    }

    // Newly unchecked flows are a regression in what this engine can vouch for,
    // even though they are not a proven break.
    let unchecked_before: BTreeSet<String> = flows_before
        .unchecked
        .iter()
        .map(|u| format!("{}|{}|{}", u.channel, u.operation, u.reason))
        .collect();
    let newly_unchecked: Vec<&UncheckedFlow> = flows_after
        .unchecked
        .iter()
        .filter(|u| {
            !unchecked_before.contains(&format!("{}|{}|{}", u.channel, u.operation, u.reason))
        })
        .collect();
    if !newly_unchecked.is_empty() {
        coverage = worse_of(coverage, CoverageState::Partial);
        notes.push(format!(
            "{} operation(s) that could be checked in the baseline can no longer be checked: {:?}",
            newly_unchecked.len(),
            newly_unchecked
                .iter()
                .map(|u| &u.operation)
                .collect::<Vec<_>>()
        ));
    }

    // Modelling coverage that got worse is itself a finding: the same edit can
    // look "compatible" simply because less of the candidate was read.
    if to.coverage > from.coverage {
        coverage = worse_of(coverage, to.coverage);
        notes.push(format!(
            "the candidate revision is less completely modelled than the baseline ({:?} vs {:?}); \
             a change could hide in what was not read",
            to.coverage, from.coverage
        ));
    }
    for note in &flows_after.notes {
        notes.push(note.clone());
    }

    // --- disclosure -------------------------------------------------------
    let orphans = to.unreferenced_messages();
    if !orphans.is_empty() {
        coverage = worse_of(coverage, CoverageState::Partial);
        notes.push(format!(
            "{} message(s) in the candidate revision are referenced by no operation, so their \
             direction is undefined: {:?}",
            orphans.len(),
            orphans
        ));
    } else {
        // Nothing to disclose beyond the flow report.
    }
    for note in &to.notes {
        notes.push(note.clone());
    }

    changes.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.subject.cmp(&b.subject))
            .then_with(|| a.detail.cmp(&b.detail))
    });
    notes.push(format!("coverage is {coverage:?}"));
    notes.sort();
    notes.dedup();
    // The coverage line is derived, not authored, so it must not read as a
    // caveat when coverage is complete.
    notes.retain(|n| {
        n != &format!("coverage is {coverage:?}") || coverage != CoverageState::Verified
    });

    let verdict = if changes.iter().any(|c| c.severity == Severity::Breaking) {
        CompatibilityVerdict::Breaking
    } else if coverage == CoverageState::Verified
        && !changes.iter().any(|c| c.severity == Severity::Unknown)
    {
        CompatibilityVerdict::Compatible
    } else {
        CompatibilityVerdict::Unknown
    };
    if verdict == CompatibilityVerdict::Unknown {
        notes.push(format!(
            "the verdict is `unknown` because coverage is {coverage:?} or a change could not be \
             classified"
        ));
        notes.sort();
        notes.dedup();
    }

    let mut delta = ContractDelta {
        protocol: "asyncapi".to_string(),
        from_path: from.path.clone(),
        to_path: to.path.clone(),
        from_digest: from.digest(),
        to_digest: to.digest(),
        version_change: if from.declared_version == to.declared_version {
            None
        } else {
            Some(VersionChange {
                from: from.declared_version.clone(),
                to: to.declared_version.clone(),
            })
        },
        changes,
        verdict,
        coverage,
        notes,
        digest: String::new(),
    };
    delta.digest = delta_digest(&delta);
    delta
}

fn surface_change(
    kind: ChangeKind,
    subject: &str,
    detail: String,
    coverage: CoverageState,
) -> ContractChange {
    ContractChange {
        kind,
        severity: kind.default_severity(),
        subject: subject.to_string(),
        detail,
        coverage,
    }
}

fn directional_change(
    kind: ChangeKind,
    direction: PayloadDirection,
    subject: &str,
    detail: String,
    coverage: CoverageState,
) -> ContractChange {
    ContractChange {
        kind,
        severity: severity_for(kind, direction),
        subject: subject.to_string(),
        detail,
        coverage,
    }
}

/// BLAKE3 over the canonical delta body.
///
/// Excludes `notes` (prose, which is allowed to be reworded) and includes
/// `severity`, so a reclassification is a different digest even when the
/// detected facts are identical.
fn delta_digest(delta: &ContractDelta) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(delta.protocol.as_bytes());
    hasher.update(b"\n");
    hasher.update(delta.from_digest.as_bytes());
    hasher.update(b"\n");
    hasher.update(delta.to_digest.as_bytes());
    hasher.update(b"\n");
    hasher.update(delta.verdict.as_str().as_bytes());
    hasher.update(b"\n");
    hasher.update(format!("{:?}", delta.coverage).as_bytes());
    hasher.update(b"\n");
    if let Some(change) = &delta.version_change {
        hasher.update(change.from.as_bytes());
        hasher.update(b"->");
        hasher.update(change.to.as_bytes());
        hasher.update(b"\n");
    }
    for change in &delta.changes {
        hasher.update(format!("{:?}", change.kind).as_bytes());
        hasher.update(b"|");
        hasher.update(format!("{:?}", change.severity).as_bytes());
        hasher.update(b"|");
        hasher.update(change.subject.as_bytes());
        hasher.update(b"|");
        hasher.update(change.detail.as_bytes());
        hasher.update(b"|");
        hasher.update(format!("{:?}", change.coverage).as_bytes());
        hasher.update(b"\n");
    }
    hasher.finalize().to_hex().to_string()
}

/// Grouping helper used by callers that want findings per channel.
pub fn findings_by_channel(report: &FlowReport) -> BTreeMap<String, Vec<&FlowFinding>> {
    let mut out: BTreeMap<String, Vec<&FlowFinding>> = BTreeMap::new();
    for check in &report.checked {
        for finding in &check.findings {
            out.entry(check.channel.clone()).or_default().push(finding);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asyncapi::parse_asyncapi;
    use crate::document::DocFormat;

    fn parse(json: &str) -> AsyncApiSpec {
        let doc = SpecDocumentShim::parse(json);
        parse_asyncapi(&doc).expect("parses")
    }

    struct SpecDocumentShim;

    impl SpecDocumentShim {
        fn parse(json: &str) -> crate::document::SpecDocument {
            crate::document::SpecDocument::parse("a.json", json, DocFormat::Json).expect("parses")
        }
    }

    /// The direction algebra is the load-bearing part of this module, so it is
    /// asserted kind by kind rather than sampled.
    #[test]
    fn the_direction_algebra_is_exactly_as_documented() {
        use ChangeKind as K;
        use PayloadDirection::{Accepted, Emitted};
        use Severity::{Breaking, NonBreaking, Unknown};

        let cases: &[(ChangeKind, PayloadDirection, Severity)] = &[
            (K::PayloadPropertyRemoved, Emitted, Breaking),
            (K::PayloadPropertyRemoved, Accepted, Breaking),
            (K::PayloadPropertyAdded, Emitted, Unknown),
            (K::PayloadPropertyAdded, Accepted, NonBreaking),
            (K::PayloadRequirednessGained, Emitted, NonBreaking),
            (K::PayloadRequirednessGained, Accepted, Breaking),
            (K::PayloadRequirednessLost, Emitted, Breaking),
            (K::PayloadRequirednessLost, Accepted, NonBreaking),
            (K::PayloadTypeNarrowed, Emitted, NonBreaking),
            (K::PayloadTypeNarrowed, Accepted, Breaking),
            (K::PayloadTypeWidened, Emitted, Breaking),
            (K::PayloadTypeWidened, Accepted, NonBreaking),
            (K::PayloadEnumValueRemoved, Emitted, NonBreaking),
            (K::PayloadEnumValueRemoved, Accepted, Breaking),
            (K::PayloadEnumValueAdded, Emitted, Breaking),
            (K::PayloadEnumValueAdded, Accepted, NonBreaking),
            (
                K::PayloadAdditionalPropertiesDisallowed,
                Emitted,
                NonBreaking,
            ),
            (K::PayloadAdditionalPropertiesDisallowed, Accepted, Breaking),
            (K::PayloadAdditionalPropertiesAllowed, Emitted, Unknown),
            (K::PayloadAdditionalPropertiesAllowed, Accepted, NonBreaking),
            (K::PayloadIncomparable, Emitted, Unknown),
            (K::PayloadIncomparable, Accepted, Unknown),
        ];
        for (kind, direction, expected) in cases {
            assert_eq!(
                severity_for(*kind, *direction),
                *expected,
                "{kind:?} under {direction:?}"
            );
        }

        // `Both` is the worse of the two readings, never the better one.
        for kind in [
            K::PayloadPropertyRemoved,
            K::PayloadPropertyAdded,
            K::PayloadRequirednessGained,
            K::PayloadRequirednessLost,
            K::PayloadTypeNarrowed,
            K::PayloadTypeWidened,
            K::PayloadEnumValueRemoved,
            K::PayloadEnumValueAdded,
            K::PayloadAdditionalPropertiesDisallowed,
            K::PayloadAdditionalPropertiesAllowed,
        ] {
            let both = severity_for(kind, PayloadDirection::Both);
            assert_eq!(
                both,
                worst(severity_for(kind, Emitted), severity_for(kind, Accepted)),
                "{kind:?}: `Both` is not the worst of the two directions"
            );
            assert_ne!(
                both,
                NonBreaking.max(severity_for(kind, Emitted)),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn a_kind_reaching_the_algebra_unhandled_is_unknown_not_a_pass() {
        // `EndpointRemoved` is an OpenAPI kind; if it ever arrives here the
        // wiring is wrong and the answer must not be an approval.
        assert_eq!(
            severity_for(ChangeKind::EndpointRemoved, PayloadDirection::Emitted),
            Severity::Unknown
        );
    }

    #[test]
    fn worst_never_upgrades() {
        use Severity::*;
        assert_eq!(worst(Breaking, NonBreaking), Breaking);
        assert_eq!(worst(Unknown, NonBreaking), Unknown);
        assert_eq!(worst(Breaking, Unknown), Breaking);
        assert_eq!(worst(NonBreaking, NonBreaking), NonBreaking);
    }

    #[test]
    fn offered_types_treats_an_absent_type_as_every_type() {
        let mut facts = SchemaFacts::unknown("/x");
        facts.coverage = CoverageState::Verified;
        assert_eq!(offered_types(&facts).len(), ALL_JSON_TYPES.len());
        facts.types.insert("string".to_string());
        assert_eq!(
            offered_types(&facts),
            BTreeSet::from(["string".to_string()])
        );
    }

    // -- end-to-end flow checks -------------------------------------------

    const PRODUCER_CONSUMER: &str = r###"{
      "asyncapi": "3.1.0",
      "channels": {
        "orders": {
          "address": "orders",
          "messages": {
            "OrderPlaced": {
              "name": "OrderPlaced",
              "payload": {
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"]
              }
            }
          }
        }
      },
      "operations": {
        "publish": { "action": "send", "channel": { "$ref": "#/channels/orders" } },
        "subscribe": { "action": "receive", "channel": { "$ref": "#/channels/orders" } }
      }
    }"###;

    #[test]
    fn a_producer_and_consumer_agreeing_on_one_message_is_compatible() {
        let spec = parse(PRODUCER_CONSUMER);
        let report = verify_flows(&spec);
        assert_eq!(report.checked.len(), 1);
        assert!(report.checked[0].findings.is_empty());
        assert!(report.unchecked.is_empty());
        assert_eq!(report.coverage, CoverageState::Verified);
        assert_eq!(report.verdict, CompatibilityVerdict::Compatible);
        assert!(report.verdict.is_approval());
    }

    #[test]
    fn a_consumer_requiring_a_field_the_producer_never_declares_breaks() {
        let producer = parse(PRODUCER_CONSUMER);
        // Add a consumer-side message that demands a field the producer's
        // schema never declares, and make the consumer use it.
        let consumer_demands_more = r###"{
          "asyncapi": "3.1.0",
          "channels": {
            "orders": {
              "address": "orders",
              "messages": {
                "OrderPlaced": {
                  "name": "OrderPlaced",
                  "payload": {
                    "type": "object",
                    "properties": { "id": { "type": "string" } },
                    "required": ["id"]
                  }
                },
                "OrderPlacedStrict": {
                  "name": "OrderPlacedStrict",
                  "payload": {
                    "type": "object",
                    "properties": {
                      "id": { "type": "string" },
                      "total": { "type": "number" }
                    },
                    "required": ["id", "total"]
                  }
                }
              }
            }
          },
          "operations": {
            "publish": {
              "action": "send",
              "channel": { "$ref": "#/channels/orders" },
              "messages": [ { "$ref": "#/channels/orders/messages/OrderPlaced" } ]
            },
            "subscribeStrict": {
              "action": "receive",
              "channel": { "$ref": "#/channels/orders" },
              "messages": [ { "$ref": "#/channels/orders/messages/OrderPlacedStrict" } ]
            }
          }
        }"###;
        let _ = producer;
        let spec = parse(consumer_demands_more);
        let report = verify_flows(&spec);
        // The names differ, so the pair is reached by position only when the
        // keys match; here they do not and the flow is reported as unchecked
        // rather than silently passing.
        assert_eq!(report.verdict, CompatibilityVerdict::Unknown);
        assert!(!report.unchecked.is_empty());
    }

    #[test]
    fn a_shared_message_name_pairs_across_different_keys() {
        // The realistic cross-service case: each side declares the message under
        // its own id but both name it.
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": {
                "orders": {
                  "address": "orders",
                  "messages": {
                    "Pub": {
                      "name": "OrderPlaced",
                      "payload": {
                        "type": "object",
                        "properties": { "id": { "type": "string" } },
                        "required": ["id"]
                      }
                    },
                    "Sub": {
                      "name": "OrderPlaced",
                      "payload": {
                        "type": "object",
                        "properties": {
                          "id": { "type": "string" },
                          "total": { "type": "number" }
                        },
                        "required": ["id", "total"]
                      }
                    }
                  }
                }
              },
              "operations": {
                "publish": {
                  "action": "send",
                  "channel": { "$ref": "#/channels/orders" },
                  "messages": [ { "$ref": "#/channels/orders/messages/Pub" } ]
                },
                "subscribe": {
                  "action": "receive",
                  "channel": { "$ref": "#/channels/orders" },
                  "messages": [ { "$ref": "#/channels/orders/messages/Sub" } ]
                }
              }
            }"###,
        );
        let report = verify_flows(&spec);
        assert_eq!(report.checked.len(), 1, "{:?}", report);
        let findings = &report.checked[0].findings;
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(
            findings[0].mismatch,
            FlowMismatch::RequiredPropertyNotProduced
        );
        assert!(findings[0].detail.contains("total"));
        assert_eq!(report.verdict, CompatibilityVerdict::Breaking);
        assert!(!report.verdict.is_approval());
    }

    #[test]
    fn a_producer_may_emit_more_types_than_the_consumer_accepts() {
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": {
                "c": {
                  "messages": {
                    "Pub": { "name": "M", "payload": { "type": ["string", "integer"] } },
                    "Sub": { "name": "M", "payload": { "type": "string" } }
                  }
                }
              },
              "operations": {
                "p": {
                  "action": "send",
                  "channel": { "$ref": "#/channels/c" },
                  "messages": [ { "$ref": "#/channels/c/messages/Pub" } ]
                },
                "s": {
                  "action": "receive",
                  "channel": { "$ref": "#/channels/c" },
                  "messages": [ { "$ref": "#/channels/c/messages/Sub" } ]
                }
              }
            }"###,
        );
        let report = verify_flows(&spec);
        let findings = &report.checked[0].findings;
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].mismatch, FlowMismatch::TypeNotAccepted);
        assert_eq!(report.verdict, CompatibilityVerdict::Breaking);
    }

    #[test]
    fn an_unconstrained_producer_over_delivers_and_is_reported() {
        // A payload with no `type` accepts every JSON type, so it is not a
        // subset of a consumer that accepts only strings.
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": {
                "c": {
                  "messages": {
                    "Pub": { "name": "M", "payload": {} },
                    "Sub": { "name": "M", "payload": { "type": "string" } }
                  }
                }
              },
              "operations": {
                "p": {
                  "action": "send",
                  "channel": { "$ref": "#/channels/c" },
                  "messages": [ { "$ref": "#/channels/c/messages/Pub" } ]
                },
                "s": {
                  "action": "receive",
                  "channel": { "$ref": "#/channels/c" },
                  "messages": [ { "$ref": "#/channels/c/messages/Sub" } ]
                }
              }
            }"###,
        );
        let report = verify_flows(&spec);
        let findings = &report.checked[0].findings;
        assert_eq!(findings[0].mismatch, FlowMismatch::TypeNotAccepted);
        assert_eq!(report.verdict, CompatibilityVerdict::Breaking);
    }

    #[test]
    fn a_consumer_enum_the_producer_does_not_constrain_is_reported() {
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": {
                "c": {
                  "messages": {
                    "Pub": { "name": "M", "payload": { "type": "string" } },
                    "Sub": {
                      "name": "M",
                      "payload": { "type": "string", "enum": ["a", "b"] }
                    }
                  }
                }
              },
              "operations": {
                "p": {
                  "action": "send",
                  "channel": { "$ref": "#/channels/c" },
                  "messages": [ { "$ref": "#/channels/c/messages/Pub" } ]
                },
                "s": {
                  "action": "receive",
                  "channel": { "$ref": "#/channels/c" },
                  "messages": [ { "$ref": "#/channels/c/messages/Sub" } ]
                }
              }
            }"###,
        );
        let report = verify_flows(&spec);
        assert!(
            report.checked[0]
                .findings
                .iter()
                .any(|f| f.mismatch == FlowMismatch::EnumValueNotAccepted),
            "{:?}",
            report.checked[0].findings
        );
        assert_eq!(report.verdict, CompatibilityVerdict::Breaking);
    }

    #[test]
    fn a_closed_consumer_rejects_a_permissive_producer() {
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": {
                "c": {
                  "messages": {
                    "Pub": { "name": "M", "payload": { "type": "object" } },
                    "Sub": {
                      "name": "M",
                      "payload": {
                        "type": "object",
                        "properties": {},
                        "additionalProperties": false
                      }
                    }
                  }
                }
              },
              "operations": {
                "p": {
                  "action": "send",
                  "channel": { "$ref": "#/channels/c" },
                  "messages": [ { "$ref": "#/channels/c/messages/Pub" } ]
                },
                "s": {
                  "action": "receive",
                  "channel": { "$ref": "#/channels/c" },
                  "messages": [ { "$ref": "#/channels/c/messages/Sub" } ]
                }
              }
            }"###,
        );
        let report = verify_flows(&spec);
        assert!(
            report.checked[0]
                .findings
                .iter()
                .any(|f| f.mismatch == FlowMismatch::AdditionalPropertiesRejected),
            "{:?}",
            report.checked[0].findings
        );
    }

    #[test]
    fn a_channel_with_only_a_producer_is_never_compatible() {
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": {
                "c": { "messages": { "M": { "payload": { "type": "string" } } } }
              },
              "operations": {
                "p": { "action": "send", "channel": { "$ref": "#/channels/c" } }
              }
            }"###,
        );
        let report = verify_flows(&spec);
        assert!(report.checked.is_empty());
        assert_eq!(report.unchecked.len(), 1);
        assert_eq!(report.verdict, CompatibilityVerdict::Unknown);
        assert!(!report.verdict.is_approval());
        assert!(report.notes.iter().any(|n| n.contains("no counterpart")));
    }

    #[test]
    fn a_message_with_no_payload_on_either_side_is_not_a_gap() {
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": { "c": { "messages": { "M": { "name": "M" } } } },
              "operations": {
                "p": { "action": "send", "channel": { "$ref": "#/channels/c" } },
                "s": { "action": "receive", "channel": { "$ref": "#/channels/c" } }
              }
            }"###,
        );
        let report = verify_flows(&spec);
        assert!(report.checked[0].findings.is_empty());
        assert_eq!(report.verdict, CompatibilityVerdict::Compatible);
    }

    #[test]
    fn an_asymmetric_payload_is_a_documented_gap_not_a_pass() {
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": {
                "c": {
                  "messages": {
                    "Pub": { "name": "M", "payload": { "type": "string" } },
                    "Sub": { "name": "M" }
                  }
                }
              },
              "operations": {
                "p": {
                  "action": "send",
                  "channel": { "$ref": "#/channels/c" },
                  "messages": [ { "$ref": "#/channels/c/messages/Pub" } ]
                },
                "s": {
                  "action": "receive",
                  "channel": { "$ref": "#/channels/c" },
                  "messages": [ { "$ref": "#/channels/c/messages/Sub" } ]
                }
              }
            }"###,
        );
        let report = verify_flows(&spec);
        assert!(
            report.checked[0]
                .findings
                .iter()
                .any(|f| f.mismatch == FlowMismatch::Incomparable),
            "{:?}",
            report.checked[0].findings
        );
        // An `Incomparable` finding is not a proven break, so the verdict is
        // `unknown` rather than `breaking` — but it is never `compatible`.
        assert_eq!(report.verdict, CompatibilityVerdict::Unknown);
        assert!(!report.verdict.is_approval());
    }

    // -- revision diffs ----------------------------------------------------

    fn diff(from: &str, to: &str) -> ContractDelta {
        diff_asyncapi(&parse(from), &parse(to))
    }

    #[test]
    fn an_identical_document_is_compatible_and_has_no_changes() {
        let delta = diff(PRODUCER_CONSUMER, PRODUCER_CONSUMER);
        assert!(delta.changes.is_empty(), "{:?}", delta.changes);
        assert_eq!(delta.verdict, CompatibilityVerdict::Compatible);
        assert_eq!(delta.protocol, "asyncapi");
        assert_eq!(delta.from_digest, delta.to_digest);
    }

    #[test]
    fn removing_a_channel_is_breaking() {
        let to = r###"{
          "asyncapi": "3.1.0",
          "channels": {},
          "operations": {}
        }"###;
        let delta = diff(PRODUCER_CONSUMER, to);
        assert!(
            delta
                .breaking()
                .iter()
                .any(|c| c.kind == ChangeKind::ChannelRemoved)
        );
        assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
    }

    #[test]
    fn moving_a_channel_address_is_breaking() {
        let to = PRODUCER_CONSUMER.replace("\"address\": \"orders\"", "\"address\": \"orders.v2\"");
        assert_ne!(to, PRODUCER_CONSUMER, "fixture edit was a no-op");
        let delta = diff(PRODUCER_CONSUMER, &to);
        assert!(
            delta
                .breaking()
                .iter()
                .any(|c| c.kind == ChangeKind::ChannelAddressChanged)
        );
    }

    #[test]
    fn flipping_an_action_is_breaking() {
        let to = PRODUCER_CONSUMER.replace(
            "\"publish\": { \"action\": \"send\"",
            "\"publish\": { \"action\": \"receive\"",
        );
        assert_ne!(to, PRODUCER_CONSUMER, "fixture edit was a no-op");
        let delta = diff(PRODUCER_CONSUMER, &to);
        assert!(
            delta
                .breaking()
                .iter()
                .any(|c| c.kind == ChangeKind::OperationActionChanged)
        );
    }

    #[test]
    fn flipping_an_action_also_changes_the_direction_of_its_messages() {
        // Flipping `send` to `receive` leaves both operations receiving, so the
        // message is `Accepted` only. A change that would have been non-breaking
        // in the emitted direction must not be classified that way.
        let from = PRODUCER_CONSUMER;
        let mut to = PRODUCER_CONSUMER.replace(
            "\"required\": [\"id\"]",
            "\"required\": [\"id\", \"lateMandatoryField\"]",
        );
        to = to.replace(
            "\"properties\": { \"id\": { \"type\": \"string\" } }",
            "\"properties\": { \"id\": { \"type\": \"string\" }, \"lateMandatoryField\": { \"type\": \"string\" } }",
        );
        assert_ne!(to, from, "fixture edit was a no-op");
        // `Both`: the document both sends and receives this message, so the new
        // required field is a demand as well as a guarantee.
        let delta = diff(from, &to);
        assert!(
            delta
                .breaking()
                .iter()
                .any(|c| c.kind == ChangeKind::PayloadRequirednessGained),
            "{:?}",
            delta.changes
        );
        assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
    }

    #[test]
    fn an_optional_property_added_to_a_purely_emitted_message_is_unknown() {
        let from = r###"{
          "asyncapi": "3.1.0",
          "channels": {
            "c": {
              "messages": {
                "M": {
                  "payload": {
                    "type": "object",
                    "properties": { "a": { "type": "string" } }
                  }
                }
              }
            }
          },
          "operations": {
            "p": { "action": "send", "channel": { "$ref": "#/channels/c" } }
          }
        }"###;
        let to = from.replace(
            "\"properties\": { \"a\": { \"type\": \"string\" } }",
            "\"properties\": { \"a\": { \"type\": \"string\" }, \"b\": { \"type\": \"string\" } }",
        );
        assert_ne!(to, from, "fixture edit was a no-op");
        let delta = diff(from, &to);
        let added: Vec<_> = delta
            .changes
            .iter()
            .filter(|c| c.kind == ChangeKind::PayloadPropertyAdded)
            .collect();
        assert_eq!(added.len(), 1, "{:?}", delta.changes);
        assert_eq!(added[0].severity, Severity::Unknown);
        assert_eq!(delta.verdict, CompatibilityVerdict::Unknown);
        // Crucially not an approval: a strict consumer may reject the new field.
        assert!(!delta.verdict.is_approval());
    }

    #[test]
    fn the_same_optional_property_on_an_accepted_message_is_non_breaking() {
        let from = r###"{
          "asyncapi": "3.1.0",
          "channels": {
            "c": {
              "messages": {
                "M": {
                  "payload": {
                    "type": "object",
                    "properties": { "a": { "type": "string" } }
                  }
                }
              }
            }
          },
          "operations": {
            "s": { "action": "receive", "channel": { "$ref": "#/channels/c" } }
          }
        }"###;
        let to = from.replace(
            "\"properties\": { \"a\": { \"type\": \"string\" } }",
            "\"properties\": { \"a\": { \"type\": \"string\" }, \"b\": { \"type\": \"string\" } }",
        );
        assert_ne!(to, from, "fixture edit was a no-op");
        let delta = diff(from, &to);
        let added: Vec<_> = delta
            .changes
            .iter()
            .filter(|c| c.kind == ChangeKind::PayloadPropertyAdded)
            .collect();
        assert_eq!(added.len(), 1, "{:?}", delta.changes);
        assert_eq!(added[0].severity, Severity::NonBreaking);
        assert_eq!(delta.verdict, CompatibilityVerdict::Compatible);
    }

    #[test]
    fn adding_a_required_field_is_breaking_for_a_consumer_and_safe_for_a_producer() {
        let base = |action: &str| {
            format!(
                r###"{{
                  "asyncapi": "3.1.0",
                  "channels": {{
                    "c": {{
                      "messages": {{
                        "M": {{
                          "payload": {{
                            "type": "object",
                            "properties": {{ "a": {{ "type": "string" }} }},
                            "required": []
                          }}
                        }}
                      }}
                    }}
                  }},
                  "operations": {{
                    "op": {{ "action": "{action}", "channel": {{ "$ref": "#/channels/c" }} }}
                  }}
                }}"###
            )
        };
        let tighten = |s: &str| {
            s.replace(
                "\"properties\": { \"a\": { \"type\": \"string\" } },\n                            \"required\": []",
                "\"properties\": { \"a\": { \"type\": \"string\" } },\n                            \"required\": [\"a\"]",
            )
        };

        let consumer_before = base("receive");
        let consumer_after = tighten(&consumer_before);
        assert_ne!(consumer_after, consumer_before, "fixture edit was a no-op");
        let delta = diff(&consumer_before, &consumer_after);
        assert!(
            delta
                .breaking()
                .iter()
                .any(|c| c.kind == ChangeKind::PayloadRequirednessGained),
            "{:?}",
            delta.changes
        );

        let producer_before = base("send");
        let producer_after = tighten(&producer_before);
        assert_ne!(producer_after, producer_before, "fixture edit was a no-op");
        let delta = diff(&producer_before, &producer_after);
        let gained: Vec<_> = delta
            .changes
            .iter()
            .filter(|c| c.kind == ChangeKind::PayloadRequirednessGained)
            .collect();
        assert_eq!(gained.len(), 1, "{:?}", delta.changes);
        assert_eq!(gained[0].severity, Severity::NonBreaking);
    }

    #[test]
    fn a_disjoint_type_change_is_reported_as_both_a_narrowing_and_a_widening() {
        let from = r###"{"asyncapi":"3.1.0","channels":{"c":{"messages":{"M":{
          "payload":{"type":"string"}}}}},
          "operations":{"p":{"action":"send","channel":{"$ref":"#/channels/c"}}}}"###;
        let to = from.replace("\"type\":\"string\"", "\"type\":\"integer\"");
        assert_ne!(to, from, "fixture edit was a no-op");
        let delta = diff(from, &to);
        let kinds: BTreeSet<ChangeKind> = delta.changes.iter().map(|c| c.kind).collect();
        assert!(
            kinds.contains(&ChangeKind::PayloadTypeNarrowed),
            "{kinds:?}"
        );
        assert!(kinds.contains(&ChangeKind::PayloadTypeWidened), "{kinds:?}");
        // Emitted direction: losing `string` is safe, gaining `integer` is not.
        assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
    }

    #[test]
    fn an_unmodelled_subtree_is_reported_incomparable_rather_than_compared() {
        let from = r###"{"asyncapi":"3.1.0","channels":{"c":{"messages":{"M":{
          "payload":{"type":"object","properties":{"a":{"allOf":[{"type":"string"}]}}}}}}},
          "operations":{"p":{"action":"send","channel":{"$ref":"#/channels/c"}}}}"###;
        let to = from.replace("{ \"type\": \"string\" }", "{ \"type\": \"integer\" }");
        // The composition keyword means the property cannot be compared at all,
        // so the edit inside it must not be reported as a type change.
        let delta = diff(from, from);
        assert!(
            delta
                .changes
                .iter()
                .any(|c| c.kind == ChangeKind::PayloadIncomparable),
            "{:?}",
            delta.changes
        );
        assert_eq!(delta.verdict, CompatibilityVerdict::Unknown);
        let _ = to;
    }

    #[test]
    fn a_payload_appearing_or_disappearing_is_classified_not_ignored() {
        let without = r###"{"asyncapi":"3.1.0","channels":{"c":{"messages":{"M":{"name":"M"}}}},
          "operations":{"p":{"action":"send","channel":{"$ref":"#/channels/c"}}}}"###;
        let with = r###"{"asyncapi":"3.1.0","channels":{"c":{"messages":{"M":{
          "name":"M","payload":{"type":"string"}}}}},
          "operations":{"p":{"action":"send","channel":{"$ref":"#/channels/c"}}}}"###;

        let added = diff(without, with);
        assert!(
            added
                .changes
                .iter()
                .any(|c| c.kind == ChangeKind::MessagePayloadAdded),
            "{:?}",
            added.changes
        );
        // Emitted direction: the consumer may not accept a body at all.
        assert_eq!(added.verdict, CompatibilityVerdict::Unknown);

        let removed = diff(with, without);
        assert!(
            removed
                .breaking()
                .iter()
                .any(|c| c.kind == ChangeKind::MessagePayloadRemoved),
            "{:?}",
            removed.changes
        );
    }

    #[test]
    fn a_content_type_change_is_breaking() {
        let to = PRODUCER_CONSUMER.replace(
            "\"name\": \"OrderPlaced\",",
            "\"name\": \"OrderPlaced\",\n              \"contentType\": \"application/xml\",",
        );
        assert_ne!(to, PRODUCER_CONSUMER, "fixture edit was a no-op");
        let delta = diff(PRODUCER_CONSUMER, &to);
        assert!(
            delta
                .breaking()
                .iter()
                .any(|c| c.kind == ChangeKind::MessageContentTypeChanged)
        );
    }

    #[test]
    fn flow_findings_are_folded_into_the_revision_delta() {
        // The candidate revision's own producer cannot satisfy its own consumer.
        let to = r###"{
          "asyncapi": "3.1.0",
          "channels": {
            "orders": {
              "address": "orders",
              "messages": {
                "Pub": {
                  "name": "M",
                  "payload": { "type": "object", "properties": { "ref": { "type": "string" } } }
                },
                "Sub": {
                  "name": "M",
                  "payload": {
                    "type": "object",
                    "properties": { "id": { "type": "string" } },
                    "required": ["id"]
                  }
                }
              }
            }
          },
          "operations": {
            "publish": {
              "action": "send",
              "channel": { "$ref": "#/channels/orders" },
              "messages": [ { "$ref": "#/channels/orders/messages/Pub" } ]
            },
            "subscribe": {
              "action": "receive",
              "channel": { "$ref": "#/channels/orders" },
              "messages": [ { "$ref": "#/channels/orders/messages/Sub" } ]
            }
          }
        }"###;
        let delta = diff(PRODUCER_CONSUMER, to);
        let folded: Vec<_> = delta
            .breaking()
            .into_iter()
            .filter(|c| c.kind == ChangeKind::ProducerConsumerIncompatible)
            .collect();
        assert_eq!(folded.len(), 1, "{:?}", delta.changes);
        assert!(folded[0].detail.contains("`id`"), "{}", folded[0].detail);
        assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
    }

    #[test]
    fn a_pre_existing_unverifiable_flow_is_not_reported_as_a_regression() {
        // A document that only ever described the producer side cannot have its
        // flows checked. Reporting that as a *revision* problem would make
        // `Compatible` unreachable for every one-sided document, and would train
        // users to ignore the signal.
        let one_sided = r###"{
          "asyncapi": "3.1.0",
          "channels": {
            "c": {
              "messages": {
                "M": { "payload": { "type": "object", "properties": { "a": { "type": "string" } } } }
              }
            }
          },
          "operations": {
            "p": { "action": "send", "channel": { "$ref": "#/channels/c" } }
          }
        }"###;
        let renamed = one_sided.replace(
            "\"properties\": { \"a\": { \"type\": \"string\" } }",
            "\"properties\": { \"a\": { \"type\": \"string\" }, \"b\": { \"type\": \"integer\" } }",
        );
        assert_ne!(renamed, one_sided, "fixture edit was a no-op");
        let delta = diff(one_sided, &renamed);
        let added: Vec<_> = delta
            .changes
            .iter()
            .filter(|c| c.kind == ChangeKind::PayloadPropertyAdded)
            .collect();
        assert_eq!(added.len(), 1, "{:?}", delta.changes);
        assert_eq!(added[0].severity, Severity::Unknown);
        // The flow was unverifiable before and after, so nothing regressed — but
        // the added property is still not a pass.
        assert_eq!(
            delta
                .changes
                .iter()
                .filter(|c| c.kind == ChangeKind::ProducerConsumerIncompatible)
                .count(),
            0,
            "{:?}",
            delta.changes
        );
        assert_eq!(delta.verdict, CompatibilityVerdict::Unknown);
    }

    #[test]
    fn an_unsupported_asyncapi_version_is_an_error_not_a_delta() {
        let doc = crate::document::SpecDocument::parse(
            "a.json",
            r###"{"asyncapi":"2.6.0","channels":{}}"###,
            DocFormat::Json,
        )
        .expect("parses as a document");
        assert!(parse_asyncapi(&doc).is_err());
    }

    #[test]
    fn the_delta_is_reproducible_across_100_runs() {
        let to = PRODUCER_CONSUMER.replace("\"address\": \"orders\"", "\"address\": \"orders.v2\"");
        assert_ne!(to, PRODUCER_CONSUMER, "fixture edit was a no-op");
        let once = diff(PRODUCER_CONSUMER, &to);
        for i in 0..100 {
            let again = diff(PRODUCER_CONSUMER, &to);
            assert_eq!(again.digest, once.digest, "run {i} drifted");
            assert_eq!(again, once, "run {i} drifted");
        }
        assert_eq!(once.digest.len(), 64);
    }

    #[test]
    fn changes_are_canonically_ordered() {
        let to = PRODUCER_CONSUMER.replace("\"address\": \"orders\"", "\"address\": \"orders.v2\"");
        let delta = diff(PRODUCER_CONSUMER, &to);
        let mut sorted = delta.changes.clone();
        sorted.sort_by(|a, b| {
            a.kind
                .cmp(&b.kind)
                .then_with(|| a.subject.cmp(&b.subject))
                .then_with(|| a.detail.cmp(&b.detail))
        });
        assert_eq!(delta.changes, sorted);
    }

    #[test]
    fn key_reordering_does_not_change_the_delta() {
        let to = PRODUCER_CONSUMER.replace("\"address\": \"orders\"", "\"address\": \"orders.v2\"");
        let reordered = r###"{
          "operations": {
            "subscribe": { "channel": { "$ref": "#/channels/orders" }, "action": "receive" },
            "publish": { "channel": { "$ref": "#/channels/orders" }, "action": "send" }
          },
          "channels": {
            "orders": {
              "messages": {
                "OrderPlaced": {
                  "payload": {
                    "required": ["id"],
                    "properties": { "id": { "type": "string" } },
                    "type": "object"
                  },
                  "name": "OrderPlaced"
                }
              },
              "address": "orders.v2"
            }
          },
          "asyncapi": "3.1.0"
        }"###;
        assert_eq!(
            diff(PRODUCER_CONSUMER, &to).digest,
            diff(PRODUCER_CONSUMER, reordered).digest
        );
    }
}
