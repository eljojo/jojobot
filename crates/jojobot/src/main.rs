//! jojobot — composition root. Loads configuration, builds the app, and serves
//! it. All wiring lives in the library so the integration tests exercise the
//! same router this binary does.

use std::sync::Arc;

use anyhow::Context;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::mailboxes::DoltMailboxes;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::sessions::DoltSessions;
use jojobot_adapters::owners::MemoryOwners;
use jojobot_adapters::search::{IndexedMailboxes, IndexedMemory, Retrieval};
use jojobot_domain::mailbox::{Mailboxes, OwnerIndex};
use jojobot_domain::memory::Memory;
use jojobot_domain::memory::search::Search;
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

    // **The SQL store jojobot runs itself**, brought up and migrated before
    // anything else is wired.
    //
    // Its directory comes from the service manager, which owns the real path
    // and hands over a stable one — so nothing here decides where state lives.
    //
    // ⚠️ **A failure here refuses the boot.** Mail and sessions are served from
    // this store, so a server that came up without it would have no board to
    // read and no run to resume, and would report that emptiness as the truth.
    // Refusing is the same fact said where somebody can act on it. A migration
    // that failed is not recorded as applied, so a restart resumes where it
    // stopped rather than skipping it.
    let dir = store_dir_from_env().context(
        "STORE MISSING — no state directory, so there is no store to serve mail and sessions \
         from. Set STATE_DIRECTORY (the service manager does).",
    )?;
    let (store, applied) = Dolt::ready(&dir, store_port_from_env())
        .await
        .with_context(|| {
            format!(
                "STORE UNAVAILABLE — the store at {} did not come up, or its schema did not apply",
                dir.display()
            )
        })?;
    if applied.is_empty() {
        tracing::info!(dir = %dir.display(), "store: up, schema already current");
    } else {
        tracing::info!(dir = %dir.display(), applied = ?applied, "store: up, schema moved");
    }

    // **Memory is served from the store**, over the same pool as mail and
    // sessions. There is no second store to reach and nothing to carry: the
    // records moved, and the documents they came from are a person's copy now.
    let memory: Arc<dyn Memory> = Arc::new(DoltMemory::open(store.pool().clone()));

    // The search projection sits in FRONT of the store, so every write through
    // the port keeps the index current. Boot is a plain full re-scan — and a
    // failed scan is not fatal: the store is the truth, and refusing to start
    // because a projection couldn't be built is worse than a thin `search`. It
    // says so loudly instead.
    let indexed = Arc::new(IndexedMemory::new(memory).context("opening the search index")?);
    match indexed.rebuild().await {
        Ok(docs) => tracing::info!(docs, "search: index built from a full scan"),
        Err(e) => tracing::warn!(
            error = %e,
            "SEARCH INDEX EMPTY — the boot scan failed, so `search` sees only what this process \
             writes from here on. The memory verbs are unaffected; restart once the store is \
             reachable to get a full index."
        ),
    }

    // **The ports mail and sessions are served from.** Rows in the SQL store,
    // both over the one pool.
    //
    // The owner index is the production one and it reads Memory: a box is
    // created FOR somebody, the entity world is somewhere else now, and "does
    // this handle resolve" is the whole of what crosses. It reads through the
    // projection so a bot created this session is an owner this session.
    let owners: Arc<dyn OwnerIndex> = Arc::new(MemoryOwners::new(indexed.clone()));
    let mail_store: Arc<dyn Mailboxes> =
        Arc::new(DoltMailboxes::open(store.pool().clone(), owners));
    let sessions: Arc<dyn Sessions> = Arc::new(DoltSessions::open(store.pool().clone()));

    // Mail goes into the SAME index — one front door, one ranked list — so the
    // mailbox store gets the same decorator treatment Memory's does: every verb
    // that changes a message re-indexes it, and boot loads the board once.
    //
    // A failed board read is not fatal, exactly as a failed doc scan is not: the
    // store is the truth, the memory half is untouched, and `search` reports the
    // gap in every answer rather than passing it off as "nothing matched".
    // Refusing to start over a projection is worse than a thin one that admits
    // what it is.
    let mailboxes = Arc::new(IndexedMailboxes::new(mail_store, indexed.index()));
    match mailboxes.rebuild().await {
        Ok(messages) => tracing::info!(messages, "search: mail indexed from a full board read"),
        Err(e) => tracing::warn!(
            error = %e,
            "MAIL SEARCH DEGRADED — the boot board read failed, so `search` starts with no \
             messages at all and says so (mail.searched: false). It does NOT stay that way: any \
             message this process posts or delivers is indexed as it goes, and from the first \
             one `search` reports partial coverage — real hits, with anything older than this \
             process missing. The mailbox verbs are unaffected; restart once the board reads to \
             get the whole store back."
        ),
    }
    // **The retrieval port holds both halves, because an answer spans both.**
    // Each half refreshes itself from its own store before a search answers, so
    // a record removed outside jojobot — the only way one leaves at all — stops
    // being served. Neither decorator can reach the other's store, which is why
    // the port is not on either of them.
    let search: Arc<dyn Search> = Arc::new(Retrieval::new(
        indexed.index(),
        vec![indexed.clone(), mailboxes.clone()],
    ));
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

    // **There is never a jojobot with no bot.** The default identity arrives
    // with the software, before anything serves — which is what makes the
    // write gate shippable at all: an identity IS a bot, so a server with none
    // could never create the first one through its own surface.
    let seed_memory: Arc<dyn Memory> = indexed.clone();
    match jojobot_mcp::seed::ensure_default_identity(&seed_memory, &mailboxes).await {
        jojobot_mcp::seed::Seeded::Created => {
            tracing::info!(
                bot = jojobot_mcp::seed::DEFAULT_BOT,
                "seeded the default identity"
            )
        }
        jojobot_mcp::seed::Seeded::AlreadyThere => {}
        jojobot_mcp::seed::Seeded::Unreachable(why) => tracing::warn!(
            error = %why,
            "DEFAULT IDENTITY NOT SEEDED — the store could not be reached at startup, so this \
             instance may have no bot to boot as. Nothing was written and nothing was lost; a \
             restart once the store is reachable puts it right."
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
        search,
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

/// Where the SQL store keeps its data — **the service manager's answer, not
/// this binary's**. With a dynamic user the real path is systemd's to choose
/// and `STATE_DIRECTORY` is the stable name it hands over; the store sits in
/// `db` beneath it, so one directory holds everything jojobot owns.
fn store_dir_from_env() -> Option<std::path::PathBuf> {
    std::env::var("STATE_DIRECTORY")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| std::path::PathBuf::from(s).join("db"))
}

/// The loopback port the store serves on. Fixed by default so an operator
/// debugging a live host knows where to look, and overridable because a
/// developer may already have something on it.
fn store_port_from_env() -> u16 {
    std::env::var("JOJOBOT_STORE_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3307)
}

fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}
