//! **The pre-built snapshot of a deploy.** The new binary, started in its
//! snapshot-only mode, reads the live log, replays it with the code boot itself
//! uses, and writes one snapshot at the position it reached. It holds no writer,
//! appends nothing and serves nothing, so it can run beside the old process whose
//! store it reads. The restart that follows loads that snapshot and replays only
//! what was appended after it.
//!
//! A snapshot of a new shape is kept in a slot of its own (see `dolt_log`), so the
//! old process publishing its last snapshot as it stops cannot replace this one.

use jojobot_domain::log::{
    Append, LogError, LogStore, Position, SealedBatch, ValidatedSnapshot, Validity,
};
use sqlx::MySqlPool;
use sqlx::mysql::MySqlPoolOptions;

use crate::dolt_log::DoltLog;
use crate::log_snapshot::take_snapshot;

/// The store's database, as the serving process names it.
const DATABASE: &str = "jojobot";

/// **The log, read through the store's pool and never appended to.** The one write
/// it makes is the snapshot, inside a transaction that rechecks the log's validity
/// as every publication does.
struct ReadOnlyLog {
    log: DoltLog,
    pool: MySqlPool,
}

#[async_trait::async_trait]
impl LogStore for ReadOnlyLog {
    async fn validity(&self) -> Result<Validity, LogError> {
        self.log.validity().await
    }

    async fn append(&self, _: Position, _: &SealedBatch) -> Result<Append, LogError> {
        Err(LogError::Store(
            "the snapshot-only mode appends nothing to the log".into(),
        ))
    }

    async fn read_from(
        &self,
        position: Position,
        bound: usize,
    ) -> Result<Vec<SealedBatch>, LogError> {
        self.log.read_from(position, bound).await
    }

    async fn load_snapshot(&self) -> Result<Option<ValidatedSnapshot>, LogError> {
        self.log.load_snapshot().await
    }

    async fn store_snapshot(&self, snapshot: &ValidatedSnapshot) -> Result<(), LogError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| LogError::Store(e.to_string()))?;
        DoltLog::put_snapshot(&mut tx, snapshot).await?;
        tx.commit()
            .await
            .map_err(|e| LogError::Store(e.to_string()))
    }
}

/// **Write the snapshot of the log in `pool`'s store** and say the position it
/// covers.
pub async fn build(pool: MySqlPool, at: jiff::Timestamp) -> Result<Position, LogError> {
    let log = ReadOnlyLog {
        log: DoltLog::unread(pool.clone()),
        pool,
    };
    take_snapshot(&log, at).await
}

#[cfg(test)]
mod malformed_tests;
#[cfg(test)]
mod tests;

/// **Build the snapshot of the store serving on `port` of the loopback address.**
/// The store is the serving process's; this connects to it and starts none.
pub async fn run(port: u16) -> Result<Position, LogError> {
    let pool = MySqlPoolOptions::new()
        .max_connections(2)
        .connect(&format!("mysql://root@127.0.0.1:{port}/{DATABASE}"))
        .await
        .map_err(|e| LogError::Store(format!("the store on port {port} did not answer: {e}")))?;
    build(pool, jiff::Timestamp::now()).await
}
