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
use jojobot_domain::session::Sid;
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
    let choice = offered["session"]["choices"][0]["sid"]
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
        .mint(&EntityId::new(EntityKind::Bot, bot), Some(card.clone()))
        .expect("a free handle")
        .as_str()
        .to_string()
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
            bot: EntityId(format!("bot:{bot}")),
            sid: Sid(format!("t{:03}", hours_ago.rem_euclid(1000))),
            focus: focus.into(),
            started_at: jiff::Timestamp::now() - jiff::SignedDuration::from_hours(hours_ago),
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
pub(crate) fn with_sessions_port(sessions: Arc<dyn Sessions>) -> Jojobot {
    Jojobot::new(
        Arc::new(InMemoryMemory::new()),
        Arc::new(SpySearch::default()),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        sessions,
        Arc::new(sid::SessionRegistry::new()),
    )
}

/// A handler over a session store the test still holds a typed handle to.
pub(crate) fn with_sessions(sessions: Arc<InMemorySessions>) -> Jojobot {
    connection(Arc::new(InMemoryMemory::new()), sessions)
}

/// A second connection to the same worlds — what a reconnect or a device hop
/// builds. The binding is per handler, so this is the only way to test that
/// resuming reads the board rather than remembering anything.
pub(crate) fn connection(memory: Arc<InMemoryMemory>, sessions: Arc<InMemorySessions>) -> Jojobot {
    connection_sharing(memory, sessions, Arc::new(sid::SessionRegistry::new()))
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
    /// Process-wide, exactly as it is in production: the connections come
    /// and go, the handles this process issued do not.
    pub(crate) registry: Arc<sid::SessionRegistry>,
}

pub(crate) async fn journal_entry(jojobot: &Jojobot, sid: &str, entry: &str) -> serde_json::Value {
    let result = jojobot
        .journal(Parameters(JournalArgs {
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
    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        self.inner.close(id, to).await
    }
    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.inner.reopen(id).await
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
    let memory = Arc::new(InMemoryMemory::new());
    let jojobot = Jojobot::new(
        memory.clone(),
        Arc::new(SpySearch::default()),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        store.clone(),
        Arc::new(sid::SessionRegistry::new()),
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
        Err(SessionError::Store("the focus could not be written".into()))
    }
    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        self.0.close(id, to).await
    }
    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.0.reopen(id).await
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
        Err(SessionError::Store("the entry could not be written".into()))
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
    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        self.0.close(id, to).await
    }
    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.0.reopen(id).await
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
        Arc::new(InMemoryMemory::new()),
        Arc::new(SpySearch::default()),
        Arc::new(InMemoryMailboxes::knowing_any_owner()),
        Arc::new(Yielding(store)),
        Arc::new(sid::SessionRegistry::new()),
    )
}

/// `SessionState::Abandoned`, spelled once so the assertion above reads.
pub(crate) fn mailbox_state_abandoned() -> SessionState {
    SessionState::Abandoned
}

impl NoAffinity {
    pub(crate) fn new() -> Self {
        NoAffinity {
            memory: Arc::new(InMemoryMemory::new()),
            sessions: Arc::new(InMemorySessions::new()),
            mailboxes: Arc::new(InMemoryMailboxes::knowing_any_owner()),
            registry: Arc::new(sid::SessionRegistry::new()),
        }
    }

    /// One tool call, on a connection that has never seen another.
    pub(crate) fn call(&self) -> Jojobot {
        Jojobot::new(
            self.memory.clone(),
            Arc::new(SpySearch::default()),
            self.mailboxes.clone(),
            self.sessions.clone(),
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
    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        if self.refuse.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(SessionError::Store("the close failed in flight".into()));
        }
        self.inner.close(id, to).await
    }
    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.inner.reopen(id).await
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
    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        self.pause().await;
        self.0.close(id, to).await
    }
    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.pause().await;
        self.0.reopen(id).await
    }
}
