//! autopsy-graph: Petgraph-backed typed multigraph with edge provenance (FR-006).
//!
//! Architectural Invariants:
//! - Offline & deterministic: No network calls, no HashMap iteration in canonical output.
//! - Petgraph-backed directed multigraph: multiple edges between identical node pairs supported.
//! - Edge provenance: includes adapter id, location, and extraction method.
//! - Incremental updates without full O(V^2) graph rebuilds.

pub use autopsy_domain::{CoverageState, EdgeKind};
use autopsy_domain::{Edge, SourceLocation, Symbol, SymbolId};
use petgraph::Direction;
use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, EdgeIndex, NodeIndex};
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use thiserror::Error;

/// Errors emitted by graph operations.
#[derive(Debug, Error)]
pub enum GraphError {
    #[error("Symbol not found: {0}")]
    SymbolNotFound(SymbolId),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

/// Explicit provenance metadata for an edge (FR-006).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EdgeProvenance {
    pub adapter: String,
    pub location: Option<SourceLocation>,
    pub method: String,
}

impl EdgeProvenance {
    pub fn new(adapter: impl Into<String>, method: impl Into<String>) -> Self {
        Self {
            adapter: adapter.into(),
            location: None,
            method: method.into(),
        }
    }

    pub fn with_location(mut self, location: SourceLocation) -> Self {
        self.location = Some(location);
        self
    }
}

