//! Step 37 acceptance: OpenAPI 3.2.1 contract engine.
//!
//! Gate: *"Parses OpenAPI 3.2.1 paths/schemas; flags breaking changes (removed
//! endpoint, added required query param)."*

use autopsy_contracts_distributed::openapi::OpenApiError;
use autopsy_contracts_distributed::{
    ChangeKind, CompatibilityVerdict, ContractDelta, DocFormat, HttpMethod, OpenApiSpec,
    ParamLocation, Severity, SpecDocument, diff_openapi, parse_openapi,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse(json: &str) -> OpenApiSpec {
    let doc = SpecDocument::parse("api.json", json, DocFormat::Json)
        .unwrap_or_else(|e| panic!("fixture is not valid: {e}"));
    parse_openapi(&doc).unwrap_or_else(|e| panic!("fixture does not parse: {e}"))
}

fn compare(from: &str, to: &str) -> ContractDelta {
    diff_openapi(&parse(from), &parse(to))
}

/// Removes a path from a parsed spec.
///
/// Mutating the model rather than string-patching the source: an earlier
/// version of these tests used `.replace()` on the JSON and silently did
/// nothing when the indentation did not match, which turned three assertions
/// into tautologies.
fn without_path(spec: &str, path: &str) -> OpenApiSpec {
    let mut parsed = parse(spec);
    let removed = parsed.paths.remove(path);
    assert!(removed.is_some(), "fixture does not contain path `{path}`");
    parsed
}

fn kinds(delta: &ContractDelta) -> Vec<ChangeKind> {
    delta.changes.iter().map(|c| c.kind).collect()
}

fn has(delta: &ContractDelta, kind: ChangeKind) -> bool {
    delta.changes.iter().any(|c| c.kind == kind)
}

/// A fully-modelled 3.2.1 document: no composition keywords, no unresolved
/// references, so its coverage is `Verified`.
const BASE: &str = r###"{
  "openapi": "3.2.1",
  "info": { "title": "Pets", "version": "1.0.0" },
  "servers": [{ "url": "https://api.example.com/v1" }],
  "paths": {
    "/pets": {
      "get": {
        "operationId": "listPets",
        "parameters": [
          { "name": "limit", "in": "query", "required": false,
            "schema": { "type": "integer" } }
        ],
        "responses": {
          "200": { "description": "ok" },
          "404": { "description": "not found" }
        }
      }
    },
    "/pets/{petId}": {
      "get": {
        "operationId": "getPet",
        "parameters": [
          { "name": "petId", "in": "path", "required": true,
            "schema": { "type": "string" } }
        ],
        "responses": { "200": { "description": "ok" } }
      }
    }
  }
}"###;

// ===========================================================================
// Parsing
// ===========================================================================

#[test]
fn step37_parses_paths_and_operations() {
    let spec = parse(BASE);

    assert_eq!(spec.declared_version, "3.2.1");
    assert_eq!(spec.info_title.as_deref(), Some("Pets"));
    assert_eq!(spec.servers, vec!["https://api.example.com/v1".to_string()]);
    assert_eq!(spec.paths.len(), 2);
    assert!(spec.operation("/pets", HttpMethod::Get).is_some());
    assert!(spec.operation("/pets/{petId}", HttpMethod::Get).is_some());
    assert!(spec.operation("/pets", HttpMethod::Post).is_none());
    assert!(spec.operation("/nope", HttpMethod::Get).is_none());
    assert_eq!(spec.operations().len(), 2);
}

#[test]
fn step37_parses_parameters_with_identity_and_requiredness() {
    let spec = parse(BASE);
    let op = spec.operation("/pets", HttpMethod::Get).unwrap();

    let limit = op
        .parameters
        .values()
        .find(|p| p.identity.name == "limit")
        .unwrap();
    assert_eq!(limit.identity.location, ParamLocation::Query);
    assert!(!limit.required);
    assert_eq!(limit.schema.types.iter().next().unwrap(), "integer");

    let get_pet = spec.operation("/pets/{petId}", HttpMethod::Get).unwrap();
    let pet_id = get_pet.parameters.values().next().unwrap();
    assert_eq!(pet_id.identity.location, ParamLocation::Path);
    assert!(pet_id.required);
}

#[test]
fn step37_parses_response_status_sets() {
    let spec = parse(BASE);
    let op = spec.operation("/pets", HttpMethod::Get).unwrap();
    assert_eq!(
        op.responses.keys().cloned().collect::<Vec<_>>(),
        vec!["200".to_string(), "404".to_string()]
    );
}

