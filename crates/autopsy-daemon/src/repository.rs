//! Repository management

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use blake3;
use parking_lot::RwLock;
use tracing::{debug, info, warn, error};

use crate::auth::LoopbackAuth;
use autopsy_storage::{StorageEngine, StorageOptions, RepoRecord};
use autopsy_cli::{builtin_plugin_registry, load_or_default_config, open_storage_engine_with_registry};
use autopsy_domain::{CoverageState, SnapshotId};

/// Repository identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RepositoryId(pub String);

/// Repository record (extends storage RepoRecord)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryRecord {
    pub repo_id: String,
    pub root_path: String,
    pub display_name: String,
    pub active_branch: Option<String>,
    pub head_commit_oid: Option<String>,
    pub latest_snap_id: Option<String>,
    pub languages_json: String,
    pub monorepo_json: String,
    pub last_indexed_seq: i64,
    pub registered_at: u64,
    pub last_accessed: u64,
}

/// Repository manager
pub struct RepositoryManager {
    repositories: Arc<RwLock<BTreeMap<String, RepositoryRecord>>>,
    storage: Arc<Mutex<Option<StorageEngine>>>,
    plugin_registry: Arc<autopsy_plugin_api::PluginRegistry>,
    data_dir: PathBuf,
    loopback_auth: Arc<LoopbackAuth>,
    active_repo_id: Arc<RwLock<Option<String>>>,
}

impl RepositoryManager {
    pub async fn new(data_dir: &PathBuf, loopback_auth: Arc<LoopbackAuth>) -> Result<Self, Box<dyn std::error::Error>> {
        let storage_path = data_dir.join("storage.db");
        let cache_dir = data_dir.join("cache");
        
        std::fs::create_dir_all(&cache_dir)?;

        let plugin_registry = Arc::new(autopsy_plugin_api::PluginRegistry::new());
        autopsy_cli::register_builtin_language_plugins(&plugin_registry, &data_dir);
        autopsy_cli::register_builtin_storage_plugins(&plugin_registry);

        let storage = open_storage_engine_with_registry(
            &plugin_registry,
            &autopsy_plugin_api::StorageEngineConfig {
                database_path: storage_path.clone(),
                cache_dir: Some(cache_dir),
                use_wal: true,
                recover_corruption: true,
            },
        ).ok();

        let manager = Self {
            repositories: Arc::new(RwLock::new(BTreeMap::new())),
            storage: Arc::new(Mutex::new(storage)),
            plugin_registry,
            data_dir: data_dir.clone(),
            loopback_auth,
            active_repo_id: Arc::new(RwLock::new(None)),
        };

        // Load persisted repositories
        manager.load_persisted_repositories().await?;

        Ok(manager)
    }

    /// Register a new repository
    pub async fn register_repository(&self, root_path: &Path, display_name: Option<String>) -> Result<RepositoryRecord, String> {
        let canonical = root_path.canonicalize()
            .map_err(|e| format!("Cannot canonicalize path: {}", e))?;
        
        let canonical_str = canonical.to_string_lossy().replace('\\', "/");
        let canonical_str = canonical_str.strip_prefix("//?/").unwrap_or(&canonical_str).to_string();
        
        let repo_id = blake3::hash(canonical_str.as_bytes()).to_hex().to_string();
        let display_name = display_name.unwrap_or_else(|| {
            canonical.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("repository")
                .to_string()
        });

        // Check if already registered
        {
            let repos = self.repositories.read();
            if let Some(existing) = repos.get(&repo_id) {
                // Update last accessed
                let mut repos_mut = self.repositories.write();
                if let Some(r) = repos_mut.get_mut(&repo_id) {
                    r.last_accessed = current_timestamp();
                }
                return Ok(existing.clone());
            }
        }

        // Open repository storage
        let repo_storage = open_storage_engine_with_registry(
            &self.plugin_registry,
            &autopsy_plugin_api::StorageEngineConfig {
                database_path: canonical.join(".autopsy").join("storage.db"),
                cache_dir: Some(canonical.join(".autopsy").join("cache")),
                use_wal: true,
                recover_corruption: true,
            },
        ).map_err(|e| format!("Failed to open repository storage: {}", e))?;

        // Index repository
        let config = load_or_default_config(&canonical).unwrap_or_default();
        let (record, _receipt, _artifacts) = autopsy_cli::index_repository_with_registry(
            &canonical,
            &repo_id,
            &display_name,
            &repo_storage,
            &self.plugin_registry,
        ).map_err(|e| format!("Failed to index repository: {}", e))?;

        let now = current_timestamp();
        let repo_record = RepositoryRecord {
            repo_id: record.repo_id.clone(),
            root_path: canonical_str,
            display_name: record.display_name.clone(),
            active_branch: record.active_branch,
            head_commit_oid: record.head_commit_oid,
            latest_snap_id: record.latest_snap_id,
            languages_json: record.languages_json,
            monorepo_json: record.monorepo_json,
            last_indexed_seq: record.last_indexed_seq,
            registered_at: now,
            last_accessed: now,
        };

        // Store in memory
        self.repositories.write().insert(repo_id.clone(), repo_record.clone());

        // Persist to central storage
        if let Some(storage) = self.storage.lock().unwrap().as_ref() {
            storage.register_repository(&record).map_err(|e| format!("Failed to persist repository: {}", e))?;
            storage.set_daemon_state("active_repo_id", &repo_id).map_err(|e| format!("Failed to set active repo: {}", e))?;
        }

        // Set as active
        *self.active_repo_id.write() = Some(repo_id.clone());

        info!("Registered repository: {} ({})", display_name, repo_id);
        Ok(repo_record)
    }

