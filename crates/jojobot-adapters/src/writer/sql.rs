//! Native writes use the owner's dedicated connection. A fenced reload uses
//! only the fresh reader opened after the old connection's terminal boundary.

use std::cell::RefCell;
use std::future::Future;
use std::ops::Deref;
use std::pin::Pin;

use futures_core::stream::BoxStream;
use futures_util::{StreamExt, TryStreamExt};
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
}

tokio::task_local! {
    static NATIVE: RefCell<Native>;
    static READING: ();
    static READER: MySqlPool;
}

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub(super) async fn job<F: Future>(writer: &Writer, work: F) -> F::Output {
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

pub(super) fn handled() {
    let _ = NATIVE.try_with(|native| native.borrow_mut().unhandled = false);
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
        E::store_native(error.to_string())
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

pub(super) async fn reloading<F: Future>(pool: MySqlPool, read: F) -> F::Output {
    READER.scope(pool, reading(read)).await
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
