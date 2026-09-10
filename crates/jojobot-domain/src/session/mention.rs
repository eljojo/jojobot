//! **A handle written into a journal beat is a link, not a spelling** — the
//! same mechanism [`crate::memory::mention`] gives a claim's words, carried
//! onto the session world.
//!
//! A session's focus and every entry a session writes into its own
//! chronology are free text, exactly as a claim's content is. `Mentioning`
//! resolves an `@kind:slug` written into either into the badge its row
//! wears, so a rename that moves the handle reaches the journal too — a beat
//! written before a rename still reads under the thing's current name when
//! the chronology is read after one.
//!
//! **Resolved on the way in, rendered on the way out, and nothing else.**
//! A mention here is never screened, the same treatment a thing's own prose
//! already gets: a journal entry is not a claim about the thing it names.

use std::sync::Arc;

use crate::memory::{Entity, Memory, mention};

use super::{
    EntryId, JournalEntry, NewEntry, NewSession, Session, SessionError, SessionId, Sessions,
};

/// Wraps any [`Sessions`] so a handle written into a session's focus or its
/// chronology is stored as the permanent id and served as the handle the
/// thing wears today.
pub struct Mentioning {
    inner: Arc<dyn Sessions>,
    memory: Arc<dyn Memory>,
}

impl Mentioning {
    pub fn new(inner: Arc<dyn Sessions>, memory: Arc<dyn Memory>) -> Self {
        Mentioning { inner, memory }
    }

    /// What exists, for a mention to resolve against on the way in and
    /// render against on the way out — one read of the index answers both.
    async fn known(&self) -> Result<Vec<Entity>, SessionError> {
        self.memory
            .list_entities(None)
            .await
            .map_err(|e| SessionError::Store(e.to_string()))
    }

    fn render_entry(entry: &mut JournalEntry, known: &[Entity]) {
        entry.text = mention::rendered(&entry.text, known);
    }

    fn render_session(session: &mut Session, known: &[Entity]) {
        session.focus = mention::rendered(&session.focus, known);
        for entry in &mut session.entries {
            Self::render_entry(entry, known);
        }
    }

    async fn render(&self, session: &mut Session) -> Result<(), SessionError> {
        let known = self.known().await?;
        Self::render_session(session, &known);
        Ok(())
    }

