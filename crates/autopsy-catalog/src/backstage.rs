//! Backstage `catalog-info.yaml` ingestion (`R28`).
//!
//! Reads the five kinds the design names — `Component`, `API`, `Resource`, `System`,
//! `Domain` — plus the ownership and relationship fields needed to compute an
//! organisational blast radius.
//!
//! # What is modelled, and what is said out loud
//!
//! | Modelled | Where it goes |
//! |---|---|
//! | `metadata.name`, `namespace`, `title`, `description` | entity identity |
//! | `metadata.annotations` — `github.com/project-slug` | the GitHub linkage `R29``R37` |
//! | `spec.owner` | team ownership |
//! | `spec.system`, `spec.domain`, `spec.partOf` | hierarchy |
//! | `spec.dependsOn` | consumed relationships |
//! | `spec.providesApis`, `spec.consumesApis` | API surface |
//! | `spec.type`, `spec.lifecycle` | labels, recorded verbatim |
//!
//! Every **other** key in `spec` is recorded by name in
//! [`CatalogEntity::unmodelled_spec_keys`] and degrades that entity to `Partial`. The
//! alternative — reading the keys this crate knows and saying nothing about the rest — is
//! how a reader comes to believe a document was fully understood.
//!
//! Two consequences of that rule are worth naming:
//!
//! * `spec.definition` on an `API` entity is the contract payload itself. It is **not**
//!   read here: parsing OpenAPI or AsyncAPI belongs to Pillar 3's engines, and reading it
//!   twice in two crates is how two answers come to disagree. Its presence is recorded and
//!   it degrades coverage.
//! * An entity kind outside the five (`Group`, `User`, `Location`, `Template`, …) is
//!   counted in [`super::CatalogReport::unmodelled_kinds`] rather than dropped or errored.
//!   A catalog is not invalid because it also contains a template.
//!
//! # A refusal, not a guess
//!
//! An unknown `apiVersion` is an **error**: an entity claiming a schema version this crate
//! has never read may use any of its fields for anything, and the safer reading is not
//! available. A missing `kind` or `metadata.name` is an error for the same reason — an
//! entity with no identity cannot be referenced, and inventing one would put a fabricated
//! id into the ownership graph.
//!
//! `spec.owner` missing on a `Component` is **not** an error: it is a real and common gap,
//! and it degrades coverage because a component without an owner cannot contribute to
//! organisational blast radius.
//!
//! # Duplicate keys are invisible here, and that is stated
//!
//! YAML permits a mapping key to appear more than once, and `serde_yaml` resolves it
//! silently. For a field like `spec.owner` — where two values mean two teams — silent
//! resolution is uncomfortable, and this crate cannot detect it: the duplicate is gone
//! before the value is reachable. The mitigation is the input size bound in
//! [`mod@super::discover`], and the limitation is recorded in the evidence document rather
//! than left as an assumption.

use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::{BTreeMap, BTreeSet};

use crate::discover::CatalogError;

/// The entity kinds this crate models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Component,
    Api,
    Resource,
    System,
    Domain,
}

impl EntityKind {
    pub fn all() -> &'static [EntityKind] {
        &[
            Self::Component,
            Self::Api,
            Self::Resource,
            Self::System,
            Self::Domain,
        ]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Component => "component",
            Self::Api => "api",
            Self::Resource => "resource",
            Self::System => "system",
            Self::Domain => "domain",
        }
    }

    /// Case-insensitive, because Backstage capitalises the `kind` field and lowercases it
    /// inside a reference, and both spellings name the same thing.
    pub fn parse(name: &str) -> Option<Self> {
        let lowered = name.to_ascii_lowercase();
        Self::all()
            .iter()
            .copied()
            .find(|kind| kind.as_str() == lowered)
    }

    /// Spec keys this crate reads for this kind. Anything else is recorded as unmodelled.
    fn modelled_spec_keys(self) -> &'static [&'static str] {
        match self {
            // Exactly the keys the reader below reads. `definition` is deliberately
            // absent: the contract payload is not read here, so it must be reported.
            Self::Component => &[
                "type",
                "owner",
                "system",
                "lifecycle",
                "dependsOn",
                "providesApis",
                "consumesApis",
                "subcomponentOf",
            ],
            Self::Api => &["type", "owner", "system", "lifecycle"],
            Self::Resource => &["type", "owner", "system", "dependsOn"],
            Self::System => &["owner", "domain"],
            Self::Domain => &["owner"],
        }
    }
}

