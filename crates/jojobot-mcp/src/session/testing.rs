//! **Sessions' test fixtures** — the bots a test boots, the runs it leaves in
//! particular states, and the stores that misbehave on purpose.
//!
//! Named for the context they belong to, mirroring `jojobot_domain::session::
//! testing`. What builds a handler — including one already booted as somebody —
//! lives in [`crate::harness`]: an identity is every context's business, not
//! this one's.

use super::*;
use crate::harness::*;
use crate::memory::testing::SpySearch;
use async_trait::async_trait;
use jojobot_domain::mailbox::testing::InMemoryMailboxes;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::session::{CharterChange, Sid, WrapWindow};
use std::sync::Mutex;

/// A distinct, well-shaped handle for a fixture, from any number a call site
/// has to hand — usually `line!()`.
///
/// **The bound is stated, not arithmetic.** This was `line!() % 1000` written
/// inline, which reads as "keep it in range" and is a no-op whenever the file
/// is under a thousand lines — so clippy calls it dead the moment the code
/// moves to a lower line, which is exactly what the reshuffle did. A handle is
/// [`SID_LEN`](jojobot_domain::session::SID_LEN) characters and that is the
/// actual requirement.
pub(crate) fn fixture_sid(nth: u32) -> Sid {
    Sid(format!("t{:03}", nth % 1_000))
}
pub(crate) use jojobot_domain::session::testing::InMemorySessions;

/// Boot as this bot and pick up the one run it is offered — what a reconnect
/// does, now that a boot finding work in flight hands back a choice rather
/// than a handle.
pub(crate) async fn resumed(jojobot: &Jojobot, name: &str) -> String {
    let offered = boot(jojobot, name).await;
    let choice = offered["session"]["choices"][0]["run"]
        .as_str()
        .unwrap_or_else(|| panic!("{name} was offered nothing to resume: {offered}"))
        .to_string();
    sid_of(&boot_answering(jojobot, name, &choice).await).expect("the resumed handle")
}

/// A handle addressing a card that already exists — what a restart rebuilds
/// off the board, and the only way to name one particular run now that the
/// handle is the address.
pub(crate) fn as_run(jojobot: &Jojobot, bot: &str, card: &SessionId) -> String {
    jojobot
        .registry
        .mint_on_card(&EntityId::new(EntityKind::BOT, bot), card.clone())
        .key
}

/// Close a session the way the sweep would, and put its last beat far
/// enough back that it reads as that old.
pub(crate) async fn abandoned_run(
    store: &InMemorySessions,
    bot: &str,
    focus: &str,
    hours_ago: i64,
) -> Session {
    let begun = store
        .begin(NewSession {
            started_on: None,
            timezone: None,
            bot: EntityId(format!("bot:{bot}")),
            sid: Sid(format!("t{:03}", hours_ago.rem_euclid(1000))),
            focus: focus.into(),
            started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(hours_ago),
            id: None,
            key_hash: None,
        })
        .await
        .expect("begin ok");
    store
        .close(&begun.id, SessionState::Abandoned)
        .await
        .expect("close ok");
    store.read_session(&begun.id).await.expect("read ok")
}

/// A handler over **any** `Sessions` implementation — the doubles that misbehave
/// on purpose, which are not `InMemorySessions` and cannot go through
/// [`with_sessions`].
/// **Hands back the memory port too**, so a caller can provision below the
/// surface. A session double that is primed to fail on its Nth `begin` counts
/// the fixture's own writes otherwise — standing a bot up is an attributed
/// write now, so it starts a session and consumes the failure the test meant
/// for its own first journal.
pub(crate) fn with_sessions_port_and_memory(
    sessions: Arc<dyn Sessions>,
) -> (Jojobot, Arc<InMemoryMemory>) {
    let memory = Arc::new(InMemoryMemory::booted());
    let jojobot = Jojobot::new(
        memory.clone(),
        Arc::new(SpySearch::default()),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        sessions,
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        crate::harness::seeded_registry(),
    );
    (jojobot, memory)
}

/// A handler over a session store the test still holds a typed handle to.
pub(crate) fn with_sessions(sessions: Arc<InMemorySessions>) -> Jojobot {
    connection(Arc::new(InMemoryMemory::booted()), sessions)
}

/// A second connection to the same worlds — what a reconnect or a device hop
/// builds. The binding is per handler, so this is the only way to test that
/// resuming reads the board rather than remembering anything.
pub(crate) fn connection(memory: Arc<InMemoryMemory>, sessions: Arc<InMemorySessions>) -> Jojobot {
    connection_sharing(memory, sessions, crate::harness::seeded_registry())
}

/// The same, over a registry the caller keeps — what two connections of one
/// PROCESS share, and the only way a handle outlives the connection it was
/// handed to.
pub(crate) fn connection_sharing(
    memory: Arc<InMemoryMemory>,
    sessions: Arc<InMemorySessions>,
    registry: Arc<sid::SessionRegistry>,
) -> Jojobot {
    Jojobot::new(
        memory,
        Arc::new(SpySearch::default()),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        sessions,
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        registry,
    )
}

/// **A client with no session affinity — a FRESH connection per tool call.**
///
/// This is what production clients actually present. The service factory
/// builds one handler per MCP session, so a client that does not hold one
/// across a conversation gets a new handler — and a new, empty binding —
/// for every single call. Both claude.ai and ChatGPT do exactly this:
/// the boot succeeds, and the journal on the very next call finds nobody
/// home.
///
/// **This stays in the suite permanently.** Every other test here holds a
/// handle across calls, which is the shape no real client has, and that is
/// the gap this whole class of bug shipped through.
pub(crate) struct NoAffinity {
    pub(crate) memory: Arc<InMemoryMemory>,
    pub(crate) sessions: Arc<InMemorySessions>,
    pub(crate) mailboxes: Arc<InMemoryMailboxes>,
    pub(crate) teachings: Arc<jojobot_domain::teaching::testing::InMemoryTeachings>,
    /// Process-wide, exactly as it is in production: the connections come
    /// and go, the handles this process issued do not.
    pub(crate) registry: Arc<sid::SessionRegistry>,
}

