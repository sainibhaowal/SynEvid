//! `autopsy-catalog` — Backstage and dependency-manifest ingestion (Steps 43, `R28``R29``R37`).
//!
//! ```text
//!   catalog-info.yaml ─┐
//!                      ├─► discover (bounded, symlink-safe) ─► entities ─► ownership index
//!   package.json      ─┤                                          │            │
//!   requirements.txt  ─┤                                    manifests     blast radius
//!   go.mod            ─┤                                          │            │
//!   Cargo.toml        ─┘                                          ▼            ▼
//!                                                        CatalogReport (one digest)
//! ```
//!
//! # What this crate is not
//!
//! The master plan forbids becoming a generic software catalog: *\"do not replicate
//! Backstage or ServiceNow; ingest their IDs to enrich semantic evidence.\"* So this crate
//! **reads** entity documents and dependency manifests and produces a value that other
//! engines can join against. It does not store a catalog, does not own a lifecycle, does not
//! resolve dependency versions, and does not invent an ownership claim that no document
//! states.
//!
//! # The join to code is where the design says it is, and no further
//!
//! `spec.owner` gives a team per **entity**, not per symbol. Nothing in a catalog says which
//! files a component owns. This crate therefore resolves *entities* and leaves the
//! symbol-level join to a caller that has a symbol table — the same boundary Step 41 drew
//! for `DeclaredSurface`. See [`ownership`] for the two halves of the join that are kept
//! apart.
//!
//! # Determinism
//!
//! Every collection in [`CatalogReport`] is a `BTreeMap`, a `BTreeSet` or a sorted `Vec`,
//! paths are POSIX-relative, and the digest covers the report with `notes` and `digest`
//! removed (`NFR-001`). The catalog **root** is not part of the report at all: an absolute
//! local path would make one repository hash differently on two machines, and it is a fact
//! the caller already has.
//!
//! # Honest coverage (`FR-014`, `FR-030`)
//!
//! [`CatalogReport::coverage`] is `Verified` only when every file in the tree was examined
//! and every document in it was read in full, and it degrades for a refusal, an unread
//! spec key, a truncated scan, or an unmodelled entity kind — each with a note saying which.
//! An empty result is `Unknown`: \"nothing was found\" and \"nothing is there\" are different
//! statements, and only the second may be reported as zero.

#![forbid(unsafe_code)]

pub mod backstage;
pub mod discover;
pub mod manifests;
pub mod ownership;

pub use backstage::{CatalogEntity, EntityKind, EntityRef, parse_catalog};
pub use discover::{
    CatalogError, DiscoveredFile, DiscoveryOptions, DiscoveryReport, MANIFEST_NAMES,
    PRUNED_DIRECTORIES, discover,
};
pub use manifests::{
    DependencyScope, DependencySource, Ecosystem, ManifestDependency, ManifestFile, parse_manifest,
};
pub use ownership::{DanglingReference, OrgBlastRadius, OwnershipIndex};

use autopsy_domain::CoverageState;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Two documents declaring the same entity reference.
///
/// Not fatal, and not silent either. An entity is keyed by its reference, so a second
/// document with the same reference either repeats the first — a harmless copy — or
/// contradicts it, and the two are indistinguishable without reading both. Keeping one and
/// saying which was kept is the only honest resolution: a last-one-wins map would make the
/// answer depend on directory iteration order, and `NFR-001` does not permit an answer that
/// depends on anything but the content.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DuplicateEntity {
    pub reference: String,
    /// The document whose declaration was used.
    pub kept_source: String,
    /// The document whose declaration was ignored.
    pub ignored_source: String,
}

/// The canonical report of one catalog scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogReport {
    /// Files examined, in path order.
    pub files_scanned: usize,
    /// Every bound the scan hit and every entry it passed over.
    pub discovery: DiscoveryReport,
    /// Backstage entities, sorted by canonical reference.
    pub entities: Vec<CatalogEntity>,
    /// Entity kinds present and not modelled (`Group`, `User`, …), counted by kind.
    pub unmodelled_kinds: BTreeMap<String, usize>,
    /// Documents refused, by reason.
    pub refusals: BTreeMap<String, usize>,
    /// Two documents declaring the same entity reference. See [`DuplicateEntity`].
    pub duplicate_entities: Vec<DuplicateEntity>,
    /// One entry per dependency manifest read.
    pub manifests: Vec<ManifestFile>,
    /// Every dependency declaration, flattened, sorted and deduplicated by
    /// `(ecosystem, package, requirement, scope, manifest_path)`.
    pub dependencies: Vec<ManifestDependency>,
    /// The ownership graph, derived from the entities.
    pub ownership: OwnershipIndex,
    /// How many manifests of each ecosystem were read.
    pub ecosystems_read: BTreeMap<String, usize>,
    /// The file names the scan looked for. Present so a reader can see what was out of
    /// scope — `pyproject.toml` is absent from this list, and that is the reason it is
    /// absent from the result.
    pub manifest_names_looked_for: Vec<String>,
    pub coverage: CoverageState,
    pub notes: Vec<String>,
    /// BLAKE3 over the report with `notes` and `digest` removed.
    pub digest: String,
}

