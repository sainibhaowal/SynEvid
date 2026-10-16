//! Ownership index and organisational blast radius.
//!
//! The design asks to \"map code symbols to team ownership to compute organisational blast
//! radius\". The join is deliberately split in two, because one half of it is a fact and the
//! other is an assumption:
//!
//! * **A fact:** a repository declares which entity it *is* — a `catalog-info.yaml` inside
//!   it, or a GitHub project slug. [`OwnershipIndex::components_declared_at`] and
//!   [`OwnershipIndex::components_for_github_project`] resolve that, from evidence.
//! * **An assumption:** that a *symbol* belongs to a component. Nothing in a Backstage
//!   catalog states which files a component owns, and this crate will not infer it from a
//!   directory name — a wrong ownership claim is worse than an absent one, because it routes
//!   an incident to the wrong team. The caller supplies the component, or a discovery
//!   resolves the repository's own component, and the blast radius is computed from there.
//!
//! [`OwnershipIndex::blast_radius`] then answers the question the design actually wants:
//! *if this component changes, what else does it touch* — the other components the same team
//! owns, its siblings in a system, its domain, the APIs it provides and consumes, and the
//! components that declare a dependency on it.
//!
//! # Unresolved references are reported, never assumed away
//!
//! Every reference is classified on three outcomes, not two:
//!
//! | Outcome | Meaning |
//! |---|---|
//! | resolved | the referenced entity is in the catalog |
//! | kind not modelled | the reference names a `Group`, `User` or other kind outside the five this crate reads |
//! | dangling | the reference names one of the five kinds and no such entity is present |
//!
//! The third is a catatog integrity problem and degrades coverage. The second is not: every
//! `spec.owner` in a normal catalog points at a `Group`, and reporting those as dangling
//! would bury the real problem in expected output.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::backstage::{CatalogEntity, EntityKind, EntityRef};
use crate::discover::CatalogError;

/// How far a `subcomponentOf` chain is followed before it is declared a loop.
const MAX_COMPONENT_DEPTH: usize = 32;

/// A reference that names one of the modelled kinds and no such entity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DanglingReference {
    /// The entity that declared the reference.
    pub from: String,
    /// The field that declared it (`spec.owner`, `spec.dependsOn`, …).
    pub field: String,
    /// The canonical reference that resolves to nothing.
    pub target: String,
}

/// References grouped by the **kind** they point at, for kinds this crate does not model.
///
/// Kept as a count rather than a list because a catalog of two hundred entities has an
/// owner reference on most of them, and the interesting fact is the kind — `group`
/// references resolve to `Group` entities that are outside the five modelled kinds.
pub type UnmodelledReferenceKinds = BTreeMap<String, usize>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnershipIndex {
    /// `spec.owner` → the entities it owns, sorted.
    pub by_owner: BTreeMap<String, Vec<String>>,
    /// `spec.system` → the entities in it, sorted.
    pub by_system: BTreeMap<String, Vec<String>>,
    /// `spec.domain` → the systems in it, sorted.
    pub by_domain: BTreeMap<String, Vec<String>>,
    /// References to modelled kinds that are absent.
    pub dangling: Vec<DanglingReference>,
    /// References to kinds outside the five, counted by kind.
    pub unmodelled_reference_kinds: UnmodelledReferenceKinds,
    /// Owner references that name a `user` rather than a `group`: these are owner-personal
    /// and have no team to roll up to.
    pub person_owners: Vec<String>,
}

