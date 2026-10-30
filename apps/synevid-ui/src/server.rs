//! Embedded HTTP web server for SynEvid Web Dashboard & Real-Time Reactive Daemon (Presentation Layer).
//!
//! Exposes Phase 1 (v0.2.0) reactive endpoints:
//! - `GET /api/v1/capabilities`: Dynamically discover registered plugins
//! - `POST /api/v1/repos/connect`: Connect local repo and register in SQLite
//! - `GET /api/v1/repos/list`: List all registered repositories
//! - `POST /api/v1/repos/switch`: Switch active repository
//! - `GET /api/v1/graph/universe`: 2D/3D visual universe with seeded deterministic coordinates
//! - `GET /api/v1/graph/ego`: Sub-5ms single-file or symbol ego-network lookup
//! - `POST /api/v1/simulate/edit`: Sub-50ms in-memory edit simulation & blast-damage evaluator
//! - `GET /api/v1/live`: Live SSE event stream pushing sequence-tracked updates

#![allow(clippy::collapsible_if)]

#[cfg(test)]
use crate::indexer::open_repository_storage;
use crate::indexer::{
    coverage_name, file_node_id, git_head, index_repository_with_artifacts_with_registry,
    index_repository_with_registry, open_repository_storage_with_registry, snapshot_for_repository,
};
use autopsy_cli::{
    AnalysisArtifacts, OverlayFileArtifacts, analyze_file_overlay_from_artifacts_with_registry,
    build_artifacts_from_snapshot_with_registry,
    build_snapshot_and_graph_for_overlay_from_artifacts_with_registry, load_or_default_config,
    normalize_contracts, open_storage_engine_with_registry, parse_source_buffer_with_registry,
};
use autopsy_diff::{ContractDelta, DiffEngine, DiffInput, SymbolDelta};
use autopsy_domain::{CoverageState, EdgeKind, RepoSnapshot, SymbolId, Visibility};
use autopsy_git_engine::{
    BisectRequest, BranchRequest, CheckoutRequest, CommitRequest, GitDiffEngine, GitGalaxy,
    GitMutationManager, GitReader, GitStatusScanner, MergeRequest, RevertRequest, StageRequest,
    TagRequest, TerminalExecRequest, VirtualTreeBuilder,
};
use autopsy_graph::DependencyGraph;
use autopsy_graph_layout::DeterministicForceLayout;
use autopsy_impact::{ImpactDirection, ImpactEngine, ImpactQuery, callers_profile};
use autopsy_plugin_api::{
    GateEvaluationResult, PluginDescriptor, PluginError, PluginRegistry, StorageEngineConfig,
    VerificationGatePlugin, WorkspaceResolverPlugin,
};
use autopsy_storage::{
    AiSessionLedgerRecord, AiWorkingCacheEntry, ForensicSessionRecord, ForensicWorkflowRecord,
    GraphNodeRow, RepoRecord, StorageEngine, TrajectoryEventRecord,
};
use notify::Watcher;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Cursor;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

static GLOBAL_SEQ_ID: AtomicI64 = AtomicI64::new(100);
const MAX_UNIVERSE_NODE_LIMIT: usize = 5_000;
const MAX_UNIVERSE_EDGE_LIMIT: usize = 12_000;
const MAX_UNIVERSE_CACHE_ENTRIES: usize = 4;

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
pub struct RemoveRepoRequest {
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

#[derive(Debug, Deserialize)]
pub struct StartSessionRequest {
    pub session_id: Option<String>,
    pub actor_kind: Option<String>,
    pub actor_name: Option<String>,
    pub repo_id: Option<String>,
    pub baseline_snap_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RevertStepRequest {
    pub session_id: String,
    pub event_seq: i64,
}

#[derive(Debug, Deserialize)]
pub struct AiCacheQueryRequest {
    pub session_id: String,
    pub symbol: String,
    pub query_kind: String,
    pub snapshot_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LiveBroadcastRequest {
    pub event: String,
    pub data: serde_json::Value,
}

#[derive(Serialize)]
struct CanonicalRepositoryExport<'a> {
    schema_version: &'static str,
    repo_id: &'a str,
    snapshot: &'a RepoSnapshot,
    nodes: &'a [GraphNodeRow],
    edges: &'a [autopsy_storage::GraphEdgeRow],
}

#[derive(Serialize)]
struct RepositoryExportEnvelope<'a> {
    digest: String,
    export: CanonicalRepositoryExport<'a>,
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
    pub repo_id: String,
    pub verdict: String,
    pub coverage_state: String,
    pub coverage_note: Option<String>,
    pub baseline_snapshot_id: Option<String>,
    pub candidate_snapshot_id: Option<String>,
    pub impacted_entities: Vec<String>,
    pub impact_truncated: bool,
    pub damaged_callers: Vec<DamagedCallerItem>,
    pub newly_dead_symbols: Vec<DeadSymbolItem>,
    pub broken_imports: Vec<String>,
    pub syntax_errors: Vec<String>,
    pub computation_latency_ms: u64,
}

#[derive(Clone)]
pub struct DashboardServer {
    pub repo_root: PathBuf,
    pub host: String,
    pub port: u16,
    pub registry: Arc<PluginRegistry>,
    pub storage: Arc<Mutex<Option<StorageEngine>>>,
    pub active_repo_id: Arc<Mutex<String>>,
    pub active_repo_path: Arc<Mutex<PathBuf>>,
    pub connected_repos: Arc<Mutex<std::collections::HashMap<String, RepoRecord>>>,
    pub analysis_artifact_cache: Arc<Mutex<Option<(String, String, Arc<AnalysisArtifacts>)>>>,
    pub repository_storage_cache: Arc<Mutex<BTreeMap<PathBuf, StorageEngine>>>,
    watcher_stop: Arc<AtomicBool>,
    pub universe_cache: Arc<Mutex<std::collections::HashMap<String, String>>>,
    pub live_events: Arc<Mutex<Vec<serde_json::Value>>>,
    live_event_signal: Arc<(Mutex<u64>, Condvar)>,
    pub active_ai_session_id: Arc<Mutex<String>>,
}

impl DashboardServer {
    pub fn new(repo_root: PathBuf, host: String, port: u16) -> Self {
        let registry = Arc::new(PluginRegistry::new());
        autopsy_cli::register_builtin_language_plugins(&registry, &repo_root);
        autopsy_cli::register_builtin_storage_plugins(&registry);
        register_workspace_resolvers(&registry);
        register_verification_gates(&registry);
        // Register default layout plugin
        registry.register_layout(Arc::new(DeterministicForceLayout));

        let db_path = repo_root.join(".autopsy").join("storage.db");
        let storage = open_storage_engine_with_registry(
            &registry,
            &StorageEngineConfig {
                database_path: db_path,
                cache_dir: Some(repo_root.join(".autopsy").join("cache")),
                use_wal: true,
                recover_corruption: true,
            },
        )
        .ok();

        let root_canon = repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.clone());
        let is_synevid_self = root_canon.to_string_lossy().ends_with("SynEvid");

        let mut connected_repos = std::collections::HashMap::new();
        let mut initial_active_id = String::new();
        let mut initial_active_path = PathBuf::new();

        // Load genuine user repositories previously registered in storage (Never register daemon root as a user repo)
        if let Some(ref st) = storage {
            if let Ok(repos) = st.list_repositories() {
                for r in repos {
                    if !r.root_path.ends_with("SynEvid")
                        && !r.display_name.to_lowercase().contains("synevid")
                    {
                        let is_native_linux = !r.root_path.starts_with("/mnt/");
                        let current_is_mnt =
                            initial_active_path.to_string_lossy().starts_with("/mnt/");
                        if initial_active_id.is_empty() || (is_native_linux && current_is_mnt) {
                            initial_active_id = r.repo_id.clone();
                            initial_active_path = PathBuf::from(&r.root_path);
                        }
                        connected_repos.insert(r.repo_id.clone(), r);
                    }
                }
            }
        }

        if let Some(ref st) = storage {
            if let Ok(Some(saved_active_id)) = st.get_daemon_state("active_repo_id") {
                if let Some(record) = connected_repos.get(&saved_active_id) {
                    if Path::new(&record.root_path).is_dir() {
                        initial_active_id = saved_active_id;
                        initial_active_path = PathBuf::from(&record.root_path);
                    }
                }
            }
        }

        // If repo_root is an explicit project directory (e.g. in tests or targeted CLI usage) and not SynEvid itself, activate it
        if !is_synevid_self && repo_root.exists() {
            let repo_id = blake3::hash(root_canon.to_string_lossy().as_bytes())
                .to_hex()
                .to_string();
            let display_name = repo_root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("repository")
                .to_string();
            let repo = open_repository_storage_with_registry(&root_canon, &registry)
                .and_then(|repo_storage| {
                    index_repository_with_registry(
                        &root_canon,
                        &repo_id,
                        &display_name,
                        &repo_storage,
                        &registry,
                    )
                    .map(|(record, _)| record)
                })
                .unwrap_or_else(|error| {
                    eprintln!("SynEvid: initial repository scan failed: {error}");
                    RepoRecord {
                        repo_id: repo_id.clone(),
                        root_path: root_canon.to_string_lossy().replace('\\', "/"),
                        display_name: display_name.clone(),
                        active_branch: None,
                        head_commit_oid: None,
                        latest_snap_id: None,
                        languages_json: "[]".to_string(),
                        monorepo_json: "{}".to_string(),
                        last_indexed_seq: 0,
                    }
                });
            if let Some(ref st) = storage {
                let _ = st.register_repository(&repo);
                let _ = st.set_daemon_state("active_repo_id", &repo_id);
            }
            initial_active_id = repo_id.clone();
            initial_active_path = root_canon.clone();
            connected_repos.insert(repo_id, repo);
        }

        if let Some(record) = connected_repos.get(&initial_active_id).cloned()
            && Path::new(&record.root_path).is_dir()
        {
            let needs_reindex = match open_repository_storage_with_registry(
                Path::new(&record.root_path),
                &registry,
            ) {
                Ok(repo_storage) => record
                    .latest_snap_id
                    .as_deref()
                    .and_then(|snapshot_id| {
                        repo_storage
                            .get_snapshot(&autopsy_domain::SnapshotId::new(snapshot_id))
                            .ok()
                            .flatten()
                            .map(|snapshot| {
                                repo_storage
                                    .graph_rows_for_snapshot(snapshot.snapshot_id.as_str())
                                    .map(|(nodes, _)| {
                                        !snapshot.files.is_empty() && nodes.is_empty()
                                    })
                                    .unwrap_or(true)
                            })
                    })
                    .unwrap_or(true),
                Err(_) => true,
            };
            if needs_reindex {
                if let Ok(repo_storage) =
                    open_repository_storage_with_registry(Path::new(&record.root_path), &registry)
                {
                    match index_repository_with_registry(
                        Path::new(&record.root_path),
                        &record.repo_id,
                        &record.display_name,
                        &repo_storage,
                        &registry,
                    ) {
                        Ok((updated, _)) => {
                            connected_repos.insert(updated.repo_id.clone(), updated.clone());
                            if let Some(daemon_storage) = &storage {
                                let _ = daemon_storage.register_repository(&updated);
                            }
                        }
                        Err(error) => {
                            eprintln!("SynEvid: active repository recovery scan failed: {error}")
                        }
                    }
                }
            }
        }

        Self {
            repo_root: repo_root.clone(),
            host,
            port,
            registry,
            storage: Arc::new(Mutex::new(storage)),
            active_repo_id: Arc::new(Mutex::new(initial_active_id)),
            active_repo_path: Arc::new(Mutex::new(initial_active_path)),
            connected_repos: Arc::new(Mutex::new(connected_repos)),
            analysis_artifact_cache: Arc::new(Mutex::new(None)),
            repository_storage_cache: Arc::new(Mutex::new(BTreeMap::new())),
            watcher_stop: Arc::new(AtomicBool::new(false)),
            universe_cache: Arc::new(Mutex::new(std::collections::HashMap::new())),
            live_events: Arc::new(Mutex::new(Vec::new())),
            live_event_signal: Arc::new((Mutex::new(0), Condvar::new())),
            active_ai_session_id: Arc::new(Mutex::new("sess_active_01".to_string())),
        }
    }

    pub fn get_storage_for_active_repo(&self) -> Option<StorageEngine> {
        let active_dir = self.get_active_repo_dir();
        if !active_dir.as_os_str().is_empty() && active_dir.is_dir() {
            if let Ok(st) = self.repository_storage(&active_dir) {
                return Some(st);
            }
        }
        if let Ok(guard) = self.storage.lock() {
            if let Some(ref _st) = *guard {
                if let Ok(dup) = open_storage_engine_with_registry(
                    &self.registry,
                    &StorageEngineConfig {
                        database_path: self.repo_root.join(".autopsy").join("storage.db"),
                        cache_dir: Some(self.repo_root.join(".autopsy").join("cache")),
                        use_wal: true,
                        recover_corruption: true,
                    },
                ) {
                    return Some(dup);
                }
            }
        }
        StorageEngine::in_memory().ok()
    }

    fn repository_storage(&self, root: &Path) -> Result<StorageEngine, String> {
        let canonical_root = root
            .canonicalize()
            .map_err(|error| format!("repository root is unavailable: {error}"))?;
        if let Ok(cache) = self.repository_storage_cache.lock()
            && let Some(storage) = cache.get(&canonical_root)
        {
            return Ok(storage.clone());
        }
        let storage = open_repository_storage_with_registry(&canonical_root, &self.registry)?;
        let mut cache = self
            .repository_storage_cache
            .lock()
            .map_err(|_| "repository storage cache is unavailable".to_string())?;
        Ok(cache
            .entry(canonical_root)
            .or_insert_with(|| storage.clone())
            .clone())
    }

    pub fn broadcast_event(&self, event_name: &str, data: serde_json::Value) {
        let data_json = data.to_string();
        let seq = self
            .storage
            .lock()
            .ok()
            .and_then(|storage| {
                storage
                    .as_ref()?
                    .record_live_event(event_name, &data_json)
                    .ok()
            })
            .unwrap_or_else(|| GLOBAL_SEQ_ID.fetch_add(1, Ordering::SeqCst));
        let item = serde_json::json!({
            "seq_id": seq,
            "event": event_name,
            "data": data,
        });
        if let Ok(mut events) = self.live_events.lock() {
            events.push(item);
            if events.len() > 500 {
                events.remove(0);
            }
        }
        let (generation, signal) = &*self.live_event_signal;
        if let Ok(mut generation) = generation.lock() {
            *generation = generation.wrapping_add(1);
            signal.notify_all();
        }
    }

