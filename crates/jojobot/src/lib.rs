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

use jojobot_domain::mailbox::Mailboxes;
use jojobot_domain::memory::Memory;
use jojobot_domain::memory::search::Search;
use jojobot_domain::session::Sessions;
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
    /// The Memory port backing the `capture`/`recall` tools. Always the real
    /// Outline adapter (possibly unconfigured) — no toy store ships.
    pub memory: Arc<dyn Memory>,
    /// The retrieval port backing `search` — the projection over the same store.
    /// A separate port, not a second store: in production both fields are the one
    /// indexed adapter, so every write keeps the index current.
    pub search: Arc<dyn Search>,
    /// The Mailboxes port backing the mailbox tools. A **different bounded
    /// context with a different store** (Vikunja, not Outline) — always the real
    /// adapter, possibly unconfigured; no toy store ships.
    pub mailboxes: Arc<dyn Mailboxes>,
    /// The Sessions port backing `journal`, `amend_journal`, `wrap_session` and
    /// the session half of `boot_bot`. A third context on the same Vikunja, in
    /// **its own project** — never the mailbox one.
    pub sessions: Arc<dyn Sessions>,
}

/// Build the full HTTP application: the guarded MCP transport plus the public
/// health and protected-resource-metadata endpoints. `ct` cancels the MCP
/// session manager on shutdown.
pub fn build_app(state: AppState, ct: CancellationToken) -> Router {
    // The transport guards against DNS rebinding by only accepting loopback
    // `Host` headers by default. Behind a TLS-terminating tunnel the inbound
    // Host is our public hostname, so add the resource's authority to the
    // allowlist. Loopback stays for local health probes.
    let mut server_config = StreamableHttpServerConfig::default().with_cancellation_token(ct);
    if let Some(authority) = crate::config::authority_of(&state.resource) {
        server_config.allowed_hosts.push(authority);
    }

    let memory = state.memory.clone();
    let search = state.search.clone();
    let mailboxes = state.mailboxes.clone();
    let sessions = state.sessions.clone();
    // **Built once, outside the factory** — every handler this process makes
    // shares it. A session handle is an address across connections, so a
    // registry built per connect would forget each one as it handed it out.
    // The other side of that: it lives in RAM, so a restart is the end of every
    // handle it ever issued, which the door says out loud rather than papering
    // over with a new session.
    let registry = Arc::new(jojobot_mcp::sid::SessionRegistry::new());
    let mcp = StreamableHttpService::new(
        // **One handler per MCP session, and that is what makes the connection
        // binding a connection binding**: the factory runs per connect, so a
        // session bound here belongs to this client and evaporates with it.
        move || {
            Ok(Jojobot::new(
                memory.clone(),
                search.clone(),
                mailboxes.clone(),
                sessions.clone(),
                registry.clone(),
            ))
        },
        LocalSessionManager::default().into(),
        server_config,
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
