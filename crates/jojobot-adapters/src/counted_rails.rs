//! [`CountedSessions`], [`CountedMailboxes`] and [`CountedTeachings`] — ports
//! that count every call reaching the store beneath them, and yield where the
//! store yields.
//!
//! The in-memory image of sessions, mail and teachings (decision log 292)
//! promises that a read after startup issues no call to the store. A call is
//! counted at the port, where the image sits above it. Each call is a **read**
//! or a **write**, because a write goes to the store by design and a read must
//! not. The count is only ever read next to an answer compared with the store's
//! own (see [`crate::fold_rails_battery`]): zero calls with a wrong answer
//! proves nothing.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use jiff::Timestamp;
use jiff::civil::Date;
use jojobot_domain::mailbox::{
    Delivered, Delivery, Guarded, Mailbox, MailboxError, MailboxName, Mailboxes, Message,
    MessageId, NewMessage, Quarantined, TakenBy,
};
use jojobot_domain::memory::EntityId;
use jojobot_domain::session::{
    CharterChange, EntryId, JournalEntry, NewEntry, NewSession, Session, SessionError, SessionId,
    SessionState, SessionSummary, Sessions, Sid, WrapWindow,
};
use jojobot_domain::teaching::{TeachingError, Teachings};

/// How a counted call reaches the store.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Read,
    Write,
}

/// The calls made, by name, since the last [`reset`](Calls::reset).
#[derive(Default)]
pub struct Calls {
    seen: std::sync::Mutex<BTreeMap<&'static str, (Kind, usize)>>,
}

impl Calls {
    fn hit(&self, name: &'static str, kind: Kind) {
        self.seen
            .lock()
            .expect("count lock")
            .entry(name)
            .or_insert((kind, 0))
            .1 += 1;
    }

    fn total(&self, kind: Kind) -> usize {
        self.seen
            .lock()
            .expect("count lock")
            .values()
            .filter(|(k, _)| *k == kind)
            .map(|(_, n)| n)
            .sum()
    }

    /// Reads made since the last reset.
    pub fn reads(&self) -> usize {
        self.total(Kind::Read)
    }

    /// Writes made since the last reset.
    pub fn writes(&self) -> usize {
        self.total(Kind::Write)
    }

    /// Every call since the last reset, by name — what a failed assertion
    /// prints, so a reader sees WHICH call reached the store.
    pub fn seen(&self) -> BTreeMap<&'static str, usize> {
        self.seen
            .lock()
            .expect("count lock")
            .iter()
            .map(|(name, (_, n))| (*name, *n))
            .collect()
    }

    pub fn reset(&self) {
        self.seen.lock().expect("count lock").clear();
    }
}

/// A sessions store that counts the calls made to it.
pub struct CountedSessions {
    inner: Arc<dyn Sessions>,
    pub calls: Calls,
}

impl CountedSessions {
    pub fn new(inner: Arc<dyn Sessions>) -> Self {
        CountedSessions {
            inner,
            calls: Calls::default(),
        }
    }
}

