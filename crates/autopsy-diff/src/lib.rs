//! autopsy-diff: Semantic Change & Diff Engine (FR-007).
//!
//! Architectural Invariants:
//! - Offline & deterministic: No network calls, no HashMap iteration in canonical output.
//! - File deltas: add, delete, modify, rename.
//! - Ambiguous rename rule: If multiple renames match the same content hash or shape,
//!   they MUST remain Added + Deleted (never guess).
//! - Symbol deltas: add, remove, move, rename, body modify, signature modify, visibility modify.
//! - Contract deltas: add, remove, modify with breaking change detection.
//! - Edge deltas: add, remove, coverage modification.
//! - Changed frontier calculation: directly changed symbols + 1-hop impacted boundary.
//! - Golden diff fixture compatibility.

use autopsy_domain::{
    ChangeSet, Contract, CoverageState, Edge, EdgeKind, FileUnit, SnapshotId, SourceLocation,
    Symbol, SymbolId, Visibility,
};
use autopsy_graph::DependencyGraph;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// Errors emitted during semantic diff operations.
#[derive(Debug, Error)]
pub enum DiffError {
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Diff calculation error: {0}")]
    Calculation(String),
}

/// Typed delta for an individual file unit (FR-007).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FileDelta {
    Added {
        path: String,
        content_hash: String,
        language: String,
    },
    Deleted {
        path: String,
        content_hash: String,
        language: String,
    },
    Modified {
        path: String,
        before_hash: String,
        after_hash: String,
    },
    Renamed {
        old_path: String,
        new_path: String,
        content_hash: String,
    },
}

impl FileDelta {
    pub fn path(&self) -> &str {
        match self {
            Self::Added { path, .. } => path,
            Self::Deleted { path, .. } => path,
            Self::Modified { path, .. } => path,
            Self::Renamed { new_path, .. } => new_path,
        }
    }
}

/// Typed delta for an individual symbol declaration (FR-007).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SymbolDelta {
    Added {
        symbol: Symbol,
    },
    Removed {
        symbol: Symbol,
    },
    Moved {
        symbol_id: SymbolId,
        old_path: String,
        new_path: String,
    },
    Renamed {
        old_id: SymbolId,
        new_id: SymbolId,
        old_name: String,
        new_name: String,
        path: String,
    },
    BodyModified {
        symbol_id: SymbolId,
        path: String,
        old_range: Option<SourceLocation>,
        new_range: Option<SourceLocation>,
    },
    SignatureModified {
        symbol_id: SymbolId,
        path: String,
        old_signature: Option<String>,
        new_signature: Option<String>,
    },
    VisibilityModified {
        symbol_id: SymbolId,
        path: String,
        old_visibility: Visibility,
        new_visibility: Visibility,
    },
    KindModified {
        symbol_id: SymbolId,
        path: String,
        old_kind: String,
        new_kind: String,
    },
}

impl SymbolDelta {
    pub fn symbol_id(&self) -> &SymbolId {
        match self {
            Self::Added { symbol } => &symbol.stable_id,
            Self::Removed { symbol } => &symbol.stable_id,
            Self::Moved { symbol_id, .. } => symbol_id,
            Self::Renamed { new_id, .. } => new_id,
            Self::BodyModified { symbol_id, .. } => symbol_id,
            Self::SignatureModified { symbol_id, .. } => symbol_id,
            Self::VisibilityModified { symbol_id, .. } => symbol_id,
            Self::KindModified { symbol_id, .. } => symbol_id,
        }
    }
}

/// Typed delta for a normalized callable or interface contract (FR-007, FR-009).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContractDelta {
    Added {
        contract: Contract,
    },
    Removed {
        contract: Contract,
    },
    Modified {
        owner: SymbolId,
        old_contract: Contract,
        new_contract: Contract,
        is_breaking: bool,
        breaking_reasons: Vec<String>,
    },
}

impl ContractDelta {
    pub fn owner(&self) -> &SymbolId {
        match self {
            Self::Added { contract } => &contract.owner,
            Self::Removed { contract } => &contract.owner,
            Self::Modified { owner, .. } => owner,
        }
    }
}

/// Typed delta for a dependency multigraph edge (FR-007).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EdgeDelta {
    Added {
        edge: Edge,
    },
    Removed {
        edge: Edge,
    },
    CoverageModified {
        source: SymbolId,
        target: SymbolId,
        kind: EdgeKind,
        old_coverage: CoverageState,
        new_coverage: CoverageState,
    },
}