#[test]
fn step37_a_fully_modelled_document_is_verified_with_nothing_hidden() {
    let spec = parse(BASE);
    assert_eq!(
        spec.coverage,
        autopsy_domain::CoverageState::Verified,
        "notes: {:?}",
        spec.notes
    );
    assert!(
        spec.unmodeled_pointers.is_empty(),
        "{:?}",
        spec.unmodeled_pointers
    );
    assert!(
        spec.unresolved_refs.is_empty(),
        "{:?}",
        spec.unresolved_refs
    );
}

#[test]
fn step37_parameter_identity_is_name_and_location_not_name_alone() {
    // The same name in two locations is two distinct parameters. Keying on the
    // name alone would silently merge them and hide a removal.
    let spec = parse(
        r###"{
          "openapi": "3.2.1",
          "paths": { "/x": { "get": {
            "parameters": [
              { "name": "token", "in": "query", "schema": { "type": "string" } },
              { "name": "token", "in": "header", "schema": { "type": "string" } }
            ],
            "responses": { "200": { "description": "ok" } }
          } } }
        }"###,
    );
    let op = spec.operation("/x", HttpMethod::Get).unwrap();
    assert_eq!(op.parameters.len(), 2);
    let locations: Vec<ParamLocation> = op.parameters.keys().map(|k| k.location).collect();
    assert_eq!(locations, vec![ParamLocation::Query, ParamLocation::Header]);
}

#[test]
fn step37_path_level_parameters_are_inherited_and_overridable() {
    let spec = parse(
        r###"{
          "openapi": "3.2.1",
          "paths": { "/x/{id}": {
            "parameters": [
              { "name": "id", "in": "path", "required": true, "schema": { "type": "string" } },
              { "name": "trace", "in": "header", "schema": { "type": "string" } }
            ],
            "get": {
              "responses": { "200": { "description": "ok" } }
            },
            "put": {
              "parameters": [
                { "name": "trace", "in": "header", "required": true, "schema": { "type": "string" } }
              ],
              "responses": { "200": { "description": "ok" } }
            }
          } }
        }"###,
    );
    let get = spec.operation("/x/{id}", HttpMethod::Get).unwrap();
    let put = spec.operation("/x/{id}", HttpMethod::Put).unwrap();

    assert_eq!(
        get.parameters.len(),
        2,
        "GET inherits both path-level params"
    );
    // PUT's own declaration replaces the inherited one rather than adding to it.
    assert_eq!(put.parameters.len(), 2);
    let put_trace = put
        .parameters
        .values()
        .find(|p| p.identity.name == "trace")
        .unwrap();
    assert!(
        put_trace.required,
        "operation-level declaration must override"
    );
}

#[test]
fn step37_path_parameter_must_appear_in_the_route() {
    let spec = parse(
        r###"{
          "openapi": "3.2.1",
          "paths": { "/x": { "get": {
            "parameters": [
              { "name": "ghost", "in": "path", "required": true, "schema": { "type": "string" } }
            ],
            "responses": { "200": { "description": "ok" } }
          } } }
        }"###,
    );
    let op = spec.operation("/x", HttpMethod::Get).unwrap();
    assert!(
        op.parameters.is_empty(),
        "a path parameter absent from the route is dropped, not invented: {:?}",
        op.parameters.keys()
    );
    assert!(
        op.notes
            .iter()
            .any(|n| n.contains("does not appear in route")),
        "{:?}",
        op.notes
    );
    assert_eq!(op.coverage, autopsy_domain::CoverageState::Partial);
}

#[test]
fn step37_resolves_local_refs_in_parameters() {
    let spec = parse(
        r###"{
          "openapi": "3.2.1",
          "components": {
            "parameters": {
              "Limit": { "name": "limit", "in": "query", "schema": { "type": "integer" } }
            }
          },
          "paths": { "/x": { "get": {
            "parameters": [ { "$ref": "#/components/parameters/Limit" } ],
            "responses": { "200": { "description": "ok" } }
          } } }
        }"###,
    );
    let op = spec.operation("/x", HttpMethod::Get).unwrap();
    assert_eq!(op.parameters.len(), 1);
    assert_eq!(
        op.parameters.values().next().unwrap().identity.name,
        "limit"
    );
}

