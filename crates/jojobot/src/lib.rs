//! jojobot library surface — the app is assembled here so both the binary and
//! the integration tests build the exact same router.

pub mod auth;
pub mod config;
pub mod routes;

use std::sync::Arc;

use axum::{Router, routing::get};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tokio_util::sync::CancellationToken;

use jojobot_mcp::Jojobot;

use crate::auth::Validator;

/// Shared state for the metadata endpoint and the auth middleware.
#[derive(Clone)]
pub struct AppState {
    /// This server's resource identifier (RFC 9728).
    pub resource: String,
    /// The configured issuer, if auth is enabled.
    pub issuer: Option<String>,
    /// The token validator, if auth is enabled.
    pub validator: Option<Arc<Validator>>,
    /// Absolute URL of the protected-resource metadata endpoint.
    pub metadata_url: String,
}

/// Build the full HTTP application: the guarded MCP transport plus the public
/// health and protected-resource-metadata endpoints. `ct` cancels the MCP
/// session manager on shutdown.
pub fn build_app(state: AppState, ct: CancellationToken) -> Router {
    let mcp = StreamableHttpService::new(
        || Ok(Jojobot::new()),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default().with_cancellation_token(ct),
    );

    // The MCP transport is guarded; the metadata + health endpoints are public.
    let mut mcp_router = Router::new().nest_service("/mcp", mcp);
    if state.validator.is_some() {
        mcp_router = mcp_router.layer(axum::middleware::from_fn_with_state(
            state.clone(),
            routes::require_bearer,
        ));
    }

    Router::new()
        .route("/healthz", get(routes::health))
        .route(
            "/.well-known/oauth-protected-resource",
            get(routes::protected_resource_metadata),
        )
        .merge(mcp_router)
        .with_state(state)
}