/// Frontier of changed entities and immediate 1-hop impact boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedFrontier {
    /// Symbols directly added, removed, or modified
    pub direct_symbols: BTreeSet<SymbolId>,
    /// 1-hop dependent boundary symbols impacted by the direct changes
    pub impacted_boundary: BTreeSet<SymbolId>,
    /// Files affected by any file or symbol modification
    pub affected_files: BTreeSet<String>,
}

/// Summary statistics for a semantic diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DiffSummary {
    pub files_added: usize,
    pub files_deleted: usize,
    pub files_modified: usize,
    pub files_renamed: usize,
    pub symbols_added: usize,
    pub symbols_removed: usize,
    pub symbols_moved: usize,
    pub symbols_renamed: usize,
    pub symbols_modified: usize,
    pub contracts_added: usize,
    pub contracts_removed: usize,
    pub contracts_modified: usize,
    pub breaking_contract_changes: usize,
    pub edges_added: usize,
    pub edges_removed: usize,
}

/// Complete deterministic semantic diff result across two repository snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticDiff {
    pub before_snapshot_id: SnapshotId,
    pub after_snapshot_id: SnapshotId,
    pub file_deltas: Vec<FileDelta>,
    pub symbol_deltas: Vec<SymbolDelta>,
    pub contract_deltas: Vec<ContractDelta>,
    pub edge_deltas: Vec<EdgeDelta>,
    pub frontier: ChangedFrontier,
    pub summary: DiffSummary,
    pub diff_digest: String,
}

impl SemanticDiff {
    /// Converts semantic diff to summary ChangeSet for legacy/generic engine consumption.
    pub fn to_changeset(&self) -> ChangeSet {
        ChangeSet {
            before_snapshot_id: self.before_snapshot_id.clone(),
            after_snapshot_id: self.after_snapshot_id.clone(),
            file_deltas: self
                .file_deltas
                .iter()
                .map(|f| format!("{:?}", f))
                .collect(),
            symbol_deltas: self
                .symbol_deltas
                .iter()
                .map(|s| format!("{:?}", s))
                .collect(),
            contract_deltas: self
                .contract_deltas
                .iter()
                .map(|c| format!("{:?}", c))
                .collect(),
            edge_deltas: self
                .edge_deltas
                .iter()
                .map(|e| format!("{:?}", e))
                .collect(),
        }
    }
}

/// Inputs provided to the semantic diff engine (FR-007).
#[derive(Debug, Clone)]
pub struct DiffInput<'a> {
    pub before_snapshot_id: SnapshotId,
    pub after_snapshot_id: SnapshotId,
    pub before_files: &'a BTreeMap<String, FileUnit>,
    pub after_files: &'a BTreeMap<String, FileUnit>,
    pub before_graph: &'a DependencyGraph,
    pub after_graph: &'a DependencyGraph,
    pub before_contracts: &'a [Contract],
    pub after_contracts: &'a [Contract],
}

/// Semantic difference engine for computing code changes (FR-007).
pub struct DiffEngine;

impl DiffEngine {
    /// Computes full semantic diff between before and after states.
    pub fn compute_diff(input: DiffInput<'_>) -> SemanticDiff {
        // 1. File deltas & rename mapping
        let (file_deltas, file_renames) =
            Self::compute_file_deltas(input.before_files, input.after_files);

        // 2. Symbol deltas
        let symbol_deltas =
            Self::compute_symbol_deltas(input.before_graph, input.after_graph, &file_renames);

        // 3. Contract deltas
        let contract_deltas =
            Self::compute_contract_deltas(input.before_contracts, input.after_contracts);

        // 4. Edge deltas
        let edge_deltas = Self::compute_edge_deltas(input.before_graph, input.after_graph);

        // 5. Frontier calculation
        let frontier = Self::compute_frontier(
            &file_deltas,
            &symbol_deltas,
            input.before_graph,
            input.after_graph,
        );

        // 6. Summary metrics
        let summary =
            Self::compute_summary(&file_deltas, &symbol_deltas, &contract_deltas, &edge_deltas);

        // 7. Cryptographic BLAKE3 digest of change
        let diff_digest = Self::compute_diff_digest(
            &input.before_snapshot_id,
            &input.after_snapshot_id,
            &file_deltas,
            &symbol_deltas,
            &contract_deltas,
            &edge_deltas,
        );

        SemanticDiff {
            before_snapshot_id: input.before_snapshot_id,
            after_snapshot_id: input.after_snapshot_id,
            file_deltas,
            symbol_deltas,
            contract_deltas,
            edge_deltas,
            frontier,
            summary,
            diff_digest,
        }
    }