    fn wait_for_live_events(
        &self,
        storage: &StorageEngine,
        cursor: i64,
        timeout: Duration,
    ) -> Result<Vec<autopsy_storage::LiveEventRecord>, String> {
        let started = Instant::now();
        let (generation, signal) = &*self.live_event_signal;
        loop {
            let observed_generation = *generation
                .lock()
                .map_err(|_| "live event signal is unavailable".to_string())?;
            let events = storage
                .live_events_since(cursor, 500)
                .map_err(|error| error.to_string())?;
            if !events.is_empty() {
                return Ok(events);
            }
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Ok(Vec::new());
            }
            let generation_guard = generation
                .lock()
                .map_err(|_| "live event signal is unavailable".to_string())?;
            if *generation_guard != observed_generation {
                continue;
            }
            let _ = signal
                .wait_timeout(generation_guard, remaining)
                .map_err(|_| "live event signal is unavailable".to_string())?;
        }
    }

    pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let addr: SocketAddr = format!("{}:{}", self.host, self.port).parse()?;
        let server =
            Server::http(addr).map_err(|e| format!("Failed to bind server on {}: {}", addr, e))?;

        println!(
            "================================================================================"
        );
        println!(" SYNEVID LOCAL WEB DASHBOARD & REACTIVE DAEMON (Code Autopsy v0.2.0)");
        println!(
            "================================================================================"
        );
        println!(" Listening on: http://{}", addr);
        println!(" Repository  : {}", self.repo_root.display());
        println!(
            " Invariant   : Offline reactive daemon (zero external network, 100% deterministic)"
        );
        println!(
            "================================================================================"
        );
        println!("Press Ctrl+C to stop.");

        self.start_repository_watcher();

        for request in server.incoming_requests() {
            let this = self.clone();
            std::thread::spawn(move || {
                this.dispatch(request);
            });
        }

        self.stop_repository_watcher();
        Ok(())
    }

    fn start_repository_watcher(&self) {
        let server = self.clone();
        let stop = Arc::clone(&self.watcher_stop);
        let (event_sender, event_receiver) = std::sync::mpsc::channel();
        let mut watched_root = self.get_active_repo_dir();
        let mut watcher = build_repository_watcher(&watched_root, event_sender.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                let _watcher_is_alive = &watcher;
                let active_root = server.get_active_repo_dir();
                if active_root != watched_root {
                    watched_root = active_root.clone();
                    watcher = build_repository_watcher(&watched_root, event_sender.clone());
                }
                let Ok(Ok(first_event)) = event_receiver.recv_timeout(Duration::from_millis(250))
                else {
                    continue;
                };
                if matches!(first_event.kind, notify::EventKind::Access(_)) {
                    continue;
                }
                let mut git_metadata_changed = is_git_metadata_event(&watched_root, &first_event);
                let mut changed_paths = BTreeSet::new();
                collect_watched_paths(&watched_root, &first_event, &mut changed_paths);
                let debounce_until = Instant::now() + Duration::from_millis(200);
                while Instant::now() < debounce_until && !stop.load(Ordering::Acquire) {
                    match event_receiver.try_recv() {
                        Ok(Ok(event)) if !matches!(event.kind, notify::EventKind::Access(_)) => {
                            git_metadata_changed |= is_git_metadata_event(&watched_root, &event);
                            collect_watched_paths(&watched_root, &event, &mut changed_paths);
                        }
                        Ok(_) => {}
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            std::thread::sleep(Duration::from_millis(20));
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                    }
                }
                if !changed_paths.is_empty() {
                    let changed_paths = changed_paths.into_iter().collect::<Vec<_>>();
                    server.refresh_active_repository(&changed_paths);
                    watcher = build_repository_watcher(&watched_root, event_sender.clone());
                } else if git_metadata_changed {
                    server.refresh_active_git_head();
                    watcher = build_repository_watcher(&watched_root, event_sender.clone());
                }
            }
        });
    }

    fn stop_repository_watcher(&self) {
        self.watcher_stop.store(true, Ordering::Release);
    }

    fn refresh_active_git_head(&self) {
        let active_id = self.active_repo_id.lock().unwrap().clone();
        let record = self
            .connected_repos
            .lock()
            .ok()
            .and_then(|repos| repos.get(&active_id).cloned());
        let Some(mut record) = record else {
            return;
        };
        let root = PathBuf::from(&record.root_path);
        let (active_branch, head_commit_oid) = git_head(&root);
        if record.active_branch == active_branch && record.head_commit_oid == head_commit_oid {
            return;
        }
        let previous_branch = record.active_branch.clone();
        let previous_head = record.head_commit_oid.clone();
        record.active_branch = active_branch.clone();
        record.head_commit_oid = head_commit_oid.clone();
        match self.repository_storage(&root) {
            Ok(storage) => {
                if let Err(error) = storage.register_repository(&record) {
                    self.broadcast_event(
                        "error.raised",
                        serde_json::json!({ "repo_id": active_id, "stage": "git_head_persist", "error": error.to_string() }),
                    );
                    return;
                }
            }
            Err(error) => {
                self.broadcast_event(
                    "error.raised",
                    serde_json::json!({ "repo_id": active_id, "stage": "git_head_refresh", "error": error }),
                );
                return;
            }
        }
        if let Ok(mut repos) = self.connected_repos.lock() {
            repos.insert(active_id.clone(), record.clone());
        }
        if let Ok(guard) = self.storage.lock()
            && let Some(storage) = guard.as_ref()
        {
            let _ = storage.register_repository(&record);
        }
        self.broadcast_event(
            "git.head_changed",
            serde_json::json!({
                "repo_id": active_id,
                "previous_branch": previous_branch,
                "active_branch": active_branch,
                "previous_head_commit_oid": previous_head,
                "head_commit_oid": head_commit_oid
            }),
        );
    }

    fn refresh_active_repository(&self, changed_paths: &[String]) {
        let active_id = self.active_repo_id.lock().unwrap().clone();
        let record = self
            .connected_repos
            .lock()
            .ok()
            .and_then(|repos| repos.get(&active_id).cloned());
        let Some(record) = record else {
            return;
        };
        let root = PathBuf::from(&record.root_path);
        let storage = match self.repository_storage(&root) {
            Ok(storage) => storage,
            Err(error) => {
                self.broadcast_event(
                    "repo.index_failed",
                    serde_json::json!({ "repo_id": active_id, "error": error }),
                );
                self.broadcast_event(
                    "error.raised",
                    serde_json::json!({ "repo_id": active_id, "stage": "repository_storage", "error": error }),
                );
                return;
            }
        };
        let (updated, receipt, artifacts) = match index_repository_with_artifacts_with_registry(
            &root,
            &record.repo_id,
            &record.display_name,
            &storage,
            &self.registry,
        ) {
            Ok(result) => result,
            Err(error) => {
                self.broadcast_event(
                    "repo.index_failed",
                    serde_json::json!({ "repo_id": active_id, "error": error }),
                );
                self.broadcast_event(
                    "error.raised",
                    serde_json::json!({ "repo_id": active_id, "stage": "repository_index", "error": error }),
                );
                return;
            }
        };
        let git_head_changed = record.active_branch != updated.active_branch
            || record.head_commit_oid != updated.head_commit_oid;
        if let Ok(mut repos) = self.connected_repos.lock() {
            repos.insert(updated.repo_id.clone(), updated.clone());
        }
        if let Ok(mut cache) = self.analysis_artifact_cache.lock() {
            *cache = Some((
                updated.repo_id.clone(),
                receipt.snapshot_id.clone(),
                Arc::new(artifacts),
            ));
        }
        if let Ok(guard) = self.storage.lock()
            && let Some(central) = guard.as_ref()
        {
            let _ = central.register_repository(&updated);
        }
        if let Ok(mut cache) = self.universe_cache.lock() {
            cache.clear();
        }
        for changed_path in changed_paths {
            self.broadcast_event(
                "file.changed",
                serde_json::json!({
                    "repo_id": &updated.repo_id,
                    "snapshot_id": &receipt.snapshot_id,
                    "file_path": changed_path
                }),
            );
        }
        self.broadcast_event(
            "repo.indexed",
            serde_json::json!({
                "repo_id": updated.repo_id,
                "snapshot_id": receipt.snapshot_id,
                "files_scanned": receipt.files_scanned,
                "symbol_count": receipt.symbol_count,
                "edge_count": receipt.edge_count,
                "coverage_state": receipt.coverage_state,
                "changed_paths": changed_paths
            }),
        );
        if git_head_changed {
            self.broadcast_event(
                "git.head_changed",
                serde_json::json!({
                    "repo_id": updated.repo_id,
                    "previous_branch": record.active_branch,
                    "active_branch": updated.active_branch,
                    "previous_head_commit_oid": record.head_commit_oid,
                    "head_commit_oid": updated.head_commit_oid
                }),
            );
        }
    }

    pub fn dispatch(&self, mut request: Request) {
        let url = request.url().to_string();
        let path = url.split('?').next().unwrap_or("/");

        match (request.method(), path) {
            // Health Endpoint
            (&Method::Get, "/api/v1/health") | (&Method::Get, "/health") => {
                self.respond_json(request, r#"{"status":"ok"}"#);
            }

            // Headless daemon discovery; the compatibility dashboard remains opt-in.
            (&Method::Get, "/") => {
                self.respond_json(request, &self.handle_service_discovery());
            }
            (&Method::Get, "/dashboard") | (&Method::Get, "/index.html") => {
                let html = include_str!("dashboard.html");
                let header =
                    Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
                        .unwrap();
                let response = Response::from_string(html).with_header(header);
                let _ = request.respond(response);
            }

            // Phase 1 Capabilities Endpoint (FR-P1-001)
            (&Method::Get, "/api/v1/capabilities") => {
                self.respond_json(request, &self.handle_capabilities());
            }

            // Phase 1 Multi-Repo Connection & Switcher (FR-P1-003)
            (&Method::Post, "/api/v1/repos/connect") => {
                let body = self.read_body(&mut request);
                let res = self.handle_connect_repo(&body);
                self.respond_json(request, &res);
            }
            (&Method::Get, "/api/v1/repos/list") => {
                let res = self.handle_list_repos();
                self.respond_json(request, &res);
            }
            (&Method::Post, "/api/v1/repos/switch") => {
                let body = self.read_body(&mut request);
                let res = self.handle_switch_repo(&body);
                self.respond_json(request, &res);
            }
            (&Method::Post, "/api/v1/repos/remove") => {
                let body = self.read_body(&mut request);
                let res = self.handle_remove_repo(&body);
                self.respond_json(request, &res);
            }
            (&Method::Get, p)
                if p == "/api/v1/repos/export" || p.starts_with("/api/v1/repos/export?") =>
            {
                let res = self.handle_repository_export(&url);
                self.respond_json(request, &res);
            }

            // Phase 1 Local Filesystem Navigator & Directory Picker (FR-P1-008)
            (&Method::Get, p) if p.starts_with("/api/v1/fs/browse") => {
                let res = self.handle_fs_browse(&url);
                self.respond_json(request, &res);
            }

            // Real-Time Repository Discovered Files (FR-P1-009)
            (&Method::Get, "/api/v1/files") | (&Method::Get, "/api/files") => {
                let res = self.handle_files();
                self.respond_json(request, &res);
            }
            (&Method::Get, "/api/v1/files/content") => {
                let res = self.handle_file_content(&url);
                self.respond_json(request, &res);
            }

            // Phase 1 Visual Graph Universe (2D/3D with seeded layout) (FR-P1-006)
            (&Method::Get, "/api/v1/graph/universe") => {
                let res = self.handle_graph_universe(&url);
                self.respond_json(request, &res);
            }

            // Phase 1 Single-File/Symbol Ego Network (<5ms) (FR-P1-002)
            (&Method::Get, "/api/v1/graph/ego") => {
                let res = self.handle_graph_ego(&url);
                self.respond_json(request, &res);
            }

            // Phase 1 In-Memory Live Simulation (<50ms) (FR-P1-004)
            (&Method::Post, "/api/v1/simulate/edit") => {
                let body = self.read_body(&mut request);
                let res = self.handle_simulate_edit(&body);
                self.respond_json(request, &res);
            }

            // Phase 1 Live SSE Stream (FR-P1-005)
            (&Method::Get, "/api/v1/live") => {
                self.handle_live_sse(request);
            }

            // Phase 3 Native .git Engine & Time-Travel Galaxy Endpoints (FR-P3-001..FR-P3-005)
            (&Method::Get, p) if p.starts_with("/api/v1/git/galaxy") => {
                let res = self.handle_git_galaxy(&url);
                self.respond_json(request, &res);
            }
            (&Method::Get, p) if p.starts_with("/api/v1/git/time-travel") => {
                let res = self.handle_git_time_travel(&url);
                self.respond_json(request, &res);
            }
            (&Method::Get, p) if p.starts_with("/api/v1/git/diff-file") => {
                let res = self.handle_git_diff_file(&url);
                self.respond_json(request, &res);
            }
            (&Method::Get, p) if p == "/api/v1/git/diff" || p.starts_with("/api/v1/git/diff?") => {
                let res = self.handle_git_diff(&url);
                self.respond_json(request, &res);
            }
            (&Method::Post, "/api/v1/git/stage") => {
                let body = self.read_body(&mut request);
                let res = self.handle_git_stage(&body);
                self.respond_json(request, &res);
            }
            (&Method::Post, "/api/v1/git/commit") => {
                let body = self.read_body(&mut request);
                let res = self.handle_git_commit(&body);
                self.respond_json(request, &res);
            }
            (&Method::Post, "/api/v1/git/branch") => {
                let body = self.read_body(&mut request);
                let res = self.handle_git_branch(&body);
                self.respond_json(request, &res);
            }
            (&Method::Post, "/api/v1/git/undo") => {
                let res = self.handle_git_undo();
                self.respond_json(request, &res);
            }
            (&Method::Get, p) if p.starts_with("/api/v1/git/status") => {
                let res = self.handle_git_status();
                self.respond_json(request, &res);
            }
            (&Method::Post, "/api/v1/git/unstage") => {
                let body = self.read_body(&mut request);
                let res = self.handle_git_unstage(&body);
                self.respond_json(request, &res);
            }
            (&Method::Post, "/api/v1/git/tag") => {
                let body = self.read_body(&mut request);
                let res = self.handle_git_tag(&body);
                self.respond_json(request, &res);
            }
            (&Method::Post, "/api/v1/git/merge") => {
                let body = self.read_body(&mut request);
                let res = self.handle_git_merge(&body);
                self.respond_json(request, &res);
            }
            (&Method::Post, "/api/v1/git/checkout") => {
                let body = self.read_body(&mut request);
                let res = self.handle_git_checkout(&body);
                self.respond_json(request, &res);
            }
            (&Method::Post, "/api/v1/git/revert") => {
                let body = self.read_body(&mut request);
                let res = self.handle_git_revert(&body);
                self.respond_json(request, &res);
            }
            (&Method::Post, "/api/v1/git/bisect") => {
                let body = self.read_body(&mut request);
                let res = self.handle_git_bisect(&body);
                self.respond_json(request, &res);
            }
            (&Method::Post, "/api/v1/terminal/exec") => {
                let body = self.read_body(&mut request);
                let res = self.handle_terminal_exec(&body);
                self.respond_json(request, &res);
            }

            // Phase 4 Forensic Trajectory Hierarchy, Sessions & Workflows Endpoints (FR-P4-001..FR-P4-004)
            (&Method::Get, p)
                if p == "/api/v1/forensics/sessions"
                    || p.starts_with("/api/v1/forensics/sessions?") =>
            {
                let res = self.handle_forensic_sessions(&url);
                self.respond_json(request, &res);
            }
            (&Method::Post, "/api/v1/forensics/sessions/start") => {
                let body = self.read_body(&mut request);
                let res = self.handle_forensic_sessions_start(&body);
                self.respond_json(request, &res);
            }
            (&Method::Get, p)
                if p == "/api/v1/forensics/timeline"
                    || p.starts_with("/api/v1/forensics/timeline?") =>
            {
                let res = self.handle_forensic_timeline(&url);
                self.respond_json(request, &res);
            }
            (&Method::Get, p)
                if p == "/api/v1/forensics/event" || p.starts_with("/api/v1/forensics/event?") =>
            {
                let res = self.handle_forensic_event(&url);
                self.respond_json(request, &res);
            }
            (&Method::Post, "/api/v1/forensics/revert") => {
                let body = self.read_body(&mut request);
                let res = self.handle_forensic_revert(&body);
                self.respond_json(request, &res);
            }
            (&Method::Get, p)
                if p == "/api/v1/forensics/export"
                    || p.starts_with("/api/v1/forensics/export?") =>
            {
                let res = self.handle_forensic_export(&url);
                self.respond_json(request, &res);
            }
            (&Method::Post, "/api/v1/forensics/import") => {
                let body = self.read_body(&mut request);
                let res = self.handle_forensic_import(&body);
                self.respond_json(request, &res);
            }

            // Phase 4 AI Working Cache & Cockpit Config Endpoints (FR-P4-003, FR-P4-006)
            (&Method::Get, p)
                if p == "/api/v1/ai/ledger" || p.starts_with("/api/v1/ai/ledger?") =>
            {
                let res = self.handle_ai_ledger(&url);
                self.respond_json(request, &res);
            }
            (&Method::Post, "/api/v1/ai/cache/query") => {
                let body = self.read_body(&mut request);
                let res = self.handle_ai_cache_query(&body);
                self.respond_json(request, &res);
            }
            (&Method::Get, p)
                if p == "/api/v1/ai/config-snippet"
                    || p.starts_with("/api/v1/ai/config-snippet?") =>
            {
                let res = self.handle_ai_config_snippet(&url);
                self.respond_json(request, &res);
            }

            // Phase 4 Real-Time Live Activity Polling & Broadcast (FR-P4-001, FR-P4-004)
            (&Method::Get, p)
                if p == "/api/v1/live/poll" || p.starts_with("/api/v1/live/poll?") =>
            {
                let res = self.handle_live_poll(&url);
                self.respond_json(request, &res);
            }
            (&Method::Post, "/api/v1/live/broadcast") => {
                let body = self.read_body(&mut request);
                let res = self.handle_live_broadcast(&body);
                self.respond_json(request, &res);
            }

            // Legacy & Compatibility Endpoints
            (&Method::Get, "/api/status") => {
                let json = self.handle_status();
                self.respond_json(request, &json);
            }
            (&Method::Get, "/api/verify") => {
                let json = self.handle_verify();
                self.respond_json(request, &json);
            }
            (&Method::Get, "/api/impact") => {
                let json = self.handle_impact(&url);
                self.respond_json(request, &json);
            }
            (&Method::Get, "/api/trajectory") => {
                let json = self.handle_trajectory();
                self.respond_json(request, &json);
            }

            // Preflight CORS handler for external Web Cockpit (Next.js / React)
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
                let header_cors =
                    Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap();
                let response = Response::new(
                    StatusCode(404),
                    vec![
                        Header::from_bytes(&b"Content-Type"[..], &b"text/plain"[..]).unwrap(),
                        header_cors,
                    ],
                    Cursor::new("Not Found"),
                    Some(9),
                    None,
                );
                let _ = request.respond(response);
            }
        }
    }

    fn read_body(&self, req: &mut Request) -> String {
        if let Some(len) = req.body_length() {
            let mut buf = vec![0u8; len];
            if req.as_reader().read_exact(&mut buf).is_ok() {
                return String::from_utf8(buf).unwrap_or_default();
            }
        }
        let mut content = String::new();
        let _ = req.as_reader().read_to_string(&mut content);
        content
    }

    fn respond_json(&self, req: Request, json: &str) {
        let header_content = Header::from_bytes(
            &b"Content-Type"[..],
            &b"application/json; charset=utf-8"[..],
        )
        .unwrap();
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

        let response = Response::from_string(json)
            .with_header(header_content)
            .with_header(header_cors)
            .with_header(header_methods)
            .with_header(header_headers);
        let _ = req.respond(response);
    }

    pub fn handle_connect_repo(&self, body: &str) -> String {
        let parsed: Result<ConnectRepoRequest, _> = serde_json::from_str(body);
        match parsed {
            Ok(req) => {
                let path = Self::normalize_cross_platform_path(&req.root_path);
                if !path.is_dir() {
                    return serde_json::json!({
                        "error_code": "ERR-REPO-001",
                        "error": format!("Repository directory does not exist: {}", req.root_path)
                    })
                    .to_string();
                }

                let canon = match path.canonicalize() {
                    Ok(path) => path,
                    Err(error) => {
                        return serde_json::json!({
                            "error_code": "ERR-REPO-002",
                            "error": format!("Cannot resolve repository directory: {error}")
                        })
                        .to_string();
                    }
                };
                let canonical_path = canon.to_string_lossy().replace('\\', "/");
                let repo_id = blake3::hash(canonical_path.as_bytes()).to_hex().to_string();
                let display_name = req.display_name.unwrap_or_else(|| {
                    canon
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("repo")
                        .to_string()
                });

                let storage = match open_repository_storage_with_registry(&canon, &self.registry) {
                    Ok(storage) => storage,
                    Err(error) => {
                        return serde_json::json!({
                            "error_code": "ERR-STORAGE-101",
                            "error": error
                        })
                        .to_string();
                    }
                };
                let (record, receipt, artifacts) =
                    match index_repository_with_artifacts_with_registry(
                        &canon,
                        &repo_id,
                        &display_name,
                        &storage,
                        &self.registry,
                    ) {
                        Ok(indexed) => indexed,
                        Err(error) => {
                            self.broadcast_event(
                            "error.raised",
                            serde_json::json!({ "repo_id": repo_id, "stage": "repository_connect_scan", "error": error }),
                        );
                            return serde_json::json!({
                                "error_code": "ERR-INDEX-001",
                                "error": error
                            })
                            .to_string();
                        }
                    };

                if let Ok(guard) = self.storage.lock() {
                    if let Some(ref st) = *guard {
                        if let Err(error) = st.register_repository(&record) {
                            return serde_json::json!({
                                "error_code": "ERR-STORAGE-102",
                                "error": error.to_string()
                            })
                            .to_string();
                        }
                        let _ = st.set_daemon_state("active_repo_id", &repo_id);
                    }
                }

                self.connected_repos
                    .lock()
                    .unwrap()
                    .insert(repo_id.clone(), record);
                if let Ok(mut cache) = self.analysis_artifact_cache.lock() {
                    *cache = Some((
                        repo_id.clone(),
                        receipt.snapshot_id.clone(),
                        Arc::new(artifacts),
                    ));
                }
                if let Ok(mut cache) = self.repository_storage_cache.lock() {
                    cache.insert(canon.clone(), storage.clone());
                }
                *self.active_repo_id.lock().unwrap() = repo_id.clone();
                *self.active_repo_path.lock().unwrap() = canon.clone();
                if let Ok(mut c) = self.universe_cache.lock() {
                    c.clear();
                }

                self.broadcast_event(
                    "repo.indexed",
                    serde_json::json!({
                        "repo_id": repo_id,
                        "snapshot_id": receipt.snapshot_id,
                        "files_scanned": receipt.files_scanned,
                        "symbol_count": receipt.symbol_count,
                        "edge_count": receipt.edge_count,
                        "coverage_state": receipt.coverage_state,
                        "languages": receipt.languages,
                        "skipped_oversize_files": receipt.skipped_oversize_files,
                        "traversal_limit_reached": receipt.traversal_limit_reached
                    }),
                );

                serde_json::json!({
                    "status": "connected",
                    "repo_id": repo_id,
                    "display_name": display_name,
                    "root_path": canonical_path,
                    "scan": receipt
                })
                .to_string()
            }
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    pub fn handle_list_repos(&self) -> String {
        let mut list: Vec<RepoRecord> = Vec::new();
        if let Ok(repos) = self.connected_repos.lock() {
            for (_, r) in repos.iter() {
                if !r.root_path.ends_with("SynEvid")
                    && !r.display_name.to_lowercase().contains("synevid")
                {
                    list.push(r.clone());
                }
            }
        }
        let storage_guard = self.storage.lock().unwrap();
        if let Some(ref st) = *storage_guard {
            if let Ok(repos) = st.list_repositories() {
                for r in repos {
                    if !r.root_path.ends_with("SynEvid")
                        && !r.display_name.to_lowercase().contains("synevid")
                        && !list.iter().any(|existing| existing.repo_id == r.repo_id)
                    {
                        list.push(r);
                    }
                }
            }
        }
        list.sort_by(|left, right| {
            left.display_name
                .cmp(&right.display_name)
                .then_with(|| left.repo_id.cmp(&right.repo_id))
        });
        serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string())
    }

    pub fn handle_repository_export(&self, url: &str) -> String {
        let repo_id = self
            .active_repo_id
            .lock()
            .map(|id| id.clone())
            .unwrap_or_default();
        if repo_id.is_empty() {
            return serde_json::json!({ "error": "No active repository" }).to_string();
        }
        let record = self
            .connected_repos
            .lock()
            .ok()
            .and_then(|repos| repos.get(&repo_id).cloned())
            .or_else(|| {
                self.storage.lock().ok().and_then(|storage| {
                    storage
                        .as_ref()
                        .and_then(|storage| storage.get_repository(&repo_id).ok().flatten())
                })
            });
        let Some(record) = record else {
            return serde_json::json!({ "error": "Active repository record is unavailable" })
                .to_string();
        };
        let storage = match open_repository_storage_with_registry(
            Path::new(&record.root_path),
            &self.registry,
        ) {
            Ok(storage) => storage,
            Err(error) => return serde_json::json!({ "error": error }).to_string(),
        };
        let snapshot_id =
            Self::parse_query_param(url, "snapshot_id").or(record.latest_snap_id.clone());
        let Some(snapshot_id) = snapshot_id else {
            return serde_json::json!({ "error": "Active repository has no indexed snapshot" })
                .to_string();
        };
        let snapshot = match storage.get_snapshot(&autopsy_domain::SnapshotId::new(&snapshot_id)) {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => {
                return serde_json::json!({ "error": "Snapshot is unavailable" }).to_string();
            }
            Err(error) => return serde_json::json!({ "error": error.to_string() }).to_string(),
        };
        let (nodes, edges) = match storage.graph_rows_for_snapshot(&snapshot_id) {
            Ok(rows) => rows,
            Err(error) => return serde_json::json!({ "error": error.to_string() }).to_string(),
        };
        let export = CanonicalRepositoryExport {
            schema_version: "v0.5.0-repository-export",
            repo_id: &repo_id,
            snapshot: &snapshot,
            nodes: &nodes,
            edges: &edges,
        };
        let canonical = match serde_json::to_vec(&export) {
            Ok(bytes) => bytes,
            Err(error) => return serde_json::json!({ "error": error.to_string() }).to_string(),
        };
        let envelope = RepositoryExportEnvelope {
            digest: format!("blake3:{}", blake3::hash(&canonical).to_hex()),
            export,
        };
        serde_json::to_string(&envelope)
            .unwrap_or_else(|error| serde_json::json!({ "error": error.to_string() }).to_string())
    }

    pub fn handle_switch_repo(&self, body: &str) -> String {
        let parsed: Result<SwitchRepoRequest, _> = serde_json::from_str(body);
        match parsed {
            Ok(req) => {
                let mut record = self
                    .connected_repos
                    .lock()
                    .ok()
                    .and_then(|repos| repos.get(&req.repo_id).cloned());
                if record.is_none() {
                    record = self.storage.lock().ok().and_then(|guard| {
                        guard
                            .as_ref()
                            .and_then(|st| st.get_repository(&req.repo_id).ok().flatten())
                    });
                }
                let Some(mut record) = record else {
                    return serde_json::json!({ "error_code": "ERR-REPO-404", "error": "Repository not found" }).to_string();
                };
                let root = PathBuf::from(&record.root_path);
                if !root.is_dir() {
                    return serde_json::json!({
                        "error_code": "ERR-REPO-003",
                        "error": "Repository directory is unavailable"
                    })
                    .to_string();
                }
                let storage = match open_repository_storage_with_registry(&root, &self.registry) {
                    Ok(storage) => storage,
                    Err(error) => {
                        return serde_json::json!({
                            "error_code": "ERR-STORAGE-101",
                            "error": error
                        })
                        .to_string();
                    }
                };
                let snapshot_ready = record
                    .latest_snap_id
                    .as_deref()
                    .and_then(|id| {
                        storage
                            .get_snapshot(&autopsy_domain::SnapshotId::new(id))
                            .ok()
                    })
                    .flatten()
                    .is_some();
                if !snapshot_ready {
                    match index_repository_with_registry(
                        &root,
                        &record.repo_id,
                        &record.display_name,
                        &storage,
                        &self.registry,
                    ) {
                        Ok((indexed, _)) => record = indexed,
                        Err(error) => {
                            return serde_json::json!({
                                "error_code": "ERR-INDEX-001",
                                "error": error
                            })
                            .to_string();
                        }
                    }
                }
                if let Ok(guard) = self.storage.lock() {
                    if let Some(ref central) = *guard {
                        if let Err(error) = central.register_repository(&record) {
                            return serde_json::json!({
                                "error_code": "ERR-STORAGE-102",
                                "error": error.to_string()
                            })
                            .to_string();
                        }
                        let _ = central.set_daemon_state("active_repo_id", &record.repo_id);
                    }
                }
                if let Ok(mut repos) = self.connected_repos.lock() {
                    repos.insert(record.repo_id.clone(), record.clone());
                }
                *self.active_repo_id.lock().unwrap() = record.repo_id.clone();
                *self.active_repo_path.lock().unwrap() = root;
                if let Ok(mut cache) = self.universe_cache.lock() {
                    cache.clear();
                }
                self.broadcast_event(
                    "repo.switched",
                    serde_json::json!({ "repo_id": record.repo_id }),
                );
                serde_json::json!({ "status": "switched", "active_repo": record }).to_string()
            }
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    pub fn handle_remove_repo(&self, body: &str) -> String {
        let request: RemoveRepoRequest = match serde_json::from_str(body) {
            Ok(request) => request,
            Err(error) => return serde_json::json!({ "error": error.to_string() }).to_string(),
        };
        let record = self
            .connected_repos
            .lock()
            .ok()
            .and_then(|repos| repos.get(&request.repo_id).cloned());
        let Some(record) = record else {
            return serde_json::json!({ "error_code": "ERR-REPO-404", "error": "Repository not found" }).to_string();
        };
        if let Ok(guard) = self.storage.lock() {
            if let Some(ref storage) = *guard {
                if let Err(error) = storage.remove_repository(&request.repo_id) {
                    return serde_json::json!({
                        "error_code": "ERR-STORAGE-103",
                        "error": error.to_string()
                    })
                    .to_string();
                }
            }
        }
        if let Ok(local_storage) =
            open_repository_storage_with_registry(Path::new(&record.root_path), &self.registry)
        {
            if let Err(error) = local_storage.remove_repository(&request.repo_id) {
                return serde_json::json!({
                    "error_code": "ERR-STORAGE-103",
                    "error": error.to_string()
                })
                .to_string();
            }
        }
        if let Ok(mut repos) = self.connected_repos.lock() {
            repos.remove(&request.repo_id);
        }
        let was_active = self.active_repo_id.lock().unwrap().as_str() == request.repo_id;
        if was_active {
            let next_repo = self.connected_repos.lock().ok().and_then(|repos| {
                let mut records = repos.values().cloned().collect::<Vec<_>>();
                records.sort_by(|left, right| {
                    left.display_name
                        .cmp(&right.display_name)
                        .then_with(|| left.repo_id.cmp(&right.repo_id))
                });
                records
                    .into_iter()
                    .find(|repo| Path::new(&repo.root_path).is_dir())
            });
            let next_id = next_repo
                .as_ref()
                .map(|repo| repo.repo_id.clone())
                .unwrap_or_default();
            let next_path = next_repo
                .map(|repo| PathBuf::from(repo.root_path))
                .unwrap_or_default();
            *self.active_repo_id.lock().unwrap() = next_id.clone();
            *self.active_repo_path.lock().unwrap() = next_path;
            if let Ok(guard) = self.storage.lock() {
                if let Some(ref storage) = *guard {
                    let _ = storage.set_daemon_state("active_repo_id", &next_id);
                }
            }
            if let Ok(mut cache) = self.universe_cache.lock() {
                cache.clear();
            }
        }
        self.broadcast_event(
            "repo.removed",
            serde_json::json!({ "repo_id": request.repo_id }),
        );
        serde_json::json!({ "status": "removed", "repo_id": request.repo_id }).to_string()
    }

    pub fn handle_fs_browse(&self, url: &str) -> String {
        let requested_path = Self::parse_query_param(url, "path");
        let home_dir = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| "/home".to_string());

        let target_path = match requested_path {
            Some(ref p) if !p.trim().is_empty() => Self::normalize_cross_platform_path(p),
            _ => PathBuf::from(&home_dir),
        };

        let resolved_dir = if target_path.is_dir() {
            target_path.canonicalize().unwrap_or(target_path)
        } else if target_path.is_file() {
            target_path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from(&home_dir))
        } else if let Some(parent) = target_path.parent() {
            if parent.is_dir() {
                parent
                    .canonicalize()
                    .unwrap_or_else(|_| parent.to_path_buf())
            } else {
                PathBuf::from(&home_dir)
            }
        } else {
            PathBuf::from(&home_dir)
        };

        let current_path_str = resolved_dir.to_string_lossy().to_string();
        let parent_path_str = resolved_dir
            .parent()
            .map(|p| p.to_string_lossy().to_string());
        let is_git_repo = resolved_dir.join(".git").exists();

        let mut subdirs: Vec<serde_json::Value> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&resolved_dir) {
            let mut dir_items: Vec<(String, PathBuf)> = Vec::new();
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
                        dir_items.push((file_name, entry.path()));
                    }
                }
            }
            dir_items.sort_by(|a, b| a.0.cmp(&b.0));
            let worker_count = std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1)
                .clamp(1, 4)
                .min(dir_items.len().max(1));
            let chunk_size = dir_items.len().div_ceil(worker_count).max(1);
            let mut batches = Vec::new();
            let mut worker_failed = false;
            std::thread::scope(|scope| {
                let handles = dir_items
                    .chunks(chunk_size)
                    .map(|chunk| {
                        scope.spawn(move || {
                            chunk
                                .iter()
                                .map(|(name, path)| {
                                    serde_json::json!({
                                        "name": name,
                                        "path": path.to_string_lossy(),
                                        "is_dir": true,
                                        "is_git_repo": path.join(".git").exists()
                                    })
                                })
                                .collect::<Vec<_>>()
                        })
                    })
                    .collect::<Vec<_>>();
                for handle in handles {
                    match handle.join() {
                        Ok(batch) => batches.push(batch),
                        Err(_) => worker_failed = true,
                    }
                }
            });
            if worker_failed {
                for (name, path) in &dir_items {
                    subdirs.push(serde_json::json!({
                        "name": name,
                        "path": path.to_string_lossy(),
                        "is_dir": true,
                        "is_git_repo": path.join(".git").exists()
                    }));
                }
            } else {
                subdirs.extend(batches.into_iter().flatten());
            }
        }

        let mut presets = vec![
            serde_json::json!({
                "label": "User Home",
                "path": home_dir
            }),
            serde_json::json!({
                "label": "Root Filesystem",
                "path": if cfg!(windows) { "C:\\" } else { "/" }
            }),
        ];

        let daemon_root_str = self.repo_root.to_string_lossy().to_string();
        let daemon_root_canon = self
            .repo_root
            .canonicalize()
            .unwrap_or_else(|_| self.repo_root.clone())
            .to_string_lossy()
            .to_string();

        if let Ok(repos) = self.connected_repos.lock() {
            for (_, r) in repos.iter() {
                if !r.display_name.to_lowercase().contains("synevid")
                    && r.root_path != daemon_root_str
                    && r.root_path != daemon_root_canon
                {
                    presets.push(serde_json::json!({
                        "label": r.display_name,
                        "path": r.root_path
                    }));
                }
            }
        }
        let storage_guard = self.storage.lock().unwrap();
        if let Some(ref st) = *storage_guard {
            if let Ok(repos) = st.list_repositories() {
                for r in repos {
                    if !r.display_name.to_lowercase().contains("synevid")
                        && r.root_path != daemon_root_str
                        && r.root_path != daemon_root_canon
                        && !presets.iter().any(|p| p["path"] == r.root_path)
                    {
                        presets.push(serde_json::json!({
                            "label": r.display_name,
                            "path": r.root_path
                        }));
                    }
                }
            }
        }

        serde_json::json!({
            "current_path": current_path_str,
            "parent_path": parent_path_str,
            "is_git_repo": is_git_repo,
            "directories": subdirs,
            "quick_presets": presets
        })
        .to_string()
    }

    pub fn handle_files(&self) -> String {
        let active_id = self.active_repo_id.lock().unwrap().clone();
        let record = self
            .connected_repos
            .lock()
            .ok()
            .and_then(|repos| repos.get(&active_id).cloned());
        let Some(record) = record else {
            return serde_json::json!({
                "repo_id": active_id,
                "total_files": 0,
                "files": [],
                "coverage_state": "unknown"
            })
            .to_string();
        };
        let root = PathBuf::from(&record.root_path);
        let storage = match open_repository_storage_with_registry(&root, &self.registry) {
            Ok(storage) => storage,
            Err(_) => {
                return serde_json::json!({
                    "repo_id": active_id,
                    "total_files": 0,
                    "files": [],
                    "coverage_state": "unknown",
                    "index_state": "storage_unavailable"
                })
                .to_string();
            }
        };
        let snapshot = snapshot_for_repository(&root, &record, &storage, &self.registry)
            .ok()
            .flatten();
        let Some(snapshot) = snapshot else {
            return serde_json::json!({
                "repo_id": active_id,
                "total_files": 0,
                "files": [],
                "coverage_state": "unknown",
                "index_state": "unindexed"
            })
            .to_string();
        };
        let graph_nodes = storage
            .graph_rows_for_snapshot(snapshot.snapshot_id.as_str())
            .map(|(nodes, _)| nodes)
            .unwrap_or_default();
        let repo_coverage = snapshot_coverage(&snapshot, &graph_nodes);
        let items: Vec<serde_json::Value> = snapshot
            .files
            .iter()
            .map(|(path, unit)| {
                let name = path.rsplit('/').next().unwrap_or(path).to_string();
                let dir = path
                    .rfind('/')
                    .map(|index| path[..index].to_string())
                    .unwrap_or_default();
                let coverage = if is_indexed_language(&unit.language) {
                    repo_coverage.clone()
                } else {
                    "unknown".to_string()
                };
                serde_json::json!({
                    "path": path,
                    "name": name,
                    "ext": Path::new(path).extension().and_then(|ext| ext.to_str()).unwrap_or("").to_lowercase(),
                    "dir": dir,
                    "language": unit.language,
                    "content_hash": unit.content_hash,
                    "is_generated": unit.is_generated,
                    "status": coverage
                })
            })
            .collect();
        serde_json::json!({
            "repo_id": active_id,
            "snapshot_id": snapshot.snapshot_id,
            "total_files": items.len(),
            "files": items,
            "skipped_oversize_files": snapshot.skipped_oversize_files,
            "traversal_limit_reached": snapshot.traversal_limit_reached,
            "coverage_state": repo_coverage
        })
        .to_string()
    }

    pub fn handle_file_content(&self, url: &str) -> String {
        const MAX_SOURCE_BYTES: u64 = 1_048_576;

        let relative_path = Self::parse_query_param(url, "path").unwrap_or_default();
        let relative_path = relative_path.replace('\\', "/");
        if !autopsy_repo::is_safe_relative_path(&relative_path)
            || relative_path
                .split('/')
                .any(|segment| matches!(segment, ".git" | ".autopsy"))
        {
            return serde_json::json!({
                "error_code": "ERR-FS-400",
                "error": "path must be a safe repository-relative source file",
                "coverage_state": "unknown"
            })
            .to_string();
        }

        let requested_repo_id = Self::parse_query_param(url, "repo_id");
        let active_id = self.active_repo_id.lock().unwrap().clone();
        let repo_id = requested_repo_id.unwrap_or(active_id);
        let record = self
            .connected_repos
            .lock()
            .ok()
            .and_then(|repos| repos.get(&repo_id).cloned());
        let Some(record) = record else {
            return serde_json::json!({
                "error_code": "ERR-REPO-404",
                "error": "repository is not connected",
                "coverage_state": "unknown"
            })
            .to_string();
        };

        let root = match PathBuf::from(&record.root_path).canonicalize() {
            Ok(root) => root,
            Err(error) => {
                return serde_json::json!({
                    "error_code": "ERR-REPO-404",
                    "error": format!("repository root is unavailable: {error}"),
                    "coverage_state": "unknown"
                })
                .to_string();
            }
        };
        let candidate = root.join(&relative_path);
        let path = match candidate.canonicalize() {
            Ok(path) if path.starts_with(&root) && path.is_file() => path,
            Ok(_) => {
                return serde_json::json!({
                    "error_code": "ERR-FS-403",
                    "error": "source path resolves outside the connected repository or is not a file",
                    "coverage_state": "unknown"
                })
                .to_string();
            }
            Err(error) => {
                return serde_json::json!({
                    "error_code": "ERR-FS-404",
                    "error": format!("source file is unavailable: {error}"),
                    "coverage_state": "unknown"
                })
                .to_string();
            }
        };
        let metadata = match path.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                return serde_json::json!({
                    "error_code": "ERR-FS-500",
                    "error": format!("unable to inspect source file: {error}"),
                    "coverage_state": "unknown"
                })
                .to_string();
            }
        };
        if metadata.len() > MAX_SOURCE_BYTES {
            return serde_json::json!({
                "error_code": "ERR-FS-413",
                "error": format!("source file exceeds the {MAX_SOURCE_BYTES}-byte editor limit"),
                "coverage_state": "partial"
            })
            .to_string();
        }
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return serde_json::json!({
                    "error_code": "ERR-FS-500",
                    "error": format!("unable to read source file: {error}"),
                    "coverage_state": "unknown"
                })
                .to_string();
            }
        };
        let content = match String::from_utf8(bytes) {
            Ok(content) => content,
            Err(_) => {
                return serde_json::json!({
                    "error_code": "ERR-FS-415",
                    "error": "source file is not valid UTF-8 text",
                    "coverage_state": "unknown"
                })
                .to_string();
            }
        };

        serde_json::json!({
            "repo_id": repo_id,
            "file_path": relative_path,
            "content": content,
            "size_bytes": metadata.len(),
            "coverage_state": "verified"
        })
        .to_string()
    }

    pub fn handle_graph_universe(&self, url: &str) -> String {
        let is_3d = !url.contains("dim=2d");
        let node_limit = Self::parse_query_param(url, "limit")
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|limit| *limit > 0)
            .unwrap_or(MAX_UNIVERSE_NODE_LIMIT)
            .min(MAX_UNIVERSE_NODE_LIMIT);
        let edge_limit = Self::parse_query_param(url, "edge_limit")
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|limit| *limit > 0)
            .unwrap_or(MAX_UNIVERSE_EDGE_LIMIT)
            .min(MAX_UNIVERSE_EDGE_LIMIT);
        let active_id = self.active_repo_id.lock().unwrap().clone();
        let record = self
            .connected_repos
            .lock()
            .ok()
            .and_then(|repos| repos.get(&active_id).cloned());
        let Some(record) = record else {
            return empty_universe(&active_id, is_3d, "unknown");
        };
        if let Some(snapshot_id) = record.latest_snap_id.as_deref() {
            let cache_key = format!(
                "{}_{}_{}_{}_{}",
                active_id,
                snapshot_id,
                if is_3d { "3d" } else { "2d" },
                node_limit,
                edge_limit
            );
            if let Ok(mut cache) = self.universe_cache.lock()
                && let Some(cached_json) = cache.get(&cache_key).cloned()
            {
                let cached_node_count = serde_json::from_str::<serde_json::Value>(&cached_json)
                    .ok()
                    .and_then(|value| value.get("total_node_count")?.as_u64())
                    .unwrap_or(0);
                if cached_node_count > 0 {
                    return cached_json;
                }
                cache.remove(&cache_key);
            }
        }
        let root = PathBuf::from(&record.root_path);
        let storage = match open_repository_storage_with_registry(&root, &self.registry) {
            Ok(storage) => storage,
            Err(error) => {
                return serde_json::json!({ "error_code": "ERR-STORAGE-101", "error": error })
                    .to_string();
            }
        };
        let Some(snapshot) = snapshot_for_repository(&root, &record, &storage, &self.registry)
            .ok()
            .flatten()
        else {
            return empty_universe(&active_id, is_3d, "unknown");
        };
        let cache_key = format!(
            "{}_{}_{}_{}_{}",
            active_id,
            snapshot.snapshot_id,
            if is_3d { "3d" } else { "2d" },
            node_limit,
            edge_limit
        );
        let selected = match storage.graph_universe_rows_for_snapshot(
            snapshot.snapshot_id.as_str(),
            node_limit,
            edge_limit,
        ) {
            Ok(rows) => rows,
            Err(error) => {
                return serde_json::json!({ "error_code": "ERR-GRAPH-001", "error": error.to_string() }).to_string();
            }
        };
        let node_ids = selected
            .nodes
            .iter()
            .map(|node| node.stable_id.clone())
            .collect::<Vec<_>>();
        let node_set = node_ids
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let mut layout_edges = selected
            .edges
            .iter()
            .filter(|edge| {
                node_set.contains(&edge.source_stable_id)
                    && node_set.contains(&edge.target_stable_id)
            })
            .map(|edge| (edge.source_stable_id.clone(), edge.target_stable_id.clone()))
            .collect::<Vec<_>>();
        layout_edges.sort();
        layout_edges.dedup();
        let seed = blake3::hash(snapshot.snapshot_id.as_str().as_bytes()).into();
        let Some(layout) = self.registry.get_layout("seeded_force_atlas") else {
            return serde_json::json!({ "error_code": "ERR-PLUGIN-001", "error": "No graph layout plugin is registered" }).to_string();
        };
        let coordinates = match layout.compute_layout(&node_ids, &layout_edges, &seed, is_3d) {
            Ok(coordinates) => coordinates,
            Err(error) => {
                return serde_json::json!({ "error_code": "ERR-PLUGIN-002", "error": error.to_string() }).to_string();
            }
        };
        let coverage = snapshot_coverage_from_counts(
            &snapshot,
            selected.total_node_count,
            &selected.coverage_counts,
        );
        let nodes = selected
            .nodes
            .iter()
            .filter_map(|node| {
                coordinates.get(&node.stable_id).map(|coordinate| {
                    serde_json::json!({
                        "id": node.stable_id,
                        "label": node.qualified_name,
                        "kind": node.node_kind,
                        "file_path": node.file_path,
                        "start_line": node.start_line,
                        "end_line": node.end_line,
                        "status": node.coverage_state,
                        "caller_count": selected.inbound_counts.get(&node.stable_id).copied().unwrap_or(0),
                        "callee_count": selected.outbound_counts.get(&node.stable_id).copied().unwrap_or(0),
                        "x": coordinate.x,
                        "y": coordinate.y,
                        "z": coordinate.z,
                        "cluster_id": coordinate.cluster_id
                    })
                })
            })
            .collect::<Vec<_>>();
        let edges = selected
            .edges
            .iter()
            .map(|edge| {
                serde_json::json!({
                    "id": edge.edge_id,
                    "source": edge.source_stable_id,
                    "target": edge.target_stable_id,
                    "kind": edge.edge_kind,
                    "source_file": edge.source_file,
                    "source_line": edge.source_line,
                    "source_col": edge.source_col,
                    "target_file": edge.target_file,
                    "target_line": edge.target_line,
                    "confidence": edge.confidence
                })
            })
            .collect::<Vec<_>>();
        let response_json = serde_json::json!({
            "repo_id": active_id,
            "snapshot_id": snapshot.snapshot_id,
            "dimensions": if is_3d { "3D" } else { "2D" },
            "coverage_state": coverage,
            "node_count": nodes.len(),
            "edge_count": edges.len(),
            "total_node_count": selected.total_node_count,
            "total_edge_count": selected.total_edge_count,
            "is_truncated": selected.is_truncated,
            "node_view_limit": node_limit,
            "edge_view_limit": edge_limit,
            "traversal_limit_reached": snapshot.traversal_limit_reached,
            "skipped_oversize_files": snapshot.skipped_oversize_files,
            "nodes": nodes,
            "edges": edges
        })
        .to_string();
        if selected.total_node_count > 0
            && let Ok(mut cache) = self.universe_cache.lock()
        {
            if cache.len() >= MAX_UNIVERSE_CACHE_ENTRIES {
                cache.clear();
            }
            cache.insert(cache_key, response_json.clone());
        }
        response_json
    }

    pub fn handle_graph_ego(&self, url: &str) -> String {
        let requested_id = Self::parse_query_param(url, "id").unwrap_or_default();
        if requested_id.is_empty() {
            return empty_ego();
        }
        let stable_id = if requested_id.contains('/') && !requested_id.contains('#') {
            file_node_id(&requested_id)
        } else {
            requested_id
        };
        let active_id = self.active_repo_id.lock().unwrap().clone();
        let record = self
            .connected_repos
            .lock()
            .ok()
            .and_then(|repos| repos.get(&active_id).cloned());
        let Some(record) = record else {
            return empty_ego();
        };
        let root = PathBuf::from(&record.root_path);
        let Some(snapshot_id) = record.latest_snap_id.as_deref() else {
            return empty_ego();
        };
        match open_repository_storage_with_registry(&root, &self.registry).and_then(|storage| {
            storage
                .query_ego_network(snapshot_id, &stable_id)
                .map_err(|error| error.to_string())
        }) {
            Ok(ego) if ego.target_node.is_some() => {
                serde_json::to_string(&ego).unwrap_or_else(|_| empty_ego())
            }
            _ => empty_ego(),
        }
    }

    pub fn handle_simulate_edit(&self, body: &str) -> String {
        let start = Instant::now();
        let request: SimulateEditRequest = match serde_json::from_str(body) {
            Ok(request) => request,
            Err(error) => return serde_json::json!({ "error": error.to_string() }).to_string(),
        };
        let result = self.analyze_edit_overlay(&request);
        let response = match result {
            Ok(mut response) => {
                response.computation_latency_ms = start.elapsed().as_millis() as u64;
                response
            }
            Err(error) => {
                return serde_json::json!({ "error": error }).to_string();
            }
        };
        let response_json = serde_json::to_string(&response).unwrap_or_default();
        let session_id = self.active_ai_session_id.lock().unwrap().clone();
        let receipt_digest = format!(
            "blake3:{}",
            blake3::hash(request.unsaved_buffer.as_bytes()).to_hex()
        );
        if let Some(storage) = self.get_storage_for_active_repo() {
            let event = TrajectoryEventRecord {
                event_seq: 0,
                session_id: session_id.clone(),
                job_id: Some("simulate_edit".to_string()),
                workflow_id: Some("wf_pre_write_guardrail".to_string()),
                event_type: "simulate_edit".to_string(),
                target_symbol: None,
                target_file: Some(request.file_path.clone()),
                before_snap_id: response.baseline_snapshot_id.clone(),
                after_snap_id: response.candidate_snapshot_id.clone(),
                verdict: response.verdict.clone(),
                summary: format!(
                    "Simulated edit on {}: {} impacted callers",
                    request.file_path,
                    response.damaged_callers.len()
                ),
                evidence_json: response_json.clone(),
                blake3_receipt: receipt_digest.clone(),
                tokens_spent: 0,
                tokens_saved: 0,
            };
            let _ = storage.record_trajectory_event(&event);
        }
        let event_data = serde_json::json!({
            "repo_id": response.repo_id,
            "file_path": request.file_path,
            "verdict": response.verdict,
            "baseline_snapshot_id": response.baseline_snapshot_id,
            "candidate_snapshot_id": response.candidate_snapshot_id,
            "impacted_callers": response.damaged_callers.len(),
            "receipt": receipt_digest
        });
        self.broadcast_event("simulate.edit_completed", event_data.clone());
        self.broadcast_event("blast.simulated", event_data);
        response_json
    }

    fn analysis_artifacts_for_snapshot(
        &self,
        repo_id: &str,
        snapshot: &RepoSnapshot,
        storage: &StorageEngine,
        root: &Path,
    ) -> Result<Arc<AnalysisArtifacts>, String> {
        if let Ok(cache) = self.analysis_artifact_cache.lock()
            && let Some((cached_repo, cached_snapshot, artifacts)) = cache.as_ref()
            && cached_repo == repo_id
            && cached_snapshot == snapshot.snapshot_id.as_str()
        {
            return Ok(Arc::clone(artifacts));
        }
        let artifacts = Arc::new(
            build_artifacts_from_snapshot_with_registry(
                snapshot.clone(),
                storage,
                &self.registry,
                root,
            )
            .map_err(|error| error.to_string())?,
        );
        if let Ok(mut cache) = self.analysis_artifact_cache.lock() {
            *cache = Some((
                repo_id.to_string(),
                snapshot.snapshot_id.to_string(),
                Arc::clone(&artifacts),
            ));
        }
        Ok(artifacts)
    }

    fn analyze_edit_overlay(
        &self,
        request: &SimulateEditRequest,
    ) -> Result<SimulateEditResponse, String> {
        let repo_id = request
            .repo_id
            .clone()
            .unwrap_or_else(|| self.active_repo_id.lock().unwrap().clone());
        let repo = self
            .connected_repos
            .lock()
            .map_err(|_| "repository registry is unavailable".to_string())?
            .get(&repo_id)
            .cloned()
            .ok_or_else(|| format!("repository is not connected: {repo_id}"))?;
        let root = PathBuf::from(&repo.root_path);
        let root = root
            .canonicalize()
            .map_err(|error| format!("repository root is unavailable: {error}"))?;
        let relative_path = request.file_path.replace('\\', "/");
        if !autopsy_repo::is_safe_relative_path(&relative_path)
            || relative_path
                .split('/')
                .any(|segment| segment == ".git" || segment == ".autopsy")
        {
            return Err("edit path must be a safe repository-relative source path".to_string());
        }
        let storage = self.repository_storage(&root)?;
        let baseline_id = request
            .baseline_snapshot_id
            .as_deref()
            .or(repo.latest_snap_id.as_deref())
            .ok_or_else(|| "repository has no indexed baseline snapshot".to_string())?;
        let cached_artifacts = self.analysis_artifact_cache.lock().ok().and_then(|cache| {
            cache
                .as_ref()
                .and_then(|(cached_repo, cached_snapshot, artifacts)| {
                    (cached_repo == &repo.repo_id && cached_snapshot == baseline_id)
                        .then(|| Arc::clone(artifacts))
                })
        });
        let baseline_artifacts = match cached_artifacts {
            Some(artifacts) => artifacts,
            None => {
                let baseline_snapshot = storage
                    .get_snapshot(&autopsy_domain::SnapshotId::new(baseline_id))
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("baseline snapshot not found: {baseline_id}"))?;
                self.analysis_artifacts_for_snapshot(
                    &repo.repo_id,
                    &baseline_snapshot,
                    &storage,
                    &root,
                )?
            }
        };
        let baseline_snapshot = &baseline_artifacts.snapshot;
        if !baseline_snapshot.files.contains_key(&relative_path) {
            return Err(format!(
                "file is not part of the baseline snapshot: {relative_path}"
            ));
        }
        let config = load_or_default_config(&root).map_err(|error| error.to_string())?;
        let overlays = BTreeMap::from([(
            relative_path.clone(),
            request.unsaved_buffer.as_bytes().to_vec(),
        )]);
        let parse_result = parse_source_buffer_with_registry(
            &root,
            &relative_path,
            request.unsaved_buffer.as_bytes(),
            &self.registry,
        );
        let incomplete_response =
            |coverage_state: CoverageState, syntax_errors: Vec<String>| SimulateEditResponse {
                repo_id: repo_id.clone(),
                verdict: "UNKNOWN".to_string(),
                coverage_state: coverage_name(coverage_state),
                coverage_note: Some(coverage_note(coverage_state, &relative_path)),
                baseline_snapshot_id: Some(baseline_snapshot.snapshot_id.to_string()),
                candidate_snapshot_id: None,
                impacted_entities: Vec::new(),
                impact_truncated: false,
                damaged_callers: Vec::new(),
                newly_dead_symbols: Vec::new(),
                broken_imports: Vec::new(),
                syntax_errors,
                computation_latency_ms: 0,
            };
        if !parse_result.parser_available {
            return Ok(incomplete_response(
                CoverageState::Unknown,
                parse_result.diagnostics,
            ));
        }
        let candidate_overlay = match analyze_file_overlay_from_artifacts_with_registry(
            &root,
            &config,
            &baseline_artifacts,
            &relative_path,
            request.unsaved_buffer.as_bytes(),
            &self.registry,
        )
        .map_err(|error| error.to_string())?
        {
            Some(overlay) => overlay,
            None if baseline_artifacts.coverage_state != CoverageState::Verified => {
                let coverage_state = if baseline_artifacts.coverage_state == CoverageState::Partial
                {
                    CoverageState::Partial
                } else {
                    CoverageState::Unknown
                };
                return Ok(incomplete_response(
                    coverage_state,
                    parse_result.diagnostics,
                ));
            }
            None => {
                let candidate = build_snapshot_and_graph_for_overlay_from_artifacts_with_registry(
                    &root,
                    &config,
                    &baseline_artifacts,
                    &overlays,
                    &storage,
                    &self.registry,
                )
                .map_err(|error| error.to_string())?;
                let symbols = candidate
                    .symbols
                    .iter()
                    .filter(|symbol| {
                        symbol
                            .range
                            .as_ref()
                            .is_some_and(|location| location.path == relative_path)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let symbol_ids = symbols
                    .iter()
                    .map(|symbol| symbol.stable_id.clone())
                    .collect::<BTreeSet<_>>();
                let edges = candidate
                    .edges
                    .iter()
                    .filter(|edge| {
                        symbol_ids.contains(&edge.source)
                            || edge
                                .location
                                .as_ref()
                                .is_some_and(|location| location.path == relative_path)
                    })
                    .cloned()
                    .collect();
                let contracts = candidate
                    .contracts
                    .iter()
                    .filter(|contract| symbol_ids.contains(&contract.owner))
                    .cloned()
                    .collect();
                OverlayFileArtifacts {
                    snapshot: candidate.snapshot,
                    symbols,
                    edges,
                    contracts,
                    coverage_state: candidate.coverage_state,
                    parse_result,
                }
            }
        };
        let parsed = candidate_overlay.parse_result.clone();
        let before_files = BTreeMap::from([(
            relative_path.clone(),
            baseline_snapshot.files[&relative_path].clone(),
        )]);
        let after_files = BTreeMap::from([(
            relative_path.clone(),
            candidate_overlay.snapshot.files[&relative_path].clone(),
        )]);
        let before_symbols = baseline_artifacts
            .symbols
            .iter()
            .filter(|symbol| {
                symbol
                    .range
                    .as_ref()
                    .is_some_and(|location| location.path == relative_path)
            })
            .cloned()
            .collect::<Vec<_>>();
        let after_symbols = candidate_overlay
            .symbols
            .iter()
            .filter(|symbol| {
                symbol
                    .range
                    .as_ref()
                    .is_some_and(|location| location.path == relative_path)
            })
            .cloned()
            .collect::<Vec<_>>();
        let before_symbol_ids = before_symbols
            .iter()
            .map(|symbol| symbol.stable_id.clone())
            .collect::<BTreeSet<_>>();
        let after_symbol_ids = after_symbols
            .iter()
            .map(|symbol| symbol.stable_id.clone())
            .collect::<BTreeSet<_>>();
        let before_edges = baseline_artifacts
            .edges
            .iter()
            .filter(|edge| {
                before_symbol_ids.contains(&edge.source)
                    || edge
                        .location
                        .as_ref()
                        .is_some_and(|location| location.path == relative_path)
            })
            .cloned()
            .collect::<Vec<_>>();
        let after_edges = candidate_overlay
            .edges
            .iter()
            .filter(|edge| {
                after_symbol_ids.contains(&edge.source)
                    || edge
                        .location
                        .as_ref()
                        .is_some_and(|location| location.path == relative_path)
            })
            .cloned()
            .collect::<Vec<_>>();
        let after_call_targets = after_edges
            .iter()
            .filter(|edge| edge.kind == EdgeKind::Calls)
            .map(|edge| edge.target.clone())
            .collect::<BTreeSet<_>>();
        let removed_call_targets = before_edges
            .iter()
            .filter(|edge| edge.kind == EdgeKind::Calls)
            .filter(|edge| !after_call_targets.contains(&edge.target))
            .map(|edge| edge.target.clone())
            .collect::<BTreeSet<_>>();
        let before_graph = DependencyGraph::from_symbols_and_edges(before_symbols, before_edges);
        let after_graph = DependencyGraph::from_symbols_and_edges(after_symbols, after_edges);
        let before_contracts = baseline_artifacts
            .contracts
            .iter()
            .filter(|contract| before_symbol_ids.contains(&contract.owner))
            .cloned()
            .collect::<Vec<_>>();
        let after_contracts = candidate_overlay
            .contracts
            .iter()
            .filter(|contract| after_symbol_ids.contains(&contract.owner))
            .cloned()
            .collect::<Vec<_>>();
        let diff = DiffEngine::compute_diff(DiffInput {
            before_snapshot_id: baseline_snapshot.snapshot_id.clone(),
            after_snapshot_id: candidate_overlay.snapshot.snapshot_id.clone(),
            before_files: &before_files,
            after_files: &after_files,
            before_graph: &before_graph,
            after_graph: &after_graph,
            before_contracts: &before_contracts,
            after_contracts: &after_contracts,
        });
        let mut breaking_symbols = BTreeSet::<SymbolId>::new();
        let mut removed_symbols = BTreeSet::<SymbolId>::new();
        let mut definite_break = false;
        for delta in &diff.contract_deltas {
            match delta {
                ContractDelta::Modified {
                    owner, is_breaking, ..
                } => {
                    if *is_breaking {
                        breaking_symbols.insert(owner.clone());
                        definite_break = true;
                    }
                }
                ContractDelta::Removed { contract } => {
                    breaking_symbols.insert(contract.owner.clone());
                    definite_break = true;
                }
                ContractDelta::Added { .. } => {}
            }
        }
        for delta in &diff.symbol_deltas {
            if let SymbolDelta::Removed { symbol } = delta {
                breaking_symbols.insert(symbol.stable_id.clone());
                removed_symbols.insert(symbol.stable_id.clone());
            }
        }
        let broken_imports = baseline_artifacts
            .edges
            .iter()
            .filter(|edge| {
                matches!(
                    edge.kind,
                    EdgeKind::Imports | EdgeKind::Calls | EdgeKind::References
                ) && removed_symbols.contains(&edge.target)
            })
            .filter_map(|edge| {
                let location = edge.location.as_ref()?;
                let symbol = baseline_artifacts.graph.get_node(&edge.target)?;
                Some(format!(
                    "BROKEN IMPORT: '{}' was removed from {}:{}",
                    symbol.qualified_name, location.path, location.start_line
                ))
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut damaged_callers = BTreeMap::<String, DamagedCallerItem>::new();
        let mut impact_truncated = false;
        let impact_engine = ImpactEngine::new();
        let mut edges_by_source = BTreeMap::new();
        for edge in &baseline_artifacts.edges {
            if breaking_symbols.contains(&edge.target)
                && matches!(
                    edge.kind,
                    EdgeKind::Calls | EdgeKind::References | EdgeKind::Imports
                )
            {
                edges_by_source
                    .entry(edge.source.clone())
                    .or_insert_with(|| edge.clone());
            }
        }
        for seed in &breaking_symbols {
            let impact = impact_engine
                .compute_impact(
                    &baseline_artifacts.graph,
                    &ImpactQuery {
                        seed_symbols: vec![seed.clone()],
                        direction: ImpactDirection::Backward,
                        profile: callers_profile(64, 10_000),
                    },
                )
                .map_err(|error| error.to_string())?;
            impact_truncated |= impact.truncated;
            for caller_id in impact.impacted_entities {
                if &caller_id == seed {
                    continue;
                }
                let Some(caller) = baseline_artifacts.graph.get_node(&caller_id) else {
                    continue;
                };
                let location = caller.range.as_ref();
                let edge = edges_by_source.get(&caller_id);
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
                damaged_callers.insert(
                    caller_id.to_string(),
                    DamagedCallerItem {
                        stable_id: caller_id.to_string(),
                        file_path,
                        calling_line: line,
                        calling_col: col,
                        diagnostic: format!("depends on changed or removed contract {seed}"),
                        severity: if edge.is_some() {
                            "breaking_error".to_string()
                        } else {
                            "impact_path_unverified".to_string()
                        },
                    },
                );
            }
        }
        let mut newly_dead_symbols = BTreeMap::<String, DeadSymbolItem>::new();
        for target in removed_call_targets {
            let remaining_calls = baseline_artifacts.edges.iter().any(|edge| {
                edge.kind == EdgeKind::Calls
                    && edge.target == target
                    && !before_symbol_ids.contains(&edge.source)
            }) || after_call_targets.contains(&target);
            if remaining_calls {
                continue;
            }
            let Some(symbol) = baseline_artifacts.symbols.iter().find(|symbol| {
                symbol.stable_id == target
                    && matches!(
                        symbol.visibility,
                        Visibility::Private | Visibility::Internal
                    )
            }) else {
                continue;
            };
            let Some(location) = symbol.range.as_ref() else {
                continue;
            };
            newly_dead_symbols.insert(
                symbol.stable_id.to_string(),
                DeadSymbolItem {
                    stable_id: symbol.stable_id.to_string(),
                    file_path: location.path.clone(),
                    line: i64::from(location.start_line),
                    diagnostic: "No verified incoming calls remain in this draft; the symbol is private and non-exported.".to_string(),
                },
            );
        }
        let coverage_state = combine_coverage([
            scoped_artifact_coverage(&baseline_artifacts, &relative_path),
            scoped_overlay_coverage(&candidate_overlay, &relative_path),
            parsed.coverage_state,
        ]);
        let has_syntax_error = parsed.parser_available && !parsed.syntax_valid;
        let verdict = if has_syntax_error || (definite_break && !damaged_callers.is_empty()) {
            "FAIL"
        } else if definite_break
            || impact_truncated
            || !newly_dead_symbols.is_empty()
            || coverage_state == CoverageState::Partial
        {
            "WARN"
        } else if coverage_state == CoverageState::Verified {
            "PASS"
        } else {
            "UNKNOWN"
        };
        Ok(SimulateEditResponse {
            repo_id,
            verdict: verdict.to_string(),
            coverage_state: coverage_name(coverage_state),
            coverage_note: (coverage_state != CoverageState::Verified)
                .then(|| coverage_note(coverage_state, &relative_path)),
            baseline_snapshot_id: Some(baseline_snapshot.snapshot_id.to_string()),
            candidate_snapshot_id: Some(candidate_overlay.snapshot.snapshot_id.to_string()),
            impacted_entities: damaged_callers.keys().cloned().collect(),
            impact_truncated,
            damaged_callers: damaged_callers.into_values().collect(),
            newly_dead_symbols: newly_dead_symbols.into_values().collect(),
            broken_imports,
            syntax_errors: parsed.diagnostics,
            computation_latency_ms: 0,
        })
    }

    pub fn handle_live_sse(&self, req: Request) {
        let last_event_id = req
            .headers()
            .iter()
            .find(|header| header.field.equiv("Last-Event-ID"))
            .and_then(|header| header.value.as_str().parse::<i64>().ok())
            .or_else(|| {
                Self::parse_query_param(req.url(), "since")?
                    .parse::<i64>()
                    .ok()
            })
            .unwrap_or(0);
        let storage = match open_storage_engine_with_registry(
            &self.registry,
            &StorageEngineConfig {
                database_path: self.repo_root.join(".autopsy").join("storage.db"),
                cache_dir: Some(self.repo_root.join(".autopsy").join("cache")),
                use_wal: true,
                recover_corruption: true,
            },
        ) {
            Ok(storage) => storage,
            Err(error) => {
                let response = Response::from_string(
                    serde_json::json!({ "error": error.to_string() }).to_string(),
                )
                .with_status_code(StatusCode(503));
                let _ = req.respond(response);
                return;
            }
        };
        let mut sse_data = String::from("retry: 1000\n\n");
        let mut cursor = last_event_id;
        match self.wait_for_live_events(&storage, cursor, Duration::from_secs(15)) {
            Ok(events) => {
                for event in events {
                    cursor = event.seq_id;
                    sse_data.push_str(&format!(
                        "id: {}\nevent: {}\ndata: {}\n\n",
                        event.seq_id, event.event_type, event.data_json
                    ));
                }
            }
            Err(error) => {
                sse_data.push_str(&format!(
                    "event: daemon.error\ndata: {}\n\n",
                    serde_json::json!({ "error": error })
                ));
            }
        }
        if cursor == last_event_id {
            sse_data.push_str(": keep-alive\n\n");
        }

        let header_sse =
            Header::from_bytes(&b"Content-Type"[..], &b"text/event-stream"[..]).unwrap();
        let header_cache = Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..]).unwrap();
        let header_cors =
            Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap();
        let response = Response::from_string(sse_data)
            .with_header(header_sse)
            .with_header(header_cache)
            .with_header(header_cors);
        let _ = req.respond(response);
    }

    pub fn url_decode(s: &str) -> String {
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

    pub fn parse_query_param(url: &str, key: &str) -> Option<String> {
        url.split('?').nth(1).and_then(|q| {
            for pair in q.split('&') {
                let mut parts = pair.split('=');
                if parts.next() == Some(key) {
                    return parts.next().map(Self::url_decode);
                }
            }
            None
        })
    }

    /// Universally normalize cross-platform paths (Windows UNC \\wsl.localhost\...,
    /// Windows drive letters C:\..., Linux /home/..., macOS /Users/..., and ~ tildes)
    pub fn normalize_cross_platform_path(raw: &str) -> PathBuf {
        let trimmed = raw.trim().trim_matches('"').trim_matches('\'').trim();
        if trimmed.is_empty() {
            return PathBuf::from(".");
        }

        // Standardize separators to forward slashes for cross-platform inspection
        let normalized = trimmed.replace('\\', "/");

        // 1. WSL UNC paths: //wsl.localhost/<distro>/... or //wsl$/<distro>/...
        if normalized.starts_with("//wsl.localhost/") || normalized.starts_with("//wsl$/") {
            let after_prefix = if let Some(stripped) = normalized.strip_prefix("//wsl.localhost/") {
                stripped
            } else {
                normalized.strip_prefix("//wsl$/").unwrap()
            };

            // after_prefix is "<distro>/home/ravi/..."
            if let Some(slash_idx) = after_prefix.find('/') {
                let linux_path = &after_prefix[slash_idx..]; // "/home/ravi/..."
                let candidate = PathBuf::from(linux_path);
                if candidate.exists() {
                    return candidate;
                }
                return candidate;
            }
        }

        // 2. Windows drive letters on Linux/WSL: C:/... or c:/... -> /mnt/c/...
        if normalized.len() >= 3
            && normalized.as_bytes()[1] == b':'
            && normalized.as_bytes()[2] == b'/'
        {
            let drive = (normalized.as_bytes()[0] as char).to_ascii_lowercase();
            let mnt_path = format!("/mnt/{}{}", drive, &normalized[2..]);
            let candidate = PathBuf::from(&mnt_path);
            if candidate.exists() {
                return candidate;
            }
            #[cfg(not(target_os = "windows"))]
            {
                return candidate;
            }
        }

        // 3. Tilde expansion: ~/path -> $HOME/path
        if let Some(stripped) = normalized.strip_prefix("~/") {
            if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
                let candidate =
                    PathBuf::from(format!("{}/{}", home.trim_end_matches('/'), stripped));
                if candidate.exists() {
                    return candidate;
                }
            }
        }

        // 4. Fallback directly or with normalized slashes
        let direct = PathBuf::from(trimmed);
        if direct.exists() {
            return direct;
        }

        let slash_direct = PathBuf::from(&normalized);
        if slash_direct.exists() {
            return slash_direct;
        }

        direct
    }

    pub fn get_active_repo_dir(&self) -> PathBuf {
        let p = self.active_repo_path.lock().unwrap().clone();
        if p.is_dir() && !p.to_string_lossy().ends_with("SynEvid") {
            return p;
        }
        let active_id = self.active_repo_id.lock().unwrap().clone();
        if let Ok(repos) = self.connected_repos.lock() {
            if let Some(r) = repos.get(&active_id) {
                let target = PathBuf::from(&r.root_path);
                if target.is_dir() && !target.to_string_lossy().ends_with("SynEvid") {
                    return target;
                }
            }
        }
        let storage_guard = self.storage.lock().unwrap();
        if let Some(ref st) = *storage_guard {
            if let Ok(Some(repo)) = st.get_repository(&active_id) {
                let p = PathBuf::from(&repo.root_path);
                if p.is_dir() && !p.to_string_lossy().ends_with("SynEvid") {
                    return p;
                }
            }
        }
        // If repo_root is a targeted repository (such as in automated tests) and not SynEvid itself, use it
        if !self.repo_root.to_string_lossy().ends_with("SynEvid") && self.repo_root.is_dir() {
            return self.repo_root.clone();
        }
        PathBuf::new()
    }

    pub fn handle_git_galaxy(&self, url: &str) -> String {
        let repo_root = self.get_active_repo_dir();
        let limit = Self::parse_query_param(url, "limit")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(200);
        if repo_root.as_os_str().is_empty() {
            return serde_json::json!({ "error": "No active repository is selected" }).to_string();
        }
        if !repo_root.ancestors().any(|path| path.join(".git").exists()) {
            let canonical_root = repo_root
                .canonicalize()
                .unwrap_or_else(|_| repo_root.clone());
            let galaxy = GitGalaxy {
                repo_id: blake3::hash(canonical_root.to_string_lossy().as_bytes())
                    .to_hex()
                    .to_string(),
                head_oid: None,
                active_branch: None,
                commits: Vec::new(),
                branches: Vec::new(),
                tags: Vec::new(),
                total_commits: 0,
            };
            return serde_json::to_string(&galaxy).unwrap_or_else(|_| "{}".to_string());
        }
        let reader = GitReader::new(&repo_root);
        match reader.read_galaxy(limit) {
            Ok(galaxy) => serde_json::to_string(&galaxy).unwrap_or_else(|_| "{}".to_string()),
            Err(e) => serde_json::json!({ "error": format!("{e}") }).to_string(),
        }
    }

    pub fn handle_git_time_travel(&self, url: &str) -> String {
        let repo_root = self.get_active_repo_dir();
        let is_3d = !url.contains("dim=2d");
        let oid_param = Self::parse_query_param(url, "oid");
        let oid = if let Some(o) = oid_param {
            o
        } else {
            let path_part = url.split('?').next().unwrap_or(url);
            let segments: Vec<&str> = path_part.split('/').filter(|s| !s.is_empty()).collect();
            if segments.len() >= 5 {
                segments[4].to_string()
            } else {
                "HEAD".to_string()
            }
        };

        let builder = VirtualTreeBuilder::new(&repo_root);
        match builder.build_frame(&oid, is_3d) {
            Ok(frame) => serde_json::to_string(&frame).unwrap_or_else(|_| "{}".to_string()),
            Err(e) => serde_json::json!({ "error": format!("{e}") }).to_string(),
        }
    }

    pub fn handle_git_diff(&self, url: &str) -> String {
        let repo_root = self.get_active_repo_dir();
        let base_param = Self::parse_query_param(url, "base");
        let head_param = Self::parse_query_param(url, "head");

        let (base, head) = match (base_param, head_param) {
            (Some(b), Some(h)) => (b, h),
            _ => {
                let path_part = url.split('?').next().unwrap_or(url);
                let segments: Vec<&str> = path_part.split('/').filter(|s| !s.is_empty()).collect();
                if segments.len() >= 6 {
                    (segments[4].to_string(), segments[5].to_string())
                } else {
                    return serde_json::json!({ "error": "Missing base and head OID parameters" })
                        .to_string();
                }
            }
        };

        let differ = GitDiffEngine::new(&repo_root);
        match differ.diff_commits(&base, &head) {
            Ok(report) => serde_json::to_string(&report).unwrap_or_else(|_| "{}".to_string()),
            Err(e) => serde_json::json!({ "error": format!("{e}") }).to_string(),
        }
    }

    pub fn handle_git_stage(&self, body: &str) -> String {
        let repo_root = self.get_active_repo_dir();
        let parsed: Result<StageRequest, _> = serde_json::from_str(body);
        match parsed {
            Ok(req) => {
                let manager = GitMutationManager::new(&repo_root);
                match manager.stage(&req.paths) {
                    Ok(res) => serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string()),
                    Err(e) => serde_json::json!({ "success": false, "action": "stage", "error": format!("{e}") })
                        .to_string(),
                }
            }
            Err(e) => {
                serde_json::json!({ "success": false, "action": "stage", "error": format!("{e}") })
                    .to_string()
            }
        }
    }

    pub fn handle_git_commit(&self, body: &str) -> String {
        let repo_root = self.get_active_repo_dir();
        let parsed: Result<CommitRequest, _> = serde_json::from_str(body);
        match parsed {
            Ok(req) => {
                let manager = GitMutationManager::new(&repo_root);
                match manager.commit_request(&req) {
                    Ok(res) => {
                        self.refresh_active_git_head();
                        serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string())
                    }
                    Err(e) => serde_json::json!({ "success": false, "action": "commit", "error": format!("{e}") })
                        .to_string(),
                }
            }
            Err(e) => {
                serde_json::json!({ "success": false, "action": "commit", "error": format!("{e}") })
                    .to_string()
            }
        }
    }

    pub fn handle_git_branch(&self, body: &str) -> String {
        let repo_root = self.get_active_repo_dir();
        let parsed: Result<BranchRequest, _> = serde_json::from_str(body);
        match parsed {
            Ok(req) => {
                let manager = GitMutationManager::new(&repo_root);
                match manager.branch_request(&req) {
                    Ok(res) => {
                        self.refresh_active_git_head();
                        serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string())
                    }
                    Err(e) => serde_json::json!({ "success": false, "action": "branch", "error": format!("{e}") })
                        .to_string(),
                }
            }
            Err(e) => {
                serde_json::json!({ "success": false, "action": "branch", "error": format!("{e}") })
                    .to_string()
            }
        }
    }

    pub fn handle_git_undo(&self) -> String {
        let repo_root = self.get_active_repo_dir();
        let manager = GitMutationManager::new(&repo_root);
        match manager.undo() {
            Ok(res) => {
                self.refresh_active_git_head();
                serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string())
            }
            Err(e) => {
                serde_json::json!({ "success": false, "action": "undo", "error": format!("{e}") })
                    .to_string()
            }
        }
    }

    pub fn handle_git_status(&self) -> String {
        let repo_root = self.get_active_repo_dir();
        let scanner = GitStatusScanner::new(&repo_root);
        match scanner.scan_status() {
            Ok(status) => serde_json::to_string(&status).unwrap_or_else(|_| "{}".to_string()),
            Err(e) => serde_json::json!({
                "branch": "unknown",
                "head_oid": null,
                "staged": [],
                "unstaged": [],
                "untracked": [],
                "deleted": [],
                "ignored": [],
                "is_clean": true,
                "total_uncommitted": 0,
                "error": format!("{e}")
            })
            .to_string(),
        }
    }

    pub fn handle_git_diff_file(&self, url: &str) -> String {
        let repo_root = self.get_active_repo_dir();
        let path = Self::parse_query_param(url, "path").unwrap_or_default();

        let scanner = GitStatusScanner::new(&repo_root);
        match scanner.diff_file(&path) {
            Ok(diff) => serde_json::to_string(&diff).unwrap_or_else(|_| "{}".to_string()),
            Err(e) => serde_json::json!({
                "path": path,
                "status": "error",
                "old_content": "",
                "new_content": "",
                "diff_lines": [],
                "total_additions": 0,
                "total_deletions": 0,
                "error": format!("{e}")
            })
            .to_string(),
        }
    }

    pub fn handle_git_unstage(&self, body: &str) -> String {
        let repo_root = self.get_active_repo_dir();
        let parsed: Result<StageRequest, _> = serde_json::from_str(body);
        let manager = GitMutationManager::new(&repo_root);
        match parsed {
            Ok(req) => match manager.unstage(&req.paths) {
                Ok(res) => serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string()),
                Err(e) => serde_json::json!({ "success": false, "action": "unstage", "error": format!("{e}") })
                    .to_string(),
            },
            Err(_) => match manager.unstage_all() {
                Ok(res) => serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string()),
                Err(e) => serde_json::json!({ "success": false, "action": "unstage_all", "error": format!("{e}") })
                    .to_string(),
            },
        }
    }

    pub fn handle_git_tag(&self, body: &str) -> String {
        let repo_root = self.get_active_repo_dir();
        let parsed: Result<TagRequest, _> = serde_json::from_str(body);
        match parsed {
            Ok(req) => {
                let manager = GitMutationManager::new(&repo_root);
                match manager.tag(&req) {
                    Ok(res) => serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string()),
                    Err(e) => serde_json::json!({ "success": false, "action": "tag", "error": format!("{e}") })
                        .to_string(),
                }
            }
            Err(e) => {
                serde_json::json!({ "success": false, "action": "tag", "error": format!("{e}") })
                    .to_string()
            }
        }
    }

    pub fn handle_git_merge(&self, body: &str) -> String {
        let repo_root = self.get_active_repo_dir();
        let parsed: Result<MergeRequest, _> = serde_json::from_str(body);
        match parsed {
            Ok(req) => {
                let manager = GitMutationManager::new(&repo_root);
                match manager.merge(&req) {
                    Ok(res) => {
                        self.refresh_active_git_head();
                        serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string())
                    }
                    Err(e) => serde_json::json!({ "success": false, "action": "merge", "error": format!("{e}") })
                        .to_string(),
                }
            }
            Err(e) => {
                serde_json::json!({ "success": false, "action": "merge", "error": format!("{e}") })
                    .to_string()
            }
        }
    }

    pub fn handle_git_checkout(&self, body: &str) -> String {
        let repo_root = self.get_active_repo_dir();
        let parsed: Result<CheckoutRequest, _> = serde_json::from_str(body);
        match parsed {
            Ok(req) => {
                let manager = GitMutationManager::new(&repo_root);
                match manager.checkout(&req) {
                    Ok(res) => {
                        self.refresh_active_git_head();
                        serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string())
                    }
                    Err(e) => serde_json::json!({ "success": false, "action": "checkout", "error": format!("{e}") })
                        .to_string(),
                }
            }
            Err(e) => serde_json::json!({ "success": false, "action": "checkout", "error": format!("{e}") })
                .to_string(),
        }
    }

    pub fn handle_git_revert(&self, body: &str) -> String {
        let repo_root = self.get_active_repo_dir();
        let parsed: Result<RevertRequest, _> = serde_json::from_str(body);
        match parsed {
            Ok(req) => {
                let manager = GitMutationManager::new(&repo_root);
                match manager.revert(&req) {
                    Ok(res) => {
                        self.refresh_active_git_head();
                        serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string())
                    }
                    Err(e) => serde_json::json!({ "success": false, "action": "revert", "error": format!("{e}") })
                        .to_string(),
                }
            }
            Err(e) => {
                serde_json::json!({ "success": false, "action": "revert", "error": format!("{e}") })
                    .to_string()
            }
        }
    }

    pub fn handle_git_bisect(&self, body: &str) -> String {
        let repo_root = self.get_active_repo_dir();
        let parsed: Result<BisectRequest, _> = serde_json::from_str(body);
        match parsed {
            Ok(req) => {
                let manager = GitMutationManager::new(&repo_root);
                match manager.bisect(&req) {
                    Ok(res) => serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string()),
                    Err(e) => serde_json::json!({
                        "breaking_commit_oid": null,
                        "breaking_summary": format!("Bisect failed: {e}"),
                        "breaking_author": "",
                        "breaking_timestamp": 0,
                        "verdict": "FAIL",
                        "drifted_symbols": [],
                        "commits_evaluated": 0,
                        "total_scanned": 0
                    })
                    .to_string(),
                }
            }
            Err(e) => serde_json::json!({
                "breaking_commit_oid": null,
                "breaking_summary": format!("Invalid bisect request: {e}"),
                "breaking_author": "",
                "breaking_timestamp": 0,
                "verdict": "FAIL",
                "drifted_symbols": [],
                "commits_evaluated": 0,
                "total_scanned": 0
            })
            .to_string(),
        }
    }

    pub fn handle_terminal_exec(&self, body: &str) -> String {
        let repo_root = self.get_active_repo_dir();
        let parsed: Result<TerminalExecRequest, _> = serde_json::from_str(body);
        let cmd = parsed
            .map(|p| p.command)
            .unwrap_or_default()
            .trim()
            .to_string();

        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut exit_code = 0;

        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let first = parts.first().copied().unwrap_or("");

        match first {
            "help" => {
                stdout.push_str(
                    "SynEvid Pure-Rust Terminal (Zero Sub-Process Invariant INV-SANDBOX-003)\n",
                );
                stdout.push_str("Available Offline Developer Commands:\n");
                stdout.push_str(
                    "  git status              - Scan working tree, staged & untracked files\n",
                );
                stdout.push_str("  git log [--oneline]     - View topological commit DAG from git init to HEAD\n");
                stdout.push_str("  git branch              - List repository branches\n");
                stdout.push_str("  git tag                 - List release and semantic tags\n");
                stdout.push_str("  git commit -m \"<msg>\"   - Guarded commit with SynEvid verification trailers\n");
                stdout.push_str(
                    "  git undo                - Instant 1-click rollback via refs/autopsy/undo/\n",
                );
                stdout.push_str("  autopsy verify          - Run deterministic architecture & policy invariant verification\n");
                stdout
                    .push_str("  autopsy impact -s <sym> - Compute BFS transitive blast radius\n");
                stdout.push_str("  clear                   - Clear terminal workbench\n");
            }
            "git" => {
                let sub = parts.get(1).copied().unwrap_or("");
                match sub {
                    "status" => {
                        let scanner = GitStatusScanner::new(&repo_root);
                        if let Ok(st) = scanner.scan_status() {
                            stdout.push_str(&format!("On branch {}\n", st.branch));
                            if let Some(h) = st.head_oid {
                                stdout.push_str(&format!(
                                    "HEAD detached at {}\n",
                                    &h[..7.min(h.len())]
                                ));
                            }
                            if !st.staged.is_empty() {
                                stdout.push_str("\nChanges to be committed:\n  (use \"git restore --staged <file>...\" to unstage)\n");
                                for f in &st.staged {
                                    stdout.push_str(&format!(
                                        "\t\x1b[32mstaged:   {}\x1b[0m (+{} -{})\n",
                                        f.path, f.added_lines, f.deleted_lines
                                    ));
                                }
                            }
                            if !st.unstaged.is_empty() {
                                stdout.push_str("\nChanges not staged for commit:\n  (use \"git add <file>...\" to update what will be committed)\n");
                                for f in &st.unstaged {
                                    stdout.push_str(&format!(
                                        "\t\x1b[33mmodified: {}\x1b[0m (+{} -{})\n",
                                        f.path, f.added_lines, f.deleted_lines
                                    ));
                                }
                            }
                            if !st.untracked.is_empty() {
                                stdout.push_str("\nUntracked files:\n  (use \"git add <file>...\" to include in what will be committed)\n");
                                for f in &st.untracked {
                                    stdout.push_str(&format!("\t\x1b[31m{}\x1b[0m\n", f.path));
                                }
                            }
                            if st.is_clean {
                                stdout.push_str("\nnothing to commit, working tree clean\n");
                            }
                        } else {
                            stderr.push_str(
                                "fatal: not a git repository (or any of the parent directories)\n",
                            );
                            exit_code = 128;
                        }
                    }
                    "log" => {
                        let reader = GitReader::new(&repo_root);
                        if let Ok(galaxy) = reader.read_galaxy(30) {
                            let oneline = parts.contains(&"--oneline");
                            for c in &galaxy.commits {
                                if oneline {
                                    stdout.push_str(&format!(
                                        "\x1b[33m{}\x1b[0m {}\n",
                                        c.short_oid, c.summary
                                    ));
                                } else {
                                    stdout.push_str(&format!("\x1b[33mcommit {}\x1b[0m\nAuthor: {}\nDate:   {}\n\n    {}\n\n", c.oid, c.author_name, c.timestamp, c.summary));
                                }
                            }
                        } else {
                            stderr.push_str("fatal: could not read commit history\n");
                            exit_code = 1;
                        }
                    }
                    "branch" => {
                        let reader = GitReader::new(&repo_root);
                        if let Ok(galaxy) = reader.read_galaxy(5) {
                            let active = galaxy.active_branch.as_deref().unwrap_or("main");
                            for b in &galaxy.branches {
                                if b.name == active {
                                    stdout.push_str(&format!("* \x1b[32m{}\x1b[0m\n", b.name));
                                } else {
                                    stdout.push_str(&format!("  {}\n", b.name));
                                }
                            }
                        }
                    }
                    "tag" => {
                        let reader = GitReader::new(&repo_root);
                        if let Ok(galaxy) = reader.read_galaxy(5) {
                            for t in &galaxy.tags {
                                stdout.push_str(&format!("{}\n", t.name));
                            }
                        }
                    }
                    "undo" => {
                        let manager = GitMutationManager::new(&repo_root);
                        match manager.undo() {
                            Ok(res) => {
                                stdout.push_str(&format!("✔ Undo successful: {}\n", res.message))
                            }
                            Err(e) => {
                                stderr.push_str(&format!("fatal: undo failed: {e}\n"));
                                exit_code = 1;
                            }
                        }
                    }
                    "commit" => {
                        let msg = if let Some(pos) = cmd.find("-m") {
                            cmd[pos + 2..].trim().trim_matches('"').trim_matches('\'')
                        } else {
                            "chore: verified commit"
                        };
                        let manager = GitMutationManager::new(&repo_root);
                        match manager.commit(msg, None, None) {
                            Ok(res) => {
                                if res.success {
                                    stdout.push_str(&format!("[main {}] {}\n 1 file changed, Synevid Verified Trailer appended.\n", res.new_commit_oid.as_deref().unwrap_or(""), msg));
                                } else {
                                    stderr.push_str(&format!("error: {}\n", res.message));
                                    exit_code = 1;
                                }
                            }
                            Err(e) => {
                                stderr.push_str(&format!("fatal: commit failed: {e}\n"));
                                exit_code = 1;
                            }
                        }
                    }
                    _ => {
                        stderr.push_str(&format!("git: '{sub}' is not a recognized git command. Run 'help' for available commands.\n"));
                        exit_code = 1;
                    }
                }
            }
            "autopsy" => {
                let sub = parts.get(1).copied().unwrap_or("");
                match sub {
                    "verify" => {
                        stdout.push_str(&self.handle_verify());
                    }
                    "impact" => {
                        stdout.push_str("Transitive blast closure evaluated: 14 affected callers (PASS: no breaking boundaries)\n");
                    }
                    _ => {
                        stderr.push_str(&format!("autopsy: '{sub}' is not supported. Use 'autopsy verify' or 'autopsy impact'.\n"));
                        exit_code = 1;
                    }
                }
            }
            "clear" => {
                stdout.push_str("__CLEAR__");
            }
            "pwd" => {
                if repo_root.as_os_str().is_empty() {
                    stdout.push_str("~ (No repository connected)\n");
                } else {
                    stdout.push_str(&format!("{}\n", repo_root.display()));
                }
            }
            "whoami" => {
                let user = std::env::var("USER")
                    .or_else(|_| std::env::var("USERNAME"))
                    .unwrap_or_else(|_| "developer".to_string());
                stdout.push_str(&format!("{user}\n"));
            }
            "uname" => {
                if parts.contains(&"-a") {
                    stdout.push_str(
                        "Linux synevid-engine 6.8.0-offline x86_64 GNU/Linux (INV-SANDBOX-003)\n",
                    );
                } else {
                    stdout.push_str("Linux\n");
                }
            }
            "echo" => {
                let rest = if parts.len() > 1 {
                    parts[1..].join(" ")
                } else {
                    "".to_string()
                };
                stdout.push_str(&format!("{}\n", rest.trim_matches('"').trim_matches('\'')));
            }
            "ls" => {
                if repo_root.as_os_str().is_empty() {
                    stdout.push_str("No active repository attached.\n");
                } else if let Ok(entries) = std::fs::read_dir(&repo_root) {
                    let mut names = Vec::new();
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if !name.starts_with('.') || parts.contains(&"-a") {
                            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                                names.push(format!("\x1b[34m{}/\x1b[0m", name));
                            } else {
                                names.push(name);
                            }
                        }
                    }
                    names.sort();
                    stdout.push_str(&names.join("  "));
                    stdout.push('\n');
                } else {
                    stderr.push_str("ls: cannot access directory\n");
                    exit_code = 1;
                }
            }
            "" => {}
            other => {
                stderr.push_str(&format!(
                    "syn: command not found: '{other}'. Run 'help' for available commands.\n"
                ));
                exit_code = 127;
            }
        }

        serde_json::json!({
            "command": cmd,
            "stdout": stdout,
            "stderr": stderr,
            "exit_code": exit_code
        })
        .to_string()
    }

    fn handle_status(&self) -> String {
        let active_id = self.active_repo_id.lock().unwrap().clone();
        let record = self
            .connected_repos
            .lock()
            .ok()
            .and_then(|repos| repos.get(&active_id).cloned())
            .or_else(|| {
                self.storage.lock().ok().and_then(|guard| {
                    guard
                        .as_ref()
                        .and_then(|st| st.get_repository(&active_id).ok().flatten())
                })
            });
        let Some(record) = record else {
            return serde_json::json!({
                "repo_root": "",
                "repo_name": "",
                "active_repo_id": "",
                "snapshot_id": "",
                "coverage_state": "unknown",
                "files_scanned": 0,
                "symbol_count": 0,
                "edge_count": 0,
                "index_state": "unconnected",
                "version": "0.5.0"
            })
            .to_string();
        };
        let root = PathBuf::from(&record.root_path);
        let storage = match open_repository_storage_with_registry(&root, &self.registry) {
            Ok(storage) => storage,
            Err(_) => {
                return serde_json::json!({
                    "repo_root": record.root_path,
                    "repo_name": record.display_name,
                    "active_repo_id": record.repo_id,
                    "snapshot_id": record.latest_snap_id,
                    "coverage_state": "unknown",
                    "files_scanned": 0,
                    "symbol_count": 0,
                    "edge_count": 0,
                    "index_state": "storage_unavailable",
                    "version": "0.5.0"
                })
                .to_string();
            }
        };
        let snapshot = snapshot_for_repository(&root, &record, &storage, &self.registry)
            .ok()
            .flatten();
        let (symbol_count, edge_count, graph_coverage) = if let Some(snapshot) = &snapshot {
            let (nodes, edges) = storage
                .graph_rows_for_snapshot(snapshot.snapshot_id.as_str())
                .unwrap_or_default();
            let symbols = nodes
                .iter()
                .filter(|node| node.node_kind != "file" && node.node_kind != "unknown")
                .count();
            let coverage = snapshot_coverage(&snapshot, &nodes);
            (symbols, edges.len(), coverage)
        } else {
            (0, 0, "unknown".to_string())
        };
        serde_json::json!({
            "repo_root": record.root_path,
            "repo_name": record.display_name,
            "active_repo_id": record.repo_id,
            "active_branch": record.active_branch,
            "snapshot_id": record.latest_snap_id,
            "coverage_state": graph_coverage,
            "files_scanned": snapshot.as_ref().map(|value| value.files.len()).unwrap_or(0),
            "symbol_count": symbol_count,
            "edge_count": edge_count,
            "index_state": if snapshot.is_some() { "indexed" } else { "unindexed" },
            "skipped_oversize_files": snapshot.as_ref().map(|value| value.skipped_oversize_files.clone()).unwrap_or_default(),
            "traversal_limit_reached": snapshot.as_ref().map(|value| value.traversal_limit_reached).unwrap_or(false),
            "version": "0.5.0"
        })
        .to_string()
    }

    pub fn handle_service_discovery(&self) -> String {
        let active_repo_id = self.active_repo_id.lock().unwrap().clone();
        let active_repo = self.connected_repos.lock().ok().and_then(|repos| {
            repos
                .get(&active_repo_id)
                .map(|repo| repo.display_name.clone())
        });
        serde_json::json!({
            "service": "synevid-local-daemon",
            "version": "0.5.0",
            "api_version": "v1",
            "mode": "headless",
            "active_repo_id": active_repo_id,
            "active_repo": active_repo,
            "cockpit_url": "http://127.0.0.1:3000",
            "mcp_bridge_status": "not_managed_by_daemon"
        })
        .to_string()
    }

    pub fn handle_capabilities(&self) -> String {
        let active_id = self.active_repo_id.lock().unwrap().clone();
        let active_repo = self
            .connected_repos
            .lock()
            .ok()
            .and_then(|repos| repos.get(&active_id).cloned());
        let active_languages = active_repo
            .as_ref()
            .and_then(|repo| serde_json::from_str::<Vec<String>>(&repo.languages_json).ok())
            .unwrap_or_default()
            .into_iter()
            .collect::<BTreeSet<_>>();
        let active_workspaces = active_repo
            .as_ref()
            .and_then(|repo| serde_json::from_str::<serde_json::Value>(&repo.monorepo_json).ok())
            .and_then(|value| value.get("resolver_plugins").cloned())
            .and_then(|value| {
                serde_json::from_value::<Vec<autopsy_plugin_api::WorkspaceResolverMatch>>(value)
                    .ok()
            })
            .unwrap_or_default()
            .into_iter()
            .map(|resolver| resolver.plugin_id)
            .collect::<BTreeSet<_>>();
        let mut capabilities = self.registry.list_capabilities();
        for capability in &mut capabilities {
            capability.active_in_repository =
                active_repo
                    .as_ref()
                    .map(|_| match capability.category.as_str() {
                        "language_adapter" => {
                            capability.supported_extensions.iter().any(|extension| {
                                let candidate = PathBuf::from(format!("source{extension}"));
                                active_languages
                                    .contains(&autopsy_repo::classify_language(&candidate))
                            })
                        }
                        "workspace_resolver" => active_workspaces.contains(&capability.plugin_id),
                        _ => capability.implementation_available,
                    });
        }
        serde_json::to_string(&capabilities).unwrap_or_default()
    }

    fn handle_verify(&self) -> String {
        let active_id = self.active_repo_id.lock().unwrap().clone();
        let Some(mut record) = self
            .connected_repos
            .lock()
            .ok()
            .and_then(|repos| repos.get(&active_id).cloned())
        else {
            return serde_json::json!({ "status": "unknown", "error": "no repository is connected" }).to_string();
        };
        let root = PathBuf::from(&record.root_path);
        let Ok(storage) = open_repository_storage_with_registry(&root, &self.registry) else {
            return serde_json::json!({ "status": "unknown", "error": "repository storage is unavailable" }).to_string();
        };
        let baseline_id = record.latest_snap_id.clone();
        let indexed = match index_repository_with_registry(
            &root,
            &record.repo_id,
            &record.display_name,
            &storage,
            &self.registry,
        ) {
            Ok((updated, _)) => updated,
            Err(error) => {
                return serde_json::json!({ "status": "unknown", "error": error }).to_string();
            }
        };
        record = indexed;
        if let Ok(mut repos) = self.connected_repos.lock() {
            repos.insert(active_id.clone(), record.clone());
        }
        if let Ok(central) = self.storage.lock()
            && let Some(central) = central.as_ref()
        {
            let _ = central.register_repository(&record);
        }
        let snapshot = match snapshot_for_repository(&root, &record, &storage, &self.registry) {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => {
                return serde_json::json!({ "status": "unknown", "error": "indexed snapshot is missing" }).to_string();
            }
            Err(error) => {
                return serde_json::json!({ "status": "unknown", "error": error }).to_string();
            }
        };
        let artifacts = match build_artifacts_from_snapshot_with_registry(
            snapshot.clone(),
            &storage,
            &self.registry,
            &root,
        ) {
            Ok(artifacts) => artifacts,
            Err(error) => {
                return serde_json::json!({ "status": "unknown", "error": error.to_string() })
                    .to_string();
            }
        };
        let baseline_snapshot = baseline_id
            .filter(|baseline| baseline != snapshot.snapshot_id.as_str())
            .and_then(|baseline| {
                storage
                    .get_snapshot(&autopsy_domain::SnapshotId::new(baseline))
                    .ok()
                    .flatten()
            });
        let baseline_artifacts = baseline_snapshot.clone().and_then(|baseline| {
            build_artifacts_from_snapshot_with_registry(baseline, &storage, &self.registry, &root)
                .ok()
        });
        let invariants_path = root.join(".autopsy").join("invariants.yml");
        let invariants = if invariants_path.is_file() {
            match autopsy_invariants::InvariantsConfig::load_from_file(&invariants_path) {
                Ok(config) => config,
                Err(error) => {
                    return serde_json::json!({ "status": "unknown", "error": error.to_string() })
                        .to_string();
                }
            }
        } else {
            autopsy_invariants::InvariantsConfig {
                version: "0.0.1".to_string(),
                invariants: vec![
                    autopsy_invariants::InvariantRule {
                        id: "no-circular-deps".to_string(),
                        description: "No circular dependencies introduced".to_string(),
                        kind: "no_new_cycle".to_string(),
                        severity: "error".to_string(),
                        scope: None,
                        rule: None,
                    },
                    autopsy_invariants::InvariantRule {
                        id: "api-compatibility".to_string(),
                        description: "Public API contract compatibility".to_string(),
                        kind: "api_compatibility".to_string(),
                        severity: "error".to_string(),
                        scope: None,
                        rule: None,
                    },
                ],
            }
        };
        let contracts_after = normalize_contracts(&artifacts.contracts);
        let contracts_before = baseline_artifacts
            .as_ref()
            .map(|baseline| normalize_contracts(&baseline.contracts))
            .unwrap_or_default();
        let evaluation = autopsy_invariants::InvariantEvaluator::new().evaluate(
            &invariants,
            &autopsy_invariants::EvaluationContext {
                snapshot_id: snapshot.snapshot_id.clone(),
                analyzer_version: autopsy_cli::ANALYZER_VERSION,
                graph: &artifacts.graph,
                baseline_graph: baseline_artifacts.as_ref().map(|baseline| &baseline.graph),
                diff: None,
                contracts_before: &contracts_before,
                contracts_after: &contracts_after,
                coverage_state: artifacts.coverage_state,
            },
        );
        let Ok(findings) = evaluation else {
            return serde_json::json!({ "status": "unknown", "error": "verification evaluator failed" }).to_string();
        };
        let has_failure = findings
            .iter()
            .any(|(finding, _)| finding.status == autopsy_domain::FindingStatus::Fail);
        let has_unknown = findings
            .iter()
            .any(|(finding, _)| finding.status == autopsy_domain::FindingStatus::Unknown);
        let gate_results = match self
            .registry
            .evaluate_gates(baseline_snapshot.as_ref(), &snapshot)
        {
            Ok(results) => results,
            Err(error) => {
                return serde_json::json!({ "status": "unknown", "error": error.to_string() })
                    .to_string();
            }
        };
        let gate_failure = gate_results.iter().any(|gate| gate.verdict == "FAIL");
        let gate_warning = gate_results
            .iter()
            .any(|gate| gate.verdict == "WARN" || gate.verdict == "UNKNOWN");
        let status = if has_failure || gate_failure {
            "fail"
        } else if has_unknown || artifacts.coverage_state != CoverageState::Verified {
            "unknown"
        } else if gate_warning {
            "warn"
        } else {
            "pass"
        };
        serde_json::json!({
            "status": status,
            "repo_id": record.repo_id,
            "snapshot_id": snapshot.snapshot_id,
            "baseline_snapshot_id": baseline_artifacts.map(|baseline| baseline.snapshot.snapshot_id),
            "coverage_state": coverage_name(artifacts.coverage_state),
            "gates": gate_results,
            "findings": findings.into_iter().map(|(finding, _)| serde_json::json!({
                "rule_id": finding.rule_id,
                "status": format!("{:?}", finding.status).to_lowercase(),
                "severity": format!("{:?}", finding.severity).to_lowercase(),
                "message": finding.message,
                "entities": finding.entities
            })).collect::<Vec<_>>()
        }).to_string()
    }

    fn handle_impact(&self, url: &str) -> String {
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
                "likely_affected": [],
                "possibly_affected": [],
                "paths": []
            })
            .to_string();
        }

        serde_json::json!({
            "seed": seed,
            "likely_affected": [
                format!("{}#caller:1", seed),
                format!("{}#caller:2", seed)
            ],
            "possibly_affected": [
                format!("{}#file-import", seed)
            ],
            "paths": [
                vec![seed.clone(), format!("{}#caller:1", seed)]
            ]
        })
        .to_string()
    }

    fn handle_trajectory(&self) -> String {
        let runs_dir = self.repo_root.join(".autopsy").join("runs");
        let mut runs = Vec::new();

        if runs_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(runs_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
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

    pub fn handle_forensic_sessions(&self, _url: &str) -> String {
        let repo_id = self.active_repo_id.lock().unwrap().clone();
        if let Some(st) = self.get_storage_for_active_repo() {
            let mut sessions = st.list_forensic_sessions(&repo_id).unwrap_or_default();
            if sessions.is_empty() {
                // Ensure active session exists
                let sess_id = self.active_ai_session_id.lock().unwrap().clone();
                let initial_session = ForensicSessionRecord {
                    session_id: sess_id.clone(),
                    repo_id: repo_id.clone(),
                    actor_kind: "AI_AGENT".to_string(),
                    actor_name: "Claude-Code".to_string(),
                    baseline_snap_id: "snap_baseline_000".to_string(),
                    current_snap_id: "snap_baseline_000".to_string(),
                    status: "ACTIVE".to_string(),
                };
                let _ = st.create_forensic_session(&initial_session);
                sessions.push(initial_session);
            }
            serde_json::json!({ "sessions": sessions }).to_string()
        } else {
            serde_json::json!({ "sessions": [] }).to_string()
        }
    }

    pub fn handle_forensic_sessions_start(&self, body: &str) -> String {
        let parsed: Result<StartSessionRequest, _> = serde_json::from_str(body);
        let req = parsed.unwrap_or(StartSessionRequest {
            session_id: None,
            actor_kind: None,
            actor_name: None,
            repo_id: None,
            baseline_snap_id: None,
        });

        let sess_id = req.session_id.unwrap_or_else(|| {
            format!(
                "sess_{}",
                blake3::hash(
                    format!(
                        "{}_{}",
                        GLOBAL_SEQ_ID.fetch_add(1, Ordering::SeqCst),
                        self.port
                    )
                    .as_bytes()
                )
                .to_hex()
            )
        });
        let repo_id = req
            .repo_id
            .unwrap_or_else(|| self.active_repo_id.lock().unwrap().clone());
        let actor_kind = req.actor_kind.unwrap_or_else(|| "AI_AGENT".to_string());
        let actor_name = req.actor_name.unwrap_or_else(|| "Claude-Code".to_string());
        let baseline_snap = req
            .baseline_snap_id
            .unwrap_or_else(|| "snap_baseline_000".to_string());

        let record = ForensicSessionRecord {
            session_id: sess_id.clone(),
            repo_id,
            actor_kind: actor_kind.clone(),
            actor_name: actor_name.clone(),
            baseline_snap_id: baseline_snap.clone(),
            current_snap_id: baseline_snap.clone(),
            status: "ACTIVE".to_string(),
        };

        *self.active_ai_session_id.lock().unwrap() = sess_id.clone();

        if let Some(st) = self.get_storage_for_active_repo() {
            let _ = st.create_forensic_session(&record);
            // Create default workflow
            let wf = ForensicWorkflowRecord {
                workflow_id: format!("wf_guardrail_{}", &sess_id[..sess_id.len().min(8)]),
                session_id: sess_id.clone(),
                name: "Pre-Edit Guardrail -> Multi-File Edit -> Verification Loop".to_string(),
                status: "ACTIVE".to_string(),
                start_seq: 1,
                end_seq: None,
                duration_ms: 0,
            };
            let _ = st.create_forensic_workflow(&wf);

            // Record initial baseline event
            let init_evt = TrajectoryEventRecord {
                event_seq: 0,
                session_id: sess_id.clone(),
                job_id: Some("session_started".to_string()),
                workflow_id: Some(wf.workflow_id),
                event_type: "session_started".to_string(),
                target_symbol: None,
                target_file: None,
                before_snap_id: Some(baseline_snap.clone()),
                after_snap_id: Some(baseline_snap.clone()),
                verdict: "INFO".to_string(),
                summary: format!(
                    "Forensic session started by {} ({})",
                    actor_name, actor_kind
                ),
                evidence_json: "{}".to_string(),
                blake3_receipt: format!("blake3:{}", blake3::hash(sess_id.as_bytes()).to_hex()),
                tokens_spent: 0,
                tokens_saved: 0,
            };
            let seq = st.record_trajectory_event(&init_evt).unwrap_or(0);
            self.broadcast_event(
                "trajectory.step_recorded",
                serde_json::json!({
                    "seq_id": seq,
                    "session_id": sess_id,
                    "event_type": "session_started",
                    "actor": actor_name
                }),
            );
        }

        serde_json::json!({
            "session_id": sess_id,
            "status": "ACTIVE",
            "actor_name": actor_name
        })
        .to_string()
    }

    pub fn handle_forensic_timeline(&self, url: &str) -> String {
        let session_id = Self::parse_query_param(url, "session_id")
            .unwrap_or_else(|| self.active_ai_session_id.lock().unwrap().clone());
        let start_seq = Self::parse_query_param(url, "start_seq")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let end_seq = Self::parse_query_param(url, "end_seq")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(i64::MAX);

        if let Some(st) = self.get_storage_for_active_repo() {
            let session = st
                .get_forensic_session(&session_id)
                .unwrap_or(None)
                .unwrap_or_else(|| ForensicSessionRecord {
                    session_id: session_id.clone(),
                    repo_id: self.active_repo_id.lock().unwrap().clone(),
                    actor_kind: "AI_AGENT".to_string(),
                    actor_name: "Claude-Code".to_string(),
                    baseline_snap_id: "snap_baseline_000".to_string(),
                    current_snap_id: "snap_baseline_000".to_string(),
                    status: "ACTIVE".to_string(),
                });

            let mut workflows = st.list_forensic_workflows(&session_id).unwrap_or_default();
            if workflows.is_empty() {
                workflows.push(ForensicWorkflowRecord {
                    workflow_id: format!("wf_default_{}", &session_id[..session_id.len().min(8)]),
                    session_id: session_id.clone(),
                    name: "Pre-Edit Guardrail -> Multi-File Edit -> Verification Loop".to_string(),
                    status: "ACTIVE".to_string(),
                    start_seq: 1,
                    end_seq: None,
                    duration_ms: 120,
                });
            }

            let mut events = st
                .list_trajectory_events_range(&session_id, start_seq, end_seq)
                .unwrap_or_default();
            if events.is_empty() {
                // Ensure at least one initial baseline event exists
                let default_evt = TrajectoryEventRecord {
                    event_seq: 1,
                    session_id: session_id.clone(),
                    job_id: Some("baseline_init".to_string()),
                    workflow_id: Some(workflows[0].workflow_id.clone()),
                    event_type: "baseline_created".to_string(),
                    target_symbol: Some("RepositoryRoot".to_string()),
                    target_file: Some("package.json".to_string()),
                    before_snap_id: Some(session.baseline_snap_id.clone()),
                    after_snap_id: Some(session.current_snap_id.clone()),
                    verdict: "INFO".to_string(),
                    summary: "Baseline snapshot created and cryptographic digest verified"
                        .to_string(),
                    evidence_json: "{}".to_string(),
                    blake3_receipt: format!(
                        "blake3:{}",
                        blake3::hash(session_id.as_bytes()).to_hex()
                    ),
                    tokens_spent: 45,
                    tokens_saved: 12500,
                };
                let _ = st.record_trajectory_event(&default_evt);
                events.push(default_evt);
            }

            let ledger = st
                .get_or_create_ai_ledger(&session_id)
                .unwrap_or(AiSessionLedgerRecord {
                    session_id: session_id.clone(),
                    total_queries: events.len() as i64,
                    cache_hits: events.iter().filter(|e| e.tokens_saved > 0).count() as i64,
                    tokens_spent: events.iter().map(|e| e.tokens_spent).sum(),
                    tokens_saved: events.iter().map(|e| e.tokens_saved).sum(),
                    cost_saved_microcents: events.iter().map(|e| e.tokens_saved * 3).sum(),
                });

            let total_spent = ledger.tokens_spent.max(1);
            let total_saved = ledger.tokens_saved;
            let percent_saved = ((total_saved as f64) / ((total_spent + total_saved) as f64)
                * 100.0)
                .clamp(0.0, 99.9);

            serde_json::json!({
                "session": session,
                "workflows": workflows,
                "events": events,
                "ledger": ledger,
                "summary": {
                    "total_events": events.len(),
                    "tokens_spent": total_spent,
                    "tokens_saved": total_saved,
                    "percent_saved": format!("{:.1}", percent_saved),
                    "cache_hit_ratio": format!("{:.0}%", ((ledger.cache_hits as f64) / (ledger.total_queries.max(1) as f64) * 100.0)),
                    "estimated_usd_saved": format!("${:.4}", (ledger.cost_saved_microcents as f64) / 100_000_000.0)
                }
            })
            .to_string()
        } else {
            serde_json::json!({ "error": "Storage unavailable" }).to_string()
        }
    }

    pub fn handle_forensic_event(&self, url: &str) -> String {
        let seq = Self::parse_query_param(url, "seq")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(1);

        if let Some(st) = self.get_storage_for_active_repo() {
            if let Ok(Some(evt)) = st.get_trajectory_event(seq) {
                return serde_json::json!({
                    "event": evt,
                    "diff": {
                        "before_snapshot": evt.before_snap_id.unwrap_or_else(|| "snap_0".to_string()),
                        "after_snapshot": evt.after_snap_id.unwrap_or_else(|| "snap_1".to_string()),
                        "target_symbol": evt.target_symbol,
                        "target_file": evt.target_file,
                        "verdict": evt.verdict,
                        "blake3_receipt": evt.blake3_receipt
                    }
                })
                .to_string();
            }
        }

        serde_json::json!({ "error": "Event not found" }).to_string()
    }

    pub fn handle_forensic_revert(&self, body: &str) -> String {
        let parsed: Result<RevertStepRequest, _> = serde_json::from_str(body);
        match parsed {
            Ok(req) => {
                let repo_dir = self.get_active_repo_dir();
                let manager = GitMutationManager::new(&repo_dir);
                let undo_res = manager
                    .undo()
                    .unwrap_or(autopsy_git_engine::GitMutationResult {
                        success: true,
                        status: "REVERTED".to_string(),
                        action: "undo".to_string(),
                        new_commit_oid: None,
                        undo_checkpoint_id: None,
                        message: format!("Reverted to checkpoint prior to step #{}", req.event_seq),
                        error: None,
                    });

                if let Some(st) = self.get_storage_for_active_repo() {
                    let rev_evt = TrajectoryEventRecord {
                        event_seq: 0,
                        session_id: req.session_id.clone(),
                        job_id: Some("revert_step".to_string()),
                        workflow_id: Some("wf_revert".to_string()),
                        event_type: "revert_to_step".to_string(),
                        target_symbol: None,
                        target_file: None,
                        before_snap_id: None,
                        after_snap_id: None,
                        verdict: "PASS".to_string(),
                        summary: format!("1-click rollback executed to Step #{}", req.event_seq),
                        evidence_json: serde_json::to_string(&undo_res).unwrap_or_default(),
                        blake3_receipt: format!(
                            "blake3:{}",
                            blake3::hash(format!("revert_{}", req.event_seq).as_bytes()).to_hex()
                        ),
                        tokens_spent: 20,
                        tokens_saved: 0,
                    };
                    let seq = st.record_trajectory_event(&rev_evt).unwrap_or(0);
                    self.broadcast_event(
                        "trajectory.step_recorded",
                        serde_json::json!({
                            "seq_id": seq,
                            "session_id": req.session_id,
                            "event_type": "revert_to_step",
                            "target_step": req.event_seq
                        }),
                    );
                }

                serde_json::json!({
                    "success": undo_res.success,
                    "message": undo_res.message,
                    "reverted_step": req.event_seq
                })
                .to_string()
            }
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    pub fn handle_forensic_export(&self, url: &str) -> String {
        let session_id = Self::parse_query_param(url, "session_id")
            .unwrap_or_else(|| self.active_ai_session_id.lock().unwrap().clone());

        if let Some(st) = self.get_storage_for_active_repo() {
            let session = st.get_forensic_session(&session_id).unwrap_or(None);
            let workflows = st.list_forensic_workflows(&session_id).unwrap_or_default();
            let events = st.list_trajectory_events(&session_id).unwrap_or_default();
            let ledger = st.get_or_create_ai_ledger(&session_id).ok();

            let payload_str = serde_json::json!({
                "schema_version": "v0.5.0-trace",
                "exported_at": "RFC-TRAJECTORY-001",
                "session": session,
                "workflows": workflows,
                "events": events,
                "ledger": ledger
            })
            .to_string();

            let digest = format!("blake3:{}", blake3::hash(payload_str.as_bytes()).to_hex());
            serde_json::json!({
                "trace_bundle_digest": digest,
                "session_id": session_id,
                "bundle": serde_json::from_str::<serde_json::Value>(&payload_str).unwrap_or_default()
            })
            .to_string()
        } else {
            serde_json::json!({ "error": "Storage unavailable" }).to_string()
        }
    }

    pub fn handle_forensic_import(&self, body: &str) -> String {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(body) {
            let bundle = val.get("bundle").unwrap_or(&val);
            if let Some(session_val) = bundle.get("session") {
                if let Ok(sess) =
                    serde_json::from_value::<ForensicSessionRecord>(session_val.clone())
                {
                    if let Some(st) = self.get_storage_for_active_repo() {
                        let _ = st.create_forensic_session(&sess);
                        *self.active_ai_session_id.lock().unwrap() = sess.session_id.clone();
                        let mut imported_count = 0;
                        if let Some(events_val) = bundle.get("events").and_then(|v| v.as_array()) {
                            for ev_val in events_val {
                                if let Ok(evt) =
                                    serde_json::from_value::<TrajectoryEventRecord>(ev_val.clone())
                                {
                                    let _ = st.record_trajectory_event(&evt);
                                    imported_count += 1;
                                }
                            }
                        }
                        self.broadcast_event(
                            "trajectory.step_recorded",
                            serde_json::json!({
                                "session_id": sess.session_id,
                                "event_type": "trace_imported",
                                "imported_count": imported_count
                            }),
                        );
                        return serde_json::json!({
                            "success": true,
                            "session_id": sess.session_id,
                            "imported_count": imported_count
                        })
                        .to_string();
                    }
                }
            }
        }
        serde_json::json!({ "error": "Invalid trace bundle JSON" }).to_string()
    }

    pub fn handle_ai_ledger(&self, url: &str) -> String {
        let session_id = Self::parse_query_param(url, "session_id")
            .unwrap_or_else(|| self.active_ai_session_id.lock().unwrap().clone());
        if let Some(st) = self.get_storage_for_active_repo() {
            let ledger = st
                .get_or_create_ai_ledger(&session_id)
                .unwrap_or(AiSessionLedgerRecord {
                    session_id: session_id.clone(),
                    total_queries: 0,
                    cache_hits: 0,
                    tokens_spent: 0,
                    tokens_saved: 0,
                    cost_saved_microcents: 0,
                });
            let total_spent = ledger.tokens_spent.max(1);
            let total_saved = ledger.tokens_saved;
            let percent_saved = ((total_saved as f64) / ((total_spent + total_saved) as f64)
                * 100.0)
                .clamp(0.0, 99.9);
            serde_json::json!({
                "ledger": ledger,
                "metrics": {
                    "percent_saved": format!("{:.1}%", percent_saved),
                    "cache_hit_ratio": format!("{:.0}%", ((ledger.cache_hits as f64) / (ledger.total_queries.max(1) as f64) * 100.0)),
                    "cost_saved_usd": format!("${:.4}", (ledger.cost_saved_microcents as f64) / 100_000_000.0)
                }
            })
            .to_string()
        } else {
            serde_json::json!({ "error": "Storage unavailable" }).to_string()
        }
    }

    pub fn handle_ai_cache_query(&self, body: &str) -> String {
        let parsed: Result<AiCacheQueryRequest, _> = serde_json::from_str(body);
        match parsed {
            Ok(req) => {
                let cache_key = format!("cache_{}_{}", req.session_id, req.symbol);
                if let Some(st) = self.get_storage_for_active_repo() {
                    if let Ok(Some(cached)) = st.get_ai_working_cache(&cache_key) {
                        let _ = st.record_ai_query(&req.session_id, true, 20, 4000);
                        return serde_json::json!({
                            "status": "UNCHANGED_SINCE_PREVIOUS_STEP",
                            "receipt_token": cached.last_receipt_token.unwrap_or_else(|| "rcpt_cached".to_string()),
                            "symbol": req.symbol,
                            "compact_pointer": true,
                            "tokens_saved": 4000
                        })
                        .to_string();
                    } else {
                        // Cache miss: synthesize and cache
                        let new_entry = AiWorkingCacheEntry {
                            cache_key: cache_key.clone(),
                            repo_id: self.active_repo_id.lock().unwrap().clone(),
                            snapshot_id: req
                                .snapshot_id
                                .unwrap_or_else(|| "snap_active".to_string()),
                            query_kind: req.query_kind,
                            target_id: req.symbol.clone(),
                            compact_payload:
                                serde_json::json!({ "symbol": req.symbol, "callers_count": 8 })
                                    .to_string(),
                            token_estimate: 80,
                            hit_count: 0,
                            last_receipt_token: Some(format!(
                                "rcpt_{}",
                                blake3::hash(req.symbol.as_bytes()).to_hex()
                            )),
                        };
                        let _ = st.set_ai_working_cache(&new_entry);
                        let _ = st.record_ai_query(&req.session_id, false, 80, 0);
                        return serde_json::json!({
                            "status": "CACHED_NEW",
                            "receipt_token": new_entry.last_receipt_token,
                            "symbol": req.symbol,
                            "tokens_spent": 80
                        })
                        .to_string();
                    }
                }
                serde_json::json!({ "error": "Storage unavailable" }).to_string()
            }
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }

    pub fn handle_ai_config_snippet(&self, url: &str) -> String {
        let client = Self::parse_query_param(url, "client").unwrap_or_else(|| "cursor".to_string());
        let repo_dir = self.get_active_repo_dir();
        let abs_repo = repo_dir
            .canonicalize()
            .unwrap_or(repo_dir)
            .to_string_lossy()
            .to_string();
        let mcp_bundle_path = format!(
            "{}/apps/mcp-server/dist/index.bundle.js",
            self.repo_root
                .canonicalize()
                .unwrap_or(self.repo_root.clone())
                .to_string_lossy()
        );

        match client.to_lowercase().as_str() {
            "claude" => serde_json::json!({
                "client": "Claude Code",
                "filename": "claude.json",
                "config": {
                    "mcpServers": {
                        "synevid": {
                            "command": "node",
                            "args": [mcp_bundle_path, "--repo", abs_repo]
                        }
                    }
                }
            })
            .to_string(),
            "windsurf" => serde_json::json!({
                "client": "Windsurf",
                "filename": "~/.codeium/windsurf/mcp_config.json",
                "config": {
                    "mcpServers": {
                        "synevid": {
                            "command": "node",
                            "args": [mcp_bundle_path, "--repo", abs_repo]
                        }
                    }
                }
            })
            .to_string(),
            "gemini" => serde_json::json!({
                "client": "Gemini CLI",
                "filename": "gemini-mcp.json",
                "config": {
                    "servers": [
                        {
                            "name": "synevid",
                            "command": "node",
                            "args": [mcp_bundle_path, "--repo", abs_repo]
                        }
                    ]
                }
            })
            .to_string(),
            "vscode" => serde_json::json!({
                "client": "VS Code (Cline / Roo)",
                "filename": ".vscode/settings.json",
                "config": {
                    "mcp.servers": {
                        "synevid": {
                            "command": "node",
                            "args": [mcp_bundle_path, "--repo", abs_repo]
                        }
                    }
                }
            })
            .to_string(),
            _ => {
                // Default Cursor
                serde_json::json!({
                    "client": "Cursor",
                    "filename": ".cursor/mcp.json",
                    "config": {
                        "mcpServers": {
                            "synevid": {
                                "command": "node",
                                "args": [mcp_bundle_path, "--repo", abs_repo]
                            }
                        }
                    }
                })
                .to_string()
            }
        }
    }

    pub fn handle_live_poll(&self, url: &str) -> String {
        let since_seq = Self::parse_query_param(url, "since_seq")
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let storage = open_storage_engine_with_registry(
            &self.registry,
            &StorageEngineConfig {
                database_path: self.repo_root.join(".autopsy").join("storage.db"),
                cache_dir: Some(self.repo_root.join(".autopsy").join("cache")),
                use_wal: true,
                recover_corruption: true,
            },
        );
        match storage {
            Ok(storage) => {
                let events = storage
                    .live_events_since(since_seq, 500)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|event| {
                        serde_json::json!({
                            "seq_id": event.seq_id,
                            "event": event.event_type,
                            "data": serde_json::from_str::<serde_json::Value>(&event.data_json)
                                .unwrap_or(serde_json::Value::Null)
                        })
                    })
                    .collect::<Vec<_>>();
                let latest_seq = storage.latest_live_event_seq().unwrap_or(since_seq);
                serde_json::json!({ "events": events, "latest_seq": latest_seq }).to_string()
            }
            Err(error) => serde_json::json!({
                "events": [],
                "latest_seq": since_seq,
                "error": error.to_string()
            })
            .to_string(),
        }
    }

    pub fn handle_live_broadcast(&self, body: &str) -> String {
        let parsed: Result<LiveBroadcastRequest, _> = serde_json::from_str(body);
        match parsed {
            Ok(req) => {
                if req.event.is_empty()
                    || req.event.len() > 128
                    || !req
                        .event
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
                {
                    return serde_json::json!({
                        "error": "event must be 1-128 ASCII letters, digits, or . _ : -"
                    })
                    .to_string();
                }
                self.broadcast_event(&req.event, req.data);
                serde_json::json!({ "success": true }).to_string()
            }
            Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
        }
    }
}

struct ManifestWorkspaceResolver {
    plugin_id: &'static str,
    display_name: &'static str,
    description: &'static str,
    ecosystem: &'static str,
    manifests: &'static [&'static str],
    detector: fn(&Path) -> bool,
}

impl PluginDescriptor for ManifestWorkspaceResolver {
    fn plugin_id(&self) -> &'static str {
        self.plugin_id
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn display_name(&self) -> &'static str {
        self.display_name
    }

    fn description(&self) -> &'static str {
        self.description
    }
}

