//! **Sessions, as rows.**
//!
//! A session is one row and its chronology is rows under it, ordered by an
//! ordinal the store assigns. That is what a table is for, and it is why this
//! moved: the append-only rule is a `MAX(ordinal)` rather than a position in a
//! rewritten page, and "only the newest is amendable" is a predicate rather
//! than a convention.
//!
//! **What this adapter does NOT carry, and the absence is the point.** No
//! read-back guard, no golden fixture, no linearization lock, no escaping. Each
//! existed because a document editor rewrites prose that passes through it; a
//! SQL store hands back the bytes it was given, and a transaction either
//! commits or does not.
//!
//! **Timestamps are stored as text, deliberately.** `DATETIME(6)` is
//! microseconds and the domain's instants are nanoseconds, so a column would
//! silently truncate and a record would not read back as it was written. The
//! sweep compares instants, so that truncation is behaviour, not formatting.

use async_trait::async_trait;
use jiff::Timestamp;
use jojobot_domain::memory::EntityId;
use jojobot_domain::session::{
    EntryId, JournalEntry, NewEntry, NewSession, Session, SessionError, SessionId, SessionState,
    Sessions, Sid, normalize_entry, validate_entry, validate_focus, validate_session_id,
};
use sqlx::{MySql, MySqlPool, Row, Transaction};

/// Sessions kept in the SQL store jojobot runs.
///
/// Cloning shares the one pool rather than opening a second: a pool is the
/// connection budget, and two of them against one server is two budgets
/// nobody set.
#[derive(Clone)]
pub struct DoltSessions {
    pool: MySqlPool,
}

impl DoltSessions {
    /// Open the store over an existing pool.
    ///
    /// **The schema is not this adapter's to create.** It arrives through the
    /// migrations the server applies on start, so there is one place a table's
    /// shape is decided and one order it changes in — see
    /// [`crate::dolt::migrate`].
    pub fn open(pool: MySqlPool) -> Self {
        DoltSessions { pool }
    }

    /// Read one whole session inside a transaction, or say it is not there.
    ///
    /// **One reader for every verb**, so the row and its chronology can never
    /// come back assembled two different ways.
    async fn read_in(
        tx: &mut Transaction<'_, MySql>,
        id: &SessionId,
    ) -> Result<Session, SessionError> {
        let row =
            sqlx::query("SELECT id, sid, bot, focus, started_at, state FROM session WHERE id = ?")
                .bind(id.as_str())
                .fetch_optional(&mut **tx)
                .await
                .map_err(store)?
                .ok_or_else(|| SessionError::UnknownSession {
                    attempted: id.to_string(),
                })?;
        let entries = sqlx::query(
            "SELECT id, at, text, touched, beat FROM journal_entry
             WHERE session = ? ORDER BY ordinal",
        )
        .bind(id.as_str())
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        session_from(&row, &entries)
    }

    /// The session a write is allowed to touch: it exists, and it is open.
    ///
    /// One helper for every write verb, so they cannot come to disagree about
    /// what closed means — the same reason the fake has one.
    async fn writable(
        tx: &mut Transaction<'_, MySql>,
        id: &SessionId,
    ) -> Result<Session, SessionError> {
        let session = Self::read_in(tx, id).await?;
        if session.state.is_terminal() {
            return Err(SessionError::Closed {
                attempted: id.to_string(),
                state: session.state,
            });
        }
        Ok(session)
    }

    /// The next ordinal in a session's chronology.
    ///
    /// Read inside the write's own transaction, so two appends cannot pick the
    /// same one — the ordering is the record, and two entries sharing a place
    /// in it is a chronology that no longer says what happened first.
    async fn next_ordinal(
        tx: &mut Transaction<'_, MySql>,
        id: &SessionId,
    ) -> Result<i32, SessionError> {
        let row = sqlx::query(
            "SELECT COALESCE(MAX(ordinal), 0) + 1 FROM journal_entry WHERE session = ?",
        )
        .bind(id.as_str())
        .fetch_one(&mut **tx)
        .await
        .map_err(store)?;
        row.try_get::<i32, _>(0).map_err(store)
    }
}

/// A store failure, in the domain's own words. **The server's account never
/// crosses** — no SQL, no table names, no product (rule 53); it goes to the log
/// where an operator debugging a real failure wants it.
fn store(e: sqlx::Error) -> SessionError {
    tracing::error!(error = %e, "the session store failed");
    SessionError::Store("the session store could not be reached".into())
}

