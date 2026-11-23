//! Server module - HTTP router and application state

use std::sync::Arc;
use axum::{
    routing::{get, post, put, delete},
    Router,
};

use crate::handlers::{
    repo, impact, verify, diff, git, graph, forensic, analytics, catalog, telemetry,
    legacy, runtime,
};
use crate::{AppState, error::AppError};

/// Application state shared across all handlers
#[derive(Clone)]
pub struct AppState {
    pub config: crate::config::DaemonConfig,
    pub event_store: Arc<crate::events::EventStore>,
    pub runtime_manager: Arc<crate::runtime::RuntimeManager>,
    pub repository_manager: Arc<crate::repository::RepositoryManager>,
    pub loopback_auth: Arc<crate::auth::LoopbackAuth>,
}

/// Build the main router with all routes
pub async fn router(state: crate::AppState) -> Result<axum::Router, crate::error::AppError> {
    let api_v1 = Router::new()
        // Health check (no auth required)
        .route("/api/v1/health", get(health_check))
        
        // Repository management
        .route("/api/v1/repos/connect", post(repo::connect))
        .route("/api/v1/repos/list", get(repo::list))
        .route("/api/v1/repos/switch", post(repo::switch))
        .route("/api/v1/repos/remove", post(repo::remove))
        .route("/api/v1/repos/export", get(repo::export))
        
        // Runtime (lease management)
        .route("/api/v1/runtime/connect", post(runtime::connect))
        .route("/api/v1/runtime/heartbeat", post(runtime::heartbeat))
        .route("/api/v1/runtime/disconnect", post(runtime::disconnect))
        .route("/api/v1/runtime/status", get(runtime::status))
        
        // Live events (SSE + WebSocket)
        .route("/api/v1/live", get(events::sse))
        .route("/api/v1/live", get(events::websocket))
        .route("/api/v1/live/poll", get(events::poll))
        .route("/api/v1/live/broadcast", post(events::broadcast))
        
        // Filesystem
        .route("/api/v1/fs/browse", get(repo::browse))
        .route("/api/v1/files", get(repo::files))
        .route("/api/v1/files/content", get(repo::file_content))
        
        // Graph
        .route("/api/v1/graph/universe", get(graph::universe))
        .route("/api/v1/graph/ego", get(graph::ego))
        .route("/api/v1/graph/ego/:id", get(graph::ego_id))
        
        // Simulation
        .route("/api/v1/simulate/edit", post(analytics::simulate))
        
        // Analytics
        .route("/api/v1/analytics/report", get(analytics::report))
        .route("/api/v1/analytics/runtime-window", post(analytics::runtime_window))
        
        // Git
        .route("/api/v1/git/galaxy", get(git::galaxy))
        .route("/api/v1/git/time-travel", get(git::time_travel))
        .route("/api/v1/git/blame", get(git::blame))
        .route("/api/v1/git/diff-file", get(git::diff_file))
        .route("/api/v1/git/diff", get(git::diff))
        .route("/api/v1/git/stage", post(git::stage))
        .route("/api/v1/git/commit", post(git::commit))
        .route("/api/v1/git/branch", post(git::branch))
        .route("/api/v1/git/unstage", post(git::unstage))
        .route("/api/v1/git/tag", post(git::tag))
        .route("/api/v1/git/merge", post(git::merge))
        .route("/api/v1/git/checkout", post(git::checkout))
        .route("/api/v1/git/revert", post(git::revert))
        .route("/api/v1/git/bisect", post(git::bisect))
        .route("/api/v1/git/status", get(git::status))
        .route("/api/v1/git/diff-file", get(git::diff_file))
        .route("/api/v1/terminal/exec", post(git::terminal_exec))
        
        // Forensics
        .route("/api/v1/forensics/sessions", get(forensic::sessions))
        .route("/api/v1/forensics/sessions/start", post(forensic::start_session))
        .route("/api/v1/forensics/event", post(forensic::record_event))
        .route("/api/v1/forensics/timeline", get(forensic::timeline))
        .route("/api/v1/forensics/event", get(forensic::event))
        .route("/api/v1/forensics/revert", post(forensic::revert))
        .route("/api/v1/forensics/export", get(forensic::export))
        .route("/api/v1/forensics/import", post(forensic::import))
        
        // AI
        .route("/api/v1/ai/ledger", get(analytics::ledger))
        .route("/api/v1/ai/cache/query", post(analytics::cache_query))
        .route("/api/v1/ai/config-snippet", get(analytics::config_snippet))
        
        // Impact & Verify (legacy + new)
        .route("/api/impact", get(impact::get))
        .route("/api/v1/impact", get(impact::get_v1))
        .route("/api/verify", get(verify::get))
        .route("/api/v1/verify", get(verify::get_v1))
        .route("/api/v1/diff", get(diff::get))
        .route("/api/v1/diff", post(diff::post))
        
        // Telemetry
        .route("/api/v1/telemetry/ingest", post(telemetry::ingest))
        .route("/api/v1/telemetry/topology", get(telemetry::topology))
        
        // Catalog
        .route("/api/v1/catalog/scan", post(catalog::scan))
        .route("/api/v1/catalog/blast-radius", post(catalog::blast_radius))
        
        // Impact & Verify (new API)
        .route("/api/v1/impact", get(impact::get_v1))
        .route("/api/v1/verify", get(verify::get_v1))
        .route("/api/v1/verify", post(verify::post))
        
        // Forensic sessions
        .route("/api/v1/forensics/sessions", get(forensic::sessions))
        .route("/api/v1/forensics/sessions/start", post(forensic::start_session))
        .route("/api/v1/forensics/event", post(forensic::record_event))
        .route("/api/v1/forensics/timeline", get(forensic::timeline))
        .route("/api/v1/forensics/revert", post(forensic::revert))
        .route("/api/v1/forensics/export", get(forensic::export))
        .route("/api/v1/forensics/import", post(forensic::import))
        
        // Capabilities
        .route("/api/v1/capabilities", get(capabilities));
    
    let router = Router::new()
        .nest("/api/v1", api_v1)
        // Legacy endpoints (for backward compatibility)
        .route("/api/status", get(legacy::status))
        .route("/api/verify", get(legacy::verify))
        .route("/api/impact", get(legacy::impact))
        .route("/api/trajectory", get(legacy::trajectory))
        .route("/api/contracts", get(legacy::contracts))
        .route("/api/graph", get(legacy::graph))
        .route("/api/trajectory", get(legacy::trajectory))
        // CORS preflight
        .route("/*path", axum::routing::options(cors_preflight))
        // Root
        .route("/", get(root));
    
    Ok(router)
}

