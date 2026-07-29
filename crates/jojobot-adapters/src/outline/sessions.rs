//! The Sessions adapter — a bot's sessions live on **a page of its own, under
//! the bot's page**.
//!
//! One page per bot. On it, a table with one row per session (the model's *a
//! session is a row*) and, below, the chronology as one fenced block per entry.
//! The encoding and the reasons for it are in [`session_codec`](super::session_codec);
//! this module is the store around it.
//!
//! # Where the page sits, and what actually says whose it is
//!
//! The page is created **nested under the bot's own page** when the bot has one,
//! and at the top of the collection when it does not. Either way the `of:` line
//! in its machine block is what says whose sessions these are — the same rule
//! the entity tree already runs on: the line is the truth, the position is
//! navigability. That is what lets this context stay a context: Sessions never
//! reads Memory to answer a question, and a bot with no page yet still gets a
//! working session store rather than a refusal about an entity it never
//! mentioned.
//!
//! # The lock is Memory's lock, deliberately
//!
//! Every verb here is a read-modify-verify over a whole document, and so is
//! every verb in the Memory store next door. They write **different documents in
//! the same collection**, so two locks would exclude nobody — and creating a
//! sessions page is a write to the collection that Memory's own reads page
//! through. So a sessions store is built *from* a Memory store
//! ([`OutlineStore::sessions`](super::OutlineStore::sessions)) and shares its
//! mutex by construction, rather than being handed one and hoping.

use std::sync::Arc;

use async_trait::async_trait;
use jiff::Timestamp;

use jojobot_domain::memory::{EntityId, MemoryError};
use jojobot_domain::session::{
    EntryId, JournalEntry, NewEntry, NewSession, Session, SessionError, SessionId, SessionState,
    Sessions, normalize_entry, validate_entry, validate_focus, validate_session_id,
};

use super::api::{DocRec, OutlineApi};
use super::session_codec::{
    Row, next_entry_id, next_session_id, parse_bot, parse_entries, parse_rows, render_entry,
    seeded_page, with_entry_replaced, with_rows_replaced,
};
use super::{Restored, Workspace, parse_id_marker};

/// The title a sessions page is created with. Cosmetic, exactly as an entity
/// doc's title is: the `of:` line resolves the page, so an operator may rename
/// this freely.
const PAGE_TITLE: &str = "Sessions";

/// The real Sessions adapter, over Outline.
pub struct OutlineSessions {
    ws: Arc<Workspace>,
}

impl OutlineSessions {
    pub(super) fn new(ws: Arc<Workspace>) -> Self {
        Self { ws }
    }

    fn api(&self) -> &dyn OutlineApi {
        self.ws.api()
    }

    async fn collection(&self) -> Result<String, SessionError> {
        self.ws.resolve_collection().await.map_err(store)
    }

    /// Every sessions page in the collection, with the bot each belongs to.
    async fn pages(&self, collection_id: &str) -> Result<Vec<(EntityId, DocRec)>, SessionError> {
        Ok(self
            .ws
            .all_docs(collection_id)
            .await
            .map_err(store)?
            .into_iter()
            .filter_map(|d| parse_bot(&d.text).map(|bot| (bot, d)))
            .collect())
    }

    /// One bot's page, or `None` if it has never begun a session.
    ///
    /// **Oldest wins**, as everywhere else here: a concurrent double-create
    /// converges on one canonical page rather than forking a bot's history
    /// across two.
    async fn page_of(
        &self,
        collection_id: &str,
        bot: &EntityId,
    ) -> Result<Option<DocRec>, SessionError> {
        let mut mine: Vec<DocRec> = self
            .pages(collection_id)
            .await?
            .into_iter()
            .filter(|(b, _)| b == bot)
            .map(|(_, d)| d)
            .collect();
        mine.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(mine.into_iter().next())
    }

    /// Create a bot's sessions page, nested under the bot's own page when the
    /// bot has one. Only [`begin`](Sessions::begin) reaches this — nothing here
    /// creates a page as a side effect of a read.
    async fn create_page(
        &self,
        collection_id: &str,
        bot: &EntityId,
    ) -> Result<DocRec, SessionError> {
        let under = self
            .ws
            .all_docs(collection_id)
            .await
            .map_err(store)?
            .into_iter()
            .find(|d| parse_id_marker(&d.text).as_deref() == Some(bot.as_str()))
            .map(|d| d.id);
        self.api()
            .create_document(
                collection_id,
                PAGE_TITLE,
                &seeded_page(bot),
                under.as_deref(),
            )
            .await
            .map_err(store)?;
        self.page_of(collection_id, bot)
            .await?
            .ok_or_else(|| store_msg(format!("the sessions page for {bot} vanished after create")))
    }

