//! Breaking-change classification for OpenAPI contracts (Step 37).
//!
//! # The verdict algebra
//!
//! ```text
//! Breaking   if any change is classified Breaking
//! Unknown    else if any change is classified Unknown, or coverage != Verified
//! Compatible otherwise
//! ```
//!
//! Two properties make this safe:
//!
//! 1. **A positive finding needs no completeness.** A removed endpoint is
//!    breaking whether or not the rest of the document was modelled, so the
//!    `Breaking` verdict does not depend on coverage.
//! 2. **A negative finding does.** `Compatible` is the only claim that the
//!    engine did not look at something and miss it, so it is the only claim
//!    gated on `coverage == Verified` (`FR-014`, `FR-030`).
//!
//! # Direction
//! Request-side and response-side schemas have opposite arrows of
//! compatibility. Narrowing a request parameter's type rejects payloads that
//! used to be accepted, which breaks existing clients. Narrowing a *response*
//! type is a change to a promise consumers were built against. The two are
//! therefore classified by different rules, and where no directional evidence
//! exists the change is classified [`Severity::Unknown`] rather than guessed at.

use crate::openapi::{HttpMethod, OpenApiSpec, Operation, ParamIdentity, SchemaFacts};
use autopsy_domain::CoverageState;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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
}

impl ChangeKind {
    /// Every variant, so a schema-completeness check can be exhaustive rather
    /// than a hand-maintained list that drifts.
    pub const ALL: [ChangeKind; 29] = [
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

/// Compares two parsed OpenAPI contracts.
pub fn diff_openapi(from: &OpenApiSpec, to: &OpenApiSpec) -> ContractDelta {
    let mut changes: Vec<ContractChange> = Vec::new();

    // --- path surface ---------------------------------------------------
    for (path, from_item) in &from.paths {
        match to.paths.get(path) {
            None => changes.push(ContractChange {
                kind: ChangeKind::EndpointRemoved,
                severity: ChangeKind::EndpointRemoved.default_severity(),
                subject: path.clone(),
                detail: format!("path `{path}` no longer exists"),
                coverage: CoverageState::Verified,
            }),
            Some(_) => {
                for method in from_item.operations.keys() {
                    if !to
                        .paths
                        .get(path)
                        .is_some_and(|i| i.operations.contains_key(method))
                    {
                        changes.push(ContractChange {
                            kind: ChangeKind::MethodRemoved,
                            severity: ChangeKind::MethodRemoved.default_severity(),
                            subject: format!("{} {}", method.as_str().to_uppercase(), path),
                            detail: format!(
                                "`{}` is no longer served on `{path}`",
                                method.as_str().to_uppercase()
                            ),
                            coverage: CoverageState::Verified,
                        });
                    }
                }
                for method in to_item_added_methods(from_item, to.paths.get(path)) {
                    changes.push(ContractChange {
                        kind: ChangeKind::MethodAdded,
                        severity: ChangeKind::MethodAdded.default_severity(),
                        subject: format!("{} {}", method.as_str().to_uppercase(), path),
                        detail: format!(
                            "`{}` was added on `{path}`",
                            method.as_str().to_uppercase()
                        ),
                        coverage: CoverageState::Verified,
                    });
                }
            }
        }
    }
    for path in to.paths.keys() {
        if !from.paths.contains_key(path) {
            changes.push(ContractChange {
                kind: ChangeKind::EndpointAdded,
                severity: ChangeKind::EndpointAdded.default_severity(),
                subject: path.clone(),
                detail: format!("path `{path}` was added"),
                coverage: CoverageState::Verified,
            });
        }
    }

    // --- per-operation surface -----------------------------------------
    for (path, from_item) in &from.paths {
        let Some(to_item) = to.paths.get(path) else {
            continue;
        };
        for (method, from_op) in &from_item.operations {
            let Some(to_op) = to_item.operations.get(method) else {
                continue;
            };
            compare_operation(from_op, to_op, &mut changes);
        }
    }

    // --- disclosure of incomparable subtrees ---------------------------
    let mut incomparable: BTreeSet<String> = BTreeSet::new();
    for spec in [from, to] {
        for pointer in &spec.unmodeled_pointers {
            incomparable.insert(pointer.clone());
        }
        for reference in &spec.unresolved_refs {
            incomparable.insert(reference.clone());
        }
    }
    for pointer in &incomparable {
        changes.push(ContractChange {
            kind: ChangeKind::IncomparableSubtree,
            severity: ChangeKind::IncomparableSubtree.default_severity(),
            subject: pointer.clone(),
            detail: format!(
                "`{pointer}` uses a construct this engine records but does not interpret, so a \
                 change inside it could not be detected"
            ),
            coverage: CoverageState::Partial,
        });
    }

    // Canonical ordering: by kind, then subject, then detail. Sorting on the
    // pre-classified severity would make the order depend on which pass found a
    // change, and a reordering of the traversal must not move bytes.
    changes.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.subject.cmp(&b.subject))
            .then_with(|| a.detail.cmp(&b.detail))
    });
    changes.dedup();

    let coverage = worse(&from.coverage, &to.coverage);
    let mut notes: Vec<String> = Vec::new();

    let has_breaking = changes.iter().any(|c| c.severity == Severity::Breaking);
    let has_unknown_change = changes.iter().any(|c| c.severity == Severity::Unknown);

    let verdict = if has_breaking {
        notes.push(format!(
            "{} breaking change(s) detected; coverage does not affect a positive finding",
            changes
                .iter()
                .filter(|c| c.severity == Severity::Breaking)
                .count()
        ));
        CompatibilityVerdict::Breaking
    } else if has_unknown_change {
        notes.push(format!(
            "{} change(s) could not be classified; a `Compatible` verdict is not asserted",
            changes
                .iter()
                .filter(|c| c.severity == Severity::Unknown)
                .count()
        ));
        CompatibilityVerdict::Unknown
    } else if coverage != CoverageState::Verified {
        notes.push(format!(
            "no breaking change was found, but coverage is `{}`; a `Compatible` verdict requires \
             `verified` coverage because `Compatible` is the only verdict that claims nothing was \
             missed",
            coverage_tag(coverage)
        ));
        CompatibilityVerdict::Unknown
    } else {
        CompatibilityVerdict::Compatible
    };

    let version_change = if from.declared_version != to.declared_version {
        Some(VersionChange {
            from: from.declared_version.clone(),
            to: to.declared_version.clone(),
        })
    } else {
        None
    };

    for spec in [from, to] {
        for note in &spec.notes {
            notes.push(format!("{}: {note}", spec.document_path));
        }
    }
    notes.sort();
    notes.dedup();

    let mut delta = ContractDelta {
        protocol: "openapi".to_string(),
        from_path: from.document_path.clone(),
        to_path: to.document_path.clone(),
        from_digest: from.digest(),
        to_digest: to.digest(),
        version_change,
        changes,
        verdict,
        coverage,
        notes,
        digest: String::new(),
    };
    delta.digest = delta_digest(&delta);
    delta
}