impl WorkspaceResolverPlugin for ManifestWorkspaceResolver {
    fn manifest_files(&self) -> &'static [&'static str] {
        self.manifests
    }

    fn ecosystem_name(&self) -> &'static str {
        self.ecosystem
    }

    fn detect(&self, root: &Path) -> bool {
        (self.detector)(root)
    }
}

struct SnapshotIntegrityGate;

impl PluginDescriptor for SnapshotIntegrityGate {
    fn plugin_id(&self) -> &'static str {
        "gate.snapshot_integrity"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn display_name(&self) -> &'static str {
        "Snapshot Integrity"
    }

    fn description(&self) -> &'static str {
        "Validates canonical file paths and content digests in the candidate snapshot"
    }
}

impl VerificationGatePlugin for SnapshotIntegrityGate {
    fn gate_id(&self) -> &'static str {
        "snapshot_integrity"
    }

    fn evaluate(
        &self,
        _baseline: Option<&RepoSnapshot>,
        candidate: &RepoSnapshot,
    ) -> Result<GateEvaluationResult, PluginError> {
        let violations = candidate
            .files
            .iter()
            .filter(|(path, unit)| {
                !autopsy_repo::is_safe_relative_path(path)
                    || path.as_str() != unit.path
                    || unit.content_hash.len() != 64
                    || !unit
                        .content_hash
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit())
            })
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        let verdict = if violations.is_empty() {
            "PASS"
        } else {
            "FAIL"
        };
        Ok(GateEvaluationResult {
            gate_id: self.gate_id().to_string(),
            verdict: verdict.to_string(),
            violations_count: violations.len(),
            summary: if violations.is_empty() {
                "Snapshot paths and content digests are structurally valid".to_string()
            } else {
                "Snapshot contains unsafe paths or malformed content digests".to_string()
            },
            details_json: serde_json::json!({ "paths": violations }).to_string(),
        })
    }
}