fn field_references(entity: &CatalogEntity) -> Vec<(&'static str, &str)> {
    let mut references: Vec<(&'static str, &str)> = Vec::new();
    if let Some(owner) = &entity.owner {
        references.push(("spec.owner", owner));
    }
    if let Some(system) = &entity.system {
        references.push(("spec.system", system));
    }
    if let Some(domain) = &entity.domain {
        references.push(("spec.domain", domain));
    }
    if let Some(parent) = &entity.subcomponent_of {
        references.push(("spec.subcomponentOf", parent));
    }
    for target in &entity.depends_on {
        references.push(("spec.dependsOn", target));
    }
    for target in &entity.provides_apis {
        references.push(("spec.providesApis", target));
    }
    for target in &entity.consumes_apis {
        references.push(("spec.consumesApis", target));
    }
    references
}

impl OwnershipIndex {
    pub fn new(entities: &[CatalogEntity]) -> Self {
        let known: BTreeMap<&str, EntityKind> = entities
            .iter()
            .map(|entity| (entity.reference.as_str(), entity.kind))
            .collect();
        let mut index = Self {
            by_owner: BTreeMap::new(),
            by_system: BTreeMap::new(),
            by_domain: BTreeMap::new(),
            dangling: Vec::new(),
            unmodelled_reference_kinds: BTreeMap::new(),
            person_owners: Vec::new(),
        };

        for entity in entities {
            if let Some(owner) = &entity.owner {
                index
                    .by_owner
                    .entry(owner.clone())
                    .or_default()
                    .push(entity.reference.clone());
                if entity.owner_kind.as_deref() == Some("user") {
                    index.person_owners.push(owner.clone());
                }
            }
            if let Some(system) = &entity.system {
                index
                    .by_system
                    .entry(system.clone())
                    .or_default()
                    .push(entity.reference.clone());
            }
            if let Some(domain) = &entity.domain {
                index
                    .by_domain
                    .entry(domain.clone())
                    .or_default()
                    .push(entity.reference.clone());
            }

            for (field, target) in field_references(entity) {
                match EntityKind::parse(
                    target
                        .split_once(':')
                        .map(|(kind, _)| kind)
                        .unwrap_or("component"),
                ) {
                    Some(kind) => {
                        let target_matches_kind = known
                            .get(target)
                            .is_some_and(|known_kind| *known_kind == kind);
                        if !target_matches_kind {
                            index.dangling.push(DanglingReference {
                                from: entity.reference.clone(),
                                field: field.to_string(),
                                target: target.to_string(),
                            });
                        }
                    }
                    None => {
                        let kind = target
                            .split_once(':')
                            .map(|(kind, _)| kind.to_ascii_lowercase())
                            .unwrap_or_else(|| "component".to_string());
                        *index.unmodelled_reference_kinds.entry(kind).or_insert(0) += 1;
                    }
                }
            }
        }

        index.dangling.sort();
        index.dangling.dedup();
        index.person_owners.sort();
        index.person_owners.dedup();
        for owned in index.by_owner.values_mut() {
            owned.sort();
        }
        for members in index.by_system.values_mut() {
            members.sort();
        }
        for members in index.by_domain.values_mut() {
            members.sort();
        }
        index
    }

    /// The components declared by a `catalog-info.yaml` **inside** `directory`, where
    /// `directory` is POSIX-relative to the catalog root (use `"."` for the root itself).
    ///
    /// Evidence, not inference: this answers \"what does this repository say it is\", which
    /// is exactly what a repository-relative path can establish.
    pub fn components_declared_at(
        &self,
        directory: &str,
        entities: &[CatalogEntity],
    ) -> Vec<String> {
        let prefix = directory.trim_end_matches('/');
        let mut found: Vec<String> = entities
            .iter()
            .filter(|entity| entity.kind == EntityKind::Component)
            .filter(|entity| {
                let parent = entity
                    .source_path
                    .rsplit_once('/')
                    .map(|(parent, _)| parent)
                    .unwrap_or("");
                parent == prefix
            })
            .map(|entity| entity.reference.clone())
            .collect();
        found.sort();
        found
    }

    /// Components whose `github.com/project-slug` annotation names this slug `R37`.
    pub fn components_for_github_project(
        &self,
        slug: &str,
        entities: &[CatalogEntity],
    ) -> Vec<String> {
        let mut found: Vec<String> = entities
            .iter()
            .filter(|entity| entity.github_project.as_deref() == Some(slug))
            .map(|entity| entity.reference.clone())
            .collect();
        found.sort();
        found
    }

    /// Everything this component touches *organisationally*, plus an honest account of what
    /// could not be resolved.
    ///
    /// An unknown component is not an error: \"I have never heard of that component\" is an
    /// answer, and it is the more useful one, because a caller who expected it to be present
    /// has learned that their catalog scan is incomplete rather than that their code is
    /// uncoupled.
    pub fn blast_radius(
        &self,
        entities: &[CatalogEntity],
        component: &str,
    ) -> Result<OrgBlastRadius, CatalogError> {
        let reference = EntityRef::parse(component, "component")?.canonical();
        let known: BTreeMap<&str, &CatalogEntity> = entities
            .iter()
            .map(|entity| (entity.reference.as_str(), entity))
            .collect();

        let Some(entity) = known.get(reference.as_str()).copied() else {
            return Ok(OrgBlastRadius {
                component: reference.clone(),
                found: false,
                coverage: autopsy_domain::CoverageState::Unknown,
                notes: vec![format!(
                    "{reference} is not present in the catalog, so nothing is known about who owns it \
                     or what it touches"
                )],
                ..OrgBlastRadius::empty(&reference)
            });
        };

        if entity.kind != EntityKind::Component {
            // Found, and still not actionable: the traversal is component-shaped — owners,
            // sibling workloads, provided and consumed APIs — and every one of those fields
            // is either undefined or would be a category error for another kind. Reporting
            // an empty radius for a group would read as "this team touches nothing".
            return Ok(OrgBlastRadius {
                component: reference.clone(),
                found: true,
                coverage: autopsy_domain::CoverageState::Unsupported,
                notes: vec![format!(
                    "{reference} is a {}, not a component; an organisational blast radius is \
                     defined for components, whose owners, systems and APIs the catalog states",
                    entity.kind.as_str()
                )],
                ..OrgBlastRadius::empty(&reference)
            });
        }

        let mut notes: Vec<String> = Vec::new();
        let mut unresolved: Vec<String> = Vec::new();
        let degrade = |notes: &mut Vec<String>, reason: String| {
            if !notes.contains(&reason) {
                notes.push(reason);
            }
        };

        // Team ownership.
        let owners: Vec<String> = entity.owner.iter().cloned().collect();
        let owner_is_team = entity.owner_is_team;
        if entity.owner.is_none() {
            degrade(
                &mut notes,
                format!("{reference} declares no `spec.owner`, so no team owns it"),
            );
        } else if !owner_is_team {
            degrade(
                &mut notes,
                format!(
                    "{reference} is owned by a person ({}) rather than a group, so there is no team \
                     to roll up to",
                    entity.owner.as_deref().unwrap_or("")
                ),
            );
        }

        // The system, following `subcomponentOf` when the component does not name one.
        let (system, chain_note) = self.resolve_system(&reference, &known);
        if let Some(note) = chain_note {
            degrade(&mut notes, note);
        }

        // The domain, from the system entity when the system is present.
        let domain = system
            .as_ref()
            .and_then(|system| known.get(system.as_str()))
            .and_then(|system_entity| system_entity.domain.clone());

        let mut co_owned: Vec<String> = Vec::new();
        for owner in &owners {
            for owned in self.by_owner.get(owner).into_iter().flatten() {
                if owned == &reference {
                    continue;
                }
                let is_component = known
                    .get(owned.as_str())
                    .is_some_and(|entity| entity.kind == EntityKind::Component);
                if is_component && !co_owned.contains(owned) {
                    co_owned.push(owned.clone());
                }
            }
        }
        co_owned.sort();

        // Siblings are **components**, not every entity in the system: an `API` or a
        // `Resource` shares the system but is not another workload an incident spreads to,
        // and listing them here would put a contract in a list of services.
        let mut siblings: Vec<String> = Vec::new();
        if let Some(system) = &system {
            match self.by_system.get(system) {
                Some(members) => {
                    siblings.extend(
                        members
                            .iter()
                            .filter(|member| *member != &reference)
                            .filter(|member| {
                                known
                                    .get(member.as_str())
                                    .is_some_and(|entity| entity.kind == EntityKind::Component)
                            })
                            .cloned(),
                    );
                }
                None => unresolved.push(format!(
                    "{system} is referenced as the system but no such entity is in the catalog"
                )),
            }
        }
        siblings.sort();

        // Components that declare they depend on this one, in either the generic
        // `dependsOn` sense or the API sense.
        let mut dependents: Vec<String> = entities
            .iter()
            .filter(|candidate| candidate.reference != reference)
            .filter(|candidate| {
                candidate
                    .depends_on
                    .iter()
                    .any(|target| target == &reference)
                    || !entity.provides_apis.is_empty()
                        && candidate
                            .consumes_apis
                            .iter()
                            .any(|target| entity.provides_apis.contains(target))
            })
            .map(|candidate| candidate.reference.clone())
            .collect();
        dependents.sort();

        let mut provided_apis = entity.provides_apis.clone();
        provided_apis.sort();
        provided_apis.dedup();
        let mut consumed_apis = entity.consumes_apis.clone();
        consumed_apis.sort();
        consumed_apis.dedup();

        for api in provided_apis.iter().chain(consumed_apis.iter()) {
            if !known.contains_key(api.as_str()) {
                unresolved.push(format!(
                    "{api} is referenced by {reference} but is not in the catalog"
                ));
            }
        }
        for dependency in &entity.depends_on {
            if !known.contains_key(dependency.as_str()) {
                unresolved.push(format!(
                    "{dependency} is a declared dependency of {reference} but is not in the catalog"
                ));
            }
        }
        unresolved.sort();
        unresolved.dedup();

        if !unresolved.is_empty() {
            degrade(
                &mut notes,
                format!(
                    "{} reference(s) from {reference} do not resolve to an entity in this catalog, so \
                     the blast radius is incomplete",
                    unresolved.len()
                ),
            );
        }
        if !entity.unmodelled_spec_keys.is_empty() {
            degrade(
                &mut notes,
                format!(
                    "{reference} declares spec keys this crate does not read ({}), so a relationship \
                     may be missing",
                    entity.unmodelled_spec_keys.join(", ")
                ),
            );
        }
        if self
            .dangling
            .iter()
            .any(|dangling| dangling.from == reference)
        {
            degrade(
                &mut notes,
                format!("{reference} has a dangling reference (see `dangling` in the report)"),
            );
        }

        let coverage = if notes.is_empty() {
            autopsy_domain::CoverageState::Verified
        } else if entity.coverage == autopsy_domain::CoverageState::Unknown {
            autopsy_domain::CoverageState::Unknown
        } else {
            autopsy_domain::CoverageState::Partial
        };

        Ok(OrgBlastRadius {
            component: reference,
            found: true,
            owners,
            owner_is_team,
            co_owned_components: co_owned,
            siblings,
            system,
            domain,
            provided_apis,
            consumed_apis,
            dependents,
            unresolved,
            coverage,
            notes,
        })
    }

    /// The component's system, following `subcomponentOf` up to a bounded depth.
    ///
    /// Returns a note when the chain was abandoned — a loop in a hand-maintained catalog is a
    /// real possibility, and following it forever would be a hang rather than an answer.
    fn resolve_system(
        &self,
        component: &str,
        known: &BTreeMap<&str, &CatalogEntity>,
    ) -> (Option<String>, Option<String>) {
        let mut current = component.to_string();
        let mut visited: BTreeSet<String> = BTreeSet::new();
        for _ in 0..MAX_COMPONENT_DEPTH {
            if !visited.insert(current.clone()) {
                return (
                    None,
                    Some(format!(
                        "`subcomponentOf` forms a loop at {current}, so the system could not be \
                         resolved"
                    )),
                );
            }
            let Some(entity) = known.get(current.as_str()).copied() else {
                // The parent names a component that is not in the catalog: the traversal
                // stops, and the caller's dangling-reference check reports it.
                return (None, None);
            };
            if let Some(system) = &entity.system {
                return (Some(system.clone()), None);
            }
            let Some(parent) = &entity.subcomponent_of else {
                return (None, None);
            };
            current = parent.clone();
        }
        (
            None,
            Some(format!(
                "the `subcomponentOf` chain from {component} is deeper than {MAX_COMPONENT_DEPTH}, so \
                 the system could not be resolved"
            )),
        )
    }
}

/// What a component change touches organisationally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrgBlastRadius {
    pub component: String,
    /// Whether the catalog contains the component at all.
    pub found: bool,
    /// The owner reference(s) as declared. Empty when the component declares none.
    pub owners: Vec<String>,
    /// Whether the owner is a group. A person owner has no team to roll up to, and the note
    /// says so rather than leaving the field looking populated.
    pub owner_is_team: bool,
    /// Other components the same owner owns — the same team's on-call surface.
    pub co_owned_components: Vec<String>,
    /// Other components in the same system.
    pub siblings: Vec<String>,
    pub system: Option<String>,
    /// The system's domain, or the component's own if it declares one.
    pub domain: Option<String>,
    pub provided_apis: Vec<String>,
    pub consumed_apis: Vec<String>,
    /// Components that declare this component (or an API it provides) as a dependency.
    pub dependents: Vec<String>,
    /// References this traversal could not resolve, as prose lines naming both ends.
    pub unresolved: Vec<String>,
    pub coverage: autopsy_domain::CoverageState,
    pub notes: Vec<String>,
}