/// A parsed Backstage entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntity {
    pub kind: EntityKind,
    pub name: String,
    pub namespace: String,
    /// Canonical `kind:namespace/name`, the form every relationship in Backstage uses.
    pub reference: String,
    pub title: Option<String>,
    pub description: Option<String>,
    /// Owner as declared, canonicalised. `None` when absent.
    pub owner: Option<String>,
    /// The owner's kind (`group` for a team, `user` for a person), when the reference
    /// carried one.
    pub owner_kind: Option<String>,
    /// `Group` owners roll up to a team; a `User` owner does not, and the distinction is
    /// kept because "which team owns this" has no answer for a person.
    pub owner_is_team: bool,
    pub system: Option<String>,
    pub domain: Option<String>,
    /// `spec.subcomponentOf`. The component's parent, used to inherit a system.
    pub subcomponent_of: Option<String>,
    pub lifecycle: Option<String>,
    /// `spec.type` — free text in Backstage, recorded verbatim and never interpreted.
    pub component_type: Option<String>,
    pub depends_on: Vec<String>,
    pub provides_apis: Vec<String>,
    pub consumes_apis: Vec<String>,
    /// `metadata.annotations["github.com/project-slug"]`, the GitHub linkage `R29``R37`.
    pub github_project: Option<String>,
    /// The file this entity came from, POSIX-relative to the catalog root.
    pub source_path: String,
    /// Spec keys present and **not** read, sorted.
    pub unmodelled_spec_keys: Vec<String>,
    pub coverage: autopsy_domain::CoverageState,
}

/// A canonical Backstage entity reference.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EntityRef {
    pub kind: String,
    pub namespace: String,
    pub name: String,
}

impl EntityRef {
    /// Canonical form: lowercase kind, with the namespace always explicit.
    ///
    /// Always spelling the namespace means `component:checkout` and
    /// `component:default/checkout` are **one** entity rather than two, which is exactly
    /// the duplicate-identity bug a graph keyed on strings would otherwise inherit.
    pub fn canonical(&self) -> String {
        format!("{}:{}/{}", self.kind, self.namespace, self.name)
    }

    /// The grammar Backstage itself parses: one segment is a name, two are
    /// `namespace/name` or `kind:namespace/name`, and three is a refusal.
    ///
    /// The asymmetry is Backstage's, and this crate keeps it rather than smoothing it over:
    /// without a colon the **first** of two segments is a namespace, so
    /// `component/checkout` is `component:component/checkout` and not
    /// `component:default/checkout`. Reading a kind out of the left half would be this
    /// crate inventing an interpretation the format does not state, and the mistake would
    /// be invisible — both readings produce a well-formed reference.
    ///
    /// The kind of a bare name comes from the **field**, not from the string, so this is
    /// the version that takes it. An `owner` is a group, a `system` is a system, an entry
    /// of `providesApis` is an API. Reading every bare name as a component — which this
    /// crate did — rewrites `owner: team-x` into `component:default/team-x`: a well-formed
    /// reference that resolves to nothing, so the entity is parsed, the owner is present,
    /// and only the join across entities is silently lost.
    pub fn parse(raw: &str, implied_kind: &str) -> Result<Self, CatalogError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(CatalogError::BadEntityRef(raw.to_string()));
        }
        let parts: Vec<&str> = trimmed.split('/').collect();
        let (kind, namespace, name) = match parts.as_slice() {
            [name] => (
                implied_kind.to_string(),
                "default".to_string(),
                name.to_string(),
            ),
            [prefix, name] => match prefix.split_once(':') {
                Some((kind, namespace)) => (
                    kind.to_ascii_lowercase(),
                    namespace.to_string(),
                    name.to_string(),
                ),
                None => (
                    implied_kind.to_string(),
                    prefix.to_string(),
                    name.to_string(),
                ),
            },
            _ => {
                return Err(CatalogError::BadEntityRef(format!(
                    "{raw} has more than two `/`-separated segments"
                )));
            }
        };
        validate_segment(&kind, "kind", raw)?;
        validate_segment(&namespace, "namespace", raw)?;
        validate_segment(&name, "name", raw)?;
        Ok(Self {
            kind,
            namespace,
            name,
        })
    }
}

