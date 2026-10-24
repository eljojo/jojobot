//! jojobot library surface — the app is assembled here so both the binary and
//! the integration tests build the exact same router.

pub mod app_api;
pub mod auth;
pub mod background;
pub mod config;
pub mod logging;
pub mod push;
pub mod request_log;
pub mod routes;
pub mod storage_check;
pub mod ui;
pub mod wiring;

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

/// **How long the calls in flight get to finish once the server is told to
/// stop.** The server takes no new call from the signal on. A call is a read or a
/// write against the store and finishes in well under a second, so ten seconds
/// is room for the slowest one with plenty to spare. A stream a client holds open
/// never finishes by itself, and this is the longest it can hold the stop up.
/// It is kept far under the service manager's default stop timeout of ninety
/// seconds, so the manager never has to kill the process.
pub const SHUTDOWN_DRAIN: std::time::Duration = std::time::Duration::from_secs(10);

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
    /// adapter (possibly unconfigured) — no toy store ships.
    pub memory: Arc<dyn Memory>,
    /// The retrieval port backing `search` — the projection over the same store.
    /// A separate port, not a second store: in production both fields are the one
    /// indexed adapter, so every write keeps the index current.
    pub search: Arc<dyn Search>,
    /// The Mailboxes port backing the mailbox tools. A **separate bounded
    /// context, kept in its own tables in the same SQL store** — always the
    /// real adapter, possibly unconfigured; no toy store ships.
    pub mailboxes: Arc<dyn Mailboxes>,
    /// The Sessions port backing `journal`, `amend_journal`, `wrap_session` and
    /// the session half of `start_here`. A third bounded context, in **its own
    /// tables in the same SQL store** — never the mailbox ones.
    pub sessions: Arc<dyn Sessions>,
    /// The Teachings port — whether a session's handle has already been
    /// taught how a domain behaves. Always the real adapter; no toy store
    /// ships.
    pub teachings: Arc<dyn jojobot_domain::teaching::Teachings>,
    /// **The ledger of request ids that writes have spent**, in the store the
    /// writes go to. `None` is an instance that keeps none, and a call that
    /// sends a `request_id` to it is refused before it writes.
    pub requests: Option<Arc<dyn jojobot_domain::request::Requests>>,
    /// **Every session handle this process can address**, shared by every
    /// connection. Built and filled from the board before the server serves, so
    /// the first caller after a restart gets the same answer as the second.
    pub registry: Arc<jojobot_mcp::sid::SessionRegistry>,
    /// The browser UI's OAuth client and its logged-in browsers. `None` means
    /// the UI is not configured, and then it is not mounted at all — there is no
    /// state in which its pages are reachable without a login.
    pub ui: Option<Arc<crate::ui::Ui>>,
    /// **The clock this server runs on.** Held here because the handler is
    /// built per connection, so a day read once at startup has to reach every
    /// handler the factory makes after it.
    pub clock: jojobot_domain::clock::Clock,
    /// **Told after a post to the operator lands**, when the instance is set up
    /// to push to a phone. `None` pushes nothing.
    pub push: Option<Arc<dyn jojobot_domain::push::Nudge>>,
    /// **How long a run must have gone without a write before its public name
    /// can take it.** `None` is the lease, which is what every instance runs
    /// with; a test of the other rules shortens it so it need not wait out the
    /// lease, and the test of this rule leaves it.
    pub quiet_after: Option<jiff::SignedDuration>,
    /// **The apps the operator registered.** Empty is an instance with none: the
    /// app route is not mounted.
    pub apps: Arc<app_api::Apps>,
}

/// Build the full HTTP application: the guarded MCP transport plus the public
/// health and protected-resource-metadata endpoints. `ct` cancels the MCP
/// session manager on shutdown.
pub fn build_app(state: AppState, ct: CancellationToken) -> Router {
    build_app_serving(state, ct, jojobot_mcp::orientation::skills::SKILLS)
}

/// [`build_app`] over a list of skills other than the shipped one, **for tests**:
/// the binary calls [`build_app`], which serves the shipped list. The scope of a
/// skill is read in one place, [`jojobot_mcp::orientation::skills::is_listed_for`],
/// and this adds no second reader.
pub fn build_app_serving(
    state: AppState,
    ct: CancellationToken,
    skills: &'static [jojobot_mcp::orientation::skills::Skill],
) -> Router {
    build_app_with(state, ct, skills, None)
}

/// [`build_app`] ending every `subscriptions/listen` after `budget`, **for
/// tests**: the binary serves the default budget with its jitter.
pub fn build_app_listening(
    state: AppState,
    ct: CancellationToken,
    budget: std::time::Duration,
) -> Router {
    build_app_with(
        state,
        ct,
        jojobot_mcp::orientation::skills::SKILLS,
        Some(budget),
    )
}

