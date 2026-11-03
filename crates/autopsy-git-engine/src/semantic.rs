//! Historical semantic analysis for Phase 3 commit diffs and time travel.
//!
//! A Git tree is materialized into an isolated temporary directory, never
//! into the connected working tree, and is then passed through the same
//! deterministic language adapters used by the normal analyzer. This keeps
//! historical symbol, contract, and edge results honest instead of guessing
//! from filenames or commit messages.

use crate::index::is_safe_index_path;
use autopsy_adapter_api::{LanguageAdapter, ParseContext, ParsedFile};
use autopsy_adapter_csharp::CSharpAdapter;
use autopsy_adapter_go::GoAdapter;
use autopsy_adapter_java::JavaAdapter;
use autopsy_adapter_python::PythonAdapter;
use autopsy_adapter_rust::RustAdapter;
use autopsy_adapter_typescript::TypeScriptAdapter;
use autopsy_domain::{Contract, CoverageState, FileUnit, SnapshotId, Symbol};
use autopsy_graph::DependencyGraph;
use autopsy_repo::{classify_language, is_text_language, normalize_line_endings};
use autopsy_temporal::git::{GitError, GitObjectStore, ObjectId, flatten_tree};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub struct SemanticState {
    pub snapshot_id: SnapshotId,
    pub files: BTreeMap<String, FileUnit>,
    pub graph: DependencyGraph,
    pub contracts: Vec<Contract>,
    pub coverage_state: CoverageState,
}

struct LocalFile {
    parsed: ParsedFile,
    symbols: Vec<Symbol>,
    contracts: Vec<Contract>,
}

pub fn analyze_tree(
    repo_root: &Path,
    store: &GitObjectStore,
    tree_oid: ObjectId,
    snapshot_label: &str,
) -> Result<SemanticState, GitError> {
    let tree_entries = flatten_tree(store, tree_oid)?;
    let temp_root = std::env::temp_dir().join(format!(
        "synevid_phase3_semantic_{}_{}",
        std::process::id(),
        blake3::hash(format!("{}\0{}", repo_root.display(), tree_oid).as_bytes())
            .to_hex()
            .chars()
            .take(16)
            .collect::<String>()
    ));
    if temp_root.exists() {
        fs::remove_dir_all(&temp_root).map_err(|e| GitError::Io {
            path: temp_root.clone(),
            source: e,
        })?;
    }
    fs::create_dir_all(&temp_root).map_err(|e| GitError::Io {
        path: temp_root.clone(),
        source: e,
    })?;

    let result = analyze_materialized_tree(store, &tree_entries, &temp_root, snapshot_label);
    let _ = fs::remove_dir_all(&temp_root);
    result
}

