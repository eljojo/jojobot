//! jojobot — composition root. Loads configuration, builds the app, and serves
//! it. All wiring lives in the library so the integration tests exercise the
//! same router this binary does.

use std::sync::Arc;

use anyhow::Context;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use jojobot::auth::Validator;
use jojobot::config::{Config, origin_of};
use jojobot::{AppState, build_app};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let config = Config::from_env().context("loading configuration")?;
    tracing::info!(
        bind = %config.bind,
        resource = %config.resource,
        auth = config.auth.is_some(),
        "starting jojobot"
    );

    // **An instance acting out a day says so before it serves.** A stated day
    // is invisible from the outside otherwise: the server looks healthy and
    // every date it fills in is fiction. It is announced on the surface too —
    // `start_here` and `ping` both carry it — and this is the half an operator
    // reading the logs of a deployment sees.
    if let Some(day) = config.clock.stated() {
        tracing::warn!(
            %day,
            "ACTING OUT A DAY — JOJOBOT_TODAY is set, so this server is NOT on the real clock. \
             Every date it fills in, every due read, every staleness sweep and every moment it \
             stamps a record with land on this day. Unset JOJOBOT_TODAY to run on the real clock."
        );
    }

    let http = reqwest::Client::builder()
        // Bound the JWKS/discovery fetch so a hung issuer can't stall startup.
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .context("building HTTP client")?;

    let (validator, issuer) = match &config.auth {
        Some(auth_cfg) => {
            let validator = Validator::discover(auth_cfg, &http)
                .await
                .context("building the token validator from the issuer JWKS")?;
            let allowlist = if auth_cfg.allowed_subjects.is_empty() {
                "open (any authenticated user)".to_string()
            } else {
                format!("{} subject(s)", auth_cfg.allowed_subjects.len())
            };
            tracing::info!(
                issuer = %auth_cfg.issuer,
                audience = %auth_cfg.audience,
                %allowlist,
                "resource-server auth enabled"
            );
            (
                Some(std::sync::Arc::new(validator)),
                Some(auth_cfg.issuer.clone()),
            )
        }
        None => {
            tracing::warn!(
                "AUTH DISABLED — JOJOBOT_ISSUER is unset, so /mcp is open. Development use only."
            );
            (None, None)
        }
    };

    // **The browser listing, when it is configured.** It is the same issuer and
    // the same allowlist as `/mcp`; what differs is that a browser cannot carry
    // a bearer token, so jojobot obtains one for it. Configuration refuses a UI
    // without an issuer, so this arm cannot be reached with auth disabled.
    let ui = match (&config.auth, &config.ui) {
        (Some(auth_cfg), Some(ui_cfg)) => {
            let endpoints = jojobot::auth::discover_endpoints(&auth_cfg.issuer, &http)
                .await
                .context("reading the issuer's discovery document for the browser login")?;
            let id_tokens = Validator::discover_for_audience(auth_cfg, &ui_cfg.client_id, &http)
                .await
                .context("building the ID-token validator from the issuer JWKS")?;
            tracing::info!(
                client_id = %ui_cfg.client_id,
                redirect_uri = %ui_cfg.redirect_uri(),
                "browser listing enabled"
            );
            Some(Arc::new(jojobot::ui::Ui::new(
                ui_cfg,
                endpoints,
                id_tokens,
                http.clone(),
            )))
        }
        _ => None,
    };

    // **The store, and everything `AppState` needs from it.** The directory
    // comes from the service manager, which owns the real path and hands over
    // a stable one — so nothing here decides where state lives. This is the
    // sequence a restart runs before it can serve anything; see
    // `jojobot::wiring::boot_store`'s own doc for what is fatal in it and why.
    let dir = jojobot::wiring::resolve_store_dir()?;
    let booted =
        jojobot::wiring::boot_store(&dir, jojobot::wiring::resolve_store_port(), config.clock)
            .await?;

    let metadata_url = format!(
        "{}/.well-known/oauth-protected-resource",
        origin_of(&config.resource)
    );
    let state = AppState {
        resource: config.resource.clone(),
        issuer,
        validator,
        metadata_url,
        memory: booted.memory,
        search: booted.search,
        mailboxes: booted.mailboxes,
        sessions: booted.sessions,
        teachings: booted.teachings,
        registry: booted.registry,
        ui,
        clock: config.clock,
    };
    // The pool every port above holds is only good for as long as this
    // process's own store is running. Kept alive for the rest of `main`
    // rather than named `_`: `Dolt` kills its child on drop.
    let _store = booted.store;

    let ct = CancellationToken::new();
    let app = build_app(state, ct.child_token());

    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("binding {}", config.bind))?;
    // **This line is a contract, not only a log.** It is printed after the
    // listener is bound and never before, so a caller that spawned this process
    // can read it and know THIS server is the one on that address. A port
    // answering says only that somebody is there.
    tracing::info!("serving http://{}/mcp", config.bind);

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            ct.cancel();
        })
        .await
        .context("server error")?;

    Ok(())
}

fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}