pub(crate) async fn journal_entry(jojobot: &Jojobot, sid: &str, entry: &str) -> serde_json::Value {
    let result = jojobot
        .journal(Parameters(JournalArgs {
            request_id: None,
            working_on: None,
            entry: entry.into(),
            focus: None,
            sid: sid.into(),
        }))
        .await
        .expect("journal call ok");
    let body = json_of(&result);
    assert_ne!(body["status"], "blocked", "the guard blocked: {body}");
    body
}

/// A session store whose **first `begin` commits and then fails** — the shape
/// a real store takes when its write lands and its read-back does not.
///
/// `put` is a write followed by a read: a dropped response or a transient fault
/// on the second call returns `Err` with the row already on the page. Nothing
/// distinguishes that from a write that never happened, so the caller retries
/// with the handle it still holds. This double is the only way to see it.
pub(crate) struct CommitsThenFails {
    pub(crate) inner: Arc<InMemorySessions>,
    pub(crate) failed_once: Mutex<bool>,
}

impl CommitsThenFails {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(InMemorySessions::new()),
            failed_once: Mutex::new(false),
        }
    }
}

#[async_trait]
impl Sessions for CommitsThenFails {
    async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
        // The write lands either way — that is the whole point.
        let begun = self.inner.begin(new).await?;
        let mut failed = self.failed_once.lock().expect("lock");
        if !*failed {
            *failed = true;
            return Err(SessionError::Store(
                "the row committed and the read-back did not".into(),
            ));
        }
        Ok(begun)
    }
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
        self.inner.sessions_of(bot).await
    }
    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
        self.inner.all_sessions().await
    }
    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.inner.read_session(id).await
    }
    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        self.inner.append(id, entry).await
    }
    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        self.inner.amend_last(id, text).await
    }
    async fn amend_beat(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        touched: jiff::Timestamp,
    ) -> Result<JournalEntry, SessionError> {
        self.inner.amend_beat(id, entry, text, touched).await
    }
    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        self.inner.set_focus(id, focus).await
    }
    async fn set_timezone(
        &self,
        id: &SessionId,
        timezone: Option<&str>,
    ) -> Result<Session, SessionError> {
        self.inner.set_timezone(id, timezone).await
    }
    async fn set_stated_day(
        &self,
        id: &SessionId,
        day: Option<jiff::civil::Date>,
    ) -> Result<Session, SessionError> {
        self.inner.set_stated_day(id, day).await
    }

    async fn set_wrap_window(
        &self,
        id: &SessionId,
        window: Option<WrapWindow>,
    ) -> Result<Session, SessionError> {
        self.inner.set_wrap_window(id, window).await
    }
    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        self.inner.close(id, to).await
    }
    async fn add_served(&self, id: &SessionId, chars: u64) -> Result<(), SessionError> {
        self.inner.add_served(id, chars).await
    }
    async fn reset_served(&self, id: &SessionId) -> Result<(), SessionError> {
        self.inner.reset_served(id).await
    }
    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.inner.reopen(id).await
    }

    async fn set_charter_told(
        &self,
        id: &SessionId,
        at: jiff::Timestamp,
    ) -> Result<(), SessionError> {
        self.inner.set_charter_told(id, at).await
    }

    async fn scan_session_history(
        &self,
    ) -> Result<
        jojobot_domain::session::history::SessionHistories,
        jojobot_domain::session::SessionError,
    > {
        self.inner.scan_session_history().await
    }

    async fn session_history(
        &self,
        id: &jojobot_domain::session::SessionId,
    ) -> Result<
        Vec<jojobot_domain::session::history::SessionChange>,
        jojobot_domain::session::SessionError,
    > {
        self.inner.session_history(id).await
    }

    async fn journal_history(
        &self,
        id: &jojobot_domain::session::SessionId,
        entry: &jojobot_domain::session::EntryId,
    ) -> Result<Vec<jojobot_domain::session::history::Wording>, jojobot_domain::session::SessionError>
    {
        self.inner.journal_history(id, entry).await
    }
    async fn set_key_hash(
        &self,
        id: &SessionId,
        hash: &str,
        trail: &jojobot_domain::session::KeyTrail,
    ) -> Result<(), SessionError> {
        self.inner.set_key_hash(id, hash, trail).await
    }
    async fn note_minted(
        &self,
        run: &jojobot_domain::session::MintedRun,
    ) -> Result<(), jojobot_domain::session::SessionError> {
        self.inner.note_minted(run).await
    }

    async fn minted_without_a_card(
        &self,
    ) -> Result<Vec<jojobot_domain::session::MintedRun>, jojobot_domain::session::SessionError>
    {
        self.inner.minted_without_a_card().await
    }

    async fn prune_minted(
        &self,
        before: jiff::Timestamp,
    ) -> Result<usize, jojobot_domain::session::SessionError> {
        self.inner.prune_minted(before).await
    }

    async fn note_charter_change(
        &self,
        bot: &EntityId,
        change: CharterChange,
        by_session: Option<&str>,
    ) -> Result<(), SessionError> {
        self.inner
            .note_charter_change(bot, change, by_session)
            .await
    }

    async fn charter_change_of(
        &self,
        bot: &EntityId,
    ) -> Result<Option<CharterChange>, SessionError> {
        self.inner.charter_change_of(bot).await
    }
}

/// A session store whose `close` refuses until it is told not to — the
/// transient failure a wrap is most likely to meet, and the only one that
/// leaves both writes already done.
pub(crate) struct RefusingClose {
    pub(crate) inner: InMemorySessions,
    pub(crate) refuse: std::sync::atomic::AtomicBool,
}

/// A handler over a store whose close refuses, and the handle a boot as
/// `gamma` hands back — the fixture both wrap-retry specs start from.
pub(crate) async fn refusing_close() -> (Jojobot, Arc<RefusingClose>, Arc<InMemoryMemory>, String) {
    let store = Arc::new(RefusingClose::new());
    let memory = Arc::new(InMemoryMemory::booted());
    let jojobot = Jojobot::new(
        memory.clone(),
        Arc::new(SpySearch::default()),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        store.clone(),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        crate::harness::seeded_registry(),
    );
    make_bot(&jojobot, "gamma").await;
    let sid = booted(&jojobot, "gamma").await;
    (jojobot, store, memory, sid)
}

