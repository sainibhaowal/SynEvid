//! Schema registry compatibility (FR-ADV-015). Pluggable policies over field sets.

use autopsy_domain::CoverageState;
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("invalid schema: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompatibilityMode {
    Strict,
    Backward,
    Forward,
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaField {
    pub name: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schema {
    pub subject: String,
    pub fields: Vec<SchemaField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityResult {
    pub mode: CompatibilityMode,
    pub compatible: bool,
    pub removed_fields: Vec<String>,
    pub added_fields: Vec<String>,
    pub coverage: CoverageState,
    pub digest: String,
}

pub fn compare(old: &Schema, new: &Schema, mode: CompatibilityMode) -> CompatibilityResult {
    let old_names: BTreeSet<String> = old.fields.iter().map(|f| f.name.clone()).collect();
    let new_names: BTreeSet<String> = new.fields.iter().map(|f| f.name.clone()).collect();
    let removed: Vec<String> = old_names.difference(&new_names).cloned().collect();
    let added: Vec<String> = new_names.difference(&old_names).cloned().collect();

    let compatible = match mode {
        CompatibilityMode::Strict => removed.is_empty() && added.is_empty(),
        CompatibilityMode::Backward => removed.is_empty(), // new can read old
        CompatibilityMode::Forward => added.is_empty(),    // old can read new
        CompatibilityMode::Full => removed.is_empty() && added.is_empty(),
    };

    let mut hasher = Hasher::new();
    hasher.update(format!("{:?}", mode).as_bytes());
    for f in &removed {
        hasher.update(f.as_bytes());
    }
    for f in &added {
        hasher.update(f.as_bytes());
    }
    let digest = hasher.finalize().to_hex().to_string();

    CompatibilityResult {
        mode,
        compatible,
        removed_fields: removed,
        added_fields: added,
        coverage: CoverageState::Verified,
        digest,
    }
}
