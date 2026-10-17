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
use jojobot_adapters::fold::mail::FoldedMailboxes;
use jojobot_adapters::fold::sessions::FoldedSessions;
use jojobot_adapters::fold::teachings::FoldedTeachings;
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
use jojobot_domain::session::history::WriteStamp;
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

/// **Stage zero: seed the kinds this build ships, then name the stored rows
/// under a supplied handle, then wrap in `Provisioned`.**
///
/// **The order is enforced here rather than left to the caller.**
/// `stored_rows_under_supplied_handles` reads every stored entity, and reading
/// one means parsing its handle's kind — which a process that has not yet
/// loaded the kind set cannot do (`kinds::resolve` answers `SetNeverLoaded`).
/// So the kinds are seeded first, against the bare store, before anything reads
/// a row. Getting this backwards once took the whole server down at boot: the
/// check's `list_entities` hit the first stored row before any kind was
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
    open_provisioned_on(bare, supplied, jojobot_domain::clock::Clock::default()).await
}

/// **[`open_provisioned`], dating the claims the layer writes with `clock`** —
/// the clock the stores beneath stamp with, so an instance acting out a day
/// dates an operator's name on a supplied record with that day.
pub async fn open_provisioned_on<M: Memory + Clone + 'static>(
    bare: M,
    supplied: Provisions,
    clock: jojobot_domain::clock::Clock,
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
    // **A stored row at a handle the build now supplies does not stop the boot**
    // (rule 508). The stored row answers there, as `Provisioned` already
    // answers it in every read, and what the build ships stays under the keys
    // the row holds. Each such handle is named once, so the operator can see
    // which of their things a later build has come to share a name with.
    //
    // **Read against the bare store, before it is wrapped in `Provisioned`**:
    // the wrapped store would resolve the handle to the supplied record and
    // report nothing.
    match jojobot_domain::memory::owned::stored_rows_under_supplied_handles(&bare, &supplied).await
    {
        Ok(handles) => {
            for handle in handles {
                tracing::info!(
                    %handle,
                    "store: a stored row is already at a handle this build supplies; the stored \
                     row answers there"
                );
            }
        }
        Err(e) => tracing::warn!(
            error = %e,
            "STORED ROWS UNDER SUPPLIED HANDLES NOT CHECKED — the store could not be read. Every \
             read still answers a stored row before a supplied record, so nothing is served \
             wrongly; a restart once the store is reachable names them."
        ),
    }
    Ok(Arc::new(Provisioned::new(bare, supplied).on_clock(clock)))
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
    /// The image of the board that `mailboxes` reads from, under the change
    /// count. A caller fills it with `rebuild` before anything reads.
    pub mail_image: Arc<FoldedMailboxes>,
    /// The image of the runs that `sessions` reads from, under the change
    /// count. A caller fills it with `rebuild` before anything reads.
    pub sessions_image: Arc<FoldedSessions>,
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
    stamp: WriteStamp,
    clock: Clock,
) -> Wired {
    // **Mentions resolve above the raw store and below the index here too**
    // — the same placement as memory's own `Mentioning`.
    //
    // **Both stores are counted underneath that**, so search can tell that
    // nothing changed without asking the store: every write to mail and to a
    // run goes through these ports.
    //
    // **And both are held in memory underneath that, closest to the store**
    // (decision log 292): the image answers every read once its `rebuild` has
    // run, and the count above it sees every write on its way to the store.
    let mail_image = Arc::new(FoldedMailboxes::new(bare_mail));
    // **The run image and the store beneath it share one stamp**, so the records
    // of a change are one record rather than two with different times.
    let sessions_image = Arc::new(FoldedSessions::new(bare_sessions, stamp, clock));
    let bare_mail: Arc<dyn Mailboxes> = Arc::new(jojobot_adapters::changes::MailChanges::new(
        mail_image.clone(),
    ));
    let bare_sessions: Arc<dyn Sessions> = Arc::new(
        jojobot_adapters::changes::SessionChanges::new(sessions_image.clone()),
    );
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
        mail_image,
        sessions_image,
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
    /// The pushes waiting to go out, read from the same rows the mail store
    /// writes them into.
    pub outbox: Arc<dyn jojobot_domain::push::Outbox>,
    /// What `AppState.sessions` is.
    pub sessions: Arc<dyn Sessions>,
    /// What `AppState.teachings` is.
    pub teachings: Arc<dyn Teachings>,
    /// What `AppState.registry` is.
    pub registry: Arc<jojobot_mcp::sid::SessionRegistry>,
    /// What `AppState.requests` is: the ledger of request ids, in the same store
    /// the writes are stored in.
    pub requests: Arc<dyn jojobot_domain::request::Requests>,
    /// **The clock this server stamps with**, which is what `AppState.clock` is.
    /// The clock the boot was given, continued after the newest stamp the store
    /// already holds when that clock states a day.
    pub clock: Clock,
}