/// Petgraph-backed typed directed dependency and reference multigraph.
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    /// Internal petgraph directed graph
    inner: DiGraph<Symbol, Edge>,
    /// Fast deterministic node lookup: SymbolId -> NodeIndex
    node_indices: BTreeMap<SymbolId, NodeIndex>,
    /// Index: File path -> Set of SymbolIds declared within the file
    file_symbols: BTreeMap<String, BTreeSet<SymbolId>>,
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl DependencyGraph {
    /// Creates an empty dependency multigraph.
    pub fn new() -> Self {
        Self {
            inner: DiGraph::new(),
            node_indices: BTreeMap::new(),
            file_symbols: BTreeMap::new(),
        }
    }

    /// Constructs a dependency graph from collections of symbols and edges.
    pub fn from_symbols_and_edges(symbols: Vec<Symbol>, edges: Vec<Edge>) -> Self {
        let mut graph = Self::new();
        for sym in symbols {
            graph.add_node(sym);
        }
        for edge in edges {
            graph.add_edge(edge);
        }
        graph
    }

    /// Adds or updates a symbol node in the graph.
    pub fn add_node(&mut self, symbol: Symbol) -> NodeIndex {
        let symbol_id = symbol.stable_id.clone();
        if let Some(&idx) = self.node_indices.get(&symbol_id) {
            // Update node weight in place
            if let Some(path) = symbol.range.as_ref().map(|r| r.path.clone()) {
                self.file_symbols
                    .entry(path)
                    .or_default()
                    .insert(symbol_id.clone());
            }
            self.inner[idx] = symbol;
            idx
        } else {
            let file_path = symbol.range.as_ref().map(|r| r.path.clone());
            let idx = self.inner.add_node(symbol);
            self.node_indices.insert(symbol_id.clone(), idx);
            if let Some(path) = file_path {
                self.file_symbols.entry(path).or_default().insert(symbol_id);
            }
            idx
        }
    }

    /// Retrieves a reference to a symbol by its SymbolId.
    pub fn get_node(&self, id: &SymbolId) -> Option<&Symbol> {
        self.node_indices.get(id).map(|&idx| &self.inner[idx])
    }

    /// Returns a reference to the underlying petgraph directed graph.
    pub fn raw_graph(&self) -> &DiGraph<Symbol, Edge> {
        &self.inner
    }

    /// Retrieves the internal NodeIndex for a SymbolId, if it exists.
    pub fn get_node_index(&self, id: &SymbolId) -> Option<NodeIndex> {
        self.node_indices.get(id).copied()
    }

    /// Checks if a symbol exists in the graph.
    pub fn contains_node(&self, id: &SymbolId) -> bool {
        self.node_indices.contains_key(id)
    }

    /// Returns the total number of symbol nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.inner.node_count()
    }

    /// Returns the total number of typed edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    /// Adds a typed directed edge to the multigraph.
    /// If the source or target nodes do not exist, creates placeholder nodes.
    pub fn add_edge(&mut self, edge: Edge) -> EdgeIndex {
        let src_idx = match self.node_indices.get(&edge.source) {
            Some(&idx) => idx,
            None => {
                let placeholder = Symbol {
                    stable_id: edge.source.clone(),
                    language_id: "unknown".to_string(),
                    kind: "external".to_string(),
                    qualified_name: edge.source.to_string(),
                    range: None,
                    normalized_signature: None,
                    visibility: autopsy_domain::Visibility::Public,
                };
                self.add_node(placeholder)
            }
        };

        let dst_idx = match self.node_indices.get(&edge.target) {
            Some(&idx) => idx,
            None => {
                let placeholder = Symbol {
                    stable_id: edge.target.clone(),
                    language_id: "unknown".to_string(),
                    kind: "external".to_string(),
                    qualified_name: edge.target.to_string(),
                    range: None,
                    normalized_signature: None,
                    visibility: autopsy_domain::Visibility::Public,
                };
                self.add_node(placeholder)
            }
        };

        self.inner.add_edge(src_idx, dst_idx, edge)
    }

    /// Retrieves outgoing dependencies (successors) of a symbol.
    pub fn successors(&self, id: &SymbolId) -> Vec<(&Symbol, &Edge)> {
        let mut results = Vec::new();
        if let Some(&node_idx) = self.node_indices.get(id) {
            for edge_ref in self.inner.edges_directed(node_idx, Direction::Outgoing) {
                let target_node = &self.inner[edge_ref.target()];
                results.push((target_node, edge_ref.weight()));
            }
        }
        // Deterministic sorting
        results.sort_by(|(s1, e1), (s2, e2)| {
            s1.stable_id
                .cmp(&s2.stable_id)
                .then_with(|| e1.kind.cmp(&e2.kind))
                .then_with(|| e1.provenance.cmp(&e2.provenance))
        });
        results
    }

    /// Retrieves incoming dependencies (predecessors) of a symbol.
    pub fn predecessors(&self, id: &SymbolId) -> Vec<(&Symbol, &Edge)> {
        let mut results = Vec::new();
        if let Some(&node_idx) = self.node_indices.get(id) {
            for edge_ref in self.inner.edges_directed(node_idx, Direction::Incoming) {
                let source_node = &self.inner[edge_ref.source()];
                results.push((source_node, edge_ref.weight()));
            }
        }
        // Deterministic sorting
        results.sort_by(|(s1, e1), (s2, e2)| {
            s1.stable_id
                .cmp(&s2.stable_id)
                .then_with(|| e1.kind.cmp(&e2.kind))
                .then_with(|| e1.provenance.cmp(&e2.provenance))
        });
        results
    }

    /// Returns outgoing edges from a symbol.
    pub fn outgoing_edges(&self, id: &SymbolId) -> Vec<&Edge> {
        let mut edges = Vec::new();
        if let Some(&node_idx) = self.node_indices.get(id) {
            for edge_ref in self.inner.edges_directed(node_idx, Direction::Outgoing) {
                edges.push(edge_ref.weight());
            }
        }
        edges.sort_by(|a, b| {
            a.target
                .cmp(&b.target)
                .then_with(|| a.kind.cmp(&b.kind))
                .then_with(|| a.provenance.cmp(&b.provenance))
        });
        edges
    }

    /// Returns incoming edges to a symbol.
    pub fn incoming_edges(&self, id: &SymbolId) -> Vec<&Edge> {
        let mut edges = Vec::new();
        if let Some(&node_idx) = self.node_indices.get(id) {
            for edge_ref in self.inner.edges_directed(node_idx, Direction::Incoming) {
                edges.push(edge_ref.weight());
            }
        }
        edges.sort_by(|a, b| {
            a.source
                .cmp(&b.source)
                .then_with(|| a.kind.cmp(&b.kind))
                .then_with(|| a.provenance.cmp(&b.provenance))
        });
        edges
    }

    /// Returns all edges between two specific symbols.
    pub fn edges_between(&self, src: &SymbolId, dst: &SymbolId) -> Vec<&Edge> {
        let mut matches = Vec::new();
        if let (Some(&src_idx), Some(&dst_idx)) =
            (self.node_indices.get(src), self.node_indices.get(dst))
        {
            for edge_ref in self.inner.edges_directed(src_idx, Direction::Outgoing) {
                if edge_ref.target() == dst_idx {
                    matches.push(edge_ref.weight());
                }
            }
        }
        matches.sort_by(|a, b| {
            a.kind
                .cmp(&b.kind)
                .then_with(|| a.provenance.cmp(&b.provenance))
        });
        matches
    }

    /// Returns all symbols sorted deterministically by SymbolId.
    pub fn all_symbols_sorted(&self) -> Vec<&Symbol> {
        self.node_indices
            .values()
            .map(|&idx| &self.inner[idx])
            .collect()
    }

    /// Returns all edges sorted deterministically.
    pub fn all_edges_sorted(&self) -> Vec<&Edge> {
        let mut edges: Vec<&Edge> = self.inner.edge_weights().collect();
        edges.sort_by(|a, b| {
            a.source
                .cmp(&b.source)
                .then_with(|| a.target.cmp(&b.target))
                .then_with(|| a.kind.cmp(&b.kind))
                .then_with(|| a.provenance.cmp(&b.provenance))
                .then_with(|| a.location.cmp(&b.location))
        });
        edges
    }

    /// Returns all symbols belonging to a specific source file.
    pub fn symbols_in_file(&self, file_path: &str) -> Vec<&Symbol> {
        let mut symbols = Vec::new();
        if let Some(ids) = self.file_symbols.get(file_path) {
            for id in ids {
                if let Some(sym) = self.get_node(id) {
                    symbols.push(sym);
                }
            }
        }
        symbols.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));
        symbols
    }

    /// Removes a symbol and its connected edges from the graph.
    pub fn remove_symbol(&mut self, id: &SymbolId) -> Option<Symbol> {
        if let Some(idx) = self.node_indices.remove(id) {
            let sym = self.inner.remove_node(idx);
            if let Some(sym_ref) = &sym
                && let Some(path) = sym_ref.range.as_ref().map(|r| &r.path)
                && let Some(set) = self.file_symbols.get_mut(path)
            {
                set.remove(id);
            }
            // Re-index remaining nodes because petgraph::remove_node swaps with the last node
            self.rebuild_node_index();
            sym
        } else {
            None
        }
    }

    /// Removes all symbols and edges originating from a file.
    pub fn remove_file(&mut self, file_path: &str) -> Vec<Symbol> {
        let mut removed = Vec::new();
        if let Some(ids) = self.file_symbols.remove(file_path) {
            for id in ids {
                if let Some(idx) = self.node_indices.remove(&id) {
                    if let Some(sym) = self.inner.remove_node(idx) {
                        removed.push(sym);
                    }
                    self.rebuild_node_index();
                }
            }
        }
        removed
    }

    /// Incremental update of a file: replaces its symbols and edges without a full graph rebuild (FR-006).
    pub fn update_file(&mut self, file_path: &str, new_symbols: Vec<Symbol>, new_edges: Vec<Edge>) {
        // 1. Remove old symbols from this file
        self.remove_file(file_path);

        // 2. Add new symbols
        for sym in new_symbols {
            self.add_node(sym);
        }

        // 3. Add new edges
        for edge in new_edges {
            self.add_edge(edge);
        }
    }

    fn rebuild_node_index(&mut self) {
        self.node_indices.clear();
        for idx in self.inner.node_indices() {
            let symbol = &self.inner[idx];
            self.node_indices.insert(symbol.stable_id.clone(), idx);
        }
    }

    /// Computes Strongly Connected Components (SCC) using Tarjan's algorithm.
    /// Returns components sorted deterministically.
    pub fn strongly_connected_components(&self) -> Vec<Vec<SymbolId>> {
        let sccs = tarjan_scc(&self.inner);
        let mut result = Vec::new();
        for component in sccs {
            let mut sym_ids: Vec<SymbolId> = component
                .into_iter()
                .map(|idx| self.inner[idx].stable_id.clone())
                .collect();
            sym_ids.sort();
            result.push(sym_ids);
        }
        // Deterministic component order: sort by first symbol ID
        result.sort_by(|a, b| a.first().cmp(&b.first()));
        result
    }

    /// Finds all cyclic dependency loops in the graph.
    pub fn find_cycles(&self) -> Vec<Vec<SymbolId>> {
        let mut cycles = Vec::new();
        for scc in self.strongly_connected_components() {
            if scc.len() > 1 {
                cycles.push(scc);
            } else if let Some(single_id) = scc.first() {
                // Check for self-loop
                if let Some(&idx) = self.node_indices.get(single_id) {
                    let has_self_loop = self
                        .inner
                        .edges_directed(idx, Direction::Outgoing)
                        .any(|e| e.target() == idx);
                    if has_self_loop {
                        cycles.push(scc);
                    }
                }
            }
        }
        cycles
    }

    /// Bounded forward reachability traversal (BFS).
    /// Returns map of SymbolId -> distance, up to max_depth.
    pub fn bounded_forward_reachability(
        &self,
        start: &SymbolId,
        max_depth: usize,
    ) -> BTreeMap<SymbolId, usize> {
        let mut distances = BTreeMap::new();
        let Some(&start_idx) = self.node_indices.get(start) else {
            return distances;
        };

        let mut queue = VecDeque::new();
        queue.push_back((start_idx, 0));
        distances.insert(start.clone(), 0);

        while let Some((curr_idx, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            let mut next_neighbors = Vec::new();
            for edge_ref in self.inner.edges_directed(curr_idx, Direction::Outgoing) {
                let target_idx = edge_ref.target();
                let target_id = &self.inner[target_idx].stable_id;
                next_neighbors.push((target_idx, target_id.clone()));
            }

            // Deterministic traversal order
            next_neighbors.sort_by(|a, b| a.1.cmp(&b.1));

            for (target_idx, target_id) in next_neighbors {
                if let std::collections::btree_map::Entry::Vacant(e) = distances.entry(target_id) {
                    e.insert(depth + 1);
                    queue.push_back((target_idx, depth + 1));
                }
            }
        }

        distances
    }

    /// Bounded backward reachability traversal (BFS).
    /// Returns map of dependent SymbolId -> distance, up to max_depth.
    pub fn bounded_backward_reachability(
        &self,
        target: &SymbolId,
        max_depth: usize,
    ) -> BTreeMap<SymbolId, usize> {
        let mut distances = BTreeMap::new();
        let Some(&target_idx) = self.node_indices.get(target) else {
            return distances;
        };

        let mut queue = VecDeque::new();
        queue.push_back((target_idx, 0));
        distances.insert(target.clone(), 0);

        while let Some((curr_idx, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            let mut prev_neighbors = Vec::new();
            for edge_ref in self.inner.edges_directed(curr_idx, Direction::Incoming) {
                let source_idx = edge_ref.source();
                let source_id = &self.inner[source_idx].stable_id;
                prev_neighbors.push((source_idx, source_id.clone()));
            }

            // Deterministic traversal order
            prev_neighbors.sort_by(|a, b| a.1.cmp(&b.1));

            for (source_idx, source_id) in prev_neighbors {
                if let std::collections::btree_map::Entry::Vacant(e) = distances.entry(source_id) {
                    e.insert(depth + 1);
                    queue.push_back((source_idx, depth + 1));
                }
            }
        }

        distances
    }

    /// Creates an induced subgraph containing only the specified symbols and their interconnecting edges.
    pub fn subgraph_for_symbols(&self, symbols: &BTreeSet<SymbolId>) -> DependencyGraph {
        let mut sub = DependencyGraph::new();
        for id in symbols {
            if let Some(sym) = self.get_node(id) {
                sub.add_node(sym.clone());
            }
        }
        for id in symbols {
            for edge in self.outgoing_edges(id) {
                if symbols.contains(&edge.target) {
                    sub.add_edge(edge.clone());
                }
            }
        }
        sub
    }

    /// Computes a deterministic BLAKE3 digest of the graph.
    pub fn compute_digest(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        for sym in self.all_symbols_sorted() {
            hasher.update(sym.stable_id.as_str().as_bytes());
            hasher.update(sym.kind.as_bytes());
            hasher.update(sym.qualified_name.as_bytes());
            if let Some(sig) = &sym.normalized_signature {
                hasher.update(sig.as_bytes());
            }
        }
        for edge in self.all_edges_sorted() {
            hasher.update(edge.source.as_str().as_bytes());
            hasher.update(edge.target.as_str().as_bytes());
            hasher.update(edge.kind.to_string().as_bytes());
            hasher.update(edge.provenance.as_bytes());
        }
        hasher.finalize().to_hex().to_string()
    }
}

/// Canonical serializable representation of a DependencyGraph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalGraphRepresentation {
    pub symbols: Vec<Symbol>,
    pub edges: Vec<Edge>,
    pub graph_digest: String,
}

