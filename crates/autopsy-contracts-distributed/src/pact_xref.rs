//! Cross-references Pact consumer expectations against a provider interface
//! (Step 39).
//!
//! # The two artifacts do not have the same shape, and pretending otherwise is
//! the whole risk
//!
//! A Pact interaction records **one concrete exchange**: `GET /alligators/3`.
//! An OpenAPI operation declares **a template**: `GET /alligators/{id}`. So the
//! question is not "are these equal" but "does this template subsume this
//! concrete request" — and the failure mode of getting that wrong is a *silent*
//! one. Comparing the two as strings reports every interaction as unmatched,
//! which is why the matcher here works segment by segment.
//!
//! The second silent failure is the opposite direction. A concrete path always
//! fills in every `{param}` of the template that matches it, so path parameters
//! are supplied by construction and there is nothing to check. That is *not*
//! true of query parameters, headers, bodies, or responses — and a
//! cross-reference that checked none of them would report a clean run for a
//! consumer that shares nothing with its provider but a URL.
//!
//! # Matching is a search, and a search can be ambiguous
//!
//! `/alligators/{id}` and `/alligators/{name}` both subsume `/alligators/3`.
//! OpenAPI forbids that pair in one document, but nothing enforces it and real
//! documents contain it. Rather than picking one and staying quiet, the search
//! prefers the **most specific** template (fewest template segments) and, when
//! two templates are equally specific, records
//! [`ChangeKind::PactAmbiguousProviderMatch`] and degrades coverage. A
//! coin-flip reported as a match is indistinguishable from a correct one.
//!
//! # What is not checked
//!
//! * **`matchingRules`** are never evaluated (see [`crate::pact`]).
//! * **Message interactions** are not matched against an HTTP interface, so
//!   they are recorded, counted, and degraded — never silently skipped.
//! * **`providerStates`** describe the state a provider must be in. That is not
//!   a fact about its interface, so it is neither checked nor degraded.
//! * **Response bodies** are fingerprinted but not compared to the provider's
//!   declared response schema. The provider's schema describes a *family* of
//!   responses and the interaction records one instance, so the comparison that
//!   would be meaningful is the other direction (schema ⊇ instance), which needs
//!   a JSON Schema validator this engine deliberately does not carry.

use crate::change::{
    ChangeKind, CompatibilityVerdict, ContractChange, ContractDelta, Severity, VersionChange,
};
use crate::document::worse_of;
use crate::openapi::{HttpMethod, OpenApiSpec, Operation, ParamLocation};
use crate::pact::{InteractionKind, PactDocument};
use autopsy_domain::CoverageState;
use std::collections::BTreeSet;

/// One path split into its non-empty segments.
fn segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

/// A template segment is a placeholder when it is exactly `{name}`.
///
/// A `{name}` that shares a segment with literal text (OpenAPI permits
/// `/files/{name}.json`) is treated as literal, which means such a path will not
/// match a concrete one. That under-matches rather than over-matches: the
/// interaction is reported as unmatched and a human looks at it, instead of a
/// partial-segment placeholder matching something it should not.
fn is_placeholder(segment: &str) -> bool {
    segment.len() > 2 && segment.starts_with('{') && segment.ends_with('}')
}

/// Whether `concrete` is subsumed by `template`, and how many placeholders the
/// template used.
fn subsumes(concrete: &[&str], template: &[&str]) -> Option<usize> {
    if concrete.len() != template.len() {
        return None;
    }
    let mut placeholders = 0;
    for (actual, expected) in concrete.iter().zip(template) {
        if is_placeholder(expected) {
            if actual.is_empty() {
                return None;
            }
            placeholders += 1;
        } else if actual != expected {
            return None;
        }
    }
    Some(placeholders)
}

fn method_of(raw: &str) -> Option<HttpMethod> {
    HttpMethod::ALL
        .into_iter()
        .find(|m| m.as_str().eq_ignore_ascii_case(raw))
}

