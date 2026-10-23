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
use jojobot_domain::clock::Clock;
use jojobot_domain::memory::{Entity, EntityId, mention};
use jojobot_domain::session::history::{
    Change, ChangeKind, SessionChange, SessionHistories, Wording, WriteStamp, next_change,
    next_wording,
};
use jojobot_domain::session::{
    CharterChange, EntryId, JournalEntry, KeyTrail, MintedRun, NewEntry, NewSession, RetiredBy,
    RetiredKey, Session, SessionError, SessionId, SessionState, Sessions, Sid, WrapWindow,
    is_readable_sid, normalize_entry, validate_entry, validate_focus, validate_session_id,
};
use jojobot_domain::usage::{CallCount, CallKey, CallOutcome, RunCall, ServedDay};
use sqlx::{AssertSqlSafe, MySql, MySqlPool, Row, Transaction};

use super::ids::{self, Draw};
use super::ordinal;
use moments::{RUN_COLUMNS, day, day_of, entry_moments, instant, instant_of, no_such_state};

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
    /// **How many times [`Self::read_in`] ran** — the one seam every full
    /// read shares, whichever verb reached for it. Test-only instrumentation
    /// for the one property nothing else here can observe: whether a call
    /// that only needed existence paid for a full read anyway. Shared across
    /// a clone, exactly as the pool it counts against is.
    full_reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// **How many `journal_entry` rows [`Self::read_in`] has actually
    /// parsed**, summed across every call. `full_reads` counts the calls;
    /// this counts what each one cost — the number that scales with a run's
    /// own chronology, which a call count alone cannot show.
    entries_read: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// **How many times [`Self::exists`] asked the store whether a row is
    /// there.** Test-only instrumentation for the one property a count of
    /// full reads cannot show: whether a write that only needs the row to
    /// exist asked a question of its own first.
    existence_checks: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// **The clock the changes of a run are stamped with.** The real one unless
    /// an operator stated a day for the whole run.
    clock: Clock,
    /// **The moment a layer above holds for the write in flight**, which a
    /// record is stamped with instead of the clock when one is held.
    held: WriteStamp,
    /// **The one lock every increment of this store takes** (`x = x + ?`), so
    /// two adds to one row never run together. Shared across a clone, as the pool
    /// is. See [`Self::increment`].
    increments: std::sync::Arc<tokio::sync::Mutex<()>>,
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
            entries_read: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            existence_checks: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            clock: Clock::Real,
            held: WriteStamp::new(),
            increments: std::sync::Arc::default(),
        }
    }

    /// **The store, told which clock it stamps with.** The real one unless an
    /// operator stated a day for the whole run.
    #[must_use]
    pub fn on_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// **The store, told which moment a layer above holds for its writes.** The
    /// layer takes the moment once per write and the records the store makes for
    /// that write carry the same one.
    #[must_use]
    pub fn stamped_by(mut self, stamp: WriteStamp) -> Self {
        self.held = stamp;
        self
    }

    /// **Take the lock an increment runs under**, held until the guard drops, so
    /// a caller keeps it to the end of its transaction.
    ///
    /// The real store merges two concurrent adds to one row by keeping one and
    /// refuses neither, so a count is short without any error. Every statement
    /// of the form `x = x + ?` in this file goes through here, which makes the
    /// store keep both adds whoever calls it, with or without the write gate of
    /// [`crate::fold::sessions`] above it (decision log 292). The lock is
    /// per process: a second writer outside this process is still not covered.
    async fn increment(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.increments.lock().await
    }

    /// The moment to stamp a record with: the one held for the write in flight,
    /// else this store's clock.
    fn moment(&self) -> Timestamp {
        self.held.current().unwrap_or_else(|| self.clock.now())
    }

    /// How many sessions have been read in full since this store opened.
    /// Test-only.
    #[cfg(any(test, feature = "testing"))]
    pub fn full_reads(&self) -> usize {
        self.full_reads.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// How many times a call asked the store whether a session row exists
    /// since this store opened. Test-only.
    #[cfg(any(test, feature = "testing"))]
    pub fn existence_checks(&self) -> usize {
        self.existence_checks
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// How many `journal_entry` rows have been parsed by a full read since
    /// this store opened. Test-only: the number a call count cannot show,
    /// because it scales with what a run has already recorded rather than
    /// with how many times something asked about it.
    #[cfg(any(test, feature = "testing"))]
    pub fn entries_read(&self) -> usize {
        self.entries_read.load(std::sync::atomic::Ordering::SeqCst)
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
    /// **The pass is idempotent, and a marker says it has finished.**
    /// [`mention::resolved`] is a pure rewrite: text already holding a stored
    /// mark is never read back as a handle to resolve (see
    /// [`mention`][jojobot_domain::memory::mention]'s own doc), so a second
    /// call reproduces exactly what is stored and touches no row. That is
    /// what makes a pass that failed partway safe to run again. It is not what
    /// stops a finished pass from running again: reading every row of a table
    /// at every boot to find nothing grows with the store, so
    /// [`crate::dolt::migrate_permanent_ids`] records the end of a finished
    /// pass in the store and a later boot does not call this at all.
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
            let updated =
                mention::resolved(&focus, known).map_err(|_| SessionError::KindsNeverLoaded)?;
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
            let updated =
                mention::resolved(&text, known).map_err(|_| SessionError::KindsNeverLoaded)?;
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
            entries_read: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            existence_checks: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            clock: Clock::Real,
            held: WriteStamp::new(),
            increments: std::sync::Arc::default(),
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
            entries_read: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            existence_checks: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            clock: Clock::Real,
            held: WriteStamp::new(),
            increments: std::sync::Arc::default(),
        }
    }

    /// Read one whole session inside a transaction, or say it is not there.
    ///
    /// **One reader for every verb**, so the row and its chronology can never
    /// come back assembled two different ways — and **one counter for every
    /// caller**, so a verb that reaches for this pays the cost in the open
    /// rather than in a signal nothing else can see: `full_reads` for the
    /// call, `entries_read` for the rows it actually parsed.
    async fn read_in(
        &self,
        tx: &mut Transaction<'_, MySql>,
        id: &SessionId,
    ) -> Result<Session, SessionError> {
        let row = sqlx::query(AssertSqlSafe(format!(
            "SELECT {RUN_COLUMNS} FROM session WHERE id = ?"
        )))
        .bind(id.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(store)?
        .ok_or_else(|| SessionError::UnknownSession {
            attempted: id.to_string(),
        })?;
        let entries = sqlx::query(
            "SELECT id, at, text, touched, beat, happened_on, closing_focus, closing FROM journal_entry
             WHERE session = ? ORDER BY ordinal",
        )
        .bind(id.as_str())
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        self.full_reads
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.entries_read
            .fetch_add(entries.len(), std::sync::atomic::Ordering::SeqCst);
        session_from(&row, &entries)
    }

    /// **The row exists, without paying for its chronology.** What
    /// [`Self::add_served`] and the history reads need and [`Self::read_in`]
    /// costs more than: a session an accounting write may touch does not have
    /// to be open, so this asks existence alone, the same question a foreign
    /// key would ask if this table had one to lean on. The one site that asks
    /// it, with the one refusal for a run no row holds (rule 498). Never counted against `full_reads`
    /// or `entries_read` — it reads neither a session row's every column nor
    /// any of its chronology.
    async fn exists(
        &self,
        tx: &mut Transaction<'_, MySql>,
        id: &SessionId,
    ) -> Result<(), SessionError> {
        self.existence_checks
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM session WHERE id = ?")
            .bind(id.as_str())
            .fetch_optional(&mut **tx)
            .await
            .map_err(store)?;
        found
            .map(|_| ())
            .ok_or_else(|| SessionError::UnknownSession {
                attempted: id.to_string(),
            })
    }

    /// **The one close**, with the run that displaced this one when that is why
    /// it closes. The reason is written in the statement that moves the state.
    async fn close_with(
        &self,
        id: &SessionId,
        to: SessionState,
        displaced: Option<&Sid>,
    ) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        // Terminal both ways: a closed session is not closed again, whichever
        // end it reached.
        let held = self.writable(&mut tx, id).await?;
        // **A run that is already wrapped, with its window open, can only be
        // wrapped again** — that close is what ends the window.
        if held.state.is_terminal() && to != SessionState::Wrapped {
            return Err(SessionError::Closed {
                attempted: id.to_string(),
                state: held.state,
                displaced_by: held.key_trail.displaced_by.clone(),
            });
        }
        sqlx::query(
            "UPDATE session SET state = ?, wrap_window = NULL, displaced_by = ? WHERE id = ?",
        )
        .bind(to.as_token())
        .bind(displaced.map(|by| by.as_str()))
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(store)?;
        // A close that ends an open window records the window going as well.
        self.keep_change(&mut tx, id, Change::State(to)).await?;
        if held.wrap_window.is_some() {
            self.keep_change(&mut tx, id, Change::WrapWindow(None))
                .await?;
        }
        let session = self.read_in(&mut tx, id).await?;
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

    /// The session a write is allowed to touch: it exists, and it is open.
    ///
    /// One helper for every write verb, so they cannot come to disagree about
    /// what closed means — the same reason the fake has one.
    async fn writable(
        &self,
        tx: &mut Transaction<'_, MySql>,
        id: &SessionId,
    ) -> Result<Session, SessionError> {
        let session = self.read_in(tx, id).await?;
        if !session.takes_writes() {
            return Err(SessionError::Closed {
                attempted: id.to_string(),
                state: session.state,
                displaced_by: session.key_trail.displaced_by.clone(),
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
        let ordinal = ordinal::next(tx, "journal_entry", &[("session", id.as_str())])
            .await
            .map_err(store)?;
        i32::try_from(ordinal).map_err(|e| SessionError::Store(e.to_string()))
    }

    /// **Keep one change of a run, in the transaction that made it.** The record
    /// is made by [`next_change`], numbered one past the highest the run holds
    /// and stamped on this store's clock, so every site that changes a session
    /// row calls this and none of them decides the number, the value or what a
    /// value must not hold (rule 498).
    ///
    /// The search index reads the count of these records through
    /// [`DoltSessions::write_summary`], and only the kinds that
    /// [`ChangeKind::moves_the_index`] names move it.
    async fn keep_change(
        &self,
        tx: &mut Transaction<'_, MySql>,
        id: &SessionId,
        change: Change<'_>,
    ) -> Result<(), SessionError> {
        // **Only the highest, never the list**: a read of every record a run
        // holds would grow with them.
        let highest = ordinal::highest(tx, "session_write", &[("session", id.as_str())])
            .await
            .map_err(store)?;
        let made = next_change(highest.map(|n| n.max(0) as usize), change, self.moment());
        sqlx::query(
            "INSERT INTO session_write (session, ordinal, written_at, changed, value)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id.as_str())
        .bind(made.ordinal as i64)
        .bind(made.written_at.map(stamp))
        .bind(made.changed.map(ChangeKind::as_token))
        .bind(made.value.as_deref())
        .execute(&mut **tx)
        .await
        .map_err(store)?;
        Ok(())
    }

    /// **Keep one wording of a journal entry, in the transaction that wrote
    /// it**, made by [`next_wording`] for the reason [`Self::keep_change`] is.
    async fn keep_wording(
        &self,
        tx: &mut Transaction<'_, MySql>,
        id: &SessionId,
        written: &JournalEntry,
    ) -> Result<(), SessionError> {
        let highest = ordinal::highest(
            tx,
            "journal_entry_write",
            &[("session", id.as_str()), ("entry", written.id.as_str())],
        )
        .await
        .map_err(store)?;
        let made = next_wording(
            highest.map(|n| n.max(0) as usize),
            &written.text,
            written.touched,
            self.moment(),
        );
        sqlx::query(
            "INSERT INTO journal_entry_write (session, entry, ordinal, text, touched, written_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id.as_str())
        .bind(written.id.as_str())
        .bind(made.revision as i32)
        .bind(&made.text)
        .bind(made.touched.map(stamp))
        .bind(stamp(made.written_at))
        .execute(&mut **tx)
        .await
        .map_err(store)?;
        Ok(())
    }
}

/// A store failure, in the domain's own words. **The server's account never
/// crosses** — no SQL, no table names, no product (rule 53); it goes to the log
/// where an operator debugging a real failure wants it.
///
/// **A conflict is told apart from every other failure here, at the one
/// seam every write already passes through** — the same check the memory
/// rail's own `store` runs, see its doc for why.
fn store(e: sqlx::Error) -> SessionError {
    if e.as_database_error().and_then(|db| db.code()).as_deref() == Some("40001") {
        tracing::warn!(error = %e, "a write conflicted with another landing the same instant");
        return SessionError::Conflict;
    }
    if let Some(rule) = super::refused_rule(&e) {
        tracing::error!(error = %e, rule, "the session store refused a write on a rule it enforces");
        return SessionError::Refused(rule.to_string());
    }
    tracing::error!(error = %e, "the session store failed");
    SessionError::Store("the session store could not be reached".into())
}

/// One row plus its entries, as the domain's record.
/// One row of the summary query, as the domain's [`SessionSummary`] — the
/// same field-by-field reading [`session_from`] does, minus the entries no
/// query for this ever asked for.
/// **A stored `sid` filtered the way the front door reads a handle.** A
/// value that fails [`is_readable_sid`] is not a card this build could have
/// minted — the earlier card-backed adapter filtered exactly this on load,
/// and a stricter read here restores that guard. The row still reads: this
/// only answers what the `sid` column resolves to, never whether the read
/// itself may proceed. `session_id` is for the warning alone.
fn readable_sid(sid: Option<String>, session_id: &str) -> Option<Sid> {
    let sid = sid?;
    if is_readable_sid(&sid) {
        return Some(Sid(sid));
    }
    tracing::warn!(
        session = session_id,
        sid = %sid,
        "a session row carries a sid this build did not mint — read as absent"
    );
    None
}

impl DoltSessions {
    /// **The summaries of the runs a clause picks, newest start first.** The
    /// store gives the rows and the moments of each run's entries, and the count,
    /// the last beat and the order are worked out here from parsed moments
    /// ([`jojobot_domain::session::history`]), never by comparing the stored text
    /// of a moment (rule 598). No entry's text leaves the query: only when it was
    /// written and when it was touched.
    async fn summaries_where(
        &self,
        clause: &str,
        bind: Option<&str>,
    ) -> Result<Vec<jojobot_domain::session::SessionSummary>, SessionError> {
        let runs_sql = format!("SELECT {RUN_COLUMNS} FROM session s WHERE {clause}");
        let beats_sql = format!(
            "SELECT j.session, j.at, j.touched, j.happened_on
             FROM journal_entry j JOIN session s ON s.id = j.session
             WHERE {clause}"
        );
        let mut runs = sqlx::query(AssertSqlSafe(runs_sql));
        let mut moments = sqlx::query(AssertSqlSafe(beats_sql));
        if let Some(bind) = bind {
            runs = runs.bind(bind.to_string());
            moments = moments.bind(bind.to_string());
        }
        let rows = runs.fetch_all(&self.pool).await.map_err(store)?;
        let entries = moments.fetch_all(&self.pool).await.map_err(store)?;
        let mut beats: Beats = Default::default();
        // **A beat whose time cannot be read makes its run unreadable, not the read.**
        let mut damaged: std::collections::BTreeMap<SessionId, String> = Default::default();
        for row in &entries {
            let run = SessionId(row.try_get::<String, _>("session").map_err(store)?);
            match entry_moments(row) {
                Ok((at, _, touched)) => beats.entry(run).or_default().push((at, touched)),
                Err(SessionError::Unreadable { what }) => {
                    damaged.insert(run, what);
                }
                Err(other) => return Err(other),
            }
        }
        let resets = self.resets_where(clause, bind).await?;
        let mut found = Vec::with_capacity(rows.len());
        for row in &rows {
            let run = SessionId(row.try_get::<String, _>("id").map_err(store)?);
            let why = match damaged.get(&run) {
                Some(why) => Some(why.clone()),
                None => unreadable::why_unreadable(row)?,
            };
            found.push(match why {
                Some(why) => unreadable::unreadable_summary(row, why)?,
                None => summary_from(row, &beats, &resets)?,
            });
        }
        found.sort_by(|a, b| {
            jojobot_domain::session::history::newest_first(
                (a.started_at, &a.id),
                (b.started_at, &b.id),
            )
        });
        Ok(found)
    }

    /// **The compactions of the runs a clause picks, read by the one rule.** The
    /// rows of `served_reset` come back and [`resets_of`] decides which of them
    /// is a compaction, so a summary counts exactly what the usage weeks and the
    /// run totals count. SQL does not count them: a cast would read a value that
    /// is not a number as a compaction of nothing.
    async fn resets_where(
        &self,
        clause: &str,
        bind: Option<&str>,
    ) -> Result<
        std::collections::BTreeMap<SessionId, Vec<jojobot_domain::usage::Reset>>,
        SessionError,
    > {
        let sql = format!(
            "SELECT w.session, w.ordinal, w.written_at, w.changed, w.value
             FROM session_write w JOIN session s ON s.id = w.session
             WHERE w.changed = 'served_reset' AND {clause}
             ORDER BY w.session, w.ordinal"
        );
        let mut query = sqlx::query(AssertSqlSafe(sql));
        if let Some(bind) = bind {
            query = query.bind(bind.to_string());
        }
        let rows = query.fetch_all(&self.pool).await.map_err(store)?;
        let mut changes: std::collections::BTreeMap<SessionId, Vec<SessionChange>> =
            Default::default();
        for row in &rows {
            let run = SessionId(row.try_get::<String, _>("session").map_err(store)?);
            changes.entry(run).or_default().push(change_from_row(row)?);
        }
        // No start day is given: a summary only counts compactions, and the day of
        // one matters only to the usage weeks, which read every change of a run.
        Ok(changes
            .into_iter()
            .map(|(run, kept)| (run, jojobot_domain::session::resets_of(&kept, None)))
            .collect())
    }
}

fn summary_from(
    row: &sqlx::mysql::MySqlRow,
    beats: &Beats,
    resets: &std::collections::BTreeMap<SessionId, Vec<jojobot_domain::usage::Reset>>,
) -> Result<jojobot_domain::session::SessionSummary, SessionError> {
    let state: String = row.try_get("state").map_err(store)?;
    let started: String = row.try_get("started_at").map_err(store)?;
    let sid: Option<String> = row.try_get("sid").map_err(store)?;
    let id: String = row.try_get("id").map_err(store)?;
    let run = SessionId(id.clone());
    let started_at = instant_of(&started, "its start time")?;
    let kept = resets.get(&run).map_or(&[][..], Vec::as_slice);
    let held = beats.get(&run).map_or(&[][..], Vec::as_slice);
    Ok(jojobot_domain::session::SessionSummary {
        sid: readable_sid(sid, &id),
        id: run,
        bot: EntityId(row.try_get::<String, _>("bot").map_err(store)?),
        focus: row.try_get::<String, _>("focus").map_err(store)?,
        started_at,
        served_chars: row.try_get::<i64, _>("served_chars").map_err(store)? as u64,
        state: SessionState::from_token(&state).ok_or_else(no_such_state)?,
        entry_count: held.len(),
        last_beat: jojobot_domain::session::history::last_beat(started_at, held.iter().copied()),
        started_on: day_of(
            row.try_get::<Option<String>, _>("started_on")
                .map_err(store)?,
            "its stated day",
        )?,
        compactions: 0,
        served_set_back: 0,
        unreadable: None,
    }
    .counting(kept))
}

/// The key history a row carries. **A retired key is read only when all three of
/// its columns are there and the reason is one this build wrote**; a half of one
/// would be a key with no moment or no reason to tell its holder.
fn key_trail_from(row: &sqlx::mysql::MySqlRow) -> Result<KeyTrail, SessionError> {
    let hash = row
        .try_get::<Option<String>, _>("retired_key_hash")
        .map_err(store)?;
    let at = row
        .try_get::<Option<String>, _>("retired_at")
        .map_err(store)?;
    let by = row
        .try_get::<Option<String>, _>("retired_by")
        .map_err(store)?;
    let retired = match (hash, at, by.as_deref().and_then(RetiredBy::from_token)) {
        (Some(hash), Some(at), Some(by)) => Some(RetiredKey {
            hash,
            at: instant_of(&at, "the moment its key was retired")?,
            by,
        }),
        _ => None,
    };
    let taken_at = row
        .try_get::<Option<String>, _>("taken_at")
        .map_err(store)?
        .as_deref()
        .map(|at| instant_of(at, "the moment its key was taken"))
        .transpose()?;
    let displaced_by = row
        .try_get::<Option<String>, _>("displaced_by")
        .map_err(store)?
        .map(Sid);
    Ok(KeyTrail {
        retired,
        taken_at,
        displaced_by,
    })
}

fn session_from(
    row: &sqlx::mysql::MySqlRow,
    entries: &[sqlx::mysql::MySqlRow],
) -> Result<Session, SessionError> {
    let state: String = row.try_get("state").map_err(store)?;
    let started: String = row.try_get("started_at").map_err(store)?;
    let sid: Option<String> = row.try_get("sid").map_err(store)?;
    let id: String = row.try_get("id").map_err(store)?;
    Ok(Session {
        sid: readable_sid(sid, &id),
        key_hash: row
            .try_get::<Option<String>, _>("key_hash")
            .map_err(store)?,
        id: SessionId(id),
        timezone: row
            .try_get::<Option<String>, _>("timezone")
            .map_err(store)?,
        bot: EntityId(row.try_get::<String, _>("bot").map_err(store)?),
        focus: row.try_get::<String, _>("focus").map_err(store)?,
        started_at: instant_of(&started, "its start time")?,
        started_on: day_of(
            row.try_get::<Option<String>, _>("started_on")
                .map_err(store)?,
            "its stated day",
        )?,
        stated_day: day_of(
            row.try_get::<Option<String>, _>("stated_day")
                .map_err(store)?,
            "its working day",
        )?,
        served_chars: row.try_get::<i64, _>("served_chars").map_err(store)? as u64,
        charter_told_at: row
            .try_get::<Option<String>, _>("charter_told_at")
            .map_err(store)?
            .as_deref()
            .map(|at| instant_of(at, "the moment its charter was told"))
            .transpose()?,
        key_trail: key_trail_from(row)?,
        wrap_window: row
            .try_get::<Option<String>, _>("wrap_window")
            .map_err(store)?
            .as_deref()
            .and_then(jojobot_domain::session::WrapWindow::from_stored),
        // A state token the store does not recognize is a record jojobot
        // cannot read. It is not a session in an unknown column — there are no
        // columns here — so it is damage a person repairs.
        state: SessionState::from_token(&state).ok_or_else(no_such_state)?,
        entries: entries.iter().map(entry_from).collect::<Result<_, _>>()?,
    })
}

/// One chronology row, as the domain's entry.
fn entry_from(row: &sqlx::mysql::MySqlRow) -> Result<JournalEntry, SessionError> {
    let (at, on, touched) = entry_moments(row)?;
    Ok(JournalEntry {
        id: EntryId(row.try_get::<String, _>("id").map_err(store)?),
        at,
        on,
        text: row.try_get::<String, _>("text").map_err(store)?,
        touched,
        beat: row.try_get::<Option<String>, _>("beat").map_err(store)?,
        closing_focus: row
            .try_get::<Option<String>, _>("closing_focus")
            .map_err(store)?,
        closing: row.try_get::<i64, _>("closing").map_err(store)? != 0,
    })
}

/// An instant as it is stored: RFC 3339, nanoseconds intact.
/// One `session_write` row as the domain's record. A record an older build
/// appended has no kind and no moment, and reads as unrecorded.
fn change_from_row(row: &sqlx::mysql::MySqlRow) -> Result<SessionChange, SessionError> {
    let changed = row
        .try_get::<Option<String>, _>("changed")
        .map_err(store)?
        .map(|token| {
            ChangeKind::from_token(&token).ok_or_else(|| {
                SessionError::Store(
                    "a stored session change is of a kind this build does not know".into(),
                )
            })
        })
        .transpose()?;
    Ok(SessionChange {
        ordinal: row.try_get::<i64, _>("ordinal").map_err(store)?.max(0) as usize,
        changed,
        value: row.try_get("value").map_err(store)?,
        written_at: row
            .try_get::<Option<String>, _>("written_at")
            .map_err(store)?
            .map(|at| instant(&at))
            .transpose()?,
    })
}

/// **The moments of each run's entries**, parsed: what a run's count and last
/// beat are made of. Parsed rather than compared as stored text, because the text
/// of a moment varies in length and text order is not time order (rule 598).
type Beats = std::collections::BTreeMap<SessionId, Vec<(Timestamp, Option<Timestamp>)>>;

/// **Run ids newest first by their moment**, the order every store gives
/// ([`jojobot_domain::session::history::newest_first`]). A start that cannot be
/// read is a store failure, never a run sorted first or last (rule 130).
fn newest_first_ids(rows: Vec<(String, String)>) -> Result<Vec<String>, SessionError> {
    let mut parsed: Vec<(Timestamp, SessionId)> = rows
        .into_iter()
        .map(|(id, started)| Ok((instant(&started)?, SessionId(id))))
        .collect::<Result<_, SessionError>>()?;
    parsed.sort_by(|a, b| jojobot_domain::session::history::newest_first((a.0, &a.1), (b.0, &b.1)));
    Ok(parsed.into_iter().map(|(_, id)| id.0).collect())
}

/// One `journal_entry_write` row as the domain's record.
fn wording_from_row(row: &sqlx::mysql::MySqlRow) -> Result<Wording, SessionError> {
    Ok(Wording {
        revision: row.try_get::<i32, _>("ordinal").map_err(store)?.max(0) as usize,
        text: row.try_get("text").map_err(store)?,
        touched: row
            .try_get::<Option<String>, _>("touched")
            .map_err(store)?
            .map(|at| instant(&at))
            .transpose()?,
        written_at: instant(&row.try_get::<String, _>("written_at").map_err(store)?)?,
    })
}

fn stamp(at: Timestamp) -> String {
    at.to_string()
}

#[async_trait]
impl Sessions for DoltSessions {
    async fn sessions_of(&self, bot: &EntityId) -> Result<Vec<Session>, SessionError> {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT id, started_at FROM session WHERE bot = ?")
                .bind(bot.as_str())
                .fetch_all(&self.pool)
                .await
                .map_err(store)?;
        let ids = newest_first_ids(rows)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        let mut found = Vec::with_capacity(ids.len());
        for id in ids {
            found.push(self.read_in(&mut tx, &SessionId(id)).await?);
        }
        tx.commit().await.map_err(store)?;
        Ok(found)
    }

    async fn all_sessions(&self) -> Result<Vec<Session>, SessionError> {
        let rows: Vec<(String, String)> = sqlx::query_as("SELECT id, started_at FROM session")
            .fetch_all(&self.pool)
            .await
            .map_err(store)?;
        let ids = newest_first_ids(rows)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        let mut found = Vec::with_capacity(ids.len());
        for id in ids {
            found.push(self.read_in(&mut tx, &SessionId(id)).await?);
        }
        tx.commit().await.map_err(store)?;
        Ok(found)
    }

    /// **One aggregate, not a read of any run.** `session_write` is written on
    /// every change of every session, and its count over the changes the search
    /// index reads answers "has anything changed" without touching a run's
    /// chronology. No moment rides along: this is a signal, and each record
    /// carries its own moment for a reader of the history.
    async fn write_summary(&self) -> Result<Option<(i64, Option<Timestamp>)>, SessionError> {
        // **The changes the index reads, and the records that predate the kind**:
        // a change nothing searches for is recorded and moves nothing, so an
        // answer's size added to a run does not make the index read every run.
        let moving: Vec<&str> = ChangeKind::ALL
            .into_iter()
            .filter(|kind| kind.moves_the_index())
            .map(ChangeKind::as_token)
            .collect();
        let marks = vec!["?"; moving.len()].join(", ");
        let sql = format!(
            "SELECT COUNT(*) FROM session_write WHERE changed IS NULL OR changed IN ({marks})"
        );
        let mut count = sqlx::query_scalar(AssertSqlSafe(sql.as_str()));
        for token in moving {
            count = count.bind(token);
        }
        let count: i64 = count.fetch_one(&self.pool).await.map_err(store)?;
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
        self.summaries_where("s.bot = ?", Some(bot.as_str())).await
    }

    /// **The same read [`summaries_of`](Self::summaries_of) makes,
    /// minus the one clause that scopes it to a bot.** Whosever a run is,
    /// this answers what it is without paying for what it said.
    async fn all_summaries(
        &self,
    ) -> Result<Vec<jojobot_domain::session::SessionSummary>, SessionError> {
        self.summaries_where("1 = 1", None).await
    }

    /// **The same read [`summaries_of`](Self::summaries_of) makes, narrowed to
    /// one run.** An id that could never be a run's is no run, and is not an
    /// error: the caller asked whether a run exists.
    async fn summary_of(
        &self,
        id: &SessionId,
    ) -> Result<Option<jojobot_domain::session::SessionSummary>, SessionError> {
        if validate_session_id(id).is_err() {
            return Ok(None);
        }
        Ok(self
            .summaries_where("s.id = ?", Some(id.as_str()))
            .await?
            .into_iter()
            .next())
    }

    /// One grouped count over the session table, and nothing of a run's text or
    /// its beats.
    async fn run_counts(
        &self,
    ) -> Result<std::collections::BTreeMap<EntityId, usize>, SessionError> {
        let rows = sqlx::query("SELECT bot, COUNT(*) AS runs FROM session GROUP BY bot")
            .fetch_all(&self.pool)
            .await
            .map_err(store)?;
        rows.iter()
            .map(|row| {
                Ok((
                    EntityId(row.try_get::<String, _>("bot").map_err(store)?),
                    row.try_get::<i64, _>("runs").map_err(store)? as usize,
                ))
            })
            .collect()
    }

    /// **Every day a run was served anything**, with the bot of the run. The bot
    /// is the badge the run is stored under.
    async fn served_days(&self) -> Result<Vec<ServedDay>, SessionError> {
        let rows = sqlx::query(
            "SELECT d.session, s.bot, d.day, d.chars FROM served_day d \
             JOIN session s ON s.id = d.session ORDER BY d.session, d.day",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store)?;
        rows.iter()
            .map(|row| {
                let day: String = row.try_get("day").map_err(store)?;
                Ok(ServedDay {
                    run: SessionId(row.try_get("session").map_err(store)?),
                    bot: EntityId(row.try_get("bot").map_err(store)?),
                    day: day.parse().map_err(|_| {
                        SessionError::Store(format!("a count of served holds the day {day:?}"))
                    })?,
                    chars: row.try_get::<i64, _>("chars").map_err(store)?.max(0) as u64,
                })
            })
            .collect()
    }

    /// **One statement that makes the row or adds to it.** A call with no bot is
    /// kept under the empty key.
    ///
    /// **Two adds that run together are both kept**: the real store would merge
    /// them by keeping one and refuse neither, so the add runs under
    /// [`Self::increment`]. The write gate of [`crate::fold::sessions`] above
    /// the store does not replace it (decision log 292).
    async fn count_call(&self, key: &CallKey) -> Result<(), SessionError> {
        let _adding = self.increment().await;
        let (outcome, fix_by) = key.outcome.tokens();
        // **Both counts or neither**, so the counts of a bot and of its runs agree.
        let mut tx = self.pool.begin().await.map_err(store)?;
        sqlx::query(
            "INSERT INTO tool_call_count (bot_key, day, verb, outcome, fix_by, calls) \
             VALUES (?, ?, ?, ?, ?, 1) ON DUPLICATE KEY UPDATE calls = calls + 1",
        )
        .bind(key.bot.as_ref().map_or("", EntityId::as_str))
        .bind(key.day.to_string())
        .bind(&key.verb)
        .bind(outcome)
        .bind(fix_by)
        .execute(&mut *tx)
        .await
        .map_err(store)?;
        if let Some(run) = &key.run {
            sqlx::query(
                "INSERT INTO run_call_count (run, verb, outcome, fix_by, calls) \
                 VALUES (?, ?, ?, ?, 1) ON DUPLICATE KEY UPDATE calls = calls + 1",
            )
            .bind(run)
            .bind(&key.verb)
            .bind(outcome)
            .bind(fix_by)
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        }
        tx.commit().await.map_err(store)?;
        Ok(())
    }

    /// **The counts of the runs asked for, or of every run.** A row whose outcome
    /// this build cannot read is stored damage and is named rather than skipped.
    async fn run_calls(&self, runs: Option<&[String]>) -> Result<Vec<RunCall>, SessionError> {
        let rows = match runs {
            Some([]) => return Ok(Vec::new()),
            Some(named) => {
                let marks = vec!["?"; named.len()].join(", ");
                let sql = format!(
                    "SELECT run, verb, outcome, fix_by, calls FROM run_call_count \
                     WHERE run IN ({marks}) ORDER BY run, verb, outcome, fix_by"
                );
                let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
                for name in named {
                    query = query.bind(name);
                }
                query.fetch_all(&self.pool).await
            }
            None => {
                sqlx::query(
                    "SELECT run, verb, outcome, fix_by, calls FROM run_call_count \
                     ORDER BY run, verb, outcome, fix_by",
                )
                .fetch_all(&self.pool)
                .await
            }
        }
        .map_err(store)?;
        rows.iter()
            .map(|row| {
                let outcome: String = row.try_get("outcome").map_err(store)?;
                let fix_by: String = row.try_get("fix_by").map_err(store)?;
                Ok(RunCall {
                    run: row.try_get("run").map_err(store)?,
                    verb: row.try_get("verb").map_err(store)?,
                    outcome: CallOutcome::from_tokens(&outcome, &fix_by).ok_or_else(|| {
                        SessionError::Store(format!(
                            "a count of a run's calls holds the outcome {outcome:?}"
                        ))
                    })?,
                    count: row.try_get::<i64, _>("calls").map_err(store)?.max(0) as u64,
                })
            })
            .collect()
    }

    /// **Every row of the counts.** A row whose day or outcome this build cannot
    /// read is stored damage and is named rather than skipped.
    async fn call_counts(&self) -> Result<Vec<CallCount>, SessionError> {
        let rows = sqlx::query(
            "SELECT bot_key, day, verb, outcome, fix_by, calls FROM tool_call_count \
                 ORDER BY bot_key, day, verb, outcome, fix_by",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store)?;
        rows.iter()
            .map(|row| {
                let bot_key: String = row.try_get("bot_key").map_err(store)?;
                let day: String = row.try_get("day").map_err(store)?;
                let outcome: String = row.try_get("outcome").map_err(store)?;
                let fix_by: String = row.try_get("fix_by").map_err(store)?;
                let unread = |what: &str, held: &str| {
                    SessionError::Store(format!("a count of calls holds {what} {held:?}"))
                };
                Ok(CallCount {
                    key: CallKey {
                        bot: (!bot_key.is_empty()).then_some(EntityId(bot_key)),
                        day: day.parse().map_err(|_| unread("the day", &day))?,
                        verb: row.try_get("verb").map_err(store)?,
                        outcome: CallOutcome::from_tokens(&outcome, &fix_by)
                            .ok_or_else(|| unread("the outcome", &outcome))?,
                        run: None,
                    },
                    count: row.try_get::<i64, _>("calls").map_err(store)?.max(0) as u64,
                })
            })
            .collect()
    }

    async fn read_session(&self, id: &SessionId) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        let session = self.read_in(&mut tx, id).await?;
        tx.commit().await.map_err(store)?;
        Ok(session)
    }

    async fn begin(&self, new: NewSession) -> Result<Session, SessionError> {
        validate_focus(&new.focus)?;
        // **A sid is minted, never written by hand.** The same refusal shape
        // a bad focus already gets: nothing is written, and the caller is
        // told why before anything reaches the store.
        if !is_readable_sid(new.sid.as_str()) {
            return Err(SessionError::InvalidEntry(format!(
                "sid '{}' is not a handle this build ever mints — a sid is {} characters drawn \
                 from its own alphabet, never one a caller writes",
                new.sid.as_str(),
                jojobot_domain::session::SID_LEN,
            )));
        }
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
            let session = self.read_in(&mut tx, &SessionId(id)).await?;
            tx.commit().await.map_err(store)?;
            return Ok(session);
        }

        // **A name the boot minted is the card's id**, and an id some card
        // already wears is refused rather than shared: the key is the table's.
        let id = match new.id {
            Some(id) => {
                validate_session_id(&id)?;
                let taken: Option<i64> = sqlx::query_scalar("SELECT 1 FROM session WHERE id = ?")
                    .bind(id.as_str())
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(store)?;
                if taken.is_some() {
                    return Err(SessionError::InvalidEntry(format!(
                        "session '{id}' already exists, so it cannot begin again"
                    )));
                }
                id
            }
            None => SessionId(
                mint(
                    &mut tx,
                    &self.draw,
                    "SELECT 1 FROM session WHERE id = ?",
                    None,
                )
                .await?,
            ),
        };
        sqlx::query(
            "INSERT INTO session (id, sid, key_hash, bot, focus, started_at, state, timezone, started_on)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.as_str())
        .bind(new.sid.as_str())
        .bind(new.key_hash.as_deref())
        .bind(new.bot.as_str())
        .bind(new.focus.trim())
        .bind(stamp(new.started_at))
        .bind(SessionState::Active.as_token())
        .bind(new.timezone.as_deref())
        .bind(new.started_on.map(|day| day.to_string()))
        .execute(&mut *tx)
        .await
        .map_err(store)?;
        self.keep_change(&mut tx, &id, Change::Begin(new.focus.trim()))
            .await?;
        let session = self.read_in(&mut tx, &id).await?;
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
        self.writable(&mut tx, id).await?;
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
            "INSERT INTO journal_entry
                 (session, id, ordinal, at, text, touched, beat, happened_on, closing_focus,
                  closing)
             VALUES (?, ?, ?, ?, ?, NULL, ?, ?, ?, ?)",
        )
        .bind(id.as_str())
        .bind(entry_id.as_str())
        .bind(ordinal)
        .bind(stamp(entry.at))
        .bind(normalize_entry(&entry.text))
        .bind(entry.beat.as_deref())
        .bind(entry.on.map(|day| day.to_string()))
        .bind(entry.closing_focus.as_deref())
        .bind(entry.closing)
        .execute(&mut *tx)
        .await
        .map_err(store)?;
        self.keep_change(&mut tx, id, Change::Entry(&entry_id))
            .await?;
        let written = read_entry(&mut tx, id, &entry_id).await?;
        self.keep_wording(&mut tx, id, &written).await?;
        // **A part of a write that commits in several** (an entry, then the focus
        // it moves), so the id's row for this part goes in with it.
        super::requests::commit(tx, &self.clock, || {
            serde_json::json!({ "entry": written.id.as_str() }).to_string()
        })
        .await
        .map_err(store)?;
        Ok(written)
    }

    async fn amend_last(&self, id: &SessionId, text: &str) -> Result<JournalEntry, SessionError> {
        validate_session_id(id)?;
        validate_entry(text)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        self.writable(&mut tx, id).await?;
        let newest: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM journal_entry WHERE session = ? AND beat IS NULL \
             ORDER BY ordinal DESC LIMIT 1",
        )
        .bind(id.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(store)?;
        let (newest,) = newest.ok_or_else(|| SessionError::NoEntries {
            attempted: id.to_string(),
        })?;
        let entry_id = EntryId(newest);
        sqlx::query("UPDATE journal_entry SET text = ? WHERE session = ? AND id = ?")
            .bind(normalize_entry(text))
            .bind(id.as_str())
            .bind(entry_id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        self.keep_change(&mut tx, id, Change::AmendLast(&entry_id))
            .await?;
        let written = read_entry(&mut tx, id, &entry_id).await?;
        self.keep_wording(&mut tx, id, &written).await?;
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
        self.writable(&mut tx, id).await?;
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
        self.keep_change(&mut tx, id, Change::AmendBeat(entry))
            .await?;
        let written = read_entry(&mut tx, id, entry).await?;
        self.keep_wording(&mut tx, id, &written).await?;
        tx.commit().await.map_err(store)?;
        Ok(written)
    }

    async fn set_focus(&self, id: &SessionId, focus: &str) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        validate_focus(focus)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        self.writable(&mut tx, id).await?;
        sqlx::query("UPDATE session SET focus = ? WHERE id = ?")
            .bind(focus.trim())
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        self.keep_change(&mut tx, id, Change::Focus(focus.trim()))
            .await?;
        let session = self.read_in(&mut tx, id).await?;
        super::requests::commit(tx, &self.clock, || {
            serde_json::json!({ "focus": focus.trim() }).to_string()
        })
        .await
        .map_err(store)?;
        Ok(session)
    }

    async fn set_timezone(
        &self,
        id: &SessionId,
        timezone: Option<&str>,
    ) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        self.writable(&mut tx, id).await?;
        sqlx::query("UPDATE session SET timezone = ? WHERE id = ?")
            .bind(timezone.map(str::trim).filter(|z| !z.is_empty()))
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        self.keep_change(
            &mut tx,
            id,
            Change::Timezone(timezone.map(str::trim).filter(|z| !z.is_empty())),
        )
        .await?;
        let session = self.read_in(&mut tx, id).await?;
        tx.commit().await.map_err(store)?;
        Ok(session)
    }

    async fn set_stated_day(
        &self,
        id: &SessionId,
        day: Option<jiff::civil::Date>,
    ) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        self.writable(&mut tx, id).await?;
        sqlx::query("UPDATE session SET stated_day = ? WHERE id = ?")
            .bind(day.map(|d| d.to_string()))
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        self.keep_change(&mut tx, id, Change::StatedDay(day))
            .await?;
        let session = self.read_in(&mut tx, id).await?;
        tx.commit().await.map_err(store)?;
        Ok(session)
    }

    async fn set_wrap_window(
        &self,
        id: &SessionId,
        window: Option<WrapWindow>,
    ) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        // **Not `writable`**: this is the write that is about a closed run, so
        // it reads the run and refuses any state but `wrapped`.
        let held = self.read_in(&mut tx, id).await?;
        if held.state != SessionState::Wrapped {
            return Err(SessionError::NotWrapped {
                attempted: id.to_string(),
                state: held.state,
            });
        }
        sqlx::query("UPDATE session SET wrap_window = ? WHERE id = ?")
            .bind(window.as_ref().map(WrapWindow::to_stored))
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        self.keep_change(&mut tx, id, Change::WrapWindow(window.as_ref()))
            .await?;
        let session = self.read_in(&mut tx, id).await?;
        tx.commit().await.map_err(store)?;
        Ok(session)
    }

    async fn close(&self, id: &SessionId, to: SessionState) -> Result<Session, SessionError> {
        self.close_with(id, to, None).await
    }

    async fn close_displaced(&self, id: &SessionId, by: &Sid) -> Result<Session, SessionError> {
        self.close_with(id, SessionState::Abandoned, Some(by)).await
    }

    async fn add_served(
        &self,
        id: &SessionId,
        chars: u64,
        day: jiff::civil::Date,
    ) -> Result<(), SessionError> {
        validate_session_id(id)?;
        // **Held to the commit**, for both adds below: the total of the run and the
        // total of its day.
        let _adding = self.increment().await;
        let mut tx = self.pool.begin().await.map_err(store)?;
        // Existence only, never refused on a closed session — see the
        // trait's own doc. Never a full read: this never touches the
        // chronology and has no state to check.
        //
        // **One statement.** The update matches the row by id, so a row that
        // is there is changed and needs no question first. Only an update that
        // changed no row asks: a total of nothing changes none, and an id no
        // run wears matches none.
        let added = sqlx::query("UPDATE session SET served_chars = served_chars + ? WHERE id = ?")
            .bind(chars as i64)
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        if added.rows_affected() == 0 {
            self.exists(&mut tx, id).await?;
        }
        // **The day is added in the same transaction**, so the days of a run add up
        // to the total above. A run that does not exist was refused by now.
        if chars > 0 {
            sqlx::query(
                "INSERT INTO served_day (session, day, chars) VALUES (?, ?, ?) \
                 ON DUPLICATE KEY UPDATE chars = chars + ?",
            )
            .bind(id.as_str())
            .bind(day.to_string())
            .bind(chars as i64)
            .bind(chars as i64)
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        }
        // **No change is recorded here.** An answer is served many times a
        // run, and a record of each would grow with them: the reset records
        // the total instead.
        tx.commit().await.map_err(store)?;
        Ok(())
    }

    async fn reset_served(&self, id: &SessionId) -> Result<(), SessionError> {
        validate_session_id(id)?;
        // **The reset reads the total and zeroes it, so it takes the lock every
        // increment takes**: an add that landed between the two would be in the
        // total the reset recorded and in neither the old total nor the new one.
        let _resetting = self.increment().await;
        let mut tx = self.pool.begin().await.map_err(store)?;
        // **The total the reset sets back is its record.** Read in the same
        // transaction, so the record holds what the row held; a run no row
        // holds is refused.
        let total: Option<i64> =
            sqlx::query_scalar("SELECT served_chars FROM session WHERE id = ?")
                .bind(id.as_str())
                .fetch_optional(&mut *tx)
                .await
                .map_err(store)?;
        // **Existence is `exists`'s question, asked only when the update matched
        // no row**, as `add_served` asks it. A run no row holds is refused there.
        let reset = sqlx::query("UPDATE session SET served_chars = 0 WHERE id = ?")
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        if reset.rows_affected() == 0 {
            self.exists(&mut tx, id).await?;
        }
        let total = total.unwrap_or(0);
        self.keep_change(&mut tx, id, Change::ServedReset(total.max(0) as u64))
            .await?;
        // **A part of a write that commits in several** (a mark, then the restart
        // of the total), so the id's row for this part goes in with it.
        super::requests::commit(tx, &self.clock, || {
            serde_json::json!({ "reset": total.max(0) }).to_string()
        })
        .await
        .map_err(store)?;
        Ok(())
    }

    async fn reopen(&self, id: &SessionId) -> Result<Session, SessionError> {
        validate_session_id(id)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        let held = self.read_in(&mut tx, id).await?;
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
                self.keep_change(&mut tx, id, Change::State(SessionState::Active))
                    .await?;
                self.read_in(&mut tx, id).await?
            }
            SessionState::Wrapped => {
                return Err(SessionError::Closed {
                    attempted: id.to_string(),
                    state: SessionState::Wrapped,
                    displaced_by: None,
                });
            }
        };
        tx.commit().await.map_err(store)?;
        Ok(session)
    }

    async fn set_key_hash(
        &self,
        id: &SessionId,
        hash: &str,
        trail: &KeyTrail,
    ) -> Result<(), SessionError> {
        validate_session_id(id)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        // Existence only, never refused on a closed session: see the trait.
        self.exists(&mut tx, id).await?;
        // **One statement for the hash and the trail**, so a restart never finds
        // a new key with the old key's history.
        sqlx::query(
            "UPDATE session SET key_hash = ?, retired_key_hash = ?, retired_at = ?, \
             retired_by = ?, taken_at = ?, displaced_by = ? WHERE id = ?",
        )
        .bind(hash)
        .bind(trail.retired.as_ref().map(|r| r.hash.as_str()))
        .bind(trail.retired.as_ref().map(|r| stamp(r.at)))
        .bind(trail.retired.as_ref().map(|r| r.by.as_token()))
        .bind(trail.taken_at.map(stamp))
        .bind(trail.displaced_by.as_ref().map(|by| by.as_str()))
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .map_err(store)?;
        self.keep_change(&mut tx, id, Change::Key).await?;
        tx.commit().await.map_err(store)?;
        Ok(())
    }

    async fn note_minted(&self, run: &MintedRun) -> Result<(), SessionError> {
        // **Written once.** A second note of one name is the same run asked
        // twice, and it changes nothing.
        sqlx::query(
            "INSERT IGNORE INTO session_key (name, key_hash, bot, zone, day, minted_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(run.name.as_str())
        .bind(&run.key_hash)
        .bind(run.bot.as_str())
        .bind(run.zone.as_deref())
        .bind(run.day.map(|day| day.to_string()))
        .bind(stamp(run.minted_at))
        .execute(&self.pool)
        .await
        .map_err(store)?;
        Ok(())
    }

    async fn minted_without_a_card(&self) -> Result<Vec<MintedRun>, SessionError> {
        let rows = sqlx::query(
            "SELECT name, key_hash, bot, zone, day, minted_at FROM session_key k \
             WHERE NOT EXISTS (SELECT 1 FROM session s WHERE s.id = k.name)",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store)?;
        let mut found = rows
            .iter()
            .map(|row| {
                Ok(MintedRun {
                    name: Sid(row.try_get::<String, _>("name").map_err(store)?),
                    key_hash: row.try_get::<String, _>("key_hash").map_err(store)?,
                    bot: EntityId(row.try_get::<String, _>("bot").map_err(store)?),
                    zone: row.try_get::<Option<String>, _>("zone").map_err(store)?,
                    day: day(row.try_get::<Option<String>, _>("day").map_err(store)?)?,
                    minted_at: instant(&row.try_get::<String, _>("minted_at").map_err(store)?)?,
                })
            })
            .collect::<Result<Vec<MintedRun>, SessionError>>()?;
        // Ordered here, not in SQL: the moments are text of no fixed width, and
        // only a parsed moment orders.
        found.sort_by(|a, b| (a.minted_at, &a.name.0).cmp(&(b.minted_at, &b.name.0)));
        Ok(found)
    }

    async fn prune_minted(&self, before: Timestamp) -> Result<usize, SessionError> {
        let rows = sqlx::query(
            "SELECT name, minted_at, EXISTS (SELECT 1 FROM session s WHERE s.id = k.name) \
             AS has_card FROM session_key k",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store)?;
        let mut pruned = 0;
        for row in &rows {
            let name: String = row.try_get("name").map_err(store)?;
            let has_card: i64 = row.try_get("has_card").map_err(store)?;
            let minted_at = instant(&row.try_get::<String, _>("minted_at").map_err(store)?)?;
            if has_card == 0 && minted_at >= before {
                continue;
            }
            sqlx::query("DELETE FROM session_key WHERE name = ?")
                .bind(&name)
                .execute(&self.pool)
                .await
                .map_err(store)?;
            pruned += 1;
        }
        Ok(pruned)
    }

    async fn set_charter_told(&self, id: &SessionId, at: Timestamp) -> Result<(), SessionError> {
        validate_session_id(id)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        // Existence only, never refused on a closed session: see the trait.
        self.exists(&mut tx, id).await?;
        sqlx::query("UPDATE session SET charter_told_at = ? WHERE id = ?")
            .bind(stamp(at))
            .bind(id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        self.keep_change(&mut tx, id, Change::CharterTold(at))
            .await?;
        tx.commit().await.map_err(store)?;
        Ok(())
    }

    async fn scan_session_history(&self) -> Result<SessionHistories, SessionError> {
        let change_rows = sqlx::query(
            "SELECT session, ordinal, written_at, changed, value FROM session_write
             ORDER BY session, ordinal",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store)?;
        let mut changes: Vec<(SessionId, Vec<SessionChange>)> = Vec::new();
        for row in &change_rows {
            let run = SessionId(row.try_get::<String, _>("session").map_err(store)?);
            let change = change_from_row(row)?;
            match changes.last_mut() {
                Some((held, kept)) if *held == run => kept.push(change),
                _ => changes.push((run, vec![change])),
            }
        }
        let wording_rows = sqlx::query(
            "SELECT session, entry, ordinal, text, touched, written_at FROM journal_entry_write
             ORDER BY session, entry, ordinal",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store)?;
        let mut wordings: Vec<(SessionId, EntryId, Vec<Wording>)> = Vec::new();
        for row in &wording_rows {
            let run = SessionId(row.try_get::<String, _>("session").map_err(store)?);
            let entry = EntryId(row.try_get::<String, _>("entry").map_err(store)?);
            let wording = wording_from_row(row)?;
            match wordings.last_mut() {
                Some((held, held_entry, kept)) if *held == run && *held_entry == entry => {
                    kept.push(wording)
                }
                _ => wordings.push((run, entry, vec![wording])),
            }
        }
        Ok(SessionHistories { changes, wordings })
    }

    async fn session_history(&self, id: &SessionId) -> Result<Vec<SessionChange>, SessionError> {
        validate_session_id(id)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        self.exists(&mut tx, id).await?;
        let rows = sqlx::query(
            "SELECT ordinal, written_at, changed, value FROM session_write
             WHERE session = ? ORDER BY ordinal",
        )
        .bind(id.as_str())
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        tx.commit().await.map_err(store)?;
        rows.iter().map(change_from_row).collect()
    }

    async fn journal_history(
        &self,
        id: &SessionId,
        entry: &EntryId,
    ) -> Result<Vec<Wording>, SessionError> {
        validate_session_id(id)?;
        let mut tx = self.pool.begin().await.map_err(store)?;
        self.exists(&mut tx, id).await?;
        let held: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM journal_entry WHERE session = ? AND id = ?")
                .bind(id.as_str())
                .bind(entry.as_str())
                .fetch_optional(&mut *tx)
                .await
                .map_err(store)?;
        held.ok_or_else(|| SessionError::NoEntries {
            attempted: id.to_string(),
        })?;
        let rows = sqlx::query(
            "SELECT ordinal, text, touched, written_at FROM journal_entry_write
             WHERE session = ? AND entry = ? ORDER BY ordinal",
        )
        .bind(id.as_str())
        .bind(entry.as_str())
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        tx.commit().await.map_err(store)?;
        rows.iter().map(wording_from_row).collect()
    }

    async fn note_charter_change(
        &self,
        bot: &EntityId,
        change: CharterChange,
        by_session: Option<&str>,
    ) -> Result<(), SessionError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let ordinal = ordinal::next(&mut tx, "charter_change_write", &[("bot", bot.as_str())])
            .await
            .map_err(store)?;
        sqlx::query(
            "INSERT INTO charter_change_write
               (bot, ordinal, changed_by, changed_by_session, changed_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(bot.as_str())
        .bind(ordinal)
        .bind(change.by.as_str())
        .bind(by_session)
        .bind(stamp(change.at))
        .execute(&mut *tx)
        .await
        .map_err(store)?;
        // A bot's own rewrite is kept above and replaces nothing here: the
        // reads answer with the newest change by another identity.
        if change.by != *bot {
            sqlx::query(
                "INSERT INTO charter_change (bot, changed_by, changed_at) VALUES (?, ?, ?) \
                 ON DUPLICATE KEY UPDATE changed_by = VALUES(changed_by), \
                 changed_at = VALUES(changed_at)",
            )
            .bind(bot.as_str())
            .bind(change.by.as_str())
            .bind(stamp(change.at))
            .execute(&mut *tx)
            .await
            .map_err(store)?;
        }
        // **A part of a write that commits in several** (a charter, then the
        // note of the change), so the id's row for this part goes in with it.
        super::requests::commit(tx, &self.clock, || {
            serde_json::json!({ "bot": bot.as_str() }).to_string()
        })
        .await
        .map_err(store)?;
        Ok(())
    }

    async fn charter_change_of(
        &self,
        bot: &EntityId,
    ) -> Result<Option<CharterChange>, SessionError> {
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT changed_by, changed_at FROM charter_change WHERE bot = ?")
                .bind(bot.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(store)?;
        row.map(|(by, at)| {
            Ok(CharterChange {
                by: EntityId(by),
                at: instant(&at)?,
            })
        })
        .transpose()
    }

    async fn charter_changes(
        &self,
    ) -> Result<Option<std::collections::BTreeMap<EntityId, CharterChange>>, SessionError> {
        let rows: Vec<(String, String, String)> =
            sqlx::query_as("SELECT bot, changed_by, changed_at FROM charter_change")
                .fetch_all(&self.pool)
                .await
                .map_err(store)?;
        rows.into_iter()
            .map(|(bot, by, at)| {
                Ok((
                    EntityId(bot),
                    CharterChange {
                        by: EntityId(by),
                        at: instant(&at)?,
                    },
                ))
            })
            .collect::<Result<_, SessionError>>()
            .map(Some)
    }
}

/// One entry by id, or the miss that says the id names nothing here.
async fn read_entry(
    tx: &mut Transaction<'_, MySql>,
    session: &SessionId,
    entry: &EntryId,
) -> Result<JournalEntry, SessionError> {
    let row = sqlx::query(
        "SELECT id, at, text, touched, beat, happened_on, closing_focus, closing FROM journal_entry \
         WHERE session = ? AND id = ?",
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

mod moments;
mod unreadable;
#[cfg(test)]
mod unreadable_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dolt::tests::{free_port, marks};
    use crate::dolt::{Dolt, migrate};
    use crate::testing::Scratch;

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
                    id: None,
                    key_hash: None,
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
                    id: None,
                    key_hash: None,
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
                matches!(refused, Err(SessionError::Unreadable { .. })),
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

    /// **Rows written by the old stamp read in moment order, with no rewrite**
    /// (rule 342, rule 598). The old writer stored a moment with the digits it
    /// needed, so one store holds `:05Z`, `:05.5Z`, `:05.100000000Z` and `:05.12Z`,
    /// which sort as text in an order that is not time. Every newest-first read
    /// answers by moment, and the stored text is as it was. The pair: a stamp
    /// that is no moment fails the read of whole runs and is named by the
    /// summaries, and no run is sorted first for it (rule 130).
    #[tokio::test]
    async fn rows_of_mixed_stamp_widths_read_in_moment_order_and_an_unreadable_stamp_fails_the_read()
     {
        let scratch = Scratch::new("mixed-stamp-widths");
        let mut store = Dolt::start(&scratch.0, free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");
        migrate::seed_kinds(store.pool())
            .await
            .expect("the kinds are seeded");
        let sessions = DoltSessions::open(store.pool().clone());

        for (id, at) in [
            ("whole", "2026-01-01T00:00:05Z"),
            ("half", "2026-01-01T00:00:05.5Z"),
            ("tenth", "2026-01-01T00:00:05.100000000Z"),
            ("hundredth", "2026-01-01T00:00:05.12Z"),
        ] {
            row(&store, id, at, "active").await;
        }
        let expected = ["half", "hundredth", "tenth", "whole"];
        let bot = EntityId("bot:gamma".into());
        let listed: Vec<String> = sessions
            .sessions_of(&bot)
            .await
            .expect("reads")
            .into_iter()
            .map(|run| run.id.0)
            .collect();
        assert_eq!(listed, expected);
        let every: Vec<String> = sessions
            .all_sessions()
            .await
            .expect("reads")
            .into_iter()
            .map(|run| run.id.0)
            .collect();
        assert_eq!(every, expected);
        let summarised: Vec<String> = sessions
            .summaries_of(&bot)
            .await
            .expect("reads")
            .into_iter()
            .map(|run| run.id.0)
            .collect();
        assert_eq!(summarised, expected);
        let stored: Vec<String> = sqlx::query_scalar("SELECT started_at FROM session ORDER BY id")
            .fetch_all(store.pool())
            .await
            .expect("the stamps read");
        assert_eq!(
            stored,
            [
                "2026-01-01T00:00:05.5Z",
                "2026-01-01T00:00:05.12Z",
                "2026-01-01T00:00:05.100000000Z",
                "2026-01-01T00:00:05Z"
            ],
            "no stored stamp was rewritten"
        );

        row(&store, "bad-stamp", "the day before yesterday", "active").await;
        for read in [
            sessions.sessions_of(&bot).await.map(|_| ()),
            sessions.all_sessions().await.map(|_| ()),
        ] {
            assert!(
                matches!(read, Err(SessionError::Unreadable { .. })),
                "an unreadable stamp fails the read of whole runs: {read:?}"
            );
        }
        // **The summaries name the run instead**, and no run is sorted first for
        // it: the four that read keep their order and the damaged one is last.
        for runs in [
            sessions.summaries_of(&bot).await.expect("names the run"),
            sessions.all_summaries().await.expect("names the run"),
        ] {
            let order: Vec<&str> = runs.iter().map(|run| run.id.0.as_str()).collect();
            assert_eq!(order, ["half", "hundredth", "tenth", "whole", "bad-stamp"]);
            assert!(
                runs[4].unreadable.is_some() && runs[..4].iter().all(|r| r.unreadable.is_none())
            );
        }

        store.stop().await;
    }

    /// **A `sid`'s shape is checked on write, and filtered on read.**
    ///
    /// `begin` refuses a sid that fails `is_readable_sid`, the same refusal
    /// shape a bad focus already gets — nothing is written. A row that
    /// already carries one — a hand edit, or a card the deployed store
    /// already holds from before this guard existed — must still read: the
    /// earlier card-backed adapter filtered exactly this on load, and this
    /// restores it, on both `read_session` and `summaries_of`. The row
    /// itself, its state and its focus are untouched; only what `sid`
    /// resolves to changes.
    #[tokio::test]
    async fn a_sids_shape_is_checked_on_write_and_filtered_on_read() {
        let scratch = Scratch::new("checked-sid-shape");
        let path = scratch.0.clone();
        let mut store = Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");
        migrate::seed_kinds(store.pool())
            .await
            .expect("the kinds are seeded");
        let sessions = DoltSessions::open(store.pool().clone());

        // Uppercase is outside `SID_ALPHABET`, so this fails the shape check
        // on length-correct input — the fixture must actually be unreadable
        // or neither half below proves anything.
        let bad = "BAD1";
        assert!(
            !jojobot_domain::session::is_readable_sid(bad),
            "the fixture must be unreadable to begin with"
        );

        let refused = sessions
            .begin(NewSession {
                timezone: None,
                started_on: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid(bad.into()),
                focus: "a run".into(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                id: None,
                key_hash: None,
            })
            .await;
        assert!(
            matches!(refused, Err(SessionError::InvalidEntry(_))),
            "begin must refuse a malformed sid, the shape a bad focus already gets: {refused:?}"
        );

        // **Straight into the table, around `begin`.** The read side must
        // never fail on this: the deployed store can already hold a row
        // like it, and a stricter parse that fails the whole read over one
        // bad column is the class of change that has taken production down
        // before.
        let id = SessionId("mmmmmm".into());
        sqlx::query(
            "INSERT INTO session (id, sid, bot, focus, started_at, state)
             VALUES (?, ?, 'bot:gamma', 'what it was doing', ?, ?)",
        )
        .bind(id.as_str())
        .bind(bad)
        .bind("2026-01-01T00:00:00Z")
        .bind(SessionState::Active.as_token())
        .execute(store.pool())
        .await
        .expect("the board takes the row");

        let read = sessions
            .read_session(&id)
            .await
            .expect("a malformed sid must not fail the read");
        assert_eq!(
            read.sid, None,
            "a sid this build did not mint must read as absent, never as the value itself"
        );
        assert_eq!(
            read.state,
            SessionState::Active,
            "the row's own state is untouched"
        );
        assert_eq!(
            read.focus, "what it was doing",
            "the row's own focus is untouched"
        );

        let listed = sessions
            .summaries_of(&EntityId("bot:gamma".into()))
            .await
            .expect("listing must not fail on it either");
        let summary = listed
            .iter()
            .find(|s| s.id == id)
            .expect("the row is still listed");
        assert_eq!(summary.sid, None);

        // **The positive the filter rests on.** Without it, a build that
        // filtered every sid — well-formed ones included — would pass
        // everything above by accident.
        let good = sessions
            .begin(NewSession {
                timezone: None,
                started_on: None,
                bot: EntityId("bot:gamma".into()),
                sid: Sid("gd01".into()),
                focus: "an ordinary run".into(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                id: None,
                key_hash: None,
            })
            .await
            .expect("a well-formed sid is not refused");
        let read_good = sessions
            .read_session(&good.id)
            .await
            .expect("a well-formed sid's row still reads");
        assert_eq!(
            read_good.sid,
            Some(Sid("gd01".into())),
            "a well-formed sid must survive the read, not be filtered along with a bad one"
        );
        let listed = sessions
            .summaries_of(&EntityId("bot:gamma".into()))
            .await
            .expect("listing ok");
        let good_summary = listed
            .iter()
            .find(|s| s.id == good.id)
            .expect("the good row is listed");
        assert_eq!(good_summary.sid, Some(Sid("gd01".into())));

        store.stop().await;
    }

    /// **A stored `served_reset` that is not a whole number is no compaction.**
    /// The row goes straight into the table, as a hand or an older build wrote
    /// it, and every summary the store gives and its run totals agree it is not
    /// one: the same rule the usage weeks read by.
    #[tokio::test]
    async fn an_unreadable_compaction_counts_for_nothing_in_the_dolt_store() {
        use jojobot_domain::session::testing::contract;

        let scratch = Scratch::new("unreadable-compaction");
        let mut store = Dolt::start(&scratch.0, free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");
        migrate::seed_kinds(store.pool())
            .await
            .expect("the kinds are seeded");
        let sessions = DoltSessions::open(store.pool().clone());
        let run = contract::a_run_with_one_compaction(&sessions).await;
        sqlx::query(
            "INSERT INTO session_write (session, ordinal, written_at, changed, value)
             VALUES (?, 1000, ?, 'served_reset', 'not-a-number')",
        )
        .bind(run.id.as_str())
        .bind("2026-09-02T00:00:00Z")
        .execute(store.pool())
        .await
        .expect("the table takes the row");
        contract::an_unreadable_compaction_counts_for_nothing(&sessions, &run).await;
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
                id: None,
                key_hash: None,
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
                id: None,
                key_hash: None,
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
                sid: Sid("sg01".to_string()),
                focus: "about @thing:contract-migrate-mentions-was".to_string(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
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
                sid: Sid("sg02".to_string()),
                focus: "written before resolution existed".to_string(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
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
                jojobot_domain::memory::Override::none(),
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
                jojobot_domain::memory::Override::none(),
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
                sid: Sid("sa01".to_string()),
                focus: "answering things".to_string(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin should succeed");
        let untouched = sessions
            .begin(NewSession {
                bot: EntityId("bot:gamma".to_string()),
                sid: Sid("sa02".to_string()),
                focus: "not yet asked anything".to_string(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                timezone: None,
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin should succeed");

        sessions
            .add_served(&handed.id, 120, jiff::civil::date(2026, 9, 2))
            .await
            .expect("add_served ok");
        sessions
            .add_served(&handed.id, 340, jiff::civil::date(2026, 9, 2))
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
            .add_served(&handed.id, 50, jiff::civil::date(2026, 9, 2))
            .await
            .expect("add_served must not be refused on a closed session");
        let after_close = sessions.read_session(&handed.id).await.expect("read ok");
        assert_eq!(
            after_close.served_chars, 510,
            "accounting keeps running after the session closes: {after_close:?}"
        );

        sessions
            .reset_served(&handed.id)
            .await
            .expect("reset_served must not be refused on a closed session");
        let after_reset = sessions.read_session(&handed.id).await.expect("read ok");
        assert_eq!(
            after_reset.served_chars, 0,
            "the total restarts from nothing: {after_reset:?}"
        );
        sessions
            .reset_served(&handed.id)
            .await
            .expect("a total already at nothing is a run that is there, not a missing one");
        assert!(
            matches!(
                sessions.reset_served(&SessionId("zzzzzz".into())).await,
                Err(SessionError::UnknownSession { .. })
            ),
            "a restart for no run is refused"
        );
        sessions
            .add_served(&handed.id, 7, jiff::civil::date(2026, 9, 2))
            .await
            .expect("add_served ok");
        let regrown = sessions.read_session(&handed.id).await.expect("read ok");
        assert_eq!(regrown.served_chars, 7, "counting resumes from the restart");

        store.stop().await;
    }

    /// 🚨 **`add_served` needs to know the row is there, never its
    /// chronology, and never has to say anything happened that a search
    /// reindex should care about.** A correctness assertion on `served_chars`
    /// alone passes whether this reads one row or the whole run, so this
    /// asserts on the WORK: `entries_read` must not move at all, on a session
    /// carrying beats, and `write_summary`'s count — the signal the search
    /// half of the index rescans everything on — must not move either, since
    /// `served_chars` is not a word anything is ever searched for.
    #[tokio::test]
    async fn add_served_neither_parses_the_chronology_nor_signals_the_search_index() {
        let scratch = Scratch::new("session-served-chars-cost");
        let path = scratch.0.clone();
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
                bot: EntityId("bot:epsilon".into()),
                sid: Sid("sv01".into()),
                focus: "a sitting with real beats".into(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");
        for (text, at) in [
            ("first beat", "2026-01-01T00:05:00Z"),
            ("second beat", "2026-01-01T00:10:00Z"),
            ("third beat", "2026-01-01T00:15:00Z"),
        ] {
            sessions
                .append(
                    &busy.id,
                    NewEntry::manual(text, at.parse().expect("a fixed instant"), None),
                )
                .await
                .expect("append ok");
        }

        let entries_before = sessions.entries_read();
        let writes_before = sessions
            .write_summary()
            .await
            .expect("write_summary ok")
            .map(|(count, _)| count);

        sessions
            .add_served(&busy.id, 120, jiff::civil::date(2026, 9, 2))
            .await
            .expect("add_served ok");

        assert_eq!(
            sessions.entries_read(),
            entries_before,
            "add_served only needs to know the row exists — it must not parse a chronology to \
             find out"
        );
        let writes_after = sessions
            .write_summary()
            .await
            .expect("write_summary ok")
            .map(|(count, _)| count);
        assert_eq!(
            writes_after, writes_before,
            "served_chars is not indexed, so an accounting-only write must not signal the \
             session search index to rescan every run"
        );

        let read_back = sessions.read_session(&busy.id).await.expect("read ok");
        assert_eq!(
            read_back.served_chars, 120,
            "the accounting itself must still run: {read_back:?}"
        );

        store.stop().await;
    }

    /// **A served total is added to a row that is there, in one statement.**
    /// The write adds to the column where the id matches; asking first whether
    /// the row exists is a second statement for the answer the first gives. The
    /// question is asked only when the update matched no row, and an id no run
    /// wears is still refused. Each is paired with the total the write left.
    #[tokio::test]
    async fn add_served_adds_to_the_row_without_asking_first_whether_it_is_there() {
        let scratch = Scratch::new("session-served-chars-one-statement");
        let path = scratch.0.clone();
        let mut store = Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");
        migrate::seed_kinds(store.pool())
            .await
            .expect("the kinds are seeded");
        let sessions = DoltSessions::open(store.pool().clone());
        let run = sessions
            .begin(NewSession {
                timezone: None,
                bot: EntityId("bot:epsilon".into()),
                sid: Sid("sv02".into()),
                focus: "a sitting".into(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");

        sessions
            .add_served(&run.id, 120, jiff::civil::date(2026, 9, 2))
            .await
            .expect("add_served ok");
        sessions
            .add_served(&run.id, 30, jiff::civil::date(2026, 9, 2))
            .await
            .expect("add_served ok");
        assert_eq!(
            sessions
                .read_session(&run.id)
                .await
                .expect("read ok")
                .served_chars,
            150,
            "the totals must add up"
        );
        assert_eq!(
            sessions.existence_checks(),
            0,
            "a total added to a row that is there asked whether it was there first"
        );

        // The store counts the rows an update matched, not the rows it changed,
        // so a total of nothing needs no question either.
        sessions
            .add_served(&run.id, 0, jiff::civil::date(2026, 9, 2))
            .await
            .expect("a total of nothing is fine");
        assert_eq!(
            sessions.existence_checks(),
            0,
            "a total of nothing matches the row, so the row is not asked for"
        );
        let missing = sessions
            .add_served(
                &SessionId("zzzzzz".into()),
                5,
                jiff::civil::date(2026, 9, 2),
            )
            .await;
        assert!(
            matches!(missing, Err(SessionError::UnknownSession { .. })),
            "a total for no run is refused: {missing:?}"
        );
        assert_eq!(
            sessions.existence_checks(),
            1,
            "an update that matched no row asks whether the row is there, to word the refusal"
        );
        assert_eq!(
            sessions
                .read_session(&run.id)
                .await
                .expect("read ok")
                .served_chars,
            150,
            "a refused total moved nothing"
        );

        store.stop().await;
    }

    /// 🚨 **The pair that proves `summaries_of` answers the same numbers
    /// `sessions_of` would, without paying for the text.** The positive
    /// alone (right numbers) would pass on a build that quietly went back to
    /// reading everything; the negative alone (nothing extra fetched) would
    /// pass on a build that answers zero for every count. Both, on the same
    /// two sessions.
    /// **One run's summary and the runs counted per bot read no run in full.** The
    /// reads exist so a caller that wants to know whose a run is, or how many
    /// there are, does not pay for what any run said; a build that answered them
    /// from the whole board would still answer correctly, and only this counter
    /// tells the two apart. Paired with the answers themselves, because a read
    /// that returned nothing reads no run either.
    #[tokio::test]
    async fn one_summary_and_the_per_bot_counts_read_no_run_in_full() {
        let scratch = Scratch::new("session-one-summary");
        let mut store = Dolt::start(&scratch.0.clone(), free_port())
            .await
            .expect("the store comes up");
        migrate::run(store.pool()).await.expect("the schema");
        migrate::seed_kinds(store.pool())
            .await
            .expect("the kinds are seeded");
        let sessions = DoltSessions::open(store.pool().clone());
        let mut ids = Vec::new();
        for (bot, sid) in [
            ("bot:gamma", "bz01"),
            ("bot:gamma", "bz02"),
            ("bot:delta", "bz03"),
        ] {
            let run = sessions
                .begin(NewSession {
                    timezone: None,
                    bot: EntityId(bot.into()),
                    sid: Sid(sid.into()),
                    id: None,
                    key_hash: None,
                    focus: "a run".into(),
                    started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                    started_on: None,
                })
                .await
                .expect("begin ok");
            sessions
                .append(
                    &run.id,
                    NewEntry::manual(
                        "a beat",
                        "2026-01-01T00:05:00Z".parse().expect("a fixed instant"),
                        None,
                    ),
                )
                .await
                .expect("append ok");
            ids.push(run.id);
        }

        let before = sessions.full_reads();
        let one = sessions
            .summary_of(&ids[2])
            .await
            .expect("summary ok")
            .expect("the run exists");
        let counts = sessions.run_counts().await.expect("count ok");
        assert_eq!(
            sessions.full_reads(),
            before,
            "neither read may fall back to reading a run in full"
        );
        assert_eq!(one.bot, EntityId("bot:delta".into()), "{one:?}");
        assert_eq!(one.entry_count, 1, "{one:?}");
        assert_eq!(
            counts.get(&EntityId("bot:gamma".into())),
            Some(&2),
            "{counts:?}"
        );
        assert_eq!(
            counts.get(&EntityId("bot:delta".into())),
            Some(&1),
            "{counts:?}"
        );
        store.stop().await;
    }

    #[tokio::test]
    async fn summaries_of_answers_sessions_ofs_own_numbers_without_the_full_read() {
        let scratch = Scratch::new("session-summaries");
        let path = scratch.0.clone();
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
                started_on: Some("2025-03-30".parse().expect("a day")),
                id: None,
                key_hash: None,
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
                id: None,
                key_hash: None,
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
        assert_eq!(
            busy_summary.started_on,
            Some("2025-03-30".parse().expect("a day")),
            "a run that stated its day carries it on its summary, not the day the clock gave: \
             {busy_summary:?}"
        );
        assert_eq!(
            quiet_summary.started_on, None,
            "a run that stated none carries none: {quiet_summary:?}"
        );
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

    /// **`entries_read` scales with what a run has recorded; `full_reads`
    /// only counts how many times something asked.** Two sessions read the
    /// same number of times (once each) must not move `entries_read` by the
    /// same amount when one has carried three beats and the other none — a
    /// call count alone cannot tell a full read of a long chronology from
    /// one of an empty run, and that is exactly the distinction the
    /// accounting-write defect turns on.
    #[tokio::test]
    async fn entries_read_scales_with_a_runs_own_chronology_not_with_the_call_count() {
        let scratch = Scratch::new("session-entries-read");
        let path = scratch.0.clone();
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
                bot: EntityId("bot:delta".into()),
                sid: Sid("er01".into()),
                focus: "a long-running sitting".into(),
                started_at: "2026-01-01T00:00:00Z".parse().expect("a fixed instant"),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");
        for (text, at) in [
            ("first beat", "2026-01-01T00:05:00Z"),
            ("second beat", "2026-01-01T00:10:00Z"),
            ("third beat", "2026-01-01T00:15:00Z"),
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
                bot: EntityId("bot:delta".into()),
                sid: Sid("er02".into()),
                focus: "a run with nothing journalled yet".into(),
                started_at: "2026-01-02T00:00:00Z".parse().expect("a fixed instant"),
                started_on: None,
                id: None,
                key_hash: None,
            })
            .await
            .expect("begin ok");

        let before = sessions.entries_read();
        sessions.read_session(&quiet.id).await.expect("read ok");
        assert_eq!(
            sessions.entries_read(),
            before,
            "a run with no beats must add none to the entries a full read parsed"
        );

        sessions.read_session(&busy.id).await.expect("read ok");
        assert_eq!(
            sessions.entries_read(),
            before + 3,
            "one full read of a three-beat run must add exactly its three rows, not a flat \
             per-call cost"
        );

        store.stop().await;
    }
}
