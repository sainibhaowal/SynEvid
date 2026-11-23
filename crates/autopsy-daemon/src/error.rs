//! Error types for the daemon

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Authentication error: {0}")]
    Auth(String),
    
    #[error("Authorization error: {0}")]
    Authorization(String),
    
    #[error("Analysis error: {0}")]
    Analysis(String),
    
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Repository error: {0}")]
    Repository(String),
    
    #[error("Git error: {0}")]
    Git(String),
    
    #[error("Analysis error: {0}")]
    AnalysisError(String),
    
    #[error("Validation error: {0}")]
    Validation(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Internal error: {0}")]
    Internal(String),
    
    #[error("Unsupported: {0}")]
    Unsupported(String),
    
    #[error("Rate limited")]
    RateLimited,
    
    #[error("Timeout")]
    Timeout,
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    
    #[error("HTTP error: {0}")]
    Http(#[from] axum::http::Error),
    
    #[error("Parse error: {0}")]
    Parse(#[from] std::num::ParseIntError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_code, message) = match &self {
            AppError::Config(msg) => (StatusCode::BAD_REQUEST, "ERR-CONFIG", msg.clone()),
            AppError::Auth(msg) => (StatusCode::UNAUTHORIZED, "ERR-AUTH-401", msg.clone()),
            AppError::Authorization(msg) => (StatusCode::FORBIDDEN, "ERR-AUTH-403", msg.clone()),
            AppError::Analysis(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "ERR-ANALYSIS-500", msg.clone()),
            AppError::Storage(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "ERR-STORAGE-500", msg.clone()),
            AppError::Repository(msg) => (StatusCode::NOT_FOUND, "ERR-REPO-404", msg.clone()),
            AppError::Git(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "ERR-GIT-500", msg.clone()),
            AppError::AnalysisError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "ERR-ANALYSIS-500", msg.clone()),
            AppError::Validation(msg) => (StatusCode::BAD_REQUEST, "ERR-VALIDATION-400", msg.clone()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "ERR-NOTFOUND-404", msg.clone()),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "ERR-INTERNAL-500", msg.clone()),
            AppError::Unsupported(msg) => (StatusCode::NOT_IMPLEMENTED, "ERR-UNSUPPORTED-501", msg.clone()),
            AppError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "ERR-RATELIMIT-429", "Rate limited".to_string()),
            AppError::Timeout => (StatusCode::REQUEST_TIMEOUT, "ERR-TIMEOUT-408", "Request timeout".to_string()),
            AppError::Io(e) => (StatusCode::INTERNAL_SERVER_ERROR, "ERR-IO-500", e.to_string()),
            AppError::Serialization(e) => (StatusCode::BAD_REQUEST, "ERR-SERIALIZATION-400", e.to_string()),
            AppError::Database(e) => (StatusCode::INTERNAL_SERVER_ERROR, "ERR-DATABASE-500", e.to_string()),
            AppError::Http(e) => (StatusCode::BAD_REQUEST, "ERR-HTTP-400", e.to_string()),
            AppError::Parse(e) => (StatusCode::BAD_REQUEST, "ERR-PARSE-400", e.to_string()),
            AppError::Unsupported(msg) => (StatusCode::NOT_IMPLEMENTED, "ERR-UNSUPPORTED-501", msg.clone()),
            AppError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "ERR-RATELIMIT-429", "Rate limited".to_string()),
            AppError::Timeout => (StatusCode::REQUEST_TIMEOUT, "ERR-TIMEOUT-408", "Request timeout".to_string()),
            AppError::Io(e) => (StatusCode::INTERNAL_SERVER_ERROR, "ERR-IO-500", e.to_string()),
            AppError::Serialization(e) => (StatusCode::BAD_REQUEST, "ERR-SERIALIZATION-400", e.to_string()),
            AppError::Database(e) => (StatusCode::INTERNAL_SERVER_ERROR, "ERR-DATABASE-500", e.to_string()),
            AppError::Http(e) => (StatusCode::BAD_REQUEST, "ERR-HTTP-400", e.to_string()),
            AppError::Parse(e) => (StatusCode::BAD_REQUEST, "ERR-PARSE-400", e.to_string()),
            AppError::Unsupported(msg) => (StatusCode::NOT_IMPLEMENTED, "ERR-UNSUPPORTED-501", msg.clone()),
            AppError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "ERR-RATELIMIT-429", "Rate limited".to_string()),
            AppError::Timeout => (StatusCode::REQUEST_TIMEOUT, "ERR-TIMEOUT-408", "Request timeout".to_string()),
        };
        
        let body = Json(json!({
            "error_code": error_code,
            "error": message,
        }));
        
        (status, body).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;