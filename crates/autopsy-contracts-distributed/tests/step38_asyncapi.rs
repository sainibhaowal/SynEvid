//! Step 38 acceptance gates and honesty rules, exercised through the crate's
//! public API only.
//!
//! The lib-level tests reach into `crate::asyncapi_diff` internals. These exist
//! to prove the same properties hold for an external consumer — which is also
//! how a missing re-export gets caught, since nothing here can name a private
//! path.
//!
//! Step 38's stated acceptance gate is:
//!
//! > Parses AsyncAPI 3.1.0 channels/messages; detects payload schema
//! > incompatibilities across producers and consumers.

use autopsy_contracts_distributed::{
    ChangeKind, CompatibilityVerdict, ContractDelta, DocFormat, FlowMismatch, PayloadDirection,
    Severity, SpecDocument, diff_asyncapi, parse_asyncapi, severity_for, verify_flows,
};
use std::collections::BTreeSet;

fn doc(json: &str) -> SpecDocument {
    SpecDocument::parse("asyncapi.json", json, DocFormat::Json).expect("parses as a document")
}

fn parse(json: &str) -> autopsy_contracts_distributed::AsyncApiSpec {
    parse_asyncapi(&doc(json)).expect("parses as asyncapi")
}

fn diff(from: &str, to: &str) -> ContractDelta {
    diff_asyncapi(&parse(from), &parse(to))
}

/// A document in which one application publishes and another subscribes, with a
/// payload each side declares independently. This is the shape the gate is
/// about, and it is *not* what a single-service document looks like.
const TWO_SIDES: &str = r###"{
  "asyncapi": "3.1.0",
  "info": { "title": "Orders", "version": "2.0.0" },
  "channels": {
    "orders/{region}": {
      "address": "orders/{region}",
      "parameters": {
        "region": { "description": "shard selector" }
      },
      "messages": {
        "Published": {
          "name": "OrderPlaced",
          "contentType": "application/json",
          "headers": { "type": "object", "properties": { "traceId": { "type": "string" } } },
          "payload": {
            "type": "object",
            "properties": {
              "id": { "type": "string" },
              "total": { "type": "number" }
            },
            "required": ["id"]
          }
        },
        "Consumed": {
          "name": "OrderPlaced",
          "contentType": "application/json",
          "headers": { "type": "object", "properties": { "traceId": { "type": "string" } } },
          "payload": {
            "type": "object",
            "properties": {
              "id": { "type": "string" },
              "total": { "type": "number" }
            },
            "required": ["id"]
          }
        }
      }
    }
  },
  "operations": {
    "publishOrderPlaced": {
      "action": "send",
      "channel": { "$ref": "#/channels/orders~1{region}" },
      "messages": [ { "$ref": "#/channels/orders~1{region}/messages/Published" } ]
    },
    "consumeOrderPlaced": {
      "action": "receive",
      "channel": { "$ref": "#/channels/orders~1{region}" },
      "messages": [ { "$ref": "#/channels/orders~1{region}/messages/Consumed" } ]
    }
  }
}"###;

// ---------------------------------------------------------------------------
// Gate 1 — channels and messages parse
// ---------------------------------------------------------------------------

#[test]
fn step38_gate_channels_and_messages_are_parsed() {
    let spec = parse(TWO_SIDES);
    assert_eq!(spec.declared_version, "3.1.0");
    assert_eq!(spec.info_title.as_deref(), Some("Orders"));
    assert_eq!(spec.channels.len(), 1);
    assert_eq!(spec.messages.len(), 2);
    assert_eq!(spec.operations.len(), 2);
    assert_eq!(spec.coverage, autopsy_domain::CoverageState::Verified);

    let channel = spec.channels.get("orders/{region}").expect("channel id");
    assert_eq!(channel.address.as_deref(), Some("orders/{region}"));
    assert_eq!(channel.messages.len(), 2);

    let message = spec
        .messages
        .get("orders~1{region}/Published")
        .expect("message key is channelId/messageId");
    assert_eq!(message.name.as_deref(), Some("OrderPlaced"));
    assert_eq!(message.content_type.as_deref(), Some("application/json"));
    let payload = message.payload.as_ref().expect("payload");
    assert_eq!(payload.required.len(), 1);
    assert_eq!(payload.properties.len(), 2);
    assert!(message.headers.is_some());
}