    /// Computes file deltas with strict ambiguous rename disambiguation (FR-007).
    fn compute_file_deltas(
        before_files: &BTreeMap<String, FileUnit>,
        after_files: &BTreeMap<String, FileUnit>,
    ) -> (Vec<FileDelta>, BTreeMap<String, String>) {
        let mut deltas = Vec::new();
        let mut file_renames = BTreeMap::new();

        let mut candidate_deleted = BTreeMap::new();
        let mut candidate_added = BTreeMap::new();

        // Find modified, deleted candidates, and unchanged files
        for (path, before_unit) in before_files {
            if let Some(after_unit) = after_files.get(path) {
                if before_unit.content_hash != after_unit.content_hash {
                    deltas.push(FileDelta::Modified {
                        path: path.clone(),
                        before_hash: before_unit.content_hash.clone(),
                        after_hash: after_unit.content_hash.clone(),
                    });
                }
            } else {
                candidate_deleted.insert(path.clone(), before_unit.clone());
            }
        }

        // Find added candidates
        for (path, after_unit) in after_files {
            if !before_files.contains_key(path) {
                candidate_added.insert(path.clone(), after_unit.clone());
            }
        }

        // Group candidate deletions and additions by content hash for rename detection
        let mut deleted_by_hash: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (path, unit) in &candidate_deleted {
            deleted_by_hash
                .entry(unit.content_hash.clone())
                .or_default()
                .push(path.clone());
        }

        let mut added_by_hash: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (path, unit) in &candidate_added {
            added_by_hash
                .entry(unit.content_hash.clone())
                .or_default()
                .push(path.clone());
        }

        let mut matched_deleted_paths = BTreeSet::new();
        let mut matched_added_paths = BTreeSet::new();

        // Ambiguous Rename Rule (FR-007):
        // Only classify as Renamed if EXACTLY ONE deleted matches EXACTLY ONE added.
        for (hash, del_paths) in &deleted_by_hash {
            if let Some(add_paths) = added_by_hash.get(hash)
                && del_paths.len() == 1
                && add_paths.len() == 1
            {
                let old_p = &del_paths[0];
                let new_p = &add_paths[0];
                deltas.push(FileDelta::Renamed {
                    old_path: old_p.clone(),
                    new_path: new_p.clone(),
                    content_hash: hash.clone(),
                });
                file_renames.insert(old_p.clone(), new_p.clone());
                matched_deleted_paths.insert(old_p.clone());
                matched_added_paths.insert(new_p.clone());
            }
        }

        // Unmatched deleted remain Deleted
        for (path, unit) in candidate_deleted {
            if !matched_deleted_paths.contains(&path) {
                deltas.push(FileDelta::Deleted {
                    path,
                    content_hash: unit.content_hash,
                    language: unit.language,
                });
            }
        }

        // Unmatched added remain Added
        for (path, unit) in candidate_added {
            if !matched_added_paths.contains(&path) {
                deltas.push(FileDelta::Added {
                    path,
                    content_hash: unit.content_hash,
                    language: unit.language,
                });
            }
        }

        // Deterministic sort
        deltas.sort_by(|a, b| a.path().cmp(b.path()));
        (deltas, file_renames)
    }

