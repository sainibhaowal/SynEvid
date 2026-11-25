//! Server module - HTTP router and application state

use std::sync::Arc;
use axum::{
    routing::{get, post, put, delete},
    Router,
};

use crate::handlers::{
    repo, runtime,
};
use crate::{AppState, error::AppError};

/// Build the main router with all routes
pub fn router(state: AppState) -> Result<axum::Router<AppState>, AppError> {
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
        
        // Filesystem
        .route("/api/v1/fs/browse", get(repo::browse))
        .route("/api/v1/files", get(repo::files))
        .route("/api/v1/files/content", get(repo::file_content))
        
        // Capabilities
        .route("/api/v1/capabilities", get(capabilities));

    let router = Router::new()
        .nest("/api/v1", api_v1)
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