//! Native writes use the owner's dedicated connection. A fenced reload uses
//! only the fresh reader opened after the old connection's terminal boundary.

use std::cell::RefCell;
use std::future::Future;
use std::ops::Deref;
use std::pin::Pin;

use futures_core::stream::BoxStream;
use futures_util::{StreamExt, TryStreamExt};
use jojobot_domain::log::Event;
use jojobot_domain::log::claims::ClaimEvent;
use sqlx::mysql::{MySqlQueryResult, MySqlRow, MySqlStatement, MySqlTypeInfo};
use sqlx::{Either, Execute, Executor, MySql, MySqlPool, SqlStr, Transaction};

use super::Writer;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Phase {
    #[default]
    InProcess,
    BeforeCommit,
    Committed,
    /// A COMMIT was sent and its outcome is not known.
    Unknown,
    /// A statement is running on the native connection. The server's own error
    /// code says it did not run; a lost connection does not say either way.
    InFlight,
}

#[derive(Default)]
struct Native {
    phase: Phase,
    unhandled: bool,
    /// The claims events the open transaction's writer has added to its batch.
    emitted: Vec<Event<ClaimEvent>>,
}

tokio::task_local! {
    static NATIVE: RefCell<Native>;
    static READING: ();
    static READER: MySqlPool;
    /// The writer whose job this is, so a writer inside it can read the State.
    static JOB_WRITER: Writer;
}

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub(super) async fn job<F: Future>(writer: &Writer, work: F) -> F::Output {
    JOB_WRITER.scope(writer.clone(), job_in(writer, work)).await
}

async fn job_in<F: Future>(writer: &Writer, work: F) -> F::Output {
    NATIVE
        .scope(RefCell::new(Native::default()), async {
            let answer = work.await;
            let unhandled = NATIVE.with(|native| native.borrow().unhandled);
            // Requests and outbox have no projection to reload. This only closes
            // an ambiguous native connection before the next owned write starts.
            if unhandled && let Some(session) = writer.0.sql.get() {
                match session.recover(writer).await {
                    Ok(read) => read.finish().await,
                    Err(error) => tracing::warn!(%error, "the native connection remains uncertain"),
                }
            }
            answer
        })
        .await
}

pub(super) fn reset() {
    let _ = NATIVE.try_with(|native| *native.borrow_mut() = Native::default());
}

pub(super) fn phase() -> Phase {
    NATIVE
        .try_with(|native| native.borrow().phase)
        .unwrap_or_default()
}

fn set_phase(phase: Phase) {
    let _ = NATIVE.try_with(|native| {
        let mut native = native.borrow_mut();
        if phase != Phase::BeforeCommit || native.phase != Phase::Committed {
            native.phase = phase;
        }
    });
}

/// **Say that this job has sent no write.** A job that only reads calls it first: an
/// error it ends with is then a failed read, and the rail is not marked uncertain
/// for it, as it is for a write whose commit may have landed.
///
/// **It says so only of a job that has sent nothing yet.** A phase past the start
/// (a commit sent and not answered, one answered, a statement in flight) is a write
/// the job did send, and no later call turns it back into one that was not.
pub(crate) fn nothing_sent() {
    if phase() == Phase::InProcess {
        set_phase(Phase::BeforeCommit);
    }
}

pub(super) fn handled() {
    let _ = NATIVE.try_with(|native| native.borrow_mut().unhandled = false);
}

/// **Add claims events to the batch of the transaction now open.** The commit
/// seals them with the request's ledger event (see `requests::commit`), so a
/// writer never stages a batch of its own. Outside a writer job there is no
/// batch to join, and the call fails.
pub(crate) fn emit(events: Vec<Event<ClaimEvent>>) -> Result<(), sqlx::Error> {
    NATIVE
        .try_with(|native| native.borrow_mut().emitted.extend(events))
        .map_err(|_| {
            sqlx::Error::Protocol("claims events were emitted outside a writer job".into())
        })
}

/// **Read the claims State the running job's writer holds.** `None` outside a
/// writer job and before the seam is open. A writer decides what to emit from
/// the State, not from the store it is writing.
pub(crate) fn read_claims<T>(
    read: impl FnOnce(&jojobot_domain::log::claims::State) -> T,
) -> Option<T> {
    let seam = JOB_WRITER.try_with(|writer| writer.seam()).ok()??;
    Some(seam.read_claims(read))
}