    async fn render_many(&self, sessions: &mut [Session]) -> Result<(), SessionError> {
        if sessions.is_empty() {
            return Ok(());
        }
        let known = self.known().await?;
        for session in sessions {
            Self::render_session(session, &known);
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Sessions for Mentioning {
    async fn sessions_of(
        &self,
        bot: &crate::memory::EntityId,
    ) -> Result<Vec<Session>, SessionError> {
        let mut sessions = self.inner.sessions_of(bot).await?;
        self.render_many(&mut sessions).await?;
        Ok(sessions)
    }

    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
        let mut sessions = self.inner.all_sessions().await?;
        self.render_many(&mut sessions).await?;
        Ok(sessions)
    }

    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
        let mut session = self.inner.read_session(id).await?;
        self.render(&mut session).await?;
        Ok(session)
    }

    /// **Resolved on the way in.** The handle a caller wrote into the
    /// opening focus becomes the badge its row wears, so the card keeps a
    /// pointer rather than a spelling.
    async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
        let known = self.known().await?;
        let mut session = self
            .inner
            .begin(NewSession {
                focus: mention::resolved(&new.focus, &known),
                ..new
            })
            .await?;
        Self::render_session(&mut session, &known);
        Ok(session)
    }

    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        let known = self.known().await?;
        let mut written = self
            .inner
            .append(
                id,
                NewEntry {
                    text: mention::resolved(&entry.text, &known),
                    ..entry
                },
            )
            .await?;
        Self::render_entry(&mut written, &known);
        Ok(written)
    }

    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        let known = self.known().await?;
        let mut written = self
            .inner
            .amend_last(id, &mention::resolved(text, &known))
            .await?;
        Self::render_entry(&mut written, &known);
        Ok(written)
    }

    async fn amend_beat(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        at: jiff::Timestamp,
    ) -> Result<JournalEntry, SessionError> {
        let known = self.known().await?;
        let mut written = self
            .inner
            .amend_beat(id, entry, &mention::resolved(text, &known), at)
            .await?;
        Self::render_entry(&mut written, &known);
        Ok(written)
    }

    /// **Resolved on the way in, exactly as an entry's text is.**
    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        let known = self.known().await?;
        let mut session = self
            .inner
            .set_focus(id, &mention::resolved(focus, &known))
            .await?;
        Self::render_session(&mut session, &known);
        Ok(session)
    }

    async fn set_timezone(
        &self,
        id: &SessionId,
        timezone: Option<&str>,
    ) -> Result<Session, SessionError> {
        let mut session = self.inner.set_timezone(id, timezone).await?;
        self.render(&mut session).await?;
        Ok(session)
    }

    async fn close(
        &self,
        id: &SessionId,
        to: super::SessionState,
    ) -> Result<Session, SessionError> {
        let mut session = self.inner.close(id, to).await?;
        self.render(&mut session).await?;
        Ok(session)
    }

    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        let mut session = self.inner.reopen(id).await?;
        self.render(&mut session).await?;
        Ok(session)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use jiff::Timestamp;
    use jiff::civil::date;

    use super::*;
    use crate::memory::NewEntity;
    use crate::memory::testing::InMemoryMemory;
    use crate::session::testing::InMemorySessions;

    fn epoch() -> Timestamp {
        Timestamp::from_second(1_780_000_000).expect("a valid fixed instant")
    }

    async fn add(memory: &dyn Memory, id: &crate::memory::EntityId) {
        memory
            .add_entity(NewEntity::new(id.clone(), id.slug(), "contract-fixture"))
            .await
            .expect("add_entity should succeed")
            .written()
            .unwrap_or_else(|| panic!("the guard must not block a fresh handle {id}"));
    }

    #[tokio::test]
    async fn a_renamed_things_journal_mention_follows_it() {
        let memory: Arc<dyn Memory> = Arc::new(InMemoryMemory::booted());
        let was = crate::memory::EntityId("thing:contract-journal-mention-was".into());
        let now = crate::memory::EntityId("thing:contract-journal-mention-now".into());
        add(memory.as_ref(), &was).await;

        let bare = Arc::new(InMemorySessions::new());
        let sessions = Mentioning::new(bare.clone(), memory.clone());

        let bot = crate::memory::EntityId("bot:gamma".into());
        let session = sessions
            .begin(NewSession {
                bot: bot.clone(),
                sid: super::super::Sid("sid-1".to_string()),
                focus: format!("looking at @{was}"),
                started_at: epoch(),
                timezone: None,
                started_on: None,
            })
            .await
            .expect("begin should succeed");
        assert_eq!(session.focus, format!("looking at @{was}"));

        let entry = sessions
            .append(
                &session.id,
                NewEntry::manual(format!("found @{was}"), epoch(), None),
            )
            .await
            .expect("append should succeed");
        assert_eq!(entry.text, format!("found @{was}"));

        let refocused = sessions
            .set_focus(&session.id, &format!("still on @{was}"))
            .await
            .expect("set_focus should succeed");
        assert_eq!(refocused.focus, format!("still on @{was}"));

        let amended = sessions
            .amend_last(&session.id, &format!("corrected: @{was}"))
            .await
            .expect("amend_last should succeed");
        assert_eq!(amended.text, format!("corrected: @{was}"));

        memory
            .rename_entity(&was, &now, None, date(2026, 6, 2), None)
            .await
            .expect("rename_entity should succeed")
            .written()
            .expect("the guard must not block the rename");

        let read_back = sessions
            .read_session(&session.id)
            .await
            .expect("read_session should succeed");
        assert_eq!(read_back.focus, format!("still on @{now}"));
        assert_eq!(read_back.entries.len(), 1);
        assert_eq!(read_back.entries[0].text, format!("corrected: @{now}"));

        // The bare store never held either spelling — only the badge.
        let raw = bare
            .read_session(&session.id)
            .await
            .expect("read_session should succeed on the bare store");
        assert!(!raw.focus.contains(was.as_str()));
        assert!(!raw.focus.contains(now.as_str()));
        assert!(raw.focus.contains(mention::MARK));
        assert!(!raw.entries[0].text.contains(was.as_str()));
        assert!(!raw.entries[0].text.contains(now.as_str()));
        assert!(raw.entries[0].text.contains(mention::MARK));
    }

    #[tokio::test]
    async fn a_never_existed_handle_is_marked_but_not_screened() {
        let memory: Arc<dyn Memory> = Arc::new(InMemoryMemory::booted());
        let bare = Arc::new(InMemorySessions::new());
        let sessions = Mentioning::new(bare.clone(), memory);
        let bot = crate::memory::EntityId("bot:delta".into());
        let never = crate::memory::EntityId("thing:contract-journal-mention-never-existed".into());

        let session = sessions
            .begin(NewSession {
                bot,
                sid: super::super::Sid("sid-2".to_string()),
                focus: format!("about @{never}"),
                started_at: epoch(),
                timezone: None,
                started_on: None,
            })
            .await
            .expect("begin should succeed");
        assert_eq!(session.focus, format!("about @{never} (no such handle)"));

        let raw = bare
            .read_session(&session.id)
            .await
            .expect("read_session should succeed on the bare store");
        assert_eq!(raw.focus, format!("about @{never}"));
    }
}