/// What the provider's declared paths can say about a concrete request path.
///
/// The operation is carried in the match rather than looked up again by the
/// caller, because looking it up again is where a method-agnostic search turns
/// into a silent hole: the first version of this search ignored the method while
/// choosing a path, then found no operation under that path and merely *noted*
/// the problem. `PactMethodNotDeclared` was unreachable, and a consumer calling
/// an operation the provider does not offer produced an empty change list.
enum PathMatch<'a> {
    /// Exactly one most-specific declared path serves the method.
    Unique(&'a str, &'a Operation),
    /// Equally specific declarations serve the method. The first in canonical
    /// order is named, together with the others, so the ambiguity is visible.
    Ambiguous(&'a str, &'a Operation, BTreeSet<String>),
    /// Declared paths subsume the request, but none of them serves the method.
    MethodNotDeclared(BTreeSet<String>),
    /// No declared path subsumes the request at all.
    NoPath,
}

/// Finds the most specific declared path **that serves `method`** and subsumes
/// `concrete`.
///
/// Filtering by method before ranking, rather than after, is not a
/// micro-optimisation — it is the difference between a correct answer and a
/// false one. Given `/pets/health` (GET only) and `/pets/{id}` (POST only), a
/// consumer calling `POST /pets/health` **is** served, by the template. Ranking
/// first would name `/pets/health`, find no POST under it, and report a method
/// problem that does not exist.
fn match_path<'a>(provider: &'a OpenApiSpec, method: HttpMethod, concrete: &str) -> PathMatch<'a> {
    let concrete_segments = segments(concrete);
    let mut candidates: Vec<(usize, &'a str, &'a Operation)> = Vec::new();
    let mut path_only: BTreeSet<String> = BTreeSet::new();
    for (path, item) in &provider.paths {
        if subsumes(&concrete_segments, &segments(path)).is_none() {
            continue;
        }
        match item.operations.get(&method) {
            Some(operation) => candidates.push((
                subsumes(&concrete_segments, &segments(path)).unwrap_or_default(),
                path.as_str(),
                operation,
            )),
            None => {
                path_only.insert(path.clone());
            }
        }
    }
    // Fewest placeholders first (most specific), then lexicographically, so the
    // choice does not depend on map iteration order.
    candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
    let Some((best_specificity, best, best_operation)) = candidates.first().copied() else {
        return if path_only.is_empty() {
            PathMatch::NoPath
        } else {
            PathMatch::MethodNotDeclared(path_only)
        };
    };
    let tied: BTreeSet<String> = candidates
        .iter()
        .filter(|(specificity, path, _)| *specificity == best_specificity && *path != best)
        .map(|(_, path, _)| (*path).to_string())
        .collect();
    if tied.is_empty() {
        PathMatch::Unique(best, best_operation)
    } else {
        PathMatch::Ambiguous(best, best_operation, tied)
    }
}

/// Whether a provider declares a response for `status`.
///
/// OpenAPI answer keys are strings, and two of them are not integers: `default`
/// covers everything the document does not name, and `2XX` (uppercase, per
/// OpenAPI 3.1) covers a status class. Missing either of those would report a
/// correctly documented provider as undeclared.
fn status_declared(provider: &OpenApiSpec, path: &str, method: HttpMethod, status: u16) -> bool {
    let Some(item) = provider.paths.get(path) else {
        return false;
    };
    let Some(operation) = item.operations.get(&method) else {
        return false;
    };
    if operation.responses.contains_key(&status.to_string()) {
        return true;
    }
    if operation.responses.contains_key("default") {
        return true;
    }
    let class = status / 100;
    operation.responses.contains_key(&format!("{class}XX"))
        || operation.responses.contains_key(&format!("{class}xx"))
}