/// Whether any event is waiting to be sealed into the transaction's batch.
pub(crate) fn has_emitted() -> bool {
    NATIVE
        .try_with(|native| !native.borrow().emitted.is_empty())
        .unwrap_or(false)
}

/// The events emitted since the transaction began, handed over once.
pub(crate) fn take_emitted() -> Vec<Event<ClaimEvent>> {
    NATIVE
        .try_with(|native| std::mem::take(&mut native.borrow_mut().emitted))
        .unwrap_or_default()
}

pub(crate) fn store_error<E: super::recovery::StoreError>(error: sqlx::Error) -> E {
    let answered = error.as_database_error().is_some();
    if phase() == Phase::Unknown || (phase() == Phase::InFlight && !answered) {
        let _ = NATIVE.try_with(|native| native.borrow_mut().unhandled = true);
        E::indeterminate()
    } else {
        if phase() == Phase::InFlight {
            // The server answered with an error code: the statement did not run.
            set_phase(Phase::BeforeCommit);
        }
        E::store_native(why(&error))
    }
}

/// **What an error of the writer's pool says went wrong.** A pool timeout while the
/// session's last wait for the log's lock missed it is the lock held by another
/// writer, and says so; a timeout from a busy pool, and every other error, keeps its
/// own text. Every place that turns a writer-pool error into text asks here.
pub(crate) fn why(error: &sqlx::Error) -> String {
    let held = matches!(error, sqlx::Error::PoolTimedOut)
        && Writer::current().is_some_and(|writer| {
            writer
                .0
                .sql
                .get()
                .is_some_and(super::fence::Session::lock_missed)
        });
    if held {
        super::fence::LOCK_HELD.to_string()
    } else {
        error.to_string()
    }
}

pub(crate) async fn commit(tx: Transaction<'_, MySql>) -> Result<(), sqlx::Error> {
    if READING.try_with(|()| ()).is_ok() {
        return tx.commit().await;
    }
    set_phase(Phase::Unknown);
    let result = tx.commit().await;
    if result.is_ok() {
        set_phase(Phase::Committed);
    }
    result
}

pub(crate) async fn reading<F: Future>(read: F) -> F::Output {
    READING.scope((), read).await
}

pub(crate) async fn reloading<F: Future>(pool: MySqlPool, read: F) -> F::Output {
    READER.scope(pool, reading(read)).await
}

/// **The reader a recovery opened after its fence**, when the read runs inside
/// one. A read made there must use it and no other connection.
pub(crate) fn fenced_reader() -> Option<MySqlPool> {
    READER.try_with(Clone::clone).ok()
}

/// Capture before spawning: loader tasks must use the same fenced reader.
pub(crate) fn inherit_read<F: Future>(read: F) -> impl Future<Output = F::Output> {
    let pool = READER.try_with(Clone::clone).ok();
    let is_reading = READING.try_with(|()| ()).is_ok();
    async move {
        match pool {
            Some(pool) => reloading(pool, read).await,
            None if is_reading => reading(read).await,
            None => read.await,
        }
    }
}

#[cfg(test)]
pub(crate) fn has_fresh_reader() -> bool {
    READER.try_with(|_| ()).is_ok()
}

#[derive(Clone)]
pub(crate) struct Pool(MySqlPool);

impl std::fmt::Debug for Pool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WriterPool").finish_non_exhaustive()
    }
}

impl From<MySqlPool> for Pool {
    fn from(pool: MySqlPool) -> Self {
        Self(pool)
    }
}