#[test]
fn step37_request_body_content_is_keyed_by_media_type() {
    let spec = parse(
        r###"{
          "openapi": "3.2.1",
          "paths": { "/x": { "post": {
            "requestBody": {
              "required": true,
              "content": {
                "application/json": { "schema": { "type": "object",
                  "required": ["a"], "properties": { "a": { "type": "string" } } } },
                "application/xml": { "schema": { "type": "object" } }
              }
            },
            "responses": { "201": { "description": "ok" } }
          } } }
        }"###,
    );
    let rb = spec
        .operation("/x", HttpMethod::Post)
        .unwrap()
        .request_body
        .as_ref()
        .unwrap();
    assert!(rb.required);
    assert_eq!(
        rb.content.keys().cloned().collect::<Vec<_>>(),
        vec![
            "application/json".to_string(),
            "application/xml".to_string()
        ]
    );
    let json = &rb.content["application/json"];
    assert_eq!(json.required.iter().next().unwrap(), "a");
    assert!(json.properties.contains_key("a"));
}

#[test]
fn step37_a_document_without_paths_is_partial_not_an_error() {
    let spec = parse(r###"{ "openapi": "3.2.1", "webhooks": { "ping": {} } }"###);
    assert!(spec.paths.is_empty());
    assert_eq!(spec.coverage, autopsy_domain::CoverageState::Partial);
    assert!(
        spec.notes.iter().any(|n| n.contains("no `paths`")),
        "{:?}",
        spec.notes
    );
}

// ===========================================================================
// Version handling
// ===========================================================================

#[test]
fn step37_unsupported_version_is_an_error_not_a_garbage_diff() {
    // Swagger 2.0 has a different structure entirely. Parsing it leniently
    // would yield an empty path set, and the diff would then report every
    // endpoint of the other side as removed.
    let doc = SpecDocument::parse(
        "swagger.json",
        r###"{ "swagger": "2.0", "paths": { "/x": {} } }"###,
        DocFormat::Json,
    )
    .unwrap();
    let err = parse_openapi(&doc).unwrap_err();
    assert!(
        matches!(err, OpenApiError::MissingVersion { .. }),
        "{err:?}"
    );

    let doc = SpecDocument::parse(
        "api.json",
        r###"{ "openapi": "4.0.0", "paths": {} }"###,
        DocFormat::Json,
    )
    .unwrap();
    assert!(matches!(
        parse_openapi(&doc).unwrap_err(),
        OpenApiError::UnsupportedVersion { .. }
    ));
}

#[test]
fn step37_declares_coverage_loss_for_older_minor_versions() {
    let spec31 = parse(r###"{ "openapi": "3.1.0", "paths": {} }"###);
    assert_eq!(spec31.coverage, autopsy_domain::CoverageState::Partial);
    assert!(
        spec31.notes.iter().any(|n| n.contains("3.1.x")),
        "{:?}",
        spec31.notes
    );
}

#[test]
fn step37_openapi_30_nullable_is_honoured() {
    // In 3.0 a nullable type is `type: string` plus `nullable: true`. In 3.1+
    // it is `type: [string, "null"]`. Reading a 3.0 document without
    // translating `nullable` would make an unchanged type look narrowed.
    let spec30 = parse(
        r###"{
          "openapi": "3.0.3",
          "paths": { "/x": { "get": {
            "parameters": [
              { "name": "q", "in": "query", "schema": { "type": "string", "nullable": true } }
            ],
            "responses": { "200": { "description": "ok" } }
          } } }
        }"###,
    );
    let schema = spec30
        .operation("/x", HttpMethod::Get)
        .unwrap()
        .parameters
        .values()
        .next()
        .unwrap()
        .schema
        .clone();
    assert_eq!(
        schema.types.iter().cloned().collect::<Vec<_>>(),
        vec!["null".to_string(), "string".to_string()]
    );
}

// ===========================================================================
// GATE: removed endpoint
// ===========================================================================

#[test]
fn step37_gate_removed_endpoint_is_breaking() {
    let delta = diff_openapi(&parse(BASE), &without_path(BASE, "/pets/{petId}"));

    assert!(
        has(&delta, ChangeKind::EndpointRemoved),
        "{:?}",
        kinds(&delta)
    );
    assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
    assert!(!delta.verdict.is_approval());

    let removed = delta
        .changes
        .iter()
        .find(|c| c.kind == ChangeKind::EndpointRemoved)
        .unwrap();
    assert_eq!(removed.severity, Severity::Breaking);
    assert_eq!(removed.subject, "/pets/{petId}");
    assert!(removed.detail.contains("/pets/{petId}"));
}

#[test]
fn step37_gate_removed_method_is_breaking() {
    let to = BASE.replace(
        r###""/pets": {
      "get": {"###,
        r###""/pets": {
      "delete": { "responses": { "204": { "description": "gone" } } },
      "get": {"###,
    );
    let delta = compare(BASE, &to);
    assert!(has(&delta, ChangeKind::MethodAdded), "{:?}", kinds(&delta));
    assert_eq!(
        delta.verdict,
        CompatibilityVerdict::Compatible,
        "adding is not breaking"
    );

    // Now the reverse: remove GET from /pets.
    let to_removed = to.replace(
        r###""get": {
        "operationId": "listPets","###,
        r###""patch": { "responses": { "200": { "description": "ok" } } },
      "unused": {
        "operationId": "listPets","###,
    );
    let delta = compare(&to, &to_removed);
    assert!(
        has(&delta, ChangeKind::MethodRemoved),
        "{:?}",
        kinds(&delta)
    );
    assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
}

// ===========================================================================
// GATE: added required query parameter
// ===========================================================================

#[test]
fn step37_gate_added_required_query_parameter_is_breaking() {
    let to = BASE.replace(
        r###"{ "name": "limit", "in": "query", "required": false,
            "schema": { "type": "integer" } }"###,
        r###"{ "name": "limit", "in": "query", "required": false,
            "schema": { "type": "integer" } },
          { "name": "apiKey", "in": "query", "required": true,
            "schema": { "type": "string" } }"###,
    );
    assert_ne!(to, BASE, "fixture edit failed");

    let delta = compare(BASE, &to);

    assert!(
        has(&delta, ChangeKind::RequiredParameterAdded),
        "{:?}",
        kinds(&delta)
    );
    assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);

    let added = delta
        .changes
        .iter()
        .find(|c| c.kind == ChangeKind::RequiredParameterAdded)
        .unwrap();
    assert!(
        added.subject.contains("query `apiKey`"),
        "{}",
        added.subject
    );
    assert!(
        added.detail.contains("existing callers do not send it"),
        "{}",
        added.detail
    );
}

#[test]
fn step37_added_optional_query_parameter_is_not_breaking() {
    let to = BASE.replace(
        r###"{ "name": "limit", "in": "query", "required": false,
            "schema": { "type": "integer" } }"###,
        r###"{ "name": "limit", "in": "query", "required": false,
            "schema": { "type": "integer" } },
          { "name": "cursor", "in": "query", "required": false,
            "schema": { "type": "string" } }"###,
    );
    // Without this guard the test would still pass if the edit silently did
    // nothing: "no change" is also "not breaking".
    assert_ne!(to, BASE, "fixture edit was a no-op");
    let delta = compare(BASE, &to);

    assert!(
        has(&delta, ChangeKind::OptionalParameterAdded),
        "{:?}",
        kinds(&delta)
    );
    assert!(delta.breaking().is_empty(), "{:?}", delta.breaking());
    assert_eq!(delta.verdict, CompatibilityVerdict::Compatible);
    assert!(delta.verdict.is_approval());
}