/// One row plus its entries, as the domain's record.
fn session_from(
    row: &sqlx::mysql::MySqlRow,
    entries: &[sqlx::mysql::MySqlRow],
) -> Result<Session, SessionError> {
    let state: String = row.try_get("state").map_err(store)?;
    let started: String = row.try_get("started_at").map_err(store)?;
    let sid: Option<String> = row.try_get("sid").map_err(store)?;
    Ok(Session {
        id: SessionId(row.try_get::<String, _>("id").map_err(store)?),
        sid: sid.map(Sid),
        bot: EntityId(row.try_get::<String, _>("bot").map_err(store)?),
        focus: row.try_get::<String, _>("focus").map_err(store)?,
        started_at: instant(&started)?,
        // A state token the store does not recognize is a record jojobot
        // cannot read. It is not a session in an unknown column — there are no
        // columns here — so it is a store fault a person repairs.
        state: SessionState::from_token(&state).ok_or_else(|| {
            SessionError::Store(format!("a session row carries the state '{state}'"))
        })?,
        entries: entries.iter().map(entry_from).collect::<Result<_, _>>()?,
    })
}

/// One chronology row, as the domain's entry.
fn entry_from(row: &sqlx::mysql::MySqlRow) -> Result<JournalEntry, SessionError> {
    let at: String = row.try_get("at").map_err(store)?;
    let touched: Option<String> = row.try_get("touched").map_err(store)?;
    Ok(JournalEntry {
        id: EntryId(row.try_get::<String, _>("id").map_err(store)?),
        at: instant(&at)?,
        text: row.try_get::<String, _>("text").map_err(store)?,
        touched: touched.as_deref().map(instant).transpose()?,
        beat: row.try_get::<Option<String>, _>("beat").map_err(store)?,
    })
}

/// Parse a stored instant. A cell that is no instant is a record jojobot
/// cannot read rather than a value to guess at.
fn instant(raw: &str) -> Result<Timestamp, SessionError> {
    raw.parse().map_err(|_| {
        tracing::error!(cell = %raw, "a session row carries a timestamp that is no timestamp");
        SessionError::Store("a session row carries a timestamp jojobot cannot read".into())
    })
}

/// An instant as it is stored: RFC 3339, nanoseconds intact.
fn stamp(at: Timestamp) -> String {
    at.to_string()
}