    /// Assemble one page into the sessions it holds, chronologies attached.
    fn assemble(bot: &EntityId, doc: &DocRec) -> Vec<Session> {
        let entries = parse_entries(&doc.text);
        parse_rows(&doc.text)
            .into_iter()
            .map(|row| Session {
                entries: entries
                    .iter()
                    .filter(|(s, _)| s == &row.id)
                    .map(|(_, e)| e.clone())
                    .collect(),
                id: row.id,
                sid: row.sid,
                bot: bot.clone(),
                focus: row.focus,
                started_at: row.started_at,
                state: row.state,
            })
            .collect()
    }

    /// Find the page holding a session, and the session itself. The address is
    /// global and the page is not derivable from it, so this walks the pages —
    /// one listing call, the same one every other read here makes.
    async fn locate(
        &self,
        collection_id: &str,
        id: &SessionId,
    ) -> Result<(EntityId, DocRec, Session), SessionError> {
        validate_session_id(id)?;
        for (bot, doc) in self.pages(collection_id).await? {
            if let Some(session) = Self::assemble(&bot, &doc).into_iter().find(|s| &s.id == id) {
                return Ok((bot, doc, session));
            }
        }
        Err(SessionError::UnknownSession {
            attempted: id.to_string(),
        })
    }

    /// The refusal every write verb owes: a closed session takes nothing more.
    fn writable(session: &Session) -> Result<(), SessionError> {
        if session.state.is_terminal() {
            return Err(SessionError::Closed {
                attempted: session.id.to_string(),
                state: session.state,
            });
        }
        Ok(())
    }

    /// Write a whole page and read it back, restoring the page if what came
    /// back is not what went out.
    async fn put(&self, doc: &DocRec, text: &str, verb: &str) -> Result<DocRec, SessionError> {
        self.api()
            .update_document(&doc.id, text)
            .await
            .map_err(store)?;
        self.reread(doc, verb).await
    }

    /// Re-read a page through the read path. Every write here ends in one.
    async fn reread(&self, doc: &DocRec, verb: &str) -> Result<DocRec, SessionError> {
        let collection_id = self.collection().await?;
        self.ws
            .all_docs(&collection_id)
            .await
            .map_err(store)?
            .into_iter()
            .find(|d| d.id == doc.id)
            .ok_or_else(|| store_msg(format!("the sessions page vanished mid-{verb}")))
    }

    /// Put a page back the way a failed write found it — the same best-effort
    /// restore the Memory store makes, and reported the same way: whether the
    /// rollback worked is the one thing a caller cannot infer.
    /// Put the page back, and report what happened as a value — see
    /// [`super::Restored`].
    async fn restore(&self, doc: &DocRec) -> Restored {
        match self.api().update_document(&doc.id, &doc.text).await {
            Ok(()) => Restored::Undone,
            Err(e) => Restored::Failed(e.to_string()),
        }
    }

    /// The error a failed write becomes once the rollback has been attempted.
    /// One place decides which of the two it is, so four call sites cannot
    /// drift on what "stranded" means.
    async fn undo(
        &self,
        doc: &DocRec,
        verb: &str,
        stranded: Vec<String>,
        cause: String,
    ) -> SessionError {
        match self.restore(doc).await {
            Restored::Undone => store_msg(format!(
                "{verb} failed ({cause}); the page was restored to its state before it"
            )),
            Restored::Failed(rollback) => SessionError::Stranded {
                verb: verb.to_string(),
                stranded,
                cause,
                rollback,
            },
        }
    }

    /// The shared shape of every write that rewrites the sessions table: apply
    /// `edit` to the row, write the page, read it back, and restore on a
    /// mismatch.
    async fn rewrite_row(
        &self,
        id: &SessionId,
        verb: &str,
        edit: impl FnOnce(&mut Row),
    ) -> Result<Session, SessionError> {
        let _writing = self.ws.write().await;
        let collection_id = self.collection().await?;
        let (bot, doc, session) = self.locate(&collection_id, id).await?;
        if verb != "reopen" {
            Self::writable(&session)?;
        }

        let mut rows = parse_rows(&doc.text);
        let row = rows
            .iter_mut()
            .find(|r| &r.id == id)
            .ok_or_else(|| store_msg(format!("session {id} lost its row mid-{verb}")))?;
        edit(row);
        let wanted = row.clone();

        let updated = with_rows_replaced(&doc.text, &rows)
            .ok_or_else(|| store_msg(format!("the sessions page for {bot} has no table")))?;
        let seen = self.put(&doc, &updated, verb).await?;

        let back = Self::assemble(&bot, &seen)
            .into_iter()
            .find(|s| &s.id == id)
            .ok_or_else(|| store_msg(format!("session {id} did not read back after {verb}")))?;
        if back.state != wanted.state || back.focus != wanted.focus || back.sid != wanted.sid {
            return Err(self
                .undo(
                    &doc,
                    verb,
                    vec![id.to_string()],
                    format!("session {id} read back changed: wrote {wanted:?}, read {back:?}"),
                )
                .await);
        }
        Ok(back)
    }
}