#[test]
fn step37_parameter_becoming_required_is_breaking() {
    let to = BASE.replace(
        r###"{ "name": "limit", "in": "query", "required": false,"###,
        r###"{ "name": "limit", "in": "query", "required": true,"###,
    );
    let delta = compare(BASE, &to);
    assert!(
        has(&delta, ChangeKind::ParameterBecameRequired),
        "{:?}",
        kinds(&delta)
    );
    assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
}

#[test]
fn step37_a_path_parameter_is_required_whatever_the_document_says() {
    // The specification mandates it. A document claiming otherwise is wrong,
    // and believing it would make a required path parameter look optional.
    let spec = parse(
        r###"{
          "openapi": "3.2.1",
          "paths": { "/x/{id}": { "get": {
            "parameters": [ { "name": "id", "in": "path", "schema": { "type": "string" } } ],
            "responses": { "200": { "description": "ok" } }
          } } }
        }"###,
    );
    let param = spec
        .operation("/x/{id}", HttpMethod::Get)
        .unwrap()
        .parameters
        .values()
        .next()
        .unwrap();
    assert!(param.required);
}

// ===========================================================================
// Schema-level classification
// ===========================================================================

#[test]
fn step37_type_narrowing_is_breaking_and_widening_is_not() {
    let widened = BASE.replace(
        r###"{ "name": "limit", "in": "query", "required": false,
            "schema": { "type": "integer" } }"###,
        r###"{ "name": "limit", "in": "query", "required": false,
            "schema": { "type": ["integer", "string"] } }"###,
    );
    let delta = compare(BASE, &widened);
    assert!(
        has(&delta, ChangeKind::ParameterTypeWidened),
        "{:?}",
        kinds(&delta)
    );
    assert_eq!(delta.verdict, CompatibilityVerdict::Compatible);

    let delta = compare(&widened, BASE);
    assert!(
        has(&delta, ChangeKind::ParameterTypeNarrowed),
        "{:?}",
        kinds(&delta)
    );
    assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
}

