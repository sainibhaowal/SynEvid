//! AsyncAPI 3.1.0 contract model (Step 38).
//!
//! Parses `channels`, `operations`, and `messages` into a canonical
//! `BTreeMap`-ordered model with local `$ref` resolution, keeping messages in a
//! catalogue keyed by `channelId/messageId`.
//!
//! # Why the direction is a first-class field
//!
//! An HTTP request has one arrow: the client sends, the server accepts. A
//! message broker has two, and which one you are looking at is decided by the
//! operation's `action`, not by the schema:
//!
//! * `action: send` — this application **produces** the message, so its payload
//!   schema describes what it *emits*. Consumers on the other end must accept
//!   what it emits.
//! * `action: receive` — this application **consumes** the message, so its
//!   payload schema describes what it *accepts*.
//!
//! Severity therefore depends on direction in a way Step 37's HTTP model never
//! had to consider. Adding an *optional* property to an accepted schema is a
//! promise the service now keeps; adding it to an emitted schema is a value
//! consumers may never have seen. The two are classified by opposite rules —
//! see [`crate::asyncapi_diff::severity_for`].
//!
//! # Identity across revisions
//!
//! A message is keyed by its **position** (`channelId/messageId`) rather than by
//! the JSON pointer it resolved to. The two differ exactly when a message body
//! is factored out into `components/messages`: the position survives that
//! refactor, the pointer does not. Keying on the pointer would report a pure
//! refactor as "message removed, message added".
//!
//! # What is deliberately not modelled
//! Payload and header schemas go through the shared [`SchemaReader`], so all of
//! its disclosures apply. On top of those:
//!
//! * `traits` are **not** applied. A message trait may inject `payload` or
//!   `headers`, so a document using them is degraded rather than read as if the
//!   trait contributed nothing.
//! * `reply` (request/reply operations) is recorded and degraded, not followed.
//! * Non-JSON Schema payload formats (Avro, Protobuf) are recorded by pointer and
//!   degraded. They are real contracts, but this engine cannot read them, and
//!   guessing would be worse than saying so.
//! * `bindings`, `tags`, `security`, `servers`, and `externalDocs` do **not**
//!   degrade coverage: they carry no payload facts, so they cannot hide an
//!   incompatibility. Treating them as gaps would make every realistic document
//!   permanently `Unknown` and teach users to ignore the signal.

use crate::document::{SpecDocument, pointer_of, type_name, worse_of};
use crate::schema::{SchemaFacts, SchemaReader, escape, major_minor, unescape};
use autopsy_domain::CoverageState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// Version of AsyncAPI this engine reads natively.
pub const TARGET_ASYNCAPI_MINOR: &str = "3.1";

/// Which side of the wire an operation puts the application on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// The application publishes to this channel.
    Send,
    /// The application subscribes to this channel.
    Receive,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Receive => "receive",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "send" => Some(Self::Send),
            "receive" => Some(Self::Receive),
            _ => None,
        }
    }

    /// True when the application is the producer for the operation's channel.
    pub fn produces(self) -> bool {
        self == Self::Send
    }
}

/// The two arrows a message can travel on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayloadDirection {
    /// The application produces it: the schema says what it will emit.
    Emitted,
    /// The application consumes it: the schema says what it will accept.
    Accepted,
    /// The same message is both sent and received by this document. A change
    /// must then be safe in *both* directions at once, which is much stronger
    /// than being safe in either.
    Both,
}

/// A message as it appears on a channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// `channelId/messageId`, with both segments escaped.
    pub key: String,
    /// The JSON pointer the message body ultimately resolved to. Reported so a
    /// user can navigate to it; not used for identity.
    pub pointer: String,
    pub name: Option<String>,
    pub content_type: Option<String>,
    /// `None` means the document declares no payload. That is different from a
    /// declared-but-unreadable payload, which is `Some` with degraded coverage.
    pub payload: Option<SchemaFacts>,
    pub headers: Option<SchemaFacts>,
    pub coverage: CoverageState,
}

impl Message {
    /// Coverage of the message's body schemas only.
    fn body_coverage(&self) -> CoverageState {
        let mut worst = CoverageState::Verified;
        if let Some(p) = &self.payload {
            worst = worse_of(worst, p.coverage);
        }
        if let Some(h) = &self.headers {
            worst = worse_of(worst, h.coverage);
        }
        worst
    }
}

/// A channel: an address plus the messages that travel over it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    /// The map key from `/channels`.
    pub id: String,
    /// The wire address (topic, queue, routing key). `None` when the document
    /// sets it to `null`, which AsyncAPI 3.0 permits.
    pub address: Option<String>,
    pub messages: BTreeSet<String>,
    pub coverage: CoverageState,
}

/// An operation: this application's relationship to a channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsyncOperation {
    /// The map key from `/operations`.
    pub id: String,
    pub action: Action,
    /// Channel **id**, not address.
    pub channel: String,
    pub pointer: String,
    /// Message keys this operation is stated to apply to.
    pub messages: BTreeSet<String>,
    pub coverage: CoverageState,
}

/// A canonical AsyncAPI contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsyncApiSpec {
    pub path: String,
    pub declared_version: String,
    pub info_title: Option<String>,
    pub info_version: Option<String>,
    pub default_content_type: Option<String>,
    pub channels: BTreeMap<String, Channel>,
    pub operations: BTreeMap<String, AsyncOperation>,
    pub messages: BTreeMap<String, Message>,
    pub coverage: CoverageState,
    pub unmodeled_pointers: BTreeSet<String>,
    pub unresolved_refs: BTreeSet<String>,
    pub notes: Vec<String>,
}

impl AsyncApiSpec {
    /// Operations that publish to their channel, in canonical order.
    pub fn producers(&self) -> Vec<&AsyncOperation> {
        self.operations
            .values()
            .filter(|o| o.action.produces())
            .collect()
    }

    /// Operations that subscribe to their channel, in canonical order.
    pub fn consumers(&self) -> Vec<&AsyncOperation> {
        self.operations
            .values()
            .filter(|o| !o.action.produces())
            .collect()
    }

