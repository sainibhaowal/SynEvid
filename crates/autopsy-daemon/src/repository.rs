//! Repository management

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
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
    active_session_id: Arc<RwLock<Option<String>>>,
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
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
            active_session_id: Arc::new(RwLock::new(None)),
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
            canonical
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("repository")
                .to_string()
        });

        // Check if already registered
        {
            let repos = self.repositories.read();
            if let Some(existing) = repos.get(&repo_id) {
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

        let now = current_timestamp();
        let repo_record = RepositoryRecord {
            repo_id: repo_id.clone(),
            root_path: canonical_str,
            display_name: display_name.clone(),
            active_branch: None,
            head_commit_oid: None,
            latest_snap_id: None,
            languages_json: "[]".to_string(),
            monorepo_json: "{}".to_string(),
            last_indexed_seq: 0,
            registered_at: now,
            last_accessed: now,
        };

        // Store in memory
        self.repositories.write().insert(repo_id.clone(), repo_record.clone());

        // Persist to central storage
        if let Some(storage) = self.storage.lock().unwrap().as_ref() {
            let repo_record_for_storage = autopsy_storage::RepoRecord {
                repo_id: repo_record.repo_id.clone(),
                root_path: repo_record.root_path.clone(),
                display_name: repo_record.display_name.clone(),
                active_branch: repo_record.active_branch.clone(),
                head_commit_oid: repo_record.head_commit_oid.clone(),
                latest_snap_id: repo_record.latest_snap_id.clone(),
                languages_json: repo_record.languages_json.clone(),
                monorepo_json: repo_record.monorepo_json.clone(),
                last_indexed_seq: repo_record.last_indexed_seq,
            };
            storage.register_repository(&repo_record_for_storage).map_err(|e| format!("Failed to persist repository: {}", e))?;
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

    /// Get active session ID
    pub fn get_active_session(&self) -> Option<String> {
        self.active_session_id.read().clone()
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
                        registered_at: 0,
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

        // Update in memory with basic info
        let mut repos = self.repositories.write();
        if let Some(r) = repos.get_mut(repo_id) {
            r.last_accessed = current_timestamp();
        }

        // Persist
        if let Some(storage) = self.storage.lock().unwrap().as_ref() {
            let repo_record_for_storage = autopsy_storage::RepoRecord {
                repo_id: record.repo_id.clone(),
                root_path: record.root_path.clone(),
                display_name: record.display_name.clone(),
                active_branch: record.active_branch.clone(),
                head_commit_oid: record.head_commit_oid.clone(),
                latest_snap_id: record.latest_snap_id.clone(),
                languages_json: record.languages_json.clone(),
                monorepo_json: record.monorepo_json.clone(),
                last_indexed_seq: record.last_indexed_seq,
            };
            storage.register_repository(&repo_record_for_storage)
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
}