impl Serialize for DependencyGraph {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let rep = CanonicalGraphRepresentation {
            symbols: self.all_symbols_sorted().into_iter().cloned().collect(),
            edges: self.all_edges_sorted().into_iter().cloned().collect(),
            graph_digest: self.compute_digest(),
        };
        rep.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DependencyGraph {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let rep = CanonicalGraphRepresentation::deserialize(deserializer)?;
        Ok(Self::from_symbols_and_edges(rep.symbols, rep.edges))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autopsy_domain::Visibility;

    fn make_symbol(id: &str, file: &str, kind: &str, name: &str) -> Symbol {
        Symbol {
            stable_id: SymbolId::new(id),
            language_id: "typescript".to_string(),
            kind: kind.to_string(),
            qualified_name: name.to_string(),
            range: Some(SourceLocation {
                path: file.to_string(),
                start_line: 1,
                end_line: 10,
                start_col: Some(0),
                end_col: Some(20),
                symbol_id: Some(SymbolId::new(id)),
            }),
            normalized_signature: Some(format!("fn {}() -> void", name)),
            visibility: Visibility::Public,
        }
    }

    fn make_edge(src: &str, dst: &str, kind: EdgeKind, prov: &str) -> Edge {
        Edge {
            source: SymbolId::new(src),
            target: SymbolId::new(dst),
            kind,
            coverage: CoverageState::Verified,
            location: None,
            provenance: prov.to_string(),
        }
    }

    #[test]
    fn test_multigraph_support() {
        let mut graph = DependencyGraph::new();
        let sym_a = make_symbol("sym::A", "src/a.ts", "class", "A");
        let sym_b = make_symbol("sym::B", "src/b.ts", "class", "B");
        graph.add_node(sym_a);
        graph.add_node(sym_b);

        // Add multiple edges between same pair
        graph.add_edge(make_edge(
            "sym::A",
            "sym::B",
            EdgeKind::Imports,
            "import_clause",
        ));
        graph.add_edge(make_edge(
            "sym::A",
            "sym::B",
            EdgeKind::References,
            "type_ref",
        ));
        graph.add_edge(make_edge(
            "sym::A",
            "sym::B",
            EdgeKind::Calls,
            "constructor_call",
        ));

        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 3);

        let edges_ab = graph.edges_between(&SymbolId::new("sym::A"), &SymbolId::new("sym::B"));
        assert_eq!(edges_ab.len(), 3);
    }

