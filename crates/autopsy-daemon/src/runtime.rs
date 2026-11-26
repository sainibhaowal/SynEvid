//! Runtime lease management

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use blake3;
use parking_lot::RwLock;
use tracing::{debug, info, warn};

use crate::auth::LoopbackAuth;
use crate::repository::RepositoryManager;

/// Unique client identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClientId(pub String);

/// Client type classification
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClientType {
    Cli,
    Tui,
    Mcp,
    Ui,
    Electron,
    Unknown(String),
}

impl From<String> for ClientType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "cli" => ClientType::Cli,
            "tui" => ClientType::Tui,
            "mcp" => ClientType::Mcp,
            "ui" => ClientType::Ui,
            "electron" => ClientType::Electron,
            other => ClientType::Unknown(other.to_string()),
        }
    }
}

/// Lease identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RuntimeLeaseId(pub String);

impl std::fmt::Display for RuntimeLeaseId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Runtime lease record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeLease {
    pub lease_id: RuntimeLeaseId,
    pub client_id: ClientId,
    pub client_type: ClientType,
    pub repository_id: Option<String>,
    pub session_id: Option<String>,
    pub workflow_id: Option<String>,
    pub created_at: u64,
    pub last_heartbeat: u64,
    pub last_event_seq: i64,
}

/// Runtime connection request
#[derive(Debug, Deserialize)]
pub struct RuntimeConnectRequest {
    pub client_id: Option<String>,
    pub client_type: Option<String>,
    pub repository_id: Option<String>,
    pub session_id: Option<String>,
    pub workflow_id: Option<String>,
    pub last_event_seq: Option<i64>,
}

/// Runtime heartbeat request
#[derive(Debug, Deserialize)]
pub struct RuntimeHeartbeatRequest {
    pub lease_id: Option<String>,
    pub last_event_seq: Option<i64>,
    pub session_id: Option<String>,
    pub workflow_id: Option<String>,
}

/// Runtime disconnect request
#[derive(Debug, Deserialize)]
pub struct RuntimeDisconnectRequest {
    pub lease_id: Option<String>,
    pub client_id: Option<String>,
}

/// Runtime manager - manages client leases and active repository
pub struct RuntimeManager {
    leases: Arc<RwLock<std::collections::BTreeMap<RuntimeLeaseId, RuntimeLease>>>,
    active_repo_id: Arc<RwLock<Option<String>>>,
    active_session_id: Arc<RwLock<Option<String>>>,
    loopback_auth: Arc<LoopbackAuth>,
    lease_counter: Arc<Mutex<u64>>,
}

impl RuntimeManager {
    pub fn new(loopback_auth: Arc<LoopbackAuth>) -> Self {
        Self {
            leases: Arc::new(RwLock::new(std::collections::BTreeMap::new())),
            active_repo_id: Arc::new(RwLock::new(None)),
            active_session_id: Arc::new(RwLock::new(None)),
            loopback_auth,
            lease_counter: Arc::new(Mutex::new(0)),
        }
    }

    pub fn get_active_repository(&self) -> Option<String> {
        self.active_repo_id.read().clone()
    }

    pub fn set_active_repository(&self, repo_id: Option<String>) {
        *self.active_repo_id.write() = repo_id;
    }

    pub fn get_active_session(&self) -> Option<String> {
        self.active_session_id.read().clone()
    }

    pub fn set_active_session(&self, session_id: Option<String>) {
        *self.active_session_id.write() = session_id;
    }

    /// Connect a client and create a lease
    pub async fn connect(
        &self,
        client_id: String,
        client_type: String,
        repository_id: Option<String>,
        session_id: Option<String>,
        workflow_id: Option<String>,
        last_event_seq: Option<i64>,
    ) -> Result<serde_json::Value, String> {
        let client_id = ClientId(client_id);
        let client_type: ClientType = client_type.into();

        // Generate lease ID
        let lease_id = RuntimeLeaseId(self.generate_lease_id(&client_id));

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let lease = RuntimeLease {
            lease_id: lease_id.clone(),
            client_id: client_id.clone(),
            client_type,
            repository_id: repository_id.clone(),
            session_id: session_id.clone(),
            workflow_id: workflow_id.clone(),
            created_at: now,
            last_heartbeat: now,
            last_event_seq: last_event_seq.unwrap_or(0),
        };

        // Update active repository if provided
        if let Some(repo_id) = &repository_id {
            *self.active_repo_id.write() = Some(repo_id.clone());
        }

        // Update active session if provided
        if let Some(session_id) = &session_id {
            *self.active_session_id.write() = Some(session_id.clone());
        }

        let client_type_for_log = lease.client_type.clone();

        // Store lease
        self.leases.write().insert(lease_id.clone(), lease);

        info!("Runtime connected: client_id={}, client_type={:?}, lease_id={}", client_id.0, client_type_for_log, lease_id.0);

        Ok(serde_json::json!({
            "lease_id": lease_id.0,
            "repository_id": repository_id,
            "session_id": session_id,
            "workflow_id": workflow_id,
        }))
    }

