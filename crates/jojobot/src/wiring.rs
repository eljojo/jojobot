//! **The decorator assembly every `AppState` is built from.**
//!
//! Both the binary and the story harness need the same stack of decorators
//! wrapping the bare stores — the search index, and the mention resolvers
//! rule 260 depends on. Splitting that assembly by hand between two files is
//! how it drifted once already: the harness built its mailboxes and sessions
//! straight off the bare stores, with no mention decorator between them and
//! the index, for as long as the decorator has existed. Calling the same
//! functions is what makes that unrepresentable rather than merely
//! undesirable — there is exactly one place either caller could drop a
//! layer, and dropping it there breaks both at once.
//!
//! **Two stages, not one call, and here is why.** The binary has boot work
//! that has to land BETWEEN wrapping memory and wrapping mail and sessions:
//! a full index rebuild, and the one-time mention-text migration, which
//! reads `indexed.list_entities()` and has to run against the BARE mail and
//! session stores before either gets its own `Mentioning`. The story harness
//! needs neither — a fresh in-memory store has nothing to migrate and
//! rebuilds only on a restart. Splitting the assembly at that seam keeps the
//! seam explicit and keeps both stages equally shared: a caller can run its
//! own boot work between them, but cannot build `AppState`'s ports without
//! going through both, so neither stage's decorators can be dropped by one
//! caller and kept by the other.
//!
//! **What this does NOT unify.** Wrapping a bare store in `Provisioned`
//! needs `.knowing(supplied)`, a method each concrete store type implements
//! on itself rather than through `Memory` — `DoltMemory::knowing` and
//! `InMemoryMemory::knowing` are two different inherent methods with no
//! shared trait to call through generically, and unifying that would mean
//! adding it to the `Memory` trait for every adapter, a larger and separate
//! change. A caller does that step before calling [`open_provisioned`].
//! Everything from there on is composition over trait objects, which is what
//! this file shares.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::mailboxes::DoltMailboxes;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::sessions::DoltSessions;
use jojobot_adapters::dolt::teaching::DoltTeachings;
use jojobot_adapters::fold::Folded;
use jojobot_adapters::owners::MemoryOwners;
use jojobot_adapters::provisioned::Provisioned;
use jojobot_adapters::search::{IndexedMailboxes, IndexedMemory, IndexedSessions, Retrieval};
use jojobot_domain::clock::Clock;
use jojobot_domain::mailbox::mention as mailbox_mention;
use jojobot_domain::mailbox::{Mailboxes, OwnerIndex};
use jojobot_domain::memory::Memory;
use jojobot_domain::memory::mention;
use jojobot_domain::memory::owned::Provisions;
use jojobot_domain::memory::search::Search;
use jojobot_domain::session::Sessions;
use jojobot_domain::session::mention as session_mention;
use jojobot_domain::teaching::Teachings;

/// **Whether this process has already provisioned a store.** Set once, by
/// the first call to [`open_provisioned`] to reach it.
///
/// **Process-wide on purpose, and narrowly so.** The kind set
/// [`open_provisioned`] seeds is itself process-wide (see
/// `memory::kinds::load`'s own doc), so a second store provisioned in the
/// same process would already be reading and writing against the first
/// store's vocabulary with nothing anywhere saying so. This does not make the
/// kind set per-store, and it does not touch `kinds::seed`, `reload` or
/// `load` themselves — every caller that seeds a store directly, including
/// the real-store contract suite, is unaffected. It only refuses a second
/// call to THIS function in one process.
static PROVISIONED: std::sync::OnceLock<()> = std::sync::OnceLock::new();