/// Map a store failure into this context's vocabulary.
fn store(e: MemoryError) -> SessionError {
    match e {
        MemoryError::NotConfigured(m) => SessionError::NotConfigured(m),
        other => SessionError::Store(other.to_string()),
    }
}

fn store_msg(message: String) -> SessionError {
    SessionError::Store(message)
}

#[async_trait]
impl Sessions for OutlineSessions {
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
        let collection_id = self.collection().await?;
        let Some(doc) = self.page_of(&collection_id, bot).await? else {
            return Ok(Vec::new());
        };
        let mut found = Self::assemble(bot, &doc);
        // Newest start first, the id breaking a tie so two reads agree.
        found.sort_by(|a, b| {
            b.started_at
                .cmp(&a.started_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        Ok(found)
    }

    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
        let collection_id = self.collection().await?;
        Ok(self
            .pages(&collection_id)
            .await?
            .iter()
            .flat_map(|(bot, doc)| Self::assemble(bot, doc))
            .collect())
    }

    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
        let collection_id = self.collection().await?;
        Ok(self.locate(&collection_id, id).await?.2)
    }

    async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
        validate_focus(&new.focus)?;
        let _writing = self.ws.write().await;
        let collection_id = self.collection().await?;

        // The page is minted here or not at all — a read never brings one into
        // being, so a bot that never begins a session leaves no page behind.
        let doc = match self.page_of(&collection_id, &new.bot).await? {
            Some(doc) => doc,
            None => self.create_page(&collection_id, &new.bot).await?,
        };

        let mut rows = parse_rows(&doc.text);
        // **One handle, one run — checked before the append, under the write
        // lock.** `put` is a write followed by a read-back: if the write LANDS
        // and the read-back fails (a dropped response, a transient fault on the
        // second call), `begin` returns `Err` with the row already committed
        // and the caller still holding the handle it meant to attach. It
        // retries — and an unconditional append then puts a second run under
        // one handle, which is the identity the whole trace hangs from naming
        // two things. Handing the committed run back finishes what the first
        // attempt started, the same shape `wrap_session`'s retry uses.
        if let Some(held) = rows
            .iter()
            .find(|r| r.sid.as_ref() == Some(&new.sid) && !r.state.is_terminal())
        {
            let held = held.id.clone();
            return Self::assemble(&new.bot, &doc)
                .into_iter()
                .find(|s| s.id == held)
                .ok_or_else(|| store_msg(format!("session {held} did not read back")));
        }

        let row = Row {
            id: next_session_id(&doc.text, &new.bot),
            sid: Some(new.sid),
            started_at: new.started_at,
            state: SessionState::Active,
            focus: new.focus.trim().to_string(),
        };
        rows.push(row.clone());
        let updated = with_rows_replaced(&doc.text, &rows)
            .ok_or_else(|| store_msg(format!("the sessions page for {} has no table", new.bot)))?;
        let seen = self.put(&doc, &updated, "begin").await?;

