//! Catalog engine for Backstage entities and blast radius

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use blake3;
use tracing::{debug, info, warn};

use autopsy_catalog::{CatalogScanner, BlastRadiusEngine, CatalogEntity, CatalogManifest};
use autopsy_storage::StorageEngine;

/// Catalog engine
pub struct CatalogEngine {
    // Stateless - uses storage
}

impl CatalogEngine {
    pub fn new() -> Self {
        Self {}
    }

    /// Scan catalog entities
    pub fn scan_catalog(
        &self,
        storage: &mut StorageEngine,
        repo_root: &Path,
        max_file_bytes: Option<u64>,
    ) -> Result<CatalogScanResult, String> {
        let scanner = CatalogScanner::new(repo_root, max_file_bytes.unwrap_or(1_048_576));
        let catalog = scanner.scan()
            .map_err(|e| format!("Catalog scan failed: {}", e))?;

        // Persist entities
        // storage.insert_catalog_entities(repo_id, &catalog.entities)?;
        // storage.insert_catalog_manifests(repo_id, &catalog.manifests)?;
        // storage.insert_catalog_dependencies(repo_id, &catalog.dependencies)?;

        Ok(CatalogScanResult {
            entities: catalog.entities.into_iter().map(Into::into).collect(),
            manifests: catalog.manifests.into_iter().map(Into::into).collect(),
            dependencies: catalog.dependencies.into_iter().map(Into::into).collect(),
            coverage: catalog.coverage,
            digest: catalog.digest,
            files_scanned: catalog.files_scanned,
            unmodelled_kinds: catalog.unmodelled_kinds,
            refusals: catalog.refusals,
            duplicate_entities: catalog.duplicate_entities,
            notes: catalog.notes,
        })
    }

    /// Compute blast radius for a component
    pub fn compute_blast_radius(
        &self,
        storage: &StorageEngine,
        repo_id: &str,
        component_ref: &str,
        max_file_bytes: Option<u64>,
    ) -> Result<BlastRadiusResult, String> {
        let engine = BlastRadiusEngine::new(storage, repo_id, max_file_bytes.unwrap_or(1_048_576));
        let blast_radius = engine.compute(component_ref)
            .map_err(|e| format!("Blast radius computation failed: {}", e))?;

        Ok(BlastRadiusResult {
            component: blast_radius.component,
            found: blast_radius.found,
            coverage: blast_radius.coverage,
            owners: blast_radius.owners,
            owner_is_team: blast_radius.owner_is_team,
            system: blast_radius.system,
            domain: blast_radius.domain,
            co_owned_components: blast_radius.co_owned_components,
            siblings: blast_radius.siblings,
            provided_apis: blast_radius.provided_apis,
            consumed_apis: blast_radius.consumed_apis,
            dependents: blast_radius.dependents,
            unresolved: blast_radius.unresolved,
            notes: blast_radius.notes,
        })
    }
}

// Result types

#[derive(Debug, Serialize, Deserialize)]
pub struct CatalogScanResult {
    pub entities: Vec<CatalogEntityInfo>,
    pub manifests: Vec<CatalogManifestInfo>,
    pub dependencies: Vec<CatalogDependencyInfo>,
    pub coverage: String,
    pub digest: String,
    pub files_scanned: usize,
    pub unmodelled_kinds: BTreeMap<String, usize>,
    pub refusals: BTreeMap<String, usize>,
    pub duplicate_entities: usize,
    pub notes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CatalogEntityInfo {
    pub entity_type: String,
    pub name: String,
    pub namespace: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub ownership: Vec<CatalogOwnershipInfo>,
    pub system: Option<String>,
    pub domain: Option<String>,
    pub provides_apis: Vec<String>,
    pub consumes_apis: Vec<String>,
    pub depends_on: Vec<String>,
    pub file_path: String,
}

impl From<CatalogEntity> for CatalogEntityInfo {
    fn from(e: CatalogEntity) -> Self {
        Self {
            entity_type: e.entity_type,
            name: e.name,
            namespace: e.namespace,
            title: e.title,
            description: e.description,
            ownership: e.ownership.into_iter().map(Into::into).collect(),
            system: e.system,
            domain: e.domain,
            provides_apis: e.provides_apis,
            consumes_apis: e.consumes_apis,
            depends_on: e.depends_on,
            file_path: e.file_path,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CatalogOwnershipInfo {
    pub type_: String,
    pub name: String,
    pub namespace: Option<String>,
}

impl From<autopsy_catalog::CatalogOwnership> for CatalogOwnershipInfo {
    fn from(o: autopsy_catalog::CatalogOwnership) -> Self {
        Self {
            type_: o.type_,
            name: o.name,
            namespace: o.namespace,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CatalogManifestInfo {
    pub file_path: String,
    pub format: String,
    pub entities: Vec<String>,
}

impl From<CatalogManifest> for CatalogManifestInfo {
    fn from(m: CatalogManifest) -> Self {
        Self {
            file_path: m.file_path,
            format: m.format,
            entities: m.entities,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CatalogDependencyInfo {
    pub from_entity: String,
    pub to_entity: String,
    pub dependency_type: String,
}

impl From<autopsy_catalog::CatalogDependency> for CatalogDependencyInfo {
    fn from(d: autopsy_catalog::CatalogDependency) -> Self {
        Self {
            from_entity: d.from_entity,
            to_entity: d.to_entity,
            dependency_type: d.dependency_type,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlastRadiusResult {
    pub component: String,
    pub found: bool,
    pub coverage: String,
    pub owners: Vec<String>,
    pub owner_is_team: bool,
    pub system: Option<String>,
    pub domain: Option<String>,
    pub co_owned_components: Vec<String>,
    pub siblings: Vec<String>,
    pub provided_apis: Vec<String>,
    pub consumed_apis: Vec<String>,
    pub dependents: Vec<String>,
    pub unresolved: Vec<String>,
    pub notes: Vec<String>,
}