    /// Computes symbol deltas with rename disambiguation and attribute tracking (FR-007).
    fn compute_symbol_deltas(
        before_graph: &DependencyGraph,
        after_graph: &DependencyGraph,
        file_renames: &BTreeMap<String, String>,
    ) -> Vec<SymbolDelta> {
        let mut deltas = Vec::new();

        let before_symbols = before_graph.all_symbols_sorted();
        let after_symbols = after_graph.all_symbols_sorted();

        let mut before_map: BTreeMap<&SymbolId, &Symbol> = BTreeMap::new();
        for sym in &before_symbols {
            before_map.insert(&sym.stable_id, sym);
        }

        let mut after_map: BTreeMap<&SymbolId, &Symbol> = BTreeMap::new();
        for sym in &after_symbols {
            after_map.insert(&sym.stable_id, sym);
        }

        let mut matched_before = BTreeSet::new();
        let mut matched_after = BTreeSet::new();

        // 1. Direct stable_id matches
        for (&id, &before_sym) in &before_map {
            if let Some(&after_sym) = after_map.get(id) {
                matched_before.insert(id);
                matched_after.insert(id);

                let before_path = before_sym.range.as_ref().map(|r| r.path.as_str());
                let after_path = after_sym.range.as_ref().map(|r| r.path.as_str());

                // Check moved file
                if let (Some(bp), Some(ap)) = (before_path, after_path)
                    && bp != ap
                {
                    deltas.push(SymbolDelta::Moved {
                        symbol_id: id.clone(),
                        old_path: bp.to_string(),
                        new_path: ap.to_string(),
                    });
                }

                // Check kind changed
                if before_sym.kind != after_sym.kind {
                    deltas.push(SymbolDelta::KindModified {
                        symbol_id: id.clone(),
                        path: after_path.unwrap_or("").to_string(),
                        old_kind: before_sym.kind.clone(),
                        new_kind: after_sym.kind.clone(),
                    });
                }

                // Check visibility changed
                if before_sym.visibility != after_sym.visibility {
                    deltas.push(SymbolDelta::VisibilityModified {
                        symbol_id: id.clone(),
                        path: after_path.unwrap_or("").to_string(),
                        old_visibility: before_sym.visibility,
                        new_visibility: after_sym.visibility,
                    });
                }

                // Check signature modified
                if before_sym.normalized_signature != after_sym.normalized_signature {
                    deltas.push(SymbolDelta::SignatureModified {
                        symbol_id: id.clone(),
                        path: after_path.unwrap_or("").to_string(),
                        old_signature: before_sym.normalized_signature.clone(),
                        new_signature: after_sym.normalized_signature.clone(),
                    });
                } else if before_sym.range != after_sym.range {
                    // Body modified if signature identical but range shifted/edited
                    deltas.push(SymbolDelta::BodyModified {
                        symbol_id: id.clone(),
                        path: after_path.unwrap_or("").to_string(),
                        old_range: before_sym.range.clone(),
                        new_range: after_sym.range.clone(),
                    });
                }
            }
        }

        // 2. Unmatched symbols: Candidate for symbol rename matching
        let mut candidate_removed: Vec<&Symbol> = Vec::new();
        for (&id, &sym) in &before_map {
            if !matched_before.contains(id) {
                candidate_removed.push(sym);
            }
        }

        let mut candidate_added: Vec<&Symbol> = Vec::new();
        for (&id, &sym) in &after_map {
            if !matched_after.contains(id) {
                candidate_added.push(sym);
            }
        }

        // Group candidate renames by (effective_file, kind, signature)
        // Ambiguous Rename Rule: If exactly one candidate removed matches one candidate added, it is a Rename.
        let mut matched_rename_rem = BTreeSet::new();
        let mut matched_rename_add = BTreeSet::new();

        for (rem_idx, rem_sym) in candidate_removed.iter().enumerate() {
            let rem_path = rem_sym
                .range
                .as_ref()
                .map(|r| r.path.as_str())
                .unwrap_or("");
            let effective_target_path = file_renames
                .get(rem_path)
                .map(|s| s.as_str())
                .unwrap_or(rem_path);

            let mut potential_matches = Vec::new();
            for (add_idx, add_sym) in candidate_added.iter().enumerate() {
                if matched_rename_add.contains(&add_idx) {
                    continue;
                }
                let add_path = add_sym
                    .range
                    .as_ref()
                    .map(|r| r.path.as_str())
                    .unwrap_or("");

                // Same effective file, same kind, same signature
                if add_path == effective_target_path
                    && rem_sym.kind == add_sym.kind
                    && rem_sym.normalized_signature.is_some()
                    && rem_sym.normalized_signature == add_sym.normalized_signature
                {
                    potential_matches.push((add_idx, *add_sym));
                }
            }

            // Only match if unambiguous (exactly 1 candidate)
            if potential_matches.len() == 1 {
                let (add_idx, add_sym) = potential_matches[0];
                // Check if add_sym is uniquely matched to this rem_sym
                let reverse_matches = candidate_removed
                    .iter()
                    .filter(|r| {
                        let r_path = r.range.as_ref().map(|x| x.path.as_str()).unwrap_or("");
                        let eff = file_renames
                            .get(r_path)
                            .map(|s| s.as_str())
                            .unwrap_or(r_path);
                        let a_path = add_sym
                            .range
                            .as_ref()
                            .map(|x| x.path.as_str())
                            .unwrap_or("");
                        eff == a_path
                            && r.kind == add_sym.kind
                            && r.normalized_signature == add_sym.normalized_signature
                    })
                    .count();

                if reverse_matches == 1 {
                    deltas.push(SymbolDelta::Renamed {
                        old_id: rem_sym.stable_id.clone(),
                        new_id: add_sym.stable_id.clone(),
                        old_name: rem_sym.qualified_name.clone(),
                        new_name: add_sym.qualified_name.clone(),
                        path: add_sym
                            .range
                            .as_ref()
                            .map(|r| r.path.clone())
                            .unwrap_or_default(),
                    });
                    matched_rename_rem.insert(rem_idx);
                    matched_rename_add.insert(add_idx);
                }
            }
        }

        // Remaining unmatched removed -> SymbolDelta::Removed
        for (idx, sym) in candidate_removed.iter().enumerate() {
            if !matched_rename_rem.contains(&idx) {
                deltas.push(SymbolDelta::Removed {
                    symbol: (*sym).clone(),
                });
            }
        }

        // Remaining unmatched added -> SymbolDelta::Added
        for (idx, sym) in candidate_added.iter().enumerate() {
            if !matched_rename_add.contains(&idx) {
                deltas.push(SymbolDelta::Added {
                    symbol: (*sym).clone(),
                });
            }
        }

        // Deterministic sort
        deltas.sort_by(|a, b| a.symbol_id().cmp(b.symbol_id()));
        deltas
    }

