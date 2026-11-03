//! **The event log, written and read through the one writer.** A batch goes into
//! the store inside a transaction on the writer's native connection, so it
//! commits or rolls back with whatever else that transaction holds, and a commit
//! whose outcome is not known is settled by the writer's own fence: the old
//! connection is killed, the lock is taken, and one read of the log's tail on the
//! reader that fence opened says what the head is. One fence, not two.
//!
//! The log's table, its batch codec and its head are [`DoltLog`]'s; this type
//! owns when they run.

use std::future::Future;

use crate::dolt_log::DoltLog;
use crate::writer::{Writer, recovery::guarded, sql};
use jojobot_domain::log::*;
use sqlx::MySqlPool;

/// The rail the log is uncertain on, in the writer's account of uncertainty.
const RAIL: &str = "event-log";

#[derive(Clone)]
pub struct WriterLog {
    writer: Writer,
    pool: sql::Pool,
    log: DoltLog,
}

impl WriterLog {
    /// Open the log over a migrated store, served by `writer`. A caller with no
    /// writer of its own passes a new one: it then has a private owner.
    pub async fn open(pool: MySqlPool, writer: Writer) -> Result<Self, LogError> {
        let log = DoltLog::open(pool.clone()).await?;
        Ok(Self {
            writer,
            pool: pool.into(),
            log,
        })
    }

    /// Run `work` as one job of the writer, under the log's uncertainty scope. A
    /// job that finds the log uncertain first fences and settles the head; a job
    /// whose commit is not known leaves it uncertain and does the same.
    async fn job<T: Send + 'static>(
        &self,
        work: impl Future<Output = Result<T, LogError>> + Send + 'static,
    ) -> Result<T, LogError> {
        let log = self.log.clone();
        let writer = self.writer.clone();
        self.writer
            .run(async move {
                guarded(&writer, RAIL, work, || {
                    let log = log.clone();
                    async move { log.settle_in_fence().await }
                })
                .await
            })
            .await
    }

    async fn append_in_job(&self, head: Position, batch: &SealedBatch) -> Result<Append, LogError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(sql::store_error::<LogError>)?;
        // **A batch already stored answers as it did.** A repeat of the same
        // batch is committed, not applied twice; the same identity with other
        // bytes is a conflict.
        let stored: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT payload FROM event_log WHERE slot = ?")
                .bind(DoltLog::slot_of(batch))
                .fetch_optional(&mut *tx)
                .await
                .map_err(sql::store_error::<LogError>)?;
        if let Some(bytes) = stored {
            return if DoltLog::decode_stored(&bytes)? == *batch {
                Ok(Append::Committed(batch.range()))
            } else {
                Err(LogError::Conflict)
            };
        }
        let staged = self.log.stage(&mut tx, head, batch).await?;
        match sql::commit(tx).await {
            Ok(()) => {
                staged.committed();
                Ok(Append::Committed(batch.range()))
            }
            Err(error) => {
                staged.unknown();
                Err(sql::store_error::<LogError>(error))
            }
        }
    }
}

#[async_trait::async_trait]
impl LogStore for WriterLog {
    async fn append(&self, head: Position, batch: &SealedBatch) -> Result<Append, LogError> {
        let this = self.clone();
        let batch = batch.clone();
        let appended = self
            .job(async move { this.append_in_job(head, &batch).await })
            .await;
        match appended {
            // The contract answers an unknown commit as a value, not an error.
            Err(LogError::Indeterminate) => Ok(Append::Indeterminate),
            other => other,
        }
    }

    async fn read_from(
        &self,
        position: Position,
        bound: usize,
    ) -> Result<Vec<SealedBatch>, LogError> {
        let log = self.log.clone();
        self.job(async move { log.read_from(position, bound).await })
            .await
    }

    async fn load_snapshot(&self) -> Result<Option<ValidatedSnapshot>, LogError> {
        let log = self.log.clone();
        self.job(async move { log.load_snapshot().await }).await
    }

    async fn store_snapshot(&self, snapshot: &ValidatedSnapshot) -> Result<(), LogError> {
        let log = self.log.clone();
        let snapshot = snapshot.clone();
        self.job(async move { log.store_snapshot(&snapshot).await })
            .await
    }
}