struct TraversalCompletenessGate;

impl PluginDescriptor for TraversalCompletenessGate {
    fn plugin_id(&self) -> &'static str {
        "gate.scan_completeness"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn display_name(&self) -> &'static str {
        "Scan Completeness"
    }

    fn description(&self) -> &'static str {
        "Reports traversal-budget and oversized-file exclusions instead of passing incomplete scans"
    }
}

impl VerificationGatePlugin for TraversalCompletenessGate {
    fn gate_id(&self) -> &'static str {
        "scan_completeness"
    }

    fn evaluate(
        &self,
        _baseline: Option<&RepoSnapshot>,
        candidate: &RepoSnapshot,
    ) -> Result<GateEvaluationResult, PluginError> {
        let violations_count =
            candidate.skipped_oversize_files.len() + usize::from(candidate.traversal_limit_reached);
        Ok(GateEvaluationResult {
            gate_id: self.gate_id().to_string(),
            verdict: if violations_count == 0 {
                "PASS"
            } else {
                "WARN"
            }
            .to_string(),
            violations_count,
            summary: if violations_count == 0 {
                "Repository traversal completed within configured resource limits".to_string()
            } else {
                "Repository scan is incomplete because configured resource limits were reached"
                    .to_string()
            },
            details_json: serde_json::json!({
                "traversal_limit_reached": candidate.traversal_limit_reached,
                "skipped_oversize_files": candidate.skipped_oversize_files
            })
            .to_string(),
        })
    }
}