fn analyze_materialized_tree(
    store: &GitObjectStore,
    tree_entries: &BTreeMap<String, (u32, ObjectId)>,
    temp_root: &Path,
    snapshot_label: &str,
) -> Result<SemanticState, GitError> {
    let mut files = BTreeMap::new();
    let mut contents = BTreeMap::new();

    for (path, (_mode, oid)) in tree_entries {
        if !is_safe_index_path(path) {
            return Err(GitError::Malformed(format!(
                "unsafe historical repository path: {path}"
            )));
        }
        let raw = store.blob(*oid)?;
        let language = classify_language(Path::new(path));
        let canonical = if is_text_language(&language) {
            normalize_line_endings(&raw)
        } else {
            raw.clone()
        };
        files.insert(
            path.clone(),
            FileUnit {
                path: path.clone(),
                language: language.clone(),
                content_hash: blake3::hash(&canonical).to_hex().to_string(),
                is_generated: false,
            },
        );

        let full_path = temp_root.join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).map_err(|e| GitError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        fs::write(&full_path, &canonical).map_err(|e| GitError::Io {
            path: full_path,
            source: e,
        })?;
        if let Ok(text) = String::from_utf8(canonical) {
            contents.insert(path.clone(), text);
        }
    }

    let ts = TypeScriptAdapter::with_root_dir(temp_root);
    let py = PythonAdapter::with_root_dir(temp_root);
    let go = GoAdapter::with_root_dir(temp_root);
    let rust = RustAdapter::with_root_dir(temp_root);
    let java = JavaAdapter::with_root_dir(temp_root);
    let csharp = CSharpAdapter::with_root_dir(temp_root);
    let context = ParseContext::new(temp_root);

    let mut locals = BTreeMap::new();
    let mut had_unknown = false;
    let mut had_partial = false;
    for (path, file) in &files {
        let Some(content) = contents.get(path) else {
            had_partial = true;
            continue;
        };
        let Some(adapter) = adapter_for(&file.language, &ts, &py, &go, &rust, &java, &csharp)
        else {
            had_partial = true;
            continue;
        };
        let parsed = match adapter.parse(&context, file, content) {
            Ok(parsed) => parsed,
            Err(_) => {
                had_unknown = true;
                continue;
            }
        };
        match parsed.coverage {
            CoverageState::Unknown => had_unknown = true,
            CoverageState::Partial | CoverageState::Unsupported => had_partial = true,
            CoverageState::Verified => {}
        }
        let symbols = adapter.extract_symbols(&parsed).unwrap_or_default();
        let contracts = adapter.extract_contracts(&parsed).unwrap_or_default();
        locals.insert(
            path.clone(),
            LocalFile {
                parsed,
                symbols,
                contracts,
            },
        );
    }

    let mut symbol_index = BTreeMap::new();
    let mut all_symbols = Vec::new();
    for local in locals.values() {
        for symbol in &local.symbols {
            symbol_index.insert(symbol.qualified_name.clone(), symbol.stable_id.clone());
            all_symbols.push(symbol.clone());
        }
    }
    all_symbols.sort_by(|a, b| a.stable_id.cmp(&b.stable_id));

    let mut graph = DependencyGraph::new();
    for symbol in all_symbols {
        graph.add_node(symbol);
    }
    let mut contracts = Vec::new();
    for local in locals.values() {
        if let Some(file) = files.get(&local.parsed.file.path)
            && let Some(adapter) = adapter_for(&file.language, &ts, &py, &go, &rust, &java, &csharp)
        {
            if let Ok(edges) = adapter.extract_edges(&local.parsed, &symbol_index) {
                let mut sorted_edges = edges;
                sorted_edges.sort();
                for edge in sorted_edges {
                    graph.add_edge(edge);
                }
            }
        }
        contracts.extend(local.contracts.iter().cloned());
    }
    contracts.sort();

    let coverage_state = if had_unknown {
        CoverageState::Unknown
    } else if had_partial {
        CoverageState::Partial
    } else {
        CoverageState::Verified
    };

    Ok(SemanticState {
        snapshot_id: SnapshotId::new(snapshot_label.to_string()),
        files,
        graph,
        contracts,
        coverage_state,
    })
}

fn adapter_for<'a>(
    language: &str,
    ts: &'a TypeScriptAdapter,
    py: &'a PythonAdapter,
    go: &'a GoAdapter,
    rust: &'a RustAdapter,
    java: &'a JavaAdapter,
    csharp: &'a CSharpAdapter,
) -> Option<&'a dyn LanguageAdapter> {
    match language {
        "typescript" | "javascript" => Some(ts),
        "python" => Some(py),
        "go" => Some(go),
        "rust" => Some(rust),
        "java" | "kotlin" => Some(java),
        "csharp" => Some(csharp),
        _ => None,
    }
}

pub fn impacted_boundary(
    graph: &DependencyGraph,
    direct_symbols: &BTreeSet<autopsy_domain::SymbolId>,
) -> BTreeSet<autopsy_domain::SymbolId> {
    let mut boundary = BTreeSet::new();
    for symbol in direct_symbols {
        for (source, _edge) in graph.predecessors(symbol) {
            if !direct_symbols.contains(&source.stable_id) {
                boundary.insert(source.stable_id.clone());
            }
        }
    }
    boundary
}
