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

/// How long the release-notes send and the push task are given to end, after the
/// drain, before the store stops.
const TASK_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // **`jojobot app-token`** prints a fresh app token and its hash and touches
    // no store and no configuration.
    if std::env::args().nth(1).as_deref() == Some("app-token") {
        print!("{}", jojobot::app_api::token_report());
        return Ok(());
    }
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
    let metrics = config
        .metrics_bind
        .map(|_| Arc::new(jojobot::metrics::Metrics::new()));
    let observation = metrics
        .as_ref()
        .map(|metrics| jojobot_adapters::observe::Observation::new(metrics.clone()))
        .unwrap_or_default();
    let booted = jojobot::wiring::boot_store_observed(
        &dir,
        jojobot::wiring::resolve_store_port(),
        config.clock,
        config.ntfy.is_some(),
        observation,
    )
    .await?;

    // **Pushes to the operator's phone**, only when the instance is set up for
    // them. An instance with no ntfy settings starts no task and says nothing.
    let runner: Option<Arc<jojobot::push::Runner>> = match &config.ntfy {
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
    let push = runner
        .clone()
        .map(|runner| runner as Arc<dyn jojobot_domain::push::Nudge>);

    let metadata_url = format!(
        "{}/.well-known/oauth-protected-resource",
        origin_of(&config.resource)
    );
    let (notes_memory, notes_mailboxes, notes_clock) = (
        booted.memory.clone(),
        booted.mailboxes.clone(),
        booted.clock,
    );
    let scraped = jojobot::metrics::Ports {
        sessions: booted.sessions.clone(),
        mailboxes: booted.mailboxes.clone(),
        memory: booted.memory.clone(),
    };
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
        apps: jojobot::app_api::Apps::new(config.apps.clone()),
        metrics: metrics.clone(),
    };
    // The pool every port above holds is only good for as long as this
    // process's own store is running. Kept alive for the rest of `main`
    // rather than named `_`: `Dolt` kills its child on drop, and a stop ends it
    // by hand after the drain, below.
    let mut store = booted.store;

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
    // **The metrics listener binds before the serving line**, so a process that
    // printed it is listening on both addresses, and one that cannot take the
    // metrics address stops here rather than serving without it.
    if let (Some(metrics), Some(address)) = (metrics, config.metrics_bind) {
        let scraping = tokio::net::TcpListener::bind(address)
            .await
            .with_context(|| format!("binding the metrics listener {address}"))?;
        tracing::info!("serving metrics http://{address}{}", jojobot::metrics::PATH);
        let router = jojobot::metrics::router(metrics, scraped);
        tokio::spawn(async move {
            if let Err(error) = axum::serve(scraping, router).await {
                tracing::warn!(%error, "the metrics listener stopped");
            }
        });
    }
    // **This line is a contract, not only a log.** It is printed after the
    // listener is bound and never before, so a caller that spawned this process
    // can read it and know THIS server is the one on that address. A port
    // answering says only that somebody is there.
    if !config.apps.is_empty() {
        tracing::info!(
            apps = config.apps.len(),
            revoked = config.apps.iter().filter(|a| a.revoked).count(),
            "apps registered: the app route is mounted"
        );
    }
    // **The address the listener holds, not the one configured.** A bind on port 0
    // takes its port from the operating system, and this line is where a caller
    // that asked for that learns which port it got.
    tracing::info!(
        "serving http://{}/mcp",
        listener.local_addr().context("reading the bound address")?
    );

    // **Each bot is told what this build changed, once the listener is open.**
    // Sending writes to every bot's box in turn, and a start that waited for it
    // would keep every caller out until it ended. A stop before it ends is safe:
    // the next start sends what is still unsent.
    let notes = tokio::spawn(async move {
        jojobot_mcp::release_notes::send_unsent_notes(
            &notes_memory,
            &notes_mailboxes,
            &notes_clock,
        )
        .await;
    });

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
    // **The tasks that write to the store end before it stops.** The release notes
    // and the push task are waited for, a short grace together, and abandoned at
    // their next await when they outlast it. The next start sends the notes and
    // pushes that are still unsent, so none is lost. **A release note is not sent
    // twice**: a write cut at the abandon may already have committed, and the pass
    // recovers what was sent from the mail history. **A push is delivered at least
    // once**: it is delivered before it is marked sent, so a cut between the two
    // delivers it again at the next start.
    let (notes_ended, push_ended) = tokio::join!(
        jojobot::background::finish_or_abandon(notes, TASK_GRACE),
        async {
            match &runner {
                Some(runner) => runner.stop_and_wait(TASK_GRACE).await,
                None => None,
            }
        }
    );
    tracing::info!(
        ?notes_ended,
        ?push_ended,
        "background tasks ended before the store stops"
    );
    // **The store outlives the drain and stops here, asked and then killed.** The
    // unit signals this process alone, and the store has a process group of its
    // own, so nothing stops it before the calls above have finished with it.
    store.stop().await;
    tracing::info!("shut down: no call in flight, the store is stopped");

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