    /// Heartbeat to keep lease alive
    pub async fn heartbeat(
        &self,
        lease_id: &str,
        last_event_seq: Option<i64>,
        session_id: Option<String>,
        workflow_id: Option<String>,
    ) -> Result<serde_json::Value, String> {
        let lease_id = RuntimeLeaseId(lease_id.to_string());

        let mut leases = self.leases.write();
        let lease = leases.get_mut(&RuntimeLeaseId(lease_id.to_string())).ok_or("Lease not found")?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        lease.last_heartbeat = now;
        if let Some(seq) = last_event_seq {
            lease.last_event_seq = seq;
        }
        if let Some(session_id) = session_id {
            lease.session_id = Some(session_id);
        }
        if let Some(workflow_id) = workflow_id {
            lease.workflow_id = Some(workflow_id);
        }

        Ok(serde_json::json!({
            "status": "ok",
            "lease_id": lease_id.0,
        }))
    }

    /// Disconnect and release lease
    pub async fn disconnect(
        &self,
        lease_id: Option<String>,
        client_id: Option<String>,
    ) -> Result<serde_json::Value, String> {
        let lease_id = lease_id.or_else(|| {
            // Try to find lease by client_id
            self.leases.read().iter()
                .find(|(_, l)| l.client_id.0 == client_id.as_deref().unwrap_or(""))
                .map(|(id, _)| id.0.clone())
        });

        if let Some(lease_id) = lease_id {
            self.leases.write().remove(&RuntimeLeaseId(lease_id.clone()));
            info!("Runtime disconnected: lease_id={}", lease_id);
        }

        Ok(serde_json::json!({ "status": "disconnected" }))
    }

    /// Get runtime status for a client
    pub fn get_status(&self, client_id: &str) -> serde_json::Value {
        let leases = self.leases.read();
        let client_leases: Vec<_> = leases.values()
            .filter(|l| l.client_id.0 == client_id)
            .cloned()
            .collect();

        let active_repo = self.active_repo_id.read().clone();
        let active_session = self.active_session_id.read().clone();

        serde_json::json!({
            "protocol_version": "v1",
            "client_id": client_id,
            "repository_id": active_repo,
            "session_id": active_session,
            "workflow_id": Value::Null,
            "active_leases": client_leases,
        })
    }

    /// List all connected clients
    pub fn list_clients(&self) -> serde_json::Value {
        let leases = self.leases.read();
        let clients: Vec<_> = leases.values()
            .map(|l| serde_json::json!({
                "client_id": l.client_id.0,
                "client_type": format!("{:?}", l.client_type),
                "lease_id": l.lease_id.0,
                "repository_id": l.repository_id,
                "session_id": l.session_id,
                "workflow_id": l.workflow_id,
                "last_event_seq": l.last_event_seq,
                "lease_age_ms": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64 - l.last_heartbeat,
            }))
            .collect();

        serde_json::json!({ "connected_clients": clients })
    }

    /// Clean up expired leases (no heartbeat for > 5 minutes)
    pub async fn cleanup_expired(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let mut leases = self.leases.write();
        let expired: Vec<_> = leases.iter()
            .filter(|(_, l)| now.saturating_sub(l.last_heartbeat) > 5 * 60 * 1000)
            .map(|(id, _)| id.clone())
            .collect();

        for id in expired {
            leases.remove(&id);
            info!("Expired lease removed: {}", id.0);
        }
    }

    /// Restore state from persistence
    pub async fn restore_state(
        &self,
        repositories: &Arc<RepositoryManager>,
        events: &Arc<crate::events::EventStore>,
    ) -> Result<(), String> {
        // Restore active repository from storage
        if let Some(repo_id) = repositories.get_stored_active_repo().await.map_err(|e| e.to_string())? {
            *self.active_repo_id.write() = Some(repo_id);
        }

        // Restore active session from events
        if let Some(session_id) = events.get_latest_session().await.map_err(|e| e.to_string())? {
            *self.active_session_id.write() = Some(session_id);
        }

        Ok(())
    }

    fn generate_lease_id(&self, client_id: &ClientId) -> String {
        let mut counter = self.lease_counter.lock().unwrap();
        *counter += 1;
        let input = format!("{}-{}-{}", client_id.0, *counter, std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
        blake3::hash(input.as_bytes()).to_hex().to_string()[..16].to_string()
    }
}