//! autopsy-impact: Deterministic change impact analysis engine (FR-008).
//!
//! Provides bounded forward/backward BFS graph traversal, impact profiling,
//! SCC condensation, deterministic path ranking, and budget truncation tracking.

use autopsy_domain::{Edge, EdgeKind, SymbolId};
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

/// Discrete confidence grade of an edge in the dependency multigraph (FR-008, NFR-001).
/// All grades are deterministic integers (parts-per-million, 0..1_000_000).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EdgeGrade {
    /// Unknown or unsupported dynamic construct.
    Unrated = 0,
    /// Unresolved or bare name-level match (e.g. `call_expression:imported-name`).
    NameLevel = 250_000,
    /// Structural container or file transit edge (e.g. `Contains` or `Imports` through module nodes).
    Structural = 500_000,
    /// Inferred or partial coverage match.
    Inferred = 600_000,
    /// Verified resolved call, reference, or member access.
    Resolved = 900_000,
    /// Verified declared relation (e.g. `ast_extends`, `ast_implements`).
    Declared = 1_000_000,
}

impl EdgeGrade {
    pub const fn score(self) -> u32 {
        self as u32
    }

    pub fn from_score(score: u32) -> Self {
        if score >= Self::Declared.score() {
            Self::Declared
        } else if score >= Self::Resolved.score() {
            Self::Resolved
        } else if score >= Self::Inferred.score() {
            Self::Inferred
        } else if score >= Self::Structural.score() {
            Self::Structural
        } else if score >= Self::NameLevel.score() {
            Self::NameLevel
        } else {
            Self::Unrated
        }
    }
}

/// Pure function mapping an edge's kind, coverage, and provenance into an `EdgeGrade`.
pub fn grade_edge(edge: &Edge) -> EdgeGrade {
    use autopsy_domain::CoverageState;

    match edge.coverage {
        CoverageState::Unknown | CoverageState::Unsupported => EdgeGrade::Unrated,
        CoverageState::Partial => {
            if edge.provenance.contains("name") || edge.provenance.contains("unresolved") {
                EdgeGrade::NameLevel
            } else {
                EdgeGrade::Inferred
            }
        }
        CoverageState::Verified => match edge.kind {
            EdgeKind::Contains | EdgeKind::Imports => EdgeGrade::Structural,
            EdgeKind::Inherits | EdgeKind::Implements => EdgeGrade::Declared,
            EdgeKind::StatisticalCoChange => EdgeGrade::Inferred,
            EdgeKind::Calls
            | EdgeKind::References
            | EdgeKind::ReadsSchema
            | EdgeKind::WritesSchema
            | EdgeKind::TestCovers => {
                let prov = edge.provenance.as_str();
                if prov.contains("imported-resolved")
                    || prov.contains("local")
                    || prov.contains("this-method")
                    || prov.contains("typed-receiver")
                    || prov.contains("package-qualified")
                {
                    EdgeGrade::Resolved
                } else if prov.contains("imported-name") || prov.contains("bare-name") {
                    EdgeGrade::NameLevel
                } else {
                    EdgeGrade::Resolved
                }
            }
        },
    }
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
    /// When false, `file::` module nodes are reported but never expanded
    /// (unless they are query seeds): files are structural entry points,
    /// not transit hubs. Suppresses file-graph flooding in exchange for
    /// relying on resolved symbol-level edges. Defaults true (legacy).
    #[serde(default = "default_traverse_file_nodes")]
    pub traverse_file_nodes: bool,
}

/// Legacy default: file nodes expand like any other node.
pub fn default_traverse_file_nodes() -> bool {
    true
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
            traverse_file_nodes: true,
        }
    }
}

/// Caller-focused precision preset: backward traversal over invocation
/// edges with file-node transit suppressed. Answers "who consumes X"
/// without flooding through module scaffolding.
pub fn callers_profile(max_depth: usize, budget: usize) -> ImpactProfile {
    let mut kinds = BTreeSet::new();
    kinds.insert(EdgeKind::Calls);
    kinds.insert(EdgeKind::References);
    ImpactProfile {
        name: "callers".to_string(),
        edge_kinds: kinds,
        max_depth,
        budget,
        traverse_file_nodes: false,
    }
}

/// Query specification for impact analysis.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ImpactQuery {
    pub seed_symbols: Vec<SymbolId>,
    pub direction: ImpactDirection,
    pub profile: ImpactProfile,
}

/// Evidence and distance metadata for an impacted entity (L4 & L5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactEntityMeta {
    pub grade: EdgeGrade,
    pub hops: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weakest_link: Option<String>,
}