/// **A session store whose `set_focus` fails while `append` works** — the
/// exact half-success a journal call met in production, and the only failure
/// on this surface that leaves the caller holding a committed write behind an
/// error.
pub(crate) struct RefusingFocus(pub(crate) InMemorySessions);

#[async_trait]
impl Sessions for RefusingFocus {
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
        self.0.sessions_of(bot).await
    }
    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
        self.0.all_sessions().await
    }
    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.0.read_session(id).await
    }
    async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
        self.0.begin(new).await
    }
    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        self.0.append(id, entry).await
    }
    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        self.0.amend_last(id, text).await
    }
    async fn amend_beat(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        touched: jiff::Timestamp,
    ) -> Result<JournalEntry, SessionError> {
        self.0.amend_beat(id, entry, text, touched).await
    }
    async fn set_focus(&self, _: &SessionId, _: &str) -> Result<Session, SessionError> {
        Err(SessionError::Store(
            "the focus cell on the page could not be written".into(),
        ))
    }
    async fn set_timezone(&self, _: &SessionId, _: Option<&str>) -> Result<Session, SessionError> {
        Err(SessionError::Store(
            "the timezone cell on the page could not be written".into(),
        ))
    }
    async fn set_stated_day(
        &self,
        id: &SessionId,
        day: Option<jiff::civil::Date>,
    ) -> Result<Session, SessionError> {
        self.0.set_stated_day(id, day).await
    }

    async fn set_wrap_window(
        &self,
        id: &SessionId,
        window: Option<WrapWindow>,
    ) -> Result<Session, SessionError> {
        self.0.set_wrap_window(id, window).await
    }
    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        self.0.close(id, to).await
    }
    async fn add_served(&self, id: &SessionId, chars: u64) -> Result<(), SessionError> {
        self.0.add_served(id, chars).await
    }
    async fn reset_served(&self, id: &SessionId) -> Result<(), SessionError> {
        self.0.reset_served(id).await
    }
    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.0.reopen(id).await
    }

    async fn set_charter_told(
        &self,
        id: &SessionId,
        at: jiff::Timestamp,
    ) -> Result<(), SessionError> {
        self.0.set_charter_told(id, at).await
    }

    async fn scan_session_history(
        &self,
    ) -> Result<
        jojobot_domain::session::history::SessionHistories,
        jojobot_domain::session::SessionError,
    > {
        self.0.scan_session_history().await
    }

    async fn session_history(
        &self,
        id: &jojobot_domain::session::SessionId,
    ) -> Result<
        Vec<jojobot_domain::session::history::SessionChange>,
        jojobot_domain::session::SessionError,
    > {
        self.0.session_history(id).await
    }

    async fn journal_history(
        &self,
        id: &jojobot_domain::session::SessionId,
        entry: &jojobot_domain::session::EntryId,
    ) -> Result<Vec<jojobot_domain::session::history::Wording>, jojobot_domain::session::SessionError>
    {
        self.0.journal_history(id, entry).await
    }
    async fn set_key_hash(
        &self,
        id: &SessionId,
        hash: &str,
        trail: &jojobot_domain::session::KeyTrail,
    ) -> Result<(), SessionError> {
        self.0.set_key_hash(id, hash, trail).await
    }
    async fn note_minted(
        &self,
        run: &jojobot_domain::session::MintedRun,
    ) -> Result<(), jojobot_domain::session::SessionError> {
        self.0.note_minted(run).await
    }

    async fn minted_without_a_card(
        &self,
    ) -> Result<Vec<jojobot_domain::session::MintedRun>, jojobot_domain::session::SessionError>
    {
        self.0.minted_without_a_card().await
    }

    async fn prune_minted(
        &self,
        before: jiff::Timestamp,
    ) -> Result<usize, jojobot_domain::session::SessionError> {
        self.0.prune_minted(before).await
    }

    async fn note_charter_change(
        &self,
        bot: &EntityId,
        change: CharterChange,
        by_session: Option<&str>,
    ) -> Result<(), SessionError> {
        self.0.note_charter_change(bot, change, by_session).await
    }

    async fn charter_change_of(
        &self,
        bot: &EntityId,
    ) -> Result<Option<CharterChange>, SessionError> {
        self.0.charter_change_of(bot).await
    }
}

/// **A session store that cannot clear a wrap window** while everything else
/// works — the failure that leaves a window standing after a newer run began.
pub(crate) struct RefusingWindowClear(pub(crate) InMemorySessions);