/// Methods present in `to` but not in `from`.
fn to_item_added_methods(
    from_item: &crate::openapi::PathItem,
    to_item: Option<&crate::openapi::PathItem>,
) -> Vec<HttpMethod> {
    match to_item {
        Some(item) => item
            .operations
            .keys()
            .filter(|m| !from_item.operations.contains_key(m))
            .copied()
            .collect(),
        None => Vec::new(),
    }
}

fn compare_operation(from: &Operation, to: &Operation, changes: &mut Vec<ContractChange>) {
    let key = from.operation_key();
    let to_key = to.operation_key();

    // --- parameters -----------------------------------------------------
    for (identity, from_param) in &from.parameters {
        match to.parameters.get(identity) {
            None => changes.push(ContractChange {
                kind: ChangeKind::ParameterRemoved,
                severity: ChangeKind::ParameterRemoved.default_severity(),
                subject: format!("{key} {}", identity.describe()),
                detail: format!("parameter {} was removed", identity.describe()),
                coverage: CoverageState::Verified,
            }),
            Some(to_param) => {
                if !from_param.required && to_param.required {
                    changes.push(ContractChange {
                        kind: ChangeKind::ParameterBecameRequired,
                        severity: ChangeKind::ParameterBecameRequired.default_severity(),
                        subject: format!("{key} {}", identity.describe()),
                        detail: format!(
                            "parameter {} became required; existing callers omit it",
                            identity.describe()
                        ),
                        coverage: CoverageState::Verified,
                    });
                } else if from_param.required && !to_param.required {
                    changes.push(ContractChange {
                        kind: ChangeKind::ParameterBecameOptional,
                        severity: ChangeKind::ParameterBecameOptional.default_severity(),
                        subject: format!("{key} {}", identity.describe()),
                        detail: format!("parameter {} became optional", identity.describe()),
                        coverage: CoverageState::Verified,
                    });
                }
                compare_request_schema(
                    &from_param.schema,
                    &to_param.schema,
                    &format!("{key} {}", identity.describe()),
                    changes,
                );
            }
        }
    }
    for (identity, to_param) in &to.parameters {
        if from.parameters.contains_key(identity) {
            continue;
        }
        let (kind, verb) = if to_param.required {
            (ChangeKind::RequiredParameterAdded, "required")
        } else {
            (ChangeKind::OptionalParameterAdded, "optional")
        };
        changes.push(ContractChange {
            kind,
            severity: kind.default_severity(),
            subject: format!("{to_key} {}", identity.describe()),
            detail: format!(
                "{} parameter {} was added; existing callers do not send it",
                verb,
                identity.describe()
            ),
            coverage: CoverageState::Verified,
        });
    }

    // --- request body ---------------------------------------------------
    match (&from.request_body, &to.request_body) {
        (None, Some(rb)) => {
            // Severity depends on `required`, so the kind must too: an added
            // *optional* body removes nothing that callers relied on.
            let kind = if rb.required {
                ChangeKind::RequestBodyAdded
            } else {
                ChangeKind::OptionalRequestBodyAdded
            };
            changes.push(ContractChange {
                kind,
                severity: kind.default_severity(),
                subject: key.clone(),
                detail: if rb.required {
                    "a required request body was added; existing callers send none".to_string()
                } else {
                    "an optional request body was added".to_string()
                },
                coverage: CoverageState::Verified,
            });
        }
        (Some(_), None) => changes.push(ContractChange {
            kind: ChangeKind::RequestBodyRemoved,
            severity: ChangeKind::RequestBodyRemoved.default_severity(),
            subject: key.clone(),
            detail: "the request body was removed".to_string(),
            coverage: CoverageState::Verified,
        }),
        (Some(from_rb), Some(to_rb)) => {
            if !from_rb.required && to_rb.required {
                changes.push(ContractChange {
                    kind: ChangeKind::RequestBodyBecameRequired,
                    severity: ChangeKind::RequestBodyBecameRequired.default_severity(),
                    subject: key.clone(),
                    detail: "the request body became required; bodyless calls now fail".to_string(),
                    coverage: CoverageState::Verified,
                });
            } else if from_rb.required && !to_rb.required {
                changes.push(ContractChange {
                    kind: ChangeKind::RequestBodyBecameOptional,
                    severity: ChangeKind::RequestBodyBecameOptional.default_severity(),
                    subject: key.clone(),
                    detail: "the request body became optional".to_string(),
                    coverage: CoverageState::Verified,
                });
            }
            for (media, from_schema) in &from_rb.content {
                match to_rb.content.get(media) {
                    None => changes.push(ContractChange {
                        kind: ChangeKind::MediaTypeRemoved,
                        severity: ChangeKind::MediaTypeRemoved.default_severity(),
                        subject: format!("{key} {media}"),
                        detail: format!("media type `{media}` was removed from the request body"),
                        coverage: CoverageState::Verified,
                    }),
                    Some(to_schema) => compare_request_schema(
                        from_schema,
                        to_schema,
                        &format!("{key} request `{media}`"),
                        changes,
                    ),
                }
            }
            for media in to_rb.content.keys() {
                if !from_rb.content.contains_key(media) {
                    changes.push(ContractChange {
                        kind: ChangeKind::MediaTypeAdded,
                        severity: ChangeKind::MediaTypeAdded.default_severity(),
                        subject: format!("{key} {media}"),
                        detail: format!("media type `{media}` was added to the request body"),
                        coverage: CoverageState::Verified,
                    });
                }
            }
        }
        (None, None) => {}
    }

    // --- responses ------------------------------------------------------
    for status in from.responses.keys() {
        if !to.responses.contains_key(status) {
            changes.push(ContractChange {
                kind: ChangeKind::ResponseStatusRemoved,
                severity: ChangeKind::ResponseStatusRemoved.default_severity(),
                subject: format!("{key} {status}"),
                detail: format!("response `{status}` is no longer documented"),
                coverage: CoverageState::Verified,
            });
        }
    }
    for status in to.responses.keys() {
        if !from.responses.contains_key(status) {
            changes.push(ContractChange {
                kind: ChangeKind::ResponseStatusAdded,
                severity: ChangeKind::ResponseStatusAdded.default_severity(),
                subject: format!("{key} {status}"),
                detail: format!("response `{status}` was added"),
                coverage: CoverageState::Verified,
            });
        }
    }
    for (status, from_response) in &from.responses {
        let Some(to_response) = to.responses.get(status) else {
            continue;
        };
        // Response-side direction is not the same as request-side direction, and
        // this engine does not have evidence for it. Any modelled difference is
        // therefore reported as a change it refuses to classify, which blocks a
        // `Compatible` verdict without asserting a break.
        if response_fingerprints(from_response) != response_fingerprints(to_response) {
            changes.push(ContractChange {
                kind: ChangeKind::ResponseSchemaChanged,
                severity: ChangeKind::ResponseSchemaChanged.default_severity(),
                subject: format!("{key} {status}"),
                detail: format!(
                    "the response body schema for `{status}` changed; request-side narrowing rules \
                     do not apply to responses and this engine has no directional evidence for them"
                ),
                coverage: CoverageState::Partial,
            });
        }
    }
}

