//! autopsy-daemon: Consolidated Daemon Binary
//!
//! This is the consolidated daemon binary that replaces both:
//! - `autopsy ui` (CLI-embedded server in crates/autopsy-cli/src/ui.rs)
//! - `synevid-ui` (Electron sidecar in apps/synevid-ui/src/server.rs)
//!
//! This crate provides the core server implementation and shared types.

pub mod auth;
pub mod config;
pub mod error;
pub mod events;
pub mod repository;
pub mod runtime;
pub mod server;
pub mod handlers;

use std::sync::Arc;
use axum::Router;
use crate::config::DaemonConfig;
use crate::events::EventStore;
use crate::runtime::RuntimeManager;
use crate::repository::RepositoryManager;
use crate::auth::LoopbackAuth;

/// Application state shared across all handlers
#[derive(Clone)]
pub struct AppState {
    pub config: crate::config::DaemonConfig,
    pub event_store: std::sync::Arc<crate::events::EventStore>,
    pub runtime_manager: std::sync::Arc<crate::runtime::RuntimeManager>,
    pub repository_manager: std::sync::Arc<crate::repository::RepositoryManager>,
    pub loopback_auth: std::sync::Arc<crate::auth::LoopbackAuth>,
}

pub use crate::error::AppError;