#[async_trait]
impl Sessions for RefusingWindowClear {
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
        self.0.sessions_of(bot).await
    }
    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
        self.0.all_sessions().await
    }
    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.0.read_session(id).await
    }
    async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
        self.0.begin(new).await
    }
    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        self.0.append(id, entry).await
    }
    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        self.0.amend_last(id, text).await
    }
    async fn amend_beat(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        touched: jiff::Timestamp,
    ) -> Result<JournalEntry, SessionError> {
        self.0.amend_beat(id, entry, text, touched).await
    }
    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        self.0.set_focus(id, focus).await
    }
    async fn set_timezone(
        &self,
        id: &SessionId,
        timezone: Option<&str>,
    ) -> Result<Session, SessionError> {
        self.0.set_timezone(id, timezone).await
    }
    async fn set_stated_day(
        &self,
        id: &SessionId,
        day: Option<jiff::civil::Date>,
    ) -> Result<Session, SessionError> {
        self.0.set_stated_day(id, day).await
    }

    async fn set_wrap_window(
        &self,
        id: &SessionId,
        window: Option<WrapWindow>,
    ) -> Result<Session, SessionError> {
        if window.is_none() {
            return Err(SessionError::Store(
                "the window cell on the page could not be cleared".into(),
            ));
        }
        self.0.set_wrap_window(id, window).await
    }
    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        self.0.close(id, to).await
    }
    async fn add_served(&self, id: &SessionId, chars: u64) -> Result<(), SessionError> {
        self.0.add_served(id, chars).await
    }
    async fn reset_served(&self, id: &SessionId) -> Result<(), SessionError> {
        self.0.reset_served(id).await
    }
    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.0.reopen(id).await
    }

    async fn set_charter_told(
        &self,
        id: &SessionId,
        at: jiff::Timestamp,
    ) -> Result<(), SessionError> {
        self.0.set_charter_told(id, at).await
    }

    async fn scan_session_history(
        &self,
    ) -> Result<
        jojobot_domain::session::history::SessionHistories,
        jojobot_domain::session::SessionError,
    > {
        self.0.scan_session_history().await
    }

    async fn session_history(
        &self,
        id: &jojobot_domain::session::SessionId,
    ) -> Result<
        Vec<jojobot_domain::session::history::SessionChange>,
        jojobot_domain::session::SessionError,
    > {
        self.0.session_history(id).await
    }

    async fn journal_history(
        &self,
        id: &jojobot_domain::session::SessionId,
        entry: &jojobot_domain::session::EntryId,
    ) -> Result<Vec<jojobot_domain::session::history::Wording>, jojobot_domain::session::SessionError>
    {
        self.0.journal_history(id, entry).await
    }
    async fn set_key_hash(
        &self,
        id: &SessionId,
        hash: &str,
        trail: &jojobot_domain::session::KeyTrail,
    ) -> Result<(), SessionError> {
        self.0.set_key_hash(id, hash, trail).await
    }
    async fn note_minted(
        &self,
        run: &jojobot_domain::session::MintedRun,
    ) -> Result<(), jojobot_domain::session::SessionError> {
        self.0.note_minted(run).await
    }

    async fn minted_without_a_card(
        &self,
    ) -> Result<Vec<jojobot_domain::session::MintedRun>, jojobot_domain::session::SessionError>
    {
        self.0.minted_without_a_card().await
    }

    async fn prune_minted(
        &self,
        before: jiff::Timestamp,
    ) -> Result<usize, jojobot_domain::session::SessionError> {
        self.0.prune_minted(before).await
    }

    async fn note_charter_change(
        &self,
        bot: &EntityId,
        change: CharterChange,
        by_session: Option<&str>,
    ) -> Result<(), SessionError> {
        self.0.note_charter_change(bot, change, by_session).await
    }

    async fn charter_change_of(
        &self,
        bot: &EntityId,
    ) -> Result<Option<CharterChange>, SessionError> {
        self.0.charter_change_of(bot).await
    }
}

/// **A session store that refuses one kind of write while everything else
/// works**, each refusal switched on and off by the case: a run's key, or the
/// reopening of a stopped run. Writing the word that voids a key still lands
/// while the key refusal is on, because taking a key away is what a failed
/// issue falls back to.
pub(crate) struct Refusing {
    pub(crate) inner: InMemorySessions,
    pub(crate) key_fails: std::sync::atomic::AtomicBool,
    pub(crate) reopen_fails: std::sync::atomic::AtomicBool,
    /// Whether the write that takes a run's key away is refused too.
    pub(crate) void_fails: std::sync::atomic::AtomicBool,
    /// Whether opening a wrap window is refused.
    pub(crate) open_fails: std::sync::atomic::AtomicBool,
}

impl Refusing {
    /// A store that refuses every key write from the start.
    pub(crate) fn refusing_keys() -> Self {
        Refusing {
            inner: InMemorySessions::new(),
            key_fails: true.into(),
            reopen_fails: false.into(),
            void_fails: false.into(),
            open_fails: false.into(),
        }
    }

    /// A store that refuses to reopen a stopped run from the start.
    pub(crate) fn refusing_reopens() -> Self {
        Refusing {
            inner: InMemorySessions::new(),
            key_fails: false.into(),
            reopen_fails: true.into(),
            void_fails: false.into(),
            open_fails: false.into(),
        }
    }

    /// A store that refuses nothing until the case switches a refusal on.
    pub(crate) fn refusing_nothing() -> Self {
        Refusing {
            inner: InMemorySessions::new(),
            key_fails: false.into(),
            reopen_fails: false.into(),
            void_fails: false.into(),
            open_fails: false.into(),
        }
    }
}

