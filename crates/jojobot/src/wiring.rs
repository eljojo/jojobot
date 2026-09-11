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
//! change. A caller does that step, and the `Provisioned::new` wrap around
//! it, before calling [`assemble_memory`]. Everything from there on is
//! composition over trait objects, which is what this file shares.

use std::sync::Arc;

use anyhow::Context;
use jojobot_adapters::search::{IndexedMailboxes, IndexedMemory, IndexedSessions, Retrieval};
use jojobot_domain::mailbox::Mailboxes;
use jojobot_domain::mailbox::mention as mailbox_mention;
use jojobot_domain::memory::Memory;
use jojobot_domain::memory::mention;
use jojobot_domain::memory::search::Search;
use jojobot_domain::session::Sessions;
use jojobot_domain::session::mention as session_mention;

/// **Stage one: wrap memory, and open the index over it.**
///
/// `resolved` is memory already carrying `.knowing(supplied)` and
/// `Provisioned::new` — see the module doc for why that step is not here
/// too. The concrete `IndexedMemory` comes back, not a trait object, because
/// a caller runs its own boot work against it (a full rebuild, or reading
/// `list_entities` for the mention-text migration) before stage two.
pub fn assemble_memory(resolved: Arc<dyn Memory>) -> anyhow::Result<Arc<IndexedMemory>> {
    // **Mentions resolve above the raw store and below the index.** A
    // mention in a claim has to see the full entity list, and what search
    // holds must be what a reader sees.
    let memory: Arc<dyn Memory> = Arc::new(mention::Mentioning::new(resolved));
    Ok(Arc::new(
        IndexedMemory::new(memory).context("opening the search index")?,
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