/// **Stage zero: seed the kinds this build ships, then guard, then wrap in
/// `Provisioned`.**
///
/// **The order is enforced here rather than left to the caller.**
/// `guard_supplied_records` reads every stored entity, and reading one means
/// parsing its handle's kind — which a process that has not yet loaded the
/// kind set cannot do (`kinds::resolve` answers `SetNeverLoaded`). So the
/// kinds are seeded first, against the bare store, before anything reads a
/// row. Getting this backwards once took the whole server down at boot: the
/// guard's `list_entities` hit the first stored row before any kind was
/// loaded, and every read since the process began refused with "the kind set
/// was never loaded".
///
/// `bare` must already carry `.knowing(supplied)` — see the module doc for
/// why that step stays with the caller.
///
/// **Refuses a second call in the same process** (see [`PROVISIONED`]),
/// before touching the store it was handed. A process serves one store.
pub async fn open_provisioned<M: Memory + Clone + 'static>(
    bare: M,
    supplied: Provisions,
) -> anyhow::Result<Arc<dyn Memory>> {
    if PROVISIONED.set(()).is_err() {
        anyhow::bail!(
            "a store is already provisioned in this process — a process serves one store, and \
             the kind set the first one seeded is process-wide (see memory::kinds::load); this \
             call was refused before touching the store it was handed"
        );
    }
    match jojobot_domain::memory::kinds::seed(&bare).await {
        Ok(kinds) => tracing::info!(kinds, "loaded the kinds this instance holds"),
        Err(e) => tracing::error!(
            error = %e,
            "KINDS NOT LOADED — the store could not be reached at startup, so no handle can be \
             read. A write or a read that has to resolve one — a subject, a mention written into \
             free text, a reference-typed field — is refused rather than guessed at; a write that \
             names no handle at all is not stopped by this alone. A restart once the store is \
             reachable puts it right."
        ),
    }
    // **Read against the bare store, before it is wrapped in `Provisioned`.**
    // A whole-record provision at an address a real row already occupies
    // breaks `Supplies::Record`'s own contract — a record the store holds
    // NOTHING of — and every existence check that reads what the build
    // supplies would see the address as already provisioned, never learning
    // the real row needs creating or re-creating. Refuse to start rather
    // than serve on a misconfiguration that silent.
    jojobot_domain::memory::owned::guard_supplied_records(&bare, &supplied)
        .await
        .context("a shipped provision collides with a stored row")?;
    Ok(Arc::new(Provisioned::new(bare, supplied)))
}

/// **Stage one: wrap memory, and open the fold and the index over it.**
///
/// `resolved` is memory already carrying `.knowing(supplied)` and
/// `Provisioned::new` — see the module doc for why that step is not here
/// too. Both concrete wrappers come back, not trait objects, because a
/// caller runs its own boot work against each of them (the fold's own
/// `rebuild`, the index's full rebuild, or reading `list_entities` for the
/// mention-text migration) before stage two.
///
/// **The fold sits closest to the store, under `Mentioning` and the
/// index.** Both of those forward `fields` straight through (decision log
/// 292's mechanism is this one wrapper, not a copy in each), so wherever a
/// caller holds the outermost `Memory` — the index — a `fields` read
/// reaches the fold before it reaches the store.
pub fn assemble_memory(
    resolved: Arc<dyn Memory>,
) -> anyhow::Result<(Arc<Folded>, Arc<IndexedMemory>)> {
    let folded = Arc::new(Folded::new(resolved));
    // **Mentions resolve above the fold and below the index.** A mention in
    // a claim has to see the full entity list, and what search holds must
    // be what a reader sees.
    let memory: Arc<dyn Memory> = Arc::new(mention::Mentioning::new(folded.clone()));
    Ok((
        folded,
        Arc::new(IndexedMemory::new(memory).context("opening the search index")?),
    ))
}

/// The three ports left, wrapped in the decorators every jojobot serves
/// through, once memory is already open.
pub struct Wired {
    /// Mention-resolving, then indexed — what `AppState.mailboxes` is.
    /// Concrete rather than `Arc<dyn Mailboxes>`, so a caller can call its
    /// own `rebuild` on it; it coerces to the trait object `AppState` wants
    /// at the point it is assigned.
    pub mailboxes: Arc<IndexedMailboxes>,
    /// Mention-resolving — what `AppState.sessions` is directly. Never
    /// further indexed: see `sessions_indexed`.
    pub sessions: Arc<dyn Sessions>,
    /// The same sessions, indexed — concrete, for a caller's own `rebuild`.
    /// `AppState.sessions` is `sessions` above, not this: a bot's own runs
    /// are indexed for `search` alone, the same asymmetry `main.rs` always
    /// had.
    pub sessions_indexed: Arc<IndexedSessions>,
    /// The one port over all three halves, refreshed from its own store on
    /// every answer.
    pub search: Arc<dyn Search>,
}