#[async_trait]
impl Sessions for CountedSessions {
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
        self.calls.hit("sessions_of", Kind::Read);
        self.inner.sessions_of(bot).await
    }
    async fn summaries_of(&self, bot: &EntityId) -> Result<Vec<SessionSummary>, SessionError> {
        self.calls.hit("summaries_of", Kind::Read);
        self.inner.summaries_of(bot).await
    }
    async fn all_summaries(&self) -> Result<Vec<SessionSummary>, SessionError> {
        self.calls.hit("all_summaries", Kind::Read);
        self.inner.all_summaries().await
    }
    async fn summary_of(&self, id: &SessionId) -> Result<Option<SessionSummary>, SessionError> {
        self.calls.hit("summary_of", Kind::Read);
        self.inner.summary_of(id).await
    }
    async fn run_counts(&self) -> Result<BTreeMap<EntityId, usize>, SessionError> {
        self.calls.hit("run_counts", Kind::Read);
        self.inner.run_counts().await
    }
    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
        self.calls.hit("all_sessions", Kind::Read);
        self.inner.all_sessions().await
    }
    async fn write_summary(&self) -> Result<Option<(i64, Option<Timestamp>)>, SessionError> {
        self.calls.hit("write_summary", Kind::Read);
        self.inner.write_summary().await
    }
    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.calls.hit("read_session", Kind::Read);
        self.inner.read_session(id).await
    }
    async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
        self.calls.hit("begin", Kind::Write);
        self.inner.begin(new).await
    }
    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        self.calls.hit("append", Kind::Write);
        self.inner.append(id, entry).await
    }
    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        self.calls.hit("amend_last", Kind::Write);
        self.inner.amend_last(id, text).await
    }
    async fn amend_beat(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        at: Timestamp,
    ) -> Result<JournalEntry, SessionError> {
        self.calls.hit("amend_beat", Kind::Write);
        self.inner.amend_beat(id, entry, text, at).await
    }
    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        self.calls.hit("set_focus", Kind::Write);
        self.inner.set_focus(id, focus).await
    }
    async fn set_timezone(
        &self,
        id: &SessionId,
        timezone: Option<&str>,
    ) -> Result<Session, SessionError> {
        self.calls.hit("set_timezone", Kind::Write);
        self.inner.set_timezone(id, timezone).await
    }
    async fn set_stated_day(
        &self,
        id: &SessionId,
        day: Option<Date>,
    ) -> Result<Session, SessionError> {
        self.calls.hit("set_stated_day", Kind::Write);
        self.inner.set_stated_day(id, day).await
    }
    async fn set_wrap_window(
        &self,
        id: &SessionId,
        window: Option<WrapWindow>,
    ) -> Result<Session, SessionError> {
        self.calls.hit("set_wrap_window", Kind::Write);
        self.inner.set_wrap_window(id, window).await
    }
    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        self.calls.hit("close", Kind::Write);
        self.inner.close(id, to).await
    }
    async fn add_served(&self, id: &SessionId, chars: u64) -> Result<(), SessionError> {
        self.calls.hit("add_served", Kind::Write);
        self.inner.add_served(id, chars).await
    }
    async fn reset_served(&self, id: &SessionId) -> Result<(), SessionError> {
        self.calls.hit("reset_served", Kind::Write);
        self.inner.reset_served(id).await
    }
    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.calls.hit("reopen", Kind::Write);
        self.inner.reopen(id).await
    }
    async fn set_charter_told(&self, id: &SessionId, at: Timestamp) -> Result<(), SessionError> {
        self.calls.hit("set_charter_told", Kind::Write);
        self.inner.set_charter_told(id, at).await
    }
    async fn set_key_hash(&self, id: &SessionId, hash: &str) -> Result<(), SessionError> {
        self.calls.hit("set_key_hash", Kind::Write);
        self.inner.set_key_hash(id, hash).await
    }
    async fn note_charter_change(
        &self,
        bot: &EntityId,
        change: CharterChange,
    ) -> Result<(), SessionError> {
        self.calls.hit("note_charter_change", Kind::Write);
        self.inner.note_charter_change(bot, change).await
    }
    async fn charter_change_of(
        &self,
        bot: &EntityId,
    ) -> Result<Option<CharterChange>, SessionError> {
        self.calls.hit("charter_change_of", Kind::Read);
        self.inner.charter_change_of(bot).await
    }
    async fn charter_changes(
        &self,
    ) -> Result<Option<BTreeMap<EntityId, CharterChange>>, SessionError> {
        self.calls.hit("charter_changes", Kind::Read);
        self.inner.charter_changes().await
    }
}

/// A mail store that counts the calls made to it.
pub struct CountedMailboxes {
    inner: Arc<dyn Mailboxes>,
    pub calls: Calls,
}