    #[test]
    fn test_bounded_reachability_forward_and_backward() {
        let mut graph = DependencyGraph::new();
        // A -> B -> C -> D
        graph.add_node(make_symbol("sym::A", "src/a.ts", "fn", "A"));
        graph.add_node(make_symbol("sym::B", "src/b.ts", "fn", "B"));
        graph.add_node(make_symbol("sym::C", "src/c.ts", "fn", "C"));
        graph.add_node(make_symbol("sym::D", "src/d.ts", "fn", "D"));

        graph.add_edge(make_edge("sym::A", "sym::B", EdgeKind::Calls, "call"));
        graph.add_edge(make_edge("sym::B", "sym::C", EdgeKind::Calls, "call"));
        graph.add_edge(make_edge("sym::C", "sym::D", EdgeKind::Calls, "call"));

        // Depth 2 from A should reach A (0), B (1), C (2) but not D
        let reach = graph.bounded_forward_reachability(&SymbolId::new("sym::A"), 2);
        assert_eq!(reach.len(), 3);
        assert_eq!(reach.get(&SymbolId::new("sym::A")), Some(&0));
        assert_eq!(reach.get(&SymbolId::new("sym::B")), Some(&1));
        assert_eq!(reach.get(&SymbolId::new("sym::C")), Some(&2));
        assert_eq!(reach.get(&SymbolId::new("sym::D")), None);

        // Backward from D with depth 2 should reach D (0), C (1), B (2) but not A
        let back = graph.bounded_backward_reachability(&SymbolId::new("sym::D"), 2);
        assert_eq!(back.len(), 3);
        assert_eq!(back.get(&SymbolId::new("sym::D")), Some(&0));
        assert_eq!(back.get(&SymbolId::new("sym::C")), Some(&1));
        assert_eq!(back.get(&SymbolId::new("sym::B")), Some(&2));
        assert_eq!(back.get(&SymbolId::new("sym::A")), None);
    }