#[test]
fn step37_removed_enum_value_is_breaking() {
    let base = BASE.replace(
        r###""schema": { "type": "integer" } }"###,
        r###""schema": { "type": "string", "enum": ["asc", "desc"] } }"###,
    );
    let to = base.replace(r###"["asc", "desc"]"###, r###"["asc"]"###);
    let delta = compare(&base, &to);

    assert!(
        has(&delta, ChangeKind::ParameterEnumValueRemoved),
        "{:?}",
        kinds(&delta)
    );
    assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
    let removed = delta
        .changes
        .iter()
        .find(|c| c.kind == ChangeKind::ParameterEnumValueRemoved)
        .unwrap();
    assert!(removed.detail.contains("`desc`"), "{}", removed.detail);
}

#[test]
fn step37_introducing_an_enum_where_none_existed_is_a_narrowing() {
    // `base` is the unmodified fixture; the second edit introduces a
    // constraint that was not there. Keeping `base` as a distinct binding
    // (rather than a no-op `.replace`) means the assertion cannot degenerate
    // into comparing the fixture with itself.
    let to = BASE.replace(
        r###""schema": { "type": "integer" } }"###,
        r###""schema": { "type": "integer", "enum": [1, 2] } }"###,
    );
    assert_ne!(to, BASE, "fixture edit was a no-op");
    let delta = compare(BASE, &to);
    assert!(
        has(&delta, ChangeKind::ParameterTypeNarrowed),
        "{:?}",
        kinds(&delta)
    );
    assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
}

#[test]
fn step37_required_request_property_added_is_breaking() {
    let base = r###"{
      "openapi": "3.2.1",
      "paths": { "/x": { "post": {
        "requestBody": { "content": { "application/json": { "schema": {
          "type": "object", "required": ["a"], "properties": { "a": { "type": "string" } } } } } },
        "responses": { "200": { "description": "ok" } }
      } } }
    }"###;
    let to = r###"{
      "openapi": "3.2.1",
      "paths": { "/x": { "post": {
        "requestBody": { "content": { "application/json": { "schema": {
          "type": "object", "required": ["a", "b"],
          "properties": { "a": { "type": "string" }, "b": { "type": "string" } } } } } },
        "responses": { "200": { "description": "ok" } }
      } } }
    }"###;
    let delta = compare(base, to);
    assert!(
        has(&delta, ChangeKind::RequiredPropertyAdded),
        "{:?}",
        kinds(&delta)
    );
    assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
}

#[test]
fn step37_disallowing_additional_properties_is_breaking() {
    let base = r###"{
      "openapi": "3.2.1",
      "paths": { "/x": { "post": {
        "requestBody": { "content": { "application/json": { "schema": {
          "type": "object", "properties": { "a": { "type": "string" } } } } } },
        "responses": { "200": { "description": "ok" } }
      } } }
    }"###;
    let to = base.replace(
        r###""type": "object", "properties""###,
        r###""type": "object", "additionalProperties": false, "properties""###,
    );
    let delta = compare(base, &to);
    assert!(
        has(&delta, ChangeKind::RequestAdditionalPropertiesDisallowed),
        "{:?}",
        kinds(&delta)
    );
    assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
}

#[test]
fn step37_removed_media_type_is_breaking() {
    let base = r###"{
      "openapi": "3.2.1",
      "paths": { "/x": { "post": {
        "requestBody": { "content": {
          "application/json": { "schema": { "type": "object" } },
          "application/xml": { "schema": { "type": "object" } } } },
        "responses": { "200": { "description": "ok" } }
      } } }
    }"###;
    let mut to = parse(base);
    to.paths
        .get_mut("/x")
        .unwrap()
        .operations
        .get_mut(&HttpMethod::Post)
        .unwrap()
        .request_body
        .as_mut()
        .unwrap()
        .content
        .remove("application/xml");
    let delta = diff_openapi(&parse(base), &to);
    assert!(
        has(&delta, ChangeKind::MediaTypeRemoved),
        "{:?}",
        kinds(&delta)
    );
    assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
}