/// Verifiable result of an impact analysis query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactResult {
    pub query: ImpactQuery,
    pub impacted_entities: Vec<SymbolId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub likely_affected: Vec<SymbolId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub possibly_affected: Vec<SymbolId>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub entity_meta: BTreeMap<SymbolId, ImpactEntityMeta>,
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
        let mut node_bottlenecks: BTreeMap<NodeIndex, EdgeGrade> = BTreeMap::new();
        let mut node_weakest_link: BTreeMap<NodeIndex, Option<String>> = BTreeMap::new();
        let mut node_hops: BTreeMap<NodeIndex, usize> = BTreeMap::new();
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
        let seed_set: BTreeSet<NodeIndex> = seed_indices.iter().map(|(_, idx)| *idx).collect();

        // File-node transit gate: structural `file::` nodes expand only
        // when they are seeds (or the profile allows transit).
        let expandable = |neighbor: NodeIndex| -> bool {
            if query.profile.traverse_file_nodes || seed_set.contains(&neighbor) {
                return true;
            }
            !raw_graph[neighbor].stable_id.as_str().starts_with("file::")
        };

        for (_seed_id, seed_idx) in seed_indices {
            // BFS queue: (node_index, current_depth, current_path)
            let mut queue: VecDeque<(NodeIndex, usize, Vec<NodeIndex>)> = VecDeque::new();
            let mut visited_depth: BTreeMap<NodeIndex, usize> = BTreeMap::new();

            queue.push_back((seed_idx, 0, vec![seed_idx]));
            visited_depth.insert(seed_idx, 0);
            impacted_nodes.insert(seed_idx);
            node_bottlenecks.insert(seed_idx, EdgeGrade::Declared);
            node_weakest_link.insert(seed_idx, None);
            node_hops.insert(seed_idx, 0);
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
                                && expandable(neighbor)
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

                        let edge_grade = grade_edge(edge_weight);
                        let curr_grade = node_bottlenecks
                            .get(&curr)
                            .copied()
                            .unwrap_or(EdgeGrade::Declared);
                        let cand_bottleneck = curr_grade.min(edge_grade);
                        let cand_weakest = if cand_bottleneck == edge_grade {
                            Some(edge_weight.provenance.clone())
                        } else {
                            node_weakest_link.get(&curr).cloned().flatten()
                        };

                        let entry = node_bottlenecks
                            .entry(neighbor)
                            .or_insert(EdgeGrade::Unrated);
                        if cand_bottleneck > *entry {
                            *entry = cand_bottleneck;
                            node_weakest_link.insert(neighbor, cand_weakest);
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
                            node_hops.entry(neighbor).or_insert(next_depth);

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

                            // Traversal gate: record non-expandable nodes
                            // (e.g. suppressed `file::` hubs) but never
                            // expand through them.
                            if expandable(neighbor) {
                                queue.push_back((neighbor, next_depth, next_path));
                            }
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

        // Build entity metadata and partition into likely vs possibly affected tiers
        let mut entity_meta = BTreeMap::new();
        let mut likely_affected = Vec::new();
        let mut possibly_affected = Vec::new();

        for sym in &impacted_entities {
            let idx_opt = graph.get_node_index(sym);
            let grade = idx_opt
                .and_then(|idx| node_bottlenecks.get(&idx))
                .copied()
                .unwrap_or(EdgeGrade::Unrated);
            let hops = idx_opt
                .and_then(|idx| node_hops.get(&idx))
                .copied()
                .unwrap_or(0);
            let weakest_link = idx_opt
                .and_then(|idx| node_weakest_link.get(&idx))
                .cloned()
                .flatten();

            entity_meta.insert(
                sym.clone(),
                ImpactEntityMeta {
                    grade,
                    hops,
                    weakest_link,
                },
            );

            if grade >= EdgeGrade::Resolved {
                likely_affected.push(sym.clone());
            } else {
                possibly_affected.push(sym.clone());
            }
        }

        // Ranking inside tiers (L4): bottleneck desc -> hop count asc -> stable id asc
        likely_affected.sort_by(|a, b| {
            let meta_a = &entity_meta[a];
            let meta_b = &entity_meta[b];
            meta_b
                .grade
                .cmp(&meta_a.grade)
                .then_with(|| meta_a.hops.cmp(&meta_b.hops))
                .then_with(|| a.cmp(b))
        });

        possibly_affected.sort_by(|a, b| {
            let meta_a = &entity_meta[a];
            let meta_b = &entity_meta[b];
            meta_b
                .grade
                .cmp(&meta_a.grade)
                .then_with(|| meta_a.hops.cmp(&meta_b.hops))
                .then_with(|| a.cmp(b))
        });

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
            likely_affected,
            possibly_affected,
            entity_meta,
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

    #[test]
    fn test_file_node_transit_suppression() {
        // caller --Calls--> callee, callee --Contains(reversed)--> file::m,
        // file::m --Contains--> sibling, file::m --Imports--> file::other.
        let mut graph = DependencyGraph::new();
        for id in [
            "f.ts#fn:caller",
            "f.ts#fn:callee",
            "file::f.ts",
            "f.ts#fn:sibling",
            "file::other.ts",
        ] {
            graph.add_node(make_test_symbol(id));
        }
        graph.add_edge(make_test_edge(
            "f.ts#fn:caller",
            "f.ts#fn:callee",
            EdgeKind::Calls,
        ));
        graph.add_edge(make_test_edge(
            "file::f.ts",
            "f.ts#fn:callee",
            EdgeKind::Contains,
        ));
        graph.add_edge(make_test_edge(
            "file::f.ts",
            "f.ts#fn:sibling",
            EdgeKind::Contains,
        ));
        graph.add_edge(make_test_edge(
            "file::f.ts",
            "file::other.ts",
            EdgeKind::Imports,
        ));

        let engine = ImpactEngine::new();
        let mut kinds = BTreeSet::new();
        kinds.insert(EdgeKind::Calls);
        kinds.insert(EdgeKind::Contains);
        kinds.insert(EdgeKind::Imports);

        // Legacy default: file nodes expand (sibling + other reachable).
        let full_query = ImpactQuery {
            seed_symbols: vec![SymbolId::new("f.ts#fn:callee")],
            direction: ImpactDirection::Bidirectional,
            profile: ImpactProfile::default(),
        };
        let full_res = engine.compute_impact(&graph, &full_query).unwrap();
        assert!(
            full_res
                .impacted_entities
                .contains(&SymbolId::new("f.ts#fn:sibling"))
        );
        assert!(
            full_res
                .impacted_entities
                .contains(&SymbolId::new("file::other.ts"))
        );

        // Suppressed transit: non-seed file nodes are reported, not expanded.
        let suppressed_query = ImpactQuery {
            seed_symbols: vec![SymbolId::new("f.ts#fn:callee")],
            direction: ImpactDirection::Bidirectional,
            profile: ImpactProfile {
                name: "no-file-transit".to_string(),
                edge_kinds: kinds,
                max_depth: 10,
                budget: 1000,
                traverse_file_nodes: false,
            },
        };
        let suppressed_res = engine.compute_impact(&graph, &suppressed_query).unwrap();
        assert!(
            suppressed_res
                .impacted_entities
                .contains(&SymbolId::new("f.ts#fn:caller"))
        );
        assert!(
            suppressed_res
                .impacted_entities
                .contains(&SymbolId::new("file::f.ts"))
        );
        assert!(
            !suppressed_res
                .impacted_entities
                .contains(&SymbolId::new("f.ts#fn:sibling")),
            "sibling must not leak through suppressed file transit: {:?}",
            suppressed_res.impacted_entities
        );
        assert!(
            !suppressed_res
                .impacted_entities
                .contains(&SymbolId::new("file::other.ts")),
            "imported files must not leak through suppressed file transit"
        );
        assert!(!suppressed_res.truncated);
    }

    #[test]
    fn test_file_seed_still_expands_under_suppression() {
        let mut graph = DependencyGraph::new();
        for id in ["file::f.ts", "f.ts#fn:member"] {
            graph.add_node(make_test_symbol(id));
        }
        graph.add_edge(make_test_edge(
            "file::f.ts",
            "f.ts#fn:member",
            EdgeKind::Contains,
        ));

        let engine = ImpactEngine::new();
        let mut profile = callers_profile(10, 1000);
        profile.edge_kinds.insert(EdgeKind::Contains);
        let query = ImpactQuery {
            seed_symbols: vec![SymbolId::new("file::f.ts")],
            direction: ImpactDirection::Forward,
            profile,
        };
        let res = engine.compute_impact(&graph, &query).unwrap();
        assert!(
            res.impacted_entities
                .contains(&SymbolId::new("f.ts#fn:member"))
        );
    }

    #[test]
    fn test_callers_profile_preset_contents() {
        let profile = callers_profile(5, 500);
        assert_eq!(profile.name, "callers");
        assert_eq!(profile.max_depth, 5);
        assert_eq!(profile.budget, 500);
        assert!(profile.edge_kinds.contains(&EdgeKind::Calls));
        assert!(profile.edge_kinds.contains(&EdgeKind::References));
        assert!(!profile.edge_kinds.contains(&EdgeKind::Contains));
        assert!(!profile.traverse_file_nodes);
        // Legacy default is unchanged (full transit, all core kinds).
        assert!(ImpactProfile::default().traverse_file_nodes);
    }

    #[test]
    fn test_edge_grade_mapping() {
        let e_verified_call = Edge {
            source: SymbolId::new("a"),
            target: SymbolId::new("b"),
            kind: EdgeKind::Calls,
            coverage: CoverageState::Verified,
            location: None,
            provenance: "call_expression:imported-resolved".to_string(),
        };
        assert_eq!(grade_edge(&e_verified_call), EdgeGrade::Resolved);

        let e_name_call = Edge {
            source: SymbolId::new("a"),
            target: SymbolId::new("b"),
            kind: EdgeKind::Calls,
            coverage: CoverageState::Partial,
            location: None,
            provenance: "call_expression:imported-name".to_string(),
        };
        assert_eq!(grade_edge(&e_name_call), EdgeGrade::NameLevel);

        let e_contains = Edge {
            source: SymbolId::new("file::a"),
            target: SymbolId::new("fn::b"),
            kind: EdgeKind::Contains,
            coverage: CoverageState::Verified,
            location: None,
            provenance: "ast_contains".to_string(),
        };
        assert_eq!(grade_edge(&e_contains), EdgeGrade::Structural);

        let e_inherits = Edge {
            source: SymbolId::new("class::sub"),
            target: SymbolId::new("class::base"),
            kind: EdgeKind::Inherits,
            coverage: CoverageState::Verified,
            location: None,
            provenance: "ast_extends".to_string(),
        };
        assert_eq!(grade_edge(&e_inherits), EdgeGrade::Declared);
    }

    #[test]
    fn test_likely_vs_possibly_affected_partitioning() {
        let mut graph = DependencyGraph::new();
        graph.add_node(make_test_symbol("seed_fn"));
        graph.add_node(make_test_symbol("resolved_caller"));
        graph.add_node(make_test_symbol("name_guess_caller"));
        graph.add_node(make_test_symbol("file::container"));
        graph.add_node(make_test_symbol("sibling_fn"));

        // 1. resolved_caller --Calls(Verified)--> seed_fn
        let edge_resolved = Edge {
            source: SymbolId::new("resolved_caller"),
            target: SymbolId::new("seed_fn"),
            kind: EdgeKind::Calls,
            coverage: CoverageState::Verified,
            location: None,
            provenance: "call_expression:imported-resolved".to_string(),
        };
        graph.add_edge(edge_resolved);

        // 2. name_guess_caller --Calls(Partial)--> seed_fn
        let edge_guess = Edge {
            source: SymbolId::new("name_guess_caller"),
            target: SymbolId::new("seed_fn"),
            kind: EdgeKind::Calls,
            coverage: CoverageState::Partial,
            location: None,
            provenance: "call_expression:imported-name".to_string(),
        };
        graph.add_edge(edge_guess);

        // 3. file::container --Contains--> seed_fn and file::container --Contains--> sibling_fn
        graph.add_edge(make_test_edge(
            "file::container",
            "seed_fn",
            EdgeKind::Contains,
        ));
        graph.add_edge(make_test_edge(
            "file::container",
            "sibling_fn",
            EdgeKind::Contains,
        ));

        let engine = ImpactEngine::new();
        let query = ImpactQuery {
            seed_symbols: vec![SymbolId::new("seed_fn")],
            direction: ImpactDirection::Bidirectional,
            profile: ImpactProfile::default(),
        };

        let res = engine.compute_impact(&graph, &query).unwrap();

        // seed_fn itself and resolved_caller should be likely_affected
        assert!(res.likely_affected.contains(&SymbolId::new("seed_fn")));
        assert!(
            res.likely_affected
                .contains(&SymbolId::new("resolved_caller"))
        );

        // name_guess_caller, file::container, and sibling_fn should be possibly_affected
        assert!(
            res.possibly_affected
                .contains(&SymbolId::new("name_guess_caller"))
        );
        assert!(
            res.possibly_affected
                .contains(&SymbolId::new("file::container"))
        );
        assert!(res.possibly_affected.contains(&SymbolId::new("sibling_fn")));

        // Disjoint and exhaustive partition
        assert_eq!(
            res.likely_affected.len() + res.possibly_affected.len(),
            res.impacted_entities.len()
        );
    }
}