/// **Stage two: wrap mail and sessions, and open `search` over all three.**
///
/// `bare_mail` and `bare_sessions` are otherwise unwrapped — no index, no
/// mention resolution — the shape a caller's own boot-time migration, if it
/// runs one, needs them in.
pub fn assemble_ports(
    indexed: Arc<IndexedMemory>,
    bare_mail: Arc<dyn Mailboxes>,
    bare_sessions: Arc<dyn Sessions>,
) -> Wired {
    // **Mentions resolve above the raw store and below the index here too**
    // — the same placement as memory's own `Mentioning`.
    let mail_mentioned: Arc<dyn Mailboxes> =
        Arc::new(mailbox_mention::Mentioning::new(bare_mail, indexed.clone()));
    let sessions_mentioned: Arc<dyn Sessions> = Arc::new(session_mention::Mentioning::new(
        bare_sessions,
        indexed.clone(),
    ));

    // Mail goes through the search index too: every verb that changes a
    // message re-indexes it, and boot loads the board once (the caller's
    // job — see each call site's own rebuild).
    let mailboxes = Arc::new(IndexedMailboxes::new(mail_mentioned, indexed.index()));
    // **A bot's own runs, indexed for search alone.** `sessions` above stays
    // the mention-resolving port directly, and this wrapped copy is only a
    // source `Retrieval` reads from — and a caller's own `rebuild` target.
    let sessions_indexed = Arc::new(IndexedSessions::new(
        sessions_mentioned.clone(),
        indexed.index(),
    ));

    // **The retrieval port over all three halves.** Each refreshes itself
    // from its own store before search answers, so a record removed outside
    // jojobot stops being served.
    let search: Arc<dyn Search> = Arc::new(Retrieval::new(
        indexed.index(),
        vec![indexed, mailboxes.clone(), sessions_indexed.clone()],
    ));

    Wired {
        mailboxes,
        sessions: sessions_mentioned,
        sessions_indexed,
        search,
    }
}

/// Where the SQL store keeps its data — **the service manager's answer, not
/// this binary's**. With a dynamic user the real path is systemd's to choose
/// and `STATE_DIRECTORY` is the stable name it hands over; the store sits in
/// `db` beneath it, so one directory holds everything jojobot owns.
pub fn resolve_store_dir() -> anyhow::Result<PathBuf> {
    std::env::var("STATE_DIRECTORY")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| PathBuf::from(s).join("db"))
        .context(
            "STORE MISSING — no state directory, so there is no store to serve mail and sessions \
             from. Set STATE_DIRECTORY (the service manager does).",
        )
}