/// Backstage's own name rule: an alphanumeric first character, then alphanumerics and
/// `.`, `_`, `-`, up to 63 characters.
fn validate_segment(segment: &str, what: &str, raw: &str) -> Result<(), CatalogError> {
    let mut characters = segment.chars();
    let Some(first) = characters.next() else {
        return Err(CatalogError::BadEntityRef(format!(
            "{raw} ({what} is empty)"
        )));
    };
    if !first.is_ascii_alphanumeric() {
        return Err(CatalogError::BadEntityRef(format!(
            "{raw} ({what} must start with an ASCII letter or digit)"
        )));
    }
    if segment.len() > 63 {
        return Err(CatalogError::BadEntityRef(format!(
            "{raw} ({what} is longer than 63 characters)"
        )));
    }
    if !segment
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(CatalogError::BadEntityRef(format!(
            "{raw} ({what} contains a character outside [A-Za-z0-9._-])"
        )));
    }
    Ok(())
}

/// The annotation that links an entity to its GitHub repository.
const GITHUB_PROJECT_ANNOTATION: &str = "github.com/project-slug";
/// Annotation keys read by this crate. Others are simply not read, and that is recorded
/// per entity as an unmodelled key.
const MODELLED_ANNOTATIONS: &[&str] = &[GITHUB_PROJECT_ANNOTATION];

/// `system:default/s` and `system/s` are one reference, and a graph keyed on the raw text
/// would hold two.
///
/// The implied kind travels with the call: canonicalising a `system` field as if it could
/// only name a component is how a relationship is lost while still looking canonical.
fn canonicalize(raw: String, implied_kind: &str) -> String {
    EntityRef::parse(&raw, implied_kind)
        .map(|reference| reference.canonical())
        .unwrap_or(raw)
}

fn as_str(value: &Value) -> Option<String> {
    value.as_str().map(|text| text.to_string())
}

fn as_string_list(value: &Value) -> Option<Vec<String>> {
    value
        .as_sequence()
        .map(|items| items.iter().filter_map(as_str).collect())
}

