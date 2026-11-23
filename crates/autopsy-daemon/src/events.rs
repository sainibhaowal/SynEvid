//! Event store for live event streaming and replay

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use blake3;
use parking_lot::RwLock;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};
use rusqlite;

/// Event sequence ID (monotonic)
pub type EventSeq = i64;

/// Event record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub seq_id: EventSeq,
    pub event_type: String,
    pub timestamp: u64,
    pub source_client_id: String,
    pub source_client_type: String,
    pub source_lease_id: Option<String>,
    pub repository_id: Option<String>,
    pub session_id: Option<String>,
    pub workflow_id: Option<String>,
    pub payload: Value,
    pub evidence_digest: String,
}

/// Event store with in-memory ring buffer and persistent storage
pub struct EventStore {
    // In-memory ring buffer for recent events (fast access)
    memory_buffer: Arc<RwLock<BTreeMap<EventSeq, EventRecord>>>,
    // Persistent storage path
    storage_path: PathBuf,
    // Next sequence ID
    next_seq: Arc<Mutex<EventSeq>>,
    // Broadcast channel for live subscribers
    broadcast_tx: broadcast::Sender<EventRecord>,
    // SQLite storage (optional, for durability)
    sqlite: Option<Arc<rusqlite::Connection>>,
}

impl EventStore {
    pub fn new(data_dir: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let storage_path = data_dir.join("events.sqlite");
        let (broadcast_tx, _) = broadcast::channel(10000);

        // Initialize SQLite for durable event storage
        let sqlite = if storage_path.exists() || storage_path.parent().map(|p| p.exists()).unwrap_or(false) {
            let conn = rusqlite::Connection::open(&storage_path)?;
            conn.execute(
                "CREATE TABLE IF NOT EXISTS events (
                    seq_id INTEGER PRIMARY KEY,
                    event_type TEXT NOT NULL,
                    timestamp INTEGER NOT NULL,
                    source_client_id TEXT NOT NULL,
                    source_client_type TEXT NOT NULL,
                    source_lease_id TEXT,
                    repository_id TEXT,
                    session_id TEXT,
                    workflow_id TEXT,
                    payload TEXT NOT NULL,
                    evidence_digest TEXT NOT NULL
                )",
                [],
            )?;
            conn.execute("CREATE INDEX IF NOT EXISTS idx_events_seq ON events(seq_id)", [])?;
            conn.execute("CREATE INDEX IF NOT EXISTS idx_events_repo ON events(repository_id)", [])?;
            conn.execute("CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id)", [])?;
            Some(Arc::new(conn))
        } else {
            None
        };

        // Load last sequence ID from SQLite
        let next_seq = if let Some(conn) = &sqlite {
            let mut stmt = conn.prepare("SELECT COALESCE(MAX(seq_id), 0) + 1 FROM events")?;
            let seq: i64 = stmt.next()?.and_then(|row| row.read::<i64, _>(0).ok()).unwrap_or(1);
            Arc::new(Mutex::new(seq))
        } else {
            Arc::new(Mutex::new(1))
        };