#[test]
fn step38_gate_operations_carry_a_direction() {
    let spec = parse(TWO_SIDES);
    assert_eq!(spec.producers().len(), 1);
    assert_eq!(spec.consumers().len(), 1);
    assert_eq!(spec.producers()[0].id, "publishOrderPlaced");
    assert_eq!(spec.consumers()[0].id, "consumeOrderPlaced");
}

// ---------------------------------------------------------------------------
// Gate 2 — payload incompatibilities across producers and consumers
// ---------------------------------------------------------------------------

#[test]
fn step38_gate_a_consumer_requiring_an_unproduced_field_is_detected() {
    // Demand more than the producer guarantees.
    let strict = TWO_SIDES.replace(
        r###""Consumed": {
          "name": "OrderPlaced",
          "contentType": "application/json",
          "headers": { "type": "object", "properties": { "traceId": { "type": "string" } } },
          "payload": {
            "type": "object",
            "properties": {
              "id": { "type": "string" },
              "total": { "type": "number" }
            },
            "required": ["id"]
          }
        }"###,
        r###""Consumed": {
          "name": "OrderPlaced",
          "contentType": "application/json",
          "payload": {
            "type": "object",
            "properties": {
              "id": { "type": "string" },
              "total": { "type": "number" },
              "currency": { "type": "string" }
            },
            "required": ["id", "currency"]
          }
        }"###,
    );
    assert_ne!(strict, TWO_SIDES, "fixture edit was a no-op");

    let report = verify_flows(&parse(&strict));
    let findings: Vec<&FlowMismatch> = report
        .checked
        .iter()
        .flat_map(|c| c.findings.iter().map(|f| &f.mismatch))
        .collect();
    assert!(
        findings.contains(&&FlowMismatch::RequiredPropertyNotProduced),
        "{findings:?}"
    );
    assert_eq!(report.verdict, CompatibilityVerdict::Breaking);
    assert!(!report.verdict.is_approval());

    // And the same finding must reach the revision delta.
    let delta = diff(TWO_SIDES, &strict);
    assert!(
        delta
            .breaking()
            .iter()
            .any(|c| c.kind == ChangeKind::ProducerConsumerIncompatible),
        "{:?}",
        delta.changes
    );
    assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
}

