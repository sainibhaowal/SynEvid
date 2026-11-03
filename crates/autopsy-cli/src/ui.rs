//! Reactive Daemon server & REST API (autopsy ui).
//!
//! Serves the headless REST and SSE API for the Synevid Web Cockpit, Electron App,
//! TUI, and IDE/MCP bridges directly from memory.
//! Zero external CDNs or network calls. Deterministic offline engine.

#![allow(clippy::collapsible_if)]

use crate::{
    ANALYZER_VERSION, Cli, CliError, EXIT_PASS, OutputFormat, StorageEngineConfig,
    build_snapshot_and_graph, builtin_plugin_registry, load_or_default_config,
    open_storage_engine_with_registry,
};
use autopsy_domain::{CoverageState, FindingStatus, SymbolId};
use autopsy_graph_layout::DeterministicForceLayout;
use autopsy_impact::{ImpactDirection, ImpactEngine, ImpactProfile, ImpactQuery};
use autopsy_invariants::{EvaluationContext, InvariantEvaluator, InvariantsConfig};
use autopsy_plugin_api::{GraphLayoutPlugin, PluginRegistry};
use autopsy_storage::{RepoRecord, StorageEngine};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Cursor;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

#[derive(Debug, Deserialize)]
pub struct ConnectRepoRequest {
    pub root_path: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SwitchRepoRequest {
    pub repo_id: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct SimulateEditRequest {
    pub repo_id: Option<String>,
    pub file_path: String,
    pub unsaved_buffer: String,
    pub baseline_snapshot_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DamagedCallerItem {
    pub stable_id: String,
    pub file_path: String,
    pub calling_line: i64,
    pub calling_col: i64,
    pub diagnostic: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadSymbolItem {
    pub stable_id: String,
    pub file_path: String,
    pub line: i64,
    pub diagnostic: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SimulateEditResponse {
    pub verdict: String, // PASS | WARN | FAIL
    pub damaged_callers: Vec<DamagedCallerItem>,
    pub newly_dead_symbols: Vec<DeadSymbolItem>,
    pub broken_imports: Vec<String>,
    pub syntax_errors: Vec<String>,
    pub computation_latency_ms: u64,
}

pub fn handle_ui(cli: &Cli, port: u16, host: &str, open: bool) -> Result<i32, CliError> {
    let url = format!("http://{}:{}", host, port);

    if cli.format == OutputFormat::Json {
        let doc = serde_json::json!({
            "command": "ui",
            "status": "ready",
            "url": url,
            "port": port,
            "host": host,
            "open_browser": open,
            "repo_root": cli.repo_root.display().to_string(),
            "instructions": format!("Open {} in your web browser.", url)
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&doc).map_err(|e| CliError::Analysis(e.to_string()))?
        );
        return Ok(EXIT_PASS);
    }

    let addr: SocketAddr = format!("{}:{}", host, port)
        .parse()
        .map_err(|e| CliError::Config(format!("Invalid host/port '{}:{}': {}", host, port, e)))?;

    let server = Server::http(addr).map_err(|e| {
        CliError::Analysis(format!(
            "Failed to bind embedded HTTP server on {}: {}",
            addr, e
        ))
    })?;

    println!(
        "╔════════════════════════════════════════════════════════════════════════════════════╗"
    );
    println!(
        "║  SynEvid Embedded Web Dashboard & Reactive Daemon (v0.2.0)                         ║"
    );
    println!(
        "╚════════════════════════════════════════════════════════════════════════════════════╝"
    );
    println!();
    println!("  Listening on: http://{}", addr);
    println!("  Repository  : {}", cli.repo_root.display());
    println!("  Sandboxing  : 100% Offline (Zero external CDNs, Zero outbound network)");
    println!("  Persistence : Local SQLite WAL (.autopsy/storage.db)");
    println!();
    if open {
        println!(
            "  To view the dashboard, open http://{} in your web browser.",
            addr
        );
    } else {
        println!("  Open http://{} in your web browser.", addr);
    }
    println!("  Press Ctrl+C to stop.");
    println!();

    let root_canon = cli
        .repo_root
        .canonicalize()
        .unwrap_or_else(|_| cli.repo_root.clone());
    let repo_id = blake3::hash(root_canon.to_string_lossy().as_bytes())
        .to_hex()
        .to_string();

    let autopsy_dir = cli.repo_root.join(".autopsy");
    let registry = builtin_plugin_registry(&root_canon);
    let storage = open_storage_engine_with_registry(
        &registry,
        &StorageEngineConfig {
            database_path: autopsy_dir.join("storage.db"),
            cache_dir: Some(autopsy_dir.join("cache")),
            use_wal: true,
            recover_corruption: true,
        },
    )
    .ok();

    let display_name = root_canon
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty() && *n != ".")
        .unwrap_or("SynEvid")
        .to_string();

    if let Some(ref st) = storage {
        let repo = RepoRecord {
            repo_id: repo_id.clone(),
            root_path: root_canon.to_string_lossy().to_string(),
            display_name,
            // Registration is metadata only. Do not invent a branch,
            // snapshot, language inventory, or index sequence before the
            // repository has actually been analyzed.
            active_branch: None,
            head_commit_oid: None,
            latest_snap_id: None,
            languages_json: "[]".to_string(),
            monorepo_json: "{}".to_string(),
            last_indexed_seq: 0,
        };
        let _ = st.register_repository(&repo);
    }

    let storage_arc = Arc::new(Mutex::new(storage));
    let active_repo_id = Arc::new(Mutex::new(repo_id));
    let registry = Arc::new(PluginRegistry::new());
    registry.register_layout(Arc::new(DeterministicForceLayout));

    for mut request in server.incoming_requests() {
        let req_url = request.url().to_string();
        let path = req_url.split('?').next().unwrap_or("/");

        match (request.method(), path) {
            (&Method::Get, "/") | (&Method::Get, "/index.html") => {
                let active_id = active_repo_id.lock().unwrap().clone();
                let body = serde_json::json!({
                    "daemon": "autopsy-ui",
                    "status": "online",
                    "version": ANALYZER_VERSION,
                    "engine": "offline-deterministic",
                    "repo_root": cli.repo_root.display().to_string(),
                    "active_repo_id": active_id,
                    "cockpit_url": "http://localhost:3000",
                    "mcp_bridge": "active",
                    "api_base": "/api/v1"
                })
                .to_string();
                let header = Header::from_bytes(
                    &b"Content-Type"[..],
                    &b"application/json; charset=utf-8"[..],
                )
                .unwrap();
                let response = Response::from_string(body).with_header(header);
                let _ = request.respond(response);
            }

            // Phase 1 Capabilities Endpoint (FR-P1-001)
            (&Method::Get, "/api/v1/capabilities") => {
                let caps = registry.list_capabilities();
                respond_json(request, &serde_json::to_string(&caps).unwrap_or_default());
            }

            // Phase 1 Multi-Repo Connection & Switcher (FR-P1-003)
            (&Method::Post, "/api/v1/repos/connect") => {
                let body = read_body(&mut request);
                let res = handle_connect_repo(&body, &storage_arc, &active_repo_id);
                respond_json(request, &res);
            }
            (&Method::Get, "/api/v1/repos/list") => {
                let res = handle_list_repos(&storage_arc);
                respond_json(request, &res);
            }
            (&Method::Post, "/api/v1/repos/switch") => {
                let body = read_body(&mut request);
                let res = handle_switch_repo(&body, &storage_arc, &active_repo_id);
                respond_json(request, &res);
            }

            // Phase 1 Local Filesystem Navigator & Directory Picker (FR-P1-007)
            (&Method::Get, "/api/v1/fs/browse") => {
                let res = handle_fs_browse(&req_url, &cli.repo_root);
                respond_json(request, &res);
            }

            // Real-Time Repository Discovered Files
            (&Method::Get, "/api/v1/files") | (&Method::Get, "/api/files") => {
                let res = handle_files(&active_repo_id, &cli.repo_root, &storage_arc);
                respond_json(request, &res);
            }

            // Phase 1 Visual Graph Universe (2D/3D with seeded layout) (FR-P1-006)
            (&Method::Get, "/api/v1/graph/universe") => {
                let res =
                    handle_graph_universe(&req_url, &active_repo_id, &cli.repo_root, &storage_arc);
                respond_json(request, &res);
            }

            // Phase 1 Single-File/Symbol Ego Network (<5ms) (FR-P1-002)
            (&Method::Get, "/api/v1/graph/ego") => {
                let res = handle_graph_ego(&req_url, &storage_arc);
                respond_json(request, &res);
            }

            // Phase 1 In-Memory Live Simulation (<50ms) (FR-P1-004)
            (&Method::Post, "/api/v1/simulate/edit") => {
                let body = read_body(&mut request);
                let res = handle_simulate_edit(&body, &cli.repo_root);
                respond_json(request, &res);
            }

            // Phase 1 Live SSE Stream (FR-P1-005)
            (&Method::Get, "/api/v1/live") => {
                handle_live_sse(request, &active_repo_id, &storage_arc, &req_url);
            }

            // Legacy & Compatibility Endpoints
            (&Method::Get, "/api/status") => {
                let json = api_status(&cli.repo_root);
                respond_json(request, &json);
            }
            (&Method::Get, "/api/verify") => {
                let json = api_verify(&cli.repo_root);
                respond_json(request, &json);
            }
            (&Method::Get, "/api/impact") => {
                let json = api_impact(&cli.repo_root, &req_url);
                respond_json(request, &json);
            }
            (&Method::Get, "/api/graph") => {
                let json = api_graph(&cli.repo_root);
                respond_json(request, &json);
            }
            (&Method::Get, "/api/contracts") => {
                let json = api_contracts(&cli.repo_root);
                respond_json(request, &json);
            }
            (&Method::Get, "/api/trajectory") => {
                let json = api_trajectory(&cli.repo_root);
                respond_json(request, &json);
            }
            (&Method::Options, _) => {
                let header_cors =
                    Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap();
                let header_methods = Header::from_bytes(
                    &b"Access-Control-Allow-Methods"[..],
                    &b"GET, POST, OPTIONS"[..],
                )
                .unwrap();
                let header_headers = Header::from_bytes(
                    &b"Access-Control-Allow-Headers"[..],
                    &b"Content-Type, Authorization"[..],
                )
                .unwrap();
                let response = Response::empty(StatusCode(204))
                    .with_header(header_cors)
                    .with_header(header_methods)
                    .with_header(header_headers);
                let _ = request.respond(response);
            }
            _ => {
                let response = Response::new(
                    StatusCode(404),
                    vec![Header::from_bytes(&b"Content-Type"[..], &b"text/plain"[..]).unwrap()],
                    Cursor::new("Not Found"),
                    Some(9),
                    None,
                );
                let _ = request.respond(response);
            }
        }
    }

    Ok(EXIT_PASS)
}

fn respond_json(req: Request, json: &str) {
    let header_content = Header::from_bytes(
        &b"Content-Type"[..],
        &b"application/json; charset=utf-8"[..],
    )
    .unwrap();
    let header_cors = Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap();
    let header_methods = Header::from_bytes(
        &b"Access-Control-Allow-Methods"[..],
        &b"GET, POST, OPTIONS"[..],
    )
    .unwrap();
    let header_headers = Header::from_bytes(
        &b"Access-Control-Allow-Headers"[..],
        &b"Content-Type, Authorization"[..],
    )
    .unwrap();

    let response = Response::from_string(json)
        .with_header(header_content)
        .with_header(header_cors)
        .with_header(header_methods)
        .with_header(header_headers);
    let _ = req.respond(response);
}

fn read_body(req: &mut Request) -> String {
    let mut buf = Vec::new();
    let _ = req.as_reader().read_to_end(&mut buf);
    String::from_utf8(buf).unwrap_or_default()
}

fn handle_connect_repo(
    body: &str,
    storage: &Arc<Mutex<Option<StorageEngine>>>,
    active_repo_id: &Arc<Mutex<String>>,
) -> String {
    let parsed: Result<ConnectRepoRequest, _> = serde_json::from_str(body);
    match parsed {
        Ok(req) => {
            let path = PathBuf::from(&req.root_path);
            if !path.exists() {
                return serde_json::json!({
                    "error_code": "ERR-REPO-001",
                    "error": format!("Path does not exist: {}", req.root_path)
                })
                .to_string();
            }

            let canon = path.canonicalize().unwrap_or(path.clone());
            let repo_id = blake3::hash(canon.to_string_lossy().as_bytes())
                .to_hex()
                .to_string();
            let display_name = req.display_name.unwrap_or_else(|| {
                canon
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("repo")
                    .to_string()
            });

            let record = RepoRecord {
                repo_id: repo_id.clone(),
                root_path: canon.to_string_lossy().to_string(),
                display_name,
                active_branch: None,
                head_commit_oid: None,
                latest_snap_id: None,
                languages_json: "[]".to_string(),
                monorepo_json: "{}".to_string(),
                last_indexed_seq: 0,
            };

            let storage_guard = storage.lock().unwrap();
            if let Some(ref st) = *storage_guard {
                let _ = st.register_repository(&record);
            }

            *active_repo_id.lock().unwrap() = repo_id.clone();

            serde_json::json!({
                "status": "connected",
                "repo_id": repo_id,
                "root_path": canon.to_string_lossy()
            })
            .to_string()
        }
        Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
    }
}

fn handle_list_repos(storage: &Arc<Mutex<Option<StorageEngine>>>) -> String {
    let storage_guard = storage.lock().unwrap();
    if let Some(ref st) = *storage_guard {
        if let Ok(repos) = st.list_repositories() {
            return serde_json::to_string(&repos).unwrap_or_else(|_| "[]".to_string());
        }
    }
    "[]".to_string()
}

fn handle_switch_repo(
    body: &str,
    storage: &Arc<Mutex<Option<StorageEngine>>>,
    active_repo_id: &Arc<Mutex<String>>,
) -> String {
    let parsed: Result<SwitchRepoRequest, _> = serde_json::from_str(body);
    match parsed {
        Ok(req) => {
            let storage_guard = storage.lock().unwrap();
            if let Some(ref st) = *storage_guard {
                if let Ok(Some(repo)) = st.get_repository(&req.repo_id) {
                    *active_repo_id.lock().unwrap() = repo.repo_id.clone();
                    return serde_json::json!({
                        "status": "switched",
                        "active_repo": repo
                    })
                    .to_string();
                }
            }
            serde_json::json!({ "error": "Repo not found" }).to_string()
        }
        Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
    }
}

fn parse_query_param(url: &str, param_name: &str) -> Option<String> {
    let query = url.split('?').nth(1)?;
    for part in query.split('&') {
        let mut kv = part.splitn(2, '=');
        if let Some(key) = kv.next() {
            if key == param_name {
                let raw_val = kv.next().unwrap_or("");
                return Some(url_decode(raw_val));
            }
        }
    }
    None
}

fn url_decode(s: &str) -> String {
    let mut result = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(val) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
            {
                result.push(val as char);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            result.push(' ');
            i += 1;
            continue;
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

fn handle_fs_browse(url: &str, repo_root: &Path) -> String {
    let requested_path = parse_query_param(url, "path");

    let target_path = match requested_path {
        Some(ref p) if !p.trim().is_empty() => PathBuf::from(p),
        _ => repo_root.to_path_buf(),
    };

    let resolved_dir = if target_path.is_dir() {
        target_path.canonicalize().unwrap_or(target_path)
    } else if target_path.parent().is_some() {
        let reason = if target_path.exists() {
            "the selected path is not a directory"
        } else {
            "the selected directory does not exist"
        };
        return serde_json::json!({
            "error_code": "ERR-FS-001",
            "error": format!("Cannot browse '{}': {reason}", target_path.display()),
            "requested_path": target_path.to_string_lossy(),
            "current_path": "",
            "parent_path": null,
            "is_git_repo": false,
            "directories": [],
            "quick_presets": []
        })
        .to_string();
    } else {
        return serde_json::json!({
            "error_code": "ERR-FS-001",
            "error": format!("Cannot browse '{}': the selected directory does not exist", target_path.display()),
            "requested_path": target_path.to_string_lossy(),
            "current_path": "",
            "parent_path": null,
            "is_git_repo": false,
            "directories": [],
            "quick_presets": []
        })
        .to_string();
    };

    let current_path_str = resolved_dir.to_string_lossy().to_string();
    let parent_path_str = resolved_dir
        .parent()
        .map(|p| p.to_string_lossy().to_string());
    let is_git_repo = resolved_dir.join(".git").exists();

    let mut subdirs: Vec<serde_json::Value> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&resolved_dir) {
        let mut dir_items: Vec<(String, String, bool)> = Vec::new();
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    if file_name.starts_with('.') || file_name == ".git" {
                        continue;
                    }
                    if file_name == "node_modules"
                        || file_name == "target"
                        || file_name == ".next"
                        || file_name == "vendor"
                    {
                        continue;
                    }
                    let path_str = entry.path().to_string_lossy().to_string();
                    let is_git = entry.path().join(".git").exists();
                    dir_items.push((file_name, path_str, is_git));
                }
            }
        }
        dir_items.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, path, is_git) in dir_items {
            subdirs.push(serde_json::json!({
                "name": name,
                "path": path,
                "is_dir": true,
                "is_git_repo": is_git
            }));
        }
    }

    serde_json::json!({
        "current_path": current_path_str,
        "parent_path": parent_path_str,
        "is_git_repo": is_git_repo,
        "directories": subdirs,
        "quick_presets": []
    })
    .to_string()
}

fn build_network_topology(
    nodes: &[String],
) -> (
    Vec<(String, String)>,
    std::collections::BTreeMap<String, u32>,
) {
    let mut edges = Vec::new();
    let mut cluster_map = std::collections::BTreeMap::new();
    if nodes.is_empty() {
        return (edges, cluster_map);
    }

    let mut groups: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for node in nodes {
        let key = if let Some(idx) = node.find('/') {
            node[..idx].to_string()
        } else {
            "root".to_string()
        };
        groups.entry(key).or_default().push(node.clone());
    }

    let mut group_leaders = Vec::new();

    for (cluster_id, members) in (1u32..).zip(groups.values()) {
        for member in members {
            cluster_map.insert(member.clone(), cluster_id);
        }

        if let Some(leader) = members.first() {
            group_leaders.push(leader.clone());
            for (idx, member) in members.iter().enumerate().skip(1) {
                if idx <= 3 {
                    edges.push((leader.clone(), member.clone()));
                } else {
                    edges.push((members[idx - 1].clone(), member.clone()));
                }
            }
        }
    }

    let hub = groups
        .get("root")
        .and_then(|r| r.first())
        .unwrap_or(&group_leaders[0]);
    for leader in &group_leaders {
        if leader != hub {
            edges.push((hub.clone(), leader.clone()));
        }
    }
    for i in 0..group_leaders.len().saturating_sub(1) {
        edges.push((group_leaders[i].clone(), group_leaders[i + 1].clone()));
    }

    (edges, cluster_map)
}

fn discover_repo_files(root: &Path, max_files: usize) -> Vec<String> {
    let mut files = Vec::new();
    let mut dirs = vec![root.to_path_buf()];
    let supported_exts = [
        "ts", "tsx", "js", "jsx", "rs", "py", "go", "json", "toml", "md",
    ];

    while let Some(current_dir) = dirs.pop() {
        if files.len() >= max_files {
            break;
        }
        if let Ok(entries) = std::fs::read_dir(&current_dir) {
            let mut dir_entries: Vec<_> = entries.flatten().collect();
            dir_entries.sort_by_key(|e| e.file_name());

            for entry in dir_entries {
                if let Ok(ft) = entry.file_type() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with('.')
                        || name == "node_modules"
                        || name == "target"
                        || name == ".next"
                        || name == "dist"
                        || name == "build"
                    {
                        continue;
                    }
                    if ft.is_dir() {
                        dirs.push(entry.path());
                    } else if ft.is_file() {
                        let ext = entry
                            .path()
                            .extension()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_lowercase();
                        if supported_exts.contains(&ext.as_str()) {
                            if let Ok(rel) = entry.path().strip_prefix(root) {
                                let rel_str = rel.to_string_lossy().replace('\\', "/");
                                files.push(rel_str);
                                if files.len() >= max_files {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    files.sort();
    files
}

fn handle_files(
    active_repo_id: &Arc<Mutex<String>>,
    default_root: &Path,
    storage: &Arc<Mutex<Option<StorageEngine>>>,
) -> String {
    let active_id = active_repo_id.lock().unwrap().clone();
    let mut target_dir = default_root.to_path_buf();
    let storage_guard = storage.lock().unwrap();
    if let Some(ref st) = *storage_guard {
        if let Ok(Some(repo)) = st.get_repository(&active_id) {
            let p = PathBuf::from(&repo.root_path);
            if p.is_dir() {
                target_dir = p;
            }
        }
    }
    drop(storage_guard);

    let discovered = discover_repo_files(&target_dir, 500);
    let items: Vec<serde_json::Value> = discovered
        .into_iter()
        .map(|path| {
            let ext = path.split('.').next_back().unwrap_or("").to_lowercase();
            let name = path.split('/').next_back().unwrap_or(&path).to_string();
            let dir = if let Some(idx) = path.rfind('/') {
                path[..idx].to_string()
            } else {
                "".to_string()
            };
            serde_json::json!({
                "path": path,
                "name": name,
                "ext": ext,
                "dir": dir,
                // File discovery is not analysis. Keep this state explicit so
                // the legacy CLI UI cannot present an unindexed file as a
                // verified result.
                "status": "discovered",
                "coverage_state": "Unknown"
            })
        })
        .collect();

    serde_json::json!({
        "repo_id": active_id,
        "total_files": items.len(),
        "files": items
    })
    .to_string()
}

fn handle_graph_universe(
    url: &str,
    active_repo_id: &Arc<Mutex<String>>,
    default_root: &Path,
    storage: &Arc<Mutex<Option<StorageEngine>>>,
) -> String {
    let is_3d = !url.contains("dim=2d");
    let active_id = active_repo_id.lock().unwrap().clone();

    let mut target_dir = default_root.to_path_buf();
    let storage_guard = storage.lock().unwrap();
    if let Some(ref st) = *storage_guard {
        if let Ok(Some(repo)) = st.get_repository(&active_id) {
            let p = PathBuf::from(&repo.root_path);
            if p.is_dir() {
                target_dir = p;
            }
        }
    }
    drop(storage_guard);

    let nodes = discover_repo_files(&target_dir, 200);

    let (edges, cluster_map) = build_network_topology(&nodes);

    let seed = blake3::hash(format!("{}_active_universe", active_id).as_bytes()).into();
    let layout_plugin = DeterministicForceLayout;
    let coords = layout_plugin
        .compute_layout(&nodes, &edges, &seed, is_3d)
        .unwrap_or_default();

    let nodes_list: Vec<serde_json::Value> = coords
        .into_iter()
        .map(|(id, coord)| {
            let cluster = *cluster_map.get(&id).unwrap_or(&coord.cluster_id);
            serde_json::json!({
                "id": id,
                "x": coord.x,
                "y": coord.y,
                "z": coord.z,
                "cluster_id": cluster
            })
        })
        .collect();

    serde_json::json!({
        "repo_id": active_id,
        "dimensions": if is_3d { "3D" } else { "2D" },
        "node_count": nodes.len(),
        "edge_count": edges.len(),
        "coverage_state": if nodes.is_empty() { "Unknown" } else { "Discovered" },
        "nodes": nodes_list,
        "edges": edges.into_iter().map(|(src, dst)| {
            serde_json::json!({ "source": src, "target": dst, "kind": "calls" })
        }).collect::<Vec<_>>()
    })
    .to_string()
}

fn handle_graph_ego(url: &str, storage: &Arc<Mutex<Option<StorageEngine>>>) -> String {
    let stable_id = url
        .split('?')
        .nth(1)
        .and_then(|q| {
            for pair in q.split('&') {
                let mut parts = pair.split('=');
                if let Some("id") = parts.next() {
                    return parts
                        .next()
                        .map(|s| s.replace("%23", "#").replace("%2F", "/"));
                }
            }
            None
        })
        .unwrap_or_default();

    if stable_id.is_empty() {
        return serde_json::json!({
            "target_node": null,
            "incoming_callers": [],
            "outgoing_callees": [],
            "coverage_state": "Unknown",
            "error_code": "ERR-GRAPH-400",
            "error": "The graph ego request requires a non-empty id"
        })
        .to_string();
    }

    let storage_guard = storage.lock().unwrap();
    if let Some(ref st) = *storage_guard {
        if let Ok(repositories) = st.list_repositories() {
            for repository in repositories {
                if let Some(snapshot_id) = repository.latest_snap_id {
                    if let Ok(ego) = st.query_ego_network(&snapshot_id, &stable_id) {
                        if ego.target_node.is_some() {
                            return serde_json::to_string(&ego).unwrap_or_default();
                        }
                    }
                }
            }
        }
    }

    serde_json::json!({
        "target_node": null,
        "incoming_callers": [],
        "outgoing_callees": [],
        "coverage_state": "Unknown",
        "error_code": "ERR-GRAPH-404",
        "error": format!("No analyzed symbol named '{stable_id}' was found")
    })
    .to_string()
}

fn handle_simulate_edit(body: &str, repo_root: &Path) -> String {
    let start = Instant::now();
    let request: SimulateEditRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => {
            return serde_json::json!({
                "verdict": "UNKNOWN",
                "error_code": "ERR-SIM-400",
                "error": error.to_string()
            })
            .to_string();
        }
    };
    let relative_path = request.file_path.replace('\\', "/");
    let invalid_path = relative_path.is_empty()
        || Path::new(&relative_path).is_absolute()
        || relative_path.split('/').any(|segment| {
            segment.is_empty() || segment == ".." || segment == ".git" || segment == ".autopsy"
        });
    if invalid_path {
        return serde_json::json!({
            "verdict": "UNKNOWN",
            "error_code": "ERR-SIM-400",
            "error": "file_path must be a safe repository-relative path",
            "computation_latency_ms": start.elapsed().as_millis() as u64
        })
        .to_string();
    }

    let config = match load_or_default_config(repo_root) {
        Ok(config) => config,
        Err(error) => {
            return serde_json::json!({
                "verdict": "UNKNOWN",
                "error_code": "ERR-SIM-500",
                "error": error.to_string(),
                "computation_latency_ms": start.elapsed().as_millis() as u64
            })
            .to_string();
        }
    };
    let baseline = match build_snapshot_and_graph(repo_root, &config, None, None) {
        Ok(artifacts) => artifacts,
        Err(error) => {
            return serde_json::json!({
                "verdict": "UNKNOWN",
                "error_code": "ERR-SIM-501",
                "error": error.to_string(),
                "computation_latency_ms": start.elapsed().as_millis() as u64
            })
            .to_string();
        }
    };
    if !baseline.snapshot.files.contains_key(&relative_path) {
        return serde_json::json!({
            "verdict": "UNKNOWN",
            "error_code": "ERR-SIM-404",
            "error": format!("file is not part of the analyzed repository: {relative_path}"),
            "computation_latency_ms": start.elapsed().as_millis() as u64
        })
        .to_string();
    }

    let parsed =
        crate::parse_source_buffer(repo_root, &relative_path, request.unsaved_buffer.as_bytes());
    if !parsed.parser_available || !parsed.syntax_valid {
        let response = SimulateEditResponse {
            verdict: if parsed.parser_available {
                "FAIL"
            } else {
                "UNKNOWN"
            }
            .to_string(),
            damaged_callers: Vec::new(),
            newly_dead_symbols: Vec::new(),
            broken_imports: Vec::new(),
            syntax_errors: parsed.diagnostics,
            computation_latency_ms: start.elapsed().as_millis() as u64,
        };
        return serde_json::to_string(&response).unwrap_or_default();
    }

    let overlays = BTreeMap::from([(relative_path, request.unsaved_buffer.into_bytes())]);
    let candidate = match crate::build_snapshot_and_graph_with_overlays(
        repo_root,
        &config,
        None,
        Some(&baseline.snapshot),
        &overlays,
        None,
    ) {
        Ok(artifacts) => artifacts,
        Err(error) => {
            return serde_json::json!({
                "verdict": "UNKNOWN",
                "error_code": "ERR-SIM-502",
                "error": error.to_string(),
                "computation_latency_ms": start.elapsed().as_millis() as u64
            })
            .to_string();
        }
    };

    let baseline_ids: BTreeSet<SymbolId> = baseline
        .graph
        .all_symbols_sorted()
        .into_iter()
        .map(|symbol| symbol.stable_id.clone())
        .collect();
    let candidate_ids: BTreeSet<SymbolId> = candidate
        .graph
        .all_symbols_sorted()
        .into_iter()
        .map(|symbol| symbol.stable_id.clone())
        .collect();
    let removed_ids: Vec<SymbolId> = baseline_ids.difference(&candidate_ids).cloned().collect();

    let mut damaged_by_id = BTreeMap::<String, DamagedCallerItem>::new();
    let impact_engine = ImpactEngine::new();
    for removed_id in &removed_ids {
        let Ok(report) = impact_engine.compute_impact(
            &baseline.graph,
            &ImpactQuery {
                seed_symbols: vec![removed_id.clone()],
                direction: ImpactDirection::Backward,
                profile: ImpactProfile {
                    name: "legacy-cli-simulation".to_string(),
                    max_depth: 10,
                    budget: 10_000,
                    edge_kinds: BTreeSet::new(),
                    traverse_file_nodes: true,
                },
            },
        ) else {
            continue;
        };
        for caller_id in report.impacted_entities {
            if caller_id == *removed_id {
                continue;
            }
            let Some(caller) = baseline.graph.get_node(&caller_id) else {
                continue;
            };
            let location = caller.range.as_ref();
            let edge = baseline
                .graph
                .all_edges_sorted()
                .into_iter()
                .find(|edge| edge.source == caller_id && edge.target == *removed_id);
            let file_path = edge
                .and_then(|item| item.location.as_ref())
                .map(|item| item.path.clone())
                .or_else(|| location.map(|item| item.path.clone()))
                .unwrap_or_default();
            let line = edge
                .and_then(|item| item.location.as_ref())
                .map(|item| i64::from(item.start_line))
                .or_else(|| location.map(|item| i64::from(item.start_line)))
                .unwrap_or(0);
            let col = edge
                .and_then(|item| item.location.as_ref())
                .and_then(|item| item.start_col)
                .map(i64::from)
                .unwrap_or(0);
            damaged_by_id.insert(
                caller_id.to_string(),
                DamagedCallerItem {
                    stable_id: caller_id.to_string(),
                    file_path,
                    calling_line: line,
                    calling_col: col,
                    diagnostic: format!("depends on removed symbol {removed_id}"),
                    severity: "breaking_error".to_string(),
                },
            );
        }
    }

    let mut broken_imports = Vec::new();
    for edge in baseline.graph.all_edges_sorted() {
        if edge.target != edge.source
            && removed_ids.contains(&edge.target)
            && matches!(edge.kind, autopsy_domain::EdgeKind::Imports)
        {
            broken_imports.push(edge.target.to_string());
        }
    }
    broken_imports.sort();
    broken_imports.dedup();

    let coverage_verified = baseline.coverage_state == CoverageState::Verified
        && candidate.coverage_state == CoverageState::Verified;
    let verdict = if !damaged_by_id.is_empty() || !broken_imports.is_empty() {
        "FAIL"
    } else if coverage_verified {
        "PASS"
    } else {
        "UNKNOWN"
    };
    let response = SimulateEditResponse {
        verdict: verdict.to_string(),
        damaged_callers: damaged_by_id.into_values().collect(),
        newly_dead_symbols: Vec::new(),
        broken_imports,
        syntax_errors: Vec::new(),
        computation_latency_ms: start.elapsed().as_millis() as u64,
    };
    serde_json::to_string(&response).unwrap_or_default()
}

fn handle_live_sse(
    req: Request,
    _active_repo_id: &Arc<Mutex<String>>,
    storage: &Arc<Mutex<Option<StorageEngine>>>,
    url: &str,
) {
    let since = url
        .split('?')
        .nth(1)
        .and_then(|query| {
            query.split('&').find_map(|pair| {
                let (key, value) = pair.split_once('=')?;
                (key == "since" || key == "since_seq")
                    .then(|| value.parse::<i64>().ok())
                    .flatten()
            })
        })
        .unwrap_or(0);
    let events = storage
        .lock()
        .ok()
        .and_then(|guard| {
            guard
                .as_ref()
                .and_then(|st| st.live_events_since(since, 500).ok())
        })
        .unwrap_or_default();
    let mut sse_data = String::from("retry: 1000\n\n");
    for event in events {
        sse_data.push_str(&format!(
            "id: {}\nevent: {}\ndata: {}\n\n",
            event.seq_id, event.event_type, event.data_json
        ));
    }
    if sse_data == "retry: 1000\n\n" {
        sse_data.push_str(": keep-alive\n\n");
    }
    let header_event = Header::from_bytes(&b"Content-Type"[..], &b"text/event-stream"[..]).unwrap();
    let header_cache = Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..]).unwrap();
    let header_conn = Header::from_bytes(&b"Connection"[..], &b"keep-alive"[..]).unwrap();
    let header_cors = Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap();

    let response = Response::from_string(sse_data)
        .with_header(header_event)
        .with_header(header_cache)
        .with_header(header_conn)
        .with_header(header_cors);
    let _ = req.respond(response);
}

fn api_status(repo_root: &Path) -> String {
    if !repo_root.is_dir() {
        return serde_json::json!({
            "repo_root": repo_root.display().to_string(),
            "snapshot_id": null,
            "coverage_state": "Unknown",
            "files_scanned": 0,
            "symbol_count": 0,
            "edge_count": 0,
            "error_code": "ERR-REPO-001",
            "error": "Repository directory does not exist",
            "version": ANALYZER_VERSION
        })
        .to_string();
    }
    let config = load_or_default_config(repo_root).unwrap_or_default();
    if let Ok(artifacts) = build_snapshot_and_graph(repo_root, &config, None, None) {
        let coverage_str = match artifacts.coverage_state {
            CoverageState::Verified => "Verified",
            CoverageState::Partial => "Partial",
            CoverageState::Unknown => "Unknown",
            CoverageState::Unsupported => "Unsupported",
        };
        serde_json::json!({
            "repo_root": repo_root.display().to_string(),
            "snapshot_id": artifacts.snapshot.snapshot_id.to_string(),
            "coverage_state": coverage_str,
            "files_scanned": artifacts.snapshot.files.len(),
            "symbol_count": artifacts.graph.node_count(),
            "edge_count": artifacts.graph.edge_count(),
            "version": ANALYZER_VERSION
        })
        .to_string()
    } else {
        serde_json::json!({
            "repo_root": repo_root.display().to_string(),
            "snapshot_id": null,
            "coverage_state": "Unknown",
            "files_scanned": 0,
            "symbol_count": 0,
            "edge_count": 0,
            "error_code": "ERR-ANALYSIS-001",
            "error": "Repository analysis did not complete",
            "version": ANALYZER_VERSION
        })
        .to_string()
    }
}

fn api_verify(repo_root: &Path) -> String {
    if !repo_root.is_dir() {
        return serde_json::json!({
            "snapshot_id": null,
            "coverage_state": "Unknown",
            "findings": [],
            "error_code": "ERR-REPO-001",
            "error": "Repository directory does not exist"
        })
        .to_string();
    }
    let config = load_or_default_config(repo_root).unwrap_or_default();
    let inv_path = repo_root.join(".autopsy").join("invariants.yml");
    let invariants_config = if inv_path.exists() {
        match InvariantsConfig::load_from_file(&inv_path) {
            Ok(config) => config,
            Err(error) => {
                return serde_json::json!({
                    "snapshot_id": null,
                    "coverage_state": "Unknown",
                    "findings": [],
                    "error_code": "ERR-VERIFY-400",
                    "error": error.to_string()
                })
                .to_string();
            }
        }
    } else {
        InvariantsConfig {
            version: "0.0.1".to_string(),
            invariants: vec![],
        }
    };

    if let Ok(artifacts) = build_snapshot_and_graph(repo_root, &config, None, None) {
        let eval_ctx = EvaluationContext {
            snapshot_id: artifacts.snapshot.snapshot_id.clone(),
            analyzer_version: ANALYZER_VERSION,
            graph: &artifacts.graph,
            baseline_graph: None,
            diff: None,
            contracts_before: &std::collections::BTreeMap::new(),
            contracts_after: &std::collections::BTreeMap::new(),
            coverage_state: artifacts.coverage_state,
        };

        let evaluator = InvariantEvaluator::new();
        let pairs = evaluator
            .evaluate(&invariants_config, &eval_ctx)
            .unwrap_or_default();

        let mut findings = Vec::new();
        for (f, _) in pairs {
            findings.push(serde_json::json!({
                "rule_id": f.rule_id,
                "status": match f.status {
                    FindingStatus::Pass => "pass",
                    FindingStatus::Fail => "fail",
                    FindingStatus::Unknown => "unknown",
                },
                "severity": format!("{:?}", f.severity).to_lowercase(),
                "message": f.message,
                "entities": f.entities
            }));
        }

        serde_json::json!({
            "snapshot_id": artifacts.snapshot.snapshot_id.to_string(),
            "findings": findings
        })
        .to_string()
    } else {
        serde_json::json!({
            "snapshot_id": null,
            "coverage_state": "Unknown",
            "findings": [],
            "error_code": "ERR-VERIFY-501",
            "error": "Repository verification did not complete"
        })
        .to_string()
    }
}

fn api_impact(repo_root: &Path, url: &str) -> String {
    let seed = url
        .split('?')
        .nth(1)
        .and_then(|q| {
            for pair in q.split('&') {
                let mut parts = pair.split('=');
                if let Some("seed") = parts.next() {
                    return parts
                        .next()
                        .map(|s| s.replace("%23", "#").replace("%2F", "/"));
                }
            }
            None
        })
        .unwrap_or_default();

    if seed.is_empty() {
        return serde_json::json!({
            "seed": "",
            "likely_affected": [],
            "possibly_affected": [],
            "paths": [],
            "coverage_state": "Unknown",
            "error_code": "ERR-IMPACT-400",
            "error": "The impact request requires a non-empty seed"
        })
        .to_string();
    }

    let config = load_or_default_config(repo_root).unwrap_or_default();
    if let Ok(artifacts) = build_snapshot_and_graph(repo_root, &config, None, None) {
        let query = ImpactQuery {
            seed_symbols: vec![SymbolId::new(&seed)],
            direction: ImpactDirection::Forward,
            profile: ImpactProfile {
                name: "web-ui-impact".to_string(),
                max_depth: 10,
                budget: 1000,
                edge_kinds: std::collections::BTreeSet::new(),
                traverse_file_nodes: true,
            },
        };
        let engine = ImpactEngine::new();
        if let Ok(report) = engine.compute_impact(&artifacts.graph, &query) {
            let likely: Vec<String> = report
                .likely_affected
                .iter()
                .map(|id| id.as_str().to_string())
                .collect();
            let possible: Vec<String> = report
                .possibly_affected
                .iter()
                .map(|id| id.as_str().to_string())
                .collect();

            serde_json::json!({
                "seed": seed,
                "likely_affected": likely,
                "possibly_affected": possible,
                "total_impacted": report.impacted_entities.len(),
                "confidence_tiers": {
                    "likely_count": report.likely_affected.len(),
                    "possible_count": report.possibly_affected.len()
                }
            })
            .to_string()
        } else {
            serde_json::json!({
                "seed": seed,
                "likely_affected": [],
                "possibly_affected": [],
                "coverage_state": "Unknown",
                "error_code": "ERR-IMPACT-502",
                "error": "Impact analysis did not complete"
            })
            .to_string()
        }
    } else {
        serde_json::json!({
            "seed": seed,
            "likely_affected": [],
            "possibly_affected": [],
            "coverage_state": "Unknown",
            "error_code": "ERR-IMPACT-501",
            "error": "Repository analysis did not complete"
        })
        .to_string()
    }
}

fn api_graph(repo_root: &Path) -> String {
    let config = load_or_default_config(repo_root).unwrap_or_default();
    if let Ok(artifacts) = build_snapshot_and_graph(repo_root, &config, None, None) {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        for sym in artifacts.graph.all_symbols_sorted().into_iter().take(100) {
            nodes.push(serde_json::json!({
                "id": sym.stable_id.as_str(),
                "label": sym.qualified_name.clone(),
                "kind": sym.kind.clone(),
                "full_name": sym.stable_id.as_str()
            }));
        }

        for edge in artifacts.graph.all_edges_sorted().into_iter().take(200) {
            edges.push(serde_json::json!({
                "source": edge.source.as_str(),
                "target": edge.target.as_str(),
                "kind": format!("{:?}", edge.kind)
            }));
        }

        serde_json::json!({
            "nodes": nodes,
            "edges": edges,
            "total_nodes": artifacts.graph.node_count(),
            "total_edges": artifacts.graph.edge_count(),
            "coverage_state": format!("{:?}", artifacts.coverage_state)
        })
        .to_string()
    } else {
        serde_json::json!({
            "nodes": [],
            "edges": [],
            "coverage_state": "Unknown",
            "error_code": "ERR-GRAPH-501",
            "error": "Repository graph analysis did not complete"
        })
        .to_string()
    }
}

fn api_contracts(repo_root: &Path) -> String {
    let config = load_or_default_config(repo_root).unwrap_or_default();
    if let Ok(artifacts) = build_snapshot_and_graph(repo_root, &config, None, None) {
        let mut contracts_list = Vec::new();
        for contract in &artifacts.contracts {
            contracts_list.push(serde_json::json!({
                "owner": contract.owner.as_str(),
                "visibility": format!("{:?}", contract.visibility),
                "inputs": contract.inputs.clone(),
                "output": contract.output.clone(),
                "effects": contract.effects.clone()
            }));
        }
        serde_json::json!({
            "contracts": contracts_list,
            "breaking_changes": [],
            "coverage_state": format!("{:?}", artifacts.coverage_state)
        })
        .to_string()
    } else {
        serde_json::json!({
            "contracts": [],
            "breaking_changes": [],
            "coverage_state": "Unknown",
            "error_code": "ERR-CONTRACT-501",
            "error": "Repository contract analysis did not complete"
        })
        .to_string()
    }
}

fn api_trajectory(repo_root: &Path) -> String {
    let mut runs = Vec::new();
    let runs_dir = repo_root.join(".autopsy").join("runs");
    if runs_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(runs_dir) {
            let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
            paths.sort();
            for p in paths {
                if p.extension().is_some_and(|e| e == "json") && !p.ends_with("latest.json") {
                    if let Ok(content) = fs::read_to_string(&p) {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                            runs.push(val);
                        }
                    }
                }
            }
        }
    }
    serde_json::json!({ "runs": runs }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_ui_never_invents_repository_graph_data() {
        let storage = Arc::new(Mutex::new(None));
        let active_repo_id = Arc::new(Mutex::new("missing-repository".to_string()));
        let response = handle_graph_universe(
            "/api/v1/graph/universe?dim=3d",
            &active_repo_id,
            Path::new("C:/path/that/does/not/exist"),
            &storage,
        );
        let value: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["node_count"], 0);
        assert_eq!(value["coverage_state"], "Unknown");
        assert!(!response.contains("PaymentService"));
        assert!(!response.contains("OrderController"));
    }

    #[test]
    fn legacy_ui_returns_explicit_unknown_for_missing_analysis() {
        let status: serde_json::Value =
            serde_json::from_str(&api_status(Path::new("C:/path/that/does/not/exist"))).unwrap();
        assert_eq!(status["coverage_state"], "Unknown");
        assert_eq!(status["error_code"], "ERR-REPO-001");

        let ego: serde_json::Value = serde_json::from_str(&handle_graph_ego(
            "/api/v1/graph/ego?id=not-indexed",
            &Arc::new(Mutex::new(None)),
        ))
        .unwrap();
        assert_eq!(ego["target_node"], serde_json::Value::Null);
        assert_eq!(ego["error_code"], "ERR-GRAPH-404");
    }

    #[test]
    fn legacy_ui_simulator_rejects_unsafe_paths_without_a_pass_fallback() {
        let response = handle_simulate_edit(
            &serde_json::json!({
                "file_path": "../outside.ts",
                "unsaved_buffer": "export const value = 1;"
            })
            .to_string(),
            Path::new("."),
        );
        let value: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["verdict"], "UNKNOWN");
        assert_eq!(value["error_code"], "ERR-SIM-400");
    }
}