    /// Which way the message travels, or `None` when nothing references it.
    ///
    /// `None` is not `Accepted`: an unreferenced message has no direction, and
    /// classifying it as one would be an invention.
    pub fn message_direction(&self, key: &str) -> Option<PayloadDirection> {
        let mut emitted = false;
        let mut accepted = false;
        for op in self.operations.values() {
            if !op.messages.contains(key) {
                continue;
            }
            if op.action.produces() {
                emitted = true;
            } else {
                accepted = true;
            }
        }
        match (emitted, accepted) {
            (true, true) => Some(PayloadDirection::Both),
            (true, false) => Some(PayloadDirection::Emitted),
            (false, true) => Some(PayloadDirection::Accepted),
            (false, false) => None,
        }
    }

    /// Messages the model holds that no operation references.
    pub fn unreferenced_messages(&self) -> BTreeSet<String> {
        self.messages
            .keys()
            .filter(|k| self.message_direction(k).is_none())
            .cloned()
            .collect()
    }

    /// BLAKE3 over the canonical contract surface.
    ///
    /// Built from the resolved model, not the source document, so a document
    /// that differs only in `$ref` layout or key order digests identically.
    pub fn digest(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.declared_version.as_bytes());
        hasher.update(b"\n");
        for (id, channel) in &self.channels {
            hasher.update(b"channel:");
            hasher.update(id.as_bytes());
            hasher.update(b"=");
            hasher.update(channel.address.as_deref().unwrap_or("\u{0}").as_bytes());
            hasher.update(b"\n");
            for key in &channel.messages {
                hasher.update(b"  msg:");
                hasher.update(key.as_bytes());
                hasher.update(b"\n");
            }
        }
        for (id, op) in &self.operations {
            hasher.update(b"op:");
            hasher.update(id.as_bytes());
            hasher.update(b"=");
            hasher.update(op.action.as_str().as_bytes());
            hasher.update(b"@");
            hasher.update(op.channel.as_bytes());
            hasher.update(b"\n");
            for key in &op.messages {
                hasher.update(b"  uses:");
                hasher.update(key.as_bytes());
                hasher.update(b"\n");
            }
        }
        for (key, message) in &self.messages {
            hasher.update(b"message:");
            hasher.update(key.as_bytes());
            hasher.update(b"\n");
            if let Some(name) = &message.name {
                hasher.update(b"  name:");
                hasher.update(name.as_bytes());
                hasher.update(b"\n");
            }
            if let Some(ct) = &message.content_type {
                hasher.update(b"  ct:");
                hasher.update(ct.as_bytes());
                hasher.update(b"\n");
            }
            if let Some(payload) = &message.payload {
                hasher.update(b"  payload:");
                hasher.update(payload.fingerprint().as_bytes());
                hasher.update(b"\n");
            }
            if let Some(headers) = &message.headers {
                hasher.update(b"  headers:");
                hasher.update(headers.fingerprint().as_bytes());
                hasher.update(b"\n");
            }
        }
        hasher.finalize().to_hex().to_string()
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AsyncApiError {
    #[error("`{path}` is not an AsyncAPI document: no `asyncapi` version string")]
    MissingVersion { path: String },
    #[error(
        "`{path}` declares AsyncAPI `{found}`; this engine reads 3.x. AsyncAPI 2.x uses \
         `publish`/`subscribe` on channels rather than operations, so parsing it here would \
         report every channel of the other revision as removed"
    )]
    UnsupportedVersion { path: String, found: String },
    #[error("`{path}`: `{pointer}` is {found}, expected {expected}")]
    WrongShape {
        path: String,
        pointer: String,
        found: &'static str,
        expected: &'static str,
    },
}

/// Is a `schemaFormat` string something this engine can read as JSON Schema?
///
/// Avro and Protobuf payloads are real contracts, but reading their field names
/// out of a JSON-Schema-shaped reader would produce confident nonsense.
fn format_is_readable(format: &str) -> bool {
    let lower = format.to_ascii_lowercase();
    lower.contains("aai.asyncapi") || lower.contains("json") || lower.contains("openapi")
}