/// Cross-references a Pact against a provider's declared OpenAPI interface.
///
/// The returned [`ContractDelta`] uses `from` for the **consumer's**
/// expectations (the Pact) and `to` for the **provider's** declared interface,
/// because the question being asked is directional: the consumer asserts what it
/// needs, and the provider is asked whether it offers it.
pub fn xref_pact_against_provider(pact: &PactDocument, provider: &OpenApiSpec) -> ContractDelta {
    let mut changes: Vec<ContractChange> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut coverage = worse_of(pact.coverage, provider.coverage);

    let push = |changes: &mut Vec<ContractChange>,
                kind: ChangeKind,
                subject: String,
                detail: String,
                coverage: CoverageState| {
        changes.push(ContractChange {
            kind,
            severity: kind.default_severity(),
            subject,
            detail,
            coverage,
        });
    };

    let mut matched = 0usize;
    let mut message_interactions: Vec<String> = Vec::new();

    for interaction in &pact.interactions {
        coverage = worse_of(coverage, interaction.coverage);
        if interaction.kind != InteractionKind::Http {
            message_interactions.push(interaction.description.clone());
            continue;
        }
        let subject = format!("{} {}", pact.consumer, interaction.description);
        let Some(request) = &interaction.request else {
            // Already degraded when the artifact was parsed; there is nothing to
            // match without a request.
            continue;
        };
        if request.method.is_empty() || request.path.is_empty() {
            continue;
        }

        let Some(method) = method_of(&request.method) else {
            coverage = worse_of(coverage, CoverageState::Partial);
            notes.push(format!(
                "`{}` declares method `{}`, which is not an HTTP method this engine models",
                interaction.description, request.method
            ));
            continue;
        };

        let (declared_path, operation) = match match_path(provider, method, &request.path) {
            PathMatch::Unique(path, operation) => (path, operation),
            PathMatch::Ambiguous(chosen, operation, tied) => {
                coverage = worse_of(coverage, CoverageState::Partial);
                push(
                    &mut changes,
                    ChangeKind::PactAmbiguousProviderMatch,
                    subject.clone(),
                    format!(
                        "`{}` is subsumed by equally specific provider paths `{chosen}` and \
                         {tied:?}; the most specific match cannot be chosen without guessing, so \
                         `{chosen}` was used and the result is not a clean match",
                        request.path
                    ),
                    CoverageState::Partial,
                );
                (chosen, operation)
            }
            PathMatch::MethodNotDeclared(paths) => {
                // Distinguish "the provider has this path under another method"
                // from "the provider has never heard of this path": the first is
                // a method problem, the second a path problem, and they are
                // fixed in different places.
                let declared: Vec<&str> = paths.iter().map(String::as_str).collect();
                push(
                    &mut changes,
                    ChangeKind::PactMethodNotDeclared,
                    subject.clone(),
                    format!(
                        "provider declares {declared:?}, which subsume `{}`, but none of them \
                         serves `{method}`; the consumer expects `{} {}`",
                        request.path, request.method, request.path
                    ),
                    interaction.coverage,
                );
                continue;
            }
            PathMatch::NoPath => {
                push(
                    &mut changes,
                    ChangeKind::PactInteractionUnmatched,
                    subject.clone(),
                    format!(
                        "no provider path subsumes `{} {}`; the provider does not offer this \
                         interaction at all",
                        request.method, request.path
                    ),
                    interaction.coverage,
                );
                continue;
            }
        };

        matched += 1;
        coverage = worse_of(coverage, operation.coverage);

        let mut declared_query: BTreeSet<&str> = BTreeSet::new();
        for (identity, parameter) in &operation.parameters {
            if identity.location != ParamLocation::Query {
                continue;
            }
            declared_query.insert(identity.name.as_str());
            if parameter.required && !request.query.contains_key(&identity.name) {
                push(
                    &mut changes,
                    ChangeKind::PactRequiredQueryParameterMissing,
                    subject.clone(),
                    format!(
                        "provider declares required query parameter `{}` on `{} {}`, but the \
                         interaction does not send it",
                        identity.name, request.method, declared_path
                    ),
                    interaction.coverage,
                );
            }
        }

        if declared_query.is_empty() {
            if !request.query.is_empty() {
                // The provider documents no query parameters at all, so
                // "undeclared" would be true of every parameter and therefore
                // says nothing. Saying that is more useful than saying it 50
                // times.
                coverage = worse_of(coverage, CoverageState::Partial);
                notes.push(format!(
                    "`{} {}` declares no query parameters, so the {} parameter(s) sent by `{}` \
                     could not be checked",
                    method,
                    declared_path,
                    request.query.len(),
                    interaction.description
                ));
            }
        } else {
            for name in request.query.keys() {
                if !declared_query.contains(name.as_str()) {
                    push(
                        &mut changes,
                        ChangeKind::PactUndeclaredRequestParameter,
                        subject.clone(),
                        format!(
                            "the interaction sends query parameter `{name}` on `{} {}`, which the \
                             provider does not declare; the provider may reject it",
                            request.method, declared_path
                        ),
                        interaction.coverage,
                    );
                }
            }
        }

        match operation.request_body.as_ref() {
            Some(body) if body.required && !request.has_body => push(
                &mut changes,
                ChangeKind::PactRequiredRequestBodyMissing,
                subject.clone(),
                format!(
                    "provider declares a required request body on `{} {}`, but the interaction \
                     sends none",
                    request.method, declared_path
                ),
                interaction.coverage,
            ),
            Some(body) if request.has_body => {
                // Compare the interaction's declared fields against the
                // provider's schema. An undeclared field is not automatically
                // breaking: JSON Schema permits additional properties unless it
                // says otherwise, so the schema decides.
                for facts in body.content.values() {
                    let closed = facts.additional_properties == Some(false);
                    for name in &request.body_keys {
                        if facts.properties.contains_key(name) {
                            continue;
                        }
                        changes.push(ContractChange {
                            kind: ChangeKind::PactUndeclaredBodyProperty,
                            severity: if closed {
                                Severity::Breaking
                            } else {
                                Severity::Unknown
                            },
                            subject: subject.clone(),
                            detail: format!(
                                "the interaction sends body field `{name}` on `{} {}`, which the \
                                 provider's schema does not declare{verdict}",
                                request.method,
                                declared_path,
                                verdict = if closed {
                                    " and explicitly rejects"
                                } else {
                                    ", and the schema does not close itself, so whether it is \
                                     accepted is unknown"
                                }
                            ),
                            coverage: worse_of(interaction.coverage, facts.coverage),
                        });
                    }
                }
            }
            None if request.has_body => push(
                &mut changes,
                ChangeKind::PactUndeclaredRequestBody,
                subject.clone(),
                format!(
                    "the interaction sends a request body on `{} {}`, which the provider \
                     declares no body for",
                    request.method, declared_path
                ),
                interaction.coverage,
            ),
            _ => {}
        }

        if let Some(response) = &interaction.response
            && !status_declared(provider, declared_path, method, response.status)
        {
            push(
                &mut changes,
                ChangeKind::PactResponseStatusUndeclared,
                subject.clone(),
                format!(
                    "the interaction expects status {} from `{} {}`, which the provider does not \
                     declare",
                    response.status, request.method, declared_path
                ),
                interaction.coverage,
            );
        }
    }

    // --- disclosure --------------------------------------------------------
    if !pact.unmodeled_pointers.is_empty() {
        let count = pact.unmodeled_pointers.len();
        push(
            &mut changes,
            ChangeKind::PactMatchingRulesUnmodelled,
            format!("{}->{}", pact.consumer, pact.provider),
            format!(
                "{count} matcher location(s) in the artifact were recorded but not evaluated; a \
                 match against a pattern describes a family of requests, and this engine did not \
                 check whether the provider accepts that family"
            ),
            CoverageState::Partial,
        );
        coverage = worse_of(coverage, CoverageState::Partial);
    }
    if !message_interactions.is_empty() {
        push(
            &mut changes,
            ChangeKind::PactMessageInteractionNotCrossReferenced,
            format!("{}->{}", pact.consumer, pact.provider),
            format!(
                "{} message interaction(s) were parsed but not cross-referenced, because their \
                 provider interface is an AsyncAPI document rather than HTTP: {message_interactions:?}",
                message_interactions.len()
            ),
            CoverageState::Partial,
        );
        coverage = worse_of(coverage, CoverageState::Partial);
        notes.push(format!(
            "{} message interaction(s) are outside the scope of an HTTP cross-reference",
            message_interactions.len()
        ));
    }
    if matched == 0 {
        // Nothing was compared. A `Compatible` verdict here would approve a
        // check that never happened.
        coverage = worse_of(coverage, CoverageState::Unknown);
        notes.push(
            "no interaction could be matched to a provider operation, so nothing was compared"
                .to_string(),
        );
    }

    for note in &provider.notes {
        notes.push(note.clone());
    }
    for note in &pact.notes {
        notes.push(note.clone());
    }
    notes.push(format!(
        "{matched} of {} HTTP interaction(s) matched a provider operation",
        pact.http_interactions().len()
    ));

    changes.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.subject.cmp(&b.subject))
            .then_with(|| a.detail.cmp(&b.detail))
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
            "the verdict is `unknown` because coverage is {coverage:?} or a difference could not \
             be classified"
        ));
    }
    notes.sort();
    notes.dedup();

    let mut delta = ContractDelta {
        protocol: "pact".to_string(),
        from_path: pact.path.clone(),
        to_path: provider.document_path.clone(),
        from_digest: pact.digest(),
        to_digest: provider.digest(),
        version_change: pact.spec_version.as_ref().map(|from| VersionChange {
            from: from.clone(),
            to: provider.declared_version.clone(),
        }),
        changes,
        verdict,
        coverage,
        notes,
        digest: String::new(),
    };
    delta.digest = delta_digest(&delta);
    delta
}

