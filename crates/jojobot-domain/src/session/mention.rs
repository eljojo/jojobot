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

use crate::memory::{
    Entity, EntityId, FormerHandle, Memory, entity_wearing, mention, resolve_handle,
};

use super::{
    EntryId, JournalEntry, NewEntry, NewSession, Session, SessionError, SessionId, Sessions,
};

/// Wraps any [`Sessions`] so a handle written into a session's focus or its
/// chronology is stored as the permanent id and served as the handle the
/// thing wears today — **and so is `bot` itself**, the same rule applied to
/// the one column that names an entity outright rather than through text.
/// Without this, a session outlives the name it was born under and a rename
/// strands every session already on the board: `sessions_of` stops finding
/// them (it still queries the OLD handle) and every attribution through a
/// live `sid` keeps reading as the identity that just stopped answering to
/// that name.
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

    /// Every rename this store still remembers — needed only to resolve a
    /// `bot` handle that may itself be stale (a `sid` whose in-process
    /// registry was not, for whatever reason, updated live). Rendering never
    /// needs this: [`entity_wearing`] finds the current wearer of a badge
    /// directly, whatever it used to be called.
    async fn former(&self) -> Result<Vec<FormerHandle>, SessionError> {
        self.memory
            .former_handles()
            .await
            .map_err(|e| SessionError::Store(e.to_string()))
    }

    /// **The key `bot` is stored under**: the badge of whatever it resolves
    /// to, or `bot` itself unchanged when it does not resolve at all, or
    /// resolves to a row with no badge yet (a fake store, or a row from
    /// before badges existed) — the same fallback the real store's own
    /// pointer columns use, so a session is never blocked on this.
    fn storage_key_for(bot: &EntityId, known: &[Entity], former: &[FormerHandle]) -> EntityId {
        match resolve_handle(bot, known, former) {
            Some(entity) => entity
                .badge
                .clone()
                .map(EntityId)
                .unwrap_or_else(|| entity.id.clone()),
            None => bot.clone(),
        }
    }

    /// **The handle `bot` renders as today**: whoever currently wears the
    /// stored badge, or the stored value unchanged when nobody does — a
    /// value stored before this existed, which is a plain handle rather
    /// than a badge, and reads back exactly as it did before.
    fn current_handle_for(stored: &EntityId, known: &[Entity]) -> EntityId {
        entity_wearing(stored.as_str(), known)
            .map(|e| e.id.clone())
            .unwrap_or_else(|| stored.clone())
    }

    fn render_entry(entry: &mut JournalEntry, known: &[Entity]) {
        entry.text = mention::rendered(&entry.text, known);
    }

    fn render_session(session: &mut Session, known: &[Entity]) {
        session.bot = Self::current_handle_for(&session.bot, known);
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
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
        let known = self.known().await?;
        let former = self.former().await?;
        let key = Self::storage_key_for(bot, &known, &former);
        let mut sessions = self.inner.sessions_of(&key).await?;
        for session in &mut sessions {
            Self::render_session(session, &known);
        }
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
        let former = self.former().await?;
        let bot = Self::storage_key_for(&new.bot, &known, &former);
        let mut session = self
            .inner
            .begin(NewSession {
                bot,
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

        let beat = sessions
            .append(
                &session.id,
                NewEntry::beat("captured", format!("captured: @{was}"), epoch(), None),
            )
            .await
            .expect("append of a beat should succeed");
        let beat_amended = sessions
            .amend_beat(
                &session.id,
                &beat.id,
                &format!("captured twice: @{was}"),
                epoch(),
            )
            .await
            .expect("amend_beat should succeed");
        assert_eq!(beat_amended.text, format!("captured twice: @{was}"));

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
        assert_eq!(read_back.entries.len(), 2);
        assert_eq!(read_back.entries[0].text, format!("corrected: @{now}"));
        assert_eq!(read_back.entries[1].text, format!("captured twice: @{now}"));

        // The bare store never held either spelling — only the badge.
        let raw = bare
            .read_session(&session.id)
            .await
            .expect("read_session should succeed on the bare store");
        assert!(!raw.focus.contains(was.as_str()));
        assert!(!raw.focus.contains(now.as_str()));
        assert!(raw.focus.contains(mention::MARK));
        assert!(!raw.entries[1].text.contains(was.as_str()));
        assert!(!raw.entries[1].text.contains(now.as_str()));
        assert!(raw.entries[1].text.contains(mention::MARK));
        assert!(!raw.entries[0].text.contains(was.as_str()));
        assert!(!raw.entries[0].text.contains(now.as_str()));
        assert!(raw.entries[0].text.contains(mention::MARK));
    }

    /// 🚨 **A renamed bot's OWN sessions stay found, under either name — and
    /// a session begun after the rename lands under the SAME row as the ones
    /// begun before it.**
    ///
    /// This is `bot` itself, not a mention written into free text: the
    /// column `sessions_of`'s `WHERE bot = ?` reads directly. Without
    /// resolving it through a badge, a rename leaves every existing session
    /// row un-findable by the new handle (the row still says the old one)
    /// and every later session begun under the new handle unable to be
    /// grouped with them (a fresh badge chosen by chance would never match
    /// the old handle's rows) — proven by both directions at once, since
    /// either alone could pass by accident.
    #[tokio::test]
    async fn a_renamed_bots_sessions_are_found_under_either_name() {
        let memory: Arc<dyn Memory> = Arc::new(InMemoryMemory::booted());
        let was = crate::memory::EntityId("bot:gamma".into());
        let now = crate::memory::EntityId("bot:sigma".into());
        add(memory.as_ref(), &was).await;

        let bare = Arc::new(InMemorySessions::new());
        let sessions = Mentioning::new(bare.clone(), memory.clone());

        let before = sessions
            .begin(NewSession {
                bot: was.clone(),
                sid: super::super::Sid("sid-3".to_string()),
                focus: "before the rename".into(),
                started_at: epoch(),
                timezone: None,
                started_on: None,
            })
            .await
            .expect("begin should succeed");

        memory
            .rename_entity(&was, &now, None, date(2026, 6, 2), None)
            .await
            .expect("rename_entity should succeed")
            .written()
            .expect("the guard must not block the rename");

        let after = sessions
            .begin(NewSession {
                bot: now.clone(),
                sid: super::super::Sid("sid-4".to_string()),
                focus: "after the rename".into(),
                started_at: epoch(),
                timezone: None,
                started_on: None,
            })
            .await
            .expect("begin should succeed");

        // Both directions: the OLD handle still finds them (a caller who has
        // not yet learned of the rename), and the NEW handle finds them too
        // (the ordinary case) — and it is the same two rows either way.
        for bot in [&was, &now] {
            let found = sessions
                .sessions_of(bot)
                .await
                .unwrap_or_else(|e| panic!("sessions_of({bot}) should succeed: {e}"));
            let ids: std::collections::BTreeSet<_> = found.iter().map(|s| s.id.clone()).collect();
            assert_eq!(
                ids,
                [before.id.clone(), after.id.clone()].into_iter().collect(),
                "sessions_of({bot}) must find both of this bot's runs: {found:?}",
            );
            assert!(
                found.iter().all(|s| s.bot == now),
                "every session renders bot as the CURRENT handle, whichever name found it: \
                 {found:?}",
            );
        }

        // The bare store never held either handle as a plain string — only
        // the one badge, shared by both rows.
        let raw_before = bare
            .read_session(&before.id)
            .await
            .expect("read_session should succeed on the bare store");
        let raw_after = bare
            .read_session(&after.id)
            .await
            .expect("read_session should succeed on the bare store");
        assert_eq!(
            raw_before.bot, raw_after.bot,
            "one badge for both rows of one bot, whichever name begat them",
        );
        assert_ne!(
            raw_before.bot, was,
            "the bare row must not hold the handle itself"
        );
        assert_ne!(
            raw_before.bot, now,
            "the bare row must not hold the handle itself"
        );
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