    /// Register initial repository from config (for managed runtime)
    pub async fn register_initial_repository(&self, root_path: &Path) -> Result<(), String> {
        let _ = self.register_repository(root_path, None).await?;
        Ok(())
    }

    /// Switch active repository
    pub async fn switch_repository(&self, repo_id: &str) -> Result<RepositoryRecord, String> {
        let repos = self.repositories.read();
        let record = repos.get(repo_id)
            .ok_or_else(|| format!("Repository not found: {}", repo_id))?
            .clone();
        drop(repos);

        // Update active
        *self.active_repo_id.write() = Some(repo_id.to_string());

        // Persist
        if let Some(storage) = self.storage.lock().unwrap().as_ref() {
            storage.set_daemon_state("active_repo_id", repo_id)
                .map_err(|e| format!("Failed to set active repo: {}", e))?;
        }

        // Update last accessed
        let mut repos = self.repositories.write();
        if let Some(r) = repos.get_mut(repo_id) {
            r.last_accessed = current_timestamp();
        }

        info!("Switched active repository to: {}", repo_id);
        Ok(record)
    }

    /// Remove repository
    pub async fn remove_repository(&self, repo_id: &str) -> Result<(), String> {
        let mut repos = self.repositories.write();
        repos.remove(repo_id);
        
        // Clear active if it was this one
        let mut active = self.active_repo_id.write();
        if active.as_ref() == Some(&repo_id.to_string()) {
            *active = None;
        }

        // Persist removal
        if let Some(storage) = self.storage.lock().unwrap().as_ref() {
            if active.as_ref() == Some(&repo_id.to_string()) {
                storage.set_daemon_state("active_repo_id", "").map_err(|e| format!("Failed to clear active repo: {}", e))?;
            }
        }

        info!("Removed repository: {}", repo_id);
        Ok(())
    }

    /// List all repositories
    pub fn list_repositories(&self) -> Vec<RepositoryRecord> {
        self.repositories.read().values().cloned().collect()
    }

    /// Get repository by ID
    pub fn get_repository(&self, repo_id: &str) -> Option<RepositoryRecord> {
        self.repositories.read().get(repo_id).cloned()
    }

    /// Get active repository
    pub fn get_active_repository(&self) -> Option<RepositoryRecord> {
        let active_id = self.active_repo_id.read().clone()?;
        self.repositories.read().get(&active_id).cloned()
    }

    /// Get active repository ID
    pub fn get_active_repo_id(&self) -> Option<String> {
        self.active_repo_id.read().clone()
    }