        let back = Self::assemble(&new.bot, &seen)
            .into_iter()
            .find(|s| s.id == row.id)
            .ok_or_else(|| store_msg(format!("session {} did not read back", row.id)))?;
        if back.focus != row.focus || back.sid != row.sid || back.started_at != row.started_at {
            return Err(self
                .undo(
                    &doc,
                    "begin",
                    vec![row.id.to_string()],
                    format!(
                        "session {} read back changed: wrote {row:?}, read {back:?}",
                        row.id
                    ),
                )
                .await);
        }
        Ok(back)
    }

    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        validate_entry(&entry.text)?;
        let _writing = self.ws.write().await;
        let collection_id = self.collection().await?;
        let (_, doc, session) = self.locate(&collection_id, id).await?;
        Self::writable(&session)?;

        let written = JournalEntry {
            id: next_entry_id(&doc.text),
            at: entry.at,
            text: normalize_entry(&entry.text),
            touched: None,
            beat: entry.beat,
        };
        // **A genuine append**, not a read-modify-write: the page is not
        // rewritten, so nothing above can be lost to this and two appends
        // cannot clobber one another's block.
        self.api()
            .append_document(&doc.id, &render_entry(id, &written))
            .await
            .map_err(store)?;

        let seen = self.reread(&doc, "append").await?;
        let back = parse_entries(&seen.text)
            .into_iter()
            .find(|(s, e)| s == id && e.id == written.id)
            .map(|(_, e)| e)
            .ok_or_else(|| store_msg(format!("entry {} did not read back", written.id)))?;
        if back != written {
            return Err(self
                .undo(
                    &doc,
                    "append",
                    vec![written.id.to_string()],
                    format!(
                        "entry {} read back changed: wrote {written:?}, read {back:?}",
                        written.id
                    ),
                )
                .await);
        }
        Ok(back)
    }

    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        validate_entry(text)?;
        let _writing = self.ws.write().await;
        let collection_id = self.collection().await?;
        let (_, doc, session) = self.locate(&collection_id, id).await?;
        Self::writable(&session)?;

        let mut amended =
            session
                .entries
                .last()
                .cloned()
                .ok_or_else(|| SessionError::NoEntries {
                    attempted: id.to_string(),
                })?;
        amended.text = normalize_entry(text);
        self.rewrite_entry(&doc, id, &amended, "amend_last").await
    }

    async fn amend_beat(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        at: Timestamp,
    ) -> Result<JournalEntry, SessionError> {
        validate_entry(text)?;
        let _writing = self.ws.write().await;
        let collection_id = self.collection().await?;
        let (_, doc, session) = self.locate(&collection_id, id).await?;
        Self::writable(&session)?;

        let mut amended = session
            .entries
            .iter()
            .find(|e| &e.id == entry)
            .cloned()
            .ok_or_else(|| SessionError::NotABeat {
                attempted: entry.to_string(),
                session: id.to_string(),
            })?;
        if !amended.is_auto() {
            return Err(SessionError::NotABeat {
                attempted: entry.to_string(),
                session: id.to_string(),
            });
        }
        amended.text = normalize_entry(text);
        // The correction lands on `touched`, never on `at`: the beat keeps its
        // place in the chronology and the sweep still sees the session working.
        amended.touched = Some(at);
        self.rewrite_entry(&doc, id, &amended, "amend_beat").await
    }

    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        validate_focus(focus)?;
        let wanted = focus.trim().to_string();
        self.rewrite_row(id, "set_focus", |row| row.focus = wanted)
            .await
    }

    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        self.rewrite_row(id, "close", |row| row.state = to).await
    }

    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        let collection_id = self.collection().await?;
        let current = self.locate(&collection_id, id).await?.2;
        match current.state {
            // A caller resuming the run they are already in has made no mistake.
            SessionState::Active => return Ok(current),
            // The one end that is the last word.
            SessionState::Wrapped => {
                return Err(SessionError::Closed {
                    attempted: id.to_string(),
                    state: SessionState::Wrapped,
                });
            }
            SessionState::Abandoned => {}
        }
        self.rewrite_row(id, "reopen", |row| row.state = SessionState::Active)
            .await
    }
}

impl OutlineSessions {
    /// Rewrite one entry's block in place, then read it back. Shared by the two
    /// amends so they cannot come to disagree about what amending means.
    async fn rewrite_entry(
        &self,
        doc: &DocRec,
        session: &SessionId,
        amended: &JournalEntry,
        verb: &str,
    ) -> Result<JournalEntry, SessionError> {
        let updated = with_entry_replaced(&doc.text, session, amended)
            .ok_or_else(|| store_msg(format!("entry {} is not on the page", amended.id)))?;
        let seen = self.put(doc, &updated, verb).await?;

        let back = parse_entries(&seen.text)
            .into_iter()
            .find(|(s, e)| s == session && e.id == amended.id)
            .map(|(_, e)| e)
            .ok_or_else(|| store_msg(format!("entry {} did not read back", amended.id)))?;
        if &back != amended {
            return Err(self
                .undo(
                    doc,
                    verb,
                    vec![amended.id.to_string()],
                    format!(
                        "entry {} read back changed: wrote {amended:?}, read {back:?}",
                        amended.id
                    ),
                )
                .await);
        }
        Ok(back)
    }
}
