//! jojobot — composition root. Loads configuration, builds the app, and serves
//! it. All wiring lives in the library so the integration tests exercise the
//! same router this binary does.

use std::sync::Arc;

use anyhow::Context;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::mailboxes::DoltMailboxes;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::sessions::DoltSessions;
use jojobot_adapters::dolt::teaching::DoltTeachings;
use jojobot_adapters::owners::MemoryOwners;
use jojobot_adapters::provisioned::Provisioned;
use jojobot_adapters::search::{IndexedMailboxes, IndexedMemory, IndexedSessions, Retrieval};
use jojobot_domain::mailbox::{Mailboxes, OwnerIndex};
use jojobot_domain::memory::Memory;
use jojobot_domain::memory::search::Search;
use jojobot_domain::session::Sessions;
use jojobot_domain::teaching::Teachings;
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
    // **…and what this build supplies over it.** The decorator resolves the
    // build's own half into every read and keeps a caller from storing it back,
    // and nothing above it has a word for either. It sits UNDER the index on
    // purpose: the index is a reader, and a reader that saw something no other
    // reader sees would be a second seam.
    // **Every row carries a badge before anything reads one.** A row gains one
    // when it is next rewritten, so a store where nothing is edited would keep
    // rows written before the column for ever. This reaches the rest, after the
    // migrations and before the index is built over them.
    //
    // **Not fatal.** The badge is nothing a caller can ask for yet, so a store
    // that could not be reached here is a store the boot already reports on —
    // and refusing to start over a column nothing reads would be worse than
    // saying so.
    let badging = DoltMemory::open(store.pool().clone());
    match badging.badge_the_unbadged().await {
        Ok(0) => {}
        Ok(given) => tracing::info!(given, "store: gave a badge to rows written before it"),
        Err(e) => tracing::warn!(
            error = %e,
            "BADGES NOT FILLED — rows written before the badge column still carry none. Nothing \
             was lost and nothing reads them yet; a restart once the store is reachable fills \
             them."
        ),
    }

    // **One set, read by both halves.** The layer above resolves what the
    // build supplies into an answer; the store below has to see the same set
    // when its guard decides whether a handle names anything, or a claim
    // pointing at a supplied record is refused as naming nothing.
    let supplied = jojobot_mcp::provisions();
    let memory: Arc<dyn Memory> = Arc::new(Provisioned::new(
        DoltMemory::open(store.pool().clone())
            .knowing(supplied.clone())
            .on_clock(config.clock),
        supplied,
    ));

    // **The kinds, before anything reads a handle.** Every kind this instance
    // holds is written and then read back, and what comes back is the set this
    // process parses handles against. A store that cannot be reached leaves
    // that set empty, and an empty set refuses every handle in its own words
    // rather than pretending the ten are there.
    match jojobot_mcp::seed::ensure_kinds(&memory).await {
        Ok(kinds) => tracing::info!(kinds, "loaded the kinds this instance holds"),
        Err(e) => tracing::error!(
            error = %e,
            "KINDS NOT LOADED — the store could not be reached at startup, so no handle can be \
             read and every write is refused. Nothing was written and nothing was lost; a restart \
             once the store is reachable puts it right."
        ),
    }

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
    let teachings: Arc<dyn Teachings> = Arc::new(DoltTeachings::open(store.pool().clone()));

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
    // **The third half: a bot's own runs.** Owner-scoped when a query asks, so
    // every bot's runs are indexed and each caller is served only its own.
    let runs = Arc::new(IndexedSessions::new(sessions.clone(), indexed.index()));
    match runs.rebuild().await {
        Ok(count) => tracing::info!(
            sessions = count,
            "search: sessions indexed from a full read"
        ),
        Err(e) => tracing::warn!(
            error = %e,
            "SESSION SEARCH DEGRADED — the boot read of the runs failed, so `search` starts with \
             no session hits. Any run this process refreshes is indexed as it goes; restart once \
             the store reads to get the rest back."
        ),
    }
    let search: Arc<dyn Search> = Arc::new(Retrieval::new(
        indexed.index(),
        vec![indexed.clone(), mailboxes.clone(), runs],
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

    let seed_memory: Arc<dyn Memory> = indexed.clone();

    // **There is never a jojobot with no bot.** The default identity arrives
    // with the software, before anything serves — which is what makes the
    // write gate shippable at all: an identity IS a bot, so a server with none
    // could never create the first one through its own surface.
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

    // **The vocabulary the software ships, declared on every boot.** Written
    // unconditionally: a shipped name is closed to callers, so the only
    // declaration this replaces is a previous build's, and that is how a key
    // this build added reaches an instance that is already running.
    match jojobot_mcp::seed::ensure_shipped_types(&seed_memory).await {
        Ok(types) => tracing::info!(types, "declared the types this build ships"),
        Err(e) => tracing::warn!(
            error = %e,
            "SHIPPED TYPES NOT DECLARED — the store could not be reached at startup, so a caller \
             asking for one of them is told the name is not declared. Nothing was written and \
             nothing was lost; a restart once the store is reachable puts it right."
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
        teachings,
        registry,
        ui,
        clock: config.clock,
    };

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
