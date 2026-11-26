//! Custom extractors for request handling

use axum::{
    async_trait,
    extract::{FromRequestParts, Query},
    http::request::Parts,
    http::StatusCode,
};
use std::collections::HashMap;

use crate::{AppState, error::AppError};

/// Extractor for AppState with proper error handling
pub struct AppStateExtractor(pub crate::AppState);

#[async_trait]
impl FromRequestParts<crate::AppState> for AppStateExtractor {
    type Rejection = crate::error::AppError;

    async fn from_request_parts(parts: &mut Parts, state: &crate::AppState) -> Result<Self, Self::Rejection> {
        Ok(AppStateExtractor(state.clone()))
    }
}

/// Extractor for query parameters with validation
pub struct ValidatedQuery<T>(pub T);

#[async_trait]
impl<T, S> FromRequestParts<S> for ValidatedQuery<T>
where
    T: serde::de::DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = crate::error::AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let query = Query::<axum::extract::Query<HashMap<String, String>>>::from_request_parts(parts, &())
            .await
            .map_err(|e| crate::error::AppError::Validation(format!("Query parse error: {}", e)))?;
        
        // Convert to target type
        let value = serde_json::to_value(query.0)
            .map_err(|e| crate::error::AppError::Validation(format!("Query serialization error: {}", e)))?;
        
        let result = serde_json::from_value(value)
            .map_err(|e| crate::error::AppError::Validation(format!("Query validation error: {}", e)))?;
        
        Ok(ValidatedQuery(result))
    }
}

/// Extractor for pagination parameters
#[derive(Debug, serde::Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize {
    50
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self { limit: 50, offset: 0 }
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for PaginationParams
where
    S: Send + Sync,
{
    type Rejection = crate::error::AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let query = Query::<HashMap<String, String>>::from_request_parts(parts, &())
            .await
            .map_err(|e| crate::error::AppError::Validation(format!("Query parse error: {}", e)))?;
        
        let limit = query.0.get("limit")
            .and_then(|v| v.parse().ok())
            .unwrap_or(50)
            .min(1000);
        
        let offset = query.0.get("offset")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        
        Ok(PaginationParams { limit, offset })
    }
}

/// Extractor for snapshot pin parameter
#[derive(Debug, serde::Deserialize)]
pub struct SnapshotPin {
    pub snapshot_id: Option<String>,
}

#[async_trait]
impl<S> FromRequestParts<S> for SnapshotPin
where
    S: Send + Sync,
{
    type Rejection = crate::error::AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let query = Query::<HashMap<String, String>>::from_request_parts(parts, &())
            .await
            .map_err(|e| crate::error::AppError::Validation(format!("Query parse error: {}", e)))?;
        
        let snapshot_id = query.0.get("snapshot_id").cloned();
        
        Ok(SnapshotPin { snapshot_id })
    }
}

/// Extractor for session/workflow context
#[derive(Debug, serde::Deserialize)]
pub struct SessionContext {
    pub session_id: Option<String>,
    pub workflow_id: Option<String>,
    pub event_seq: Option<i64>,
}

#[async_trait]
impl<S> FromRequestParts<S> for SessionContext
where
    S: Send + Sync,
{
    type Rejection = crate::error::AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let query = Query::<HashMap<String, String>>::from_request_parts(parts, &())
            .await
            .map_err(|e| crate::error::AppError::Validation(format!("Query parse error: {}", e)))?;
        
        Ok(SessionContext {
            session_id: query.0.get("session_id").cloned(),
            workflow_id: query.0.get("workflow_id").cloned(),
            event_seq: query.0.get("event_seq").and_then(|v| v.parse().ok()),
        })
    }
}

/// Custom extractor for client identification
#[derive(Debug, Clone)]
pub struct ClientContext {
    pub client_id: String,
    pub client_type: String,
    pub lease_id: Option<String>,
}

#[async_trait]
impl<S> FromRequestParts<S> for ClientContext
where
    S: Send + Sync,
{
    type Rejection = crate::error::AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let headers = &parts.headers;
        
        let client_id = headers
            .get("X-Synevid-Client-Id")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("anonymous")
            .to_string();
        
        let client_type = headers
            .get("X-Synevid-Client-Type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("unknown")
            .to_string();
        
        let lease_id = headers
            .get("X-Synevid-Lease-Id")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());
        
        Ok(ClientContext {
            client_id,
            client_type,
            lease_id,
        })
    }
}