#[async_trait]
impl Sessions for Refusing {
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
        self.inner.sessions_of(bot).await
    }
    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
        self.inner.all_sessions().await
    }
    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.inner.read_session(id).await
    }
    async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
        self.inner.begin(new).await
    }
    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        self.inner.append(id, entry).await
    }
    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        self.inner.amend_last(id, text).await
    }
    async fn amend_beat(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        touched: jiff::Timestamp,
    ) -> Result<JournalEntry, SessionError> {
        self.inner.amend_beat(id, entry, text, touched).await
    }
    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        self.inner.set_focus(id, focus).await
    }
    async fn set_timezone(
        &self,
        id: &SessionId,
        timezone: Option<&str>,
    ) -> Result<Session, SessionError> {
        self.inner.set_timezone(id, timezone).await
    }
    async fn set_stated_day(
        &self,
        id: &SessionId,
        day: Option<jiff::civil::Date>,
    ) -> Result<Session, SessionError> {
        self.inner.set_stated_day(id, day).await
    }

    async fn set_wrap_window(
        &self,
        id: &SessionId,
        window: Option<WrapWindow>,
    ) -> Result<Session, SessionError> {
        if matches!(window, Some(WrapWindow::Open(_)))
            && self.open_fails.load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(SessionError::Store(
                "the window cell on the page could not be written".into(),
            ));
        }
        self.inner.set_wrap_window(id, window).await
    }
    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        self.inner.close(id, to).await
    }
    async fn add_served(&self, id: &SessionId, chars: u64) -> Result<(), SessionError> {
        self.inner.add_served(id, chars).await
    }
    async fn reset_served(&self, id: &SessionId) -> Result<(), SessionError> {
        self.inner.reset_served(id).await
    }
    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        if self.reopen_fails.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(SessionError::Store(
                "the state cell on the page could not be written".into(),
            ));
        }
        self.inner.reopen(id).await
    }

    async fn set_charter_told(
        &self,
        id: &SessionId,
        at: jiff::Timestamp,
    ) -> Result<(), SessionError> {
        self.inner.set_charter_told(id, at).await
    }

    async fn scan_session_history(
        &self,
    ) -> Result<
        jojobot_domain::session::history::SessionHistories,
        jojobot_domain::session::SessionError,
    > {
        self.inner.scan_session_history().await
    }

    async fn session_history(
        &self,
        id: &jojobot_domain::session::SessionId,
    ) -> Result<
        Vec<jojobot_domain::session::history::SessionChange>,
        jojobot_domain::session::SessionError,
    > {
        self.inner.session_history(id).await
    }

    async fn journal_history(
        &self,
        id: &jojobot_domain::session::SessionId,
        entry: &jojobot_domain::session::EntryId,
    ) -> Result<Vec<jojobot_domain::session::history::Wording>, jojobot_domain::session::SessionError>
    {
        self.inner.journal_history(id, entry).await
    }
    async fn set_key_hash(
        &self,
        id: &SessionId,
        hash: &str,
        trail: &jojobot_domain::session::KeyTrail,
    ) -> Result<(), SessionError> {
        let voiding = hash == jojobot_domain::session::KEY_VOID;
        let refusing = match voiding {
            true => self.void_fails.load(std::sync::atomic::Ordering::SeqCst),
            false => self.key_fails.load(std::sync::atomic::Ordering::SeqCst),
        };
        if !refusing {
            return self.inner.set_key_hash(id, hash, trail).await;
        }
        Err(SessionError::Store(
            "the key cell on the page could not be written".into(),
        ))
    }
    async fn note_minted(
        &self,
        run: &jojobot_domain::session::MintedRun,
    ) -> Result<(), jojobot_domain::session::SessionError> {
        self.inner.note_minted(run).await
    }

    async fn minted_without_a_card(
        &self,
    ) -> Result<Vec<jojobot_domain::session::MintedRun>, jojobot_domain::session::SessionError>
    {
        self.inner.minted_without_a_card().await
    }

    async fn prune_minted(
        &self,
        before: jiff::Timestamp,
    ) -> Result<usize, jojobot_domain::session::SessionError> {
        self.inner.prune_minted(before).await
    }

    async fn note_charter_change(
        &self,
        bot: &EntityId,
        change: CharterChange,
        by_session: Option<&str>,
    ) -> Result<(), SessionError> {
        self.inner
            .note_charter_change(bot, change, by_session)
            .await
    }

    async fn charter_change_of(
        &self,
        bot: &EntityId,
    ) -> Result<Option<CharterChange>, SessionError> {
        self.inner.charter_change_of(bot).await
    }
}

/// **A session store that says how often the runs were read.** `sessions_of`
/// and `all_sessions` return every run they match, so how many times one boot
/// reaches for them is the difference between one read shared by the steps
/// that need it and the same question asked of the store again.
pub(crate) struct CountingSessions {
    inner: InMemorySessions,
    reads: std::sync::atomic::AtomicUsize,
    /// How many times every run on the board was asked for as summaries.
    board_summaries: std::sync::atomic::AtomicUsize,
    /// How many more reads of a bot's runs are answered before the store
    /// refuses them. A store that never refuses holds `usize::MAX`.
    reads_left: std::sync::atomic::AtomicUsize,
}

impl CountingSessions {
    pub(crate) fn new(inner: InMemorySessions) -> Self {
        CountingSessions {
            inner,
            reads: std::sync::atomic::AtomicUsize::new(0),
            board_summaries: std::sync::atomic::AtomicUsize::new(0),
            reads_left: std::sync::atomic::AtomicUsize::new(usize::MAX),
        }
    }

    /// **Answer this many more reads of a bot's runs, then refuse every one**,
    /// the way a store that went away between two steps of one call does.
    pub(crate) fn refuse_runs_after(&self, reads: usize) {
        self.reads_left
            .store(reads, std::sync::atomic::Ordering::Release);
    }

    /// The store the fixtures stage runs in; staging is not counted by the
    /// reads below, which a test takes either side of the call it measures.
    pub(crate) fn store(&self) -> &InMemorySessions {
        &self.inner
    }

    /// How many whole-run reads there have been so far.
    pub(crate) fn reads(&self) -> usize {
        self.reads.load(std::sync::atomic::Ordering::Acquire)
    }

    /// How many times every run on the board has been read as summaries, so far.
    /// A read that answers for one bot, one run or the count of runs does not
    /// move it.
    pub(crate) fn board_summaries(&self) -> usize {
        self.board_summaries
            .load(std::sync::atomic::Ordering::Acquire)
    }

    fn counted(&self, runs: Vec<Session>) -> Vec<Session> {
        self.reads.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        runs
    }
}

