//! Events handlers - SSE and WebSocket

use axum::{
    extract::{Query, State, WebSocketUpgrade},
    response::{IntoResponse, Response, Sse},
    Json,
};
use axum::extract::ws::{Message, WebSocket};
use futures::{stream, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use crate::{AppState, error::AppError};

#[derive(Debug, Deserialize)]
pub struct PollQuery {
    pub since_seq: Option<i64>,
    pub limit: Option<usize>,
}

/// SSE endpoint for live events
pub async fn sse(
    Query(params): Query<PollQuery>,
    State(state): State<crate::AppState>,
) -> Sse<impl Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let since_seq = params.since_seq.unwrap_or(0);
    let limit = params.limit.unwrap_or(500);
    
    // Get events from store
    let events = state.event_store.get_events_since(since_seq, limit).await;
    
    // Create initial events stream
    let initial_stream = stream::iter(events.into_iter().map(|event| {
        Ok(axum::response::sse::Event::default()
            .event(&event.event_type)
            .data(serde_json::to_string(&event.payload).unwrap_or_default())
            .id(event.seq_id.to_string()))
    }));
    
    // Subscribe to live events
    let rx = state.event_store.subscribe();
    let live_stream = BroadcastStream::new(rx)
        .filter_map(|event| async move {
            event.ok().map(|event| {
                Ok(axum::response::sse::Event::default()
                    .event(&event.event_type)
                    .data(serde_json::to_string(&event.payload).unwrap_or_default())
                    .id(event.seq_id.to_string()))
            })
        });
    
    let combined = initial_stream.chain(live_stream);
    
    Sse::new(combined)
        .keep_alive(axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"))
}

/// WebSocket endpoint for live events
pub async fn websocket(
    ws: WebSocketUpgrade,
    Query(params): Query<PollQuery>,
    State(state): State<crate::AppState>,
) -> Response {
    let since_seq = params.since_seq.unwrap_or(0);
    
    ws.on_upgrade(|socket| handle_websocket(socket, state, since_seq))
}

async fn handle_websocket(
    mut socket: WebSocket,
    state: crate::AppState,
    since_seq: i64,
) {
    // Send historical events
    let events = state.event_store.get_events_since(since_seq, 500).await;
    for event in events {
        let msg = Message::Text(serde_json::to_string(&json!({
            "seq_id": event.seq_id,
            "event_type": event.event_type,
            "data": event.payload,
            "evidence_digest": event.evidence_digest
        })).unwrap());
        
        if socket.send(msg).await.is_err() {
            return;
        }
    }
    
    // Subscribe to live events
    let mut rx = state.event_store.subscribe();
    
    // Send keep-alive ping every 30 seconds
    let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
    
    loop {
        tokio::select! {
            // Receive from broadcast
            event = rx.recv() => {
                match event {
                    Ok(event) => {
                        let msg = Message::Text(serde_json::to_string(&json!({
                            "seq_id": event.seq_id,
                            "event_type": event.event_type,
                            "data": event.payload,
                            "evidence_digest": event.evidence_digest
                        })).unwrap());
                        
                        if socket.send(msg).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break, // Channel closed
                }
            }
            _ = ping_interval.tick() => {
                if socket.send(Message::Ping(vec![])).await.is_err() {
                    break;
                }
            }
            // Receive messages from client
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        // Handle client messages (ping/poll)
                        if let Ok(req) = serde_json::from_str::<serde_json::Value>(&text) {
                            if req.get("event") == Some(&json!("live.poll")) {
                                // Send recent events
                                let since = req.get("data").and_then(|d| d.get("since_seq")).and_then(|v| v.as_i64()).unwrap_or(0);
                                let events = state.event_store.get_events_since(since, 500).await;
                                for event in events {
                                    let msg = Message::Text(serde_json::to_string(&json!({
                                        "seq_id": event.seq_id,
                                        "event_type": event.event_type,
                                        "data": event.payload,
                                        "evidence_digest": event.evidence_digest
                                    })).unwrap());
                                    
                                    if socket.send(msg).await.is_err() {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        // Client responded to ping
                    }
                    Some(Ok(Message::Close(_))) => break,
                    _ => break,
                }
            }
        }
    }
}

/// Poll endpoint for long-polling clients
pub async fn poll(
    Query(params): Query<PollQuery>,
    State(state): State<crate::AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let since_seq = params.since_seq.unwrap_or(0);
    let limit = params.limit.unwrap_or(500);
    
    let events = state.event_store.get_events_since(since_seq, limit).await;
    
    let replay_complete = events.len() < limit;
    
    Ok(axum::Json(json!({
        "events": events,
        "replay_complete": replay_complete
    })))
}

/// Broadcast endpoint for clients to publish events
#[derive(Debug, serde::Deserialize)]
pub struct BroadcastRequest {
    pub event: String,
    pub data: Value,
}

pub async fn broadcast(
    State(state): State<crate::AppState>,
    Json(request): Json<BroadcastRequest>,
) -> Result<Json<Value>, crate::error::AppError> {
    let seq = state.event_store.publish(
        &request.event,
        request.data,
        "unknown", // source_client_id
        "unknown", // source_client_type
        None,      // source_lease_id
        None,      // repository_id
        None,      // session_id
        None,      // workflow_id
    );
    
    Ok(axum::Json(json!({
        "seq_id": seq,
        "status": "broadcast"
    })))
}