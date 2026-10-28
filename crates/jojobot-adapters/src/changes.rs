//! **A count of the writes that pass through a port**, so the search index can
//! tell that nothing changed without asking the store (decision log 292).
//!
//! Search keeps three halves current — memory, mail and sessions — and refreshes
//! each on every answer. Memory answers the question from its image. Mail and
//! sessions have no image of their own; what they have is that jojobot is the
//! only writer, so every change to what the index holds goes through the port,
//! and a port that counts its writes knows whether anything changed. The count
//! moves AFTER the write has landed in the store, so a reader that sees a number
//! reads at least the state it counts.
//!
//! ⚠️ **Only right with exactly one writer in the process**, for the reason the
//! image is. A row changed past this process is seen after a restart.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::writer::owned;
use async_trait::async_trait;
use jiff::Timestamp;
use jiff::civil::Date;
use jojobot_domain::mailbox::{
    Delivered, Delivery, Guarded, Mailbox, MailboxError, MailboxName, Mailboxes, Message,
    MessageId, NewMessage, Quarantined, TakenBy,
};
use jojobot_domain::memory::EntityId;
use jojobot_domain::session::{
    CharterChange, EntryId, JournalEntry, NewEntry, NewSession, RunTotals, Session, SessionError,
    SessionId, SessionState, SessionSummary, Sessions, WrapWindow,
};

/// A [`Mailboxes`] that counts every call that can change what the index holds
/// about mail: a box made or repointed, a message posted, delivered, read,
/// retired or quarantined. A call counts whatever it answers, because a write
/// that failed partway may have changed something, and one refresh too many
/// costs a board read where one too few costs a wrong hit.
pub struct MailChanges {
    inner: Arc<dyn Mailboxes>,
    changes: AtomicU64,
}

impl MailChanges {
    pub fn new(inner: Arc<dyn Mailboxes>) -> Self {
        MailChanges {
            inner,
            changes: AtomicU64::new(0),
        }
    }

    fn changed(&self) {
        self.changes.fetch_add(1, Ordering::SeqCst);
    }
}

#[async_trait]
impl Mailboxes for MailChanges {
    fn changes(&self) -> Option<u64> {
        Some(self.changes.load(Ordering::SeqCst))
    }

    async fn create_mailbox(
        &self,
        name: &MailboxName,
        owner: &EntityId,
        override_token: Option<&str>,
    ) -> Result<Guarded<Mailbox>, MailboxError> {
        let made = self.inner.create_mailbox(name, owner, override_token).await;
        self.changed();
        made
    }

    async fn repoint_owner(
        &self,
        from: &EntityId,
        to: &EntityId,
    ) -> Result<Option<Mailbox>, MailboxError> {
        let moved = self.inner.repoint_owner(from, to).await;
        self.changed();
        moved
    }

    async fn list_mailboxes(&self) -> Result<Vec<Mailbox>, MailboxError> {
        self.inner.list_mailboxes().await
    }

