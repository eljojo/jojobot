//! jojobot — composition root. Loads configuration, builds the app, and serves
//! it. All wiring lives in the library so the integration tests exercise the
//! same router this binary does.

use std::future::IntoFuture;
use std::sync::Arc;

use anyhow::Context;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::util::SubscriberInitExt;

use jojobot::auth::Validator;
use jojobot::config::{Config, origin_of};
use jojobot::{AppState, build_app};

/// **How long what the cut left open is given to end**, after the drain window.
/// The process ends either way; this only lets a connection that closes at once
/// do so cleanly.
const CUT_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

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
    let booted = jojobot::wiring::boot_store_pushing(
        &dir,
        jojobot::wiring::resolve_store_port(),
        config.clock,
        config.ntfy.is_some(),
    )
    .await?;

    // **Pushes to the operator's phone**, only when the instance is set up for
    // them. An instance with no ntfy settings starts no task and says nothing.
    let push: Option<Arc<dyn jojobot_domain::push::Nudge>> = match &config.ntfy {
        None => None,
        Some(ntfy_config) => {
            tracing::info!(
                server = %ntfy_config.url,
                "mail posted to the operator is pushed to their phone through ntfy"
            );
            let notifier = jojobot::push::Ntfy::new(ntfy_config.clone())
                .context("building the ntfy client")?;
            Some(Arc::new(jojobot::push::Runner::start(
                booted.outbox.clone(),
                booted.mailboxes.clone(),
                Arc::new(notifier),
                jojobot_domain::push::Policy::default(),
            )))
        }
    };

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
        requests: Some(booted.requests),
        registry: booted.registry,
        ui,
        clock: booted.clock,
        push,
        quiet_after: None,
    };
    // The pool every port above holds is only good for as long as this
    // process's own store is running. Kept alive for the rest of `main`
    // rather than named `_`: `Dolt` kills its child on drop.
    let _store = booted.store;

    let ct = CancellationToken::new();
    let app = build_app(state, ct.child_token());

    // **Both stop signals are listened for before the serving line is printed.**
    // A service manager stops a unit with SIGTERM and a terminal with SIGINT; a
    // signal that arrived before its listener existed would take the default
    // action and end the process with no shutdown at all, and the serving line is
    // the moment a caller starts signalling.
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .context("listening for SIGINT")?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .context("listening for SIGTERM")?;

    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("binding {}", config.bind))?;
    // **This line is a contract, not only a log.** It is printed after the
    // listener is bound and never before, so a caller that spawned this process
    // can read it and know THIS server is the one on that address. A port
    // answering says only that somebody is there.
    tracing::info!("serving http://{}/mcp", config.bind);

    // **A stop drains before it cuts.** The signal closes the listener, so no new
    // call is taken, and the calls already running get up to
    // [`jojobot::SHUTDOWN_DRAIN`] to finish. Only then is the token cancelled,
    // which ends every session still open: a stream a client holds open never
    // finishes by itself, and without the window it would hold the stop up until
    // the service manager killed the process. Cancelling at the signal instead
    // cut the call in flight and returned it empty.
    let (stopping, begun) = tokio::sync::oneshot::channel::<()>();
    let serving = axum::serve(listener, app).with_graceful_shutdown(async move {
        let signal = tokio::select! {
            _ = interrupt.recv() => "SIGINT",
            _ = terminate.recv() => "SIGTERM",
        };
        tracing::info!(
            signal,
            drain_seconds = jojobot::SHUTDOWN_DRAIN.as_secs(),
            "shutting down: no new calls are taken, and the calls in flight get the drain \
             window to finish"
        );
        let _ = stopping.send(());
    });
    let serving = serving.into_future();
    tokio::pin!(serving);
    let window_ends = async {
        // A sender dropped without a signal means serving ended on its own.
        if begun.await.is_err() {
            std::future::pending::<()>().await;
        }
        tokio::time::sleep(jojobot::SHUTDOWN_DRAIN).await;
    };
    tokio::select! {
        served = &mut serving => served.context("server error")?,
        () = window_ends => {
            tracing::warn!(
                "the drain window ended with calls still open: cutting them"
            );
            ct.cancel();
            // The cut ends the sessions; whatever else is open is given a short
            // grace and then left to the process ending.
            let _ = tokio::time::timeout(CUT_GRACE, &mut serving).await;
        }
    }
    ct.cancel();
    tracing::info!("shut down: no call in flight, the store stops with this process");

    Ok(())
}

fn init_tracing() {
    // **An empty `RUST_LOG` is no filter, and no filter logs at info.** A filter
    // parsed from an empty string has no directives and drops every event, the
    // serving line included. A build sandbox exports exactly that, and nobody
    // sets an empty filter on purpose. A non-empty value is a real filter.
    let set = std::env::var("RUST_LOG").is_ok_and(|raw| !raw.trim().is_empty());
    let filter = if set {
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())
    } else {
        "info".into()
    };
    let ansi = jojobot::logging::colour_for(&std::io::stdout());
    jojobot::logging::logging(std::io::stdout, ansi, filter).init();
}