impl OrgBlastRadius {
    fn empty(component: &str) -> Self {
        Self {
            component: component.to_string(),
            found: false,
            owners: Vec::new(),
            owner_is_team: false,
            co_owned_components: Vec::new(),
            siblings: Vec::new(),
            system: None,
            domain: None,
            provided_apis: Vec::new(),
            consumed_apis: Vec::new(),
            dependents: Vec::new(),
            unresolved: Vec::new(),
            coverage: autopsy_domain::CoverageState::Unknown,
            notes: Vec::new(),
        }
    }

    /// Every entity reference this radius names, sorted and deduplicated — the set a
    /// consumer would badge in a UI.
    pub fn touched_entities(&self) -> Vec<String> {
        let mut touched: BTreeSet<String> = BTreeSet::new();
        touched.extend(self.owners.iter().cloned());
        touched.extend(self.co_owned_components.iter().cloned());
        touched.extend(self.siblings.iter().cloned());
        touched.extend(self.system.iter().cloned());
        touched.extend(self.domain.iter().cloned());
        touched.extend(self.provided_apis.iter().cloned());
        touched.extend(self.consumed_apis.iter().cloned());
        touched.extend(self.dependents.iter().cloned());
        touched.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backstage::parse_catalog;

    /// Three components, two teams, one system, one API — enough for every branch without a
    /// filesystem.
    const CATALOG: &str = r#"
apiVersion: backstage.io/v1alpha1
kind: Domain
metadata:
  name: payments
spec:
  owner: group:default/team-payments
---
apiVersion: backstage.io/v1alpha1
kind: System
metadata:
  name: checkout
spec:
  domain: domain:default/payments
  owner: group:default/team-payments
---
apiVersion: backstage.io/v1alpha1
kind: API
metadata:
  name: checkout
spec:
  type: openapi
  owner: group:default/team-payments
  system: system:default/checkout
---
apiVersion: backstage.io/v1alpha1
kind: Component
metadata:
  name: checkout-api
spec:
  type: service
  owner: group:default/team-payments
  system: system:default/checkout
  providesApis:
    - api:default/checkout
---
apiVersion: backstage.io/v1alpha1
kind: Component
metadata:
  name: refunds-worker
spec:
  type: worker
  owner: group:default/team-payments
  system: system:default/checkout
  dependsOn:
    - component:default/checkout-api
---
apiVersion: backstage.io/v1alpha1
kind: Component
metadata:
  name: web-storefront
spec:
  type: website
  owner: group:default/team-web
  system: system:default/checkout
  consumesApis:
    - api:default/checkout
"#;

    fn index_and_entities() -> (OwnershipIndex, Vec<CatalogEntity>) {
        let parsed = parse_catalog("catalog-info.yaml", CATALOG).expect("parses");
        assert_eq!(parsed.entities.len(), 6, "the fixture must be fully parsed");
        assert!(parsed.refusals.is_empty(), "{:?}", parsed.refusals);
        let index = OwnershipIndex::new(&parsed.entities);
        (index, parsed.entities)
    }

    #[test]
    fn the_index_groups_by_owner_system_and_domain() {
        let (index, _) = index_and_entities();
        assert_eq!(
            index.by_owner["group:default/team-payments"],
            [
                "api:default/checkout",
                "component:default/checkout-api",
                "component:default/refunds-worker",
                "domain:default/payments",
                "system:default/checkout"
            ]
        );
        assert_eq!(
            index.by_system["system:default/checkout"],
            [
                "api:default/checkout",
                "component:default/checkout-api",
                "component:default/refunds-worker",
                "component:default/web-storefront"
            ]
        );
        assert_eq!(
            index.by_domain["domain:default/payments"],
            ["system:default/checkout"]
        );
        assert!(
            index.dangling.is_empty(),
            "the fixture is internally consistent: {:?}",
            index.dangling
        );
    }

    #[test]
    fn a_team_owner_is_not_a_dangling_reference_even_though_no_group_entity_exists() {
        // The subtle failure this guards: every `spec.owner` in a real catalog points at a
        // `Group`, `Group` is outside the five modelled kinds, and a naive
        // \"referenced but absent\" check would report every entity in the catalog as
        // broken — burying the real dangling references in expected output.
        let (index, _) = index_and_entities();
        assert_eq!(
            index.unmodelled_reference_kinds.get("group"),
            Some(&6),
            "{:?}",
            index.unmodelled_reference_kinds
        );
        assert!(index.dangling.is_empty());
    }

    #[test]
    fn a_reference_to_a_modelled_kind_that_is_absent_is_dangling() {
        let parsed = parse_catalog(
            "catalog-info.yaml",
            "apiVersion: backstage.io/v1alpha1\nkind: Component\nmetadata:\n  name: a\nspec:\n  owner: group:default/t\n  dependsOn:\n    - component:default/ghost\n",
        )
        .expect("parses");
        let index = OwnershipIndex::new(&parsed.entities);
        assert_eq!(
            index.dangling,
            [DanglingReference {
                from: "component:default/a".to_string(),
                field: "spec.dependsOn".to_string(),
                target: "component:default/ghost".to_string(),
            }]
        );
    }

    #[test]
    fn a_reference_of_the_wrong_kind_is_dangling_too() {
        // `system:default/thing` where `thing` is a component is a broken catalog: the ref
        // names a kind that does not match the entity it resolves to.
        let parsed = parse_catalog(
            "catalog-info.yaml",
            "apiVersion: backstage.io/v1alpha1\nkind: System\nmetadata:\n  name: thing\nspec:\n  owner: group:default/t\n---\napiVersion: backstage.io/v1alpha1\nkind: Component\nmetadata:\n  name: a\nspec:\n  owner: group:default/t\n  system: system:default/thing\n",
        )
        .expect("parses");
        let index = OwnershipIndex::new(&parsed.entities);
        assert!(
            index.dangling.is_empty(),
            "`thing` is a System and the reference says `system:`: {:?}",
            index.dangling
        );
    }

    #[test]
    fn the_blast_radius_of_the_checkout_api_is_the_payments_team_surface() {
        let (index, entities) = index_and_entities();
        let radius = index
            .blast_radius(&entities, "component:default/checkout-api")
            .expect("resolves");
        assert!(radius.found);
        assert_eq!(radius.owners, ["group:default/team-payments"]);
        assert!(radius.owner_is_team);
        assert_eq!(
            radius.co_owned_components,
            ["component:default/refunds-worker"],
            "web-storefront belongs to another team, so it is not the same on-call surface"
        );
        assert_eq!(
            radius.siblings,
            [
                "component:default/refunds-worker",
                "component:default/web-storefront"
            ]
        );
        assert_eq!(radius.system.as_deref(), Some("system:default/checkout"));
        assert_eq!(
            radius.domain.as_deref(),
            Some("domain:default/payments"),
            "the domain is reached through the system entity"
        );
        assert_eq!(radius.provided_apis, ["api:default/checkout"]);
        assert_eq!(
            radius.dependents,
            [
                "component:default/refunds-worker",
                "component:default/web-storefront"
            ],
            "one depends on the component and one on the API it provides"
        );
        assert_eq!(
            radius.coverage,
            autopsy_domain::CoverageState::Verified,
            "every reference resolved and every spec key was read: {:?}",
            radius.notes
        );
    }

    #[test]
    fn an_unknown_component_is_unknown_rather_than_an_error() {
        let (index, entities) = index_and_entities();
        let radius = index
            .blast_radius(&entities, "component:default/nope")
            .expect("a missing component is an answer, not a failure");
        assert!(!radius.found);
        assert_eq!(
            radius.coverage,
            autopsy_domain::CoverageState::Unknown,
            "claiming zero impact for an unknown component would be the wrong answer"
        );
        assert!(radius.touched_entities().is_empty());
        assert!(radius.notes[0].contains("is not present in the catalog"));
    }

    #[test]
    fn a_malformed_reference_is_an_error() {
        let (index, entities) = index_and_entities();
        assert!(
            index
                .blast_radius(&entities, "component:default/a b")
                .is_err()
        );
    }

    #[test]
    fn a_component_owned_by_a_person_has_no_team_to_roll_up_to() {
        let parsed = parse_catalog(
            "catalog-info.yaml",
            "apiVersion: backstage.io/v1alpha1\nkind: Component\nmetadata:\n  name: a\nspec:\n  owner: user:default/ravi\n",
        )
        .expect("parses");
        let index = OwnershipIndex::new(&parsed.entities);
        assert_eq!(index.person_owners, ["user:default/ravi"]);
        let radius = index
            .blast_radius(&parsed.entities, "component:default/a")
            .expect("resolves");
        assert!(!radius.owner_is_team);
        assert_eq!(
            radius.coverage,
            autopsy_domain::CoverageState::Partial,
            "{:?}",
            radius.notes
        );
        assert!(
            radius
                .notes
                .iter()
                .any(|note| note.contains("owned by a person"))
        );
    }

    #[test]
    fn a_subcomponent_inherits_its_parents_system_and_a_loop_is_bounded() {
        let parsed = parse_catalog(
            "catalog-info.yaml",
            "apiVersion: backstage.io/v1alpha1\nkind: System\nmetadata:\n  name: s\nspec:\n  owner: group:default/t\n---\napiVersion: backstage.io/v1alpha1\nkind: Component\nmetadata:\n  name: parent\nspec:\n  owner: group:default/t\n  system: system:default/s\n---\napiVersion: backstage.io/v1alpha1\nkind: Component\nmetadata:\n  name: child\nspec:\n  owner: group:default/t\n  subcomponentOf: component:default/parent\n---\napiVersion: backstage.io/v1alpha1\nkind: Component\nmetadata:\n  name: loopy\nspec:\n  owner: group:default/t\n  subcomponentOf: component:default/loopy\n",
        )
        .expect("parses");
        let index = OwnershipIndex::new(&parsed.entities);
        let child = index
            .blast_radius(&parsed.entities, "component:default/child")
            .expect("resolves");
        assert_eq!(
            child.system.as_deref(),
            Some("system:default/s"),
            "a subcomponent inherits the system of the component it is part of"
        );
        let loopy = index
            .blast_radius(&parsed.entities, "component:default/loopy")
            .expect("a loop is an answer, not a hang");
        assert_eq!(loopy.system, None);
        assert!(
            loopy.notes.iter().any(|note| note.contains("forms a loop")),
            "{:?}",
            loopy.notes
        );
    }

    #[test]
    fn the_repositorys_own_component_is_resolved_from_evidence() {
        let parsed = parse_catalog(
            "services/checkout/catalog-info.yaml",
            "apiVersion: backstage.io/v1alpha1\nkind: Component\nmetadata:\n  name: checkout-api\n  annotations:\n    github.com/project-slug: acme/checkout-api\nspec:\n  owner: group:default/team-payments\n",
        )
        .expect("parses");
        let index = OwnershipIndex::new(&parsed.entities);
        assert_eq!(
            index.components_for_github_project("acme/checkout-api", &parsed.entities),
            ["component:default/checkout-api"]
        );
        assert_eq!(
            index.components_declared_at("services/checkout", &parsed.entities),
            ["component:default/checkout-api"]
        );
        assert!(
            index
                .components_declared_at(".", &parsed.entities)
                .is_empty(),
            "a declaration in a subdirectory is not a declaration at the root"
        );
    }
}