/// Health check endpoint
async fn health_check() -> impl axum::response::IntoResponse {
    use axum::Json;
    use serde_json::json;
    
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "daemon": "autopsy-daemon",
        "engine": "offline-deterministic"
    }))
}

/// Root endpoint
async fn root() -> impl axum::response::IntoResponse {
    use axum::Json;
    use serde_json::json;
    
    Json(serde_json::json!({
        "daemon": "autopsy-daemon",
        "status": "online",
        "version": env!("CARGO_PKG_VERSION"),
        "engine": "offline-deterministic",
        "api_base": "/api/v1",
        "documentation": "/api/v1/capabilities"
    }))
}

/// CORS preflight handler
async fn cors_preflight() -> impl axum::response::IntoResponse {
    use axum::http::StatusCode;
    (StatusCode::OK, [("Access-Control-Allow-Origin", "*")])
}

/// Capabilities endpoint
async fn capabilities() -> impl axum::response::IntoResponse {
    use axum::Json;
    use serde_json::json;
    
    Json(json!({
        "languages": ["TypeScript", "JavaScript", "Python", "Go", "Rust", "Java", "C#"],
        "resolvers": ["imports", "references", "calls", "inherits", "implements"],
        "layouts": ["force-directed", "hierarchical"],
        "gates": ["CLEAN_ARCHITECTURE_LAYERS", "NO_CIRCULAR_DEPS", "API_COMPATIBILITY"],
        "storage_engine": "sqlite",
        "coverage_state": "verified",
        "error": null
    }))
}

pub use crate::handlers;
pub use crate::handlers::legacy;