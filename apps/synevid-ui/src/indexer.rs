use autopsy_cli::{
    AnalysisArtifacts, build_snapshot_and_graph_with_registry, load_or_default_config,
};
use autopsy_domain::{CoverageState, Edge, RepoSnapshot, Symbol};
use autopsy_plugin_api::{PluginRegistry, StorageEngineConfig};
use autopsy_repo::{is_text_language, normalize_line_endings};
use autopsy_storage::{GraphEdgeRow, GraphNodeRow, RepoRecord, StorageEngine};
use autopsy_workspace_registry::MonorepoWorkspace;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IndexReceipt {
    pub snapshot_id: String,
    pub files_scanned: usize,
    pub symbol_count: usize,
    pub edge_count: usize,
    pub coverage_state: String,
    pub languages: Vec<String>,
    pub skipped_oversize_files: Vec<String>,
    pub traversal_limit_reached: bool,
}

#[cfg(test)]
pub fn open_repository_storage(root: &Path) -> Result<StorageEngine, String> {
    let registry = autopsy_cli::builtin_plugin_registry(root);
    open_repository_storage_with_registry(root, &registry)
}

pub fn open_repository_storage_with_registry(
    root: &Path,
    registry: &PluginRegistry,
) -> Result<StorageEngine, String> {
    let autopsy_dir = root.join(".autopsy");
    autopsy_cli::open_storage_engine_with_registry(
        registry,
        &StorageEngineConfig {
            database_path: autopsy_dir.join("storage.db"),
            use_wal: true,
            cache_dir: Some(autopsy_dir.join("cache")),
            recover_corruption: true,
        },
    )
    .map_err(|error| error.to_string())
}

pub fn index_repository_with_registry(
    root: &Path,
    repo_id: &str,
    display_name: &str,
    storage: &StorageEngine,
    registry: &PluginRegistry,
) -> Result<(RepoRecord, IndexReceipt), String> {
    let (record, receipt, _) = index_repository_with_artifacts_with_registry(
        root,
        repo_id,
        display_name,
        storage,
        registry,
    )?;
    Ok((record, receipt))
}