#[async_trait]
impl Sessions for CountingSessions {
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
        let answered = self.reads_left.fetch_update(
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
            |left| match left {
                usize::MAX => Some(left),
                0 => None,
                left => Some(left - 1),
            },
        );
        if answered.is_err() {
            return Err(SessionError::Store("the runs could not be read".into()));
        }
        self.inner.sessions_of(bot).await.map(|r| self.counted(r))
    }
    async fn all_summaries(
        &self,
    ) -> Result<Vec<jojobot_domain::session::SessionSummary>, SessionError> {
        self.board_summaries
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        self.inner.all_summaries().await
    }
    /// Forwarded, as the layers over a real store forward them: left to the
    /// default, this double would answer from the whole board and count it.
    async fn summary_of(
        &self,
        id: &SessionId,
    ) -> Result<Option<jojobot_domain::session::SessionSummary>, SessionError> {
        self.inner.summary_of(id).await
    }
    async fn run_counts(
        &self,
    ) -> Result<std::collections::BTreeMap<EntityId, usize>, SessionError> {
        self.inner.run_counts().await
    }
    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
        self.inner.all_sessions().await.map(|r| self.counted(r))
    }
    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.inner.read_session(id).await
    }
    async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
        self.inner.begin(new).await
    }
    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        self.inner.append(id, entry).await
    }
    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        self.inner.amend_last(id, text).await
    }
    async fn amend_beat(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        touched: jiff::Timestamp,
    ) -> Result<JournalEntry, SessionError> {
        self.inner.amend_beat(id, entry, text, touched).await
    }
    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        self.inner.set_focus(id, focus).await
    }
    async fn set_timezone(
        &self,
        id: &SessionId,
        timezone: Option<&str>,
    ) -> Result<Session, SessionError> {
        self.inner.set_timezone(id, timezone).await
    }
    async fn set_stated_day(
        &self,
        id: &SessionId,
        day: Option<jiff::civil::Date>,
    ) -> Result<Session, SessionError> {
        self.inner.set_stated_day(id, day).await
    }
    async fn set_wrap_window(
        &self,
        id: &SessionId,
        window: Option<WrapWindow>,
    ) -> Result<Session, SessionError> {
        self.inner.set_wrap_window(id, window).await
    }
    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        self.inner.close(id, to).await
    }
    async fn add_served(&self, id: &SessionId, chars: u64) -> Result<(), SessionError> {
        self.inner.add_served(id, chars).await
    }
    async fn reset_served(&self, id: &SessionId) -> Result<(), SessionError> {
        self.inner.reset_served(id).await
    }
    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.inner.reopen(id).await
    }

    async fn set_charter_told(
        &self,
        id: &SessionId,
        at: jiff::Timestamp,
    ) -> Result<(), SessionError> {
        self.inner.set_charter_told(id, at).await
    }

    async fn scan_session_history(
        &self,
    ) -> Result<
        jojobot_domain::session::history::SessionHistories,
        jojobot_domain::session::SessionError,
    > {
        self.inner.scan_session_history().await
    }

    async fn session_history(
        &self,
        id: &jojobot_domain::session::SessionId,
    ) -> Result<
        Vec<jojobot_domain::session::history::SessionChange>,
        jojobot_domain::session::SessionError,
    > {
        self.inner.session_history(id).await
    }

    async fn journal_history(
        &self,
        id: &jojobot_domain::session::SessionId,
        entry: &jojobot_domain::session::EntryId,
    ) -> Result<Vec<jojobot_domain::session::history::Wording>, jojobot_domain::session::SessionError>
    {
        self.inner.journal_history(id, entry).await
    }
    async fn set_key_hash(
        &self,
        id: &SessionId,
        hash: &str,
        trail: &jojobot_domain::session::KeyTrail,
    ) -> Result<(), SessionError> {
        self.inner.set_key_hash(id, hash, trail).await
    }
    async fn note_minted(
        &self,
        run: &jojobot_domain::session::MintedRun,
    ) -> Result<(), jojobot_domain::session::SessionError> {
        self.inner.note_minted(run).await
    }

    async fn minted_without_a_card(
        &self,
    ) -> Result<Vec<jojobot_domain::session::MintedRun>, jojobot_domain::session::SessionError>
    {
        self.inner.minted_without_a_card().await
    }

    async fn prune_minted(
        &self,
        before: jiff::Timestamp,
    ) -> Result<usize, jojobot_domain::session::SessionError> {
        self.inner.prune_minted(before).await
    }

    async fn note_charter_change(
        &self,
        bot: &EntityId,
        change: CharterChange,
        by_session: Option<&str>,
    ) -> Result<(), SessionError> {
        self.inner
            .note_charter_change(bot, change, by_session)
            .await
    }

    async fn charter_change_of(
        &self,
        bot: &EntityId,
    ) -> Result<Option<CharterChange>, SessionError> {
        self.inner.charter_change_of(bot).await
    }
}

/// **A session store whose `append` fails** — the earlier half of a journal
/// call, and the one whose failure cannot say whether the write landed.
pub(crate) struct RefusingAppend(pub(crate) InMemorySessions);

#[async_trait]
impl Sessions for RefusingAppend {
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
        self.0.sessions_of(bot).await
    }
    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
        self.0.all_sessions().await
    }
    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.0.read_session(id).await
    }
    async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
        self.0.begin(new).await
    }
    async fn append(&self, _: &SessionId, _: NewEntry) -> Result<JournalEntry, SessionError> {
        Err(SessionError::Store(
            "the entry row on the page could not be written".into(),
        ))
    }
    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        self.0.amend_last(id, text).await
    }
    async fn amend_beat(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        touched: jiff::Timestamp,
    ) -> Result<JournalEntry, SessionError> {
        self.0.amend_beat(id, entry, text, touched).await
    }
    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        self.0.set_focus(id, focus).await
    }
    async fn set_timezone(
        &self,
        id: &SessionId,
        timezone: Option<&str>,
    ) -> Result<Session, SessionError> {
        self.0.set_timezone(id, timezone).await
    }
    async fn set_stated_day(
        &self,
        id: &SessionId,
        day: Option<jiff::civil::Date>,
    ) -> Result<Session, SessionError> {
        self.0.set_stated_day(id, day).await
    }

    async fn set_wrap_window(
        &self,
        id: &SessionId,
        window: Option<WrapWindow>,
    ) -> Result<Session, SessionError> {
        self.0.set_wrap_window(id, window).await
    }
    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        self.0.close(id, to).await
    }
    async fn add_served(&self, id: &SessionId, chars: u64) -> Result<(), SessionError> {
        self.0.add_served(id, chars).await
    }
    async fn reset_served(&self, id: &SessionId) -> Result<(), SessionError> {
        self.0.reset_served(id).await
    }
    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.0.reopen(id).await
    }

    async fn set_charter_told(
        &self,
        id: &SessionId,
        at: jiff::Timestamp,
    ) -> Result<(), SessionError> {
        self.0.set_charter_told(id, at).await
    }

    async fn scan_session_history(
        &self,
    ) -> Result<
        jojobot_domain::session::history::SessionHistories,
        jojobot_domain::session::SessionError,
    > {
        self.0.scan_session_history().await
    }

    async fn session_history(
        &self,
        id: &jojobot_domain::session::SessionId,
    ) -> Result<
        Vec<jojobot_domain::session::history::SessionChange>,
        jojobot_domain::session::SessionError,
    > {
        self.0.session_history(id).await
    }

    async fn journal_history(
        &self,
        id: &jojobot_domain::session::SessionId,
        entry: &jojobot_domain::session::EntryId,
    ) -> Result<Vec<jojobot_domain::session::history::Wording>, jojobot_domain::session::SessionError>
    {
        self.0.journal_history(id, entry).await
    }
    async fn set_key_hash(
        &self,
        id: &SessionId,
        hash: &str,
        trail: &jojobot_domain::session::KeyTrail,
    ) -> Result<(), SessionError> {
        self.0.set_key_hash(id, hash, trail).await
    }
    async fn note_minted(
        &self,
        run: &jojobot_domain::session::MintedRun,
    ) -> Result<(), jojobot_domain::session::SessionError> {
        self.0.note_minted(run).await
    }

    async fn minted_without_a_card(
        &self,
    ) -> Result<Vec<jojobot_domain::session::MintedRun>, jojobot_domain::session::SessionError>
    {
        self.0.minted_without_a_card().await
    }

    async fn prune_minted(
        &self,
        before: jiff::Timestamp,
    ) -> Result<usize, jojobot_domain::session::SessionError> {
        self.0.prune_minted(before).await
    }

    async fn note_charter_change(
        &self,
        bot: &EntityId,
        change: CharterChange,
        by_session: Option<&str>,
    ) -> Result<(), SessionError> {
        self.0.note_charter_change(bot, change, by_session).await
    }

    async fn charter_change_of(
        &self,
        bot: &EntityId,
    ) -> Result<Option<CharterChange>, SessionError> {
        self.0.charter_change_of(bot).await
    }
}