    /// Computes contract deltas and classifies breaking changes (FR-007, FR-009).
    fn compute_contract_deltas(
        before_contracts: &[Contract],
        after_contracts: &[Contract],
    ) -> Vec<ContractDelta> {
        let mut deltas = Vec::new();

        let mut before_map: BTreeMap<&SymbolId, &Contract> = BTreeMap::new();
        for c in before_contracts {
            before_map.insert(&c.owner, c);
        }

        let mut after_map: BTreeMap<&SymbolId, &Contract> = BTreeMap::new();
        for c in after_contracts {
            after_map.insert(&c.owner, c);
        }

        // Matched contracts
        for (&owner, &before_c) in &before_map {
            if let Some(&after_c) = after_map.get(owner) {
                if before_c != after_c {
                    let mut is_breaking = false;
                    let mut breaking_reasons = Vec::new();

                    // Visibility reduction check
                    if before_c.visibility == Visibility::Public
                        && after_c.visibility != Visibility::Public
                    {
                        is_breaking = true;
                        breaking_reasons.push(format!(
                            "Visibility reduced from Public to {:?}",
                            after_c.visibility
                        ));
                    }

                    // Inputs check
                    if before_c.inputs != after_c.inputs {
                        is_breaking = true;
                        breaking_reasons.push(format!(
                            "Inputs changed from {:?} to {:?}",
                            before_c.inputs, after_c.inputs
                        ));
                    }

                    // Return type check
                    if before_c.output != after_c.output {
                        is_breaking = true;
                        breaking_reasons.push(format!(
                            "Output return type changed from {:?} to {:?}",
                            before_c.output, after_c.output
                        ));
                    }

                    deltas.push(ContractDelta::Modified {
                        owner: owner.clone(),
                        old_contract: before_c.clone(),
                        new_contract: after_c.clone(),
                        is_breaking,
                        breaking_reasons,
                    });
                }
            } else {
                deltas.push(ContractDelta::Removed {
                    contract: before_c.clone(),
                });
            }
        }

        // Added contracts
        for (&owner, &after_c) in &after_map {
            if !before_map.contains_key(owner) {
                deltas.push(ContractDelta::Added {
                    contract: after_c.clone(),
                });
            }
        }

        deltas.sort_by(|a, b| a.owner().cmp(b.owner()));
        deltas
    }