pub fn index_repository_with_artifacts_with_registry(
    root: &Path,
    repo_id: &str,
    display_name: &str,
    storage: &StorageEngine,
    registry: &PluginRegistry,
) -> Result<(RepoRecord, IndexReceipt, AnalysisArtifacts), String> {
    let config = load_or_default_config(root).map_err(|error| error.to_string())?;
    let artifacts =
        build_snapshot_and_graph_with_registry(root, &config, None, Some(storage), registry)
            .map_err(|error| error.to_string())?;

    persist_snapshot_contents(
        root,
        &artifacts.snapshot,
        config.analysis.max_file_bytes,
        storage,
    )?;
    let (nodes, edges) = graph_rows(&artifacts);
    storage
        .save_snapshot(&artifacts.snapshot)
        .map_err(|error| error.to_string())?;
    storage
        .save_graph_nodes_and_edges(&nodes, &edges)
        .map_err(|error| error.to_string())?;

    let languages = artifacts
        .snapshot
        .files
        .values()
        .map(|unit| unit.language.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let workspace = MonorepoWorkspace::detect(root);
    let resolver_matches = registry.detect_workspaces(root);
    let monorepo_json = serde_json::json!({
        "workspace": workspace,
        "resolver_plugins": resolver_matches
    })
    .to_string();
    let (active_branch, head_commit_oid) = git_head(root);
    let last_indexed_seq = storage
        .get_repository(repo_id)
        .map_err(|error| error.to_string())?
        .map(|record| record.last_indexed_seq.saturating_add(1))
        .unwrap_or(1);
    let root_path = canonical_root_string(root)?;
    let record = RepoRecord {
        repo_id: repo_id.to_string(),
        root_path,
        display_name: display_name.to_string(),
        active_branch,
        head_commit_oid,
        latest_snap_id: Some(artifacts.snapshot.snapshot_id.to_string()),
        languages_json: serde_json::to_string(&languages).map_err(|error| error.to_string())?,
        monorepo_json,
        last_indexed_seq,
    };
    storage
        .register_repository(&record)
        .map_err(|error| error.to_string())?;

    let receipt = IndexReceipt {
        snapshot_id: artifacts.snapshot.snapshot_id.to_string(),
        files_scanned: artifacts.snapshot.files.len(),
        symbol_count: artifacts.symbols.len(),
        edge_count: artifacts.edges.len(),
        coverage_state: coverage_name(artifacts.coverage_state),
        languages,
        skipped_oversize_files: artifacts.snapshot.skipped_oversize_files.clone(),
        traversal_limit_reached: artifacts.snapshot.traversal_limit_reached,
    };
    Ok((record, receipt, artifacts))
}

pub fn snapshot_for_repository(
    root: &Path,
    record: &RepoRecord,
    storage: &StorageEngine,
    registry: &PluginRegistry,
) -> Result<Option<RepoSnapshot>, String> {
    let Some(snapshot_id) = &record.latest_snap_id else {
        return Ok(None);
    };
    storage
        .get_snapshot(&autopsy_domain::SnapshotId::new(snapshot_id))
        .map_err(|error| error.to_string())
        .and_then(|snapshot| {
            if snapshot.is_none() {
                let local = open_repository_storage_with_registry(root, registry)?;
                local
                    .get_snapshot(&autopsy_domain::SnapshotId::new(snapshot_id))
                    .map_err(|error| error.to_string())
            } else {
                Ok(snapshot)
            }
        })
}

fn canonical_root_string(root: &Path) -> Result<String, String> {
    root.canonicalize()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .map_err(|error| error.to_string())
}

fn persist_snapshot_contents(
    root: &Path,
    snapshot: &RepoSnapshot,
    max_file_bytes: u64,
    storage: &StorageEngine,
) -> Result<(), String> {
    let canonical_root = root.canonicalize().map_err(|error| error.to_string())?;
    for (relative_path, unit) in &snapshot.files {
        if !autopsy_repo::is_safe_relative_path(relative_path)
            || relative_path
                .split('/')
                .any(|segment| segment == ".git" || segment == ".autopsy")
        {
            return Err(format!("unsafe snapshot path: {relative_path}"));
        }
        let path = root.join(relative_path);
        let mut checked_path = root.to_path_buf();
        for segment in relative_path.split('/') {
            checked_path.push(segment);
            let component = fs::symlink_metadata(&checked_path)
                .map_err(|error| format!("{}: {error}", checked_path.display()))?;
            if component.file_type().is_symlink() {
                return Err(format!(
                    "snapshot path contains a symlink during indexing: {relative_path}"
                ));
            }
        }
        let canonical_path = path
            .canonicalize()
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if !canonical_path.starts_with(&canonical_root) {
            return Err(format!(
                "snapshot path escapes repository root: {relative_path}"
            ));
        }
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "snapshot path became a symlink during indexing: {relative_path}"
            ));
        }
        if metadata.len() > max_file_bytes {
            return Err(format!(
                "snapshot file exceeded max_file_bytes during indexing: {relative_path}"
            ));
        }
        let raw = fs::read(&canonical_path)
            .map_err(|error| format!("{}: {error}", canonical_path.display()))?;
        if raw.len() as u64 > max_file_bytes {
            return Err(format!(
                "snapshot file exceeded max_file_bytes during indexing: {relative_path}"
            ));
        }
        let canonical = if is_text_language(&unit.language) {
            normalize_line_endings(&raw)
        } else {
            raw
        };
        let digest = blake3::hash(&canonical).to_hex().to_string();
        if digest != unit.content_hash {
            return Err(format!("repository changed during indexing: {}", unit.path));
        }
        storage
            .put_cache_object(&canonical)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn graph_rows(artifacts: &AnalysisArtifacts) -> (Vec<GraphNodeRow>, Vec<GraphEdgeRow>) {
    let snapshot_id = artifacts.snapshot.snapshot_id.to_string();
    let overall_coverage = coverage_name(artifacts.coverage_state);
    let mut nodes = BTreeMap::new();
    let mut symbols = BTreeMap::<String, &Symbol>::new();

    for (path, file) in &artifacts.snapshot.files {
        let stable_id = file_node_id(path);
        let coverage = if is_analyzed_language(&file.language) {
            overall_coverage.clone()
        } else {
            "unknown".to_string()
        };
        nodes.insert(
            stable_id.clone(),
            GraphNodeRow {
                snapshot_id: snapshot_id.clone(),
                stable_id,
                node_kind: "file".to_string(),
                qualified_name: path.clone(),
                file_path: path.clone(),
                start_line: 1,
                end_line: 1,
                signature_hash: file.content_hash.clone(),
                visibility: "unknown".to_string(),
                is_exported: false,
                is_dead_code: false,
                coverage_state: coverage,
            },
        );
    }

    for symbol in &artifacts.symbols {
        let stable_id = symbol.stable_id.to_string();
        symbols.insert(stable_id.clone(), symbol);
        let location = symbol.range.as_ref();
        let file_path = location.map(|range| range.path.clone()).unwrap_or_default();
        let signature_hash = symbol
            .normalized_signature
            .as_deref()
            .map(|signature| blake3::hash(signature.as_bytes()).to_hex().to_string())
            .unwrap_or_default();
        nodes.insert(
            stable_id.clone(),
            GraphNodeRow {
                snapshot_id: snapshot_id.clone(),
                stable_id,
                node_kind: symbol.kind.clone(),
                qualified_name: symbol.qualified_name.clone(),
                file_path,
                start_line: location
                    .map(|range| i64::from(range.start_line))
                    .unwrap_or(0),
                end_line: location.map(|range| i64::from(range.end_line)).unwrap_or(0),
                signature_hash,
                visibility: format!("{:?}", symbol.visibility).to_lowercase(),
                is_exported: symbol.visibility == autopsy_domain::Visibility::Public,
                is_dead_code: false,
                coverage_state: overall_coverage.clone(),
            },
        );
    }

    for edge in &artifacts.edges {
        let source_id = edge.source.to_string();
        let target_id = edge.target.to_string();
        let source_symbol = symbols.get(&source_id).copied();
        let source_location = edge.location.as_ref();
        let source_file = source_location
            .map(|location| location.path.clone())
            .or_else(|| {
                source_symbol.and_then(|symbol| symbol.range.as_ref().map(|r| r.path.clone()))
            })
            .unwrap_or_default();
        nodes.entry(source_id.clone()).or_insert_with(|| {
            unknown_node(&snapshot_id, &source_id, &source_file, &overall_coverage)
        });
        let target_symbol = symbols.get(&target_id).copied();
        let target_file = target_symbol
            .and_then(|symbol| symbol.range.as_ref().map(|range| range.path.clone()))
            .unwrap_or_default();
        nodes.entry(target_id.clone()).or_insert_with(|| {
            unknown_node(&snapshot_id, &target_id, &target_file, &overall_coverage)
        });
    }

    let mut edges = artifacts
        .edges
        .iter()
        .map(|edge| to_graph_edge_row(edge, &snapshot_id, &symbols, &overall_coverage))
        .collect::<Vec<_>>();
    for symbol in &artifacts.symbols {
        let Some(location) = symbol.range.as_ref() else {
            continue;
        };
        let source_id = file_node_id(&location.path);
        let target_id = symbol.stable_id.to_string();
        let edge_id = edge_digest(
            &snapshot_id,
            &source_id,
            &target_id,
            "contains",
            &location.path,
            1,
            0,
            "snapshot_manifest_contains",
        );
        edges.push(GraphEdgeRow {
            snapshot_id: snapshot_id.clone(),
            edge_id,
            source_stable_id: source_id,
            target_stable_id: target_id,
            edge_kind: "contains".to_string(),
            source_file: location.path.clone(),
            source_line: 1,
            source_col: 0,
            target_file: location.path.clone(),
            target_line: i64::from(location.start_line),
            confidence: "exact".to_string(),
        });
    }
    edges.sort_by(|left, right| left.edge_id.cmp(&right.edge_id));
    (nodes.into_values().collect(), edges)
}

