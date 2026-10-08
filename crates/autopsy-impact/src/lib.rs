//! autopsy-impact: Deterministic change impact analysis engine (FR-008).
//!
//! Provides bounded forward/backward BFS graph traversal, impact profiling,
//! SCC condensation, deterministic path ranking, and budget truncation tracking.

use autopsy_domain::{EdgeKind, SymbolId};
use autopsy_graph::DependencyGraph;
use petgraph::Direction;
use petgraph::graph::NodeIndex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImpactError {
    #[error("Seed symbol not found in graph: {0}")]
    SeedSymbolNotFound(String),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

/// Traversal direction for impact analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpactDirection {
    /// Follow outgoing edges (callees, dependencies).
    Forward,
    /// Follow incoming edges (callers, dependents).
    Backward,
    /// Follow edges in both directions.
    Bidirectional,
}

/// Configuration profile governing impact traversal limits and filtering.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ImpactProfile {
    pub name: String,
    pub edge_kinds: BTreeSet<EdgeKind>,
    pub max_depth: usize,
    pub budget: usize,
}

impl Default for ImpactProfile {
    fn default() -> Self {
        let mut kinds = BTreeSet::new();
        kinds.insert(EdgeKind::Contains);
        kinds.insert(EdgeKind::Imports);
        kinds.insert(EdgeKind::References);
        kinds.insert(EdgeKind::Calls);
        kinds.insert(EdgeKind::Implements);
        kinds.insert(EdgeKind::Inherits);
        Self {
            name: "default".to_string(),
            edge_kinds: kinds,
            max_depth: 10,
            budget: 1000,
        }
    }
}

/// Query specification for impact analysis.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ImpactQuery {
    pub seed_symbols: Vec<SymbolId>,
    pub direction: ImpactDirection,
    pub profile: ImpactProfile,
}

/// Verifiable result of an impact analysis query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactResult {
    pub query: ImpactQuery,
    pub impacted_entities: Vec<SymbolId>,
    pub paths: Vec<Vec<SymbolId>>,
    pub traversal_profile: String,
    pub truncated: bool,
    pub impact_digest: String,
}

/// Deterministic impact analysis engine.
#[derive(Debug, Default)]
pub struct ImpactEngine;

impl ImpactEngine {
    pub fn new() -> Self {
        Self
    }