    /// Computes edge deltas between graphs (FR-007).
    fn compute_edge_deltas(
        before_graph: &DependencyGraph,
        after_graph: &DependencyGraph,
    ) -> Vec<EdgeDelta> {
        let mut deltas = Vec::new();

        let before_edges = before_graph.all_edges_sorted();
        let after_edges = after_graph.all_edges_sorted();

        // Key: (source, target, kind, provenance)
        type EdgeKey<'a> = (&'a SymbolId, &'a SymbolId, EdgeKind, &'a str);

        let mut before_map: BTreeMap<EdgeKey, &Edge> = BTreeMap::new();
        for e in &before_edges {
            before_map.insert((&e.source, &e.target, e.kind, &e.provenance), e);
        }

        let mut after_map: BTreeMap<EdgeKey, &Edge> = BTreeMap::new();
        for e in &after_edges {
            after_map.insert((&e.source, &e.target, e.kind, &e.provenance), e);
        }

        for (&key, &before_e) in &before_map {
            if let Some(&after_e) = after_map.get(&key) {
                if before_e.coverage != after_e.coverage {
                    deltas.push(EdgeDelta::CoverageModified {
                        source: before_e.source.clone(),
                        target: before_e.target.clone(),
                        kind: before_e.kind,
                        old_coverage: before_e.coverage,
                        new_coverage: after_e.coverage,
                    });
                }
            } else {
                deltas.push(EdgeDelta::Removed {
                    edge: (*before_e).clone(),
                });
            }
        }

        for (&key, &after_e) in &after_map {
            if !before_map.contains_key(&key) {
                deltas.push(EdgeDelta::Added {
                    edge: (*after_e).clone(),
                });
            }
        }

        deltas.sort_by(|a, b| match (a, b) {
            (EdgeDelta::Added { edge: e1 }, EdgeDelta::Added { edge: e2 }) => e1
                .source
                .cmp(&e2.source)
                .then_with(|| e1.target.cmp(&e2.target)),
            (EdgeDelta::Removed { edge: e1 }, EdgeDelta::Removed { edge: e2 }) => e1
                .source
                .cmp(&e2.source)
                .then_with(|| e1.target.cmp(&e2.target)),
            (
                EdgeDelta::CoverageModified {
                    source: s1,
                    target: t1,
                    ..
                },
                EdgeDelta::CoverageModified {
                    source: s2,
                    target: t2,
                    ..
                },
            ) => s1.cmp(s2).then_with(|| t1.cmp(t2)),
            (EdgeDelta::Added { .. }, _) => std::cmp::Ordering::Less,
            (_, EdgeDelta::Added { .. }) => std::cmp::Ordering::Greater,
            (EdgeDelta::Removed { .. }, _) => std::cmp::Ordering::Less,
            (_, EdgeDelta::Removed { .. }) => std::cmp::Ordering::Greater,
        });

