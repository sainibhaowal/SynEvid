//! Virtual in-memory historical semantic snapshot and visual graph frame builder.
//!
//! Historical commits are analyzed in an isolated temporary tree. The connected
//! working tree is never used as an input and never mutated by this module.

use crate::model::*;
use crate::semantic;
use autopsy_graph_layout::DeterministicForceLayout;
use autopsy_plugin_api::GraphLayoutPlugin;
use autopsy_temporal::git::{GitError, GitObjectStore, ObjectId, flatten_tree};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub struct VirtualTreeBuilder {
    pub repo_root: PathBuf,
}

impl VirtualTreeBuilder {
    pub fn new(repo_root: impl AsRef<Path>) -> Self {
        Self {
            repo_root: repo_root.as_ref().to_path_buf(),
        }
    }

    /// Construct a semantic historical visual frame for any commit.
    pub fn build_frame(
        &self,
        commit_hex: &str,
        is_3d: bool,
    ) -> Result<TimeTravelVisualFrame, GitError> {
        let mut store = GitObjectStore::discover(&self.repo_root)?;
        let git_dir = store.git_dir().to_path_buf();
        let oid = crate::reader::resolve_revision(&mut store, &git_dir, commit_hex)?;
        let commit = store.commit(oid)?;
        let tree_curr = flatten_tree(&store, commit.tree)?;

        let (tree_prev, previous_state) = if let Some(parent_oid) = commit.parents.first() {
            let parent = store.commit(*parent_oid)?;
            let previous_tree = flatten_tree(&store, parent.tree)?;
            let previous = semantic::analyze_tree(
                &self.repo_root,
                &store,
                parent.tree,
                &format!("git:{}", parent_oid.to_hex()),
            )?;
            (previous_tree, Some(previous))
        } else {
            (BTreeMap::new(), None)
        };
        let current = semantic::analyze_tree(
            &self.repo_root,
            &store,
            commit.tree,
            &format!("git:{}", oid.to_hex()),
        )?;

        let (added_files, modified_files, deleted_files) =
            Self::file_changes(&tree_curr, &tree_prev);

        let previous_graph = previous_state.as_ref().map(|state| &state.graph);
        let mut symbols: BTreeMap<String, SymbolVisual> = BTreeMap::new();
        for symbol in current.graph.all_symbols_sorted() {
            let status = match previous_graph.and_then(|graph| graph.get_node(&symbol.stable_id)) {
                None => "added",
                Some(previous) if previous != symbol => "modified",
                Some(_) => "unchanged",
            };
            symbols.insert(
                symbol.stable_id.to_string(),
                SymbolVisual::from_symbol(symbol, status),
            );
        }

        if let Some(previous) = previous_state.as_ref() {
            for symbol in previous.graph.all_symbols_sorted() {
                if !current.graph.contains_node(&symbol.stable_id) {
                    symbols.insert(
                        symbol.stable_id.to_string(),
                        SymbolVisual::from_symbol(symbol, "deleted"),
                    );
                }
            }
        }

        // Keep the existing renderer's bounded-node behavior, while making the
        // selected historical state semantic rather than file-name based.
        let display: Vec<SymbolVisual> = symbols.into_values().take(250).collect();
        let display_ids: BTreeSet<String> = display.iter().map(|node| node.id.clone()).collect();
        let display_nodes: Vec<String> = display.iter().map(|node| node.id.clone()).collect();

        let mut edges = Vec::new();
        for edge in current.graph.all_edges_sorted() {
            let source = edge.source.to_string();
            let target = edge.target.to_string();
            if display_ids.contains(&source) && display_ids.contains(&target) {
                edges.push((source, target));
            }
        }
        edges.sort();
        edges.dedup();

        let cluster_map = Self::cluster_map(&display);
        let seed: [u8; 32] =
            blake3::hash(format!("{}:{}:{}", oid.to_hex(), is_3d, current.snapshot_id).as_bytes())
                .into();
        let layout_plugin = DeterministicForceLayout;
        let coords = layout_plugin
            .compute_layout(&display_nodes, &edges, &seed, is_3d)
            .unwrap_or_default();

        let visual_nodes: Vec<TimeTravelNode> = display
            .into_iter()
            .filter_map(|node| {
                let coord = coords.get(&node.id)?;
                let node_id = node.id.clone();
                Some(TimeTravelNode {
                    id: node.id,
                    x: coord.x,
                    y: coord.y,
                    z: coord.z,
                    cluster_id: *cluster_map.get(&node_id).unwrap_or(&coord.cluster_id),
                    diff_status: node.diff_status,
                    file_path: node.file_path,
                    kind: Some(node.kind),
                    start_line: node.start_line,
                    end_line: node.end_line,
                })
            })
            .collect();

        let visual_edges: Vec<TimeTravelEdge> = edges
            .into_iter()
            .map(|(source, target)| TimeTravelEdge {
                source,
                target,
                kind: "calls".to_string(),
            })
            .collect();

        let full_hex = oid.to_hex();
        let short_oid = full_hex[..7.min(full_hex.len())].to_string();
        let digest_input = serde_json::to_vec(&(
            &full_hex,
            &visual_nodes,
            &visual_edges,
            &added_files,
            &modified_files,
            &deleted_files,
        ))
        .map_err(|error| {
            GitError::Malformed(format!("historical frame encoding failed: {error}"))
        })?;

        Ok(TimeTravelVisualFrame {
            oid: full_hex,
            short_oid,
            summary: commit.summary().to_string(),
            timestamp: commit.committer_timestamp().unwrap_or(0),
            dimensions: if is_3d { "3D" } else { "2D" }.to_string(),
            node_count: visual_nodes.len(),
            edge_count: visual_edges.len(),
            nodes: visual_nodes,
            edges: visual_edges,
            added_files,
            modified_files,
            deleted_files,
            coverage_state: format!("{:?}", current.coverage_state),
            result_digest: blake3::hash(&digest_input).to_hex().to_string(),
        })
    }