#[test]
fn step37_adding_an_optional_request_body_is_not_breaking() {
    let base = r###"{
      "openapi": "3.2.1",
      "paths": { "/x": { "post": {
        "responses": { "200": { "description": "ok" } }
      } } }
    }"###;
    let with_optional = base.replace(
        r###""responses": { "200": { "description": "ok" } }"###,
        r###""requestBody": { "required": false, "content": { "application/json": { "schema": { "type": "object" } } } },
        "responses": { "200": { "description": "ok" } }"###,
    );
    let delta = compare(base, &with_optional);
    assert!(
        has(&delta, ChangeKind::OptionalRequestBodyAdded),
        "{:?}",
        kinds(&delta)
    );
    assert!(
        delta.breaking().is_empty(),
        "an added *optional* body rejects nothing: {:?}",
        delta.breaking()
    );
    assert_eq!(delta.verdict, CompatibilityVerdict::Compatible);

    // ...but a *required* one is.
    let with_required = with_optional.replace(
        r###""required": false, "content""###,
        r###""required": true, "content""###,
    );
    let delta = compare(base, &with_required);
    assert!(
        has(&delta, ChangeKind::RequestBodyAdded),
        "{:?}",
        kinds(&delta)
    );
    assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
}

#[test]
fn step37_removed_response_status_is_breaking() {
    let to = BASE.replace(
        r###""200": { "description": "ok" },
          "404": { "description": "not found" }"###,
        r###""200": { "description": "ok" }"###,
    );
    let delta = compare(BASE, &to);
    assert!(
        has(&delta, ChangeKind::ResponseStatusRemoved),
        "{:?}",
        kinds(&delta)
    );
    assert_eq!(delta.verdict, CompatibilityVerdict::Breaking);
}

// ===========================================================================
// The honesty rule: `Compatible` is the only verdict that claims completeness
// ===========================================================================

#[test]
fn step37_identical_documents_are_compatible() {
    let delta = compare(BASE, BASE);
    assert!(delta.changes.is_empty(), "{:?}", delta.changes);
    assert_eq!(delta.verdict, CompatibilityVerdict::Compatible);
    assert_eq!(delta.coverage, autopsy_domain::CoverageState::Verified);
    assert!(delta.notes.is_empty(), "{:?}", delta.notes);
    assert!(delta.version_change.is_none());
}

#[test]
fn step37_an_unchanged_but_unmodelled_document_is_unknown_never_compatible() {
    // `allOf` is recorded but not interpreted. Two byte-identical documents
    // containing it must NOT be reported as compatible: a change inside the
    // composed subtree would be invisible, and `Compatible` is precisely the
    // claim that nothing was invisible.
    let spec = r###"{
      "openapi": "3.2.1",
      "paths": { "/x": { "get": {
        "parameters": [ { "name": "q", "in": "query", "schema": {
          "allOf": [ { "type": "string" } ] } } ],
        "responses": { "200": { "description": "ok" } }
      } } }
    }"###;
    let delta = compare(spec, spec);

    assert_eq!(delta.verdict, CompatibilityVerdict::Unknown);
    assert!(!delta.verdict.is_approval());
    assert!(delta.breaking().is_empty(), "nothing definite was found");
    assert!(
        has(&delta, ChangeKind::IncomparableSubtree),
        "{:?}",
        kinds(&delta)
    );
    assert!(
        delta.changes.iter().any(|c| c.subject.contains("allOf")),
        "the unmodelled pointer must be named: {:?}",
        delta.changes.iter().map(|c| &c.subject).collect::<Vec<_>>()
    );
    assert!(
        delta
            .notes
            .iter()
            .any(|n| n.contains("could not be classified")),
        "the verdict reason must say why: {:?}",
        delta.notes
    );
    assert!(
        delta.notes.iter().any(|n| n.contains("does not interpret")),
        "the unmodelled construct must be disclosed in prose, not only as a pointer: {:?}",
        delta.notes
    );
}

#[test]
fn step37_an_unresolvable_ref_blocks_a_compatible_verdict() {
    let spec = r###"{
      "openapi": "3.2.1",
      "paths": { "/x": { "get": {
        "parameters": [ { "name": "q", "in": "query", "schema": { "$ref": "#/components/schemas/Missing" } } ],
        "responses": { "200": { "description": "ok" } }
      } } }
    }"###;
    let parsed = parse(spec);
    assert_eq!(parsed.coverage, autopsy_domain::CoverageState::Unknown);
    assert!(!parsed.unresolved_refs.is_empty());

    let delta = compare(spec, spec);
    assert_eq!(delta.verdict, CompatibilityVerdict::Unknown);
    assert!(!delta.verdict.is_approval());
}