/// **The fill a server makes before it serves**: the fold takes a full read of
/// the store, then the search index takes a full scan. The binary's boot and the
/// story harness both call this, so a story serves from the image the operator's
/// server serves from. A failed read is not fatal: the store is the truth, and
/// refusing to start because a projection could not be built is worse than a thin
/// `search`. It says so loudly instead.
pub async fn fill_memory(folded: &Folded, indexed: &IndexedMemory) {
    let filling = std::time::Instant::now();
    match folded.rebuild().await {
        Ok(things) => tracing::info!(
            things,
            elapsed_ms = filling.elapsed().as_millis() as u64,
            "fold: everything the memory verbs read is held in memory, from a full read"
        ),
        Err(e) => tracing::warn!(
            error = %e,
            "FOLD EMPTY — the boot read failed, so every memory read goes to the store until a \
             write through this process fills it again. Nothing is wrong; every read stays \
             correct and only slower until then."
        ),
    }
    match indexed.rebuild_scanned().await {
        Ok(scan) => {
            tracing::info!(docs = scan.len(), "search: index built from a full scan");
            report_stored_records_over_the_ceiling(&scan);
        }
        Err(e) => tracing::warn!(
            error = %e,
            "SEARCH INDEX EMPTY — the boot scan failed, so `search` sees only what this process \
             writes from here on. The memory verbs are unaffected; restart once the store is \
             reachable to get a full index."
        ),
    }
}

/// **Name the records the store already holds over the ceiling**, from the scan
/// the boot just indexed: no second read of the store. The write refusal keeps a
/// new record from exceeding one answer and nothing else finds the ones that
/// were written before it, so this is where a person learns they are there.
///
/// One warning per record, naming the thing, the record's address, its size and
/// the overage; one warning per thing whose own name, aliases and keys do not
/// fit, since no record on it can. When there are none, one line with the number
/// of records measured, so that silence is never read as a pass that did not run.
/// The measure is the write refusal's own.
fn report_stored_records_over_the_ceiling(scan: &[jojobot_domain::memory::search::DocScan]) {
    let found = jojobot_mcp::memory::claim_room::stored_records_over_the_ceiling(
        scan,
        jiff::Timestamp::now(),
    );
    for record in &found.over {
        tracing::warn!(
            thing = %record.thing,
            address = %record.address,
            size = record.size,
            over = record.size - record.room,
            "STORED RECORD OVER THE CEILING — a read of this thing cannot serve this record \
             whole in one answer. It was written before the write refusal existed. Nothing is \
             lost; a person decides whether it is shortened or kept as a page behind a pointer."
        );
    }
    for (thing, size, room) in &found.things_over {
        tracing::warn!(
            thing = %thing,
            size,
            over = size - room,
            "STORED THING OVER THE CEILING — its own name, aliases and keys do not fit one \
             answer, so no record on it can be served. Nothing is lost; a person shortens them."
        );
    }
    if found.over.is_empty() && found.things_over.is_empty() {
        tracing::info!(
            records = found.measured,
            "stored records: none is over the ceiling"
        );
    }
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
    boot_store_pushing(dir, port, clock, false).await
}

