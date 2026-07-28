//! jojobot — composition root. Loads configuration, builds the app, and serves
//! it. All wiring lives in the library so the integration tests exercise the
//! same router this binary does.

use std::sync::Arc;

use anyhow::Context;
use jojobot_adapters::outline::{OutlineConfig, OutlineStore, Secret};
use jojobot_adapters::search::{IndexedMailboxes, IndexedMemory};
use jojobot_domain::mailbox::Mailboxes;
use jojobot_domain::memory::Memory;
use jojobot_domain::session::Sessions;
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

    // The Memory port. Always the real Outline adapter — no toy store ships. It
    // discovers/creates its own `jojobot` collection by name; the only config is
    // credentials. Unset credentials yield an unconfigured store: the server
    // still boots and serves `ping`, but `capture`/`recall` refuse loudly until
    // Outline is wired (see the fail-soft rationale in the handoff/report).
    let outline = match outline_from_env() {
        Some(cfg) => {
            tracing::info!(base_url = %cfg.base_url, "memory: Outline store wired");
            OutlineStore::new(http.clone(), cfg)
        }
        None => {
            tracing::warn!(
                "MEMORY DISABLED — set JOJOBOT_OUTLINE_URL and JOJOBOT_OUTLINE_TOKEN to enable \
                 capture/recall. Serving ping only; memory verbs return a NotConfigured error."
            );
            OutlineStore::unconfigured()
        }
    };

    // **The Sessions port, built FROM the memory store rather than beside it.**
    // A bot's sessions are a page under the bot's own page, in the same
    // collection as every entity — so the two stores write different documents
    // in one place, and building this one from that one is what makes them
    // share a write lock instead of each holding a lock that excludes nobody.
    //
    // It is not indexed: a sessions page declares itself jojobot's own
    // machinery, and the boot scan skips those.
    let sessions: Arc<dyn Sessions> = Arc::new(outline.sessions());
    // Built here too, off the same handle and therefore the same lock, before
    // the store is moved behind the search projection.
    let mailbox_store: Arc<dyn Mailboxes> = Arc::new(outline.mailboxes());
    let store: Arc<dyn Memory> = Arc::new(outline);

    // The search projection sits in FRONT of the store, so every write through
    // the port keeps the index current. Boot is a plain full re-scan — and a
    // failed scan is not fatal: the store is the truth, and refusing to start
    // because a projection couldn't be built is worse than a thin `search`. It
    // says so loudly instead.
    let indexed = Arc::new(IndexedMemory::new(store).context("opening the search index")?);
    match indexed.rebuild().await {
        Ok(docs) => tracing::info!(docs, "search: index built from a full scan"),
        Err(e) => tracing::warn!(
            error = %e,
            "SEARCH INDEX EMPTY — the boot scan failed, so `search` sees only what this process \
             writes from here on. The memory verbs are unaffected; restart once the store is \
             reachable to get a full index."
        ),
    }

    // Mail goes into the SAME index — one front door, one ranked list — so the
    // mailbox store gets the same decorator treatment Memory's does: every verb
    // that changes a message re-indexes it, and boot loads the board once.
    //
    // A failed board read is not fatal, exactly as a failed doc scan is not: the
    // store is the truth, the memory half is untouched, and `search` reports the
    // gap in every answer rather than passing it off as "nothing matched".
    // Refusing to start over a projection is worse than a thin one that admits
    // what it is.
    let mailboxes = Arc::new(IndexedMailboxes::new(mailbox_store, indexed.index()));
    match mailboxes.rebuild().await {
        Ok(messages) => tracing::info!(messages, "search: mail indexed from a full board read"),
        Err(e) => tracing::warn!(
            error = %e,
            "MAIL SEARCH DEGRADED — the boot board read failed, so `search` starts with no \
             messages at all and says so (mail.searched: false). It does NOT stay that way: any \
             message this process posts or delivers is indexed as it goes, and from the first \
             one `search` reports partial coverage — real hits, with anything older than this \
             process missing. The mailbox verbs are unaffected; restart once Outline is \
             reachable to get the whole store back."
        ),
    }
    let mailboxes: Arc<dyn Mailboxes> = mailboxes;

    // **The handle registry, filled from the board before anything is served.**
    // Eagerly rather than on first miss: a lazy rebuild would hand the first
    // caller after a restart a different answer from the second, and that is the
    // class of difference nobody can reproduce.
    //
    // A failed read is not fatal, for the same reason a failed index scan is
    // not. What it costs is stated rather than hidden: handles minted before the
    // restart come back "that session is gone", the work on the board is
    // untouched, and booting again offers it back by what it was working on.
    let registry = Arc::new(jojobot_mcp::sid::SessionRegistry::new());
    match sessions.all_sessions().await {
        Ok(board) => {
            let recovered = registry.rebuild_from(&board);
            tracing::info!(
                recovered,
                cards = board.len(),
                "sessions: handle registry rebuilt from the board"
            );
        }
        Err(e) => tracing::warn!(
            error = %e,
            "SESSION HANDLES NOT RECOVERED — the board could not be read at startup, so every \
             handle issued before this restart now answers 'that session is gone'. Nothing on the \
             board was lost: booting an identity still offers its runs back by what they were \
             working on. Restart once the store is reachable to recover the handles."
        ),
    }

    let metadata_url = format!(
        "{}/.well-known/oauth-protected-resource",
        origin_of(&config.resource)
    );
    let state = AppState {
        resource: config.resource.clone(),
        issuer,
        validator,
        metadata_url,
        memory: indexed.clone(),
        search: indexed,
        mailboxes,
        sessions,
        registry,
    };

    let ct = CancellationToken::new();
    let app = build_app(state, ct.child_token());

    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("binding {}", config.bind))?;
    tracing::info!("listening on http://{}/mcp", config.bind);

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            ct.cancel();
        })
        .await
        .context("server error")?;

    Ok(())
}

/// Read the Outline store's **credentials** from the environment — the only
/// config there is; the collection and docs are discovered by convention. Both
/// must be set; either missing → `None` (memory disabled). The adapter never
/// reads env itself — the operator sets it here, at the composition root.
fn outline_from_env() -> Option<OutlineConfig> {
    let nonempty = |k: &str| std::env::var(k).ok().filter(|s| !s.is_empty());
    Some(OutlineConfig {
        base_url: nonempty("JOJOBOT_OUTLINE_URL")?,
        token: Secret::new(nonempty("JOJOBOT_OUTLINE_TOKEN")?),
    })
}

fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}