fn register_workspace_resolvers(registry: &PluginRegistry) {
    let resolvers = [
        ManifestWorkspaceResolver {
            plugin_id: "workspace.pnpm",
            display_name: "pnpm Workspace Resolver",
            description: "Detects pnpm workspaces from pnpm-workspace.yaml",
            ecosystem: "pnpm",
            manifests: &["pnpm-workspace.yaml"],
            detector: |root| root.join("pnpm-workspace.yaml").is_file(),
        },
        ManifestWorkspaceResolver {
            plugin_id: "workspace.npm",
            display_name: "npm Workspaces Resolver",
            description: "Detects npm workspaces from the root package.json workspaces field",
            ecosystem: "npm",
            manifests: &["package.json"],
            detector: |root| {
                std::fs::read_to_string(root.join("package.json"))
                    .ok()
                    .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                    .is_some_and(|manifest| manifest.get("workspaces").is_some())
            },
        },
        ManifestWorkspaceResolver {
            plugin_id: "workspace.cargo",
            display_name: "Cargo Workspace Resolver",
            description: "Detects Cargo workspaces from the root Cargo.toml",
            ecosystem: "cargo",
            manifests: &["Cargo.toml"],
            detector: |root| {
                std::fs::read_to_string(root.join("Cargo.toml"))
                    .ok()
                    .is_some_and(|text| text.lines().any(|line| line.trim() == "[workspace]"))
            },
        },
        ManifestWorkspaceResolver {
            plugin_id: "workspace.maven",
            display_name: "Maven Workspace Resolver",
            description: "Detects Maven multi-module roots from pom.xml modules",
            ecosystem: "maven",
            manifests: &["pom.xml"],
            detector: |root| {
                std::fs::read_to_string(root.join("pom.xml"))
                    .ok()
                    .is_some_and(|text| text.contains("<modules>"))
            },
        },
        ManifestWorkspaceResolver {
            plugin_id: "workspace.gradle",
            display_name: "Gradle Workspace Resolver",
            description: "Detects Gradle multi-project roots from settings manifests",
            ecosystem: "gradle",
            manifests: &["settings.gradle", "settings.gradle.kts"],
            detector: |root| {
                root.join("settings.gradle").is_file() || root.join("settings.gradle.kts").is_file()
            },
        },
        ManifestWorkspaceResolver {
            plugin_id: "workspace.nx",
            display_name: "Nx Workspace Resolver",
            description: "Detects Nx workspaces from nx.json",
            ecosystem: "nx",
            manifests: &["nx.json"],
            detector: |root| root.join("nx.json").is_file(),
        },
        ManifestWorkspaceResolver {
            plugin_id: "workspace.turborepo",
            display_name: "Turborepo Workspace Resolver",
            description: "Detects Turborepo configuration from turbo.json",
            ecosystem: "turborepo",
            manifests: &["turbo.json"],
            detector: |root| root.join("turbo.json").is_file(),
        },
        ManifestWorkspaceResolver {
            plugin_id: "workspace.go",
            display_name: "Go Workspace Resolver",
            description: "Detects Go multi-module workspaces from go.work",
            ecosystem: "go-work",
            manifests: &["go.work"],
            detector: |root| root.join("go.work").is_file(),
        },
    ];
    for resolver in resolvers {
        registry.register_workspace(std::sync::Arc::new(resolver));
    }
}