/// A session store that hands the runtime a chance to run the other task at
/// every call — what an HTTP round trip does, and what the in-memory fake
/// never does on its own.
///
/// **Without this the concurrency cases below prove nothing**: a fake that
/// never yields runs one whole verb before the other starts, so the two
/// futures never interleave and the race under test cannot happen.
pub(crate) struct Yielding(pub(crate) Arc<InMemorySessions>);

/// A handler whose session store yields at every call — see [`Yielding`].
pub(crate) fn racing(store: Arc<InMemorySessions>) -> Jojobot {
    Jojobot::new(
        Arc::new(InMemoryMemory::booted()),
        Arc::new(SpySearch::default()),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(Yielding(store)),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        crate::harness::seeded_registry(),
    )
}

/// `SessionState::Abandoned`, spelled once so the assertion above reads.
pub(crate) fn mailbox_state_abandoned() -> SessionState {
    SessionState::Abandoned
}

impl NoAffinity {
    pub(crate) fn new() -> Self {
        NoAffinity {
            memory: Arc::new(InMemoryMemory::booted()),
            sessions: Arc::new(InMemorySessions::new()),
            mailboxes: Arc::new(InMemoryMailboxes::knowing_any_owner()),
            teachings: Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            registry: crate::harness::seeded_registry(),
        }
    }

    /// One tool call, on a connection that has never seen another.
    pub(crate) fn call(&self) -> Jojobot {
        Jojobot::new(
            self.memory.clone(),
            Arc::new(SpySearch::default()),
            self.mailboxes.clone(),
            self.sessions.clone(),
            self.teachings.clone(),
            // **The one thing a reconnect must NOT rebuild.** A handle is
            // an address across connections or it is nothing.
            self.registry.clone(),
        )
    }
}

impl RefusingClose {
    pub(crate) fn new() -> Self {
        RefusingClose {
            inner: InMemorySessions::new(),
            refuse: std::sync::atomic::AtomicBool::new(true),
        }
    }
    pub(crate) fn allow_close(&self) {
        self.refuse
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

#[async_trait]
impl Sessions for RefusingClose {
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
        self.inner.sessions_of(bot).await
    }
    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
        self.inner.all_sessions().await
    }
    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.inner.read_session(id).await
    }
    async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
        self.inner.begin(new).await
    }
    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        self.inner.append(id, entry).await
    }
    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        self.inner.amend_last(id, text).await
    }
    async fn amend_beat(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        at: jiff::Timestamp,
    ) -> Result<JournalEntry, SessionError> {
        self.inner.amend_beat(id, entry, text, at).await
    }
    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        self.inner.set_focus(id, focus).await
    }
    async fn set_timezone(
        &self,
        id: &SessionId,
        timezone: Option<&str>,
    ) -> Result<Session, SessionError> {
        self.inner.set_timezone(id, timezone).await
    }
    async fn set_stated_day(
        &self,
        id: &SessionId,
        day: Option<jiff::civil::Date>,
    ) -> Result<Session, SessionError> {
        self.inner.set_stated_day(id, day).await
    }

    async fn set_wrap_window(
        &self,
        id: &SessionId,
        window: Option<WrapWindow>,
    ) -> Result<Session, SessionError> {
        self.inner.set_wrap_window(id, window).await
    }
    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        if self.refuse.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(SessionError::Store("the close failed in flight".into()));
        }
        self.inner.close(id, to).await
    }
    async fn add_served(&self, id: &SessionId, chars: u64) -> Result<(), SessionError> {
        self.inner.add_served(id, chars).await
    }
    async fn reset_served(&self, id: &SessionId) -> Result<(), SessionError> {
        self.inner.reset_served(id).await
    }
    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.inner.reopen(id).await
    }

    async fn set_charter_told(
        &self,
        id: &SessionId,
        at: jiff::Timestamp,
    ) -> Result<(), SessionError> {
        self.inner.set_charter_told(id, at).await
    }

    async fn scan_session_history(
        &self,
    ) -> Result<
        jojobot_domain::session::history::SessionHistories,
        jojobot_domain::session::SessionError,
    > {
        self.inner.scan_session_history().await
    }

    async fn session_history(
        &self,
        id: &jojobot_domain::session::SessionId,
    ) -> Result<
        Vec<jojobot_domain::session::history::SessionChange>,
        jojobot_domain::session::SessionError,
    > {
        self.inner.session_history(id).await
    }

    async fn journal_history(
        &self,
        id: &jojobot_domain::session::SessionId,
        entry: &jojobot_domain::session::EntryId,
    ) -> Result<Vec<jojobot_domain::session::history::Wording>, jojobot_domain::session::SessionError>
    {
        self.inner.journal_history(id, entry).await
    }
    async fn set_key_hash(
        &self,
        id: &SessionId,
        hash: &str,
        trail: &jojobot_domain::session::KeyTrail,
    ) -> Result<(), SessionError> {
        self.inner.set_key_hash(id, hash, trail).await
    }
    async fn note_minted(
        &self,
        run: &jojobot_domain::session::MintedRun,
    ) -> Result<(), jojobot_domain::session::SessionError> {
        self.inner.note_minted(run).await
    }

    async fn minted_without_a_card(
        &self,
    ) -> Result<Vec<jojobot_domain::session::MintedRun>, jojobot_domain::session::SessionError>
    {
        self.inner.minted_without_a_card().await
    }

    async fn prune_minted(
        &self,
        before: jiff::Timestamp,
    ) -> Result<usize, jojobot_domain::session::SessionError> {
        self.inner.prune_minted(before).await
    }

    async fn note_charter_change(
        &self,
        bot: &EntityId,
        change: CharterChange,
        by_session: Option<&str>,
    ) -> Result<(), SessionError> {
        self.inner
            .note_charter_change(bot, change, by_session)
            .await
    }

    async fn charter_change_of(
        &self,
        bot: &EntityId,
    ) -> Result<Option<CharterChange>, SessionError> {
        self.inner.charter_change_of(bot).await
    }
}