/// **[`boot_store`], with the mail store told whether a post to a person's box
/// writes a push.** An instance with no push service set up passes `false`, and
/// then writes no outbox row nobody would send.
pub async fn boot_store_pushing(
    dir: &Path,
    port: u16,
    clock: Clock,
    pushing: bool,
) -> anyhow::Result<Booted> {
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

    // **A clock that states a day continues after the newest stamp the store
    // already holds, on that day or after it.** It counts from midnight in every
    // process, so a process started over a store another process filled would
    // stamp its first writes before that process's last. The clock is made once
    // here and every stamp below, in the memory store and above it, comes from
    // this one.
    //
    // **Fatal on a stated clock.** A store that cannot answer here would leave a
    // clock that stamps from the day's midnight, and a new record may then sort
    // before an older one with nothing to say so afterwards. Refusing is the same
    // fact said where somebody can act on it. The real clock reads nothing here.
    let clock = match clock.stated() {
        None => clock,
        Some(day) => {
            let newest = jojobot_adapters::dolt::newest_stamp_from(store.pool(), day)
                .await
                .with_context(|| {
                    format!(
                        "STATED CLOCK NOT CONTINUED — the newest stamp from the stated day {day} \
                         on could not be read, so the server would stamp from the day's midnight \
                         and a new record could sort before an older one. Nothing stored was \
                         changed; start again once the store is reachable."
                    )
                })?;
            clock.continuing_after(newest)
        }
    };

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

    // **One set, read by both halves.** The layer above resolves what the
    // build supplies into an answer; the store below has to see the same set
    // when its guard decides whether a handle names anything, or a claim
    // pointing at a supplied record is refused as naming nothing.
    let supplied = jojobot_mcp::provisions();
    let linking = DoltMemory::open(store.pool().clone()).knowing(supplied.clone());
    let bare_memory = DoltMemory::open(store.pool().clone())
        .knowing(supplied.clone())
        .on_clock(clock);
    let resolved = open_provisioned_on(bare_memory, supplied, clock).await?;

    // **A handle the build stopped supplying, or renamed, keeps the writes the
    // operator made while it was supplied** (rule 508). They are filed under the
    // handle's own text with no row, so a read finds nothing and calls the handle
    // unknown. This gives such a handle a row, so it reads back as any stored
    // thing. **After the kinds are loaded**, which `open_provisioned` does: the
    // kind of the row comes from the handle. **Over an instance told what the
    // build supplies**, so a handle still supplied is left alone. Every boot, and
    // cheap when there is nothing to do: a build can stop supplying a handle at
    // any release.
    // **Only a handle a build once supplied is adopted**, so this build records
    // what it supplies, and what earlier builds supplied before the record
    // existed, before the pass reads the record.
    if let Err(e) = linking
        .record_supplied_handles(&jojobot_mcp::retired_handles())
        .await
    {
        tracing::error!(
            error = %e,
            "SUPPLIED HANDLES NOT RECORDED — a handle this build stops supplying later will not \
             be given a row, and what the operator wrote on it will not be reachable. A restart \
             once the store is reachable records them."
        );
    }
    match linking.adopt_handles_without_a_row().await {
        Ok(report) => {
            for handle in &report.adopted {
                tracing::info!(
                    %handle,
                    "store: gave a row to a handle whose writes outlived its record, so it reads \
                     back as a stored thing"
                );
            }
            for key in &report.left {
                tracing::warn!(
                    %key,
                    "WRITES FILED UNDER A HANDLE OF A KIND THIS BUILD DOES NOT DECLARE — they \
                     were not given a row and no read reaches them. A person has to say what \
                     they are."
                );
            }
        }
        Err(e) => tracing::error!(
            error = %e,
            "HANDLES WITHOUT A ROW NOT ADOPTED — what the operator wrote on a handle the build \
             no longer supplies is still in the store and not reachable. Nothing was lost; a \
             restart once the store is reachable repeats the pass."
        ),
    }

    // **`fact.edge_object` (and its `fact_write` mirror), `fact_event_ref.entity`
    // and `entity.parent` onto the badge each names, for a row written before
    // this build resolved them at write time** (rule 268). Runs after the two
    // passes above for the same reason: it reads the badge and the rename
    // history they just made current. **And after the adoption above**: a
    // pointer at a handle the build stopped supplying names nothing until the
    // adoption gives that handle a row, so a scan that ran first would report
    // it as unresolvable at every upgrade. **Over the instance that is told what
    // the build supplies**, which `badging` is not: a pointer at a shipped record
    // has no row to resolve through, and is valid as it stands.
    //
    // **Louder than its two neighbours, on purpose.** Those two leave a row
    // merely unreachable until the next restart. This one, left unresolved,
    // leaves a stale handle sitting in a row a later handle collision could
    // still hijack — the exact defect this rule exists to make unrepresentable
    // — so an operator reading the log has to see it as a problem to repair,
    // not a routine retry. It is still not fatal to boot: refusing to serve a
    // live instance over historical rows would be a worse outcome than
    // reporting on itself loudly and continuing.
    match linking.resolve_stale_pointer_columns().await {
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
    // as a problem to repair rather than a routine retry. **Over the instance
    // that is told what the build supplies**, as the pass above is: a value
    // naming a shipped record has no row to resolve through, and is valid as it
    // stands.
    match linking.migrate_reference_fields().await {
        Ok(0) => {}
        Ok(rewritten) => tracing::info!(
            rewritten,
            "store: resolved stale reference-typed field values onto the badges they name"
        ),
        Err(e) => tracing::error!(
            error = %e,
            "REFERENCE FIELDS NOT RESOLVED — the values named above still hold a stale handle \
             rather than a badge. Nothing already resolved was undone. The pass has finished \
             and is not repeated, so this is the one report; a person has to repair them."
        ),
    }

    // **Plain handle text under a key nobody declared, lowered onto the permanent
    // id it names, and the link table brought into agreement with the field
    // writes.** Runs after the reference-field pass above, which owns the declared
    // keys, and after the kinds are loaded, for the reason that pass does. **A
    // value whose handle names no one thing stays as written and is named here**,
    // one line each, because the pass does not guess; the pass is marked finished
    // after that report, so it is made once and never repeated. The link table is
    // checked cheaply on every boot and rebuilt only when it disagrees.
    // **Over an instance that is told what the build supplies**, which `badging`
    // is not: a value naming a shipped record has no row to resolve through, and
    // an instance with no provisions reads that handle as naming nothing and
    // leaves the value as text with no link.
    match linking.migrate_field_links().await {
        Ok(report) => {
            if report.disagreed {
                tracing::warn!(
                    linked = report.linked,
                    unlinked = report.unlinked,
                    "LINK TABLE DISAGREED WITH THE FIELD WRITES — it was rebuilt from them. The \
                     table is derived, so nothing was lost; a disagreement means a build or a \
                     failure left it behind."
                );
            }
            if report.settled {
                tracing::info!("store: field value lowering: already done");
            }
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

    // **Forwarding rows an older build left naming a handle their survivor wore
    // before a rename, brought to the handle it wears now.** Runs before the pass
    // below, which reads the same rows. Marked finished once it has committed
    // and skipped no row; a row it could not place is warned about once, the first boot that
    // meets it.
    match linking.migrate_forward_names().await {
        Ok(report) if report.settled => {
            tracing::info!("store: forwarding rows name their survivor: already done");
        }
        Ok(report) => tracing::info!(
            renamed = report.renamed,
            skipped = report.skipped.len(),
            "store: forwarding rows brought to the handle their survivor wears now"
        ),
        Err(e) => tracing::error!(
            error = %e,
            "FORWARDING ROWS NOT BROUGHT TO THEIR SURVIVOR'S HANDLE — a folded thing may \
             name the handle its survivor wore before a rename. Nothing was lost; a restart \
             repeats the pass."
        ),
    }

    // **Links a fold of an older build left on the forwarding row, repointed at
    // the thing it ended in.** Runs after the pass above, which makes the link
    // table agree with the writes it reads. Marked finished once it has
    // committed and skipped no forward, so it is made once; a forward it could
    // not place keeps it open and is warned about once, the first boot that meets it.
    match linking.migrate_forwarded_links().await {
        Ok(report) if report.settled => {
            tracing::info!("store: links on forwarding rows: already done");
        }
        Ok(report) => tracing::info!(
            repointed = report.repointed,
            skipped = report.skipped.len(),
            "store: repointed the field links that named a folded thing onto the thing it \
             ended in"
        ),
        Err(e) => tracing::error!(
            error = %e,
            "FIELD LINKS ON FORWARDING ROWS NOT REPOINTED — a field that names a folded thing \
             still names the row that forwards. Nothing was lost; a restart repeats the pass."
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
    fill_memory(&folded, &indexed).await;

    // **The ports mail and sessions are served from.** Rows in the SQL store,
    // both over the one pool.
    //
    // The owner index is the production one and it reads Memory: a box is
    // created FOR somebody, the entity world is somewhere else now, and "does
    // this handle resolve" is the whole of what crosses. It reads through the
    // projection so a bot created this session is an owner this session.
    let owners: Arc<dyn OwnerIndex> = Arc::new(MemoryOwners::new(indexed.clone()));
    let bare_mail_store = match pushing {
        true => DoltMailboxes::open(store.pool().clone(), owners)
            .pushing()
            .on_clock(clock),
        false => DoltMailboxes::open(store.pool().clone(), owners).on_clock(clock),
    };
    let bare_mail_store = Arc::new(bare_mail_store);
    let outbox: Arc<dyn jojobot_domain::push::Outbox> = bare_mail_store.clone();
    let session_stamp = WriteStamp::new();
    let bare_sessions = Arc::new(
        DoltSessions::open(store.pool().clone())
            .on_clock(clock)
            .stamped_by(session_stamp.clone()),
    );

    // **The one-time text migration (rule 260).** A journal beat or a
    // mailbox message written before mention resolution reached these ports
    // may still hold a bare `@kind:slug`; this rewrites it onto the badge,
    // once, against the bare stores — before either is wrapped in its own
    // `Mentioning` below. It is not a migration proper, for the same reason
    // `DoltMemory::backfill_handle_keyed_rows` is not one: it needs the
    // badged entity list, which exists only now that the memory store has
    // run its own boot steps. Idempotent by construction — see
    // `DoltSessions::migrate_mentions`'s own doc — so a failed half is not
    // lost progress. A pass that finished is marked in the store, and a later
    // boot reads none of its table (old text naming a handle nothing wore then
    // stays text; see `PASSES` in the adapter). A pass that failed or was
    // skipped is not marked, and the next boot runs it again.
    //
    // The decision of what a failed read means to each migration —
    // distinct from an empty store, which is what `Migrated::Skipped`
    // exists to say — lives in `migrate_permanent_ids` itself, where it can
    // be tested against a real store; this only renders what came back.
    for (name, outcome) in jojobot_adapters::dolt::migrate_permanent_ids(
        store.pool(),
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
            jojobot_adapters::dolt::Migrated::Settled => {
                tracing::info!(migration = name, "permanent ids: already done")
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

    // **The one-time pass that gives a thing whose schedule cannot be read the
    // day it became late.** The owed read selects on the stored due day alone, so
    // a loop left unreadable before a write stored one for it never reached the
    // read. Stamped with the day of this boot in the instance's zone (UTC when it
    // holds none), once, and marked finished; a thing that becomes unreadable
    // afterwards is stamped by the write that made it so.
    // Not fatal: a pass that failed or was skipped is owed again at the next boot.
    match jojobot_adapters::dolt::backfill_unreadable_due_days(
        store.pool(),
        indexed.as_ref(),
        the_day_of_this_boot(indexed.as_ref(), &clock).await,
    )
    .await
    {
        jojobot_adapters::dolt::Migrated::Ran(n) => {
            tracing::info!(stamped = n, "due days: stamped what could not be read")
        }
        jojobot_adapters::dolt::Migrated::Settled => {
            tracing::info!("due days: already stamped")
        }
        jojobot_adapters::dolt::Migrated::Skipped(reason) => tracing::warn!(
            error = %reason,
            "UNREADABLE-DUE PASS SKIPPED — the read it needed failed, so nothing was stamped. \
             Not fatal; a restart retries."
        ),
        jojobot_adapters::dolt::Migrated::Failed(reason) => tracing::warn!(
            error = %reason,
            "UNREADABLE-DUE PASS FAILED — some things that cannot say when they fall due were \
             not stamped, so the owed read may miss them. A restart retries."
        ),
    }

    let requests_pool = store.pool().clone();
    let folded_teachings = Arc::new(FoldedTeachings::new(Arc::new(DoltTeachings::open(
        store.pool().clone(),
    ))));
    let teachings: Arc<dyn Teachings> = folded_teachings.clone();

    // **Wrapped in the same decorators the story harness shares** — see this
    // module's own doc.
    let wired = assemble_ports(
        indexed.clone(),
        bare_mail_store,
        bare_sessions,
        session_stamp,
        clock,
    );
    let sessions = wired.sessions.clone();

    // **Mail, runs and the teaching ledger are held in memory too, before
    // anything reads them** (decision log 292). Each fill is a full read of its
    // own store, once; a failed fill is not fatal, because the store is the
    // truth and every read answers from it until a write fills the image again.
    let filling = std::time::Instant::now();
    match wired.sessions_image.rebuild().await {
        Ok(runs) => tracing::info!(
            runs,
            elapsed_ms = filling.elapsed().as_millis() as u64,
            "fold: every run is held in memory, from a full read"
        ),
        Err(e) => tracing::warn!(
            error = %e,
            "RUNS NOT HELD IN MEMORY — the boot read failed, so every read of a run goes to the \
             store until a write through this process fills the image again. Nothing is wrong; \
             every read stays correct and only slower until then."
        ),
    }
    let filling = std::time::Instant::now();
    match wired.mail_image.rebuild().await {
        Ok(messages) => tracing::info!(
            messages,
            elapsed_ms = filling.elapsed().as_millis() as u64,
            "fold: every box and message is held in memory, from a full read"
        ),
        Err(e) => tracing::warn!(
            error = %e,
            "MAIL NOT HELD IN MEMORY — the boot read failed, so every read of the board goes \
             to the store until a write through this process fills the image again. Nothing is \
             wrong; every read stays correct and only slower until then."
        ),
    }
    let filling = std::time::Instant::now();
    match folded_teachings.rebuild().await {
        Ok(taught) => tracing::info!(
            taught,
            elapsed_ms = filling.elapsed().as_millis() as u64,
            "fold: the teaching ledger is held in memory, from a full read"
        ),
        Err(e) => tracing::warn!(
            error = %e,
            "TEACHING LEDGER NOT HELD IN MEMORY — the boot read failed, so every first contact \
             goes to the store until a write through this process fills the ledger again. \
             Nothing is wrong; every answer stays correct and only slower until then."
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

    let registry = run_startup_passes(
        &sessions,
        &(indexed.clone() as Arc<dyn Memory>),
        &mailboxes,
        &clock,
    )
    .await;
    let seed_memory: Arc<dyn Memory> = indexed.clone();
    // **A role claimed before roles were objects is carried onto its object.** An
    // older build kept the holder and the moment on the bot, and nothing reads
    // them as a lease now, so a store restored from before the change would seat a
    // second session in every such role. Each claim is written onto its role
    // object, the keys come off the bot, and the pass is marked finished. Not
    // fatal: a pass that failed or was skipped is owed again at the next boot, and
    // the keys of a role that could not be moved stay where they are.
    match jojobot_adapters::dolt::move_old_role_claims_once(
        store.pool(),
        seed_memory.as_ref(),
        the_day_of_this_boot(seed_memory.as_ref(), &clock).await,
    )
    .await
    {
        jojobot_adapters::dolt::Migrated::Ran(moved) => {
            tracing::info!(moved, "carried the old role claims onto their role objects")
        }
        jojobot_adapters::dolt::Migrated::Settled => {
            tracing::info!("old role claims: already carried")
        }
        jojobot_adapters::dolt::Migrated::Skipped(reason) => tracing::warn!(
            error = %reason,
            "OLD ROLE CLAIMS NOT CARRIED — the bots could not be read at startup. Nothing was \
             written; a role claimed in the old shape reads as empty until a restart repeats \
             the pass."
        ),
        jojobot_adapters::dolt::Migrated::Failed(reason) => tracing::warn!(
            error = %reason,
            "OLD ROLE CLAIMS NOT ALL CARRIED — a role named above was not moved, so it reads as \
             empty until a restart repeats the pass."
        ),
    }

    Ok(Booted {
        store,
        memory: indexed,
        search,
        mailboxes,
        outbox,
        sessions,
        teachings,
        registry,
        requests: Arc::new(
            jojobot_adapters::dolt::requests::DoltRequests::new(requests_pool).on_clock(clock),
        ),
        clock,
    })
}

/// **The passes that run after the stores are wired and before anything
/// serves**: the handle registry is filled from the board, the default identity,
/// the instance record and the shipped types are seeded. The binary's boot and
/// the story harness both call this, so breaking a pass here reddens a story. A
/// pass that cannot read its store logs what it cost and does not stop the boot.
///
/// What stays out is what needs the real store: the old role claims, which want
/// the pool, and the kinds, which the store opens with.
pub async fn run_startup_passes(
    sessions: &Arc<dyn Sessions>,
    memory: &Arc<dyn Memory>,
    mailboxes: &Arc<dyn Mailboxes>,
    clock: &Clock,
) -> Arc<jojobot_mcp::sid::SessionRegistry> {
    let seed_memory = memory.clone();
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
            // **The notes a card has spent, and the notes past their keep
            // window, go first.** What is read next is then only the notes of
            // runs that have not written and are still inside the window, so a
            // key past its window opens nothing at the first start that finds it.
            if let Err(e) = sessions
                .prune_minted(clock.now() - jojobot_domain::session::MINTED_NOTE_KEPT)
                .await
            {
                tracing::warn!(error = %e, "spent notes of minted runs could not be pruned");
            }
            // **The runs a boot handed a key and that have not written yet**
            // have no card, so only their note can bring them back.
            match sessions.minted_without_a_card().await {
                Ok(minted) => {
                    let put_back = registry.rebuild_minted(&minted);
                    tracing::info!(
                        put_back,
                        "sessions: keys of runs that had not written put back"
                    );
                }
                Err(e) => tracing::warn!(
                    error = %e,
                    "RUN KEYS NOT RECOVERED — the notes of runs that had not written could \
                     not be read at startup, so those runs' keys answer 'opens no session'. \
                     Booting again begins a run; nothing on the board was lost."
                ),
            }
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

    // **There is never a jojobot with no bot.** The default identity arrives
    // with the software, before anything serves — which is what makes the
    // write gate shippable at all: an identity IS a bot, so a server with none
    // could never create the first one through its own surface.
    match jojobot_mcp::seed::ensure_default_identity(&seed_memory, mailboxes).await {
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

    // **The instance's own record, there from the first boot.** The operator,
    // the zone and every setting the instance holds are keys on it, so a first
    // write onto it must not need a step that only makes it. Created only when
    // no row exists: an instance that already holds one is left as it is.
    match jojobot_mcp::seed::ensure_instance_record(&seed_memory).await {
        jojobot_mcp::seed::Seeded::Created => {
            tracing::info!(
                record = jojobot_domain::memory::INSTANCE_RECORD,
                "seeded the instance record"
            )
        }
        jojobot_mcp::seed::Seeded::AlreadyThere => {}
        jojobot_mcp::seed::Seeded::Unreachable(why) => tracing::warn!(
            error = %why,
            "INSTANCE RECORD NOT SEEDED — nothing was written and nothing was lost; the operator \
             and the zone cannot be written until the record exists. A restart once the store is \
             reachable puts it right."
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

    registry
}

/// **The day a boot-time pass stamps**, read the way a run that states no zone is
/// answered: in the instance's zone, and in UTC when the instance holds none.
async fn the_day_of_this_boot(memory: &dyn Memory, clock: &Clock) -> jiff::civil::Date {
    clock.today_in(&jojobot_mcp::orientation::instance_zone::unzoned_frame(memory).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::memory::{EntityId, NewEntity, NewFact};

    /// **The zone that is a different day from UTC right now**, chosen at run time:
    /// one zone fourteen hours ahead and one eleven behind cannot both share
    /// UTC's day at any moment.
    fn a_zone_on_another_day_than_utc() -> (&'static str, jiff::civil::Date) {
        let now = jiff::Timestamp::now();
        let utc = now.to_zoned(jiff::tz::TimeZone::UTC).date();
        ["Pacific/Kiritimati", "Pacific/Pago_Pago"]
            .into_iter()
            .map(|name| {
                let zone = jiff::tz::TimeZone::get(name).expect("a zone this build resolves");
                (name, now.to_zoned(zone).date())
            })
            .find(|(_, day)| *day != utc)
            .expect("one of the two zones is on another day")
    }

    /// **A boot stamps the instance's day, not UTC's**, so an evening boot in a
    /// zone behind UTC does not stamp tomorrow. With no zone on the instance the
    /// day is UTC's.
    #[tokio::test]
    async fn the_day_a_boot_stamps_is_the_instances_and_utc_when_it_holds_none() {
        let (zone, instances_day) = a_zone_on_another_day_than_utc();
        let utc_day = Clock::Real.today_in(&jiff::tz::TimeZone::UTC);
        let memory = InMemoryMemory::booted();
        assert_eq!(
            the_day_of_this_boot(&memory, &Clock::Real).await,
            utc_day,
            "no zone on the instance: UTC's day"
        );

        let instance = EntityId::person(jojobot_domain::memory::INSTANCE_RECORD);
        memory
            .add_entity(NewEntity::new(instance.clone(), "Instance", "test"))
            .await
            .expect("add ok")
            .written()
            .expect("not blocked");
        memory
            .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                fields: std::collections::BTreeMap::from([(
                    "timezone".to_string(),
                    zone.to_string(),
                )]),
                ..NewFact::about(instance, "the operator's zone", date(2026, 9, 1))
            }))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        assert_ne!(instances_day, utc_day, "the case needs two different days");
        assert_eq!(
            the_day_of_this_boot(&memory, &Clock::Real).await,
            instances_day,
            "the instance holds {zone}: its day"
        );
    }

    /// **`Folded` genuinely sits inside the whole assembled stack — it is not
    /// merely forwarding to a store that happens to hold the right answer
    /// anyway.** `fields` falls through to the store when no image is held (see
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
        // The boot fills the image; a fold nobody filled sends every read to
        // the store, and the staleness below would prove nothing.
        folded.rebuild().await.expect("the image fills");

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
                derived: ["due_on".to_string()].into_iter().collect(),
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
            derived: ["due_on".to_string()].into_iter().collect(),
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

    /// **The images of mail and runs sit inside the whole assembled stack.** A
    /// filled image answers from memory, so a write made AROUND the stack,
    /// straight to the bare store, is invisible to a read through the outermost
    /// port; and a write made THROUGH it is visible. The first alone would pass
    /// on a stack that dropped the image and the second alone on one that never
    /// read the store, so the pair is the proof.
    #[tokio::test]
    async fn mail_and_runs_are_held_in_memory_inside_the_assembled_stack() {
        use jojobot_domain::mailbox::testing::InMemoryMailboxes;
        use jojobot_domain::mailbox::{MailboxName, Mailboxes, NewMessage};
        use jojobot_domain::session::testing::InMemorySessions;
        use jojobot_domain::session::{NewSession, Sessions, Sid};

        jojobot_domain::memory::kinds::load_shipped();
        let bare: Arc<dyn Memory> = Arc::new(InMemoryMemory::booted());
        let (folded, indexed) = assemble_memory(bare).expect("index opens");
        folded.rebuild().await.expect("the memory image fills");
        let mail = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let runs = Arc::new(InMemorySessions::new());
        let wired = assemble_ports(
            indexed,
            mail.clone(),
            runs.clone(),
            runs.write_stamp(),
            Clock::Real,
        );
        wired.mail_image.rebuild().await.expect("the board fills");
        wired.sessions_image.rebuild().await.expect("the runs fill");

        let new_run = |id: &str, sid: &str| NewSession {
            bot: EntityId("bot:gamma".into()),
            sid: Sid(sid.into()),
            id: Some(jojobot_domain::session::SessionId(id.into())),
            key_hash: None,
            focus: "a sitting".into(),
            started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
            timezone: None,
            started_on: None,
        };
        let post = |body: &str| NewMessage {
            mailbox: MailboxName("gamma".into()),
            body: body.into(),
            subject: None,
            sender: "delta".into(),
            sent_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
            in_reply_to: None,
            supersedes: None,
            sender_mail_waiting_at_send: None,
            posted_by_session: None,
        };

        // Through the stack: both are visible from the outermost port.
        wired
            .mailboxes
            .create_mailbox(
                &MailboxName("gamma".into()),
                &EntityId("bot:gamma".into()),
                None,
            )
            .await
            .expect("create ok")
            .written()
            .expect("not blocked");
        wired
            .mailboxes
            .post_message(post("through the stack"))
            .await
            .expect("post ok")
            .written()
            .expect("not blocked");
        wired
            .sessions
            .begin(new_run("through", "th01"))
            .await
            .expect("begin ok");
        assert_eq!(
            wired.mailboxes.scan_messages().await.expect("scan").len(),
            1,
            "a post through the outermost port is visible from it"
        );
        assert_eq!(
            wired.sessions.all_sessions().await.expect("list").len(),
            1,
            "a run begun through the outermost port is visible from it"
        );

        // Around the stack, straight to the bare stores: the images answer.
        mail.post_message(post("around the stack"))
            .await
            .expect("post ok")
            .written()
            .expect("not blocked");
        runs.begin(new_run("around", "ar01"))
            .await
            .expect("begin ok");
        assert_eq!(
            wired.mailboxes.scan_messages().await.expect("scan").len(),
            1,
            "the board is answered from memory, so a write around the stack is not seen"
        );
        assert_eq!(
            wired.sessions.all_sessions().await.expect("list").len(),
            1,
            "the runs are answered from memory, so a write around the stack is not seen"
        );
    }
}
