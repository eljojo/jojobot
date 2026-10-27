//! A connection's terminal boundary precedes every recovery read.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use sqlx::mysql::{MySqlConnectOptions, MySqlDatabaseError, MySqlPoolOptions};
use sqlx::{AssertSqlSafe, Connection, MySqlConnection, MySqlPool};

pub(crate) const LOCK: &str = "jojobot-owned-writer";
const WAIT_SECONDS: u32 = 1;
const CONNECT_WITHIN: Duration = Duration::from_secs(5);

pub(super) struct Session {
    options: Arc<MySqlConnectOptions>,
    pub(super) pool: MySqlPool,
    old_id: Arc<AtomicU64>,
}

impl Session {
    pub(super) fn new(source: &MySqlPool) -> Self {
        let options = source.connect_options();
        let old_id = Arc::new(AtomicU64::new(0));
        let recorded = old_id.clone();
        let pool = MySqlPoolOptions::new()
            .max_connections(1)
            .min_connections(0)
            .idle_timeout(None)
            .max_lifetime(None)
            .acquire_timeout(CONNECT_WITHIN)
            .after_connect(move |connection, _| {
                let recorded = recorded.clone();
                Box::pin(async move {
                    let held: Option<i64> = sqlx::query_scalar("SELECT GET_LOCK(?, ?)")
                        .bind(LOCK)
                        .bind(WAIT_SECONDS)
                        .fetch_one(&mut *connection)
                        .await?;
                    if held != Some(1) {
                        return Err(sqlx::Error::PoolTimedOut);
                    }
                    let id: u64 = sqlx::query_scalar("SELECT CONNECTION_ID()")
                        .fetch_one(&mut *connection)
                        .await?;
                    recorded.store(id, Ordering::SeqCst);
                    // No RELEASE_LOCK: the dedicated physical connection owns
                    // the lock through COMMIT, a lost reply and its teardown.
                    Ok(())
                })
            })
            .connect_lazy_with((*options).clone());
        Self {
            options,
            pool,
            old_id,
        }
    }

    pub(super) fn same_store(&self, source: &MySqlPool) -> bool {
        let other = source.connect_options();
        self.options.get_host() == other.get_host()
            && self.options.get_port() == other.get_port()
            && self.options.get_database() == other.get_database()
    }

    pub(super) async fn recover(&self, writer: &super::Writer) -> Result<Read, sqlx::Error> {
        tokio::time::timeout(CONNECT_WITHIN, self.take_after_old(writer))
            .await
            .map_err(|_| sqlx::Error::PoolTimedOut)?
    }

    async fn take_after_old(&self, writer: &super::Writer) -> Result<Read, sqlx::Error> {
        let old = self.old_id.load(Ordering::SeqCst);
        if old == 0 {
            return Err(sqlx::Error::PoolClosed);
        }
        let mut fence = MySqlConnection::connect_with(&self.options).await?;
        let killed = sqlx::query(AssertSqlSafe(format!("KILL CONNECTION {old}")))
            .execute(&mut fence)
            .await;
        if let Err(error) = killed {
            // ER_NO_SUCH_THREAD means it has already closed; the lock, rather
            // than KILL's return, still decides whether a read is safe.
            let gone = error
                .as_database_error()
                .and_then(|db| db.try_downcast_ref::<MySqlDatabaseError>())
                .is_some_and(|db| db.number() == 1094);
            if !gone {
                return Err(error);
            }
        }
        let mut held: Option<i64> = sqlx::query_scalar("SELECT GET_LOCK(?, 0)")
            .bind(LOCK)
            .fetch_one(&mut fence)
            .await?;
        if held != Some(1) {
            #[cfg(test)]
            writer.0.contended.notify_one();
            tracing::info!(
                writer = writer.0.id,
                old_connection = old,
                "recovery fence waits for the old connection"
            );
            held = sqlx::query_scalar("SELECT GET_LOCK(?, ?)")
                .bind(LOCK)
                .bind(WAIT_SECONDS)
                .fetch_one(&mut fence)
                .await?;
        }
        if held != Some(1) {
            return Err(sqlx::Error::PoolTimedOut);
        }
        // This pool opens a NEW reader only after the lock has been acquired.
        // It is never the connection that may still own an old transaction.
        let reader = MySqlPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(CONNECT_WITHIN)
            .connect_with((*self.options).clone())
            .await?;
        Ok(Read { fence, reader })
    }
}

pub(super) struct Read {
    fence: MySqlConnection,
    pub(super) reader: MySqlPool,
}

impl Read {
    pub(super) async fn finish(self) {
        self.reader.close().await;
        let _ = self.fence.close().await;
    }
}
