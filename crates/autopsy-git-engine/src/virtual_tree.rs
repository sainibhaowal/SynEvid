//! Virtual In-Memory Historical Snapshot & Visual Graph Frame Builder.
//!
//! Complies with FR-P3-002: Reads .git/objects directly from packfiles/loose objects
//! in memory without touching or mutating working-tree files or mtimes on disk.

use crate::model::*;
use autopsy_graph_layout::DeterministicForceLayout;
use autopsy_plugin_api::GraphLayoutPlugin;
use autopsy_temporal::git::{GitError, GitObjectStore, flatten_tree};
use std::collections::BTreeMap;
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

    /// Construct a complete `TimeTravelVisualFrame` for any historical commit.
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

        // Compare with parent commit if available
        let tree_prev = if let Some(parent_oid) = commit.parents.first() {
            if let Ok(p_commit) = store.commit(*parent_oid) {
                flatten_tree(&store, p_commit.tree).unwrap_or_default()
            } else {
                BTreeMap::new()
            }
        } else {
            BTreeMap::new()
        };

        let supported_exts = [
            "ts", "tsx", "js", "jsx", "rs", "py", "go", "java", "cs", "cpp", "c", "json", "toml",
            "yaml", "yml", "md", "sql", "html", "css",
        ];

        let mut added_files = Vec::new();
        let mut modified_files = Vec::new();
        let mut unchanged_files = Vec::new();
        let mut deleted_files = Vec::new();

        let mut filtered_nodes: Vec<String> = Vec::new();
        let mut node_diff_map: BTreeMap<String, String> = BTreeMap::new();

        for (path, (_mode, blob_oid)) in &tree_curr {
            let ext = path.split('.').last().unwrap_or("").to_lowercase();
            if !supported_exts.contains(&ext.as_str()) {
                continue;
            }

            filtered_nodes.push(path.clone());

            match tree_prev.get(path) {
                None => {
                    added_files.push(path.clone());
                    node_diff_map.insert(path.clone(), "added".to_string());
                }
                Some((_p_mode, p_blob_oid)) => {
                    if p_blob_oid != blob_oid {
                        modified_files.push(path.clone());
                        node_diff_map.insert(path.clone(), "modified".to_string());
                    } else {
                        unchanged_files.push(path.clone());
                        node_diff_map.insert(path.clone(), "unchanged".to_string());
                    }
                }
            }
        }

        // Identify deleted ghost nodes from previous commit
        for path in tree_prev.keys() {
            let ext = path.split('.').last().unwrap_or("").to_lowercase();
            if supported_exts.contains(&ext.as_str()) && !tree_curr.contains_key(path) {
                deleted_files.push(path.clone());
            }
        }

        // Cap to 250 nodes for smooth 60 FPS WebGL rendering
        let display_nodes: Vec<String> = filtered_nodes.into_iter().take(250).collect();

        // Build deterministic topology edges & cluster map
        let (edges, cluster_map) = Self::build_topology(&display_nodes);

        // Seeded deterministic layout calculation
        let seed = blake3::hash(format!("{}_virtual_galaxy", commit_hex).as_bytes()).into();
        let layout_plugin = DeterministicForceLayout;
        let coords = layout_plugin
            .compute_layout(&display_nodes, &edges, &seed, is_3d)
            .unwrap_or_default();

        let visual_nodes: Vec<TimeTravelNode> = coords
            .into_iter()
            .map(|(id, coord)| {
                let cluster = *cluster_map.get(&id).unwrap_or(&coord.cluster_id);
                let diff_status = node_diff_map
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| "unchanged".to_string());
                TimeTravelNode {
                    id,
                    x: coord.x,
                    y: coord.y,
                    z: coord.z,
                    cluster_id: cluster,
                    diff_status,
                }
            })
            .collect();

        let visual_edges: Vec<TimeTravelEdge> = edges
            .into_iter()
            .map(|(src, dst)| TimeTravelEdge {
                source: src,
                target: dst,
                kind: "calls".to_string(),
            })
            .collect();

        let full_hex = oid.to_hex();
        let short_oid = full_hex[..7.min(full_hex.len())].to_string();

        Ok(TimeTravelVisualFrame {
            oid: full_hex,
            short_oid,
            summary: commit.summary().to_string(),
            timestamp: commit.committer_timestamp().unwrap_or(0),
            dimensions: if is_3d {
                "3D".to_string()
            } else {
                "2D".to_string()
            },
            node_count: visual_nodes.len(),
            edge_count: visual_edges.len(),
            nodes: visual_nodes,
            edges: visual_edges,
            added_files,
            modified_files,
            deleted_files,
        })
    }

    fn build_topology(nodes: &[String]) -> (Vec<(String, String)>, BTreeMap<String, u32>) {
        let mut edges = Vec::new();
        let mut cluster_map = BTreeMap::new();
        if nodes.is_empty() {
            return (edges, cluster_map);
        }

        let mut folder_groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for node in nodes {
            let folder = if let Some(idx) = node.rfind('/') {
                node[..idx].to_string()
            } else {
                "root".to_string()
            };
            folder_groups.entry(folder).or_default().push(node.clone());
        }

        let mut cluster_id = 1u32;
        for (_folder, members) in folder_groups {
            for m in &members {
                cluster_map.insert(m.clone(), cluster_id);
            }
            if members.len() > 1 {
                for i in 0..members.len() - 1 {
                    edges.push((members[i].clone(), members[i + 1].clone()));
                }
            }
            cluster_id += 1;
        }

        (edges, cluster_map)
    }
}
