//! Workspace / tenant registry (FR-ADV-010..012, 4.6). Read-only federation metadata.

use autopsy_domain::CoverageState;
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("duplicate repository id: {0}")]
    DuplicateRepo(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RepoEntry {
    pub id: String,
    pub provider: String,
    pub org: String,
    pub repo: String,
    pub accessible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registry {
    pub repos: BTreeMap<String, RepoEntry>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            repos: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, entry: RepoEntry) -> Result<(), RegistryError> {
        if self.repos.contains_key(&entry.id) {
            return Err(RegistryError::DuplicateRepo(entry.id.clone()));
        }
        self.repos.insert(entry.id.clone(), entry);
        Ok(())
    }

    /// Coverage counts: known, accessible, analyzed(=accessible here), excluded.
    pub fn coverage_summary(&self) -> (usize, usize, usize, usize) {
        let known = self.repos.len();
        let accessible = self.repos.values().filter(|r| r.accessible).count();
        let analyzed = accessible;
        let excluded = known - accessible;
        (known, accessible, analyzed, excluded)
    }

    pub fn digest(&self) -> String {
        let mut hasher = Hasher::new();
        hasher.update(b"registry-v1");
        for (id, r) in &self.repos {
            hasher.update(id.as_bytes());
            hasher.update(r.provider.as_bytes());
            hasher.update(r.org.as_bytes());
            hasher.update(r.repo.as_bytes());
        }
        hasher.finalize().to_hex().to_string()
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

/// Coverage state for a federation query given the registry and a caller that can see
/// `accessible` repos only.
pub fn federation_coverage(registry: &Registry) -> CoverageState {
    let (known, accessible, _, excluded) = registry.coverage_summary();
    if known == 0 {
        CoverageState::Unknown
    } else if excluded > 0 {
        CoverageState::Partial
    } else if accessible == known {
        CoverageState::Verified
    } else {
        CoverageState::Partial
    }
}

pub mod monorepo;
pub use monorepo::{MonorepoKind, MonorepoPackage, MonorepoWorkspace};
