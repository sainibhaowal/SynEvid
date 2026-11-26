//! Event store for live event streaming and replay

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use blake3;
use parking_lot::RwLock;
use tokio::sync::broadcast;
use tracing::{debug, warn};
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
    // SQLite storage (optional, for durability) - wrapped in Mutex for Send/Sync
    sqlite: Arc<Mutex<Option<rusqlite::Connection>>>,
}

impl EventStore {
    pub fn new(data_dir: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let storage_path = data_dir.join("events.sqlite");
        let (broadcast_tx, _) = broadcast::channel(10000);

        // Initialize SQLite for durable event storage
        let sqlite = Arc::new(Mutex::new(None));
        let storage_path_clone = storage_path.clone();
        
        // Initialize database in a blocking task
        let conn = rusqlite::Connection::open(&storage_path_clone)?;
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
        
        *sqlite.lock().unwrap() = Some(conn);

        // Load last sequence ID from SQLite
        let next_seq = {
            let conn_guard = sqlite.lock().unwrap();
            if let Some(conn) = conn_guard.as_ref() {
                let mut stmt = conn.prepare("SELECT COALESCE(MAX(seq_id), 0) + 1 FROM events")?;
                let seq: i64 = stmt.query_row([], |row| row.get(0)).unwrap_or(1);
                Arc::new(Mutex::new(seq))
            } else {
                Arc::new(Mutex::new(1))
            }
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

    /// Append an event to the store
    pub async fn append(&self, mut event: EventRecord) -> EventSeq {
        // Assign sequence ID
        let seq = {
            let mut next = self.next_seq.lock().unwrap();
            *next += 1;
            *next
        };
        event.seq_id = seq;

        // Add to memory buffer
        self.memory_buffer.write().insert(seq, event.clone());

        // Broadcast to subscribers
        let _ = self.broadcast_tx.send(event.clone());

        // Persist to SQLite in background
        let sqlite = self.sqlite.clone();
        let event_clone = event.clone();
        tokio::task::spawn_blocking(move || {
            if let Ok(mut conn_guard) = sqlite.lock() {
                if let Some(conn) = conn_guard.as_mut() {
                    let _ = conn.execute(
                        "INSERT INTO events (seq_id, event_type, timestamp, source_client_id, source_client_type, source_lease_id, repository_id, session_id, workflow_id, payload, evidence_digest)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                        rusqlite::params![
                            event_clone.seq_id,
                            event_clone.event_type,
                            event_clone.timestamp as i64,
                            event_clone.source_client_id,
                            event_clone.source_client_type,
                            event_clone.source_lease_id,
                            event_clone.repository_id,
                            event_clone.session_id,
                            event_clone.workflow_id,
                            serde_json::to_string(&event_clone.payload).unwrap_or_default(),
                            event_clone.evidence_digest,
                        ],
                    );
                }
            }
        });

        seq
    }

    /// Get events since a sequence ID
    pub async fn get_events_since(&self, since_seq: EventSeq, limit: usize) -> Vec<EventRecord> {
        let mut events: Vec<EventRecord> = self.memory_buffer.read()
            .range((std::ops::Bound::Excluded(since_seq), std::ops::Bound::Unbounded))
            .take(limit)
            .map(|(_, v)| v.clone())
            .collect();

        // If not enough in memory, query SQLite
        if events.len() < limit {
            let remaining = limit - events.len();
            let sqlite = self.sqlite.clone();
            let since = since_seq;
            
            // Use blocking task for SQLite query
            if let Ok(extra_events) = tokio::task::spawn_blocking(move || {
                let mut extra = Vec::new();
                if let Ok(conn_guard) = sqlite.lock() {
                    if let Some(conn) = conn_guard.as_ref() {
                        let mut stmt = conn.prepare(
                            "SELECT seq_id, event_type, timestamp, source_client_id, source_client_type, 
                                   source_lease_id, repository_id, session_id, workflow_id, payload, evidence_digest
                              FROM events 
                              WHERE seq_id > ? 
                              ORDER BY seq_id ASC 
                              LIMIT ?"
                        ).ok();
                        
                        if let Some(mut stmt) = stmt {
                            let rows = stmt.query(rusqlite::params![since, remaining as i64]);
                            if let Ok(mut rows) = rows {
                                while let Some(row) = rows.next().ok().flatten() {
                                    if let Ok(event) = Self::row_to_event(row) {
                                        // Avoid duplicates from memory buffer
                                        if !extra.iter().any(|e: &EventRecord| e.seq_id == event.seq_id) {
                                            extra.push(event);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                extra
            }).await {
                events.extend(extra_events);
            }
        }

        events
    }

    /// Get latest event sequence ID
    pub fn latest_seq(&self) -> EventSeq {
        self.next_seq.lock().unwrap().saturating_sub(1)
    }

    /// Get latest session ID from events
    pub async fn get_latest_session(&self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        let sqlite = self.sqlite.clone();
        let session_id = tokio::task::spawn_blocking(move || -> Result<Option<String>, rusqlite::Error> {
            if let Ok(conn_guard) = sqlite.lock() {
                if let Some(conn) = conn_guard.as_ref() {
                    let mut stmt = conn.prepare(
                        "SELECT DISTINCT session_id FROM events 
                         WHERE session_id IS NOT NULL 
                         ORDER BY seq_id DESC LIMIT 1"
                    )?;
                    let mut rows = stmt.query([])?;
                    if let Some(row) = rows.next()? {
                        return Ok(row.get(0).ok());
                    }
                }
            }
            Ok(None)
        }).await??;
        
        Ok(session_id)
    }

    fn row_to_event(row: &rusqlite::Row) -> Result<EventRecord, rusqlite::Error> {
        let payload_str: String = row.get(9)?;
        let payload: Value = serde_json::from_str(&payload_str).unwrap_or(Value::Null);
        
        let timestamp: i64 = row.get(2)?;
        
        Ok(EventRecord {
            seq_id: row.get(0)?,
            event_type: row.get(1)?,
            timestamp: timestamp as u64,
            source_client_id: row.get(3)?,
            source_client_type: row.get(4)?,
            source_lease_id: row.get(5)?,
            repository_id: row.get(6)?,
            session_id: row.get(7)?,
            workflow_id: row.get(8)?,
            payload,
            evidence_digest: row.get(10)?,
        })
    }
}