/// The loopback port the store serves on. Fixed by default so an operator
/// debugging a live host knows where to look, and overridable because a
/// developer may already have something on it.
pub fn resolve_store_port() -> u16 {
    std::env::var("JOJOBOT_STORE_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3307)
}

/// Everything `AppState` needs once the store is open and every boot step
/// has run against it — the store handle included, because the connection
/// pool everything above holds is only good for as long as the process that
/// spawned the server is still running one. Dropping this drops the server
/// with it (`Dolt` kills its child on drop), so a caller holds it for the
/// life of whatever serves through the ports beside it.
pub struct Booted {
    /// The store process and its pool. Kept alive rather than read from
    /// again — everything else here already holds the pool it needs.
    pub store: Dolt,
    /// What `AppState.memory` is.
    pub memory: Arc<dyn Memory>,
    /// What `AppState.search` is.
    pub search: Arc<dyn Search>,
    /// What `AppState.mailboxes` is.
    pub mailboxes: Arc<dyn Mailboxes>,
    /// What `AppState.sessions` is.
    pub sessions: Arc<dyn Sessions>,
    /// What `AppState.teachings` is.
    pub teachings: Arc<dyn Teachings>,
    /// What `AppState.registry` is.
    pub registry: Arc<jojobot_mcp::sid::SessionRegistry>,
}

/// **Bring the store up, migrate it, and assemble everything `AppState`
/// needs from it** — the sequence a restart runs before it can serve
/// anything, moved here so a test can run it against a real store rather
/// than only a human watching the binary boot.
///
/// **A failure to bring the store up refuses the boot.** Mail and sessions
/// are served from this store, so a server that came up without it would
/// have no board to read and no run to resume, and would report that
/// emptiness as the truth. Refusing is the same fact said where somebody can
/// act on it. A migration that failed is not recorded as applied, so a
/// restart resumes where it stopped rather than skipping it.
///
/// **Everything after the store answers is best-effort.** Each step below
/// says, at its own call site, why a failure there does not refuse the boot
/// — the store is the truth, and every one of these is a cache, an index or
/// a convenience over it. Refusing to start over one of them failing would
/// be a worse outcome than serving on what the store already has.
pub async fn boot_store(dir: &Path, port: u16, clock: Clock) -> anyhow::Result<Booted> {
    // **The SQL store jojobot runs itself**, brought up and migrated before
    // anything else is wired.
    let (store, applied) = Dolt::ready(dir, port).await.with_context(|| {
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

    // **Rekey what the badge fill just made possible.** A row written before
    // a thing wore a badge still names it by handle in `fact`, `fact_write`
    // and `field_write`; every read here resolves a handle to a badge before
    // it queries, so a row still keyed by the handle is a row nothing can
    // reach. This has to run after the fill above — it reads the badge the
    // fill just gave out — so it cannot be a migration, which runs before
    // either exists.
    //
    // **Not fatal, for the same reason the fill above is not**: the store
    // already reports on itself, and a restart once it is reachable finishes
    // what this pass could not.
    match badging.backfill_handle_keyed_rows().await {
        Ok(0) => {}
        Ok(given) => tracing::info!(
            given,
            "store: rekeyed rows written under a handle to the badge they answer to now"
        ),
        Err(e) => tracing::warn!(
            error = %e,
            "ROWS NOT REKEYED — some rows written before a thing's badge existed may still be \
             unreachable by handle. Nothing was lost; a restart once the store is reachable \
             finishes the rekey."
        ),
    }

    // **`fact.edge_object` (and its `fact_write` mirror), `fact_event_ref.entity`
    // and `entity.parent` onto the badge each names, for a row written before
    // this build resolved them at write time** (rule 268). Runs after the two
    // passes above for the same reason: it reads the badge and the rename
    // history they just made current.
    //
    // **Louder than its two neighbours, on purpose.** Those two leave a row
    // merely unreachable until the next restart. This one, left unresolved,
    // leaves a stale handle sitting in a row a later handle collision could
    // still hijack — the exact defect this rule exists to make unrepresentable
    // — so an operator reading the log has to see it as a problem to repair,
    // not a routine retry. It is still not fatal to boot: refusing to serve a
    // live instance over historical rows would be a worse outcome than
    // reporting on itself loudly and continuing.
    match badging.resolve_stale_pointer_columns().await {
        Ok(0) => {}
        Ok(rewritten) => tracing::info!(
            rewritten,
            "store: resolved stale pointer columns onto the badges they name"
        ),
        Err(e) => tracing::error!(
            error = %e,
            "POINTERS NOT RESOLVED — the values named above still hold a stale handle rather than \
             a badge, and a person has to repair them. Nothing already resolved was undone; a \
             restart repeats the scan and finds less to do."
        ),
    }

    // **One set, read by both halves.** The layer above resolves what the
    // build supplies into an answer; the store below has to see the same set
    // when its guard decides whether a handle names anything, or a claim
    // pointing at a supplied record is refused as naming nothing.
    let supplied = jojobot_mcp::provisions();
    let bare_memory = DoltMemory::open(store.pool().clone())
        .knowing(supplied.clone())
        .on_clock(clock);
    let resolved = open_provisioned(bare_memory, supplied).await?;

    // **A reference-typed field value, stored as plain handle text before
    // this build lowered it at write time, rewritten onto the permanent id
    // it names** (rule 290). Runs after the three passes above for the same
    // reason: it reads the badge and the rename history they just made
    // current. **It also runs after the kinds are loaded**, which
    // `open_provisioned` does: a `reference:<kind>` cell is parsed through the
    // kind set, so a pass that ran before it would fail on any store holding a
    // kind-narrowed reference type. Louder than the first two, for the reason
    // `resolve_stale_pointer_columns` is: a row left unresolved holds a
    // stale handle a later collision could still hijack, so it is reported
    // as a problem to repair rather than a routine retry.
    match badging.migrate_reference_fields().await {
        Ok(0) => {}
        Ok(rewritten) => tracing::info!(
            rewritten,
            "store: resolved stale reference-typed field values onto the badges they name"
        ),
        Err(e) => tracing::error!(
            error = %e,
            "REFERENCE FIELDS NOT RESOLVED — the values named above still hold a stale handle \
             rather than a badge, and a person has to repair them. Nothing already resolved was \
             undone; a restart repeats the scan and finds less to do."
        ),
    }

    // **Plain handle text under a key nobody declared, lowered onto the permanent
    // id it names, and the link table brought into agreement with the field
    // writes.** Runs after the reference-field pass above, which owns the declared
    // keys, and after the kinds are loaded, for the reason that pass does. **A
    // value whose handle names no one thing stays as written and is named here**,
    // one line each, because the pass does not guess; a restart repeats the list
    // for as long as the value stays.
    match badging.migrate_field_links().await {
        Ok(report) => {
            if report.lowered > 0 || report.linked > 0 || report.unlinked > 0 {
                tracing::info!(
                    lowered = report.lowered,
                    linked = report.linked,
                    unlinked = report.unlinked,
                    "store: lowered plain handle text onto the ids it names and updated the link \
                     table"
                );
            }
            for line in &report.left_as_text {
                tracing::warn!(
                    value = %line,
                    "FIELD VALUE LEFT AS TEXT — the handle in it does not name exactly one thing, \
                     so it was not lowered and links nothing. A person has to say which thing it \
                     meant."
                );
            }
        }
        Err(e) => tracing::error!(
            error = %e,
            "FIELD LINKS NOT BUILT — who points at a thing is answered from the link table, so \
             some links may not be found. Nothing was lost; a restart repeats the pass."
        ),
    }

    // **Wrapped in the decorators the story harness shares too** — see this
    // module's own doc for why this is two stages rather than one. The search
    // projection sits in FRONT of the store, so every write through the port
    // keeps the index current. Boot is a plain full re-scan — and a failed
    // scan is not fatal: the store is the truth, and refusing to start
    // because a projection couldn't be built is worse than a thin `search`.
    // It says so loudly instead.
    let (folded, indexed) = assemble_memory(resolved)?;
    match folded.rebuild().await {
        Ok(things) => tracing::info!(things, "fold: candidate fields built from a full read"),
        Err(e) => tracing::warn!(
            error = %e,
            "FOLD EMPTY — the boot read failed, so a fields read falls through to the store \
             until a write through this process fills the cache entry it touches. Nothing is \
             wrong; every read stays correct and only slower until then."
        ),
    }
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
    let bare_mail_store = Arc::new(DoltMailboxes::open(store.pool().clone(), owners));
    let bare_sessions = Arc::new(DoltSessions::open(store.pool().clone()));

    // **The one-time text migration (rule 260).** A journal beat or a
    // mailbox message written before mention resolution reached these ports
    // may still hold a bare `@kind:slug`; this rewrites it onto the badge,
    // once, against the bare stores — before either is wrapped in its own
    // `Mentioning` below. It is not a migration proper, for the same reason
    // `DoltMemory::backfill_handle_keyed_rows` is not one: it needs the
    // badged entity list, which exists only now that the memory store has
    // run its own boot steps. Idempotent by construction — see
    // `DoltSessions::migrate_mentions`'s own doc — so a failed half is not
    // lost progress and a retried boot converges rather than repeating work.
    //
    // The decision of what a failed read means to each migration —
    // distinct from an empty store, which is what `Migrated::Skipped`
    // exists to say — lives in `migrate_permanent_ids` itself, where it can
    // be tested against a real store; this only renders what came back.
    for (name, outcome) in jojobot_adapters::dolt::migrate_permanent_ids(
        indexed.as_ref(),
        &bare_mail_store,
        &bare_sessions,
    )
    .await
    {
        match outcome {
            jojobot_adapters::dolt::Migrated::Ran(n) => {
                tracing::info!(rewritten = n, migration = name, "permanent ids: migrated")
            }
            jojobot_adapters::dolt::Migrated::Skipped(reason) => tracing::warn!(
                error = %reason,
                migration = name,
                "PERMANENT-ID MIGRATION SKIPPED — the read it needed failed, so nothing was \
                 rewritten rather than guessing the store is empty. Not fatal; a restart \
                 retries."
            ),
            jojobot_adapters::dolt::Migrated::Failed(reason) => tracing::warn!(
                error = %reason,
                migration = name,
                "PERMANENT-ID MIGRATION FAILED — a restart retries, and nothing already \
                 migrated is undone."
            ),
        }
    }

    let teachings: Arc<dyn Teachings> = Arc::new(DoltTeachings::open(store.pool().clone()));

    // **Wrapped in the same decorators the story harness shares** — see this
    // module's own doc.
    let wired = assemble_ports(indexed.clone(), bare_mail_store, bare_sessions);
    let sessions = wired.sessions.clone();

    // Mail goes into the SAME index — one front door, one ranked list — so the
    // mailbox store gets the same decorator treatment Memory's does: every verb
    // that changes a message re-indexes it, and boot loads the board once.
    //
    // A failed board read is not fatal, exactly as a failed doc scan is not: the
    // store is the truth, the memory half is untouched, and `search` reports the
    // gap in every answer rather than passing it off as "nothing matched".
    // Refusing to start over a projection is worse than a thin one that admits
    // what it is.
    match wired.mailboxes.rebuild().await {
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
    let mailboxes: Arc<dyn Mailboxes> = wired.mailboxes;
    // **The retrieval port holds both halves, because an answer spans both.**
    // Each half refreshes itself from its own store before a search answers, so
    // a record removed outside jojobot — the only way one leaves at all — stops
    // being served. Neither decorator can reach the other's store, which is why
    // the port is not on either of them.
    // **The third half: a bot's own runs.** Owner-scoped when a query asks, so
    // every bot's runs are indexed and each caller is served only its own.
    match wired.sessions_indexed.rebuild().await {
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
    let search = wired.search;

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
            let rebuilt = registry.rebuild_from(&board);
            tracing::info!(
                recovered = rebuilt.recovered,
                cards = board.len(),
                "sessions: handle registry rebuilt from the board"
            );
            // **Only when there is something to say.** A clean rebuild — every
            // gap between `cards` and `recovered` explained by a card with no
            // `sid` at all — reports nothing further: the routine gap already
            // has its own count, and a line that fired on every boot would
            // stop meaning anything the day it fired on one that mattered.
            if rebuilt.unreadable > 0 {
                tracing::warn!(
                    unreadable = rebuilt.unreadable,
                    "SESSION HANDLE UNREADABLE — a stored sid failed the shape check and was \
                     not put back into the registry. Nothing this process writes produces one, \
                     so this names damage on the board rather than the ordinary pre-handle gap. \
                     The session itself is untouched; only its handle is gone, and booting its \
                     bot again offers the run back by what it was working on."
                );
            }
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

    Ok(Booted {
        store,
        memory: indexed,
        search,
        mailboxes,
        sessions,
        teachings,
        registry,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::memory::{EntityId, NewEntity, NewFact};

    /// **`Folded` genuinely sits inside the whole assembled stack — it is not
    /// merely forwarding to a store that happens to hold the right answer
    /// anyway.** `fields` falls through to the store on a cache miss (see
    /// `Folded::fields`), so a value-only check here would pass even if
    /// `assemble_memory` dropped `Folded` from the chain entirely: the store
    /// underneath still has the right data either way. What actually proves
    /// the fold is in the live write path is staleness — a write made AROUND
    /// the whole assembled stack, straight to the bare store, must be
    /// invisible to a `fields` read through `folded`, exactly as
    /// `Folded`'s own single-writer doc warns.
    #[tokio::test]
    async fn a_write_through_the_assembled_stack_reaches_the_fold_underneath() {
        let bare = Arc::new(InMemoryMemory::booted());
        let resolved: Arc<dyn Memory> = bare.clone();
        let (folded, indexed) = assemble_memory(resolved).expect("index opens");

        let alpha = EntityId::person("person:alpha");
        indexed
            .add_entity(NewEntity::new(alpha.clone(), "Alpha", "user-named"))
            .await
            .expect("add ok")
            .written()
            .expect("not blocked");
        indexed
            .capture(NewFact {
                fields: std::collections::BTreeMap::from([(
                    "due_on".to_string(),
                    "2026-09-14".to_string(),
                )]),
                ..NewFact::about(
                    alpha.clone(),
                    "wired through the whole stack",
                    date(2026, 9, 1),
                )
            })
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        assert_eq!(
            folded
                .fields(&alpha)
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"2026-09-14".to_string()),
            "a write through the outermost assembled port must reach the fold beneath it"
        );

        // Straight to the bare store, around `indexed`, `Mentioning` and
        // `folded` alike — the shape a second writer would take.
        bare.capture(NewFact {
            fields: std::collections::BTreeMap::from([(
                "due_on".to_string(),
                "2026-09-21".to_string(),
            )]),
            ..NewFact::about(
                alpha.clone(),
                "written around the whole stack",
                date(2026, 9, 8),
            )
        })
        .await
        .expect("capture ok")
        .written()
        .expect("not blocked");

        assert_eq!(
            folded
                .fields(&alpha)
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"2026-09-14".to_string()),
            "the fold must still answer with what it cached, proving it is genuinely wired into \
             the write path rather than always falling through to the store"
        );
    }
}
