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
    /// While set, the reads that fill an image fail.
    unreachable: std::sync::atomic::AtomicBool,
    /// Set once: the next call of this name commits in the store and then never
    /// answers.
    hang_after: std::sync::Mutex<Option<&'static str>>,
    /// Told when a call has committed and begun to hang.
    hung: tokio::sync::Notify,
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

    /// **A store that cannot be reached for the reads that fill an image.** Those
    /// reads fail while `down` is set; every other call, writes included, goes
    /// through, so a case can fail a first fill and let the store recover.
    pub fn set_unreachable(&self, down: bool) {
        self.unreachable
            .store(down, std::sync::atomic::Ordering::SeqCst);
    }

    /// **The next call of `name` commits and its caller never hears.** The store
    /// writes, then the call stays pending, so a caller that stops waiting drops a
    /// future whose write already landed. Every write verb of the three ports
    /// hangs.
    pub fn hang_after_next(&self, name: &'static str) {
        *self.hang_after.lock().expect("count lock") = Some(name);
    }

    /// **Resolves once a call has committed and begun to hang.** A caller that
    /// drops the call then drops it after the commit, however long the store
    /// took to commit: a timeout would drop a slow store's write before it landed.
    pub async fn hung(&self) {
        self.hung.notified().await;
    }

    async fn hang_if_armed(&self, name: &'static str) {
        let armed = {
            let mut slot = self.hang_after.lock().expect("count lock");
            match *slot == Some(name) {
                true => slot.take().is_some(),
                false => false,
            }
        };
        if armed {
            self.hung.notify_one();
            std::future::pending::<()>().await;
        }
    }

    /// The message a fill read fails with while the store is unreachable.
    fn outage(&self) -> Result<(), String> {
        match self.unreachable.load(std::sync::atomic::Ordering::SeqCst) {
            true => Err("the store is unreachable".to_string()),
            false => Ok(()),
        }
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
        self.calls.outage().map_err(SessionError::Store)?;
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
        let landed = self.inner.begin(new).await;
        self.calls.hang_if_armed("begin").await;
        landed
    }
    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        self.calls.hit("append", Kind::Write);
        let landed = self.inner.append(id, entry).await;
        self.calls.hang_if_armed("append").await;
        landed
    }
    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        self.calls.hit("amend_last", Kind::Write);
        let landed = self.inner.amend_last(id, text).await;
        self.calls.hang_if_armed("amend_last").await;
        landed
    }
    async fn amend_beat(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        at: Timestamp,
    ) -> Result<JournalEntry, SessionError> {
        self.calls.hit("amend_beat", Kind::Write);
        let landed = self.inner.amend_beat(id, entry, text, at).await;
        self.calls.hang_if_armed("amend_beat").await;
        landed
    }
    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        self.calls.hit("set_focus", Kind::Write);
        let landed = self.inner.set_focus(id, focus).await;
        self.calls.hang_if_armed("set_focus").await;
        landed
    }
    async fn set_timezone(
        &self,
        id: &SessionId,
        timezone: Option<&str>,
    ) -> Result<Session, SessionError> {
        self.calls.hit("set_timezone", Kind::Write);
        let landed = self.inner.set_timezone(id, timezone).await;
        self.calls.hang_if_armed("set_timezone").await;
        landed
    }
    async fn set_stated_day(
        &self,
        id: &SessionId,
        day: Option<Date>,
    ) -> Result<Session, SessionError> {
        self.calls.hit("set_stated_day", Kind::Write);
        let landed = self.inner.set_stated_day(id, day).await;
        self.calls.hang_if_armed("set_stated_day").await;
        landed
    }
    async fn set_wrap_window(
        &self,
        id: &SessionId,
        window: Option<WrapWindow>,
    ) -> Result<Session, SessionError> {
        self.calls.hit("set_wrap_window", Kind::Write);
        let landed = self.inner.set_wrap_window(id, window).await;
        self.calls.hang_if_armed("set_wrap_window").await;
        landed
    }
    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        self.calls.hit("close", Kind::Write);
        let landed = self.inner.close(id, to).await;
        self.calls.hang_if_armed("close").await;
        landed
    }
    async fn add_served(&self, id: &SessionId, chars: u64) -> Result<(), SessionError> {
        self.calls.hit("add_served", Kind::Write);
        let landed = self.inner.add_served(id, chars).await;
        self.calls.hang_if_armed("add_served").await;
        landed
    }
    async fn reset_served(&self, id: &SessionId) -> Result<(), SessionError> {
        self.calls.hit("reset_served", Kind::Write);
        let landed = self.inner.reset_served(id).await;
        self.calls.hang_if_armed("reset_served").await;
        landed
    }
    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.calls.hit("reopen", Kind::Write);
        let landed = self.inner.reopen(id).await;
        self.calls.hang_if_armed("reopen").await;
        landed
    }
    async fn set_charter_told(&self, id: &SessionId, at: Timestamp) -> Result<(), SessionError> {
        self.calls.hit("set_charter_told", Kind::Write);
        let landed = self.inner.set_charter_told(id, at).await;
        self.calls.hang_if_armed("set_charter_told").await;
        landed
    }

    async fn scan_session_history(
        &self,
    ) -> Result<
        jojobot_domain::session::history::SessionHistories,
        jojobot_domain::session::SessionError,
    > {
        self.calls.hit("scan_session_history", Kind::Read);
        self.calls
            .outage()
            .map_err(jojobot_domain::session::SessionError::Store)?;
        self.inner.scan_session_history().await
    }

    async fn session_history(
        &self,
        id: &jojobot_domain::session::SessionId,
    ) -> Result<
        Vec<jojobot_domain::session::history::SessionChange>,
        jojobot_domain::session::SessionError,
    > {
        self.calls.hit("session_history", Kind::Read);
        self.inner.session_history(id).await
    }

    async fn journal_history(
        &self,
        id: &jojobot_domain::session::SessionId,
        entry: &jojobot_domain::session::EntryId,
    ) -> Result<Vec<jojobot_domain::session::history::Wording>, jojobot_domain::session::SessionError>
    {
        self.calls.hit("journal_history", Kind::Read);
        self.inner.journal_history(id, entry).await
    }
    async fn set_key_hash(
        &self,
        id: &SessionId,
        hash: &str,
        trail: &jojobot_domain::session::KeyTrail,
    ) -> Result<(), SessionError> {
        self.calls.hit("set_key_hash", Kind::Write);
        let landed = self.inner.set_key_hash(id, hash, trail).await;
        self.calls.hang_if_armed("set_key_hash").await;
        landed
    }
    async fn note_minted(
        &self,
        run: &jojobot_domain::session::MintedRun,
    ) -> Result<(), SessionError> {
        self.calls.hit("note_minted", Kind::Write);
        self.inner.note_minted(run).await
    }
    async fn minted_without_a_card(
        &self,
    ) -> Result<Vec<jojobot_domain::session::MintedRun>, SessionError> {
        self.calls.hit("minted_without_a_card", Kind::Read);
        self.inner.minted_without_a_card().await
    }
    async fn prune_minted(&self, before: jiff::Timestamp) -> Result<usize, SessionError> {
        self.calls.hit("prune_minted", Kind::Write);
        self.inner.prune_minted(before).await
    }
    async fn note_charter_change(
        &self,
        bot: &EntityId,
        change: CharterChange,
        by_session: Option<&str>,
    ) -> Result<(), SessionError> {
        self.calls.hit("note_charter_change", Kind::Write);
        let landed = self
            .inner
            .note_charter_change(bot, change, by_session)
            .await;
        self.calls.hang_if_armed("note_charter_change").await;
        landed
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
        self.calls.outage().map_err(SessionError::Store)?;
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
        let landed = self.inner.create_mailbox(name, owner, override_token).await;
        self.calls.hang_if_armed("create_mailbox").await;
        landed
    }
    async fn repoint_owner(
        &self,
        from: &EntityId,
        to: &EntityId,
    ) -> Result<Option<Mailbox>, MailboxError> {
        self.calls.hit("repoint_owner", Kind::Write);
        let landed = self.inner.repoint_owner(from, to).await;
        self.calls.hang_if_armed("repoint_owner").await;
        landed
    }
    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>, MailboxError> {
        self.calls.hit("list_mailboxes", Kind::Read);
        self.calls.outage().map_err(MailboxError::Store)?;
        self.inner.list_mailboxes().await
    }
    async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
        self.calls.hit("scan_messages", Kind::Read);
        self.calls.outage().map_err(MailboxError::Store)?;
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
        let landed = self.inner.post_message(message).await;
        self.calls.hang_if_armed("post_message").await;
        landed
    }
    async fn read_mailbox(
        &self,
        name: &MailboxName,
        taken_by: TakenBy,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<Guarded<Delivery>, MailboxError> {
        self.calls.hit("read_mailbox", Kind::Write);
        let landed = self.inner.read_mailbox(name, taken_by, acting).await;
        self.calls.hang_if_armed("read_mailbox").await;
        landed
    }
    async fn read_message(
        &self,
        id: &MessageId,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<Delivered, MailboxError> {
        self.calls.hit("read_message", Kind::Write);
        let landed = self.inner.read_message(id, acting).await;
        self.calls.hang_if_armed("read_message").await;
        landed
    }
    async fn mark_processed(
        &self,
        id: &MessageId,
        notes: Option<&str>,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<Message, MailboxError> {
        self.calls.hit("mark_processed", Kind::Write);
        let landed = self.inner.mark_processed(id, notes, acting).await;
        self.calls.hang_if_armed("mark_processed").await;
        landed
    }
    async fn quarantine(
        &self,
        id: &MessageId,
        by: &MailboxName,
        reason: &str,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<Quarantined, MailboxError> {
        self.calls.hit("quarantine", Kind::Write);
        let landed = self.inner.quarantine(id, by, reason, acting).await;
        self.calls.hang_if_armed("quarantine").await;
        landed
    }

    async fn message_history(
        &self,
        id: &jojobot_domain::mailbox::MessageId,
    ) -> Result<Vec<jojobot_domain::mailbox::MessageEvent>, jojobot_domain::mailbox::MailboxError>
    {
        self.calls.hit("message_history", Kind::Read);
        self.inner.message_history(id).await
    }

    async fn scan_message_events(
        &self,
    ) -> Result<
        Vec<(
            jojobot_domain::mailbox::MessageId,
            Vec<jojobot_domain::mailbox::MessageEvent>,
        )>,
        jojobot_domain::mailbox::MailboxError,
    > {
        self.calls.hit("scan_message_events", Kind::Read);
        self.calls
            .outage()
            .map_err(jojobot_domain::mailbox::MailboxError::Store)?;
        self.inner.scan_message_events().await
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
        let landed = self.inner.first_contact(sid, domain, at).await;
        self.calls.hang_if_armed("first_contact").await;
        landed
    }
    async fn has_contact(&self, sid: &Sid, domain: &str) -> Result<bool, TeachingError> {
        self.calls.hit("has_contact", Kind::Read);
        self.calls.outage().map_err(TeachingError::Store)?;
        self.inner.has_contact(sid, domain).await
    }
    async fn forget(&self, sid: &Sid) -> Result<(), TeachingError> {
        self.calls.hit("forget", Kind::Write);
        let landed = self.inner.forget(sid).await;
        self.calls.hang_if_armed("forget").await;
        landed
    }
    async fn taught(&self) -> Result<Option<Vec<(Sid, String)>>, TeachingError> {
        self.calls.hit("taught", Kind::Read);
        self.calls.outage().map_err(TeachingError::Store)?;
        self.inner.taught().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jojobot_domain::mailbox::NewMessage;
    use jojobot_domain::mailbox::testing::InMemoryMailboxes;

    /// **A read of a message's history is counted as a read.** A counted store
    /// that forwarded it unseen would let a case about reads pass over a call that
    /// reached the store.
    #[tokio::test]
    async fn a_read_of_a_messages_history_is_counted_as_a_read() {
        // A handle can be read only once the kind set is loaded, which a case in
        // this process may have unloaded.
        jojobot_domain::memory::kinds::load_shipped();
        let store = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let counted = CountedMailboxes::new(store);
        let owner = EntityId("bot:alpha".into());
        counted
            .create_mailbox(&MailboxName("alpha".into()), &owner, None)
            .await
            .expect("the box is made");
        let posted = counted
            .post_message(NewMessage {
                mailbox: MailboxName("alpha".into()),
                body: "a message".into(),
                subject: None,
                sender: "beta".into(),
                sent_at: jiff::Timestamp::UNIX_EPOCH,
                in_reply_to: None,
                sender_mail_waiting_at_send: None,
                posted_by_session: None,
                supersedes: None,
            })
            .await
            .expect("the post lands")
            .written()
            .expect("nothing blocks it");
        counted.calls.reset();

        counted
            .message_history(&posted.id)
            .await
            .expect("the history reads");

        assert_eq!(counted.calls.seen().get("message_history"), Some(&1));
        assert_eq!(counted.calls.reads(), 1, "{:?}", counted.calls.seen());
        assert_eq!(counted.calls.writes(), 0);
    }

    /// **The one read of every message's events is counted as a read**, for the
    /// reason the history of one message is.
    #[tokio::test]
    async fn a_scan_of_every_messages_events_is_counted_as_a_read() {
        let counted = CountedMailboxes::new(Arc::new(InMemoryMailboxes::knowing_any_owner()));

        counted
            .scan_message_events()
            .await
            .expect("the events scan");

        assert_eq!(counted.calls.seen().get("scan_message_events"), Some(&1));
        assert_eq!(counted.calls.reads(), 1, "{:?}", counted.calls.seen());
        assert_eq!(counted.calls.writes(), 0);
    }
}