        deltas
    }

    /// Computes the changed frontier and 1-hop impact boundary.
    fn compute_frontier(
        file_deltas: &[FileDelta],
        symbol_deltas: &[SymbolDelta],
        before_graph: &DependencyGraph,
        after_graph: &DependencyGraph,
    ) -> ChangedFrontier {
        let mut direct_symbols = BTreeSet::new();
        let mut affected_files = BTreeSet::new();

        for fd in file_deltas {
            affected_files.insert(fd.path().to_string());
        }

        for sd in symbol_deltas {
            direct_symbols.insert(sd.symbol_id().clone());
            match sd {
                SymbolDelta::Added { symbol } => {
                    if let Some(r) = &symbol.range {
                        affected_files.insert(r.path.clone());
                    }
                }
                SymbolDelta::Removed { symbol } => {
                    if let Some(r) = &symbol.range {
                        affected_files.insert(r.path.clone());
                    }
                }
                SymbolDelta::Moved {
                    old_path, new_path, ..
                } => {
                    affected_files.insert(old_path.clone());
                    affected_files.insert(new_path.clone());
                }
                SymbolDelta::Renamed { path, .. }
                | SymbolDelta::BodyModified { path, .. }
                | SymbolDelta::SignatureModified { path, .. }
                | SymbolDelta::VisibilityModified { path, .. }
                | SymbolDelta::KindModified { path, .. } => {
                    if !path.is_empty() {
                        affected_files.insert(path.clone());
                    }
                }
            }
        }

        // 1-hop impacted boundary: all symbols in after_graph or before_graph that depend directly on direct_symbols
        let mut impacted_boundary = BTreeSet::new();
        for sym_id in &direct_symbols {
            for (pred, _) in after_graph.predecessors(sym_id) {
                if !direct_symbols.contains(&pred.stable_id) {
                    impacted_boundary.insert(pred.stable_id.clone());
                }
            }
            for (pred, _) in before_graph.predecessors(sym_id) {
                if !direct_symbols.contains(&pred.stable_id) {
                    impacted_boundary.insert(pred.stable_id.clone());
                }
            }
        }

        ChangedFrontier {
            direct_symbols,
            impacted_boundary,
            affected_files,
        }
    }

    fn compute_summary(
        file_deltas: &[FileDelta],
        symbol_deltas: &[SymbolDelta],
        contract_deltas: &[ContractDelta],
        edge_deltas: &[EdgeDelta],
    ) -> DiffSummary {
        let mut summary = DiffSummary::default();

        for fd in file_deltas {
            match fd {
                FileDelta::Added { .. } => summary.files_added += 1,
                FileDelta::Deleted { .. } => summary.files_deleted += 1,
                FileDelta::Modified { .. } => summary.files_modified += 1,
                FileDelta::Renamed { .. } => summary.files_renamed += 1,
            }
        }

        for sd in symbol_deltas {
            match sd {
                SymbolDelta::Added { .. } => summary.symbols_added += 1,
                SymbolDelta::Removed { .. } => summary.symbols_removed += 1,
                SymbolDelta::Moved { .. } => summary.symbols_moved += 1,
                SymbolDelta::Renamed { .. } => summary.symbols_renamed += 1,
                SymbolDelta::BodyModified { .. }
                | SymbolDelta::SignatureModified { .. }
                | SymbolDelta::VisibilityModified { .. }
                | SymbolDelta::KindModified { .. } => summary.symbols_modified += 1,
            }
        }

        for cd in contract_deltas {
            match cd {
                ContractDelta::Added { .. } => summary.contracts_added += 1,
                ContractDelta::Removed { .. } => summary.contracts_removed += 1,
                ContractDelta::Modified { is_breaking, .. } => {
                    summary.contracts_modified += 1;
                    if *is_breaking {
                        summary.breaking_contract_changes += 1;
                    }
                }
            }
        }

        for ed in edge_deltas {
            match ed {
                EdgeDelta::Added { .. } => summary.edges_added += 1,
                EdgeDelta::Removed { .. } => summary.edges_removed += 1,
                EdgeDelta::CoverageModified { .. } => {}
            }
        }

        summary
    }

    fn compute_diff_digest(
        before_id: &SnapshotId,
        after_id: &SnapshotId,
        file_deltas: &[FileDelta],
        symbol_deltas: &[SymbolDelta],
        contract_deltas: &[ContractDelta],
        edge_deltas: &[EdgeDelta],
    ) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(before_id.as_str().as_bytes());
        hasher.update(after_id.as_str().as_bytes());

        for fd in file_deltas {
            hasher.update(format!("{:?}", fd).as_bytes());
        }
        for sd in symbol_deltas {
            hasher.update(format!("{:?}", sd).as_bytes());
        }
        for cd in contract_deltas {
            hasher.update(format!("{:?}", cd).as_bytes());
        }
        for ed in edge_deltas {
            hasher.update(format!("{:?}", ed).as_bytes());
        }

        hasher.finalize().to_hex().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_unit(path: &str, hash: &str) -> FileUnit {
        FileUnit {
            path: path.to_string(),
            language: "typescript".to_string(),
            content_hash: hash.to_string(),
            is_generated: false,
        }
    }

    fn sample_sym(id: &str, file: &str, name: &str, sig: &str) -> Symbol {
        Symbol {
            stable_id: SymbolId::new(id),
            language_id: "typescript".to_string(),
            kind: "function".to_string(),
            qualified_name: name.to_string(),
            range: Some(SourceLocation {
                path: file.to_string(),
                start_line: 1,
                end_line: 5,
                start_col: Some(0),
                end_col: Some(10),
                symbol_id: Some(SymbolId::new(id)),
            }),
            normalized_signature: Some(sig.to_string()),
            visibility: Visibility::Public,
        }
    }

    #[test]
    fn test_unambiguous_file_rename() {
        let mut before = BTreeMap::new();
        before.insert(
            "src/old.ts".to_string(),
            sample_unit("src/old.ts", "hash_abc"),
        );

        let mut after = BTreeMap::new();
        after.insert(
            "src/new.ts".to_string(),
            sample_unit("src/new.ts", "hash_abc"),
        );

        let (deltas, renames) = DiffEngine::compute_file_deltas(&before, &after);
        assert_eq!(deltas.len(), 1);
        match &deltas[0] {
            FileDelta::Renamed {
                old_path,
                new_path,
                content_hash,
            } => {
                assert_eq!(old_path, "src/old.ts");
                assert_eq!(new_path, "src/new.ts");
                assert_eq!(content_hash, "hash_abc");
            }
            _ => panic!("Expected Renamed delta"),
        }
        assert_eq!(renames.get("src/old.ts"), Some(&"src/new.ts".to_string()));
    }

    #[test]
    fn test_ambiguous_file_rename_stays_add_and_delete() {
        // Two deleted files have the exact same hash, one added file matches
        let mut before = BTreeMap::new();
        before.insert(
            "src/dup1.ts".to_string(),
            sample_unit("src/dup1.ts", "same_hash"),
        );
        before.insert(
            "src/dup2.ts".to_string(),
            sample_unit("src/dup2.ts", "same_hash"),
        );

        let mut after = BTreeMap::new();
        after.insert(
            "src/new.ts".to_string(),
            sample_unit("src/new.ts", "same_hash"),
        );

        let (deltas, renames) = DiffEngine::compute_file_deltas(&before, &after);
        assert!(
            renames.is_empty(),
            "Ambiguous rename must not produce a rename mapping"
        );

        let added_count = deltas
            .iter()
            .filter(|d| matches!(d, FileDelta::Added { .. }))
            .count();
        let deleted_count = deltas
            .iter()
            .filter(|d| matches!(d, FileDelta::Deleted { .. }))
            .count();
        assert_eq!(added_count, 1, "Must remain Added");
        assert_eq!(deleted_count, 2, "Must remain Deleted");
    }

    #[test]
    fn test_symbol_signature_and_body_modifications() {
        let mut g1 = DependencyGraph::new();
        let s1 = sample_sym("sym::fn", "src/foo.ts", "run", "fn() -> void");
        g1.add_node(s1);

        let mut g2 = DependencyGraph::new();
        let mut s2 = sample_sym("sym::fn", "src/foo.ts", "run", "fn(val: number) -> void");
        s2.range = Some(SourceLocation {
            path: "src/foo.ts".to_string(),
            start_line: 1,
            end_line: 8, // line expanded
            start_col: Some(0),
            end_col: Some(20),
            symbol_id: Some(SymbolId::new("sym::fn")),
        });
        g2.add_node(s2);

        let deltas = DiffEngine::compute_symbol_deltas(&g1, &g2, &BTreeMap::new());
        assert_eq!(deltas.len(), 1);
        match &deltas[0] {
            SymbolDelta::SignatureModified {
                symbol_id,
                old_signature,
                new_signature,
                ..
            } => {
                assert_eq!(symbol_id, &SymbolId::new("sym::fn"));
                assert_eq!(old_signature.as_deref(), Some("fn() -> void"));
                assert_eq!(new_signature.as_deref(), Some("fn(val: number) -> void"));
            }
            _ => panic!("Expected SignatureModified"),
        }
    }

    #[test]
    fn test_contract_breaking_change_detection() {
        let before_c = Contract {
            owner: SymbolId::new("sym::api"),
            visibility: Visibility::Public,
            inputs: vec!["req: Request".to_string()],
            output: Some("Response".to_string()),
            effects: vec![],
        };

        // Breaking: visibility reduced to Internal, output changed
        let after_c = Contract {
            owner: SymbolId::new("sym::api"),
            visibility: Visibility::Internal,
            inputs: vec!["req: Request".to_string()],
            output: Some("Result<Response, Error>".to_string()),
            effects: vec![],
        };

        let deltas = DiffEngine::compute_contract_deltas(&[before_c], &[after_c]);
        assert_eq!(deltas.len(), 1);
        match &deltas[0] {
            ContractDelta::Modified {
                is_breaking,
                breaking_reasons,
                ..
            } => {
                assert!(*is_breaking);
                assert_eq!(breaking_reasons.len(), 2);
            }
            _ => panic!("Expected Modified contract delta"),
        }
    }
}
