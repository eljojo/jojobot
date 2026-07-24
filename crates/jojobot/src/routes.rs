//! HTTP surface around the MCP transport: the RFC 9728 protected-resource
//! metadata endpoint, a health probe, and the bearer-auth middleware that
//! guards `/mcp`.

use axum::{
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::AppState;
use crate::auth::bearer_from_header;

/// Liveness probe. Unauthenticated.
pub async fn health() -> &'static str {
    "ok"
}

/// RFC 9728 protected-resource metadata. Tells a client which authorization
/// server(s) mint tokens for this resource. Unauthenticated by design.
pub async fn protected_resource_metadata(State(state): State<AppState>) -> Json<serde_json::Value> {
    let authorization_servers: Vec<String> = state.issuer.iter().cloned().collect();
    Json(json!({
        "resource": state.resource,
        "authorization_servers": authorization_servers,
        "bearer_methods_supported": ["header"],
    }))
}

/// Require a valid bearer token. Mounted only when a validator is configured.
/// On failure it returns 401 with a `WWW-Authenticate` header pointing at the
/// protected-resource metadata, so a spec-compliant client can discover the AS.
pub async fn require_bearer(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let validator = state
        .validator
        .as_ref()
        .expect("require_bearer mounted without a validator");

    let header_value = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let outcome = bearer_from_header(header_value).and_then(|token| validator.validate(token));

    match outcome {
        Ok(claims) => {
            req.extensions_mut().insert(claims);
            next.run(req).await
        }
        Err(err) => {
            tracing::debug!(%err, "rejected /mcp request");
            unauthorized(&state.metadata_url)
        }
    }
}

fn unauthorized(metadata_url: &str) -> Response {
    let challenge = format!("Bearer resource_metadata=\"{metadata_url}\"");
    let mut resp = (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    if let Ok(value) = HeaderValue::from_str(&challenge) {
        resp.headers_mut().insert(header::WWW_AUTHENTICATE, value);
    }
    resp
}