fn response_fingerprints(response: &crate::openapi::Response) -> Vec<(String, String)> {
    response
        .content
        .iter()
        .map(|(media, facts)| (media.clone(), facts.fingerprint()))
        .collect()
}

/// Compares request-side schemas, where narrowing breaks senders.
fn compare_request_schema(
    from: &SchemaFacts,
    to: &SchemaFacts,
    subject: &str,
    changes: &mut Vec<ContractChange>,
) {
    if !from.is_comparable() || !to.is_comparable() {
        if from.fingerprint() != to.fingerprint() {
            changes.push(ContractChange {
                kind: ChangeKind::IncomparableSubtree,
                severity: ChangeKind::IncomparableSubtree.default_severity(),
                subject: subject.to_string(),
                detail: format!(
                    "`{subject}` changed, but at least one side is not fully modelled \
                     (`{from_cov}` -> `{to_cov}`), so the change could not be classified",
                    from_cov = coverage_tag(from.coverage),
                    to_cov = coverage_tag(to.coverage),
                ),
                coverage: CoverageState::Partial,
            });
        }
        return;
    }

    // --- type direction ------------------------------------------------
    if !from.types.is_empty() && !to.types.is_empty() && from.types != to.types {
        let narrowed = to.types.is_subset(&from.types);
        let widened = from.types.is_subset(&to.types);
        let (kind, what) = if narrowed {
            (
                ChangeKind::ParameterTypeNarrowed,
                format!(
                    "accepted types narrowed from {{{}}} to {{{}}}",
                    join(&from.types),
                    join(&to.types)
                ),
            )
        } else if widened {
            (
                ChangeKind::ParameterTypeWidened,
                format!(
                    "accepted types widened from {{{}}} to {{{}}}",
                    join(&from.types),
                    join(&to.types)
                ),
            )
        } else {
            (
                ChangeKind::IncomparableSubtree,
                format!(
                    "accepted types changed from {{{}}} to {{{}}}, which is neither a narrowing nor \
                     a widening",
                    join(&from.types),
                    join(&to.types)
                ),
            )
        };
        changes.push(ContractChange {
            kind,
            severity: kind.default_severity(),
            subject: subject.to_string(),
            detail: what,
            coverage: CoverageState::Verified,
        });
    }

    // --- enum direction ------------------------------------------------
    match (&from.enum_values, &to.enum_values) {
        (Some(from_values), Some(to_values)) => {
            for value in from_values.difference(to_values) {
                changes.push(ContractChange {
                    kind: ChangeKind::ParameterEnumValueRemoved,
                    severity: ChangeKind::ParameterEnumValueRemoved.default_severity(),
                    subject: subject.to_string(),
                    detail: format!("accepted enum value `{value}` was removed"),
                    coverage: CoverageState::Verified,
                });
            }
            for value in to_values.difference(from_values) {
                changes.push(ContractChange {
                    kind: ChangeKind::ParameterEnumValueAdded,
                    severity: ChangeKind::ParameterEnumValueAdded.default_severity(),
                    subject: subject.to_string(),
                    detail: format!("enum value `{value}` was added"),
                    coverage: CoverageState::Verified,
                });
            }
        }
        (None, Some(values)) => {
            changes.push(ContractChange {
                kind: ChangeKind::ParameterTypeNarrowed,
                severity: ChangeKind::ParameterTypeNarrowed.default_severity(),
                subject: subject.to_string(),
                detail: format!(
                    "an enum constraint {{{}}} was introduced where none existed",
                    join(values)
                ),
                coverage: CoverageState::Verified,
            });
        }
        (Some(_), None) => {
            changes.push(ContractChange {
                kind: ChangeKind::ParameterTypeWidened,
                severity: ChangeKind::ParameterTypeWidened.default_severity(),
                subject: subject.to_string(),
                detail: "the enum constraint was removed".to_string(),
                coverage: CoverageState::Verified,
            });
        }
        (None, None) => {}
    }

    // --- object properties ---------------------------------------------
    for (name, from_child) in &from.properties {
        match to.properties.get(name) {
            None => {
                let kind = ChangeKind::RequestPropertyRemoved;
                changes.push(ContractChange {
                    kind,
                    severity: kind.default_severity(),
                    subject: format!("{subject}.{name}"),
                    detail: format!("property `{name}` was removed"),
                    coverage: CoverageState::Verified,
                });
            }
            Some(to_child) => {
                compare_request_schema(from_child, to_child, &format!("{subject}.{name}"), changes)
            }
        }
    }
    for name in to.properties.keys() {
        if from.properties.contains_key(name) {
            continue;
        }
        let kind = if to.required.contains(name) {
            ChangeKind::RequiredPropertyAdded
        } else {
            ChangeKind::OptionalPropertyAdded
        };
        changes.push(ContractChange {
            kind,
            severity: kind.default_severity(),
            subject: format!("{subject}.{name}"),
            detail: if to.required.contains(name) {
                format!("required property `{name}` was added; existing callers omit it")
            } else {
                format!("optional property `{name}` was added")
            },
            coverage: CoverageState::Verified,
        });
    }

    // --- additionalProperties ------------------------------------------
    if from.additional_properties != Some(false) && to.additional_properties == Some(false) {
        changes.push(ContractChange {
            kind: ChangeKind::RequestAdditionalPropertiesDisallowed,
            severity: ChangeKind::RequestAdditionalPropertiesDisallowed.default_severity(),
            subject: subject.to_string(),
            detail: "additional properties are no longer accepted".to_string(),
            coverage: CoverageState::Verified,
        });
    }
}