#[test]
fn step37_a_definite_break_is_reported_even_with_incomplete_coverage() {
    // A positive finding needs no completeness. This is the other half of the
    // verdict algebra: degradation must not silence a real break.
    let from = r###"{
      "openapi": "3.2.1",
      "paths": {
        "/kept": { "get": {
          "parameters": [ { "name": "q", "in": "query", "schema": { "allOf": [ { "type": "string" } ] } } ],
          "responses": { "200": { "description": "ok" } }
        } },
        "/gone": { "get": { "responses": { "200": { "description": "ok" } } } }
      }
    }"###;
    let to = without_path(from, "/gone");
    let delta = diff_openapi(&parse(from), &to);

    assert_eq!(delta.coverage, autopsy_domain::CoverageState::Partial);
    assert!(has(&delta, ChangeKind::EndpointRemoved));
    assert_eq!(
        delta.verdict,
        CompatibilityVerdict::Breaking,
        "a removed endpoint is breaking regardless of what else was modelled"
    );
}

#[test]
fn step37_a_changed_response_schema_is_unknown_not_breaking() {
    // Response-side direction is not request-side direction. Asserting
    // `Breaking` would be a claim this engine cannot support; asserting
    // `Compatible` would be a false approval. It says `Unknown`.
    let base = r###"{
      "openapi": "3.2.1",
      "paths": { "/x": { "get": {
        "responses": { "200": { "description": "ok", "content": {
          "application/json": { "schema": { "type": "object",
            "properties": { "a": { "type": "string" } } } } } } }
      } } }
    }"###;
    let to = base.replace(
        r###""properties": { "a": { "type": "string" } }"###,
        r###""properties": { "a": { "type": "string" }, "b": { "type": "string" } }"###,
    );
    let delta = compare(base, &to);

    assert!(
        has(&delta, ChangeKind::ResponseSchemaChanged),
        "{:?}",
        kinds(&delta)
    );
    assert!(delta.breaking().is_empty(), "{:?}", delta.breaking());
    assert_eq!(delta.verdict, CompatibilityVerdict::Unknown);
    assert_eq!(delta.unclassified().len(), 1);
}

#[test]
fn step37_an_unchanged_response_schema_does_not_block_compatibility() {
    let base = r###"{
      "openapi": "3.2.1",
      "paths": { "/x": { "get": {
        "responses": { "200": { "description": "ok", "content": {
          "application/json": { "schema": { "type": "object",
            "properties": { "a": { "type": "string" } } } } } } }
      } } }
    }"###;
    let delta = compare(base, base);
    assert_eq!(delta.verdict, CompatibilityVerdict::Compatible);
    assert!(delta.changes.is_empty(), "{:?}", kinds(&delta));
}

// ===========================================================================
// Robustness
// ===========================================================================

#[test]
fn step37_a_cyclic_ref_terminates_and_degrades() {
    let spec = r###"{
      "openapi": "3.2.1",
      "components": { "schemas": {
        "A": { "$ref": "#/components/schemas/B" },
        "B": { "$ref": "#/components/schemas/A" }
      } },
      "paths": { "/x": { "get": {
        "parameters": [ { "name": "q", "in": "query", "schema": { "$ref": "#/components/schemas/A" } } ],
        "responses": { "200": { "description": "ok" } }
      } } }
    }"###;
    let parsed = parse(spec);
    assert!(parsed.coverage != autopsy_domain::CoverageState::Verified);
    assert!(
        parsed.notes.iter().any(|n| n.contains("cyclic")),
        "{:?}",
        parsed.notes
    );
    // And the diff on it does not claim compatibility.
    assert_eq!(compare(spec, spec).verdict, CompatibilityVerdict::Unknown);
}

#[test]
fn step37_a_self_referential_schema_declaration_is_not_followed_forever() {
    let spec = r###"{
      "openapi": "3.2.1",
      "components": { "schemas": { "Node": {
        "type": "object", "properties": { "next": { "$ref": "#/components/schemas/Node" } } } } },
      "paths": { "/x": { "get": {
        "responses": { "200": { "description": "ok", "content": {
          "application/json": { "schema": { "$ref": "#/components/schemas/Node" } } } } }
      } } }
    }"###;
    let parsed = parse(spec);
    assert!(
        parsed.coverage != autopsy_domain::CoverageState::Verified,
        "a recursive schema is disclosed, not silently accepted"
    );
    assert_eq!(parsed.paths.len(), 1, "the path surface still parsed");
}