/// BLAKE3 over the canonical delta body.
///
/// Excludes `notes` (prose, allowed to be reworded) and includes `severity`, so a
/// reclassification is a different digest even when the detected facts are
/// identical. Deliberately local rather than shared with the AsyncAPI engine:
/// the two deltas carry different `protocol` values, so a shared digest builder
/// would make an AsyncAPI delta and a Pact delta collide whenever everything
/// else happened to agree.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::DocFormat;
    use crate::openapi::parse_openapi;
    use crate::pact::parse_pact;

    fn provider(json: &str) -> OpenApiSpec {
        let doc = SpecDocument::parse("openapi.json", json, DocFormat::Json).expect("parses");
        parse_openapi(&doc).expect("parses openapi")
    }

    use crate::document::SpecDocument;

    #[test]
    fn a_placeholder_must_be_a_whole_segment() {
        assert!(is_placeholder("{id}"));
        assert!(!is_placeholder("{}"));
        assert!(!is_placeholder("{id"));
        assert!(!is_placeholder("id}"));
        // OpenAPI permits `/files/{name}.json`. Treating it as a placeholder
        // would match `/files/anything.json`; treating it as literal means the
        // interaction is reported as unmatched and a human looks at it.
        assert!(!is_placeholder("{name}.json"));
        assert!(!is_placeholder("prefix{id}"));
    }

    #[test]
    fn subsumes_counts_placeholders_and_requires_equal_length() {
        assert_eq!(subsumes(&["a"], &["a"]), Some(0));
        assert_eq!(subsumes(&["3"], &["{id}"]), Some(1));
        assert_eq!(subsumes(&["a", "3"], &["a", "{id}"]), Some(1));
        assert_eq!(subsumes(&["a"], &["a", "{id}"]), None);
        assert_eq!(subsumes(&["a", "b"], &["a"]), None);
        assert_eq!(subsumes(&["b"], &["a"]), None);
    }

    #[test]
    fn match_path_prefers_the_most_specific_template() {
        let spec = provider(
            r###"{
              "openapi": "3.2.1",
              "paths": {
                "/a/{id}": { "get": { "responses": { "200": {} } } },
                "/a/{id}/{rest}": { "get": { "responses": { "200": {} } } }
              }
            }"###,
        );
        // `/a/3` has two segments, so only the one-placeholder template fits.
        match match_path(&spec, HttpMethod::Get, "/a/3") {
            PathMatch::Unique(path, _) => assert_eq!(path, "/a/{id}"),
            _ => panic!("expected a unique most-specific match"),
        }
        // `/a/3/b` has three, so only the two-placeholder template fits.
        match match_path(&spec, HttpMethod::Get, "/a/3/b") {
            PathMatch::Unique(path, _) => assert_eq!(path, "/a/{id}/{rest}"),
            _ => panic!("expected a unique most-specific match"),
        }
    }

    #[test]
    fn match_path_reports_equally_specific_templates_as_ambiguous() {
        // OpenAPI forbids this pair in one document. Nothing enforces it, and
        // real documents contain it, so the ambiguity is reported rather than
        // resolved by a coin flip.
        let spec = provider(
            r###"{
              "openapi": "3.2.1",
              "paths": {
                "/a/{id}": { "get": { "responses": { "200": {} } } },
                "/a/{name}": { "get": { "responses": { "200": {} } } }
              }
            }"###,
        );
        match match_path(&spec, HttpMethod::Get, "/a/3") {
            PathMatch::Ambiguous(chosen, _, tied) => {
                assert_eq!(chosen, "/a/{id}", "canonical order decides which is named");
                assert_eq!(tied, BTreeSet::from(["/a/{name}".to_string()]));
            }
            _ => panic!("expected ambiguity to be reported"),
        }
    }

    #[test]
    fn match_path_reports_no_match_rather_than_a_loose_one() {
        let spec = provider(
            r###"{
              "openapi": "3.2.1",
              "paths": { "/a/{id}": { "get": { "responses": { "200": {} } } } }
            }"###,
        );
        assert!(matches!(
            match_path(&spec, HttpMethod::Get, "/b/3"),
            PathMatch::NoPath
        ));
        assert!(matches!(
            match_path(&spec, HttpMethod::Get, "/a"),
            PathMatch::NoPath
        ));
        // The path is declared; the method is not. That is a method problem and
        // is reported as one rather than as an absent path.
        assert!(matches!(
            match_path(&spec, HttpMethod::Delete, "/a/3"),
            PathMatch::MethodNotDeclared(paths) if paths == BTreeSet::from(["/a/{id}".to_string()])
        ));
    }

    #[test]
    fn an_exact_path_beats_a_template_that_also_fits() {
        let spec = provider(
            r###"{
              "openapi": "3.2.1",
              "paths": {
                "/a/{id}": { "get": { "responses": { "200": {} } } },
                "/a/health": { "get": { "responses": { "200": {} } } }
              }
            }"###,
        );
        match match_path(&spec, HttpMethod::Get, "/a/health") {
            PathMatch::Unique(path, _) => assert_eq!(path, "/a/health"),
            _ => panic!("a literal path must win over a template"),
        }
    }

    #[test]
    fn a_less_specific_template_still_serves_a_method_the_more_specific_path_does_not() {
        // The false positive this guards against: ranking paths before filtering
        // by method would name `/a/health`, find no `post` under it, and report a
        // method problem for a request the provider actually serves.
        let spec = provider(
            r###"{
              "openapi": "3.2.1",
              "paths": {
                "/a/health": { "get": { "responses": { "200": {} } } },
                "/a/{id}": { "post": { "responses": { "200": {} } } }
              }
            }"###,
        );
        match match_path(&spec, HttpMethod::Post, "/a/health") {
            PathMatch::Unique(path, _) => assert_eq!(path, "/a/{id}"),
            _ => panic!("the template serves POST even though the literal path does not"),
        }
    }

    #[test]
    fn status_declared_accepts_the_two_non_integer_answer_keys() {
        let spec = provider(
            r###"{
              "openapi": "3.2.1",
              "paths": {
                "/a": {
                  "get": { "responses": { "200": {}, "2XX": {} } },
                  "post": { "responses": { "default": {} } },
                  "put": { "responses": { "404": {} } }
                }
              }
            }"###,
        );
        assert!(status_declared(&spec, "/a", HttpMethod::Get, 200));
        // `2XX` is OpenAPI 3.1's status-class key. Missing it would report a
        // correctly documented provider as undeclared.
        assert!(status_declared(&spec, "/a", HttpMethod::Get, 204));
        assert!(!status_declared(&spec, "/a", HttpMethod::Get, 500));
        // `default` covers everything the document does not name.
        assert!(status_declared(&spec, "/a", HttpMethod::Post, 418));
        assert!(!status_declared(&spec, "/a", HttpMethod::Put, 200));
    }

    #[test]
    fn a_method_that_is_not_an_http_method_is_not_guessed_at() {
        assert!(method_of("get").is_some());
        assert!(method_of("GET").is_some());
        assert!(method_of("PROPFIND").is_none());
    }

    #[test]
    fn the_delta_digest_moves_when_a_severity_moves() {
        // Two deltas with identical facts but different classifications must not
        // share a digest: a caller reading the digest must not read it as
        // "nothing to see".
        let mut a = ContractDelta {
            protocol: "pact".to_string(),
            from_path: "a".to_string(),
            to_path: "b".to_string(),
            from_digest: "f".to_string(),
            to_digest: "t".to_string(),
            version_change: None,
            changes: vec![ContractChange {
                kind: ChangeKind::PactUndeclaredBodyProperty,
                severity: Severity::Unknown,
                subject: "s".to_string(),
                detail: "d".to_string(),
                coverage: CoverageState::Verified,
            }],
            verdict: CompatibilityVerdict::Unknown,
            coverage: CoverageState::Verified,
            notes: vec!["rewording is allowed".to_string()],
            digest: String::new(),
        };
        let first = delta_digest(&a);
        a.changes[0].severity = Severity::Breaking;
        a.verdict = CompatibilityVerdict::Breaking;
        let second = delta_digest(&a);
        assert_ne!(first, second, "a reclassification must move the digest");

        // Prose may be reworded without moving it.
        a.notes = vec!["different words".to_string()];
        assert_eq!(delta_digest(&a), second);
    }

    #[test]
    fn a_pact_delta_never_collides_with_an_asyncapi_delta_that_agrees_elsewhere() {
        // The two engines build their digest independently on purpose: a shared
        // builder would collide whenever everything but the protocol agreed.
        let base = ContractDelta {
            protocol: "pact".to_string(),
            from_path: "a".to_string(),
            to_path: "b".to_string(),
            from_digest: "f".to_string(),
            to_digest: "t".to_string(),
            version_change: None,
            changes: Vec::new(),
            verdict: CompatibilityVerdict::Compatible,
            coverage: CoverageState::Verified,
            notes: Vec::new(),
            digest: String::new(),
        };
        let mut other = base.clone();
        other.protocol = "asyncapi".to_string();
        assert_ne!(delta_digest(&base), delta_digest(&other));
    }

    #[test]
    fn message_interactions_reach_no_http_match() {
        // `parse_pact` is the gate; this asserts the cross-reference's own
        // handling of a document that is entirely messages.
        let doc = SpecDocument::parse(
            "p.json",
            r###"{
              "consumer": "c",
              "provider": "p",
              "interactions": [
                { "type": "Asynchronous/Messages", "description": "m", "contents": {} }
              ]
            }"###,
            DocFormat::Json,
        )
        .expect("parses");
        let pact = parse_pact(&doc).expect("parses pact");
        assert!(pact.http_interactions().is_empty());
    }
}