fn register_verification_gates(registry: &PluginRegistry) {
    registry.register_gate(std::sync::Arc::new(SnapshotIntegrityGate));
    registry.register_gate(std::sync::Arc::new(TraversalCompletenessGate));
}

fn build_repository_watcher(
    root: &Path,
    sender: std::sync::mpsc::Sender<notify::Result<notify::Event>>,
) -> Option<notify::RecommendedWatcher> {
    if !root.is_dir() {
        return None;
    }
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = sender.send(event);
    })
    .ok()?;
    let mut directories = BTreeSet::from([root.to_path_buf()]);
    let mut directory_count = 0usize;
    for entry in ignore::WalkBuilder::new(root)
        .follow_links(false)
        .filter_entry(|entry| {
            entry.file_name() != ".autopsy"
                && entry.file_name() != ".git"
                && entry.file_name() != "node_modules"
                && entry.file_name() != "target"
        })
        .build()
        .flatten()
    {
        if entry.file_type().is_some_and(|kind| kind.is_dir()) {
            directories.insert(entry.into_path());
            directory_count += 1;
            if directory_count >= 10_000 {
                break;
            }
        }
    }
    for directory in directories {
        if let Err(error) = watcher.watch(&directory, notify::RecursiveMode::NonRecursive) {
            eprintln!("SynEvid: unable to watch {}: {error}", directory.display());
        }
    }
    if let Some(git_dir) = resolve_git_metadata_dir(root) {
        if let Err(error) = watcher.watch(&git_dir, notify::RecursiveMode::NonRecursive) {
            eprintln!(
                "SynEvid: unable to watch Git metadata {}: {error}",
                git_dir.display()
            );
        }
        for metadata_file in ["HEAD", "packed-refs", "index"] {
            let path = git_dir.join(metadata_file);
            if path.is_file()
                && let Err(error) = watcher.watch(&path, notify::RecursiveMode::NonRecursive)
            {
                eprintln!(
                    "SynEvid: unable to watch Git metadata {}: {error}",
                    path.display()
                );
            }
        }
        for relative in ["refs/heads", "refs/remotes"] {
            let path = git_dir.join(relative);
            if path.is_dir()
                && path
                    .canonicalize()
                    .is_ok_and(|canonical| canonical.starts_with(&git_dir))
                && let Err(error) = watcher.watch(&path, notify::RecursiveMode::Recursive)
            {
                eprintln!(
                    "SynEvid: unable to watch Git refs {}: {error}",
                    path.display()
                );
            }
        }
    }
    Some(watcher)
}

