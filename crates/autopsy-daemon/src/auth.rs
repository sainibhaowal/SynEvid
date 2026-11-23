//! Loopback authentication for daemon

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::RwLock;
use blake3;
use tracing::{info, warn};

/// Loopback authentication manager
#[derive(Clone)]
pub struct LoopbackAuth {
    token: Arc<RwLock<Option<String>>>,
    token_file: PathBuf,
    disabled: bool,
}

impl LoopbackAuth {
    pub fn new(data_dir: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let token_file = data_dir.join("loopback-token");
        let token = if token_file.exists() {
            let token = fs::read_to_string(&token_file)?.trim().to_string();
            if !token.is_empty() {
                info!("Loaded existing loopback token from {}", token_file.display());
                Some(token)
            } else {
                None
            }
        } else {
            None
        };

        Ok(Self {
            token: Arc::new(RwLock::new(token)),
            token_file,
            disabled: false,
        })
    }

    pub fn disabled() -> Self {
        Self {
            token: Arc::new(RwLock::new(None)),
            token_file: PathBuf::new(),
            disabled: true,
        }
    }

    pub fn get_token(&self) -> Option<String> {
        if self.disabled {
            return None;
        }
        self.token.read().clone()
    }

    pub fn ensure_token(&self) -> String {
        if self.disabled {
            return String::new();
        }
        
        let mut token_guard = self.token.write();
        if let Some(token) = token_guard.as_ref() {
            return token.clone();
        }

        // Generate new token
        let token = generate_loopback_token();
        
        // Persist token
        if let Some(parent) = self.token_file.parent() {
            fs::create_dir_all(parent).ok();
        }
        if fs::write(&self.token_file, &token).is_ok() {
            info!("Generated and persisted new loopback token to {}", self.token_file.display());
        } else {
            warn!("Failed to persist loopback token to {}", self.token_file.display());
        }

        *token_guard = Some(token.clone());
        token
    }

    pub fn validate(&self, provided: &str) -> bool {
        if self.disabled {
            return true; // Auth disabled
        }
        
        self.token.read().as_ref().map(|t| t == provided).unwrap_or(false)
    }

    pub fn extract_from_headers(&self, headers: &axum::http::HeaderMap) -> Option<String> {
        // Check Authorization header
        headers
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|s| s.to_string())
            .or_else(|| {
                // Fallback to query param (for SSE/WebSocket)
                None // Would need request context
            })
    }

    pub fn disabled(&self) -> bool {
        self.disabled
    }
}

fn generate_loopback_token() -> String {
    use blake3;
    let input = format!(
        "synevid-loopback-{}-{}", 
        std::process::id(), 
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    blake3::hash(input.as_bytes()).to_hex().to_string()
}