/// Parses every document in one `catalog-info.yaml`.
///
/// Returns the entities, the kinds that are not modelled (counted by name) and the
/// documents that were refused (counted by reason). Nothing is silently discarded: a
/// document this crate cannot read is a document whose entities are absent from the
/// ownership graph, and a reader has to be able to see that.
pub fn parse_catalog(source_path: &str, content: &str) -> Result<ParsedCatalogFile, CatalogError> {
    let mut entities = Vec::new();
    let mut unmodelled_kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut refusals: BTreeMap<String, usize> = BTreeMap::new();

    for document in serde_yaml::Deserializer::from_str(content) {
        let value = match Value::deserialize(document) {
            Ok(value) => value,
            Err(error) => {
                *refusals.entry(format!("not YAML: {error}")).or_insert(0) += 1;
                continue;
            }
        };
        if value.is_null() {
            // An empty document between `---` separators is legal YAML and carries no
            // entity. Not a refusal: there is nothing to refuse.
            continue;
        }
        if !value.is_mapping() {
            *refusals
                .entry("a document that is not a mapping".to_string())
                .or_insert(0) += 1;
            continue;
        }

        let api_version = value.get("apiVersion").and_then(as_str);
        match api_version.as_deref() {
            Some("backstage.io/v1alpha1") | Some("backstage.io/v1beta1") => {}
            Some(other) => {
                *refusals
                    .entry(format!("apiVersion {other} is not modelled"))
                    .or_insert(0) += 1;
                continue;
            }
            None => {
                *refusals.entry("no apiVersion".to_string()).or_insert(0) += 1;
                continue;
            }
        }

        let Some(kind_name) = value.get("kind").and_then(as_str) else {
            *refusals.entry("no kind".to_string()).or_insert(0) += 1;
            continue;
        };
        let Some(kind) = EntityKind::parse(&kind_name) else {
            *unmodelled_kinds.entry(kind_name).or_insert(0) += 1;
            continue;
        };

        let metadata = value.get("metadata");
        let Some(name) = metadata.and_then(|m| m.get("name")).and_then(as_str) else {
            *refusals
                .entry(format!("a {} with no metadata.name", kind.as_str()))
                .or_insert(0) += 1;
            continue;
        };
        let namespace = metadata
            .and_then(|m| m.get("namespace"))
            .and_then(as_str)
            .unwrap_or_else(|| "default".to_string());
        let entity_ref = EntityRef {
            kind: kind.as_str().to_string(),
            namespace,
            name: name.clone(),
        };
        validate_segment(&entity_ref.namespace, "namespace", &name)?;
        validate_segment(&entity_ref.name, "name", &name)?;

        let spec = value.get("spec");
        let mut unmodelled: BTreeSet<String> = BTreeSet::new();
        if let Some(spec) = spec.and_then(Value::as_mapping) {
            let modelled = kind.modelled_spec_keys();
            for key in spec.keys().filter_map(|key| key.as_str()) {
                if !modelled.contains(&key) {
                    unmodelled.insert(key.to_string());
                }
            }
        }
        let annotations = metadata.and_then(|m| m.get("annotations"));
        if let Some(annotations) = annotations.and_then(Value::as_mapping) {
            for key in annotations.keys().filter_map(|key| key.as_str()) {
                if !MODELLED_ANNOTATIONS.contains(&key) {
                    unmodelled.insert(format!("metadata.annotations[{key}]"));
                }
            }
        }

        let spec_field = |field: &str| spec.and_then(|s| s.get(field)).and_then(as_str);
        // Fields are read only for the kinds that define them. Reading `spec.system` off a
        // `Domain` would populate a field the format does not define and simultaneously
        // report the key as unread — the reader claiming a fact and disclaiming it at once.
        let declares_system = matches!(
            kind,
            EntityKind::Component | EntityKind::Api | EntityKind::Resource
        );
        let declares_relationships = matches!(kind, EntityKind::Component | EntityKind::Resource);
        let relationship = |field: &str, implied_kind: &str| -> Result<Vec<String>, CatalogError> {
            match spec.and_then(|s| s.get(field)) {
                None => Ok(Vec::new()),
                Some(value) => {
                    let Some(items) = as_string_list(value) else {
                        return Err(CatalogError::Malformed {
                            path: source_path.to_string(),
                            format: "backstage entity",
                            message: format!("`spec.{field}` is not a list of references"),
                        });
                    };
                    items
                        .iter()
                        .map(|item| EntityRef::parse(item, implied_kind).map(|r| r.canonical()))
                        .collect()
                }
            }
        };

        let (owner, owner_kind, owner_is_team) = match spec_field("owner") {
            Some(raw) => {
                // Backstage's own default for an owner: a bare name is a group.
                let owner = EntityRef::parse(&raw, "group")?;
                let is_team = owner.kind == "group";
                (Some(owner.canonical()), Some(owner.kind.clone()), is_team)
            }
            None => (None, None, false),
        };

        let mut coverage = autopsy_domain::CoverageState::Verified;
        if !unmodelled.is_empty() {
            // A key this crate did not read could carry anything, including the ownership
            // it is looking for.
            coverage = autopsy_domain::CoverageState::Partial;
        }
        if kind == EntityKind::Component && owner.is_none() {
            coverage = autopsy_domain::CoverageState::Partial;
        }
        if kind == EntityKind::Api && spec_field("definition").is_some() {
            // The contract payload itself: Pillar 3 reads it, this crate does not, and the
            // presence of an unread contract is a real limitation on what the catalog can
            // claim about the API.
            coverage = autopsy_domain::CoverageState::Partial;
            unmodelled
                .insert("spec.definition (read by the contract engines, not here)".to_string());
        }

        entities.push(CatalogEntity {
            kind,
            name: entity_ref.name.clone(),
            namespace: entity_ref.namespace.clone(),
            reference: entity_ref.canonical(),
            title: metadata.and_then(|m| m.get("title")).and_then(as_str),
            description: metadata.and_then(|m| m.get("description")).and_then(as_str),
            owner,
            owner_kind,
            owner_is_team,
            // An unparsable reference in `system`/`domain` is kept verbatim: it is
            // reported as dangling by the ownership index, and refusing the whole document
            // over a relationship field would lose the ownership it *does* state.
            system: declares_system
                .then(|| spec_field("system"))
                .flatten()
                .map(|raw| canonicalize(raw, "system")),
            domain: (kind == EntityKind::System)
                .then(|| spec_field("domain"))
                .flatten()
                .map(|raw| canonicalize(raw, "domain")),
            subcomponent_of: (kind == EntityKind::Component)
                .then(|| spec_field("subcomponentOf"))
                .flatten()
                .map(|raw| canonicalize(raw, "component")),
            lifecycle: spec_field("lifecycle"),
            component_type: spec_field("type"),
            depends_on: if declares_relationships {
                relationship("dependsOn", "component")?
            } else {
                Vec::new()
            },
            provides_apis: if kind == EntityKind::Component {
                relationship("providesApis", "api")?
            } else {
                Vec::new()
            },
            consumes_apis: if kind == EntityKind::Component {
                relationship("consumesApis", "api")?
            } else {
                Vec::new()
            },
            github_project: annotations
                .and_then(|a| a.get(GITHUB_PROJECT_ANNOTATION))
                .and_then(as_str),
            source_path: source_path.to_string(),
            unmodelled_spec_keys: unmodelled.into_iter().collect(),
            coverage,
        });
    }

    entities.sort_by(|left, right| left.reference.cmp(&right.reference));
    Ok(ParsedCatalogFile {
        entities,
        unmodelled_kinds,
        refusals,
    })
}