#[test]
fn step37_external_refs_are_reported_not_silently_ignored() {
    let doc = SpecDocument::parse(
        "api.yaml",
        "openapi: 3.2.1\npaths:\n  /x:\n    get:\n      responses:\n        '200':\n          description: ok\n",
        DocFormat::Yaml,
    )
    .unwrap();
    let spec = parse_openapi(&doc).unwrap();
    assert_eq!(spec.declared_version, "3.2.1");
    assert!(spec.operation("/x", HttpMethod::Get).is_some());

    let with_external = SpecDocument::parse(
        "api.json",
        r###"{ "openapi": "3.2.1", "paths": { "/x": { "get": {
          "parameters": [ { "$ref": "./shared.yaml#/Limit" } ],
          "responses": { "200": { "description": "ok" } } } } } }"###,
        DocFormat::Json,
    )
    .unwrap();
    assert_eq!(with_external.external_refs.len(), 1);
    assert!(with_external.notes.iter().any(|n| n.contains("external")));
}

// ===========================================================================
// Determinism
// ===========================================================================

#[test]
fn step37_delta_is_reproducible_across_100_runs() {
    let to = BASE.replace(
        r###"{ "name": "limit", "in": "query", "required": false,
            "schema": { "type": "integer" } }"###,
        r###"{ "name": "limit", "in": "query", "required": true,
            "schema": { "type": "integer" } },
          { "name": "apiKey", "in": "query", "required": true,
            "schema": { "type": "string" } }"###,
    );
    let baseline = compare(BASE, &to);
    for run in 0..100 {
        let delta = compare(BASE, &to);
        assert_eq!(delta, baseline, "run {run} diverged");
        assert_eq!(delta.digest, baseline.digest, "run {run} digest diverged");
    }
    assert_eq!(baseline.digest.len(), 64);
}

#[test]
fn step37_key_reordering_does_not_change_the_parse_or_the_delta() {
    // Same contract, keys written in a different order. Reformatting is not a
    // contract change, so neither the document digest nor the delta may move.
    let reordered = r###"{
      "paths": {
        "/pets/{petId}": {
          "get": {
            "responses": { "200": { "description": "ok" } },
            "parameters": [
              { "schema": { "type": "string" }, "required": true, "in": "path", "name": "petId" }
            ],
            "operationId": "getPet"
          }
        },
        "/pets": {
          "get": {
            "responses": {
              "404": { "description": "not found" },
              "200": { "description": "ok" }
            },
            "parameters": [
              { "schema": { "type": "integer" }, "required": false, "in": "query", "name": "limit" }
            ],
            "operationId": "listPets"
          }
        }
      },
      "servers": [{ "url": "https://api.example.com/v1" }],
      "info": { "version": "1.0.0", "title": "Pets" },
      "openapi": "3.2.1"
    }"###;

    let a = parse(BASE);
    let b = parse(reordered);
    assert_eq!(
        a.digest(),
        b.digest(),
        "reformatting moved the contract digest"
    );
    assert_eq!(a.paths, b.paths, "reformatting changed the parsed model");

    let delta = compare(BASE, reordered);
    assert_eq!(delta.verdict, CompatibilityVerdict::Compatible);
    assert!(delta.changes.is_empty(), "{:?}", kinds(&delta));
}

#[test]
fn step37_reference_layout_does_not_change_the_contract_digest() {
    // A schema declared inline and the same schema behind a `$ref` are the same
    // contract. The digest is taken over the resolved model for exactly this
    // reason.
    let inline = r###"{
      "openapi": "3.2.1",
      "paths": { "/x": { "get": {
        "parameters": [ { "name": "q", "in": "query", "schema": { "type": "string" } } ],
        "responses": { "200": { "description": "ok" } }
      } } }
    }"###;
    let referenced = r###"{
      "openapi": "3.2.1",
      "components": { "parameters": {
        "Q": { "name": "q", "in": "query", "schema": { "type": "string" } } } },
      "paths": { "/x": { "get": {
        "parameters": [ { "$ref": "#/components/parameters/Q" } ],
        "responses": { "200": { "description": "ok" } }
      } } }
    }"###;
    assert_eq!(parse(inline).digest(), parse(referenced).digest());
}

// ===========================================================================
// Unknown HTTP methods and extensions are not treated as surface
// ===========================================================================

#[test]
fn step37_non_method_keys_are_not_operations() {
    let spec = parse(
        r###"{
          "openapi": "3.2.1",
          "paths": { "/x": {
            "summary": "a path item summary",
            "x-internal": true,
            "get": { "responses": { "200": { "description": "ok" } } }
          } }
        }"###,
    );
    let item = &spec.paths["/x"];
    assert_eq!(item.operations.len(), 1);
    assert!(item.operations.contains_key(&HttpMethod::Get));
}