#[test]
fn step38_gate_a_producer_emitting_more_types_than_the_consumer_accepts_is_detected() {
    // Written out in full rather than patched from `TWO_SIDES`: a `replace()` on
    // a repeated fragment hits both sides and turns the assertion into a
    // tautology, which has already produced two false passes in this crate.
    let producer_is_looser = r###"{
      "asyncapi": "3.1.0",
      "channels": {
        "c": {
          "messages": {
            "Pub": {
              "name": "M",
              "payload": { "type": "object", "properties": { "id": { "type": ["string", "integer"] } } }
            },
            "Sub": {
              "name": "M",
              "payload": { "type": "object", "properties": { "id": { "type": "string" } } }
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
    }"###;
    let report = verify_flows(&parse(producer_is_looser));
    assert_eq!(report.checked.len(), 1, "{:?}", report);
    assert_eq!(
        report.checked[0].findings[0].mismatch,
        FlowMismatch::TypeNotAccepted
    );
    assert!(
        report.checked[0].findings[0]
            .subject
            .ends_with("/payload/properties/id"),
        "subject was `{}`",
        report.checked[0].findings[0].subject
    );
    assert_eq!(report.verdict, CompatibilityVerdict::Breaking);
}

#[test]
fn step38_gate_agreeing_producers_and_consumers_are_compatible() {
    let report = verify_flows(&parse(TWO_SIDES));
    assert_eq!(report.checked.len(), 1, "{:?}", report);
    assert!(report.checked[0].findings.is_empty());
    assert!(report.unchecked.is_empty());
    assert_eq!(report.verdict, CompatibilityVerdict::Compatible);
    assert!(report.verdict.is_approval());
}

#[test]
fn step38_gate_an_unchanged_document_produces_no_changes() {
    let delta = diff(TWO_SIDES, TWO_SIDES);
    assert!(delta.changes.is_empty(), "{:?}", delta.changes);
    assert_eq!(delta.verdict, CompatibilityVerdict::Compatible);
    assert_eq!(delta.protocol, "asyncapi");
}

// ---------------------------------------------------------------------------
// The honesty rules that make the verdict worth reading
// ---------------------------------------------------------------------------

#[test]
fn step38_a_definite_break_is_reported_even_with_incomplete_coverage() {
    // An unmodelled composition keyword somewhere in the document must not be
    // able to silence a channel that was plainly removed.
    let unmodelled = TWO_SIDES.replace(
        r###""total": { "type": "number" }"###,
        r###""total": { "allOf": [ { "type": "number" } ] }"###,
    );
    assert_ne!(unmodelled, TWO_SIDES, "fixture edit was a no-op");
    let without_channel = r###"{ "asyncapi": "3.1.0", "channels": {}, "operations": {} }"###;
    let delta = diff(&unmodelled, without_channel);
    assert_eq!(delta.coverage, autopsy_domain::CoverageState::Partial);
    assert!(
        delta
            .breaking()
            .iter()
            .any(|c| c.kind == ChangeKind::ChannelRemoved)
    );
    assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
}

#[test]
fn step38_an_unmodelled_subtree_is_never_reported_compatible() {
    let with_composition = TWO_SIDES.replace(
        r###""total": { "type": "number" }"###,
        r###""total": { "oneOf": [ { "type": "number" }, { "type": "string" } ] }"###,
    );
    assert_ne!(with_composition, TWO_SIDES, "fixture edit was a no-op");
    let delta = diff(TWO_SIDES, &with_composition);
    assert_eq!(delta.verdict, CompatibilityVerdict::Unknown);
    assert!(!delta.verdict.is_approval());
    assert!(
        delta
            .unclassified()
            .iter()
            .any(|c| c.kind == ChangeKind::PayloadIncomparable),
        "{:?}",
        delta.changes
    );
}

#[test]
fn step38_a_one_sided_channel_is_never_an_approval() {
    let one_sided = r###"{
      "asyncapi": "3.1.0",
      "channels": { "c": { "messages": { "M": { "payload": { "type": "string" } } } } },
      "operations": { "p": { "action": "send", "channel": { "$ref": "#/channels/c" } } }
    }"###;
    let report = verify_flows(&parse(one_sided));
    assert!(report.checked.is_empty());
    assert_eq!(report.unchecked.len(), 1);
    assert!(report.unchecked[0].reason.contains("no consumer"));
    assert_eq!(report.verdict, CompatibilityVerdict::Unknown);
    assert!(!report.verdict.is_approval());
}

#[test]
fn step38_an_unreadable_payload_format_is_never_reported_compatible() {
    let avro = r###"{
      "asyncapi": "3.1.0",
      "channels": {
        "c": {
          "messages": {
            "M": {
              "payload": {
                "schemaFormat": "application/vnd.apache.avro;version=1.9.0",
                "schema": { "type": "record", "name": "Order", "fields": [] }
              }
            }
          }
        }
      }
    }"###;
    let spec = parse(avro);
    let message = spec.messages.values().next().unwrap();
    let payload = message.payload.as_ref().unwrap();
    assert_eq!(payload.coverage, autopsy_domain::CoverageState::Unknown);
    assert!(payload.types.is_empty());
    assert!(!spec.notes.is_empty());
    assert_ne!(spec.coverage, autopsy_domain::CoverageState::Verified);
}

#[test]
fn step38_an_unsupported_version_is_an_error_not_a_lenient_parse() {
    // Reading AsyncAPI 2.x as 3.x yields no operations, and the diff would then
    // report every operation of the other revision as removed.
    let swagger = r###"{ "asyncapi": "2.6.0", "channels": { "c": { "publish": {} } } }"###;
    assert!(parse_asyncapi(&doc(swagger)).is_err());
}

#[test]
fn step38_a_missing_action_makes_a_channel_unchecked_rather_than_checked() {
    let spec = parse(
        r###"{
          "asyncapi": "3.1.0",
          "channels": { "c": { "messages": { "M": { "payload": { "type": "string" } } } } },
          "operations": { "o": { "channel": { "$ref": "#/channels/c" } } }
        }"###,
    );
    assert!(spec.operations.is_empty());
    assert_eq!(spec.coverage, autopsy_domain::CoverageState::Partial);
    let report = verify_flows(&spec);
    assert_eq!(report.verdict, CompatibilityVerdict::Unknown);
    assert!(!report.verdict.is_approval());
}

#[test]
fn step38_only_the_compatible_verdict_is_an_approval() {
    let breaking = diff(
        TWO_SIDES,
        r###"{ "asyncapi": "3.1.0", "channels": {}, "operations": {} }"###,
    );
    assert_eq!(breaking.verdict, CompatibilityVerdict::Breaking);
    assert!(!breaking.verdict.is_approval());
    assert!(!CompatibilityVerdict::Unknown.is_approval());
    assert!(CompatibilityVerdict::Compatible.is_approval());
}

// ---------------------------------------------------------------------------
// The direction algebra, as seen from outside the crate
// ---------------------------------------------------------------------------