fn to_graph_edge_row(
    edge: &Edge,
    snapshot_id: &str,
    symbols: &BTreeMap<String, &Symbol>,
    _overall_coverage: &str,
) -> GraphEdgeRow {
    let source_id = edge.source.to_string();
    let target_id = edge.target.to_string();
    let source_symbol = symbols.get(&source_id).copied();
    let target_symbol = symbols.get(&target_id).copied();
    let source_location = edge.location.as_ref();
    let source_file = source_location
        .map(|location| location.path.clone())
        .or_else(|| source_symbol.and_then(|symbol| symbol.range.as_ref().map(|r| r.path.clone())))
        .unwrap_or_default();
    let target_location = target_symbol.and_then(|symbol| symbol.range.as_ref());
    let target_file = target_location
        .map(|location| location.path.clone())
        .unwrap_or_default();
    let source_line = source_location
        .map(|location| i64::from(location.start_line))
        .unwrap_or(0);
    let source_col = source_location
        .and_then(|location| location.start_col)
        .map(i64::from)
        .unwrap_or(0);
    let target_line = target_location
        .map(|location| i64::from(location.start_line))
        .unwrap_or(0);
    let kind = edge.kind.to_string();
    let edge_id = edge_digest(
        snapshot_id,
        &source_id,
        &target_id,
        &kind,
        &source_file,
        source_line,
        source_col,
        &edge.provenance,
    );
    let confidence = match edge.coverage {
        CoverageState::Verified => "exact",
        CoverageState::Partial => "heuristic",
        CoverageState::Unsupported | CoverageState::Unknown => "dynamic_unknown",
    };
    GraphEdgeRow {
        snapshot_id: snapshot_id.to_string(),
        edge_id,
        source_stable_id: source_id,
        target_stable_id: target_id,
        edge_kind: kind,
        source_file,
        source_line,
        source_col,
        target_file,
        target_line,
        confidence: confidence.to_string(),
    }
}

