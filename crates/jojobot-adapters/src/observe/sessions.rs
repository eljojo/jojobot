//! Forward the Sessions port without losing its overrides.

use super::{Observed, Operation, SessionOperation};
use async_trait::async_trait;
use jiff::Timestamp;
use jiff::civil::Date;
use jojobot_domain::memory::EntityId;
use jojobot_domain::session::{
    CharterChange, EntryId, JournalEntry, NewEntry, NewSession, RunTotals, Session, SessionError,
    SessionId, SessionState, SessionSummary, Sessions, WrapWindow,
};
use std::collections::BTreeMap;

#[async_trait]
impl Sessions for Observed<dyn Sessions> {
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::SessionsOf),
            self.inner.sessions_of(bot),
        )
        .await
    }
    async fn summaries_of(&self, bot: &EntityId) -> Result<Vec<SessionSummary>, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::SummariesOf),
            self.inner.summaries_of(bot),
        )
        .await
    }
    async fn all_summaries(&self) -> Result<Vec<SessionSummary>, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::AllSummaries),
            self.inner.all_summaries(),
        )
        .await
    }
    async fn summary_of(&self, id: &SessionId) -> Result<Option<SessionSummary>, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::SummaryOf),
            self.inner.summary_of(id),
        )
        .await
    }
    async fn run_counts(&self) -> Result<BTreeMap<EntityId, usize>, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::RunCounts),
            self.inner.run_counts(),
        )
        .await
    }
    async fn run_totals(&self) -> Result<Vec<RunTotals>, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::RunTotals),
            self.inner.run_totals(),
        )
        .await
    }
    async fn served_days(&self) -> Result<Vec<jojobot_domain::usage::ServedDay>, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::ServedDays),
            self.inner.served_days(),
        )
        .await
    }
    async fn count_call(&self, key: &jojobot_domain::usage::CallKey) -> Result<(), SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::CountCall),
            self.inner.count_call(key),
        )
        .await
    }
    async fn run_calls(
        &self,
        runs: Option<&[String]>,
    ) -> Result<Vec<jojobot_domain::usage::RunCall>, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::RunCalls),
            self.inner.run_calls(runs),
        )
        .await
    }
    async fn call_counts(&self) -> Result<Vec<jojobot_domain::usage::CallCount>, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::CallCounts),
            self.inner.call_counts(),
        )
        .await
    }
    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::AllSessions),
            self.inner.all_sessions(),
        )
        .await
    }
    async fn write_summary(&self) -> Result<Option<(i64, Option<Timestamp>)>, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::WriteSummary),
            self.inner.write_summary(),
        )
        .await
    }
    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::ReadSession),
            self.inner.read_session(id),
        )
        .await
    }
    async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::Begin),
            self.inner.begin(new),
        )
        .await
    }
    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::Append),
            self.inner.append(id, entry),
        )
        .await
    }
    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::AmendLast),
            self.inner.amend_last(id, text),
        )
        .await
    }
    async fn amend_beat(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        at: Timestamp,
    ) -> Result<JournalEntry, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::AmendBeat),
            self.inner.amend_beat(id, entry, text, at),
        )
        .await
    }
    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::SetFocus),
            self.inner.set_focus(id, focus),
        )
        .await
    }
    async fn set_timezone(
        &self,
        id: &SessionId,
        timezone: Option<&str>,
    ) -> Result<Session, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::SetTimezone),
            self.inner.set_timezone(id, timezone),
        )
        .await
    }
    async fn set_stated_day(
        &self,
        id: &SessionId,
        day: Option<Date>,
    ) -> Result<Session, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::SetStatedDay),
            self.inner.set_stated_day(id, day),
        )
        .await
    }
    async fn set_wrap_window(
        &self,
        id: &SessionId,
        window: Option<WrapWindow>,
    ) -> Result<Session, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::SetWrapWindow),
            self.inner.set_wrap_window(id, window),
        )
        .await
    }
    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::Close),
            self.inner.close(id, to),
        )
        .await
    }
    async fn close_displaced(
        &self,
        id: &SessionId,
        by: &jojobot_domain::session::Sid,
    ) -> Result<Session, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::CloseDisplaced),
            self.inner.close_displaced(id, by),
        )
        .await
    }
    async fn add_served(
        &self,
        id: &SessionId,
        chars: u64,
        day: jiff::civil::Date,
    ) -> Result<(), SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::AddServed),
            self.inner.add_served(id, chars, day),
        )
        .await
    }
    async fn reset_served(&self, id: &SessionId) -> Result<(), SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::ResetServed),
            self.inner.reset_served(id),
        )
        .await
    }
    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::Reopen),
            self.inner.reopen(id),
        )
        .await
    }
    async fn set_charter_told(&self, id: &SessionId, at: Timestamp) -> Result<(), SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::SetCharterTold),
            self.inner.set_charter_told(id, at),
        )
        .await
    }
    async fn scan_session_history(
        &self,
    ) -> Result<
        jojobot_domain::session::history::SessionHistories,
        jojobot_domain::session::SessionError,
    > {
        self.result(
            Operation::Sessions(SessionOperation::ScanSessionHistory),
            self.inner.scan_session_history(),
        )
        .await
    }
    async fn session_history(
        &self,
        id: &jojobot_domain::session::SessionId,
    ) -> Result<
        Vec<jojobot_domain::session::history::SessionChange>,
        jojobot_domain::session::SessionError,
    > {
        self.result(
            Operation::Sessions(SessionOperation::SessionHistory),
            self.inner.session_history(id),
        )
        .await
    }
    async fn journal_history(
        &self,
        id: &jojobot_domain::session::SessionId,
        entry: &jojobot_domain::session::EntryId,
    ) -> Result<Vec<jojobot_domain::session::history::Wording>, jojobot_domain::session::SessionError>
    {
        self.result(
            Operation::Sessions(SessionOperation::JournalHistory),
            self.inner.journal_history(id, entry),
        )
        .await
    }
    async fn set_key_hash(
        &self,
        id: &SessionId,
        hash: &str,
        trail: &jojobot_domain::session::KeyTrail,
    ) -> Result<(), SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::SetKeyHash),
            self.inner.set_key_hash(id, hash, trail),
        )
        .await
    }
    async fn note_minted(
        &self,
        run: &jojobot_domain::session::MintedRun,
    ) -> Result<(), SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::NoteMinted),
            self.inner.note_minted(run),
        )
        .await
    }
    async fn minted_without_a_card(
        &self,
    ) -> Result<Vec<jojobot_domain::session::MintedRun>, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::MintedWithoutACard),
            self.inner.minted_without_a_card(),
        )
        .await
    }
    async fn prune_minted(&self, before: jiff::Timestamp) -> Result<usize, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::PruneMinted),
            self.inner.prune_minted(before),
        )
        .await
    }
    async fn note_charter_change(
        &self,
        bot: &EntityId,
        change: CharterChange,
        by_session: Option<&str>,
    ) -> Result<(), SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::NoteCharterChange),
            self.inner.note_charter_change(bot, change, by_session),
        )
        .await
    }
    async fn charter_change_of(
        &self,
        bot: &EntityId,
    ) -> Result<Option<CharterChange>, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::CharterChangeOf),
            self.inner.charter_change_of(bot),
        )
        .await
    }
    async fn charter_changes(
        &self,
    ) -> Result<Option<BTreeMap<EntityId, CharterChange>>, SessionError> {
        self.result(
            Operation::Sessions(SessionOperation::CharterChanges),
            self.inner.charter_changes(),
        )
        .await
    }
}