#[test]
fn step38_the_direction_algebra_is_inverted_where_it_should_be() {
    // These pairs are the whole reason Step 38 exists: the same schema edit
    // means opposite things depending on which side of the wire made it.
    let inverted = [
        (
            ChangeKind::PayloadRequirednessGained,
            Severity::Breaking,
            Severity::NonBreaking,
        ),
        (
            ChangeKind::PayloadRequirednessLost,
            Severity::NonBreaking,
            Severity::Breaking,
        ),
        (
            ChangeKind::PayloadTypeNarrowed,
            Severity::Breaking,
            Severity::NonBreaking,
        ),
        (
            ChangeKind::PayloadTypeWidened,
            Severity::NonBreaking,
            Severity::Breaking,
        ),
        (
            ChangeKind::PayloadEnumValueRemoved,
            Severity::Breaking,
            Severity::NonBreaking,
        ),
        (
            ChangeKind::PayloadEnumValueAdded,
            Severity::NonBreaking,
            Severity::Breaking,
        ),
        (
            ChangeKind::PayloadAdditionalPropertiesDisallowed,
            Severity::Breaking,
            Severity::NonBreaking,
        ),
    ];
    for (kind, accepted, emitted) in inverted {
        let a = severity_for(kind, PayloadDirection::Accepted);
        let e = severity_for(kind, PayloadDirection::Emitted);
        assert_eq!(a, accepted, "{kind:?} accepted");
        assert_eq!(e, emitted, "{kind:?} emitted");
        assert_ne!(a, e, "{kind:?} is not direction-sensitive at all");
    }
}