fn build_app_with(
    state: AppState,
    ct: CancellationToken,
    skills: &'static [jojobot_mcp::orientation::skills::Skill],
    listen_budget: Option<std::time::Duration>,
) -> Router {
    // The transport guards against DNS rebinding by only accepting loopback
    // `Host` headers by default. Behind a TLS-terminating tunnel the inbound
    // Host is our public hostname, so add the resource's authority to the
    // allowlist. Loopback stays for local health probes.
    let cut = ct.clone();
    let mut server_config = StreamableHttpServerConfig::default().with_cancellation_token(ct);
    if let Some(authority) = crate::config::authority_of(&state.resource) {
        server_config.allowed_hosts.push(authority);
    }

    let memory = state.memory.clone();
    let search = state.search.clone();
    // **Every path into a person's box wakes the push**, because the wake is
    // made on the port every handler posts through. A path that posts to the
    // operator and is not the `post_message` verb (a code asked for at the door)
    // cannot leave its message unpushed.
    let mailboxes: Arc<dyn jojobot_domain::mailbox::Mailboxes> = match &state.push {
        Some(push) => Arc::new(jojobot_domain::push::Nudging::new(
            state.mailboxes.clone(),
            push.clone(),
        )),
        None => state.mailboxes.clone(),
    };
    let sessions = state.sessions.clone();
    let teachings = state.teachings.clone();
    let requests = state.requests.clone();
    // **One registry per process, never per connection** — a session handle is
    // an address across connections, so a registry built per connect would
    // forget each one as it handed it out. Filled from the board before the
    // server serves (see `main`), which is what stops a restart from orphaning
    // every handle it ever issued.
    let registry = state.registry.clone();
    let clock = state.clock;
    let quiet_after = state.quiet_after;
    let mcp = StreamableHttpService::new(
        // **One handler per MCP session, and that is what makes the connection
        // binding a connection binding**: the factory runs per connect, so a
        // session bound here belongs to this client and evaporates with it.
        move || {
            let handler = Jojobot::new(
                memory.clone(),
                search.clone(),
                mailboxes.clone(),
                sessions.clone(),
                teachings.clone(),
                registry.clone(),
            )
            .on_clock(clock)
            .with_skills(skills)
            .quiet_after_unless_default(quiet_after);
            let handler = match listen_budget {
                Some(budget) => handler.listening_for(budget),
                None => handler,
            };
            Ok(match &requests {
                Some(requests) => handler.with_requests(requests.clone()),
                None => handler,
            })
        },
        LocalSessionManager::default().into(),
        server_config,
    );

    // The MCP transport is guarded; the metadata + health endpoints are public.
    //
    // **`route_layer`, never `layer`.** `layer` wraps the router's FALLBACK as
    // well as its routes, and a layered fallback wins on `merge`, so every
    // unmatched path in the whole app went through the bearer check and
    // answered 401. That answer names the wrong problem: a client probing a
    // path jojobot does not implement is told its credentials are wrong.
    // `route_layer` guards the routes and leaves the fallback alone, so an
    // unmounted path reports itself missing (rule 68).
    //
    // **The order of the three layers is the point, innermost first.** The body is
    // read to name the verb inside the bearer check, so a caller who is refused
    // costs us no read of what it sent. The line is written outside it, so a
    // refused request is logged too, and a request's line covers its whole life.
    let log = request_log::RequestLog::new(state.registry.clone()).cut_by(cut);
    let mut mcp_router =
        Router::new()
            .nest_service("/mcp", mcp)
            .route_layer(axum::middleware::from_fn_with_state(
                log.clone(),
                request_log::name_the_request,
            ));
    if state.validator.is_some() {
        mcp_router = mcp_router.route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            routes::require_bearer,
        ));
    }
    // **An app's token never opens the tool surface**, and this layer is mounted
    // whether or not a validator is: with auth off, nothing else looks at a bearer.
    mcp_router = mcp_router.route_layer(axum::middleware::from_fn(app_api::refuse_app_bearer));
    mcp_router = mcp_router.route_layer(axum::middleware::from_fn_with_state(
        log,
        request_log::log_request,
    ));

    // **The listing is mounted only when it is configured**, and it cannot be
    // configured without an issuer. That is the whole of the no-bypass rule:
    // there is no build, flag or environment in which these pages answer without
    // a login, because without a login they are not routes at all.
    let ui_router = state.ui.is_some().then(|| {
        Router::new()
            .route("/", get(ui::pages::index))
            .route(ui::org::PATH, get(ui::org::chart))
            .route(ui::search::PATH, get(ui::search::results))
            .route("/ui/kind/{kind}/", get(ui::home::kind_page))
            .route("/{*path}", get(ui::pages::node))
            .route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                ui::require_browser,
            ))
            // The login is outside the gate. A gate over the way in has nowhere
            // to send anybody.
            .route("/ui/login", get(ui::login::begin))
            .route("/ui/callback", get(ui::login::finish))
            // **The actions the UI has**, behind a session of its own gate:
            // the listing's gate answers a path that is no entity with "not
            // found" before it asks about a login, and an action is no entity.
            .merge(
                Router::new()
                    .route("/ui/mail/open", axum::routing::post(ui::mail::open))
                    .route(
                        "/ui/mail/processed",
                        axum::routing::post(ui::mail::processed),
                    )
                    .route_layer(axum::middleware::from_fn_with_state(
                        state.clone(),
                        ui::require_session,
                    )),
            )
    });

    let mut app = Router::new()
        .route("/healthz", get(routes::health))
        .route(
            "/.well-known/oauth-protected-resource",
            get(routes::protected_resource_metadata),
        )
        .merge(mcp_router);
    // **The app route exists only when an app is registered.** It sits outside
    // the UI's GET catch-all and behind its own gate.
    if !state.apps.is_empty() {
        app = app.merge(
            Router::new()
                .route(
                    app_api::DEPLOY_EVENTS_PATH,
                    axum::routing::post(app_api::deploy_event),
                )
                .route_layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    app_api::require_app,
                )),
        );
    }
    if let Some(ui_router) = ui_router {
        app = app.merge(ui_router);
    }
    app.with_state(state)
}