fn edge_digest(
    snapshot_id: &str,
    source_id: &str,
    target_id: &str,
    kind: &str,
    source_file: &str,
    source_line: i64,
    source_col: i64,
    provenance: &str,
) -> String {
    let canonical = format!(
        "{snapshot_id}\0{source_id}\0{target_id}\0{kind}\0{source_file}\0{source_line}\0{source_col}\0{provenance}"
    );
    blake3::hash(canonical.as_bytes()).to_hex().to_string()
}

fn unknown_node(snapshot_id: &str, stable_id: &str, path: &str, coverage: &str) -> GraphNodeRow {
    GraphNodeRow {
        snapshot_id: snapshot_id.to_string(),
        stable_id: stable_id.to_string(),
        node_kind: "unknown".to_string(),
        qualified_name: stable_id.to_string(),
        file_path: path.to_string(),
        start_line: 0,
        end_line: 0,
        signature_hash: String::new(),
        visibility: "unknown".to_string(),
        is_exported: false,
        is_dead_code: false,
        coverage_state: if coverage == "verified" {
            "unknown".to_string()
        } else {
            coverage.to_string()
        },
    }
}

pub fn file_node_id(path: &str) -> String {
    format!("file:{path}")
}

pub fn coverage_name(coverage: CoverageState) -> String {
    match coverage {
        CoverageState::Verified => "verified".to_string(),
        CoverageState::Partial => "partial".to_string(),
        CoverageState::Unsupported => "unknown".to_string(),
        CoverageState::Unknown => "unknown".to_string(),
    }
}

pub fn is_indexed_language(language: &str) -> bool {
    matches!(
        language,
        "typescript" | "javascript" | "python" | "go" | "rust" | "java" | "kotlin" | "csharp"
    )
}

fn is_analyzed_language(language: &str) -> bool {
    is_indexed_language(language)
}

pub fn git_head(root: &Path) -> (Option<String>, Option<String>) {
    let git_path = root.join(".git");
    let git_dir = if git_path.is_dir() {
        git_path
    } else if let Ok(contents) = fs::read_to_string(&git_path) {
        let Some(target) = contents.trim().strip_prefix("gitdir:") else {
            return (None, None);
        };
        let target = PathBuf::from(target.trim());
        if target.is_absolute() {
            target
        } else {
            root.join(target)
        }
    } else {
        return (None, None);
    };
    let Ok(git_dir) = git_dir.canonicalize() else {
        return (None, None);
    };
    let Ok(head) = fs::read_to_string(git_dir.join("HEAD")) else {
        return (None, None);
    };
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref: refs/heads/") {
        if reference.is_empty()
            || reference.split('/').any(|segment| {
                segment.is_empty() || segment == "." || segment == ".." || segment.contains('\\')
            })
        {
            return (None, None);
        }
        let loose_ref = git_dir.join("refs/heads").join(reference);
        let direct = loose_ref
            .canonicalize()
            .ok()
            .filter(|path| path.starts_with(&git_dir))
            .and_then(|path| fs::read_to_string(path).ok())
            .map(|value| value.trim().to_string())
            .filter(|oid| valid_git_oid(oid));
        let commit = direct.or_else(|| {
            let packed_path = git_dir.join("packed-refs");
            if fs::metadata(&packed_path).ok()?.len() > 4 * 1024 * 1024 {
                return None;
            }
            let packed_refs = fs::read_to_string(packed_path).ok()?;
            packed_refs.lines().find_map(|line| {
                let (oid, ref_name) = line.split_once(' ')?;
                (ref_name == format!("refs/heads/{reference}") && valid_git_oid(oid))
                    .then(|| oid.to_string())
            })
        });
        (Some(reference.to_string()), commit)
    } else if head.is_empty() {
        (None, None)
    } else if valid_git_oid(head) {
        (None, Some(head.to_string()))
    } else {
        (None, None)
    }
}

fn valid_git_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