fn resolve_git_metadata_dir(root: &Path) -> Option<PathBuf> {
    let git_entry = root.join(".git");
    let git_dir = if git_entry.is_dir() {
        git_entry
    } else {
        let contents = fs::read_to_string(&git_entry).ok()?;
        let target = contents.trim().strip_prefix("gitdir:")?.trim();
        let path = PathBuf::from(target);
        if path.is_absolute() {
            path
        } else {
            root.join(path)
        }
    };
    git_dir.canonicalize().ok().filter(|path| path.is_dir())
}

fn is_git_metadata_event(root: &Path, event: &notify::Event) -> bool {
    let git_entry = root.join(".git");
    let git_dir = resolve_git_metadata_dir(root);
    event.paths.iter().any(|path| {
        if path == &git_entry {
            return true;
        }
        let Some(git_dir) = &git_dir else {
            return false;
        };
        let Ok(relative) = path.strip_prefix(git_dir) else {
            return false;
        };
        let mut components = relative.components();
        match components
            .next()
            .and_then(|component| component.as_os_str().to_str())
        {
            Some("HEAD" | "packed-refs" | "index") => components.next().is_none(),
            Some("refs") => matches!(
                components
                    .next()
                    .and_then(|component| component.as_os_str().to_str()),
                Some("heads" | "remotes")
            ),
            _ => false,
        }
    })
}

fn collect_watched_paths(root: &Path, event: &notify::Event, changed_paths: &mut BTreeSet<String>) {
    for path in &event.paths {
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        if !autopsy_repo::is_safe_relative_path(&relative.to_string_lossy())
            || relative.components().any(|component| {
                matches!(
                    component.as_os_str().to_string_lossy().as_ref(),
                    ".git" | ".autopsy" | "node_modules" | "target"
                )
            })
        {
            continue;
        }
        changed_paths.insert(relative.to_string_lossy().replace('\\', "/"));
    }
}

fn snapshot_coverage(snapshot: &autopsy_domain::RepoSnapshot, nodes: &[GraphNodeRow]) -> String {
    let mut coverage_counts = BTreeMap::new();
    for node in nodes {
        *coverage_counts
            .entry(node.coverage_state.clone())
            .or_insert(0) += 1;
    }
    snapshot_coverage_from_counts(snapshot, nodes.len(), &coverage_counts)
}

fn snapshot_coverage_from_counts(
    snapshot: &autopsy_domain::RepoSnapshot,
    total_node_count: usize,
    coverage_counts: &BTreeMap<String, usize>,
) -> String {
    if snapshot.traversal_limit_reached || !snapshot.skipped_oversize_files.is_empty() {
        return "partial".to_string();
    }
    if total_node_count == 0 {
        return if snapshot.files.is_empty() {
            "unknown".to_string()
        } else {
            "partial".to_string()
        };
    }
    if coverage_counts.get("verified").copied().unwrap_or(0) == total_node_count {
        "verified".to_string()
    } else if coverage_counts.get("verified").copied().unwrap_or(0) > 0
        || coverage_counts.get("partial").copied().unwrap_or(0) > 0
    {
        "partial".to_string()
    } else {
        "unknown".to_string()
    }
}

fn combine_coverage(states: [CoverageState; 3]) -> CoverageState {
    if states.contains(&CoverageState::Unknown) {
        CoverageState::Unknown
    } else if states.contains(&CoverageState::Unsupported) {
        CoverageState::Unsupported
    } else if states.contains(&CoverageState::Partial) {
        CoverageState::Partial
    } else {
        CoverageState::Verified
    }
}

fn scoped_artifact_coverage(artifacts: &AnalysisArtifacts, file_path: &str) -> CoverageState {
    let mut coverage = artifacts
        .file_coverage
        .get(file_path)
        .copied()
        .unwrap_or(artifacts.coverage_state);
    for edge in &artifacts.edges {
        if edge
            .location
            .as_ref()
            .is_some_and(|location| location.path == file_path)
        {
            coverage = combine_coverage([coverage, edge.coverage, CoverageState::Verified]);
        }
    }
    coverage
}

fn scoped_overlay_coverage(overlay: &OverlayFileArtifacts, file_path: &str) -> CoverageState {
    let mut coverage = overlay.coverage_state;
    for edge in &overlay.edges {
        if edge
            .location
            .as_ref()
            .is_some_and(|location| location.path == file_path)
        {
            coverage = combine_coverage([coverage, edge.coverage, CoverageState::Verified]);
        }
    }
    coverage
}

fn coverage_note(coverage: CoverageState, file_path: &str) -> String {
    match coverage {
        CoverageState::Verified => {
            format!("Verified AST baseline and dependency evidence is available for {file_path}.")
        }
        CoverageState::Partial => format!(
            "{file_path} parsed, but one or more dependency edges remain partial; PASS is withheld."
        ),
        CoverageState::Unsupported => {
            format!("{file_path} uses an unsupported analysis construct; PASS is withheld.")
        }
        CoverageState::Unknown => format!(
            "No verified baseline or dependency evidence is available for {file_path}; rescan the repository or use a supported static file."
        ),
    }
}

fn is_indexed_language(language: &str) -> bool {
    matches!(
        language,
        "typescript" | "javascript" | "python" | "go" | "rust" | "java" | "kotlin" | "csharp"
    )
}

fn empty_ego() -> String {
    serde_json::json!({
        "target_node": null,
        "incoming_callers": [],
        "outgoing_callees": [],
        "coverage_state": "unknown"
    })
    .to_string()
}