/// One file's worth of parsed entities and everything about it that was not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCatalogFile {
    pub entities: Vec<CatalogEntity>,
    pub unmodelled_kinds: BTreeMap<String, usize>,
    /// Refusals keyed by reason, so a scan reports "three documents with no kind" rather
    /// than three anonymous omissions.
    pub refusals: BTreeMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMPONENT: &str = r#"
apiVersion: backstage.io/v1alpha1
kind: Component
metadata:
  name: checkout-api
  description: Takes payments
  annotations:
    github.com/project-slug: acme/checkout-api
spec:
  type: service
  lifecycle: production
  owner: group:default/team-checkout
  system: system:default/payments
  providesApis:
    - api:default/checkout
"#;

    #[test]
    fn a_component_produces_its_reference_owner_and_relationships() {
        let parsed = parse_catalog("catalog-info.yaml", COMPONENT).expect("parses");
        assert_eq!(parsed.entities.len(), 1);
        let entity = &parsed.entities[0];
        assert_eq!(entity.reference, "component:default/checkout-api");
        assert_eq!(entity.owner.as_deref(), Some("group:default/team-checkout"));
        assert!(entity.owner_is_team);
        assert_eq!(
            entity.system.as_deref(),
            Some("system:default/payments"),
            "a system reference is canonicalised, so `payments` and `default/payments` are one"
        );
        assert_eq!(entity.provides_apis, ["api:default/checkout"]);
        assert_eq!(entity.github_project.as_deref(), Some("acme/checkout-api"));
        assert_eq!(entity.component_type.as_deref(), Some("service"));
        assert_eq!(
            entity.coverage,
            autopsy_domain::CoverageState::Verified,
            "every key this entity declares is read: {:?}",
            entity.unmodelled_spec_keys
        );
        assert!(
            entity.unmodelled_spec_keys.is_empty(),
            "the GitHub annotation **is** read, so it must not also be reported as unread: {:?}",
            entity.unmodelled_spec_keys
        );
    }

    #[test]
    fn an_unknown_api_version_is_refused_rather_than_guessed_at() {
        let yaml = COMPONENT.replace("backstage.io/v1alpha1", "backstage.io/v9");
        let parsed = parse_catalog("catalog-info.yaml", &yaml).expect("parses as YAML");
        assert!(parsed.entities.is_empty());
        assert_eq!(
            parsed.refusals.keys().next().map(String::as_str),
            Some("apiVersion backstage.io/v9 is not modelled")
        );
    }

    #[test]
    fn an_entity_kind_outside_the_five_is_counted_not_dropped() {
        let yaml =
            "apiVersion: backstage.io/v1alpha1\nkind: Group\nmetadata:\n  name: team-checkout\n";
        let parsed = parse_catalog("catalog-info.yaml", yaml).expect("parses");
        assert!(parsed.entities.is_empty());
        assert_eq!(parsed.unmodelled_kinds.get("Group"), Some(&1));
    }

    #[test]
    fn several_documents_in_one_file_are_all_read() {
        let yaml = format!(
            "{COMPONENT}\n---\n{}",
            COMPONENT.replace("checkout-api", "refunds-api")
        );
        let parsed = parse_catalog("catalog-info.yaml", &yaml).expect("parses");
        assert_eq!(parsed.entities.len(), 2);
        assert_eq!(parsed.entities[0].name, "checkout-api");
        assert_eq!(parsed.entities[1].name, "refunds-api");
    }

    #[test]
    fn a_component_without_an_owner_degrades_but_is_not_an_error() {
        let yaml = COMPONENT
            .replace("  owner: group:default/team-checkout\n", "")
            .replace("github.com/project-slug: acme/checkout-api", "x: y");
        assert!(
            !yaml.contains("owner:"),
            "the fixture edit must have applied"
        );
        let parsed = parse_catalog("catalog-info.yaml", &yaml).expect("parses");
        assert_eq!(parsed.entities[0].owner, None);
        assert!(!parsed.entities[0].owner_is_team);
        assert_eq!(
            parsed.entities[0].coverage,
            autopsy_domain::CoverageState::Partial
        );
    }

    #[test]
    fn a_bad_reference_is_an_error_rather_than_a_kept_string() {
        let yaml = COMPONENT.replace("group:default/team-checkout", "group:default/team checkout");
        assert!(parse_catalog("catalog-info.yaml", &yaml).is_err());
    }

    #[test]
    fn the_reference_grammar_is_backstages_own() {
        assert_eq!(
            EntityRef::parse("checkout", "component")
                .expect("parses")
                .canonical(),
            "component:default/checkout"
        );
        assert_eq!(
            EntityRef::parse("group:default/team-x", "component")
                .expect("parses")
                .canonical(),
            "group:default/team-x"
        );
        let two_segments = EntityRef::parse("component/checkout", "component").expect("parses");
        assert_eq!(two_segments.namespace, "component");
        assert_eq!(two_segments.kind, "component");
        assert_eq!(two_segments.canonical(), "component:component/checkout");
        assert_eq!(
            EntityRef::parse("default/checkout", "component")
                .expect("parses")
                .canonical(),
            "component:default/checkout"
        );
        // Refusals, not guesses.
        assert!(
            EntityRef::parse("a/b/c", "component").is_err(),
            "three segments is not a reference"
        );
        assert!(
            EntityRef::parse("group:team-x", "component").is_err(),
            "a kind with no namespace is not a reference"
        );
    }

    #[test]
    fn a_name_that_backstage_would_reject_is_rejected_here_too() {
        for bad in ["-leading-dash", "has space", "", "a", &"x".repeat(64)] {
            let reference = EntityRef::parse(&format!("group:default/{bad}"), "component");
            if bad == "a" {
                // A single character is a legal Backstage name; the loop asserts on the
                // others, and this arm keeps the fixture honest rather than silently
                // dropping the case.
                assert!(reference.is_ok());
            } else {
                assert!(reference.is_err(), "{bad:?} should be refused");
            }
        }
    }

    #[test]
    fn the_implied_kind_of_a_bare_reference_comes_from_its_field() {
        // The defect this test exists for: every bare name was read as a *component*, so
        // `owner: team-x` became `component:default/team-x`, which resolves to nothing. The
        // entity still parsed and still had an owner, so nothing failed — the only symptom
        // was a join across entities that silently did not happen.
        let yaml = r#"
apiVersion: backstage.io/v1alpha1
kind: Component
metadata:
  name: checkout
spec:
  type: service
  lifecycle: production
  owner: team-x
  system: checkout-system
  providesApis:
    - checkout-api
  consumesApis:
    - payments-api
  dependsOn:
    - cart
"#;
        let parsed = parse_catalog("catalog-info.yaml", yaml).expect("parses");
        let entity = &parsed.entities[0];
        assert_eq!(entity.owner.as_deref(), Some("group:default/team-x"));
        assert!(entity.owner_is_team, "a group owner is a team");
        assert_eq!(
            entity.system.as_deref(),
            Some("system:default/checkout-system")
        );
        assert_eq!(entity.provides_apis, vec!["api:default/checkout-api"]);
        assert_eq!(entity.consumes_apis, vec!["api:default/payments-api"]);
        assert_eq!(
            entity.depends_on,
            vec!["component:default/cart"],
            "`dependsOn` is the one relationship field whose implied kind is component"
        );
    }

    #[test]
    fn an_explicit_kind_in_a_bare_owners_reference_still_wins() {
        let yaml = COMPONENT.replace("group:default/team-checkout", "user:default/ravi");
        let parsed = parse_catalog("catalog-info.yaml", &yaml).expect("parses");
        assert_eq!(
            parsed.entities[0].owner.as_deref(),
            Some("user:default/ravi")
        );
        assert!(
            !parsed.entities[0].owner_is_team,
            "a person owner has no team to roll up to"
        );
    }

    #[test]
    fn an_api_definition_is_recorded_as_unread() {
        let yaml = "apiVersion: backstage.io/v1alpha1\nkind: API\nmetadata:\n  name: checkout\nspec:\n  type: openapi\n  owner: group:default/team-checkout\n  definition: |\n    openapi: 3.0.0\n";
        let parsed = parse_catalog("catalog-info.yaml", yaml).expect("parses");
        let entity = &parsed.entities[0];
        assert!(
            entity
                .unmodelled_spec_keys
                .iter()
                .any(|key| key.starts_with("spec.definition") || key == "definition"),
            "{:?}",
            entity.unmodelled_spec_keys
        );
        assert_eq!(entity.coverage, autopsy_domain::CoverageState::Partial);
    }
}