        Ok(Self {
            memory_buffer: Arc::new(RwLock::new(BTreeMap::new())),
            storage_path,
            next_seq,
            broadcast_tx,
            sqlite,
        })
    }

    /// Subscribe to live events
    pub fn subscribe(&self) -> broadcast::Receiver<EventRecord> {
        self.broadcast_tx.subscribe()
    }

    /// Publish a new event
    pub fn publish(
        &self,
        event_type: &str,
        payload: Value,
        source_client_id: &str,
        source_client_type: &str,
        source_lease_id: Option<&str>,
        repository_id: Option<&str>,
        session_id: Option<&str>,
        workflow_id: Option<&str>,
    ) -> EventSeq {
        let seq = {
            let mut next = self.next_seq.lock().unwrap();
            let seq = *next;
            *next += 1;
            seq
        };

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Compute evidence digest
        let digest_input = format!("{}:{}:{}", seq, event_type, payload);
        let evidence_digest = format!("blake3:{}", blake3::hash(digest_input.as_bytes()).to_hex());

        let event = EventRecord {
            seq_id: seq,
            event_type: event_type.to_string(),
            timestamp,
            source_client_id: source_client_id.to_string(),
            source_client_type: source_client_type.to_string(),
            source_lease_id: source_lease_id.map(|s| s.to_string()),
            repository_id: repository_id.map(|s| s.to_string()),
            session_id: session_id.map(|s| s.to_string()),
            workflow_id: workflow_id.map(|s| s.to_string()),
            payload,
            evidence_digest,
        };

        // Store in memory buffer
        {
            let mut buffer = self.memory_buffer.write();
            buffer.insert(seq, event.clone());
            
            // Keep only last 10000 events in memory
            if buffer.len() > 10000 {
                let first_key = *buffer.keys().next().unwrap();
                buffer.remove(&first_key);
            }
        }

        // Persist to SQLite
        if let Some(conn) = &self.sqlite {
            let payload_json = serde_json::to_string(&event.payload).unwrap_or_default();
            if let Err(e) = conn.execute(
                "INSERT INTO events (seq_id, event_type, timestamp, source_client_id, source_client_type, source_lease_id, repository_id, session_id, workflow_id, payload, evidence_digest)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                rusqlite::params![
                    seq,
                    &event.event_type,
                    event.timestamp as i64,
                    &event.source_client_id,
                    &event.source_client_type,
                    &event.source_lease_id,
                    &event.repository_id,
                    &event.session_id,
                    &event.workflow_id,
                    &payload_json,
                    &event.evidence_digest,
                ]
            ) {
                warn!("Failed to persist event to SQLite: {}", e);
            }
        }

        // Broadcast to subscribers
        let _ = self.broadcast_tx.send(event);

        debug!("Published event seq={} type={}", seq, event_type);
        seq
    }

    /// Get events since a sequence ID (for replay)
    pub fn get_events_since(&self, since_seq: EventSeq, limit: usize) -> Vec<EventRecord> {
        // First check memory buffer
        let buffer = self.memory_buffer.read();
        let mut events: Vec<_> = buffer.range((since_seq + 1)..)
            .take(limit)
            .map(|(_, v)| v.clone())
            .collect();

        if events.len() >= limit {
            return events;
        }

        // If not enough in memory, query SQLite
        if let Some(conn) = &self.sqlite {
            let remaining = limit - events.len();
            let mut stmt = conn.prepare(
                "SELECT seq_id, event_type, timestamp, source_client_id, source_client_type, 
                       source_lease_id, repository_id, session_id, workflow_id, payload, evidence_digest
                 FROM events 
                 WHERE seq_id > ? 
                 ORDER BY seq_id ASC 
                 LIMIT ?"
            ).ok();
            
            if let Some(mut stmt) = stmt {
                if let Ok(rows) = stmt.query(rusqlite::params![since_seq, remaining as i64]) {
                    for row in rows {
                        if let Ok(event) = Self::row_to_event(&row) {
                            // Avoid duplicates from memory buffer
                            if !events.iter().any(|e| e.seq_id == event.seq_id) {
                                events.push(event);
                            }
                        }
                    }
                }
            }
        }

        events
    }

    /// Get latest event sequence ID
    pub fn latest_seq(&self) -> EventSeq {
        *self.next_seq.lock().unwrap().saturating_sub(1)
    }

    /// Get latest session ID from events
    pub async fn get_latest_session(&self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        if let Some(conn) = &self.sqlite {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT session_id FROM events 
                 WHERE session_id IS NOT NULL 
                 ORDER BY seq_id DESC LIMIT 1"
            )?;
            if let Ok(mut rows) = stmt.query([]) {
                if let Some(row) = rows.next()? {
                    return Ok(row.read::<String, _>(0).ok());
                }
            }
        }
        Ok(None)
    }

    fn row_to_event(row: &rusqlite::Row) -> Result<EventRecord, rusqlite::Error> {
        let payload_str: String = row.read(9)?;
        let payload: Value = serde_json::from_str(&payload_str).unwrap_or(Value::Null);
        
        Ok(EventRecord {
            seq_id: row.read(0)?,
            event_type: row.read(1)?,
            timestamp: row.read::<i64, _>(2)? as u64,
            source_client_id: row.read(3)?,
            source_client_type: row.read(4)?,
            source_lease_id: row.read(5)?,
            repository_id: row.read(6)?,
            session_id: row.read(7)?,
            workflow_id: row.read(8)?,
            payload,
            evidence_digest: row.read(10)?,
        })
    }
}