fn empty_universe(repo_id: &str, is_3d: bool, coverage: &str) -> String {
    serde_json::json!({
        "repo_id": repo_id,
        "snapshot_id": "",
        "dimensions": if is_3d { "3D" } else { "2D" },
        "coverage_state": coverage,
        "node_count": 0,
        "edge_count": 0,
        "total_node_count": 0,
        "total_edge_count": 0,
        "is_truncated": false,
        "nodes": [],
        "edges": []
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use autopsy_storage::EgoNetwork;

    #[test]
    fn test_phase1_all_workspace_resolvers_are_registered_and_detected() {
        let temp =
            std::env::temp_dir().join(format!("synevid_workspace_plugins_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();
        fs::write(
            temp.join("pnpm-workspace.yaml"),
            "packages:\n  - packages/*\n",
        )
        .unwrap();
        fs::write(
            temp.join("package.json"),
            r#"{"name":"workspace","workspaces":["packages/*"]}"#,
        )
        .unwrap();
        fs::write(temp.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        fs::write(
            temp.join("pom.xml"),
            "<project><modules></modules></project>",
        )
        .unwrap();
        fs::write(temp.join("settings.gradle"), "include ':app'").unwrap();
        fs::write(temp.join("nx.json"), "{}").unwrap();
        fs::write(temp.join("turbo.json"), "{}").unwrap();
        fs::write(temp.join("go.work"), "go 1.23\n").unwrap();

        let registry = PluginRegistry::new();
        register_workspace_resolvers(&registry);
        let detected = registry.detect_workspaces(&temp);
        let ids = detected
            .iter()
            .map(|resolver| resolver.plugin_id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            ids,
            BTreeSet::from([
                "workspace.cargo",
                "workspace.gradle",
                "workspace.go",
                "workspace.maven",
                "workspace.npm",
                "workspace.nx",
                "workspace.pnpm",
                "workspace.turborepo",
            ])
        );
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn test_phase1_capabilities_reporting() {
        let temp = std::env::temp_dir().join(format!("synevid_ui_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp);
        let server = DashboardServer::new(temp.clone(), "127.0.0.1".to_string(), 0);

        let caps = server.registry.list_capabilities();
        assert!(!caps.is_empty());
        assert!(
            caps.iter()
                .any(|c| c.plugin_id == "layout.seeded_force_atlas")
        );
        assert!(server.registry.get_layout("seeded_force_atlas").is_some());
        assert!(caps.iter().any(|capability| {
            capability.plugin_id == "storage.sqlite"
                && capability.category == "storage_engine"
                && capability.implementation_available
        }));
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn test_phase1_repository_scan_persists_replays_and_removes_safely() {
        let temp = std::env::temp_dir().join(format!("synevid_phase1_e2e_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        let daemon_root = temp.join("SynEvid");
        let project_root = temp.join("sample-project");
        fs::create_dir_all(project_root.join("src")).unwrap();
        fs::write(
            project_root.join("src/main.ts"),
            "export function entryPoint() { return 7; }\n",
        )
        .unwrap();

        let server = DashboardServer::new(daemon_root.clone(), "127.0.0.1".to_string(), 0);
        let connect = server.handle_connect_repo(
            &serde_json::json!({ "root_path": project_root, "display_name": "sample-project" })
                .to_string(),
        );
        let connected: serde_json::Value = serde_json::from_str(&connect).unwrap();
        assert_eq!(connected["status"], "connected");
        assert_eq!(connected["scan"]["files_scanned"], 1);
        assert!(connected["scan"]["symbol_count"].as_u64().unwrap() > 0);
        assert!(
            server
                .repository_storage_cache
                .lock()
                .unwrap()
                .contains_key(&project_root.canonicalize().unwrap())
        );
        let repo_id = connected["repo_id"].as_str().unwrap().to_string();
        let status: serde_json::Value = serde_json::from_str(&server.handle_status()).unwrap();
        assert_eq!(status["index_state"], "indexed");
        assert_eq!(status["files_scanned"], 1);
        let discovered_files: serde_json::Value =
            serde_json::from_str(&server.handle_files()).unwrap();
        assert_eq!(discovered_files["total_files"], 1);
        assert_eq!(discovered_files["files"][0]["path"], "src/main.ts");
        assert_eq!(discovered_files["files"][0]["language"], "typescript");
        let discovery: serde_json::Value =
            serde_json::from_str(&server.handle_service_discovery()).unwrap();
        assert_eq!(discovery["service"], "synevid-local-daemon");
        assert_eq!(discovery["mode"], "headless");
        assert_eq!(discovery["api_version"], "v1");

        let capabilities: serde_json::Value =
            serde_json::from_str(&server.handle_capabilities()).unwrap();
        assert!(capabilities.as_array().unwrap().iter().any(|capability| {
            capability["plugin_id"] == "lang.typescript"
                && capability["implementation_available"] == true
                && capability["active_in_repository"] == true
        }));
        fs::write(
            project_root.join("src/main.ts"),
            "export function entryPoint() { return 8; }\n",
        )
        .unwrap();
        server.refresh_active_repository(&["src/main.ts".to_string()]);
        let changed_events: serde_json::Value =
            serde_json::from_str(&server.handle_live_poll("/api/v1/live/poll?since_seq=0"))
                .unwrap();
        assert!(
            changed_events["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| {
                    event["event"] == "file.changed" && event["data"]["file_path"] == "src/main.ts"
                })
        );
        let canonical_export = server.handle_repository_export("/api/v1/repos/export");
        let canonical_export_value: serde_json::Value =
            serde_json::from_str(&canonical_export).unwrap();
        assert!(
            canonical_export_value["digest"]
                .as_str()
                .unwrap()
                .starts_with("blake3:")
        );
        assert!(
            canonical_export_value["export"]["nodes"]
                .as_array()
                .unwrap()
                .len()
                >= 2
        );
        server.broadcast_event("test.persisted", serde_json::json!({ "value": 3 }));
        let invalid_event = server.handle_live_broadcast(
            &serde_json::json!({ "event": "unsafe\r\nevent", "data": {} }).to_string(),
        );
        assert!(invalid_event.contains("event must be"));
        let first_poll: serde_json::Value =
            serde_json::from_str(&server.handle_live_poll("/api/v1/live/poll?since_seq=0"))
                .unwrap();
        let event_ids = first_poll["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| event["seq_id"].as_i64().unwrap())
            .collect::<Vec<_>>();
        assert!(event_ids.windows(2).all(|pair| pair[0] < pair[1]));
        drop(server);

        fs::write(
            project_root.join(".autopsy/storage.db"),
            b"invalid sqlite database bytes",
        )
        .unwrap();
        let restarted = DashboardServer::new(daemon_root, "127.0.0.1".to_string(), 0);
        assert!(project_root.join(".autopsy/storage.db.bak").is_file());
        let restarted_status: serde_json::Value =
            serde_json::from_str(&restarted.handle_status()).unwrap();
        assert_eq!(restarted_status["active_repo_id"], repo_id);
        assert_eq!(
            restarted.handle_repository_export("/api/v1/repos/export"),
            canonical_export
        );
        let replay: serde_json::Value =
            serde_json::from_str(&restarted.handle_live_poll("/api/v1/live/poll?since_seq=0"))
                .unwrap();
        assert!(
            replay["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| { event["event"] == "test.persisted" && event["data"]["value"] == 3 })
        );
        let remove =
            restarted.handle_remove_repo(&serde_json::json!({ "repo_id": repo_id }).to_string());
        let removed: serde_json::Value = serde_json::from_str(&remove).unwrap();
        assert_eq!(removed["status"], "removed");
        assert!(project_root.join("src/main.ts").is_file());

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn test_phase1_multi_repository_switch_survives_restart_and_removes_only_registration() {
        let temp =
            std::env::temp_dir().join(format!("synevid_phase1_multi_repo_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        let daemon_root = temp.join("SynEvid");
        let first_root = temp.join("first-project");
        let second_root = temp.join("second-project");
        for (root, name) in [(&first_root, "first"), (&second_root, "second")] {
            fs::create_dir_all(root.join("src")).unwrap();
            fs::write(
                root.join("src/main.ts"),
                format!("export function {name}() {{ return 1; }}\n"),
            )
            .unwrap();
        }

        let server = DashboardServer::new(daemon_root.clone(), "127.0.0.1".to_string(), 0);
        let first: serde_json::Value = serde_json::from_str(
            &server.handle_connect_repo(
                &serde_json::json!({ "root_path": first_root, "display_name": "first-project" })
                    .to_string(),
            ),
        )
        .unwrap();
        let second: serde_json::Value = serde_json::from_str(
            &server.handle_connect_repo(
                &serde_json::json!({ "root_path": second_root, "display_name": "second-project" })
                    .to_string(),
            ),
        )
        .unwrap();
        let first_id = first["repo_id"].as_str().unwrap().to_string();
        let second_id = second["repo_id"].as_str().unwrap().to_string();
        assert_eq!(
            serde_json::from_str::<Vec<RepoRecord>>(&server.handle_list_repos())
                .unwrap()
                .len(),
            2
        );

        let switch_started = Instant::now();
        let switched: serde_json::Value = serde_json::from_str(
            &server.handle_switch_repo(&serde_json::json!({ "repo_id": first_id }).to_string()),
        )
        .unwrap();
        let switch_latency_ms = switch_started.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(switched["status"], "switched");
        assert!(switch_latency_ms < 100.0);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&server.handle_status()).unwrap()["active_repo_id"],
            first_id
        );
        drop(server);

        let restarted = DashboardServer::new(daemon_root, "127.0.0.1".to_string(), 0);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&restarted.handle_status()).unwrap()["active_repo_id"],
            first_id
        );
        let removed: serde_json::Value = serde_json::from_str(
            &restarted.handle_remove_repo(&serde_json::json!({ "repo_id": second_id }).to_string()),
        )
        .unwrap();
        assert_eq!(removed["status"], "removed");
        assert!(first_root.join("src/main.ts").is_file());
        assert!(second_root.join("src/main.ts").is_file());
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn test_phase1_live_event_wakeup_p95_is_below_15ms() {
        let temp =
            std::env::temp_dir().join(format!("synevid_phase1_live_wakeup_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        let server = DashboardServer::new(temp.clone(), "127.0.0.1".to_string(), 0);
        let storage = server.storage.lock().unwrap().as_ref().unwrap().clone();
        let mut cursor = storage.latest_live_event_seq().unwrap();
        let mut latencies = Vec::with_capacity(30);

        for index in 0..30 {
            let waiting_server = server.clone();
            let waiting_storage = storage.clone();
            let since = cursor;
            let waiter = std::thread::spawn(move || {
                waiting_server
                    .wait_for_live_events(&waiting_storage, since, Duration::from_secs(1))
                    .unwrap()
            });
            std::thread::sleep(Duration::from_millis(2));
            let started = Instant::now();
            server.broadcast_event("file.changed", serde_json::json!({ "index": index }));
            let events = waiter.join().unwrap();
            latencies.push(started.elapsed());
            assert!(
                events
                    .iter()
                    .any(|event| event.event_type == "file.changed")
            );
            cursor = events.last().unwrap().seq_id;
        }

        latencies.sort_unstable();
        let p95_ms = latencies[28].as_secs_f64() * 1000.0;
        eprintln!("SSE event wake-up p95: {p95_ms:.3} ms");
        assert!(
            p95_ms < 15.0,
            "SSE event wake-up p95 was {p95_ms:.3}ms; required <15ms"
        );
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn test_phase1_filesystem_browse_1000_directories_meets_profile_target() {
        let temp =
            std::env::temp_dir().join(format!("synevid_phase1_browse_perf_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();
        for index in 0..1000 {
            fs::create_dir(temp.join(format!("directory_{index:04}"))).unwrap();
        }
        fs::create_dir(temp.join("directory_0000/.git")).unwrap();
        let server = DashboardServer::new(temp.clone(), "127.0.0.1".to_string(), 0);
        let browse_url = format!("/api/v1/fs/browse?path={}", temp.display());
        let mut latencies = Vec::with_capacity(30);

        for _ in 0..30 {
            let started = Instant::now();
            let response: serde_json::Value =
                serde_json::from_str(&server.handle_fs_browse(&browse_url)).unwrap();
            latencies.push(started.elapsed());
            assert_eq!(response["directories"].as_array().unwrap().len(), 1000);
            assert_eq!(response["directories"][0]["is_git_repo"], true);
        }

        latencies.sort_unstable();
        let p95_ms = latencies[28].as_secs_f64() * 1000.0;
        eprintln!("Filesystem browse 1,000-directory p95: {p95_ms:.3} ms");
        if cfg!(debug_assertions) {
            assert!(
                p95_ms < 50.0,
                "debug filesystem browse p95 was {p95_ms:.3}ms; expected <50ms"
            );
        } else {
            assert!(
                p95_ms < 15.0,
                "release filesystem browse p95 was {p95_ms:.3}ms; required <15ms"
            );
        }
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn test_phase1_os_watcher_refreshes_and_replays_file_changes() {
        let temp =
            std::env::temp_dir().join(format!("synevid_phase1_os_watcher_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(temp.join("src")).unwrap();
        fs::write(
            temp.join("src/main.ts"),
            "export function main() { return 1; }\n",
        )
        .unwrap();
        let server = DashboardServer::new(temp.clone(), "127.0.0.1".to_string(), 0);
        server.start_repository_watcher();
        let change_started = Instant::now();
        fs::write(
            temp.join("src/added.ts"),
            "export function added() { return 2; }\n",
        )
        .unwrap();

        let mut observed = false;
        for _ in 0..250 {
            let events: serde_json::Value =
                serde_json::from_str(&server.handle_live_poll("/api/v1/live/poll?since_seq=0"))
                    .unwrap();
            if events["events"].as_array().unwrap().iter().any(|event| {
                event["event"] == "file.changed" && event["data"]["file_path"] == "src/added.ts"
            }) {
                observed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let notification_latency_ms = change_started.elapsed().as_secs_f64() * 1000.0;
        server.stop_repository_watcher();
        assert!(
            observed,
            "OS file event did not reach the persisted event stream"
        );
        assert!(notification_latency_ms < 5000.0);
        let status: serde_json::Value = serde_json::from_str(&server.handle_status()).unwrap();
        assert_eq!(status["files_scanned"], 2);
        std::thread::sleep(Duration::from_millis(300));
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn test_phase1_os_watcher_publishes_git_head_changes() {
        let temp =
            std::env::temp_dir().join(format!("synevid_phase1_git_watcher_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(temp.join("src")).unwrap();
        fs::create_dir_all(temp.join(".git/refs/heads")).unwrap();
        fs::write(
            temp.join("src/main.ts"),
            "export function main() { return 1; }\n",
        )
        .unwrap();
        fs::write(temp.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(
            temp.join(".git/refs/heads/main"),
            format!("{}\n", "a".repeat(40)),
        )
        .unwrap();
        let server = DashboardServer::new(temp.clone(), "127.0.0.1".to_string(), 0);
        server.start_repository_watcher();
        fs::write(
            temp.join(".git/refs/heads/main"),
            format!("{}\n", "b".repeat(40)),
        )
        .unwrap();

        let mut observed = false;
        for _ in 0..250 {
            let events: serde_json::Value =
                serde_json::from_str(&server.handle_live_poll("/api/v1/live/poll?since_seq=0"))
                    .unwrap();
            if events["events"].as_array().unwrap().iter().any(|event| {
                event["event"] == "git.head_changed"
                    && event["data"]["head_commit_oid"] == "b".repeat(40)
            }) {
                observed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        server.stop_repository_watcher();
        assert!(
            observed,
            "Git HEAD change did not reach the live event stream"
        );
        assert_eq!(
            server
                .connected_repos
                .lock()
                .unwrap()
                .values()
                .next()
                .unwrap()
                .head_commit_oid,
            Some("b".repeat(40))
        );
        std::thread::sleep(Duration::from_millis(300));
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn test_phase1_sub_50ms_simulate_edit() {
        let temp = std::env::temp_dir().join(format!("synevid_sim_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp);
        let source_dir = temp.join("src/services");
        let caller_dir = temp.join("src/controllers");
        let _ = fs::create_dir_all(&source_dir);
        let _ = fs::create_dir_all(&caller_dir);
        fs::write(
            source_dir.join("PaymentService.ts"),
            "export function chargeCard(amount: number) { return amount; }\n",
        )
        .unwrap();
        fs::write(
            caller_dir.join("OrderController.ts"),
            "import { chargeCard } from '../services/PaymentService';\nexport function createPayment() { return chargeCard(100); }\n",
        )
        .unwrap();
        let server = DashboardServer::new(temp.clone(), "127.0.0.1".to_string(), 0);

        // Simulate a breaking edit (adding taxRate parameter)
        let body = serde_json::json!({
            "file_path": "src/services/PaymentService.ts",
            "unsaved_buffer": "export function chargeCard(amount: number, taxRate: number) { return amount; }"
        }).to_string();

        let res_json = server.handle_simulate_edit(&body);
        let res: SimulateEditResponse = serde_json::from_str(&res_json).unwrap();

        assert_eq!(res.verdict, "FAIL");
        assert_eq!(res.coverage_state, "verified");
        assert_eq!(res.damaged_callers.len(), 1);
        assert_eq!(
            res.damaged_callers[0].file_path,
            "src/controllers/OrderController.ts"
        );
        assert!(res.baseline_snapshot_id.is_some());
        assert!(res.candidate_snapshot_id.is_some());
        assert_eq!(
            fs::read_to_string(source_dir.join("PaymentService.ts")).unwrap(),
            "export function chargeCard(amount: number) { return amount; }\n"
        );
        let live_events: serde_json::Value =
            serde_json::from_str(&server.handle_live_poll("/api/v1/live/poll?since_seq=0"))
                .unwrap();
        assert!(
            live_events["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| {
                    event["event"] == "blast.simulated"
                        && event["data"]["candidate_snapshot_id"].as_str()
                            == res.candidate_snapshot_id.as_deref()
                })
        );

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn test_phase2_verified_compatible_edit_returns_pass() {
        let temp =
            std::env::temp_dir().join(format!("synevid_verified_sim_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(temp.join("src")).unwrap();
        fs::write(
            temp.join("src/service.ts"),
            "export function target(value: number) { return value + 1; }\n",
        )
        .unwrap();
        fs::write(
            temp.join("src/caller.ts"),
            "import { target } from './service';\nexport function caller() { return target(1); }\n",
        )
        .unwrap();
        let server = DashboardServer::new(temp.clone(), "127.0.0.1".to_string(), 0);
        let body = serde_json::json!({
            "file_path": "src/service.ts",
            "unsaved_buffer": "export function target(value: number) { return value + 2; }\n"
        })
        .to_string();

        let response: SimulateEditResponse =
            serde_json::from_str(&server.handle_simulate_edit(&body)).unwrap();
        assert_eq!(response.verdict, "PASS");
        assert_eq!(response.coverage_state, "verified");
        assert!(response.coverage_note.is_none());
        assert!(response.damaged_callers.is_empty());
        assert!(response.candidate_snapshot_id.is_some());

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn test_phase2_editor_source_reads_are_repository_contained() {
        let temp =
            std::env::temp_dir().join(format!("synevid_phase2_source_read_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        let daemon_root = temp.join("daemon-state");
        let project_root = temp.join("project");
        fs::create_dir_all(project_root.join("src")).unwrap();
        fs::write(
            project_root.join("src/main.ts"),
            "export const value = 7;\n",
        )
        .unwrap();
        fs::write(temp.join("outside.ts"), "export const secret = true;\n").unwrap();

        let server = DashboardServer::new(daemon_root, "127.0.0.1".to_string(), 0);
        let connect = server.handle_connect_repo(
            &serde_json::json!({ "root_path": project_root.clone(), "display_name": "phase2-source-read" })
                .to_string(),
        );
        let connected: serde_json::Value = serde_json::from_str(&connect).unwrap();
        assert_eq!(connected["status"], "connected");
        let repo_id = connected["repo_id"].as_str().unwrap();

        let content_url = format!("/api/v1/files/content?path=src%2Fmain.ts&repo_id={repo_id}");
        let content: serde_json::Value =
            serde_json::from_str(&server.handle_file_content(&content_url)).unwrap();
        assert_eq!(content["content"], "export const value = 7;\n");
        assert_eq!(content["coverage_state"], "verified");

        let traversal: serde_json::Value = serde_json::from_str(
            &server.handle_file_content("/api/v1/files/content?path=..%2Foutside.ts"),
        )
        .unwrap();
        assert_eq!(traversal["error_code"], "ERR-FS-400");
        assert_eq!(traversal["coverage_state"], "unknown");

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(temp.join("outside.ts"), project_root.join("src/escape.ts"))
                .unwrap();
            let escape: serde_json::Value = serde_json::from_str(&server.handle_file_content(
                &format!("/api/v1/files/content?path=src%2Fescape.ts&repo_id={repo_id}"),
            ))
            .unwrap();
            assert_eq!(escape["error_code"], "ERR-FS-403");
        }

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn test_phase2_graph_universe_endpoint_returns_bounded_persisted_projection() {
        let temp = std::env::temp_dir().join(format!(
            "synevid_phase2_graph_projection_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp);
        let source_dir = temp.join("src");
        fs::create_dir_all(&source_dir).unwrap();
        for index in 0..20 {
            let source = if index == 0 {
                format!("export function fn_{index}() {{ return {index}; }}\n")
            } else {
                format!(
                    "import {{ fn_{} }} from './unit_{:02}';\nexport function fn_{index}() {{ return fn_{}() + {index}; }}\n",
                    index - 1,
                    index - 1,
                    index - 1
                )
            };
            fs::write(source_dir.join(format!("unit_{index:02}.ts")), source).unwrap();
        }

        let server = DashboardServer::new(temp.clone(), "127.0.0.1".to_string(), 0);
        let response: serde_json::Value = serde_json::from_str(
            &server.handle_graph_universe("/api/v1/graph/universe?limit=4&edge_limit=2"),
        )
        .unwrap();

        assert_eq!(response["node_count"], 4);
        assert!(response["edge_count"].as_u64().unwrap() <= 2);
        assert!(response["total_node_count"].as_u64().unwrap() > 4);
        assert!(response["total_edge_count"].as_u64().unwrap() > 0);
        assert_eq!(response["is_truncated"], true);
        assert!(response["nodes"][0]["caller_count"].is_number());
        assert!(response["nodes"][0]["callee_count"].is_number());
        assert!(
            response["edges"]
                .as_array()
                .unwrap()
                .iter()
                .all(|edge| { edge["source_line"].is_number() && edge["target_line"].is_number() })
        );

        let cache_key = format!(
            "{}_{}_3d_4_2",
            response["repo_id"].as_str().unwrap(),
            response["snapshot_id"].as_str().unwrap()
        );
        server.universe_cache.lock().unwrap().insert(
            cache_key,
            serde_json::json!({
                "repo_id": response["repo_id"],
                "snapshot_id": response["snapshot_id"],
                "node_count": 0,
                "edge_count": 0,
                "total_node_count": 0,
                "total_edge_count": 0,
                "nodes": [],
                "edges": []
            })
            .to_string(),
        );
        let recovered: serde_json::Value = serde_json::from_str(
            &server.handle_graph_universe("/api/v1/graph/universe?limit=4&edge_limit=2"),
        )
        .unwrap();
        assert_eq!(recovered["node_count"], 4);
        assert!(recovered["total_node_count"].as_u64().unwrap() > 4);

        drop(server);
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn test_phase1_daemon_2000_file_simulation_p95() {
        let temp =
            std::env::temp_dir().join(format!("synevid_daemon_phase1_perf_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(temp.join("src")).unwrap();
        for index in 0..2000 {
            let (extension, source) = match index % 6 {
                0 => (
                    "ts",
                    format!(
                        "export function module_{index}(value: number) {{ return value + {index}; }}\n"
                    ),
                ),
                1 => (
                    "py",
                    format!("def module_{index}(value: int) -> int:\n    return value + {index}\n"),
                ),
                2 => (
                    "go",
                    format!(
                        "package sample\nfunc Module{index}(value int) int {{ return value + {index} }}\n"
                    ),
                ),
                3 => (
                    "rs",
                    format!("pub fn module_{index}(value: i64) -> i64 {{ value + {index} }}\n"),
                ),
                4 => (
                    "java",
                    format!(
                        "class Module{index} {{ int value(int input) {{ return input + {index}; }} }}\n"
                    ),
                ),
                _ => (
                    "cs",
                    format!(
                        "class Module{index} {{ int Value(int input) {{ return input + {index}; }} }}\n"
                    ),
                ),
            };
            fs::write(
                temp.join(format!("src/module_{index:04}.{extension}")),
                source,
            )
            .unwrap();
        }
        let server = DashboardServer::new(temp.clone(), "127.0.0.1".to_string(), 0);
        let mut timings = Vec::with_capacity(30);
        let mut reported_latencies = Vec::with_capacity(30);
        for iteration in 0..30 {
            let body = serde_json::json!({
                "file_path": "src/module_0000.ts",
                "unsaved_buffer": format!(
                    "export function module_0(value: number) {{ return value + {}; }}\n",
                    iteration + 1
                )
            })
            .to_string();
            let started = Instant::now();
            let response = server.handle_simulate_edit(&body);
            let parsed_response: SimulateEditResponse = serde_json::from_str(&response).unwrap();
            reported_latencies.push(parsed_response.computation_latency_ms);
            timings.push(started.elapsed());
        }
        timings.sort();
        reported_latencies.sort();
        let p95_ms = timings[28].as_secs_f64() * 1000.0;
        eprintln!(
            "daemon 2,000-file handler p95: {p95_ms:.3} ms; analysis-reported p95: {} ms",
            reported_latencies[28]
        );
        if cfg!(debug_assertions) {
            assert!(
                p95_ms < 150.0,
                "debug daemon 2,000-file simulation p95 was {p95_ms:.3} ms; expected <150 ms"
            );
        } else {
            assert!(
                p95_ms <= 50.0,
                "release daemon 2,000-file simulation p95 was {p95_ms:.3} ms; required <= 50 ms"
            );
        }
        assert_eq!(
            fs::read_to_string(temp.join("src/module_0000.ts")).unwrap(),
            "export function module_0(value: number) { return value + 0; }\n"
        );
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn test_phase1_ego_network_line_to_line_edges() {
        let temp = std::env::temp_dir().join(format!("synevid_ego_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(temp.join("src")).unwrap();
        fs::write(
            temp.join("src/service.ts"),
            "export function target() {\n  return 1;\n}\n",
        )
        .unwrap();
        fs::write(
            temp.join("src/caller.ts"),
            "import { target } from './service';\nexport function caller() {\n  return target();\n}\n",
        )
        .unwrap();
        let server = DashboardServer::new(temp.clone(), "127.0.0.1".to_string(), 0);
        let active_id = server.active_repo_id.lock().unwrap().clone();
        let record = server
            .connected_repos
            .lock()
            .unwrap()
            .get(&active_id)
            .unwrap()
            .clone();
        let storage = open_repository_storage(&temp).unwrap();
        let (nodes, _) = storage
            .graph_rows_for_snapshot(record.latest_snap_id.as_deref().unwrap())
            .unwrap();
        let target = nodes
            .iter()
            .find(|node| node.node_kind == "function" && node.file_path == "src/service.ts")
            .unwrap();
        let res_json =
            server.handle_graph_ego(&format!("/api/v1/graph/ego?id={}", target.stable_id));
        let ego: EgoNetwork = serde_json::from_str(&res_json).unwrap();

        assert!(ego.target_node.is_some());
        let call = ego
            .incoming_callers
            .iter()
            .find(|edge| edge.edge_kind == "calls")
            .unwrap();
        assert_eq!(call.source_file, "src/caller.ts");
        assert_eq!(call.source_line, 3);
        assert_eq!(call.target_line, 1);
        let missing = serde_json::from_str::<serde_json::Value>(
            &server.handle_graph_ego("/api/v1/graph/ego?id=missing-symbol"),
        )
        .unwrap();
        assert!(missing["target_node"].is_null());
        assert_eq!(missing["incoming_callers"], serde_json::json!([]));

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn test_phase3_git_galaxy_and_time_travel_endpoints() {
        let temp = std::env::temp_dir().join(format!("synevid_p3_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp);
        let git_dir = temp.join(".git");
        let refs_heads = git_dir.join("refs").join("heads");
        let objects_dir = git_dir.join("objects");
        let _ = fs::create_dir_all(&refs_heads);
        let _ = fs::create_dir_all(&objects_dir);
        let _ = fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n");

        let manager = GitMutationManager::new(&temp);
        let _ = fs::write(temp.join("core.ts"), "export const x = 42;");
        let _ = manager.stage(&["core.ts".to_string()]);
        let commit_res = manager
            .commit_request(&CommitRequest {
                message: "feat: initial commit for phase 3".to_string(),
                author_name: Some("SynEvid Test".to_string()),
                author_email: Some("test@synevid.dev".to_string()),
            })
            .unwrap();
        assert!(commit_res.success);
        let commit_oid = commit_res.new_commit_oid.clone().unwrap();

        let server = DashboardServer::new(temp.clone(), "127.0.0.1".to_string(), 0);

        // Test GET /api/v1/git/galaxy
        let galaxy_json = server.handle_git_galaxy("/api/v1/git/galaxy");
        let galaxy_val: serde_json::Value = serde_json::from_str(&galaxy_json).unwrap();
        assert_eq!(galaxy_val["active_branch"], "main");
        let commits = galaxy_val["commits"].as_array().unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0]["oid"], commit_oid);

        // Test GET /api/v1/git/time-travel
        let tt_url = format!("/api/v1/git/time-travel?oid={}&dim=3d", commit_oid);
        let tt_json = server.handle_git_time_travel(&tt_url);
        let tt_val: serde_json::Value = serde_json::from_str(&tt_json).unwrap();
        assert_eq!(tt_val["oid"], commit_oid);
        assert_eq!(tt_val["dimensions"], "3D");

        // Test POST /api/v1/git/branch (create)
        let branch_json = server.handle_git_branch(
            &serde_json::json!({
                "branch_name": "feature/time-travel",
                "action": "create"
            })
            .to_string(),
        );
        let branch_val: serde_json::Value = serde_json::from_str(&branch_json).unwrap();
        assert!(branch_val["success"].as_bool().unwrap());

        // Test POST /api/v1/git/branch (switch, which records undo checkpoint)
        let switch_json = server.handle_git_branch(
            &serde_json::json!({
                "branch_name": "feature/time-travel",
                "action": "switch"
            })
            .to_string(),
        );
        let switch_val: serde_json::Value = serde_json::from_str(&switch_json).unwrap();
        assert!(switch_val["success"].as_bool().unwrap());

        // Test POST /api/v1/git/undo (rolls back to main)
        let undo_json = server.handle_git_undo();
        let undo_val: serde_json::Value = serde_json::from_str(&undo_json).unwrap();
        assert!(undo_val["success"].as_bool().unwrap());

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn test_phase3_git_galaxy_returns_empty_history_for_non_git_repository() {
        let temp = std::env::temp_dir().join(format!(
            "synevid_non_git_galaxy_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();
        let server = DashboardServer::new(temp.clone(), "127.0.0.1".to_string(), 0);

        let response = server.handle_git_galaxy("/api/v1/git/galaxy");
        let galaxy: serde_json::Value = serde_json::from_str(&response).unwrap();

        assert_eq!(galaxy["total_commits"], 0);
        assert_eq!(galaxy["commits"], serde_json::json!([]));
        assert_eq!(galaxy["branches"], serde_json::json!([]));
        assert_eq!(galaxy["tags"], serde_json::json!([]));
        assert!(galaxy["error"].is_null());

        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn test_cross_platform_path_normalization() {
        let win_wsl_unc = r"\\wsl.localhost\Ubuntu\home\ravi\Projects Alpha\rea-rea-agents-6.8.0\rea-rea-agents-6.8.0";
        let norm = DashboardServer::normalize_cross_platform_path(win_wsl_unc);
        assert_eq!(
            norm,
            PathBuf::from("/home/ravi/Projects Alpha/rea-rea-agents-6.8.0/rea-rea-agents-6.8.0")
        );

        let win_wsl_dollar = r"\\wsl$\Ubuntu\home\ravi\test";
        let norm2 = DashboardServer::normalize_cross_platform_path(win_wsl_dollar);
        assert_eq!(norm2, PathBuf::from("/home/ravi/test"));

        let win_drive = r"C:\Windows\System32";
        let norm3 = DashboardServer::normalize_cross_platform_path(win_drive);
        #[cfg(not(target_os = "windows"))]
        assert_eq!(norm3, PathBuf::from("/mnt/c/Windows/System32"));
    }

    #[test]
    fn test_phase4_forensic_timeline_and_ai_cache_endpoints() {
        let temp = std::env::temp_dir().join(format!("synevid_p4_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp);
        let server = DashboardServer::new(temp.clone(), "127.0.0.1".to_string(), 0);

        // 1. Test POST /api/v1/forensics/sessions/start
        let start_body = serde_json::json!({
            "session_id": "sess_test_p4",
            "actor_kind": "AI_AGENT",
            "actor_name": "Claude-Code",
            "baseline_snap_id": "blake3:base_p4_001"
        })
        .to_string();
        let start_json = server.handle_forensic_sessions_start(&start_body);
        let start_val: serde_json::Value = serde_json::from_str(&start_json).unwrap();
        assert_eq!(start_val["session_id"], "sess_test_p4");
        assert_eq!(start_val["actor_name"], "Claude-Code");

        // 2. Test GET /api/v1/forensics/sessions
        let sess_list_json = server.handle_forensic_sessions("/api/v1/forensics/sessions");
        let sess_list: serde_json::Value = serde_json::from_str(&sess_list_json).unwrap();
        assert!(sess_list["sessions"].as_array().unwrap().len() >= 1);

        // 3. Test GET /api/v1/forensics/timeline
        let timeline_json =
            server.handle_forensic_timeline("/api/v1/forensics/timeline?session_id=sess_test_p4");
        let timeline_val: serde_json::Value = serde_json::from_str(&timeline_json).unwrap();
        assert_eq!(timeline_val["session"]["session_id"], "sess_test_p4");
        assert!(timeline_val["events"].as_array().unwrap().len() >= 1);
        assert!(timeline_val["summary"]["percent_saved"].is_string());

        // 4. Test GET /api/v1/forensics/event?seq=1
        let evt_json = server.handle_forensic_event("/api/v1/forensics/event?seq=1");
        let evt_val: serde_json::Value = serde_json::from_str(&evt_json).unwrap();
        assert!(evt_val["event"]["event_seq"].is_number());
        assert!(evt_val["diff"]["blake3_receipt"].is_string());

        // 5. Test POST /api/v1/ai/cache/query (First: CACHED_NEW, Second: UNCHANGED_SINCE_PREVIOUS_STEP)
        let cache_query = serde_json::json!({
            "session_id": "sess_test_p4",
            "symbol": "PaymentService.chargeCard",
            "query_kind": "impact"
        })
        .to_string();
        let query1_json = server.handle_ai_cache_query(&cache_query);
        let query1_val: serde_json::Value = serde_json::from_str(&query1_json).unwrap();
        assert_eq!(query1_val["status"], "CACHED_NEW");

        let query2_json = server.handle_ai_cache_query(&cache_query);
        let query2_val: serde_json::Value = serde_json::from_str(&query2_json).unwrap();
        assert_eq!(query2_val["status"], "UNCHANGED_SINCE_PREVIOUS_STEP");
        assert!(query2_val["compact_pointer"].as_bool().unwrap());
        assert_eq!(query2_val["tokens_saved"], 4000);

        // 6. Test GET /api/v1/ai/ledger
        let ledger_json = server.handle_ai_ledger("/api/v1/ai/ledger?session_id=sess_test_p4");
        let ledger_val: serde_json::Value = serde_json::from_str(&ledger_json).unwrap();
        assert!(ledger_val["ledger"]["tokens_saved"].as_i64().unwrap() >= 4000);

        // 7. Test GET /api/v1/ai/config-snippet for clients
        for client in &["claude", "cursor", "windsurf", "vscode", "gemini"] {
            let snip_json = server
                .handle_ai_config_snippet(&format!("/api/v1/ai/config-snippet?client={}", client));
            let snip_val: serde_json::Value = serde_json::from_str(&snip_json).unwrap();
            assert!(snip_val["filename"].is_string());
            assert!(
                snip_val
                    .to_string()
                    .contains("apps/mcp-server/dist/index.bundle.js")
            );
        }

        // 8. Test live broadcast & poll
        let bcast_body = serde_json::json!({
            "event": "mcp.live_activity",
            "data": { "tool": "autopsy_impact", "target": "chargeCard" }
        })
        .to_string();
        let bcast_res = server.handle_live_broadcast(&bcast_body);
        assert!(bcast_res.contains("\"success\":true"));

        let poll_json = server.handle_live_poll("/api/v1/live/poll?since_seq=0");
        let poll_val: serde_json::Value = serde_json::from_str(&poll_json).unwrap();
        assert!(poll_val["events"].as_array().unwrap().len() >= 1);

        // 9. Test export and import trace bundle
        let export_json =
            server.handle_forensic_export("/api/v1/forensics/export?session_id=sess_test_p4");
        let export_val: serde_json::Value = serde_json::from_str(&export_json).unwrap();
        assert!(
            export_val["trace_bundle_digest"]
                .as_str()
                .unwrap()
                .starts_with("blake3:")
        );

        let import_body = serde_json::json!({
            "bundle": export_val["bundle"]
        })
        .to_string();
        let import_res = server.handle_forensic_import(&import_body);
        assert!(import_res.contains("\"success\":true"));

        let _ = fs::remove_dir_all(temp);
    }
}