impl CountedMailboxes {
    pub fn new(inner: Arc<dyn Mailboxes>) -> Self {
        CountedMailboxes {
            inner,
            calls: Calls::default(),
        }
    }
}

#[async_trait]
impl Mailboxes for CountedMailboxes {
    fn changes(&self) -> Option<u64> {
        self.inner.changes()
    }
    async fn create_mailbox(
        &self,
        name: &MailboxName,
        owner: &EntityId,
        override_token: Option<&str>,
    ) -> Result<Guarded<Mailbox>, MailboxError> {
        self.calls.hit("create_mailbox", Kind::Write);
        self.inner.create_mailbox(name, owner, override_token).await
    }
    async fn repoint_owner(
        &self,
        from: &EntityId,
        to: &EntityId,
    ) -> Result<Option<Mailbox>, MailboxError> {
        self.calls.hit("repoint_owner", Kind::Write);
        self.inner.repoint_owner(from, to).await
    }
    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>, MailboxError> {
        self.calls.hit("list_mailboxes", Kind::Read);
        self.inner.list_mailboxes().await
    }
    async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
        self.calls.hit("scan_messages", Kind::Read);
        self.inner.scan_messages().await
    }
    async fn message_by_id(&self, id: &MessageId) -> Result<Option<Message>, MailboxError> {
        self.calls.hit("message_by_id", Kind::Read);
        self.inner.message_by_id(id).await
    }
    async fn sent_by(&self, senders: &[&str]) -> Result<Vec<Message>, MailboxError> {
        self.calls.hit("sent_by", Kind::Read);
        self.inner.sent_by(senders).await
    }
    async fn post_message(&self, message: NewMessage) -> Result<Guarded<Message>, MailboxError> {
        self.calls.hit("post_message", Kind::Write);
        self.inner.post_message(message).await
    }
    async fn read_mailbox(
        &self,
        name: &MailboxName,
        taken_by: TakenBy,
    ) -> Result<Guarded<Delivery>, MailboxError> {
        self.calls.hit("read_mailbox", Kind::Write);
        self.inner.read_mailbox(name, taken_by).await
    }
    async fn read_message(&self, id: &MessageId) -> Result<Delivered, MailboxError> {
        self.calls.hit("read_message", Kind::Write);
        self.inner.read_message(id).await
    }
    async fn mark_processed(
        &self,
        id: &MessageId,
        notes: Option<&str>,
    ) -> Result<Message, MailboxError> {
        self.calls.hit("mark_processed", Kind::Write);
        self.inner.mark_processed(id, notes).await
    }
    async fn quarantine(
        &self,
        id: &MessageId,
        by: &MailboxName,
        reason: &str,
        at: Timestamp,
    ) -> Result<Quarantined, MailboxError> {
        self.calls.hit("quarantine", Kind::Write);
        self.inner.quarantine(id, by, reason, at).await
    }
}

/// A teaching ledger that counts the calls made to it.
pub struct CountedTeachings {
    inner: Arc<dyn Teachings>,
    pub calls: Calls,
}

impl CountedTeachings {
    pub fn new(inner: Arc<dyn Teachings>) -> Self {
        CountedTeachings {
            inner,
            calls: Calls::default(),
        }
    }
}

#[async_trait]
impl Teachings for CountedTeachings {
    async fn first_contact(
        &self,
        sid: &Sid,
        domain: &str,
        at: Timestamp,
    ) -> Result<bool, TeachingError> {
        self.calls.hit("first_contact", Kind::Write);
        self.inner.first_contact(sid, domain, at).await
    }
    async fn forget(&self, sid: &Sid) -> Result<(), TeachingError> {
        self.calls.hit("forget", Kind::Write);
        self.inner.forget(sid).await
    }
    async fn taught(&self) -> Result<Option<Vec<(Sid, String)>>, TeachingError> {
        self.calls.hit("taught", Kind::Read);
        self.inner.taught().await
    }
}