impl Deref for Pool {
    type Target = MySqlPool;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Pool {
    fn reader(&self) -> MySqlPool {
        if phase() == Phase::InProcess {
            set_phase(Phase::BeforeCommit);
        }
        READER
            .try_with(Clone::clone)
            .unwrap_or_else(|_| self.0.clone())
    }

    pub(crate) fn writer(&self) -> MySqlPool {
        if self.0.is_closed() {
            return self.0.clone();
        }
        match Writer::current() {
            Some(writer) => {
                let session = writer
                    .0
                    .sql
                    .get_or_init(|| super::fence::Session::new(&self.0));
                assert!(
                    session.same_store(&self.0),
                    "one writer cannot serve two stores"
                );
                session.pool.clone()
            }
            None => self.0.clone(),
        }
    }

    pub(crate) async fn begin(&self) -> Result<Transaction<'static, MySql>, sqlx::Error> {
        if READING.try_with(|()| ()).is_ok() {
            return self.reader().begin().await;
        }
        set_phase(Phase::BeforeCommit);
        // **A transaction starts with nothing emitted**: what an earlier one
        // emitted and did not commit (it rolled back) is not this one's.
        take_emitted();
        // **The ledgers open before the first transaction of a job begins**, never
        // inside one: the writer holds one connection, and the open writes.
        if let Some(writer) = Writer::current() {
            let seam = writer
                .ensure_seam(self)
                .await
                .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
            if seam.behind() {
                seam.settle()
                    .await
                    .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
            }
        }
        self.writer().begin().await
    }

    pub(crate) async fn acquire(&self) -> Result<sqlx::pool::PoolConnection<MySql>, sqlx::Error> {
        if READING.try_with(|()| ()).is_ok() {
            return self.reader().acquire().await;
        }
        set_phase(Phase::BeforeCommit);
        let conn = self.writer().acquire().await?;
        set_phase(Phase::InFlight);
        Ok(conn)
    }
}

impl<'p> Executor<'p> for &Pool {
    type Database = MySql;

    fn execute<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> BoxFuture<'e, Result<MySqlQueryResult, sqlx::Error>>
    where
        E: 'q + Execute<'q, MySql>,
    {
        let pool = self.writer();
        Box::pin(async move {
            set_phase(Phase::BeforeCommit);
            let mut conn = pool.acquire().await?;
            set_phase(Phase::InFlight);
            let result = conn.execute(query).await;
            if result.is_ok() {
                set_phase(Phase::Committed);
            }
            result
        })
    }

    fn execute_many<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> BoxStream<'e, Result<MySqlQueryResult, sqlx::Error>>
    where
        E: 'q + Execute<'q, MySql>,
    {
        set_phase(Phase::InFlight);
        self.writer()
            .fetch_many(query)
            .try_filter_map(|step| async move {
                Ok(match step {
                    Either::Left(rows) => Some(rows),
                    Either::Right(_) => None,
                })
            })
            .boxed()
    }

    fn fetch_many<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> BoxStream<'e, Result<Either<MySqlQueryResult, MySqlRow>, sqlx::Error>>
    where
        E: 'q + Execute<'q, MySql>,
    {
        self.reader().fetch_many(query)
    }

    fn fetch_optional<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> BoxFuture<'e, Result<Option<MySqlRow>, sqlx::Error>>
    where
        E: 'q + Execute<'q, MySql>,
    {
        let pool = self.reader();
        Box::pin(async move { pool.fetch_optional(query).await })
    }

    fn prepare_with<'e>(
        self,
        sql: SqlStr,
        parameters: &'e [MySqlTypeInfo],
    ) -> BoxFuture<'e, Result<MySqlStatement, sqlx::Error>>
    where
        'p: 'e,
    {
        let pool = self.reader();
        Box::pin(async move { pool.prepare_with(sql, parameters).await })
    }

    fn describe<'e>(self, sql: SqlStr) -> BoxFuture<'e, Result<sqlx::Describe<MySql>, sqlx::Error>>
    where
        'p: 'e,
    {
        let pool = self.reader();
        Box::pin(async move { pool.describe(sql).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A job that has sent a write is never told it has sent nothing.** The
    /// phases past the start are a commit whose answer is lost, one that was
    /// answered and a statement still running: each stays as it was when a read's
    /// call says nothing was sent, while a job that has sent nothing yet is told so.
    #[tokio::test]
    async fn nothing_sent_leaves_a_phase_past_the_start_as_it_was() {
        for sent in [Phase::Unknown, Phase::Committed, Phase::InFlight] {
            let after = NATIVE
                .scope(RefCell::new(Native::default()), async move {
                    set_phase(sent);
                    nothing_sent();
                    phase()
                })
                .await;
            assert!(after == sent, "a phase past the start was moved");
        }
        let fresh = NATIVE
            .scope(RefCell::new(Native::default()), async {
                nothing_sent();
                phase()
            })
            .await;
        assert!(
            fresh == Phase::BeforeCommit,
            "a job that has sent nothing was not told so"
        );
    }
}