fn join(set: &BTreeSet<String>) -> String {
    set.iter().cloned().collect::<Vec<_>>().join(", ")
}

fn coverage_tag(state: CoverageState) -> &'static str {
    match state {
        CoverageState::Verified => "verified",
        CoverageState::Partial => "partial",
        CoverageState::Unsupported => "unsupported",
        CoverageState::Unknown => "unknown",
    }
}

fn worse(a: &CoverageState, b: &CoverageState) -> CoverageState {
    use CoverageState::*;
    match (a, b) {
        (Unknown, _) | (_, Unknown) => Unknown,
        (Unsupported, _) | (_, Unsupported) => Unsupported,
        (Partial, _) | (_, Partial) => Partial,
        _ => Verified,
    }
}

/// BLAKE3 over the canonical delta body.
///
/// Deliberately excludes `notes`, whose prose is not part of the finding, and
/// includes `severity` so that a reclassification is a different digest even
/// when the underlying change set is identical.
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
    hasher.update(coverage_tag(delta.coverage).as_bytes());
    hasher.update(b"\n");
    for change in &delta.changes {
        hasher.update(severity_tag(change.severity).as_bytes());
        hasher.update(b"\x00");
        hasher.update(format!("{:?}", change.kind).as_bytes());
        hasher.update(b"\x00");
        hasher.update(change.subject.as_bytes());
        hasher.update(b"\n");
    }
    hasher.finalize().to_hex().to_string()
}

fn severity_tag(severity: Severity) -> &'static str {
    match severity {
        Severity::Breaking => "breaking",
        Severity::Unknown => "unknown",
        Severity::NonBreaking => "non_breaking",
    }
}

/// Convenience: identity of a parameter set, used in tests and reports.
pub fn parameter_identities(operation: &Operation) -> BTreeMap<ParamIdentity, bool> {
    operation
        .parameters
        .iter()
        .map(|(id, p)| (id.clone(), p.required))
        .collect()
}