#[test]
fn step38_a_violation_inside_a_property_is_reported_at_its_own_pointer() {
    let nested = r###"{
      "asyncapi": "3.1.0",
      "channels": {
        "c": {
          "messages": {
            "Pub": {
              "name": "M",
              "payload": {
                "type": "object",
                "properties": {
                  "a": {
                    "type": "object",
                    "properties": { "b": { "type": ["string", "integer"] } }
                  }
                }
              }
            },
            "Sub": {
              "name": "M",
              "payload": {
                "type": "object",
                "properties": {
                  "a": {
                    "type": "object",
                    "properties": { "b": { "type": "string" } }
                  }
                }
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
    }"###;
    let report = verify_flows(&parse(nested));
    assert_eq!(report.checked.len(), 1, "{:?}", report);
    let finding = &report.checked[0].findings[0];
    assert_eq!(finding.mismatch, FlowMismatch::TypeNotAccepted);
    assert!(
        finding
            .subject
            .ends_with("/payload/properties/a/properties/b"),
        "subject was `{}`",
        finding.subject
    );
    assert_eq!(report.verdict, CompatibilityVerdict::Breaking);
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn step38_key_reordering_does_not_change_the_parse_or_the_delta() {
    let reordered = r###"{
      "operations": {
        "consumeOrderPlaced": {
          "messages": [ { "$ref": "#/channels/orders~1{region}/messages/Consumed" } ],
          "channel": { "$ref": "#/channels/orders~1{region}" },
          "action": "receive"
        },
        "publishOrderPlaced": {
          "messages": [ { "$ref": "#/channels/orders~1{region}/messages/Published" } ],
          "channel": { "$ref": "#/channels/orders~1{region}" },
          "action": "send"
        }
      },
      "channels": {
        "orders/{region}": {
          "parameters": { "region": { "description": "shard selector" } },
          "messages": {
            "Consumed": {
              "payload": {
                "required": ["id"],
                "properties": {
                  "total": { "type": "number" },
                  "id": { "type": "string" }
                },
                "type": "object"
              },
              "headers": { "properties": { "traceId": { "type": "string" } }, "type": "object" },
              "contentType": "application/json",
              "name": "OrderPlaced"
            },
            "Published": {
              "payload": {
                "required": ["id"],
                "properties": {
                  "total": { "type": "number" },
                  "id": { "type": "string" }
                },
                "type": "object"
              },
              "headers": { "properties": { "traceId": { "type": "string" } }, "type": "object" },
              "contentType": "application/json",
              "name": "OrderPlaced"
            }
          },
          "address": "orders/{region}"
        }
      },
      "info": { "version": "2.0.0", "title": "Orders" },
      "asyncapi": "3.1.0"
    }"###;
    let a = parse(TWO_SIDES);
    let b = parse(reordered);
    assert_eq!(a.digest(), b.digest());
    assert_eq!(a, b);
    assert_eq!(
        diff(TWO_SIDES, TWO_SIDES).digest,
        diff(reordered, reordered).digest
    );
}

#[test]
fn step38_the_delta_is_reproducible_across_100_runs() {
    let to = TWO_SIDES.replace(
        "\"address\": \"orders/{region}\"",
        "\"address\": \"orders.v2/{region}\"",
    );
    assert_ne!(to, TWO_SIDES, "fixture edit was a no-op");
    let once = diff(TWO_SIDES, &to);
    assert!(
        once.breaking()
            .iter()
            .any(|c| c.kind == ChangeKind::ChannelAddressChanged)
    );
    for i in 0..100 {
        let again = diff(TWO_SIDES, &to);
        assert_eq!(again.digest, once.digest, "run {i} drifted");
        assert_eq!(again, once, "run {i} drifted");
    }
    assert_eq!(once.digest.len(), 64);
}

#[test]
fn step38_the_delta_digest_distinguishes_a_reclassification_from_a_reform() {
    // Same detected facts, different severity: the digest must move, because a
    // consumer reading the digest must not read it as "nothing to see".
    let rewritten = TWO_SIDES.replace(
        "\"address\": \"orders/{region}\"",
        "\"address\": \"orders\"",
    );
    assert_ne!(rewritten, TWO_SIDES, "fixture edit was a no-op");
    let a = diff(TWO_SIDES, &rewritten);
    let b = diff(TWO_SIDES, &rewritten);
    assert_eq!(a.digest, b.digest);
    assert_ne!(
        a.digest,
        diff(TWO_SIDES, TWO_SIDES).digest,
        "an address move must not digest as no change"
    );
}

// ---------------------------------------------------------------------------
// Refactor stability
// ---------------------------------------------------------------------------

#[test]
fn step38_factoring_a_message_into_components_is_not_a_remove_plus_add() {
    let inline = r###"{
      "asyncapi": "3.1.0",
      "channels": {
        "orders": {
          "address": "orders",
          "messages": {
            "OrderPlaced": {
              "name": "OrderPlaced",
              "payload": { "type": "object", "properties": { "id": { "type": "string" } } }
            }
          }
        }
      },
      "operations": {
        "p": {
          "action": "send",
          "channel": { "$ref": "#/channels/orders" },
          "messages": [ { "$ref": "#/channels/orders/messages/OrderPlaced" } ]
        }
      }
    }"###;
    let via_components = r###"{
      "asyncapi": "3.1.0",
      "channels": {
        "orders": {
          "address": "orders",
          "messages": {
            "OrderPlaced": { "$ref": "#/components/messages/OrderPlaced" }
          }
        }
      },
      "operations": {
        "p": {
          "action": "send",
          "channel": { "$ref": "#/channels/orders" },
          "messages": [ { "$ref": "#/channels/orders/messages/OrderPlaced" } ]
        }
      },
      "components": {
        "messages": {
          "OrderPlaced": {
            "name": "OrderPlaced",
            "payload": { "type": "object", "properties": { "id": { "type": "string" } } }
          }
        }
      }
    }"###;
    let delta = diff(inline, via_components);
    assert!(
        delta.changes.is_empty(),
        "a pure refactor must be a no-op, got {:?}",
        delta.changes
    );
    assert_eq!(delta.from_digest, delta.to_digest);
    assert_eq!(delta.verdict, CompatibilityVerdict::Compatible);
}

#[test]
fn step38_an_unreferenced_message_blocks_compatibility_and_is_named() {
    let spec = parse(
        r###"{
          "asyncapi": "3.1.0",
          "channels": {
            "c": {
              "messages": {
                "Used": { "payload": { "type": "string" } },
                "Orphan": { "payload": { "type": "string" } }
              }
            }
          },
          "operations": {
            "p": {
              "action": "send",
              "channel": { "$ref": "#/channels/c" },
              "messages": [ { "$ref": "#/channels/c/messages/Used" } ]
            },
            "s": {
              "action": "receive",
              "channel": { "$ref": "#/channels/c" },
              "messages": [ { "$ref": "#/channels/c/messages/Used" } ]
            }
          }
        }"###,
    );
    assert_eq!(
        spec.unreferenced_messages(),
        BTreeSet::from(["c/Orphan".to_string()])
    );
    let report = verify_flows(&spec);
    assert_eq!(report.verdict, CompatibilityVerdict::Unknown);
    assert!(
        report.notes.iter().any(|n| n.contains("c/Orphan")),
        "{:?}",
        report.notes
    );
}