    #[test]
    fn test_cycle_detection_via_tarjan_scc() {
        let mut graph = DependencyGraph::new();
        // Cycle: A -> B -> C -> A
        graph.add_node(make_symbol("sym::A", "src/a.ts", "fn", "A"));
        graph.add_node(make_symbol("sym::B", "src/b.ts", "fn", "B"));
        graph.add_node(make_symbol("sym::C", "src/c.ts", "fn", "C"));
        graph.add_node(make_symbol("sym::X", "src/x.ts", "fn", "X")); // non-cyclic

        graph.add_edge(make_edge("sym::A", "sym::B", EdgeKind::Calls, "call"));
        graph.add_edge(make_edge("sym::B", "sym::C", EdgeKind::Calls, "call"));
        graph.add_edge(make_edge("sym::C", "sym::A", EdgeKind::Calls, "call"));
        graph.add_edge(make_edge("sym::X", "sym::A", EdgeKind::Calls, "call"));

        let cycles = graph.find_cycles();
        assert_eq!(cycles.len(), 1);
        let cycle = &cycles[0];
        assert_eq!(cycle.len(), 3);
        assert!(cycle.contains(&SymbolId::new("sym::A")));
        assert!(cycle.contains(&SymbolId::new("sym::B")));
        assert!(cycle.contains(&SymbolId::new("sym::C")));
    }