    /// Get stored active repository ID from persistence
    pub async fn get_stored_active_repo(&self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        if let Some(storage) = self.storage.lock().unwrap().as_ref() {
            if let Ok(Some(id)) = storage.get_daemon_state("active_repo_id") {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    /// Load persisted repositories from storage
    async fn load_persisted_repositories(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(storage) = self.storage.lock().unwrap().as_ref() {
            if let Ok(repos) = storage.list_repositories() {
                let mut repos_map = self.repositories.write();
                for record in repos {
                    let repo_record = RepositoryRecord {
                        repo_id: record.repo_id.clone(),
                        root_path: record.root_path,
                        display_name: record.display_name,
                        active_branch: record.active_branch,
                        head_commit_oid: record.head_commit_oid,
                        latest_snap_id: record.latest_snap_id,
                        languages_json: record.languages_json,
                        monorepo_json: record.monorepo_json,
                        last_indexed_seq: record.last_indexed_seq,
                        registered_at: 0, // Unknown from storage
                        last_accessed: 0,
                    };
                    repos_map.insert(record.repo_id.clone(), repo_record);
                }
            }
        }
        Ok(())
    }

    /// Refresh repository (re-index on file changes)
    pub async fn refresh_repository(&self, repo_id: &str) -> Result<(), String> {
        let record = self.get_repository(repo_id)
            .ok_or_else(|| format!("Repository not found: {}", repo_id))?;

        let root_path = PathBuf::from(&record.root_path);
        let repo_storage = open_storage_engine_with_registry(
            &self.plugin_registry,
            &autopsy_plugin_api::StorageEngineConfig {
                database_path: root_path.join(".autopsy").join("storage.db"),
                cache_dir: Some(root_path.join(".autopsy").join("cache")),
                use_wal: true,
                recover_corruption: true,
            },
        ).map_err(|e| format!("Failed to open repository storage: {}", e))?;

        let config = load_or_default_config(&root_path).unwrap_or_default();
        let (updated_record, _receipt, _artifacts) = autopsy_cli::index_repository_with_registry(
            &root_path,
            &record.repo_id,
            &record.display_name,
            &repo_storage,
            &self.plugin_registry,
        ).map_err(|e| format!("Failed to re-index repository: {}", e))?;

        // Update in memory
        let mut repos = self.repositories.write();
        if let Some(r) = repos.get_mut(repo_id) {
            r.active_branch = updated_record.active_branch;
            r.head_commit_oid = updated_record.head_commit_oid;
            r.latest_snap_id = updated_record.latest_snap_id;
            r.languages_json = updated_record.languages_json;
            r.monorepo_json = updated_record.monorepo_json;
            r.last_indexed_seq = updated_record.last_indexed_seq;
            r.last_accessed = current_timestamp();
        }

        // Persist
        if let Some(storage) = self.storage.lock().unwrap().as_ref() {
            storage.register_repository(&updated_record)
                .map_err(|e| format!("Failed to persist updated repository: {}", e))?;
        }

        info!("Refreshed repository: {}", repo_id);
        Ok(())
    }

    /// Get storage engine for a repository
    pub fn get_storage_for_repo(&self, repo_id: &str) -> Option<StorageEngine> {
        let record = self.get_repository(repo_id)?;
        let root_path = PathBuf::from(&record.root_path);
        
        open_storage_engine_with_registry(
            &self.plugin_registry,
            &autopsy_plugin_api::StorageEngineConfig {
                database_path: root_path.join(".autopsy").join("storage.db"),
                cache_dir: Some(root_path.join(".autopsy").join("cache")),
                use_wal: true,
                recover_corruption: true,
            },
        ).ok()
    }

    /// Get storage for active repository
    pub fn get_storage(&self, snapshot_id: &str) -> Result<StorageEngine, String> {
        let active_repo_id = self.get_active_repo_id()
            .ok_or_else(|| "No active repository".to_string())?;
        
        let record = self.get_repository(&active_repo_id)
            .ok_or_else(|| format!("Active repository not found: {}", active_repo_id))?;
        
        let root_path = PathBuf::from(&record.root_path);
        let repo_storage = open_storage_engine_with_registry(
            &self.plugin_registry,
            &autopsy_plugin_api::StorageEngineConfig {
                database_path: root_path.join(".autopsy").join("storage.db"),
                cache_dir: Some(root_path.join(".autopsy").join("cache")),
                use_wal: true,
                recover_corruption: true,
            },
        ).map_err(|e| format!("Failed to open repository storage: {}", e))?;
        
        let snapshot = autopsy_domain::SnapshotId::new(snapshot_id);
        let _snapshot = repo_storage.get_snapshot(&snapshot)
            .map_err(|e| format!("Failed to get snapshot: {}", e))?
            .ok_or_else(|| "Snapshot not found".to_string())?;
        
        Ok(repo_storage)
    }

    /// Get stored active repository ID from persistence
    pub async fn get_stored_active_repo(&self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        if let Some(storage) = self.storage.lock().unwrap().as_ref() {
            if let Ok(Some(id)) = storage.get_daemon_state("active_repo_id") {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    /// Persist catalog entities
    pub async fn persist_catalog(&self, repo_id: &str, catalog: &autopsy_catalog::Catalog) -> Result<(), String> {
        if let Some(storage) = self.storage.lock().unwrap().as_ref() {
            storage.insert_catalog_entities(repo_id, &catalog.entities)
                .map_err(|e| format!("Failed to persist catalog entities: {}", e))?;
            storage.insert_catalog_manifests(repo_id, &catalog.manifests)
                .map_err(|e| format!("Failed to persist catalog manifests: {}", e))?;
            storage.insert_catalog_dependencies(repo_id, &catalog.dependencies)
                .map_err(|e| format!("Failed to persist catalog dependencies: {}", e))?;
        }
        Ok(())
    }

    /// Record forensic event
    pub async fn record_forensic_event(
        &self,
        session_id: &str,
        repo_root: Option<&str>,
        job_id: Option<&str>,
        workflow_id: Option<&str>,
        event_type: &str,
        target_symbol: Option<&str>,
        target_file: Option<&str>,
        before_snap_id: Option<&str>,
        after_snap_id: Option<&str>,
        verdict: Option<&str>,
        summary: &str,
        evidence: serde_json::Value,
        tokens_spent: Option<i64>,
        tokens_saved: Option<i64>,
    ) -> Result<i64, String> {
        let event_seq = self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .next_forensic_event_seq()
            .map_err(|e| format!("Failed to get next event seq: {}", e))?;

        let record = autopsy_storage::TrajectoryEventRecord {
            event_seq,
            session_id: session_id.to_string(),
            job_id: job_id.map(|s| s.to_string()),
            workflow_id: workflow_id.map(|s| s.to_string()),
            event_type: event_type.to_string(),
            target_symbol: target_symbol.map(|s| s.to_string()),
            target_file: target_file.map(|s| s.to_string()),
            before_snap_id: before_snap_id.map(|s| s.to_string()),
            after_snap_id: after_snap_id.map(|s| s.to_string()),
            verdict: verdict.unwrap_or("INFO").to_string(),
            summary: summary.to_string(),
            evidence_json: serde_json::to_string(&evidence).unwrap_or_default(),
            blake3_receipt: format!("blake3:{}", blake3::hash(serde_json::to_string(&evidence).unwrap_or_default().as_bytes()).to_hex()),
            tokens_spent: tokens_spent.unwrap_or(0),
            tokens_saved: tokens_saved.unwrap_or(0),
            timestamp: current_timestamp(),
        };

        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_trajectory_event(&record)
            .map_err(|e| format!("Failed to insert trajectory event: {}", e))?;

        Ok(event_seq)
    }

    /// Update session activity
    pub async fn update_session_activity(&self, session_id: &str, event_seq: i64, verdict: &str) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .update_session_activity(session_id, event_seq, verdict)
            .map_err(|e| format!("Failed to update session: {}", e))
    }

    /// Get forensic session
    pub async fn get_forensic_session(&self, session_id: &str) -> Result<Option<autopsy_storage::ForensicSessionRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_forensic_session(session_id)
            .map_err(|e| format!("Failed to get session: {}", e))
    }

    /// List forensic sessions
    pub async fn list_forensic_sessions(&self, repo_id: Option<&str>) -> Result<Vec<autopsy_storage::ForensicSessionRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .list_forensic_sessions(repo_id)
            .map_err(|e| format!("Failed to list sessions: {}", e))
    }

    /// Get forensic timeline
    pub async fn get_forensic_timeline(&self, session_id: &str) -> Result<crate::handlers::forensic::ForensicTimelineResult, String> {
        let session = self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_forensic_session(session_id)
            .map_err(|e| format!("Failed to get session: {}", e))?
            .ok_or("Session not found")?;

        let events = self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_trajectory_events(session_id)
            .map_err(|e| format!("Failed to get events: {}", e))?;

        let workflows = self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .list_forensic_workflows(session_id)
            .map_err(|e| format!("Failed to get workflows: {}", e))?;

        let ledger = self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_ai_session_ledger(session_id)
            .map_err(|e| format!("Failed to get ledger: {}", e))?;

        Ok(crate::handlers::forensic::ForensicTimelineResult {
            session: session.into(),
            events: events.into_iter().map(Into::into).collect(),
            workflows: workflows.into_iter().map(Into::into).collect(),
            ledger: ledger.map(Into::into),
        })
    }

    /// Record trajectory event
    pub async fn record_trajectory_event(
        &self,
        session_id: &str,
        repo_root: Option<&str>,
        job_id: Option<&str>,
        workflow_id: Option<&str>,
        event_type: &str,
        target_symbol: Option<&str>,
        target_file: Option<&str>,
        before_snap_id: Option<&str>,
        after_snap_id: Option<&str>,
        verdict: Option<&str>,
        summary: &str,
        evidence: serde_json::Value,
        tokens_spent: Option<i64>,
        tokens_saved: Option<i64>,
    ) -> Result<i64, String> {
        let event_seq = self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .next_forensic_event_seq()
            .map_err(|e| format!("Failed to get next event seq: {}", e))?;

        let record = autopsy_storage::TrajectoryEventRecord {
            event_seq,
            session_id: session_id.to_string(),
            job_id: job_id.map(|s| s.to_string()),
            workflow_id: workflow_id.map(|s| s.to_string()),
            event_type: event_type.to_string(),
            target_symbol: target_symbol.map(|s| s.to_string()),
            target_file: target_file.map(|s| s.to_string()),
            before_snap_id: before_snap_id.map(|s| s.to_string()),
            after_snap_id: after_snap_id.map(|s| s.to_string()),
            verdict: verdict.unwrap_or("INFO").to_string(),
            summary: summary.to_string(),
            evidence_json: serde_json::to_string(&evidence).unwrap_or_default(),
            blake3_receipt: format!("blake3:{}", blake3::hash(serde_json::to_string(&evidence).unwrap_or_default().as_bytes()).to_hex()),
            tokens_spent: tokens_spent.unwrap_or(0),
            tokens_saved: tokens_saved.unwrap_or(0),
            timestamp: current_timestamp(),
        };

        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_trajectory_event(&record)
            .map_err(|e| format!("Failed to insert trajectory event: {}", e))?;

        Ok(event_seq)
    }

    /// Revert forensic step
    pub async fn revert_forensic_step(
        &self,
        session_id: &str,
        event_seq: i64,
    ) -> Result<crate::handlers::forensic::ForensicRevertResult, String> {
        let event = self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_trajectory_event(session_id, event_seq)
            .map_err(|e| format!("Failed to get event: {}", e))?
            .ok_or("Event not found")?;

        let revert_seq = self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .next_forensic_event_seq()
            .map_err(|e| format!("Failed to get next event seq: {}", e))?;

        let revert_record = autopsy_storage::TrajectoryEventRecord {
            event_seq: revert_seq,
            session_id: session_id.to_string(),
            job_id: event.job_id.clone(),
            workflow_id: event.workflow_id.clone(),
            event_type: "revert".to_string(),
            target_symbol: event.target_symbol.clone(),
            target_file: event.target_file.clone(),
            before_snap_id: event.after_snap_id.clone(),
            after_snap_id: event.before_snap_id.clone(),
            verdict: "REVERTED".to_string(),
            summary: format!("Reverted event {}", event_seq),
            evidence_json: serde_json::to_string(&json!({
                "reverted_event_seq": event_seq,
                "original_event_type": event.event_type,
            })).unwrap_or_default(),
            blake3_receipt: format!("blake3:{}", blake3::hash(format!("revert-{}", event_seq).as_bytes()).to_hex()),
            tokens_spent: 0,
            tokens_saved: 0,
            timestamp: current_timestamp(),
        };

        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_trajectory_event(&revert_record)
            .map_err(|e| format!("Failed to insert revert event: {}", e))?;

        Ok(crate::handlers::forensic::ForensicRevertResult {
            reverted_event_seq: event_seq,
            revert_event_seq: revert_seq,
        })
    }

    /// List trajectory runs
    pub async fn list_trajectory_runs(&self) -> Result<Vec<autopsy_storage::ForensicSessionRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .list_forensic_sessions(None)
            .map_err(|e| format!("Failed to list trajectory runs: {}", e))
    }

    /// Get forensic session
    pub async fn get_forensic_session(&self, session_id: &str) -> Result<Option<autopsy_storage::ForensicSessionRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_forensic_session(session_id)
            .map_err(|e| format!("Failed to get session: {}", e))
    }

    /// Get trajectory events
    pub async fn get_trajectory_events(&self, session_id: &str) -> Result<Vec<autopsy_storage::TrajectoryEventRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_trajectory_events(session_id)
            .map_err(|e| format!("Failed to get events: {}", e))
    }

