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
use jojobot_domain::memory::{Entity, EntityId, mention};
use jojobot_domain::session::{
    EntryId, JournalEntry, NewEntry, NewSession, Session, SessionError, SessionId, SessionState,
    Sessions, Sid, normalize_entry, validate_entry, validate_focus, validate_session_id,
};
use sqlx::{MySql, MySqlPool, Row, Transaction};

use super::ids::{self, Draw};

/// Sessions kept in the SQL store jojobot runs.
///
/// Cloning shares the one pool rather than opening a second: a pool is the
/// connection budget, and two of them against one server is two budgets
/// nobody set.
#[derive(Clone)]
pub struct DoltSessions {
    pool: MySqlPool,
    draw: Draw,
    /// **Where the boundary marks are written**, which in production is the
    /// pool beside it. It is a field of its own because a mark that fails must
    /// leave the session standing, and the only way to watch that happen is to
    /// break the marking and nothing else — see [`Self::snapshotting`].
    snapshots: MySqlPool,
    /// **How many sessions have been read in full** — every entry, every
    /// word — through [`sessions_of`](Sessions::sessions_of) or
    /// [`all_sessions`](Sessions::all_sessions). Test-only instrumentation
    /// for the one property nothing else here can observe: whether a
    /// summary read paid for the text a full read carries. Shared across a
    /// clone, exactly as the pool it counts against is.
    full_reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl DoltSessions {
    /// Open the store over an existing pool.
    ///
    /// **The schema is not this adapter's to create.** It arrives through the
    /// migrations the server applies on start, so there is one place a table's
    /// shape is decided and one order it changes in — see
    /// [`crate::dolt::migrate`].
    pub fn open(pool: MySqlPool) -> Self {
        DoltSessions {
            snapshots: pool.clone(),
            pool,
            draw: ids::drawing(),
            full_reads: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// How many sessions have been read in full since this store opened.
    /// Test-only.
    #[cfg(any(test, feature = "testing"))]
    pub fn full_reads(&self) -> usize {
        self.full_reads.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// **Rewrite a handle written into a focus line or a journal beat before
    /// mention resolution reached this port, onto the permanent id it
    /// names** (rule 260) — the text-column sibling of
    /// [`crate::dolt::memory::DoltMemory::backfill_handle_keyed_rows`].
    ///
    /// **Not a migration**, for the same reason that one is not: it needs the
    /// badged entity list, which exists only once the memory store has run
    /// its own boot steps, and a SQL migration runs before any of them.
    ///
    /// **The completion gate is [`mention::resolved`] itself, not a ledger.**
    /// It is a pure, idempotent rewrite: text already holding a stored mark
    /// is never read back as a handle to resolve (see
    /// [`mention`][jojobot_domain::memory::mention]'s own doc), so recomputing
    /// it a second time reproduces exactly what is stored, a row is written
    /// only when the recomputed text actually differs, and a second call
    /// touches none.
    ///
    /// Returns how many rows were rewritten, across the focus column and the
    /// chronology together.
    pub async fn migrate_mentions(&self, known: &[Entity]) -> Result<usize, SessionError> {
        let mut rewritten = 0;
        let focuses: Vec<(String, String)> = sqlx::query_as("SELECT id, focus FROM session")
            .fetch_all(&self.pool)
            .await
            .map_err(store)?;
        for (id, focus) in focuses {
            let updated = mention::resolved(&focus, known);
            if updated == focus {
                continue;
            }
            sqlx::query("UPDATE session SET focus = ? WHERE id = ?")
                .bind(&updated)
                .bind(&id)
                .execute(&self.pool)
                .await
                .map_err(store)?;
            rewritten += 1;
        }
        let entries: Vec<(String, String, String)> =
            sqlx::query_as("SELECT session, id, text FROM journal_entry")
                .fetch_all(&self.pool)
                .await
                .map_err(store)?;
        for (session, id, text) in entries {
            let updated = mention::resolved(&text, known);
            if updated == text {
                continue;
            }
            sqlx::query("UPDATE journal_entry SET text = ? WHERE session = ? AND id = ?")
                .bind(&updated)
                .bind(&session)
                .bind(&id)
                .execute(&self.pool)
                .await
                .map_err(store)?;
            rewritten += 1;
        }
        Ok(rewritten)
    }

    /// **Rewrite `bot` itself, once, onto the permanent id it names** — the
    /// column sibling of [`Self::migrate_mentions`], which only ever touched
    /// TEXT. A row written before `Mentioning` resolved this column holds a
    /// plain handle, and a rename since then left it exactly as stale as an
    /// un-migrated mention would have: `sessions_of` on the CURRENT handle
    /// stops finding it, because the row still names the one before.
    ///
    /// **Chases a former handle too**, unlike `migrate_mentions`: a row can
    /// only ever have been written under a handle that was current at the
    /// time, but nothing stops a caller from renaming twice before this
    /// migration runs, and the row must still resolve past both moves.
    ///
    /// Idempotent for the same reason `migrate_mentions` is: a row already
    /// holding a badge resolves through neither `known` nor `former` (both
    /// index by HANDLE), so it is read, found to resolve to nothing, and
    /// left untouched.
    pub async fn migrate_bot_column(
        &self,
        known: &[Entity],
        former: &[jojobot_domain::memory::FormerHandle],
    ) -> Result<usize, SessionError> {
        let mut rewritten = 0;
        let rows: Vec<(String, String)> = sqlx::query_as("SELECT id, bot FROM session")
            .fetch_all(&self.pool)
            .await
            .map_err(store)?;
        for (id, bot) in rows {
            let Some(entity) =
                jojobot_domain::memory::resolve_handle(&EntityId(bot.clone()), known, former)
            else {
                continue;
            };
            let Some(badge) = entity.badge.as_deref() else {
                continue;
            };
            if badge == bot {
                continue;
            }
            sqlx::query("UPDATE session SET bot = ? WHERE id = ?")
                .bind(badge)
                .bind(&id)
                .execute(&self.pool)
                .await
                .map_err(store)?;
            rewritten += 1;
        }
        Ok(rewritten)
    }

    /// The same store over a supplied draw, **so the collision path can be
    /// watched through the verb that mints**. Entropy will not produce a
    /// collision on demand.
    #[cfg(test)]
    pub(crate) fn drawing(pool: MySqlPool, draw: Draw) -> Self {
        DoltSessions {
            snapshots: pool.clone(),
            pool,
            draw,
            full_reads: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// The same store marking its boundaries somewhere else, **so a store that
    /// cannot be marked can be watched failing to stop a session**. A boundary
    /// mark is a convenience beside the act it bounds; a store that refuses one
    /// must still open and close runs, and a broken pool is the failure that
    /// reaches the mark and nothing else.
    #[cfg(test)]
    pub(crate) fn snapshotting(pool: MySqlPool, snapshots: MySqlPool) -> Self {
        DoltSessions {
            pool,
            draw: ids::drawing(),
            snapshots,
            full_reads: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Read one whole session inside a transaction, or say it is not there.
    ///
    /// **One reader for every verb**, so the row and its chronology can never
    /// come back assembled two different ways.
    async fn read_in(
        tx: &mut Transaction<'_, MySql>,
        id: &SessionId,
    ) -> Result<Session, SessionError> {
        let row = sqlx::query(
            "SELECT id, sid, bot, focus, started_at, state, timezone, started_on, served_chars
             FROM session WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(store)?
        .ok_or_else(|| SessionError::UnknownSession {
            attempted: id.to_string(),
        })?;
        let entries = sqlx::query(
            "SELECT id, at, text, touched, beat, happened_on FROM journal_entry
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

    /// **Keep that this session was written, and once per write** — the
    /// cheap signal [`DoltSessions::write_summary`] reads. See the
    /// migration's own doc for why no moment rides along with it.
    async fn append_session_write(
        tx: &mut Transaction<'_, MySql>,
        id: &SessionId,
    ) -> Result<(), SessionError> {
        let row = sqlx::query(
            "SELECT COALESCE(MAX(ordinal), 0) + 1 FROM session_write WHERE session = ?",
        )
        .bind(id.as_str())
        .fetch_one(&mut **tx)
        .await
        .map_err(store)?;
        let ordinal: i64 = row.try_get(0).map_err(store)?;
        sqlx::query("INSERT INTO session_write (session, ordinal) VALUES (?, ?)")
            .bind(id.as_str())
            .bind(ordinal)
            .execute(&mut **tx)
            .await
            .map_err(store)?;
        Ok(())
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
/// One row of the summary query, as the domain's [`SessionSummary`] — the
/// same field-by-field reading [`session_from`] does, minus the entries no
/// query for this ever asked for.
fn summary_from(
    row: &sqlx::mysql::MySqlRow,
) -> Result<jojobot_domain::session::SessionSummary, SessionError> {
    let state: String = row.try_get("state").map_err(store)?;
    let started: String = row.try_get("started_at").map_err(store)?;
    let sid: Option<String> = row.try_get("sid").map_err(store)?;
    let last_beat: String = row.try_get("last_beat").map_err(store)?;
    Ok(jojobot_domain::session::SessionSummary {
        id: SessionId(row.try_get::<String, _>("id").map_err(store)?),
        sid: sid.map(Sid),
        bot: EntityId(row.try_get::<String, _>("bot").map_err(store)?),
        focus: row.try_get::<String, _>("focus").map_err(store)?,
        started_at: instant(&started)?,
        served_chars: row.try_get::<i64, _>("served_chars").map_err(store)? as u64,
        state: SessionState::from_token(&state).ok_or_else(|| {
            SessionError::Store(format!("a session row carries the state '{state}'"))
        })?,
        entry_count: row.try_get::<i64, _>("entry_count").map_err(store)? as usize,
        last_beat: instant(&last_beat)?,
    })
}

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
        timezone: row
            .try_get::<Option<String>, _>("timezone")
            .map_err(store)?,
        bot: EntityId(row.try_get::<String, _>("bot").map_err(store)?),
        focus: row.try_get::<String, _>("focus").map_err(store)?,
        started_at: instant(&started)?,
        started_on: day(row
            .try_get::<Option<String>, _>("started_on")
            .map_err(store)?)?,
        served_chars: row.try_get::<i64, _>("served_chars").map_err(store)? as u64,
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
        on: day(row
            .try_get::<Option<String>, _>("happened_on")
            .map_err(store)?)?,
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

/// **A stated day as the store holds it**, or nothing when the run stated none.
///
/// A cell that is no date is a store fault a person repairs, exactly as a
/// timestamp that is no timestamp is: it is not a run without a frame, it is a
/// run whose frame jojobot cannot read, and answering it on the clock would
/// quietly serve the wrong one.
fn day(raw: Option<String>) -> Result<Option<jiff::civil::Date>, SessionError> {
    raw.map(|cell| {
        cell.parse().map_err(|_| {
            tracing::error!(cell = %cell, "a session row carries a day that is no day");
            SessionError::Store("a session row carries a day jojobot cannot read".into())
        })
    })
    .transpose()
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
            self.full_reads
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
            self.full_reads
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        tx.commit().await.map_err(store)?;
        Ok(found)
    }

    /// **One aggregate, not a read of any run.** `session_write` is written
    /// on every mutation of every session (see the migration's own doc), so
    /// its count answers "has anything changed" without touching a run's
    /// chronology. No moment rides along — see the same doc for why.
    async fn write_summary(&self) -> Result<Option<(i64, Option<Timestamp>)>, SessionError> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_write")
            .fetch_one(&self.pool)
            .await
            .map_err(store)?;
        Ok(Some((count, None)))
    }

    /// **The count and the last beat, answered by the store rather than
    /// measured after reading every entry.** `journal_entry`'s own `at` and
    /// `touched` are read into an aggregate; its `text`, `beat` and
    /// `happened_on` never leave the query at all — this is what
    /// [`DoltSessions::full_reads`] proves nothing here increments.
    ///
    /// **A session with no entries counts zero and falls back to its own
    /// `started_at`** — the `LEFT JOIN` leaves nothing for `MAX` to see, and
    /// `COALESCE` is the same fallback [`Session::last_beat`] uses in Rust,
    /// moved into the query.
    async fn summaries_of(
        &self,
        bot: &EntityId,
    ) -> Result<Vec<jojobot_domain::session::SessionSummary>, SessionError> {
        let rows = sqlx::query(
            "SELECT s.id, s.sid, s.bot, s.focus, s.started_at, s.state, s.served_chars,
                    COUNT(j.id) AS entry_count,
                    COALESCE(MAX(GREATEST(j.at, COALESCE(j.touched, j.at))), s.started_at)
                        AS last_beat
             FROM session s
             LEFT JOIN journal_entry j ON j.session = s.id
             WHERE s.bot = ?
             GROUP BY s.id, s.sid, s.bot, s.focus, s.started_at, s.state, s.served_chars
             ORDER BY s.started_at DESC, s.id DESC",
        )
        .bind(bot.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(store)?;
        rows.iter().map(summary_from).collect()
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

        let id = SessionId(
            mint(
                &mut tx,
                &self.draw,
                "SELECT 1 FROM session WHERE id = ?",
                None,
            )
            .await?,
        );
        sqlx::query(
            "INSERT INTO session (id, sid, bot, focus, started_at, state, timezone, started_on)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.as_str())
        .bind(new.sid.as_str())
        .bind(new.bot.as_str())
        .bind(new.focus.trim())
        .bind(stamp(new.started_at))
        .bind(SessionState::Active.as_token())
        .bind(new.timezone.as_deref())
        .bind(new.started_on.map(|day| day.to_string()))
        .execute(&mut *tx)
        .await
        .map_err(store)?;
        Self::append_session_write(&mut tx, &id).await?;
        let session = Self::read_in(&mut tx, &id).await?;
        tx.commit().await.map_err(store)?;
        // **The near end of the run's span.** It fixes what the store looked
        // like before this run touched anything, so a sitting is bounded at
        // both ends rather than inferred from wherever the last one stopped.
        // The retry above does not reach here: a caller offering a handle that
        // already holds a run began nothing.
        super::snapshot(
            &self.snapshots,
            &format!("session {} ({}) opened", id.as_str(), new.bot.as_str()),
        )
        .await;
        Ok(session)
    }

    async fn append(&self, id: &SessionId, entry: NewEntry) -> Result<JournalEntry, SessionError> {
        validate_session_id(id)?;
        validate_entry(&entry.text)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        Self::writable(&mut tx, id).await?;
        let ordinal = Self::next_ordinal(&mut tx, id).await?;
        // **Free within its session**, which is the whole of this table's key:
        // a chronology entry is addressed by the run it sits on and the id it
        // wears, so a draw only has to miss the entries of that one run.
        let entry_id = EntryId(
            mint(
                &mut tx,
                &self.draw,
                "SELECT 1 FROM journal_entry WHERE session = ? AND id = ?",
                Some(id.as_str()),
            )
            .await?,
        );
        sqlx::query(
            "INSERT INTO journal_entry (session, id, ordinal, at, text, touched, beat, happened_on)
             VALUES (?, ?, ?, ?, ?, NULL, ?, ?)",
        )
        .bind(id.as_str())
        .bind(entry_id.as_str())
        .bind(ordinal)
        .bind(stamp(entry.at))
        .bind(normalize_entry(&entry.text))
        .bind(entry.beat.as_deref())
        .bind(entry.on.map(|day| day.to_string()))
        .execute(&mut *tx)
        .await
        .map_err(store)?;
        Self::append_session_write(&mut tx, id).await?;
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
        Self::append_session_write(&mut tx, id).await?;
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
        Self::append_session_write(&mut tx, id).await?;
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
        Self::append_session_write(&mut tx, id).await?;
        let session = Self::read_in(&mut tx, id).await?;
        tx.commit().await.map_err(store)?;
        Ok(session)
    }

    async fn set_timezone(
        &self,
        id: &SessionId,
        timezone: Option<&str>,
    ) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        Self::writable(&mut tx, id).await?;
        sqlx::query("UPDATE session SET timezone = ? WHERE id = ?")
            .bind(timezone.map(str::trim).filter(|z| !z.is_empty()))
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        Self::append_session_write(&mut tx, id).await?;
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
        Self::append_session_write(&mut tx, id).await?;
        let session = Self::read_in(&mut tx, id).await?;
        tx.commit().await.map_err(store)?;
        // The far end of the span, whichever ending this was: a run that told
        // its story and one the sweep found stopped are both runs that ended.
        super::snapshot(
            &self.snapshots,
            &format!(
                "session {} ({}) ended {}",
                id.as_str(),
                session.bot.as_str(),
                to.as_token()
            ),
        )
        .await;
        Ok(session)
    }

    async fn add_served(&self, id: &SessionId, chars: u64) -> Result<(), SessionError> {
        validate_session_id(id)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        // Existence only, never refused on a closed session — see the
        // trait's own doc.
        Self::read_in(&mut tx, id).await?;
        sqlx::query("UPDATE session SET served_chars = served_chars + ? WHERE id = ?")
            .bind(chars as i64)
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        Self::append_session_write(&mut tx, id).await?;
        tx.commit().await.map_err(store)?;
        Ok(())
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
                Self::append_session_write(&mut tx, id).await?;
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
        "SELECT id, at, text, touched, beat, happened_on FROM journal_entry WHERE session = ? AND id = ?",
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
async fn mint(
    tx: &mut Transaction<'_, MySql>,
    draw: &Draw,
    taken: &'static str,
    scope: Option<&str>,
) -> Result<String, SessionError> {
    ids::draw_free(tx, draw, taken, scope)
        .await
        .map_err(store)?
        .ok_or_else(|| SessionError::Store("no free id could be drawn".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dolt::tests::{Scratch, free_port, marks};
    use crate::dolt::{Dolt, migrate};

    /// A draw that hands back a fixed sequence, so what the store does with a
    /// candidate it cannot use is watchable.
    fn rigged(candidates: &[&str]) -> Draw {
        let queued = std::sync::Mutex::new(
            candidates
                .iter()
                .map(|c| c.to_string())
                .collect::<std::collections::VecDeque<_>>(),
        );
        std::sync::Arc::new(move || {
            queued
                .lock()
                .expect("the rigged draw is poisoned")
                .pop_front()
                .expect("the case supplied enough candidates")
        })
    }

    /// **Both ids on this rail are drawn, and a candidate that is already taken
    /// is drawn again** — watched through `begin` and `append`, the two verbs
    /// that mint.
    ///
    /// **The two ids are taken differently, and that is the point of one case
    /// covering both.** A session id has to miss every session in the store; an
    /// entry id only has to miss the entries of the run it is being appended
    /// to, because that table is keyed by the pair. A probe that asked the
    /// wider question for an entry would redraw over a free id forever, and one
    /// that asked the narrower question for a session would hand out an id
    /// another bot's run already wears.
    #[tokio::test]
    async fn a_drawn_id_something_already_wears_is_drawn_again() {
        let scratch = Scratch::new("drawn-session-ids");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");
        // The rest of a boot: without it this process cannot read a handle,
        // which is what a caller meets rather than a state a suite hides.
        migrate::seed_kinds(store.pool())
            .await
            .expect("the kinds are seeded");
        let sessions = DoltSessions::drawing(
            store.pool().clone(),
            rigged(&[
                // the first run takes it
                "aaaaaa", // the second run is handed the same one, twice, then a free one
                "aaaaaa", "aaaaaa", "bbbbbb", // the first entry, on the second run
                "cccccc", // the second entry is handed the entry id its own run wears
                "cccccc", "dddddd",
            ]),
        );
        let begin = async |slug: &str, sid: &str, at: &str| {
            sessions
                .begin(NewSession {
                    timezone: None,
                    started_on: None,
                    bot: EntityId(format!("bot:{slug}")),
                    sid: Sid(sid.into()),
                    focus: "a run".into(),
                    started_at: at.parse().expect("a fixed instant"),
                })
                .await
                .expect("begin ok")
        };

        let first = begin("gamma", "ab12", "2026-01-01T00:00:00Z").await;
        assert_eq!(first.id.as_str(), "aaaaaa");
        let second = begin("delta", "cd34", "2026-01-01T00:01:00Z").await;
        assert_eq!(
            second.id.as_str(),
            "bbbbbb",
            "a session id another run wears must not be re-issued"
        );

        let entry = sessions
            .append(
                &second.id,
                NewEntry::manual(
                    "what I set out to do",
                    "2026-01-01T00:02:00Z".parse().unwrap(),
                    None,
                ),
            )
            .await
            .expect("append ok");
        assert_eq!(entry.id.as_str(), "cccccc");
        let next = sessions
            .append(
                &second.id,
                NewEntry::manual(
                    "what I found",
                    "2026-01-01T00:03:00Z".parse().unwrap(),
                    None,
                ),
            )
            .await
            .expect("append ok");
        assert_eq!(
            next.id.as_str(),
            "dddddd",
            "an entry id this run already wears must not be re-issued"
        );

        // **The positive the redraws rest on.** A store that had written over
        // the earlier records would satisfy every assertion above.
        let board = sessions.all_sessions().await.expect("list ok");
        assert_eq!(board.len(), 2, "both runs stand: {board:?}");
        let run = board
            .iter()
            .find(|s| s.id == second.id)
            .expect("the second run is on the board");
        assert_eq!(
            run.entries
                .iter()
                .map(|e| (e.id.as_str(), e.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("cccccc", "what I set out to do"),
                ("dddddd", "what I found")
            ],
            "both entries stand, in order, each with its own text"
        );

        store.stop().await;
    }

    /// **A drawn entry id is free within its run, not across the store.** The
    /// narrower key is what the probe asks about, and the tell is that an id
    /// another run's chronology wears is accepted here rather than redrawn
    /// past — which is correct, and is the assertion that fails if the probe is
    /// widened to the whole table.
    #[tokio::test]
    async fn an_entry_id_only_has_to_be_free_inside_its_own_run() {
        let scratch = Scratch::new("entry-id-scope");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");
        // The rest of a boot: without it this process cannot read a handle,
        // which is what a caller meets rather than a state a suite hides.
        migrate::seed_kinds(store.pool())
            .await
            .expect("the kinds are seeded");
        let sessions = DoltSessions::drawing(
            store.pool().clone(),
            rigged(&["aaaaaa", "bbbbbb", "eeeeee", "eeeeee"]),
        );
        let begin = async |slug: &str, sid: &str, at: &str| {
            sessions
                .begin(NewSession {
                    timezone: None,
                    started_on: None,
                    bot: EntityId(format!("bot:{slug}")),
                    sid: Sid(sid.into()),
                    focus: "a run".into(),
                    started_at: at.parse().expect("a fixed instant"),
                })
                .await
                .expect("begin ok")
        };
        let one = begin("gamma", "ab12", "2026-01-01T00:00:00Z").await;
        let other = begin("delta", "cd34", "2026-01-01T00:01:00Z").await;

        let mine = sessions
            .append(
                &one.id,
                NewEntry::manual("mine", "2026-01-01T00:02:00Z".parse().unwrap(), None),
            )
            .await
            .expect("append ok");
        let theirs = sessions
            .append(
                &other.id,
                NewEntry::manual("theirs", "2026-01-01T00:03:00Z".parse().unwrap(), None),
            )
            .await
            .expect("append ok");
        assert_eq!(
            (mine.id.as_str(), theirs.id.as_str()),
            ("eeeeee", "eeeeee"),
            "two runs may each hold an entry of the same id, and neither redraws"
        );

        store.stop().await;
    }

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
        // The rest of a boot: without it this process cannot read a handle,
        // which is what a caller meets rather than a state a suite hides.
        migrate::seed_kinds(store.pool())
            .await
            .expect("the kinds are seeded");
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

    /// **Both ends of a run are marked, and they are two marks.**
    ///
    /// A sitting is bounded at both ends rather than inferred from wherever
    /// the last one stopped, so the opening is its own mark: what the store
    /// looked like before this run touched anything. What the pair honestly
    /// bounds is what changed between them — runs overlap, and a mark is
    /// global — which is why neither says the run did the work.
    #[tokio::test]
    async fn both_ends_of_a_run_are_marked_in_the_store() {
        let scratch = Scratch::new("session-boundaries");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");
        // The rest of a boot: without it this process cannot read a handle,
        // which is what a caller meets rather than a state a suite hides.
        migrate::seed_kinds(store.pool())
            .await
            .expect("the kinds are seeded");
        let sessions = DoltSessions::open(store.pool().clone());

        let before = marks(store.pool()).await;
        let run = sessions
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("ab12".into()),
                focus: "a run".into(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                started_on: None,
            })
            .await
            .expect("begin ok");

        let opened = marks(store.pool()).await;
        assert_eq!(
            opened.len(),
            before.len() + 1,
            "opening a run marks a boundary: {opened:?}"
        );
        assert!(
            opened
                .iter()
                .any(|m| m.contains(run.id.as_str()) && m.contains("opened")),
            "the mark names the run it bounds: {opened:?}"
        );

        sessions
            .close(&run.id, SessionState::Wrapped)
            .await
            .expect("close ok");

        let ended = marks(store.pool()).await;
        assert_eq!(
            ended.len(),
            opened.len() + 1,
            "the ending is a boundary of its own, not the same one: {ended:?}"
        );
        assert!(
            ended
                .iter()
                .any(|m| m.contains(run.id.as_str()) && m.contains("ended")),
            "the far end names the run and how it ended: {ended:?}"
        );

        store.stop().await;
    }

    /// **A boundary that cannot be marked does not take the run down with it.**
    ///
    /// Opening and closing a run are the acts that matter; the mark is a
    /// convenience beside them. The failure is real rather than staged around:
    /// the marks go to a pool that has been closed, so every mark this store
    /// tries to write fails while every session write lands.
    ///
    /// **Both halves.** That the run stands is asserted by reading it back off
    /// the board, and that the mark truly failed is asserted by the store's
    /// history not naming it — without the second, this passes on a build
    /// where marking works perfectly.
    #[tokio::test]
    async fn a_boundary_that_cannot_be_marked_leaves_the_run_standing() {
        let scratch = Scratch::new("boundary-unmarkable");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");
        // The rest of a boot: without it this process cannot read a handle,
        // which is what a caller meets rather than a state a suite hides.
        migrate::seed_kinds(store.pool())
            .await
            .expect("the kinds are seeded");
        let unreachable = store
            .database("marks_go_nowhere")
            .await
            .expect("a pool of its own");
        unreachable.close().await;
        let sessions = DoltSessions::snapshotting(store.pool().clone(), unreachable);

        let run = sessions
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("cd34".into()),
                focus: "a run whose boundaries cannot be marked".into(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                started_on: None,
            })
            .await
            .expect("a run starts even when its boundary cannot be marked");
        let read = sessions
            .read_session(&run.id)
            .await
            .expect("and the run is on the board");
        assert_eq!(read.state, SessionState::Active);

        let closed = sessions
            .close(&run.id, SessionState::Wrapped)
            .await
            .expect("and it ends, too");
        assert_eq!(closed.state, SessionState::Wrapped);

        assert!(
            marks(store.pool())
                .await
                .iter()
                .all(|m| !m.contains(run.id.as_str())),
            "the marks really did fail, or this proves nothing"
        );
        // **And this store CAN be marked**, so what failed above was the pool
        // the marks went to and not the store refusing every mark there is.
        crate::dolt::snapshot(store.pool(), "a mark from a pool that works").await;
        assert!(
            marks(store.pool())
                .await
                .iter()
                .any(|m| m == "a mark from a pool that works"),
            "the store takes a mark from a pool that works"
        );

        store.stop().await;
    }

    /// **The one-time migration, against text stored the old way** — written
    /// through the bare adapter, which resolves nothing itself, exactly as
    /// every row written before this port had a `Mentioning` in front of it
    /// did.
    #[tokio::test]
    async fn migrate_mentions_rewrites_a_handle_stored_before_resolution_existed() {
        use jojobot_domain::memory::{Memory, NewEntity};

        let scratch = Scratch::new("migrate-mentions-sessions");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");
        migrate::seed_kinds(store.pool())
            .await
            .expect("the kinds are seeded");

        let memory = crate::dolt::memory::DoltMemory::open(store.pool().clone());
        memory
            .add_entity(NewEntity::new(
                EntityId("thing:contract-migrate-mentions-was".to_string()),
                "was",
                "contract-fixture",
            ))
            .await
            .expect("add_entity should succeed")
            .written()
            .expect("the guard must not block a fresh handle");
        memory.badge_the_unbadged().await.expect("badges are drawn");
        let known = memory
            .list_entities(None)
            .await
            .expect("list_entities should succeed");

        let sessions = DoltSessions::open(store.pool().clone());
        let session = sessions
            .begin(NewSession {
                bot: EntityId("bot:gamma".to_string()),
                sid: Sid("sid-migrate".to_string()),
                focus: "about @thing:contract-migrate-mentions-was".to_string(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                timezone: None,
                started_on: None,
            })
            .await
            .expect("begin should succeed");
        sessions
            .append(
                &session.id,
                NewEntry::manual(
                    "found @thing:contract-migrate-mentions-was",
                    "2026-01-01T00:01:00Z".parse().expect("a fixed instant"),
                    None,
                ),
            )
            .await
            .expect("append should succeed");

        let rewritten = sessions
            .migrate_mentions(&known)
            .await
            .expect("migrate_mentions should succeed");
        assert_eq!(rewritten, 2, "the focus row and the entry row both changed");

        let after = sessions
            .read_session(&session.id)
            .await
            .expect("read_session should succeed");
        assert!(
            !after.focus.contains("thing:contract-migrate-mentions-was"),
            "the bare handle must not survive the migration: {}",
            after.focus
        );
        assert!(after.focus.contains(mention::MARK));
        assert!(
            !after.entries[0]
                .text
                .contains("thing:contract-migrate-mentions-was"),
            "the bare handle must not survive the migration: {}",
            after.entries[0].text
        );
        assert!(after.entries[0].text.contains(mention::MARK));

        let second_pass = sessions
            .migrate_mentions(&known)
            .await
            .expect("a second run should succeed");
        assert_eq!(second_pass, 0, "nothing left to rewrite touches nothing");

        store.stop().await;
    }

    /// 🚨 **The `bot` column's own migration** — a row `begin`s with a plain
    /// handle (written the old way, exactly as the mentions case is), the
    /// bot is renamed TWICE after that, and the migration still resolves the
    /// row onto the badge, past both moves — proving `former` is genuinely
    /// consulted and not just accepted as a parameter.
    #[tokio::test]
    async fn migrate_bot_column_rewrites_a_handle_stored_before_resolution_existed() {
        use jojobot_domain::memory::{Memory, NewEntity};

        let scratch = Scratch::new("migrate-bot-column-sessions");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");
        migrate::seed_kinds(store.pool())
            .await
            .expect("the kinds are seeded");

        let memory = crate::dolt::memory::DoltMemory::open(store.pool().clone());
        memory
            .add_entity(NewEntity::new(
                EntityId("bot:contract-migrate-bot-column-gamma".to_string()),
                "gamma",
                "contract-fixture",
            ))
            .await
            .expect("add_entity should succeed")
            .written()
            .expect("the guard must not block a fresh handle");
        memory.badge_the_unbadged().await.expect("badges are drawn");

        let sessions = DoltSessions::open(store.pool().clone());
        let session = sessions
            .begin(NewSession {
                bot: EntityId("bot:contract-migrate-bot-column-gamma".to_string()),
                sid: Sid("sid-migrate-bot".to_string()),
                focus: "written before resolution existed".to_string(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                timezone: None,
                started_on: None,
            })
            .await
            .expect("begin should succeed");

        // Renamed twice, so the row's stored handle is now two moves stale.
        memory
            .rename_entity(
                &EntityId("bot:contract-migrate-bot-column-gamma".to_string()),
                &EntityId("bot:contract-migrate-bot-column-delta".to_string()),
                None,
                jiff::civil::date(2026, 1, 2),
                None,
            )
            .await
            .expect("rename_entity should succeed")
            .written()
            .expect("the guard must not block the rename");
        memory
            .rename_entity(
                &EntityId("bot:contract-migrate-bot-column-delta".to_string()),
                &EntityId("bot:contract-migrate-bot-column-sigma".to_string()),
                None,
                jiff::civil::date(2026, 1, 3),
                None,
            )
            .await
            .expect("rename_entity should succeed")
            .written()
            .expect("the guard must not block the second rename");

        let known = memory
            .list_entities(None)
            .await
            .expect("list_entities should succeed");
        let former = memory
            .former_handles()
            .await
            .expect("former_handles should succeed");

        let rewritten = sessions
            .migrate_bot_column(&known, &former)
            .await
            .expect("migrate_bot_column should succeed");
        assert_eq!(rewritten, 1, "the one stale row changed");

        let stored: (String,) = sqlx::query_as("SELECT bot FROM session WHERE id = ?")
            .bind(session.id.as_str())
            .fetch_one(store.pool())
            .await
            .expect("the row is there");
        let sigma = known
            .iter()
            .find(|e| e.id.as_str() == "bot:contract-migrate-bot-column-sigma")
            .expect("sigma is in the known list");
        assert_eq!(
            Some(stored.0.as_str()),
            sigma.badge.as_deref(),
            "the row must hold the badge, not any handle it was ever called",
        );

        let second_pass = sessions
            .migrate_bot_column(&known, &former)
            .await
            .expect("a second run should succeed");
        assert_eq!(second_pass, 0, "nothing left to rewrite touches nothing");

        store.stop().await;
    }

    /// **`add_served` against the real store — the migration's own column,
    /// written and read back.** Two writes total, an untouched session in
    /// the same database reads zero, and closing the first run does not
    /// refuse a further add — the trait's own "never refused on a closed
    /// session" contract, proven against the real column rather than only
    /// the fake.
    #[tokio::test]
    async fn add_served_totals_across_writes_and_survives_a_close() {
        let scratch = Scratch::new("session-served-chars");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");
        migrate::seed_kinds(store.pool())
            .await
            .expect("the kinds are seeded");

        let sessions = DoltSessions::open(store.pool().clone());
        let handed = sessions
            .begin(NewSession {
                bot: EntityId("bot:milhouse".to_string()),
                sid: Sid("served-sid-a".to_string()),
                focus: "answering things".to_string(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                timezone: None,
                started_on: None,
            })
            .await
            .expect("begin should succeed");
        let untouched = sessions
            .begin(NewSession {
                bot: EntityId("bot:gamma".to_string()),
                sid: Sid("served-sid-b".to_string()),
                focus: "not yet asked anything".to_string(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                timezone: None,
                started_on: None,
            })
            .await
            .expect("begin should succeed");

        sessions
            .add_served(&handed.id, 120)
            .await
            .expect("add_served ok");
        sessions
            .add_served(&handed.id, 340)
            .await
            .expect("add_served ok");

        let read_handed = sessions.read_session(&handed.id).await.expect("read ok");
        let read_untouched = sessions.read_session(&untouched.id).await.expect("read ok");
        assert_eq!(
            read_handed.served_chars, 460,
            "the two writes total: {read_handed:?}"
        );
        assert_eq!(
            read_untouched.served_chars, 0,
            "an untouched session reads zero: {read_untouched:?}"
        );

        sessions
            .close(&handed.id, SessionState::Wrapped)
            .await
            .expect("close should succeed");
        sessions
            .add_served(&handed.id, 50)
            .await
            .expect("add_served must not be refused on a closed session");
        let after_close = sessions.read_session(&handed.id).await.expect("read ok");
        assert_eq!(
            after_close.served_chars, 510,
            "accounting keeps running after the session closes: {after_close:?}"
        );

        store.stop().await;
    }

    /// 🚨 **The pair that proves `summaries_of` answers the same numbers
    /// `sessions_of` would, without paying for the text.** The positive
    /// alone (right numbers) would pass on a build that quietly went back to
    /// reading everything; the negative alone (nothing extra fetched) would
    /// pass on a build that answers zero for every count. Both, on the same
    /// two sessions.
    #[tokio::test]
    async fn summaries_of_answers_sessions_ofs_own_numbers_without_the_full_read() {
        let scratch = Scratch::new("session-summaries");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");
        migrate::seed_kinds(store.pool())
            .await
            .expect("the kinds are seeded");
        let sessions = DoltSessions::open(store.pool().clone());

        let busy = sessions
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("bz01".into()),
                focus: "a busy run".into(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                started_on: None,
            })
            .await
            .expect("begin ok");
        for (text, at) in [
            ("first beat", "2026-01-01T00:05:00Z"),
            ("second beat", "2026-01-01T00:10:00Z"),
            ("third and newest beat", "2026-01-01T00:15:00Z"),
        ] {
            sessions
                .append(
                    &busy.id,
                    NewEntry::manual(text, at.parse().expect("a fixed instant"), None),
                )
                .await
                .expect("append ok");
        }
        let quiet = sessions
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("qt01".into()),
                focus: "a run with nothing journalled".into(),
                started_at: "2026-01-02T00:00:00Z".parse().expect("a fixed instant"),
                started_on: None,
            })
            .await
            .expect("begin ok");

        let before = sessions.full_reads();
        let summaries = sessions
            .summaries_of(&EntityId("bot:gamma".into()))
            .await
            .expect("summaries_of ok");
        assert_eq!(
            sessions.full_reads(),
            before,
            "a summary read must not fall back to reading any session in full"
        );

        assert_eq!(summaries.len(), 2, "{summaries:?}");
        let busy_summary = summaries
            .iter()
            .find(|s| s.id == busy.id)
            .expect("the busy run is in the summary");
        assert_eq!(busy_summary.entry_count, 3, "{busy_summary:?}");
        assert_eq!(
            busy_summary.last_beat,
            "2026-01-01T00:15:00Z"
                .parse::<Timestamp>()
                .expect("a fixed instant"),
            "the newest beat's own moment, not the run's start: {busy_summary:?}"
        );
        let quiet_summary = summaries
            .iter()
            .find(|s| s.id == quiet.id)
            .expect("the quiet run is in the summary");
        assert_eq!(quiet_summary.entry_count, 0, "{quiet_summary:?}");
        assert_eq!(
            quiet_summary.last_beat, quiet.started_at,
            "a run with nothing journalled falls back to when it began: {quiet_summary:?}"
        );

        // **The positive the counter rests on.** Without this, `full_reads`
        // staying flat above would say nothing — a counter that never moves
        // proves the same thing a broken one does.
        sessions
            .sessions_of(&EntityId("bot:gamma".into()))
            .await
            .expect("sessions_of ok");
        assert_eq!(
            sessions.full_reads(),
            before + 2,
            "sessions_of must read both sessions in full, which is the cost this exists to avoid"
        );

        store.stop().await;
    }
}