/// Parses an AsyncAPI document into a canonical contract.
pub fn parse_asyncapi(doc: &SpecDocument) -> Result<AsyncApiSpec, AsyncApiError> {
    let path = doc.path.clone();

    let version = match doc.pointer("asyncapi") {
        Some(Value::String(v)) => v.clone(),
        Some(other) => {
            return Err(AsyncApiError::WrongShape {
                path,
                pointer: "/asyncapi".to_string(),
                found: type_name(other),
                expected: "a version string",
            });
        }
        None => {
            return Err(AsyncApiError::MissingVersion { path });
        }
    };

    // 2.x is refused rather than parsed leniently. Its channel model is
    // `publish`/`subscribe` and its operations live inside channels, so a
    // lenient 3.x read yields an empty operation set — and the diff then reports
    // every operation of the other revision as removed.
    if !version.starts_with('3') {
        return Err(AsyncApiError::UnsupportedVersion {
            path,
            found: version,
        });
    }

    let mut notes: Vec<String> = Vec::new();
    let mut coverage = CoverageState::Verified;
    match major_minor(&version).as_deref() {
        Some(TARGET_ASYNCAPI_MINOR) => {}
        Some("3.0") => {
            coverage = worse_of(coverage, CoverageState::Partial);
            notes.push(
                "document declares AsyncAPI 3.0.x; parsed under 3.1 semantics and 3.0-to-3.1 \
                 differences are not modelled"
                    .to_string(),
            );
        }
        _ => {
            coverage = worse_of(coverage, CoverageState::Partial);
            notes.push(format!(
                "document declares AsyncAPI `{version}`, whose minor version could not be read; \
                 parsed under 3.1 semantics"
            ));
        }
    }

    let info_title = doc
        .pointer("/info/title")
        .and_then(Value::as_str)
        .map(str::to_string);
    let info_version = doc
        .pointer("/info/version")
        .and_then(Value::as_str)
        .map(str::to_string);
    let default_content_type = doc
        .pointer("/defaultContentType")
        .and_then(Value::as_str)
        .map(str::to_string);

    let mut messages: BTreeMap<String, Message> = BTreeMap::new();
    let mut channels: BTreeMap<String, Channel> = BTreeMap::new();
    let mut unresolved_refs: BTreeSet<String> = BTreeSet::new();

    // --- channels ---------------------------------------------------------
    let channel_entries: Vec<(String, Value)> = match doc.pointer("/channels") {
        Some(Value::Object(map)) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        Some(other) => {
            coverage = worse_of(coverage, CoverageState::Unknown);
            notes.push(format!(
                "`/channels` is {}; no channels could be read",
                type_name(other)
            ));
            Vec::new()
        }
        None => Vec::new(),
    };

    for (channel_id, _) in &channel_entries {
        let pointer = format!("/channels/{}", escape(channel_id));
        let mut local = CoverageState::Verified;
        let mut address: Option<String> = None;
        let mut channel_messages: BTreeSet<String> = BTreeSet::new();

        let Some(node) = doc.pointer(&pointer) else {
            continue;
        };
        let mut reader = SchemaReader::new(doc);
        let Some((node, resolved)) = reader.resolve_node(node, &pointer) else {
            unresolved_refs.extend(reader.unresolved().iter().cloned());
            unresolved_refs.insert(pointer.clone());
            // The channel exists by id but its body is unreachable, so we know
            // it was there and nothing about it. Zero messages is *not* the same
            // statement as "this channel has no messages" once coverage says so.
            coverage = worse_of(coverage, CoverageState::Unknown);
            for note in reader.notes() {
                notes.push(note.clone());
            }
            notes.push(format!(
                "channel `{channel_id}` could not be resolved; its messages are unknown"
            ));
            channels.insert(
                channel_id.clone(),
                Channel {
                    id: channel_id.clone(),
                    address: None,
                    messages: BTreeSet::new(),
                    coverage: CoverageState::Unknown,
                },
            );
            continue;
        };

        match node.get("address") {
            Some(Value::String(a)) => {
                address = Some(a.clone());
                let declared: BTreeSet<String> = match node.get("parameters") {
                    Some(Value::Object(params)) => params.keys().cloned().collect(),
                    Some(other) => {
                        local = worse_of(local, CoverageState::Partial);
                        notes.push(format!(
                            "channel `{channel_id}`: `parameters` is {}",
                            type_name(other)
                        ));
                        BTreeSet::new()
                    }
                    None => BTreeSet::new(),
                };
                let used = placeholders(a);
                let undeclared: Vec<&String> = used.difference(&declared).collect();
                let unused: Vec<&String> = declared.difference(&used).collect();
                if !undeclared.is_empty() || !unused.is_empty() {
                    // AsyncAPI requires the two to correspond exactly, and a
                    // mismatch means the address as declared is not the address
                    // that will be used.
                    local = worse_of(local, CoverageState::Partial);
                    if !undeclared.is_empty() {
                        notes.push(format!(
                            "channel `{channel_id}`: address uses parameters that are not \
                             declared: {undeclared:?}"
                        ));
                    }
                    if !unused.is_empty() {
                        notes.push(format!(
                            "channel `{channel_id}`: declared parameters do not appear in the \
                             address: {unused:?}"
                        ));
                    }
                }
            }
            Some(Value::Null) | None => {}
            Some(other) => {
                local = worse_of(local, CoverageState::Partial);
                notes.push(format!(
                    "channel `{channel_id}`: `address` is {}",
                    type_name(other)
                ));
            }
        }

        match node.get("messages") {
            Some(Value::Object(map)) => {
                for (message_id, _) in map {
                    let message_pointer = format!("{resolved}/messages/{}", escape(message_id));
                    let key = format!("{}/{}", escape(channel_id), escape(message_id));
                    match doc.pointer(&message_pointer) {
                        Some(value) => {
                            let message = parse_message(
                                doc,
                                &key,
                                &message_pointer,
                                value,
                                default_content_type.as_deref(),
                                &mut notes,
                                &mut unresolved_refs,
                            );
                            local = worse_of(local, message.coverage);
                            channel_messages.insert(key.clone());
                            messages.insert(key, message);
                        }
                        None => {
                            local = worse_of(local, CoverageState::Unknown);
                            notes.push(format!(
                                "channel `{channel_id}`: message `{message_id}` resolved to \
                                 `{message_pointer}`, which does not exist"
                            ));
                        }
                    }
                }
            }
            Some(other) => {
                local = worse_of(local, CoverageState::Partial);
                notes.push(format!(
                    "channel `{channel_id}`: `messages` is {}",
                    type_name(other)
                ));
            }
            None => {}
        }

        coverage = worse_of(coverage, local);
        channels.insert(
            channel_id.clone(),
            Channel {
                id: channel_id.clone(),
                address,
                messages: channel_messages,
                coverage: local,
            },
        );
    }

    // --- operations -------------------------------------------------------
    let mut operations: BTreeMap<String, AsyncOperation> = BTreeMap::new();
    let operation_entries: Vec<(String, Value)> = match doc.pointer("/operations") {
        Some(Value::Object(map)) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        Some(other) => {
            coverage = worse_of(coverage, CoverageState::Unknown);
            notes.push(format!(
                "`/operations` is {}; no operations could be read",
                type_name(other)
            ));
            Vec::new()
        }
        None => Vec::new(),
    };

    for (operation_id, _) in &operation_entries {
        let pointer = format!("/operations/{}", escape(operation_id));
        let mut local = CoverageState::Verified;

        let Some(node) = doc.pointer(&pointer) else {
            continue;
        };
        let mut reader = SchemaReader::new(doc);
        let Some((node, resolved)) = reader.resolve_node(node, &pointer) else {
            unresolved_refs.extend(reader.unresolved().iter().cloned());
            unresolved_refs.insert(pointer.clone());
            coverage = worse_of(coverage, CoverageState::Unknown);
            notes.push(format!(
                "operation `{operation_id}` could not be resolved; its direction and channel are \
                 unknown"
            ));
            continue;
        };

        let action = match node.get("action").and_then(Value::as_str) {
            Some(raw) => match Action::parse(raw) {
                Some(a) => a,
                None => {
                    // Without the action there is no direction, and a severity
                    // assigned without a direction is a coin flip. Skip the
                    // operation so its channel reads as unchecked rather than
                    // as checked-and-fine.
                    coverage = worse_of(coverage, CoverageState::Partial);
                    notes.push(format!(
                        "operation `{operation_id}`: action `{raw}` is not `send` or `receive`; \
                         it contributes no producer or consumer"
                    ));
                    continue;
                }
            },
            None => {
                coverage = worse_of(coverage, CoverageState::Partial);
                notes.push(format!(
                    "operation `{operation_id}` has no `action`; it contributes no producer or \
                     consumer"
                ));
                continue;
            }
        };

        // `channel` is a reference to `/channels/{id}` in 3.x. The reference is
        // read **one hop** from its raw `$ref`, never followed to the end: a
        // channel is identified by its own pointer, so resolving further would
        // turn `#/channels/orders` into whatever that channel happens to
        // contain and lose the identity entirely.
        let channel = match node.get("channel") {
            Some(_) => {
                let reference_pointer = format!("{resolved}/channel");
                let target = doc.ref_at(&reference_pointer).map(str::to_string);
                match target {
                    Some(target) => match channel_message_from_target(&target) {
                        Some((id, None)) => Some(id),
                        Some((_, Some(_))) => {
                            local = worse_of(local, CoverageState::Partial);
                            notes.push(format!(
                                "operation `{operation_id}`: `channel` resolves to `{target}`, \
                                 which names a message rather than a channel"
                            ));
                            None
                        }
                        None => {
                            local = worse_of(local, CoverageState::Partial);
                            notes.push(format!(
                                "operation `{operation_id}`: `channel` references `{target}`, \
                                 which is outside `/channels`"
                            ));
                            None
                        }
                    },
                    None => {
                        // An inline channel object has no id to key on, so its
                        // operations cannot be lined up with a channel at all.
                        local = worse_of(local, CoverageState::Partial);
                        notes.push(format!(
                            "operation `{operation_id}`: `channel` is not a reference, so the \
                             channel it applies to has no stable identity"
                        ));
                        None
                    }
                }
            }
            None => {
                local = worse_of(local, CoverageState::Partial);
                notes.push(format!("operation `{operation_id}` has no `channel`"));
                None
            }
        };

        let Some(channel) = channel else {
            coverage = worse_of(coverage, local);
            continue;
        };

        // Absent `messages` means the operation applies to every message on the
        // channel, per AsyncAPI 3.
        let declared = node.get("messages");
        let mut used: BTreeSet<String> = BTreeSet::new();
        match declared {
            Some(Value::Array(items)) => {
                for (index, _) in items.iter().enumerate() {
                    let item_pointer = format!("{resolved}/messages/{index}");
                    // Same one-hop rule as `channel` above: the reference names
                    // a message position, and following the chain past it would
                    // replace that position with wherever the message body
                    // happens to live — which changes when a message is factored
                    // out into `components`.
                    match doc.ref_at(&item_pointer).map(str::to_string) {
                        Some(target) => match channel_message_from_target(&target) {
                            Some((id, Some(message_id))) => {
                                if id != channel {
                                    local = worse_of(local, CoverageState::Partial);
                                    notes.push(format!(
                                        "operation `{operation_id}`: message reference \
                                             `{target}` is on channel `{id}`, not `{channel}`"
                                    ));
                                }
                                used.insert(format!("{}/{}", escape(&id), escape(&message_id)));
                            }
                            Some((id, None)) => {
                                local = worse_of(local, CoverageState::Partial);
                                notes.push(format!(
                                    "operation `{operation_id}`: reference `{target}` names \
                                         the channel `{id}` rather than one of its messages"
                                ));
                            }
                            None => {
                                local = worse_of(local, CoverageState::Partial);
                                notes.push(format!(
                                    "operation `{operation_id}`: message reference \
                                         `{target}` is not a channel message reference"
                                ));
                            }
                        },
                        None => {
                            local = worse_of(local, CoverageState::Partial);
                            notes.push(format!(
                                "operation `{operation_id}`: a message entry is not a reference, \
                                 so it has no stable identity"
                            ));
                        }
                    }
                }
            }
            Some(other) => {
                local = worse_of(local, CoverageState::Partial);
                notes.push(format!(
                    "operation `{operation_id}`: `messages` is {}",
                    type_name(other)
                ));
            }
            None => {}
        }

        if used.is_empty() {
            match channels.get(&channel) {
                Some(c) if !c.messages.is_empty() => {
                    if declared.is_none() {
                        // Implicit: the operation applies to the whole channel.
                        used = c.messages.clone();
                    } else {
                        local = worse_of(local, CoverageState::Partial);
                        notes.push(format!(
                            "operation `{operation_id}` declares an empty message list; it \
                             applies to nothing"
                        ));
                    }
                }
                Some(_) => {
                    local = worse_of(local, CoverageState::Partial);
                    notes.push(format!(
                        "operation `{operation_id}` names channel `{channel}`, which has no \
                         messages"
                    ));
                }
                None => {
                    local = worse_of(local, CoverageState::Unknown);
                    notes.push(format!(
                        "operation `{operation_id}` names channel `{channel}`, which is not \
                         declared in `/channels`"
                    ));
                }
            }
        }

        // A trait can inject `payload` or `headers`, so a document using them is
        // not fully read. `reply` introduces a whole second channel/message
        // relationship that this engine does not model.
        for keyword in ["traits", "reply"] {
            if node.get(keyword).is_some() {
                local = worse_of(local, CoverageState::Partial);
                notes.push(format!(
                    "operation `{operation_id}`: `{keyword}` is present and is not modelled"
                ));
            }
        }

        coverage = worse_of(coverage, local);
        operations.insert(
            operation_id.clone(),
            AsyncOperation {
                id: operation_id.clone(),
                action,
                channel,
                pointer,
                messages: used,
                coverage: local,
            },
        );
    }

    // --- message-level trait disclosure -----------------------------------
    // Message `traits` live on the message, not the operation.
    for (key, message) in &messages {
        let pointer = format!("{}/traits", message.pointer);
        if doc.pointer(&pointer).is_some() {
            coverage = worse_of(coverage, CoverageState::Partial);
            notes.push(format!(
                "message `{key}`: `traits` is present and is not modelled; it may contribute \
                 payload or header properties"
            ));
        }
    }

    // A message that no operation references describes no wire surface, so it
    // is excluded from flow checks — but silently dropping it would hide the
    // fact that the document declares a contract nothing uses.
    let referenced: BTreeSet<&String> = operations
        .values()
        .flat_map(|op| op.messages.iter())
        .collect();
    let orphaned: Vec<&String> = messages
        .keys()
        .filter(|k| !referenced.contains(k))
        .collect();
    if !orphaned.is_empty() {
        notes.push(format!(
            "{} message(s) are declared but no operation references them: {orphaned:?}. They \
             describe no wire surface and are excluded from flow checks",
            orphaned.len()
        ));
    }

    // Collect every unmodelled pointer from the body schemas so a caller can
    // report exactly which keywords were recorded rather than interpreted.
    let mut unmodeled_pointers: BTreeSet<String> = BTreeSet::new();
    for message in messages.values() {
        for facts in [message.payload.as_ref(), message.headers.as_ref()]
            .into_iter()
            .flatten()
        {
            let mut pointers = BTreeSet::new();
            facts.unmodeled_into(&mut pointers);
            unmodeled_pointers.extend(pointers.into_iter().map(str::to_string));
        }
    }

    notes.sort();
    notes.dedup();

    Ok(AsyncApiSpec {
        path,
        declared_version: version,
        info_title,
        info_version,
        default_content_type,
        channels,
        operations,
        messages,
        coverage,
        unmodeled_pointers,
        unresolved_refs,
        notes,
    })
}