    /// Get forensic workflows
    pub async fn list_forensic_workflows(&self, session_id: &str) -> Result<Vec<autopsy_storage::ForensicWorkflowRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .list_forensic_workflows(session_id)
            .map_err(|e| format!("Failed to get workflows: {}", e))
    }

    /// Get AI session ledger
    pub async fn get_ai_session_ledger(&self, session_id: &str) -> Result<Option<autopsy_storage::AiSessionLedgerRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_ai_session_ledger(session_id)
            .map_err(|e| format!("Failed to get ledger: {}", e))
    }

    /// Get trajectory event
    pub async fn get_trajectory_event(&self, session_id: &str, event_seq: i64) -> Result<Option<autopsy_storage::TrajectoryEventRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_trajectory_event(session_id, event_seq)
            .map_err(|e| format!("Failed to get trajectory event: {}", e))
    }

    /// Get next forensic event sequence
    async fn next_forensic_event_seq(&self) -> Result<i64, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .next_forensic_event_seq()
            .map_err(|e| format!("Failed to get next event seq: {}", e))
    }

    /// Insert trajectory event
    async fn insert_trajectory_event(&self, record: &autopsy_storage::TrajectoryEventRecord) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_trajectory_event(record)
            .map_err(|e| format!("Failed to insert trajectory event: {}", e))
    }

    /// Update session activity
    async fn update_session_activity(&self, session_id: &str, event_seq: i64, verdict: &str) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .update_session_activity(session_id, event_seq, verdict)
            .map_err(|e| format!("Failed to update session: {}", e))
    }

    /// Get forensic session
    async fn get_forensic_session(&self, session_id: &str) -> Result<Option<autopsy_storage::ForensicSessionRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_forensic_session(session_id)
            .map_err(|e| format!("Failed to get session: {}", e))
    }

    /// List forensic sessions
    async fn list_forensic_sessions(&self, repo_id: Option<&str>) -> Result<Vec<autopsy_storage::ForensicSessionRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .list_forensic_sessions(repo_id)
            .map_err(|e| format!("Failed to list sessions: {}", e))
    }

    /// Get trajectory events
    async fn get_trajectory_events(&self, session_id: &str) -> Result<Vec<autopsy_storage::TrajectoryEventRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_trajectory_events(session_id)
            .map_err(|e| format!("Failed to get events: {}", e))
    }

    /// List forensic workflows
    async fn list_forensic_workflows(&self, session_id: &str) -> Result<Vec<autopsy_storage::ForensicWorkflowRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .list_forensic_workflows(session_id)
            .map_err(|e| format!("Failed to get workflows: {}", e))
    }

    /// Get AI session ledger
    async fn get_ai_session_ledger(&self, session_id: &str) -> Result<Option<autopsy_storage::AiSessionLedgerRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_ai_session_ledger(session_id)
            .map_err(|e| format!("Failed to get ledger: {}", e))
    }

    /// Get trajectory event
    async fn get_trajectory_event(&self, session_id: &str, event_seq: i64) -> Result<Option<autopsy_storage::TrajectoryEventRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_trajectory_event(session_id, event_seq)
            .map_err(|e| format!("Failed to get trajectory event: {}", e))
    }

    /// Close forensic session
    pub async fn close_forensic_session(&self, session_id: &str, verdict: &str) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .close_forensic_session(session_id, verdict)
            .map_err(|e| format!("Failed to close session: {}", e))
    }

    /// Persist catalog entities
    pub async fn persist_catalog_entities(&self, repo_id: &str, entities: &[autopsy_catalog::CatalogEntity]) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_catalog_entities(repo_id, entities)
            .map_err(|e| format!("Failed to persist catalog entities: {}", e))
    }

    /// Persist catalog manifests
    pub async fn persist_catalog_manifests(&self, repo_id: &str, manifests: &[autopsy_catalog::CatalogManifest]) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_catalog_manifests(repo_id, manifests)
            .map_err(|e| format!("Failed to persist catalog manifests: {}", e))
    }

    /// Persist catalog dependencies
    pub async fn persist_catalog_dependencies(&self, repo_id: &str, dependencies: &[autopsy_catalog::CatalogDependency]) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_catalog_dependencies(repo_id, dependencies)
            .map_err(|e| format!("Failed to persist catalog dependencies: {}", e))
    }

    /// Get stored active repository ID from persistence
    pub async fn get_stored_active_repo(&self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        if let Some(storage) = self.storage.lock().unwrap().as_ref() {
            if let Ok(Some(id)) = storage.get_daemon_state("active_repo_id") {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    /// Create forensic session
    pub async fn create_forensic_session(&self, record: &autopsy_storage::ForensicSessionRecord) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_forensic_session(record)
            .map_err(|e| format!("Failed to insert session: {}", e))
    }

    /// Get forensic session
    pub async fn get_forensic_session(&self, session_id: &str) -> Result<Option<autopsy_storage::ForensicSessionRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_forensic_session(session_id)
            .map_err(|e| format!("Failed to get session: {}", e))
    }

    /// Update session activity
    pub async fn update_session_activity(&self, session_id: &str, event_seq: i64, verdict: &str) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .update_session_activity(session_id, event_seq, verdict)
            .map_err(|e| format!("Failed to update session: {}", e))
    }

    /// Insert forensic session
    pub async fn insert_forensic_session(&self, record: &autopsy_storage::ForensicSessionRecord) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_forensic_session(record)
            .map_err(|e| format!("Failed to insert session: {}", e))
    }

    /// Get forensic session
    pub async fn get_forensic_session(&self, session_id: &str) -> Result<Option<autopsy_storage::ForensicSessionRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_forensic_session(session_id)
            .map_err(|e| format!("Failed to get session: {}", e))
    }

    /// List forensic sessions
    pub async fn list_forensic_sessions(&self, repo_id: Option<&str>) -> Result<Vec<autopsy_storage::ForensicSessionRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .list_forensic_sessions(repo_id)
            .map_err(|e| format!("Failed to list sessions: {}", e))
    }

    /// Get trajectory events
    pub async fn get_trajectory_events(&self, session_id: &str) -> Result<Vec<autopsy_storage::TrajectoryEventRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_trajectory_events(session_id)
            .map_err(|e| format!("Failed to get events: {}", e))
    }

    /// List forensic workflows
    pub async fn list_forensic_workflows(&self, session_id: &str) -> Result<Vec<autopsy_storage::ForensicWorkflowRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .list_forensic_workflows(session_id)
            .map_err(|e| format!("Failed to list workflows: {}", e))
    }

    /// Get AI session ledger
    pub async fn get_ai_session_ledger(&self, session_id: &str) -> Result<Option<autopsy_storage::AiSessionLedgerRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_ai_session_ledger(session_id)
            .map_err(|e| format!("Failed to get ledger: {}", e))
    }

    /// Get trajectory event
    pub async fn get_trajectory_event(&self, session_id: &str, event_seq: i64) -> Result<Option<autopsy_storage::TrajectoryEventRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_trajectory_event(session_id, event_seq)
            .map_err(|e| format!("Failed to get trajectory event: {}", e))
    }

    /// Insert trajectory event
    pub async fn insert_trajectory_event(&self, record: &autopsy_storage::TrajectoryEventRecord) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_trajectory_event(record)
            .map_err(|e| format!("Failed to insert trajectory event: {}", e))
    }

    /// Update session activity
    pub async fn update_session_activity(&self, session_id: &str, event_seq: i64, verdict: &str) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .update_session_activity(session_id, event_seq, verdict)
            .map_err(|e| format!("Failed to update session: {}", e))
    }

    /// Get forensic session
    pub async fn get_forensic_session(&self, session_id: &str) -> Result<Option<autopsy_storage::ForensicSessionRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_forensic_session(session_id)
            .map_err(|e| format!("Failed to get session: {}", e))
    }

    /// List forensic sessions
    pub async fn list_forensic_sessions(&self, repo_id: Option<&str>) -> Result<Vec<autopsy_storage::ForensicSessionRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .list_forensic_sessions(repo_id)
            .map_err(|e| format!("Failed to list sessions: {}", e))
    }

    /// Get trajectory events
    pub async fn get_trajectory_events(&self, session_id: &str) -> Result<Vec<autopsy_storage::TrajectoryEventRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_trajectory_events(session_id)
            .map_err(|e| format!("Failed to get events: {}", e))
    }

    /// List forensic workflows
    pub async fn list_forensic_workflows(&self, session_id: &str) -> Result<Vec<autopsy_storage::ForensicWorkflowRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .list_forensic_workflows(session_id)
            .map_err(|e| format!("Failed to list workflows: {}", e))
    }

    /// Get AI session ledger
    pub async fn get_ai_session_ledger(&self, session_id: &str) -> Result<Option<autopsy_storage::AiSessionLedgerRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_ai_session_ledger(session_id)
            .map_err(|e| format!("Failed to get ledger: {}", e))
    }

    /// Get trajectory event
    pub async fn get_trajectory_event(&self, session_id: &str, event_seq: i64) -> Result<Option<autopsy_storage::TrajectoryEventRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_trajectory_event(session_id, event_seq)
            .map_err(|e| format!("Failed to get trajectory event: {}", e))
    }

    /// Insert trajectory event
    pub async fn insert_trajectory_event(&self, record: &autopsy_storage::TrajectoryEventRecord) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_trajectory_event(record)
            .map_err(|e| format!("Failed to insert trajectory event: {}", e))
    }

    /// Update session activity
    pub async fn update_session_activity(&self, session_id: &str, event_seq: i64, verdict: &str) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .update_session_activity(session_id, event_seq, verdict)
            .map_err(|e| format!("Failed to update session: {}", e))
    }

    /// Close forensic session
    pub async fn close_forensic_session(&self, session_id: &str, verdict: &str) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .close_forensic_session(session_id, verdict)
            .map_err(|e| format!("Failed to close session: {}", e))
    }

    /// Persist catalog entities
    pub async fn persist_catalog_entities(&self, repo_id: &str, entities: &[autopsy_catalog::CatalogEntity]) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_catalog_entities(repo_id, entities)
            .map_err(|e| format!("Failed to persist catalog entities: {}", e))
    }

    /// Get stored active repo
    pub async fn get_stored_active_repo(&self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        if let Some(storage) = self.storage.lock().unwrap().as_ref() {
            if let Ok(Some(id)) = storage.get_daemon_state("active_repo_id") {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    /// Create forensic session
    pub async fn create_forensic_session(&self, record: &autopsy_storage::ForensicSessionRecord) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_forensic_session(record)
            .map_err(|e| format!("Failed to insert session: {}", e))
    }

    /// Get forensic session
    pub async fn get_forensic_session(&self, session_id: &str) -> Result<Option<autopsy_storage::ForensicSessionRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_forensic_session(session_id)
            .map_err(|e| format!("Failed to get session: {}", e))
    }

    /// Update session activity
    pub async fn update_session_activity(&self, session_id: &str, event_seq: i64, verdict: &str) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .update_session_activity(session_id, event_seq, verdict)
            .map_err(|e| format!("Failed to update session: {}", e))
    }

    /// Insert forensic session
    pub async fn insert_forensic_session(&self, record: &autopsy_storage::ForensicSessionRecord) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_forensic_session(record)
            .map_err(|e| format!("Failed to insert session: {}", e))
    }

    /// Get forensic session
    pub async fn get_forensic_session(&self, session_id: &str) -> Result<Option<autopsy_storage::ForensicSessionRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_forensic_session(session_id)
            .map_err(|e| format!("Failed to get session: {}", e))
    }

    /// List forensic sessions
    pub async fn list_forensic_sessions(&self, repo_id: Option<&str>) -> Result<Vec<autopsy_storage::ForensicSessionRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .list_forensic_sessions(repo_id)
            .map_err(|e| format!("Failed to list sessions: {}", e))
    }

    /// Get trajectory events
    pub async fn get_trajectory_events(&self, session_id: &str) -> Result<Vec<autopsy_storage::TrajectoryEventRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_trajectory_events(session_id)
            .map_err(|e| format!("Failed to get events: {}", e))
    }

    /// List forensic workflows
    pub async fn list_forensic_workflows(&self, session_id: &str) -> Result<Vec<autopsy_storage::ForensicWorkflowRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .list_forensic_workflows(session_id)
            .map_err(|e| format!("Failed to list workflows: {}", e))
    }

    /// Get AI session ledger
    pub async fn get_ai_session_ledger(&self, session_id: &str) -> Result<Option<autopsy_storage::AiSessionLedgerRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_ai_session_ledger(session_id)
            .map_err(|e| format!("Failed to get ledger: {}", e))
    }

    /// Get trajectory event
    pub async fn get_trajectory_event(&self, session_id: &str, event_seq: i64) -> Result<Option<autopsy_storage::TrajectoryEventRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_trajectory_event(session_id, event_seq)
            .map_err(|e| format!("Failed to get trajectory event: {}", e))
    }

    /// Insert trajectory event
    pub async fn insert_trajectory_event(&self, record: &autopsy_storage::TrajectoryEventRecord) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_trajectory_event(record)
            .map_err(|e| format!("Failed to insert trajectory event: {}", e))
    }

    /// Update session activity
    pub async fn update_session_activity(&self, session_id: &str, event_seq: i64, verdict: &str) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .update_session_activity(session_id, event_seq, verdict)
            .map_err(|e| format!("Failed to update session: {}", e))
    }

    /// Close forensic session
    pub async fn close_forensic_session(&self, session_id: &str, verdict: &str) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .close_forensic_session(session_id, verdict)
            .map_err(|e| format!("Failed to close session: {}", e))
    }

    /// Persist catalog entities
    pub async fn persist_catalog_entities(&self, repo_id: &str, entities: &[autopsy_catalog::CatalogEntity]) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_catalog_entities(repo_id, entities)
            .map_err(|e| format!("Failed to persist catalog entities: {}", e))
    }

    /// Get stored active repo
    pub async fn get_stored_active_repo(&self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        if let Some(storage) = self.storage.lock().unwrap().as_ref() {
            if let Ok(Some(id)) = storage.get_daemon_state("active_repo_id") {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    /// Create forensic session
    pub async fn create_forensic_session(&self, record: &autopsy_storage::ForensicSessionRecord) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_forensic_session(record)
            .map_err(|e| format!("Failed to insert session: {}", e))
    }

    /// Get forensic session
    pub async fn get_forensic_session(&self, session_id: &str) -> Result<Option<autopsy_storage::ForensicSessionRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_forensic_session(session_id)
            .map_err(|e| format!("Failed to get session: {}", e))
    }

    /// Update session activity
    pub async fn update_session_activity(&self, session_id: &str, event_seq: i64, verdict: &str) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .update_session_activity(session_id, event_seq, verdict)
            .map_err(|e| format!("Failed to update session: {}", e))
    }

    /// Insert forensic session
    pub async fn insert_forensic_session(&self, record: &autopsy_storage::ForensicSessionRecord) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_forensic_session(record)
            .map_err(|e| format!("Failed to insert session: {}", e))
    }

    /// Get forensic session
    pub async fn get_forensic_session(&self, session_id: &str) -> Result<Option<autopsy_storage::ForensicSessionRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_forensic_session(session_id)
            .map_err(|e| format!("Failed to get session: {}", e))
    }

    /// List forensic sessions
    pub async fn list_forensic_sessions(&self, repo_id: Option<&str>) -> Result<Vec<autopsy_storage::ForensicSessionRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .list_forensic_sessions(repo_id)
            .map_err(|e| format!("Failed to list sessions: {}", e))
    }

    /// Get trajectory events
    pub async fn get_trajectory_events(&self, session_id: &str) -> Result<Vec<autopsy_storage::TrajectoryEventRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_trajectory_events(session_id)
            .map_err(|e| format!("Failed to get events: {}", e))
    }

    /// List forensic workflows
    pub async fn list_forensic_workflows(&self, session_id: &str) -> Result<Vec<autopsy_storage::ForensicWorkflowRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .list_forensic_workflows(session_id)
            .map_err(|e| format!("Failed to list workflows: {}", e))
    }

    /// Get AI session ledger
    pub async fn get_ai_session_ledger(&self, session_id: &str) -> Result<Option<autopsy_storage::AiSessionLedgerRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_ai_session_ledger(session_id)
            .map_err(|e| format!("Failed to get ledger: {}", e))
    }

    /// Get trajectory event
    pub async fn get_trajectory_event(&self, session_id: &str, event_seq: i64) -> Result<Option<autopsy_storage::TrajectoryEventRecord>, String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .get_trajectory_event(session_id, event_seq)
            .map_err(|e| format!("Failed to get trajectory event: {}", e))
    }

    /// Insert trajectory event
    pub async fn insert_trajectory_event(&self, record: &autopsy_storage::TrajectoryEventRecord) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_trajectory_event(record)
            .map_err(|e| format!("Failed to insert trajectory event: {}", e))
    }

    /// Update session activity
    pub async fn update_session_activity(&self, session_id: &str, event_seq: i64, verdict: &str) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .update_session_activity(session_id, event_seq, verdict)
            .map_err(|e| format!("Failed to update session: {}", e))
    }

    /// Close forensic session
    pub async fn close_forensic_session(&self, session_id: &str, verdict: &str) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .close_forensic_session(session_id, verdict)
            .map_err(|e| format!("Failed to close session: {}", e))
    }

    /// Persist catalog entities
    pub async fn persist_catalog_entities(&self, repo_id: &str, entities: &[autopsy_catalog::CatalogEntity]) -> Result<(), String> {
        self.storage.lock().unwrap().as_ref()
            .ok_or("Storage not available")?
            .insert_catalog_entities(repo_id, entities)
            .map_err(|e| format!("Failed to persist catalog entities: {}", e))
    }
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}