impl Yielding {
    pub(crate) async fn pause(&self) {
        tokio::task::yield_now().await;
    }
}

#[async_trait]
impl Sessions for Yielding {
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
        self.pause().await;
        self.0.sessions_of(bot).await
    }
    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
        self.0.all_sessions().await
    }
    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.pause().await;
        self.0.read_session(id).await
    }
    /// **Yields on both sides of the write, because reality does.** A real
    /// `begin` is a round trip: the card exists on the board the moment the
    /// server commits it, and the caller learns its id only when the
    /// response comes back. A double that suspends only on the way in never
    /// makes the board observable without its registry entry, which is the
    /// one interleaving worth being hostile about here.
    async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
        self.pause().await;
        let begun = self.0.begin(new).await;
        self.pause().await;
        begun
    }
    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        self.pause().await;
        self.0.append(id, entry).await
    }
    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        self.pause().await;
        self.0.amend_last(id, text).await
    }
    async fn amend_beat(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        at: jiff::Timestamp,
    ) -> Result<JournalEntry, SessionError> {
        self.pause().await;
        self.0.amend_beat(id, entry, text, at).await
    }
    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        self.pause().await;
        self.0.set_focus(id, focus).await
    }
    async fn set_timezone(
        &self,
        id: &SessionId,
        timezone: Option<&str>,
    ) -> Result<Session, SessionError> {
        self.pause().await;
        self.0.set_timezone(id, timezone).await
    }
    async fn set_stated_day(
        &self,
        id: &SessionId,
        day: Option<jiff::civil::Date>,
    ) -> Result<Session, SessionError> {
        self.pause().await;
        self.0.set_stated_day(id, day).await
    }

    async fn set_wrap_window(
        &self,
        id: &SessionId,
        window: Option<WrapWindow>,
    ) -> Result<Session, SessionError> {
        self.0.set_wrap_window(id, window).await
    }
    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        self.pause().await;
        self.0.close(id, to).await
    }
    async fn add_served(&self, id: &SessionId, chars: u64) -> Result<(), SessionError> {
        self.pause().await;
        self.0.add_served(id, chars).await
    }
    async fn reset_served(&self, id: &SessionId) -> Result<(), SessionError> {
        self.pause().await;
        self.0.reset_served(id).await
    }
    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.pause().await;
        self.0.reopen(id).await
    }

    async fn set_charter_told(
        &self,
        id: &SessionId,
        at: jiff::Timestamp,
    ) -> Result<(), SessionError> {
        self.0.set_charter_told(id, at).await
    }

    async fn scan_session_history(
        &self,
    ) -> Result<
        jojobot_domain::session::history::SessionHistories,
        jojobot_domain::session::SessionError,
    > {
        self.0.scan_session_history().await
    }

    async fn session_history(
        &self,
        id: &jojobot_domain::session::SessionId,
    ) -> Result<
        Vec<jojobot_domain::session::history::SessionChange>,
        jojobot_domain::session::SessionError,
    > {
        self.0.session_history(id).await
    }

    async fn journal_history(
        &self,
        id: &jojobot_domain::session::SessionId,
        entry: &jojobot_domain::session::EntryId,
    ) -> Result<Vec<jojobot_domain::session::history::Wording>, jojobot_domain::session::SessionError>
    {
        self.0.journal_history(id, entry).await
    }
    async fn set_key_hash(
        &self,
        id: &SessionId,
        hash: &str,
        trail: &jojobot_domain::session::KeyTrail,
    ) -> Result<(), SessionError> {
        self.0.set_key_hash(id, hash, trail).await
    }
    async fn note_minted(
        &self,
        run: &jojobot_domain::session::MintedRun,
    ) -> Result<(), jojobot_domain::session::SessionError> {
        self.0.note_minted(run).await
    }

    async fn minted_without_a_card(
        &self,
    ) -> Result<Vec<jojobot_domain::session::MintedRun>, jojobot_domain::session::SessionError>
    {
        self.0.minted_without_a_card().await
    }

    async fn prune_minted(
        &self,
        before: jiff::Timestamp,
    ) -> Result<usize, jojobot_domain::session::SessionError> {
        self.0.prune_minted(before).await
    }

    async fn note_charter_change(
        &self,
        bot: &EntityId,
        change: CharterChange,
        by_session: Option<&str>,
    ) -> Result<(), SessionError> {
        self.0.note_charter_change(bot, change, by_session).await
    }

    async fn charter_change_of(
        &self,
        bot: &EntityId,
    ) -> Result<Option<CharterChange>, SessionError> {
        self.0.charter_change_of(bot).await
    }
}

/// **A role's lease moment as it would stand `minutes` after its last
/// renewal**, written the way time would have left it, on the role's own
/// object. A write inside the renewal age renews nothing, so a case about a
/// renewal ages the lease first.
pub(crate) fn age_role_lease(
    memory: &jojobot_domain::memory::testing::InMemoryMemory,
    role: &str,
    minutes: i64,
) {
    let moment = jiff::Timestamp::now() - jiff::SignedDuration::from_mins(minutes);
    memory.fields_past_the_guard(
        &jojobot_domain::memory::EntityId(format!("role:{role}")),
        &[(
            jojobot_domain::session::ROLE_CLAIMED_AT,
            &moment.to_string(),
        )],
    );
}