/// Splits a resolved `/channels/...` target into its channel id and message id.
///
/// Both ids are escaped, so a literal `/` inside one is `~1` and the only real
/// separator is the structural `/messages/`. Unescaping the whole tail *before*
/// splitting merges the two ids back into one — which is how a channel id of
/// `user/signedup` once turned into a message key of
/// `user/signedup/messages/M`.
fn channel_message_from_target(target: &str) -> Option<(String, Option<String>)> {
    // `pointer_of` also rejects external references and the bare `#`, both of
    // which would otherwise be accepted here as a channel named `...`.
    let pointer = pointer_of(target).ok()?;
    let rest = pointer.strip_prefix("/channels/")?;
    match rest.split_once("/messages/") {
        Some((channel, message)) => Some((unescape(channel)?, Some(unescape(message)?))),
        None => Some((unescape(rest)?, None)),
    }
}

/// Channel address placeholders, e.g. `orders/{orderId}` → `{orderId}`.
fn placeholders(address: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = address;
    while let Some(start) = rest.find('{') {
        let Some(end) = rest[start + 1..].find('}') else {
            break;
        };
        out.insert(rest[start + 1..start + 1 + end].to_string());
        rest = &rest[start + 1 + end + 1..];
    }
    out
}

fn parse_message(
    doc: &SpecDocument,
    key: &str,
    pointer: &str,
    value: &Value,
    default_content_type: Option<&str>,
    notes: &mut Vec<String>,
    unresolved_refs: &mut BTreeSet<String>,
) -> Message {
    let mut reader = SchemaReader::new(doc);
    let Some((node, resolved)) = reader.resolve_node(value, pointer) else {
        for note in reader.notes() {
            notes.push(note.clone());
        }
        unresolved_refs.extend(reader.unresolved().iter().cloned());
        unresolved_refs.insert(pointer.to_string());
        return Message {
            key: key.to_string(),
            pointer: pointer.to_string(),
            name: None,
            content_type: default_content_type.map(str::to_string),
            payload: Some(SchemaFacts::unknown(pointer)),
            headers: None,
            coverage: CoverageState::Unknown,
        };
    };

    let name = node.get("name").and_then(Value::as_str).map(str::to_string);
    let content_type = node
        .get("contentType")
        .and_then(Value::as_str)
        .or(default_content_type)
        .map(str::to_string);

    let payload = match node.get("payload") {
        None => None,
        Some(payload_value) => {
            let payload_pointer = format!("{resolved}/payload");
            // A Multi Format Schema Object carries a `schemaFormat` and the
            // schema separately; otherwise the payload *is* the schema.
            let (schema_pointer, declared_format) =
                match payload_value.get("schemaFormat").and_then(Value::as_str) {
                    // AsyncAPI 3.x: a Multi Format Schema Object carries
                    // `schemaFormat` and the schema together.
                    Some(format) if payload_value.get("schema").is_some() => (
                        format!("{payload_pointer}/schema"),
                        Some(format.to_string()),
                    ),
                    Some(format) => {
                        notes.push(format!(
                            "message `{key}`: `schemaFormat` is declared as `{format}` but there \
                             is no `schema` beside it"
                        ));
                        (payload_pointer, Some(format.to_string()))
                    }
                    // AsyncAPI 2.x placed `schemaFormat` on the message and the
                    // schema directly in `payload`. Ignoring it would read an
                    // Avro record through the JSON Schema reader and then report
                    // the message as `Verified` — a false claim of understanding.
                    None => match node.get("schemaFormat").and_then(Value::as_str) {
                        Some(format) => {
                            notes.push(format!(
                                "message `{key}`: `schemaFormat` is declared on the message \
                                 rather than inside `payload` (the AsyncAPI 2.x layout)"
                            ));
                            (payload_pointer, Some(format.to_string()))
                        }
                        None => (payload_pointer, None),
                    },
                };

            if let Some(format) = declared_format
                && !format_is_readable(&format)
            {
                notes.push(format!(
                    "message `{key}`: payload format `{format}` is not JSON Schema; the payload is \
                     recorded but not interpreted"
                ));
                reader.degrade(CoverageState::Partial);
                Some(SchemaFacts::unknown(&schema_pointer))
            } else {
                Some(reader.read(&schema_pointer))
            }
        }
    };

    let headers = if node.get("headers").is_some() {
        Some(reader.read(&format!("{resolved}/headers")))
    } else {
        None
    };

    for note in reader.notes() {
        notes.push(note.clone());
    }
    unresolved_refs.extend(reader.unresolved().iter().cloned());

    let mut message = Message {
        key: key.to_string(),
        pointer: resolved.clone(),
        name,
        content_type,
        payload,
        headers,
        coverage: CoverageState::Verified,
    };
    message.coverage = message.body_coverage();
    if node.get("traits").is_some() {
        message.coverage = worse_of(message.coverage, CoverageState::Partial);
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::DocFormat;

    fn parse(json: &str) -> AsyncApiSpec {
        let doc = SpecDocument::parse("a.json", json, DocFormat::Json).expect("parses");
        parse_asyncapi(&doc).expect("parses asyncapi")
    }

    fn error(json: &str) -> AsyncApiError {
        let doc = SpecDocument::parse("a.json", json, DocFormat::Json).expect("parses");
        parse_asyncapi(&doc).unwrap_err()
    }

    const MINIMAL: &str = r###"{
      "asyncapi": "3.1.0",
      "info": { "title": "Account", "version": "1.0.0" },
      "channels": {
        "userSignedUp": {
          "address": "user/signedup",
          "messages": {
            "UserSignedUp": {
              "name": "UserSignedUp",
              "payload": {
                "type": "object",
                "properties": { "email": { "type": "string" } },
                "required": ["email"]
              }
            }
          }
        }
      },
      "operations": {
        "sendUserSignedUp": {
          "action": "send",
          "channel": { "$ref": "#/channels/userSignedUp" },
          "messages": [
            { "$ref": "#/channels/userSignedUp/messages/UserSignedUp" }
          ]
        }
      }
    }"###;

    #[test]
    fn minimal_document_parses() {
        let spec = parse(MINIMAL);
        assert_eq!(spec.declared_version, "3.1.0");
        assert_eq!(spec.info_title.as_deref(), Some("Account"));
        assert_eq!(spec.channels.len(), 1);
        assert_eq!(spec.operations.len(), 1);
        assert_eq!(spec.messages.len(), 1);
        assert_eq!(spec.coverage, CoverageState::Verified);
        assert_eq!(spec.producers().len(), 1);
        assert_eq!(spec.consumers().len(), 0);
    }

    #[test]
    fn a_refed_message_and_an_inline_message_share_a_key_and_a_payload() {
        // The same message expressed through components must produce the same
        // key, payload fingerprint, and digest.
        let via_components = r###"{
          "asyncapi": "3.1.0",
          "channels": {
            "userSignedUp": {
              "address": "user/signedup",
              "messages": {
                "UserSignedUp": { "$ref": "#/components/messages/UserSignedUp" }
              }
            }
          },
          "operations": {
            "sendUserSignedUp": {
              "action": "send",
              "channel": { "$ref": "#/channels/userSignedUp" },
              "messages": [
                { "$ref": "#/channels/userSignedUp/messages/UserSignedUp" }
              ]
            }
          },
          "components": {
            "messages": {
              "UserSignedUp": {
                "name": "UserSignedUp",
                "payload": {
                  "type": "object",
                  "properties": { "email": { "type": "string" } },
                  "required": ["email"]
                }
              }
            }
          }
        }"###;
        let a = parse(MINIMAL);
        let b = parse(via_components);
        assert_eq!(
            a.messages.keys().collect::<Vec<_>>(),
            b.messages.keys().collect::<Vec<_>>()
        );
        let ka = a.messages.keys().next().unwrap();
        let kb = b.messages.keys().next().unwrap();
        // The resolved pointers differ; the modelled payloads do not.
        assert_ne!(a.messages[ka].pointer, b.messages[kb].pointer);
        assert_eq!(
            a.messages[ka].payload.as_ref().unwrap().fingerprint(),
            b.messages[kb].payload.as_ref().unwrap().fingerprint()
        );
        assert_eq!(a.messages[ka].name, b.messages[kb].name);
    }

    #[test]
    fn channel_ids_containing_slashes_survive_round_trip() {
        // Some documents use the raw topic as the channel *id*.
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": {
                "user/signedup": {
                  "address": "user/signedup",
                  "messages": { "M": { "payload": { "type": "string" } } }
                }
              },
              "operations": {
                "publish": {
                  "action": "send",
                  "channel": { "$ref": "#/channels/user~1signedup" },
                  "messages": [ { "$ref": "#/channels/user~1signedup/messages/M" } ]
                }
              }
            }"###,
        );
        assert_eq!(spec.channels.len(), 1);
        assert!(spec.channels.contains_key("user/signedup"));
        let op = spec.operations.values().next().unwrap();
        assert_eq!(op.channel, "user/signedup");
        let key: &String = op.messages.iter().next().unwrap();
        assert_eq!(key, "user~1signedup/M");
        assert!(spec.messages.contains_key(key));
    }

    #[test]
    fn a_channel_that_is_itself_a_ref_keeps_its_identity() {
        // Following the chain to the end turns `#/channels/orders` into whatever
        // the channel contains, and the operation then keys on nothing.
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": {
                "orders": { "$ref": "#/components/channels/orders" }
              },
              "operations": {
                "p": {
                  "action": "send",
                  "channel": { "$ref": "#/channels/orders" },
                  "messages": [ { "$ref": "#/channels/orders/messages/M" } ]
                }
              },
              "components": {
                "channels": {
                  "orders": {
                    "address": "orders",
                    "messages": {
                      "M": { "$ref": "#/components/messages/M" }
                    }
                  }
                },
                "messages": {
                  "M": { "name": "M", "payload": { "type": "string" } }
                }
              }
            }"###,
        );
        let op = spec.operations.get("p").expect("operation is kept");
        assert_eq!(op.channel, "orders");
        assert_eq!(
            op.messages.iter().cloned().collect::<Vec<_>>(),
            vec!["orders/M".to_string()]
        );
        assert_eq!(
            spec.message_direction("orders/M"),
            Some(PayloadDirection::Emitted)
        );
        assert_eq!(spec.messages.len(), 1);
        let message = spec.messages.values().next().unwrap();
        assert_eq!(message.name.as_deref(), Some("M"));
        assert!(message.payload.is_some());
    }

    #[test]
    fn asyncapi_2_is_refused_not_read_leniently() {
        // Reading 2.x as 3.x yields no operations, and a diff would then report
        // every operation of the other side as removed.
        let e = error(r###"{"asyncapi": "2.6.0", "channels": {}}"###);
        assert!(matches!(e, AsyncApiError::UnsupportedVersion { .. }));
    }

    #[test]
    fn a_patch_version_is_not_mistaken_for_a_minor_version() {
        // Reading `3.1.0` as minor `1` flagged every correct document as a
        // version this engine does not target.
        assert_eq!(major_minor("3.1.0").as_deref(), Some("3.1"));
        assert_eq!(major_minor("3.1").as_deref(), Some("3.1"));
        assert_eq!(major_minor("3.10.4").as_deref(), Some("3.10"));
        assert_eq!(major_minor("3").as_deref(), None);
        assert_eq!(major_minor("").as_deref(), None);
        let spec = parse(MINIMAL);
        assert_eq!(spec.coverage, CoverageState::Verified);
        assert!(
            !spec.notes.iter().any(|n| n.contains("minor version")),
            "notes: {:?}",
            spec.notes
        );
    }

    #[test]
    fn asyncapi_3_0_parses_with_a_partial_note() {
        let spec = parse(r###"{"asyncapi": "3.0.0", "channels": {}}"###);
        assert_eq!(spec.coverage, CoverageState::Partial);
        assert!(spec.notes.iter().any(|n| n.contains("3.0")));
    }

    #[test]
    fn a_document_without_a_version_is_an_error() {
        let e = error(r###"{"channels": {}}"###);
        assert!(matches!(e, AsyncApiError::MissingVersion { .. }));
    }

    #[test]
    fn an_absent_action_makes_the_channel_unchecked_rather_than_checked() {
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": { "c": { "messages": { "M": { "payload": {} } } } },
              "operations": { "o": { "channel": { "$ref": "#/channels/c" } } }
            }"###,
        );
        assert!(spec.operations.is_empty());
        assert_eq!(spec.coverage, CoverageState::Partial);
    }

    #[test]
    fn an_unknown_action_makes_the_channel_unchecked_rather_than_checked() {
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": { "c": { "messages": { "M": { "payload": {} } } } },
              "operations": {
                "o": { "action": "publish", "channel": { "$ref": "#/channels/c" } }
              }
            }"###,
        );
        assert!(spec.operations.is_empty());
        assert_eq!(spec.coverage, CoverageState::Partial);
    }

    #[test]
    fn an_operation_without_messages_applies_to_the_whole_channel() {
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": {
                "c": {
                  "messages": {
                    "A": { "payload": { "type": "string" } },
                    "B": { "payload": { "type": "integer" } }
                  }
                }
              },
              "operations": {
                "o": { "action": "send", "channel": { "$ref": "#/channels/c" } }
              }
            }"###,
        );
        let op = spec.operations.values().next().unwrap();
        assert_eq!(op.messages.len(), 2);
    }

    #[test]
    fn an_address_placeholder_without_a_parameter_degrades_and_says_why() {
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": {
                "c": { "address": "orders/{orderId}", "messages": {} }
              }
            }"###,
        );
        assert_eq!(spec.coverage, CoverageState::Partial);
        assert!(
            spec.notes
                .iter()
                .any(|n| n.contains("address uses parameters that are not declared")),
            "notes: {:?}",
            spec.notes
        );
    }

    #[test]
    fn a_declared_parameter_absent_from_the_address_degrades() {
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": {
                "c": {
                  "address": "orders",
                  "parameters": { "orderId": { "description": "unused" } },
                  "messages": {}
                }
              }
            }"###,
        );
        assert_eq!(spec.coverage, CoverageState::Partial);
    }

    #[test]
    fn a_non_json_payload_format_is_disclosed_not_guessed_at() {
        // Both layouts are checked, because both are found in the wild and
        // neither may be read as a JSON Schema.
        let multi_format = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": {
                "c": {
                  "messages": {
                    "M": {
                      "payload": {
                        "schemaFormat": "application/vnd.apache.avro;version=1.9.0",
                        "schema": { "type": "record", "fields": [] }
                      }
                    }
                  }
                }
              }
            }"###,
        );
        // The 2.x layout, where `schemaFormat` sits on the message.
        let message_level = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": {
                "c": {
                  "messages": {
                    "M": {
                      "schemaFormat": "application/vnd.apache.avro;version=1.9.0",
                      "payload": { "type": "record", "fields": [] }
                    }
                  }
                }
              }
            }"###,
        );

        for (label, spec) in [
            ("multi-format", multi_format),
            ("message-level", message_level),
        ] {
            let message = spec.messages.values().next().unwrap();
            let payload = message
                .payload
                .as_ref()
                .unwrap_or_else(|| panic!("{label}: payload is declared"));
            assert_eq!(payload.coverage, CoverageState::Unknown, "{label}");
            assert!(payload.types.is_empty(), "{label}");
            assert_eq!(message.coverage, CoverageState::Unknown, "{label}");
            assert!(
                spec.notes.iter().any(|n| n.contains("avro")),
                "{label}: notes: {:?}",
                spec.notes
            );
            // And the claim must not be able to reach `Verified` anywhere.
            assert_ne!(spec.coverage, CoverageState::Verified, "{label}");
        }
    }

    #[test]
    fn a_multi_format_payload_is_read_at_the_schema_child() {
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": {
                "c": {
                  "messages": {
                    "M": {
                      "payload": {
                        "schemaFormat": "application/vnd.aai.asyncapi+json;version=3.1.0",
                        "schema": {
                          "type": "object",
                          "properties": { "a": { "type": "string" } },
                          "required": ["a"]
                        }
                      }
                    }
                  }
                }
              }
            }"###,
        );
        let message = spec.messages.values().next().unwrap();
        let payload = message.payload.as_ref().unwrap();
        assert_eq!(payload.coverage, CoverageState::Verified);
        assert!(payload.properties.contains_key("a"));
        assert_eq!(payload.required.len(), 1);
    }

    #[test]
    fn a_declared_but_unreadable_payload_is_not_the_same_as_no_payload() {
        let with_payload = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": { "c": { "messages": { "M": { "payload": { "type": "string" } } } } }
            }"###,
        );
        let without_payload = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": { "c": { "messages": { "M": { "name": "M" } } } }
            }"###,
        );
        assert!(
            with_payload
                .messages
                .values()
                .next()
                .unwrap()
                .payload
                .is_some()
        );
        assert!(
            without_payload
                .messages
                .values()
                .next()
                .unwrap()
                .payload
                .is_none()
        );
    }

    #[test]
    fn operation_traits_and_reply_degrade_coverage() {
        for keyword in ["\"traits\": [{}]", "\"reply\": {}"] {
            let spec = parse(&format!(
                r###"{{
                  "asyncapi": "3.1.0",
                  "channels": {{ "c": {{ "messages": {{ "M": {{ "payload": {{ "type": "string" }} }} }} }} }},
                  "operations": {{
                    "o": {{
                      "action": "send",
                      "channel": {{ "$ref": "#/channels/c" }},
                      {keyword}
                    }}
                  }}
                }}"###
            ));
            assert_eq!(spec.coverage, CoverageState::Partial, "{keyword}");
            assert!(
                spec.notes.iter().any(|n| n.contains("not modelled")),
                "{keyword}"
            );
        }
    }

    #[test]
    fn message_traits_degrade_coverage() {
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": {
                "c": { "messages": { "M": { "payload": { "type": "string" }, "traits": [{}] } } }
              }
            }"###,
        );
        assert_eq!(spec.coverage, CoverageState::Partial);
    }

    #[test]
    fn unreferenced_messages_are_named_in_the_notes() {
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
                "o": {
                  "action": "send",
                  "channel": { "$ref": "#/channels/c" },
                  "messages": [ { "$ref": "#/channels/c/messages/Used" } ]
                }
              }
            }"###,
        );
        assert!(spec.message_direction("c/Used").is_some());
        assert!(spec.message_direction("c/Orphan").is_none());
        assert_eq!(
            spec.unreferenced_messages(),
            BTreeSet::from(["c/Orphan".to_string()])
        );
        assert!(spec.notes.iter().any(|n| n.contains("c/Orphan")));
    }

    #[test]
    fn direction_is_both_when_one_message_is_sent_and_received() {
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": {
                "c": { "messages": { "M": { "payload": { "type": "string" } } } }
              },
              "operations": {
                "out": { "action": "send", "channel": { "$ref": "#/channels/c" } },
                "in": { "action": "receive", "channel": { "$ref": "#/channels/c" } }
              }
            }"###,
        );
        assert_eq!(spec.message_direction("c/M"), Some(PayloadDirection::Both));
    }

    #[test]
    fn an_address_change_is_part_of_the_digest() {
        let a =
            parse(r###"{"asyncapi":"3.1.0","channels":{"c":{"address":"a/b","messages":{}}}}"###);
        let b =
            parse(r###"{"asyncapi":"3.1.0","channels":{"c":{"address":"a/c","messages":{}}}}"###);
        assert_ne!(a.digest(), b.digest());
    }

    #[test]
    fn key_reordering_does_not_change_the_digest() {
        let a = parse(MINIMAL);
        let reordered = r###"{
          "operations": {
            "sendUserSignedUp": {
              "messages": [ { "$ref": "#/channels/userSignedUp/messages/UserSignedUp" } ],
              "channel": { "$ref": "#/channels/userSignedUp" },
              "action": "send"
            }
          },
          "channels": {
            "userSignedUp": {
              "messages": {
                "UserSignedUp": {
                  "payload": {
                    "required": ["email"],
                    "properties": { "email": { "type": "string" } },
                    "type": "object"
                  },
                  "name": "UserSignedUp"
                }
              },
              "address": "user/signedup"
            }
          },
          "info": { "version": "1.0.0", "title": "Account" },
          "asyncapi": "3.1.0"
        }"###;
        let b = parse(reordered);
        assert_eq!(a.digest(), b.digest());
        assert_eq!(a, b);
    }

    #[test]
    fn the_digest_is_reproducible() {
        let spec = parse(MINIMAL);
        let once = spec.digest();
        for _ in 0..100 {
            assert_eq!(parse(MINIMAL).digest(), once);
        }
        assert_eq!(once.len(), 64);
    }

    #[test]
    fn a_channel_whose_reference_is_broken_is_unknown_not_empty() {
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": { "c": { "$ref": "#/components/channels/nope" } }
            }"###,
        );
        assert_eq!(spec.coverage, CoverageState::Unknown);
        let channel = spec.channels.get("c").expect("channel keeps its id");
        assert_eq!(channel.coverage, CoverageState::Unknown);
        assert!(channel.messages.is_empty());
    }

    #[test]
    fn an_operation_pointing_at_a_missing_channel_degrades() {
        let spec = parse(
            r###"{
              "asyncapi": "3.1.0",
              "channels": {},
              "operations": {
                "o": { "action": "send", "channel": { "$ref": "#/channels/ghost" } }
              }
            }"###,
        );
        assert_eq!(spec.coverage, CoverageState::Unknown);
    }

    #[test]
    fn placeholders_are_extracted_from_the_address() {
        assert_eq!(
            placeholders("orders/{orderId}"),
            BTreeSet::from(["orderId".to_string()])
        );
        assert_eq!(
            placeholders("{a}/x/{b}"),
            BTreeSet::from(["a".to_string(), "b".to_string()])
        );
        assert_eq!(placeholders("plain"), BTreeSet::new());
        assert_eq!(placeholders("{unterminated"), BTreeSet::new());
    }

    #[test]
    fn format_readability_is_conservative_about_other_formats() {
        assert!(format_is_readable(
            "application/vnd.aai.asyncapi+json;version=3.1.0"
        ));
        assert!(format_is_readable("application/schema+json"));
        assert!(format_is_readable(
            "application/vnd.oai.openapi;version=3.0.0"
        ));
        assert!(!format_is_readable(
            "application/vnd.apache.avro;version=1.9.0"
        ));
        assert!(!format_is_readable(
            "application/vnd.google.protobuf;version=3"
        ));
        assert!(!format_is_readable("application/x-raml"));
    }
}