    #[test]
    fn test_incremental_file_update() {
        let mut graph = DependencyGraph::new();
        let sym_a1 = make_symbol("sym::A1", "src/a.ts", "fn", "A1");
        let sym_a2 = make_symbol("sym::A2", "src/a.ts", "fn", "A2");
        let sym_b = make_symbol("sym::B", "src/b.ts", "fn", "B");

        graph.add_node(sym_a1);
        graph.add_node(sym_a2);
        graph.add_node(sym_b);
        graph.add_edge(make_edge("sym::A1", "sym::B", EdgeKind::Calls, "call"));

        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 1);

        // Update file src/a.ts with single new symbol A_NEW
        let sym_a_new = make_symbol("sym::A_NEW", "src/a.ts", "fn", "A_NEW");
        let new_edge = make_edge("sym::A_NEW", "sym::B", EdgeKind::Calls, "new_call");
        graph.update_file("src/a.ts", vec![sym_a_new], vec![new_edge]);

        assert_eq!(graph.node_count(), 2);
        assert!(!graph.contains_node(&SymbolId::new("sym::A1")));
        assert!(!graph.contains_node(&SymbolId::new("sym::A2")));
        assert!(graph.contains_node(&SymbolId::new("sym::A_NEW")));
        assert!(graph.contains_node(&SymbolId::new("sym::B")));
        assert_eq!(graph.edge_count(), 1);
    }

    #[test]
    fn test_deterministic_digest_stability() {
        let mut g1 = DependencyGraph::new();
        let mut g2 = DependencyGraph::new();

        let s1 = make_symbol("sym::1", "src/1.ts", "fn", "one");
        let s2 = make_symbol("sym::2", "src/2.ts", "fn", "two");

        // Insert in different order
        g1.add_node(s1.clone());
        g1.add_node(s2.clone());
        g1.add_edge(make_edge("sym::1", "sym::2", EdgeKind::Calls, "call"));

        g2.add_node(s2);
        g2.add_node(s1);
        g2.add_edge(make_edge("sym::1", "sym::2", EdgeKind::Calls, "call"));

        assert_eq!(g1.compute_digest(), g2.compute_digest());
    }
}