    async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
        self.inner.scan_messages().await
    }

    async fn message_by_id(&self, id: &MessageId) -> Result<Option<Message>, MailboxError> {
        self.inner.message_by_id(id).await
    }

    async fn messages_by_id(&self, ids: &[MessageId]) -> Result<Vec<Message>, MailboxError> {
        self.inner.messages_by_id(ids).await
    }

    async fn sent_by(&self, senders: &[&str]) -> Result<Vec<Message>, MailboxError> {
        self.inner.sent_by(senders).await
    }

    async fn post_message(&self, message: NewMessage) -> Result<Guarded<Message>, MailboxError> {
        let posted = self.inner.post_message(message).await;
        self.changed();
        posted
    }

    async fn read_mailbox(
        &self,
        name: &MailboxName,
        taken_by: TakenBy,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<Guarded<Delivery>, MailboxError> {
        let delivered = self.inner.read_mailbox(name, taken_by, acting).await;
        self.changed();
        delivered
    }

    async fn unchanged_delivery(
        &self,
        name: &MailboxName,
        taken_by: TakenBy,
    ) -> Result<Option<Delivery>, MailboxError> {
        self.inner.unchanged_delivery(name, taken_by).await
    }

    async fn read_message(
        &self,
        id: &MessageId,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<Delivered, MailboxError> {
        let read = self.inner.read_message(id, acting).await;
        self.changed();
        read
    }

    async fn mark_processed(
        &self,
        id: &MessageId,
        by: &jojobot_domain::mailbox::MailboxName,
        notes: Option<&str>,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<Message, MailboxError> {
        let retired = self.inner.mark_processed(id, by, notes, acting).await;
        self.changed();
        retired
    }

    async fn quarantine(
        &self,
        id: &MessageId,
        by: &MailboxName,
        reason: &str,
        acting: jojobot_domain::mailbox::Acting<'_>,
    ) -> Result<Quarantined, MailboxError> {
        let held = self.inner.quarantine(id, by, reason, acting).await;
        self.changed();
        held
    }

    async fn message_history(
        &self,
        id: &jojobot_domain::mailbox::MessageId,
    ) -> Result<Vec<jojobot_domain::mailbox::MessageEvent>, jojobot_domain::mailbox::MailboxError>
    {
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
        self.inner.scan_message_events().await
    }
}

/// A [`Sessions`] that answers `write_summary` with a count of its own writes.
///
/// **The store counts the rows it appends to `session_write` for a run begun, an
/// entry appended or amended, and a focus set — and for nothing else.** It
/// records every other change of a run too, but a run's state, zone, stated
/// day, wrap window, served count, key hash and charter marks are never indexed,
/// so changing one leaves the index current. This counts exactly those five
/// calls: counting more would send the index back to read every run after each
/// answer, which is what the signal exists to avoid.
/// The shared writer owns both the indexed write and this signal, even when
/// its caller stops waiting for the answer.
#[derive(Clone)]
pub struct SessionChanges {
    writer: crate::writer::Writer,
    inner: Arc<dyn Sessions>,
    changes: Arc<AtomicU64>,
}

impl SessionChanges {
    pub fn new(inner: Arc<dyn Sessions>, writer: crate::writer::Writer) -> Self {
        SessionChanges {
            writer,
            inner,
            changes: Arc::new(AtomicU64::new(0)),
        }
    }

    fn changed(&self) {
        self.changes.fetch_add(1, Ordering::SeqCst);
    }
}

#[async_trait]
impl Sessions for SessionChanges {
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
        self.inner.sessions_of(bot).await
    }

    async fn summaries_of(&self, bot: &EntityId) -> Result<Vec<SessionSummary>, SessionError> {
        self.inner.summaries_of(bot).await
    }

    async fn all_summaries(&self) -> Result<Vec<SessionSummary>, SessionError> {
        self.inner.all_summaries().await
    }

    async fn summary_of(&self, id: &SessionId) -> Result<Option<SessionSummary>, SessionError> {
        self.inner.summary_of(id).await
    }

    async fn run_counts(
        &self,
    ) -> Result<std::collections::BTreeMap<EntityId, usize>, SessionError> {
        self.inner.run_counts().await
    }

    async fn run_totals(&self) -> Result<Vec<RunTotals>, SessionError> {
        self.inner.run_totals().await
    }

    async fn served_days(&self) -> Result<Vec<jojobot_domain::usage::ServedDay>, SessionError> {
        self.inner.served_days().await
    }

    /// A count of calls changes nothing a search index holds, so it is not a
    /// change to signal.
    async fn count_call(&self, key: &jojobot_domain::usage::CallKey) -> Result<(), SessionError> {
        self.inner.count_call(key).await
    }

    async fn run_calls(
        &self,
        runs: Option<&[String]>,
    ) -> Result<Vec<jojobot_domain::usage::RunCall>, SessionError> {
        self.inner.run_calls(runs).await
    }

    async fn call_counts(&self) -> Result<Vec<jojobot_domain::usage::CallCount>, SessionError> {
        self.inner.call_counts().await
    }

    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
        self.inner.all_sessions().await
    }

    async fn write_summary(&self) -> Result<Option<(i64, Option<Timestamp>)>, SessionError> {
        Ok(Some((self.changes.load(Ordering::SeqCst) as i64, None)))
    }

    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.inner.read_session(id).await
    }

    async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
        owned!(self, this, [], this.begin(new).await);
        let begun = self.inner.begin(new).await;
        self.changed();
        begun
    }

    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        owned!(self, this, [id = id.clone()], this.append(&id, entry).await);
        let appended = self.inner.append(id, entry).await;
        self.changed();
        appended
    }

    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        owned!(
            self,
            this,
            [id = id.clone(), text = text.to_string()],
            this.amend_last(&id, &text).await
        );
        let amended = self.inner.amend_last(id, text).await;
        self.changed();
        amended
    }

    async fn amend_beat(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        at: Timestamp,
    ) -> Result<JournalEntry, SessionError> {
        owned!(
            self,
            this,
            [
                id = id.clone(),
                entry = entry.clone(),
                text = text.to_string()
            ],
            this.amend_beat(&id, &entry, &text, at).await
        );
        let amended = self.inner.amend_beat(id, entry, text, at).await;
        self.changed();
        amended
    }

    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        owned!(
            self,
            this,
            [id = id.clone(), focus = focus.to_string()],
            this.set_focus(&id, &focus).await
        );
        let set = self.inner.set_focus(id, focus).await;
        self.changed();
        set
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
        day: Option<Date>,
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
    async fn close_displaced(
        &self,
        id: &SessionId,
        by: &jojobot_domain::session::Sid,
    ) -> Result<Session, SessionError> {
        self.inner.close_displaced(id, by).await
    }

    async fn add_served(
        &self,
        id: &SessionId,
        chars: u64,
        day: jiff::civil::Date,
    ) -> Result<(), SessionError> {
        self.inner.add_served(id, chars, day).await
    }
    async fn reset_served(&self, id: &SessionId) -> Result<(), SessionError> {
        self.inner.reset_served(id).await
    }

    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.inner.reopen(id).await
    }

    async fn set_charter_told(&self, id: &SessionId, at: Timestamp) -> Result<(), SessionError> {
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
    ) -> Result<(), SessionError> {
        self.inner.note_minted(run).await
    }

    async fn minted_without_a_card(
        &self,
    ) -> Result<Vec<jojobot_domain::session::MintedRun>, SessionError> {
        self.inner.minted_without_a_card().await
    }

    async fn prune_minted(&self, before: jiff::Timestamp) -> Result<usize, SessionError> {
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