    /// Compute bounded transitive impact over specified directions and edge profiles.
    pub fn compute_impact(
        &self,
        graph: &DependencyGraph,
        query: &ImpactQuery,
    ) -> Result<ImpactResult, ImpactError> {
        let raw_graph = graph.raw_graph();
        let mut impacted_nodes: BTreeSet<NodeIndex> = BTreeSet::new();
        let mut shortest_paths: BTreeMap<NodeIndex, Vec<NodeIndex>> = BTreeMap::new();
        let mut truncated = false;

        // Collect strongly connected components for cycle condensation
        let sccs = graph.find_cycles();
        let mut scc_map: BTreeMap<SymbolId, usize> = BTreeMap::new();
        for (idx, scc) in sccs.iter().enumerate() {
            for sym in scc {
                scc_map.insert(sym.clone(), idx);
            }
        }

        // Validate and seed start nodes
        let mut seed_indices = Vec::new();
        for seed in &query.seed_symbols {
            match graph.get_node_index(seed) {
                Some(idx) => seed_indices.push((seed.clone(), idx)),
                None => return Err(ImpactError::SeedSymbolNotFound(seed.to_string())),
            }
        }

        for (_seed_id, seed_idx) in seed_indices {
            // BFS queue: (node_index, current_depth, current_path)
            let mut queue: VecDeque<(NodeIndex, usize, Vec<NodeIndex>)> = VecDeque::new();
            let mut visited_depth: BTreeMap<NodeIndex, usize> = BTreeMap::new();

            queue.push_back((seed_idx, 0, vec![seed_idx]));
            visited_depth.insert(seed_idx, 0);
            impacted_nodes.insert(seed_idx);
            shortest_paths
                .entry(seed_idx)
                .or_insert_with(|| vec![seed_idx]);

            while let Some((curr, depth, path)) = queue.pop_front() {
                if depth >= query.profile.max_depth {
                    // Check if there are unvisited eligible neighbors that we skipped due to max_depth
                    let directions: &[Direction] = match query.direction {
                        ImpactDirection::Forward => &[Direction::Outgoing],
                        ImpactDirection::Backward => &[Direction::Incoming],
                        ImpactDirection::Bidirectional => {
                            &[Direction::Outgoing, Direction::Incoming]
                        }
                    };
                    for &dir in directions {
                        let mut walker = raw_graph.neighbors_directed(curr, dir).detach();
                        while let Some((edge_idx, neighbor)) = walker.next(raw_graph) {
                            let edge_weight = &raw_graph[edge_idx];
                            if query.profile.edge_kinds.contains(&edge_weight.kind)
                                && !visited_depth.contains_key(&neighbor)
                            {
                                truncated = true;
                                break;
                            }
                        }
                    }
                    continue;
                }

                let directions: &[Direction] = match query.direction {
                    ImpactDirection::Forward => &[Direction::Outgoing],
                    ImpactDirection::Backward => &[Direction::Incoming],
                    ImpactDirection::Bidirectional => &[Direction::Outgoing, Direction::Incoming],
                };

                for &dir in directions {
                    let mut walker = raw_graph.neighbors_directed(curr, dir).detach();
                    while let Some((edge_idx, neighbor)) = walker.next(raw_graph) {
                        let edge_weight = &raw_graph[edge_idx];
                        if !query.profile.edge_kinds.contains(&edge_weight.kind) {
                            continue;
                        }

                        let next_depth = depth + 1;
                        let should_explore = match visited_depth.get(&neighbor) {
                            Some(&existing_depth) => next_depth < existing_depth,
                            None => true,
                        };

                        if should_explore {
                            if impacted_nodes.len() >= query.profile.budget
                                && !impacted_nodes.contains(&neighbor)
                            {
                                truncated = true;
                                continue;
                            }

                            visited_depth.insert(neighbor, next_depth);
                            impacted_nodes.insert(neighbor);

                            let mut next_path = path.clone();
                            next_path.push(neighbor);

                            // Store or update shortest path
                            match shortest_paths.get(&neighbor) {
                                Some(existing) => {
                                    if next_path.len() < existing.len() {
                                        shortest_paths.insert(neighbor, next_path.clone());
                                    }
                                }
                                None => {
                                    shortest_paths.insert(neighbor, next_path.clone());
                                }
                            }

                            queue.push_back((neighbor, next_depth, next_path));
                        }
                    }
                }
            }
        }

        // Convert impacted nodes to canonically sorted SymbolIds
        let mut impacted_entities: Vec<SymbolId> = impacted_nodes
            .into_iter()
            .map(|idx| raw_graph[idx].stable_id.clone())
            .collect();
        impacted_entities.sort();

        // Convert and rank paths deterministically
        // Ranking rule: 1. Shortest path length first, 2. Lexical order of SymbolId sequence
        let mut ranked_paths: Vec<Vec<SymbolId>> = shortest_paths
            .into_values()
            .map(|node_path| {
                node_path
                    .into_iter()
                    .map(|idx| raw_graph[idx].stable_id.clone())
                    .collect::<Vec<_>>()
            })
            .collect();

        ranked_paths.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));

        // Compute deterministic BLAKE3 digest over canonical representation
        let canonical_bytes =
            serde_json::to_vec(&(&query, &impacted_entities, &ranked_paths, truncated))?;
        let impact_digest = blake3::hash(&canonical_bytes).to_hex().to_string();

        Ok(ImpactResult {
            query: query.clone(),
            impacted_entities,
            paths: ranked_paths,
            traversal_profile: query.profile.name.clone(),
            truncated,
            impact_digest,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autopsy_domain::{CoverageState, Edge, SourceLocation, Symbol, Visibility};

    fn make_test_symbol(id: &str) -> Symbol {
        Symbol {
            stable_id: SymbolId::new(id),
            language_id: "typescript".to_string(),
            kind: "function".to_string(),
            qualified_name: id.to_string(),
            range: Some(SourceLocation {
                path: "src/index.ts".to_string(),
                start_line: 1,
                end_line: 5,
                start_col: Some(0),
                end_col: Some(10),
                symbol_id: Some(SymbolId::new(id)),
            }),
            normalized_signature: Some(format!("fn {id}()")),
            visibility: Visibility::Public,
        }
    }

    fn make_test_edge(source: &str, target: &str, kind: EdgeKind) -> Edge {
        Edge {
            source: SymbolId::new(source),
            target: SymbolId::new(target),
            kind,
            coverage: CoverageState::Verified,
            location: None,
            provenance: "test".to_string(),
        }
    }

    #[test]
    fn test_forward_and_backward_impact() {
        let mut graph = DependencyGraph::new();
        let sym_a = make_test_symbol("pkg::a");
        let sym_b = make_test_symbol("pkg::b");
        let sym_c = make_test_symbol("pkg::c");

        graph.add_node(sym_a);
        graph.add_node(sym_b);
        graph.add_node(sym_c);

        // A calls B, B calls C
        graph.add_edge(make_test_edge("pkg::a", "pkg::b", EdgeKind::Calls));
        graph.add_edge(make_test_edge("pkg::b", "pkg::c", EdgeKind::Calls));

        let engine = ImpactEngine::new();

        // Forward from A
        let fwd_query = ImpactQuery {
            seed_symbols: vec![SymbolId::new("pkg::a")],
            direction: ImpactDirection::Forward,
            profile: ImpactProfile::default(),
        };
        let fwd_res = engine.compute_impact(&graph, &fwd_query).unwrap();
        assert_eq!(
            fwd_res.impacted_entities,
            vec![
                SymbolId::new("pkg::a"),
                SymbolId::new("pkg::b"),
                SymbolId::new("pkg::c")
            ]
        );
        assert!(!fwd_res.truncated);

        // Backward from C
        let bwd_query = ImpactQuery {
            seed_symbols: vec![SymbolId::new("pkg::c")],
            direction: ImpactDirection::Backward,
            profile: ImpactProfile::default(),
        };
        let bwd_res = engine.compute_impact(&graph, &bwd_query).unwrap();
        assert_eq!(
            bwd_res.impacted_entities,
            vec![
                SymbolId::new("pkg::a"),
                SymbolId::new("pkg::b"),
                SymbolId::new("pkg::c")
            ]
        );
    }

    #[test]
    fn test_budget_truncation() {
        let mut graph = DependencyGraph::new();
        for i in 0..10 {
            graph.add_node(make_test_symbol(&format!("sym::{i}")));
            if i > 0 {
                graph.add_edge(make_test_edge(
                    &format!("sym::{}", i - 1),
                    &format!("sym::{i}"),
                    EdgeKind::Calls,
                ));
            }
        }

        let engine = ImpactEngine::new();
        let profile = ImpactProfile {
            budget: 3,
            ..Default::default()
        };

        let query = ImpactQuery {
            seed_symbols: vec![SymbolId::new("sym::0")],
            direction: ImpactDirection::Forward,
            profile,
        };

        let res = engine.compute_impact(&graph, &query).unwrap();
        assert_eq!(res.impacted_entities.len(), 3);
        assert!(res.truncated);
    }

    #[test]
    fn test_path_ranking_deterministic() {
        let mut graph = DependencyGraph::new();
        graph.add_node(make_test_symbol("a"));
        graph.add_node(make_test_symbol("b"));
        graph.add_node(make_test_symbol("c"));

        graph.add_edge(make_test_edge("a", "b", EdgeKind::Calls));
        graph.add_edge(make_test_edge("b", "c", EdgeKind::Calls));
        graph.add_edge(make_test_edge("a", "c", EdgeKind::Calls));

        let engine = ImpactEngine::new();
        let query = ImpactQuery {
            seed_symbols: vec![SymbolId::new("a")],
            direction: ImpactDirection::Forward,
            profile: ImpactProfile::default(),
        };

        let res = engine.compute_impact(&graph, &query).unwrap();
        // Shortest path to C should be [a, c] (len 2) rather than [a, b, c] (len 3)
        let path_to_c = res
            .paths
            .iter()
            .find(|p| p.last() == Some(&SymbolId::new("c")))
            .unwrap();
        assert_eq!(path_to_c, &vec![SymbolId::new("a"), SymbolId::new("c")]);
    }
}