impl CatalogReport {
    /// The digest input: this report as canonical JSON, minus the fields that are prose or
    /// derived from prose.
    fn digest_input(&self) -> String {
        let mut value = serde_json::to_value(self).expect("the report serializes");
        if let Some(object) = value.as_object_mut() {
            object.insert("notes".to_string(), serde_json::Value::Array(Vec::new()));
            object.insert(
                "digest".to_string(),
                serde_json::Value::String(String::new()),
            );
        }
        // `serde_json::Map` is a `BTreeMap` by default in this workspace, so this string is
        // key-sorted and independent of field declaration order.
        serde_json::to_string(&value).expect("canonical JSON")
    }

    /// Recomputes the digest. Called by [`scan`]; exposed so a caller that mutates a report
    /// can see the digest change.
    pub fn recompute_digest(&mut self) {
        self.digest = blake3::hash(self.digest_input().as_bytes())
            .to_hex()
            .to_string();
    }

    /// The entities of one kind, sorted by reference.
    pub fn entities_of_kind(&self, kind: EntityKind) -> Vec<&CatalogEntity> {
        self.entities
            .iter()
            .filter(|entity| entity.kind == kind)
            .collect()
    }

    /// Every distinct team (group) that owns something in this catalog.
    pub fn teams(&self) -> Vec<&str> {
        self.ownership
            .by_owner
            .iter()
            .filter(|(owner, _)| owner.starts_with("group:"))
            .map(|(owner, _)| owner.as_str())
            .collect()
    }

    /// The blast radius for one component, delegated to the ownership index.
    pub fn blast_radius(&self, component: &str) -> Result<OrgBlastRadius, CatalogError> {
        self.ownership.blast_radius(&self.entities, component)
    }
}