#[async_trait]
impl Sessions for DoltSessions {
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM session WHERE bot = ? ORDER BY started_at DESC, id DESC",
        )
        .bind(bot.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(store)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        let mut found = Vec::with_capacity(ids.len());
        for id in ids {
            found.push(Self::read_in(&mut tx, &SessionId(id)).await?);
        }
        tx.commit().await.map_err(store)?;
        Ok(found)
    }

    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
        let ids: Vec<String> =
            sqlx::query_scalar("SELECT id FROM session ORDER BY started_at DESC, id DESC")
                .fetch_all(&self.pool)
                .await
                .map_err(store)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        let mut found = Vec::with_capacity(ids.len());
        for id in ids {
            found.push(Self::read_in(&mut tx, &SessionId(id)).await?);
        }
        tx.commit().await.map_err(store)?;
        Ok(found)
    }

    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        let session = Self::read_in(&mut tx, id).await?;
        tx.commit().await.map_err(store)?;
        Ok(session)
    }

    async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
        validate_focus(&new.focus)?;
        let mut tx = self.pool.begin().await.map_err(store)?;

        // **One handle, one run.** A caller retrying a `begin` whose write
        // committed before its answer came back offers the same handle again;
        // minting unconditionally would fork the run.
        let held: Option<String> = sqlx::query_scalar(
            "SELECT id FROM session WHERE sid = ? AND state = ? ORDER BY id LIMIT 1",
        )
        .bind(new.sid.as_str())
        .bind(SessionState::Active.as_token())
        .fetch_optional(&mut *tx)
        .await
        .map_err(store)?;
        if let Some(id) = held {
            let session = Self::read_in(&mut tx, &SessionId(id)).await?;
            tx.commit().await.map_err(store)?;
            return Ok(session);
        }

        let id = SessionId(mint(&mut tx, "session").await?);
        sqlx::query(
            "INSERT INTO session (id, sid, bot, focus, started_at, state) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id.as_str())
        .bind(new.sid.as_str())
        .bind(new.bot.as_str())
        .bind(new.focus.trim())
        .bind(stamp(new.started_at))
        .bind(SessionState::Active.as_token())
        .execute(&mut *tx)
        .await
        .map_err(store)?;
        let session = Self::read_in(&mut tx, &id).await?;
        tx.commit().await.map_err(store)?;
        Ok(session)
    }

    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        validate_session_id(id)?;
        validate_entry(&entry.text)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        Self::writable(&mut tx, id).await?;
        let ordinal = Self::next_ordinal(&mut tx, id).await?;
        let entry_id = EntryId(mint(&mut tx, "entry").await?);
        sqlx::query(
            "INSERT INTO journal_entry (session, id, ordinal, at, text, touched, beat)
             VALUES (?, ?, ?, ?, ?, NULL, ?)",
        )
        .bind(id.as_str())
        .bind(entry_id.as_str())
        .bind(ordinal)
        .bind(stamp(entry.at))
        .bind(normalize_entry(&entry.text))
        .bind(entry.beat.as_deref())
        .execute(&mut *tx)
        .await
        .map_err(store)?;
        let written = read_entry(&mut tx, id, &entry_id).await?;
        tx.commit().await.map_err(store)?;
        Ok(written)
    }

    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        validate_session_id(id)?;
        validate_entry(text)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        Self::writable(&mut tx, id).await?;
        let newest: Option<String> = sqlx::query_scalar(
            "SELECT id FROM journal_entry WHERE session = ? ORDER BY ordinal DESC LIMIT 1",
        )
        .bind(id.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(store)?;
        let entry_id = EntryId(newest.ok_or_else(|| SessionError::NoEntries {
            attempted: id.to_string(),
        })?);
        sqlx::query("UPDATE journal_entry SET text = ? WHERE session = ? AND id = ?")
            .bind(normalize_entry(text))
            .bind(id.as_str())
            .bind(entry_id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        let written = read_entry(&mut tx, id, &entry_id).await?;
        tx.commit().await.map_err(store)?;
        Ok(written)
    }

    async fn amend_beat(
        &self,
        id: &SessionId,
        entry: &EntryId,
        text: &str,
        at: Timestamp,
    ) -> Result<JournalEntry, SessionError> {
        validate_session_id(id)?;
        validate_entry(text)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        Self::writable(&mut tx, id).await?;
        let held = read_entry(&mut tx, id, entry).await?;
        // **Only an automatic beat.** An entry the session wrote is its own
        // account of what it was doing, and nothing but `amend_last` touches
        // those — and only the newest of them.
        if held.beat.is_none() {
            return Err(SessionError::NotABeat {
                attempted: entry.to_string(),
                session: id.to_string(),
            });
        }
        sqlx::query("UPDATE journal_entry SET text = ?, touched = ? WHERE session = ? AND id = ?")
            .bind(normalize_entry(text))
            .bind(stamp(at))
            .bind(id.as_str())
            .bind(entry.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        let written = read_entry(&mut tx, id, entry).await?;
        tx.commit().await.map_err(store)?;
        Ok(written)
    }

    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        validate_focus(focus)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        Self::writable(&mut tx, id).await?;
        sqlx::query("UPDATE session SET focus = ? WHERE id = ?")
            .bind(focus.trim())
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        let session = Self::read_in(&mut tx, id).await?;
        tx.commit().await.map_err(store)?;
        Ok(session)
    }

    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        // Terminal both ways: a closed session is not closed again, whichever
        // end it reached.
        Self::writable(&mut tx, id).await?;
        sqlx::query("UPDATE session SET state = ? WHERE id = ?")
            .bind(to.as_token())
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        let session = Self::read_in(&mut tx, id).await?;
        tx.commit().await.map_err(store)?;
        Ok(session)
    }

    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        let held = Self::read_in(&mut tx, id).await?;
        // A run already open is a caller resuming the run they are in, which
        // is no mistake. A wrapped one told its story and is the last word.
        let session = match held.state {
            SessionState::Active => held,
            SessionState::Abandoned => {
                sqlx::query("UPDATE session SET state = ? WHERE id = ?")
                    .bind(SessionState::Active.as_token())
                    .bind(id.as_str())
                    .execute(&mut *tx)
                    .await
                    .map_err(store)?;
                Self::read_in(&mut tx, id).await?
            }
            SessionState::Wrapped => {
                return Err(SessionError::Closed {
                    attempted: id.to_string(),
                    state: SessionState::Wrapped,
                });
            }
        };
        tx.commit().await.map_err(store)?;
        Ok(session)
    }
}