    fn file_changes(
        current: &BTreeMap<String, (u32, ObjectId)>,
        previous: &BTreeMap<String, (u32, ObjectId)>,
    ) -> (Vec<String>, Vec<String>, Vec<String>) {
        let mut added = Vec::new();
        let mut modified = Vec::new();
        let mut deleted = Vec::new();
        for (path, (_mode, oid)) in current {
            match previous.get(path) {
                None => added.push(path.clone()),
                Some((_previous_mode, previous_oid)) if previous_oid != oid => {
                    modified.push(path.clone())
                }
                Some(_) => {}
            }
        }
        for path in previous.keys() {
            if !current.contains_key(path) {
                deleted.push(path.clone());
            }
        }
        (added, modified, deleted)
    }

    fn cluster_map(nodes: &[SymbolVisual]) -> BTreeMap<String, u32> {
        let mut folders: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for node in nodes {
            let folder = node
                .file_path
                .as_deref()
                .and_then(|path| path.rsplit_once('/').map(|(folder, _)| folder))
                .unwrap_or("root")
                .to_string();
            folders.entry(folder).or_default().push(node.id.clone());
        }
        let mut clusters = BTreeMap::new();
        for (cluster, (_folder, ids)) in folders.into_iter().enumerate() {
            for id in ids {
                clusters.insert(id, (cluster + 1) as u32);
            }
        }
        clusters
    }
}

struct SymbolVisual {
    id: String,
    file_path: Option<String>,
    kind: String,
    start_line: Option<u32>,
    end_line: Option<u32>,
    diff_status: String,
}

impl SymbolVisual {
    fn from_symbol(symbol: &autopsy_domain::Symbol, diff_status: &str) -> Self {
        Self {
            id: symbol.stable_id.to_string(),
            file_path: symbol.range.as_ref().map(|range| range.path.clone()),
            kind: symbol.kind.clone(),
            start_line: symbol.range.as_ref().map(|range| range.start_line),
            end_line: symbol.range.as_ref().map(|range| range.end_line),
            diff_status: diff_status.to_string(),
        }
    }
}