/// Reads a repository tree and returns the catalog report.
///
/// A missing root is an **error**; an empty one is `Unknown`. The distinction matters
/// because an empty catalog and an absent catalog are indistinguishable downstream once a
/// zero is on the page.
pub fn scan(root: &Path, options: &DiscoveryOptions) -> Result<CatalogReport, CatalogError> {
    let (files, discovery) = discover::discover(root, options)?;

    let mut entities: Vec<CatalogEntity> = Vec::new();
    let mut unmodelled_kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut refusals: BTreeMap<String, usize> = BTreeMap::new();
    let mut manifests: Vec<ManifestFile> = Vec::new();

    for file in &files {
        let name = file.path.rsplit('/').next().unwrap_or(&file.path);
        if name == "catalog-info.yaml" || name == "catalog-info.yml" {
            let parsed = parse_catalog(&file.path, &file.content)?;
            entities.extend(parsed.entities);
            for (kind, count) in parsed.unmodelled_kinds {
                *unmodelled_kinds.entry(kind).or_insert(0) += count;
            }
            for (reason, count) in parsed.refusals {
                *refusals.entry(reason).or_insert(0) += count;
            }
            continue;
        }
        // A malformed manifest is recorded as unsupported for **that file** rather than
        // aborting the scan: one broken `package.json` in a monorepo must not remove the
        // ownership data of every other service.
        match parse_manifest(&file.path, &file.content) {
            Ok(manifest) => manifests.push(manifest),
            Err(error) => {
                *refusals
                    .entry(format!("unparsable manifest: {}", reason_of(&error)))
                    .or_insert(0) += 1;
            }
        }
    }

    // Sorted by reference, then by the file it came from, so which declaration is *kept* is
    // a function of content rather than of directory iteration order.
    entities.sort_by(|left, right| {
        left.reference
            .cmp(&right.reference)
            .then_with(|| left.source_path.cmp(&right.source_path))
    });
    let mut duplicate_entities: Vec<DuplicateEntity> = Vec::new();
    let mut unique: Vec<CatalogEntity> = Vec::with_capacity(entities.len());
    for entity in entities.drain(..) {
        if let Some(kept) = unique.last()
            && kept.reference == entity.reference
        {
            duplicate_entities.push(DuplicateEntity {
                reference: entity.reference.clone(),
                kept_source: kept.source_path.clone(),
                ignored_source: entity.source_path.clone(),
            });
            continue;
        }
        unique.push(entity);
    }
    let entities = unique;
    manifests.sort_by(|left, right| left.path.cmp(&right.path));

    let mut dependencies: Vec<ManifestDependency> = manifests
        .iter()
        .flat_map(|manifest| manifest.dependencies.iter().cloned())
        .collect();
    dependencies.sort();
    dependencies.dedup();

    let mut ecosystems_read: BTreeMap<String, usize> = BTreeMap::new();
    for manifest in &manifests {
        *ecosystems_read
            .entry(manifest.ecosystem.as_str().to_string())
            .or_insert(0) += 1;
    }

    let ownership = OwnershipIndex::new(&entities);

    let mut notes: Vec<String> = Vec::new();
    let mut coverage = CoverageState::Verified;

    if !discovery.is_complete() {
        coverage = CoverageState::Partial;
        if discovery.skipped_symlinks > 0 {
            // Recorded, because a symlink is exactly how a manifest outside the tree would
            // be reached by a path inside it (`INV-SANDBOX-002`).
            notes.push(format!(
                "{} symbolic link(s) were not followed, so a manifest reachable only through one \
                 was not read",
                discovery.skipped_symlinks
            ));
        }
        if !discovery.skipped_oversize_files.is_empty() {
            notes.push(format!(
                "{} file(s) exceeded the size bound and were not read: {}",
                discovery.skipped_oversize_files.len(),
                discovery.skipped_oversize_files.join(", ")
            ));
        }
        if !discovery.skipped_beyond_file_limit.is_empty() {
            notes.push(format!(
                "the file bound was reached; {} further file(s) were not read",
                discovery.skipped_beyond_file_limit.len()
            ));
        }
        if !discovery.depth_truncated.is_empty() {
            notes.push(format!(
                "the depth bound was reached in {} director(ies), whose subtrees were not walked",
                discovery.depth_truncated.len()
            ));
        }
    }

    if !refusals.is_empty() {
        coverage = CoverageState::Partial;
        for (reason, count) in &refusals {
            notes.push(format!("{count} document(s) refused: {reason}"));
        }
    }
    if !unmodelled_kinds.is_empty() {
        coverage = CoverageState::Partial;
        let kinds: Vec<String> = unmodelled_kinds
            .iter()
            .map(|(kind, count)| format!("{kind} ({count})"))
            .collect();
        notes.push(format!(
            "entity kinds outside the five this crate models are present and were not read: {}. A \
             `Group` may declare a parent team, so ownership rolls up only as far as the reference.",
            kinds.join(", ")
        ));
    }
    if entities
        .iter()
        .any(|entity| entity.coverage == CoverageState::Partial)
    {
        coverage = CoverageState::Partial;
        let count = entities
            .iter()
            .filter(|entity| entity.coverage == CoverageState::Partial)
            .count();
        notes.push(format!(
            "{count} entity(ies) have a spec key this crate does not read or declare no owner, so \
             their relationships may be incomplete"
        ));
    }
    if manifests
        .iter()
        .any(|manifest| manifest.coverage == CoverageState::Partial)
    {
        coverage = CoverageState::Partial;
        let affected: Vec<&str> = manifests
            .iter()
            .filter(|manifest| manifest.coverage == CoverageState::Partial)
            .map(|manifest| manifest.path.as_str())
            .collect();
        notes.push(format!(
            "{} manifest(s) declare conditional dependencies or unread sections: {}",
            affected.len(),
            affected.join(", ")
        ));
    }
    if !dependencies.is_empty() {
        notes.push(
            "requirements are reported as written. No lockfile was read, so transitive \
             dependencies are not in this report and a requirement is not the version that is \
             installed."
                .to_string(),
        );
    }
    if !duplicate_entities.is_empty() {
        coverage = CoverageState::Partial;
        notes.push(format!(
            "{} entity reference(s) are declared by more than one document; the first in path order \
             was kept and the rest are listed in `duplicate_entities`",
            duplicate_entities.len()
        ));
    }
    if !ownership.dangling.is_empty() {
        coverage = CoverageState::Partial;
        notes.push(format!(
            "{} reference(s) name a modelled kind that is not in this catalog (see `dangling`)",
            ownership.dangling.len()
        ));
    }

    if files.is_empty() {
        // Nothing to read is not the same as reading nothing.
        coverage = CoverageState::Unknown;
        notes.push(
            "no catalog or dependency manifest was found under this root, so nothing is claimed \
             about ownership or dependencies"
                .to_string(),
        );
    }

    notes.sort();
    notes.dedup();

    let mut report = CatalogReport {
        files_scanned: files.len(),
        discovery,
        entities,
        unmodelled_kinds,
        refusals,
        duplicate_entities,
        manifests,
        dependencies,
        ownership,
        ecosystems_read,
        manifest_names_looked_for: MANIFEST_NAMES
            .iter()
            .map(|name| (*name).to_string())
            .collect(),
        coverage,
        notes,
        digest: String::new(),
    };
    report.recompute_digest();
    Ok(report)
}

/// The first line of an error, so a refusal key is stable rather than path-specific.
fn reason_of(error: &CatalogError) -> String {
    let text = error.to_string();
    text.lines().next().unwrap_or("unknown error").to_string()
}

/// Names of the ecosystems this crate trusts to be *complete* about a repository's
/// dependencies. Used by documentation and by callers deciding whether a report can back a
/// supply-chain claim.
pub fn modelled_ecosystems() -> BTreeSet<&'static str> {
    Ecosystem::all()
        .iter()
        .map(|ecosystem| ecosystem.as_str())
        .collect()
}