/// One entry by id, or the miss that says the id names nothing here.
async fn read_entry(
    tx: &mut Transaction<'_, MySql>,
    session: &SessionId,
    entry: &EntryId,
) -> Result<JournalEntry, SessionError> {
    let row = sqlx::query(
        "SELECT id, at, text, touched, beat FROM journal_entry WHERE session = ? AND id = ?",
    )
    .bind(session.as_str())
    .bind(entry.as_str())
    .fetch_optional(&mut **tx)
    .await
    .map_err(store)?
    .ok_or_else(|| SessionError::NotABeat {
        attempted: entry.to_string(),
        session: session.to_string(),
    })?;
    entry_from(&row)
}

/// Mint an id from a counter the store keeps.
///
/// **Inside the caller's transaction**, so two writers cannot take the same
/// one. Opaque to everybody above this file: an id is a token, and nothing on
/// the surface reads meaning out of it.
async fn mint(tx: &mut Transaction<'_, MySql>, kind: &str) -> Result<String, SessionError> {
    // **`counter`, not `next`.** This store's parser treats `next` as a
    // reserved word and refuses the statement, which is the kind of quirk that
    // only shows up against the real thing. The table itself comes from the
    // migrations, like every other.
    sqlx::query(
        "INSERT INTO minted (kind, counter) VALUES (?, 1)
         ON DUPLICATE KEY UPDATE counter = counter + 1",
    )
    .bind(kind)
    .execute(&mut **tx)
    .await
    .map_err(store)?;
    let counter: i64 = sqlx::query_scalar("SELECT counter FROM minted WHERE kind = ?")
        .bind(kind)
        .fetch_one(&mut **tx)
        .await
        .map_err(store)?;
    Ok(counter.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dolt::tests::{Scratch, free_port};
    use crate::dolt::{Dolt, migrate};

    /// Put a session row on the board that jojobot cannot read, the way a hand
    /// edit or a record from a schema nobody remembers would.
    async fn row(store: &Dolt, id: &str, started_at: &str, state: &str) {
        sqlx::query(
            "INSERT INTO session (id, sid, bot, focus, started_at, state)
             VALUES (?, NULL, 'bot:gamma', 'what it was doing', ?, ?)",
        )
        .bind(id)
        .bind(started_at)
        .bind(state)
        .execute(store.pool())
        .await
        .expect("the board takes the row");
    }

    /// **A session row jojobot cannot read is refused, never guessed at.**
    ///
    /// Neither branch is reachable through the port — every write goes through
    /// verbs that validate — so the contract cannot produce either, and both
    /// were unasserted. The cost of guessing is not abstract: a state token
    /// read as `active` offers a run whose story is already told back as
    /// resumable, and a stamp read as some default makes a live session look
    /// ancient enough for the sweep to abandon it.
    #[tokio::test]
    async fn a_row_that_cannot_be_read_is_refused_rather_than_guessed_at() {
        let scratch = Scratch::new("unreadable-session");
        let path = scratch.0.clone();
        // Leaked deliberately: dropping it removes the data under a running
        // server. The process is stopped below and the temp dir goes with it.
        std::mem::forget(scratch);
        let mut store = Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");
        let sessions = DoltSessions::open(store.pool().clone());

        // Two ways a row stops being readable: a state that is no state, and a
        // stamp that is no time. Both, so neither channel can rot unnoticed.
        row(&store, "bad-state", "2026-01-01T00:00:00Z", "in-flight").await;
        row(&store, "bad-stamp", "the day before yesterday", "active").await;
        for id in ["bad-state", "bad-stamp"] {
            let refused = sessions.read_session(&SessionId(id.into())).await;
            assert!(
                matches!(refused, Err(SessionError::Store(_))),
                "reading {id} must refuse rather than guess: {refused:?}"
            );
        }

        // **The positive the refusals rest on.** Without it both assertions
        // hold on a store that refuses every read it is given, which would say
        // nothing about reading a damaged row in particular.
        row(&store, "sound", "2026-01-01T00:00:00Z", "active").await;
        let read = sessions
            .read_session(&SessionId("sound".into()))
            .await
            .expect("a well-formed row reads back");
        assert_eq!(read.state, SessionState::Active);
        assert_eq!(read.focus, "what it was doing");

        store.stop().await;
    }
}
