//! Atomic event batches and a connection-lifetime commit fence.
use jojobot_domain::log::*;
use serde::{Deserialize, Serialize};
use sqlx::{AssertSqlSafe, Connection, MySqlConnection, MySqlPool, Row};
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;

#[cfg(any(test, feature = "testing"))]
use jojobot_domain::log::testing::{Fault, FaultLog};

#[derive(Clone)]
pub struct DoltLog(Arc<Inner>);
struct Inner {
    pool: MySqlPool,
    lock: String,
    session: Mutex<Session>,
    #[cfg(any(test, feature = "testing"))]
    fault: std::sync::Mutex<Option<Fault>>,
    #[cfg(any(test, feature = "testing"))]
    acknowledged: tokio::sync::Notify,
}
struct Session {
    connection: Option<MySqlConnection>,
    id: u64,
    uncertain: bool,
    #[cfg(any(test, feature = "testing"))]
    delayed: bool,
}

fn failed(error: impl std::fmt::Display) -> LogError {
    LogError::Store(error.to_string())
}
async fn within<T>(
    work: impl std::future::Future<Output = Result<T, sqlx::Error>>,
) -> Result<T, LogError> {
    tokio::time::timeout(Duration::from_secs(5), work)
        .await
        .map_err(|_| LogError::Indeterminate)?
        .map_err(failed)
}

#[derive(Serialize, Deserialize)]
struct StoredBatch(
    BatchId,
    Position,
    jiff::Timestamp,
    u32,
    [u8; 32],
    Vec<Vec<u8>>,
);
impl StoredBatch {
    fn encode(batch: &SealedBatch) -> Result<Vec<u8>, LogError> {
        let h = batch.header();
        serde_json::to_vec(&Self(
            h.batch_id.clone(),
            h.expected_previous_position,
            h.append_time_utc,
            h.representation_version,
            h.checksum,
            batch.members().to_vec(),
        ))
        .map_err(failed)
    }
    fn decode(bytes: &[u8]) -> Result<SealedBatch, LogError> {
        let Self(id, head, at, version, checksum, members) =
            serde_json::from_slice(bytes).map_err(failed)?;
        let batch = SealedBatch::seal(id, head, at, version, members)?;
        if batch.header().checksum != checksum {
            return Err(failed("event log checksum mismatch"));
        }
        Ok(batch)
    }
}

impl DoltLog {
    pub async fn open(pool: MySqlPool) -> Result<Self, LogError> {
        let lock = jojobot_domain::request::digest_of(&format!(
            "jojobot-event-log:{}",
            pool.connect_options().get_database().unwrap_or_default()
        ));
        let (connection, id) = Self::take(&pool, &lock).await?;
        Ok(Self(Arc::new(Inner {
            pool,
            lock,
            session: Mutex::new(Session {
                connection: Some(connection),
                id,
                uncertain: false,
                #[cfg(any(test, feature = "testing"))]
                delayed: false,
            }),
            #[cfg(any(test, feature = "testing"))]
            fault: std::sync::Mutex::new(None),
            #[cfg(any(test, feature = "testing"))]
            acknowledged: tokio::sync::Notify::new(),
        })))
    }

    async fn fresh(pool: &MySqlPool) -> Result<MySqlConnection, LogError> {
        within(MySqlConnection::connect_with(&pool.connect_options())).await
    }

    async fn take(pool: &MySqlPool, lock: &str) -> Result<(MySqlConnection, u64), LogError> {
        let mut connection = Self::fresh(pool).await?;
        let held: Option<i64> = within(
            sqlx::query_scalar("SELECT GET_LOCK(?, 1)")
                .bind(lock)
                .fetch_one(&mut connection),
        )
        .await?;
        if held != Some(1) {
            return Err(LogError::Indeterminate);
        }
        let id = sqlx::query_scalar("SELECT CONNECTION_ID()")
            .fetch_one(&mut connection)
            .await
            .map_err(failed)?;
        // The physical connection keeps this lock through COMMIT and teardown.
        Ok((connection, id))
    }

    async fn fence(&self, session: &mut Session, kill: bool) -> Result<(), LogError> {
        if kill {
            let mut control = Self::fresh(&self.0.pool).await?;
            let killed = tokio::time::timeout(
                Duration::from_secs(5),
                sqlx::query(AssertSqlSafe(format!("KILL CONNECTION {}", session.id)))
                    .execute(&mut control),
            )
            .await
            .map_err(|_| LogError::Indeterminate)?;
            if let Err(error) = killed {
                let gone = error
                    .as_database_error()
                    .and_then(|e| e.try_downcast_ref::<sqlx::mysql::MySqlDatabaseError>())
                    .is_some_and(|e| e.number() == 1094);
                if !gone {
                    return Err(LogError::Indeterminate);
                }
            }
        }
        // KILL returning is not the fence. A busy lock preserves uncertainty.
        let (connection, id) = Self::take(&self.0.pool, &self.0.lock)
            .await
            .map_err(|_| LogError::Indeterminate)?;
        session.connection = Some(connection);
        session.id = id;
        session.uncertain = false;
        #[cfg(any(test, feature = "testing"))]
        {
            session.delayed = false;
        }
        Ok(())
    }

    async fn ready(&self, session: &mut Session) -> Result<(), LogError> {
        #[cfg(any(test, feature = "testing"))]
        if session.delayed {
            return self.fence(session, false).await;
        }
        if session.uncertain {
            self.fence(session, true).await?;
        }
        Ok(())
    }

    async fn append_owned(&self, head: Position, batch: SealedBatch) -> Result<Append, LogError> {
        let mut session = self.0.session.lock().await;
        if self.ready(&mut session).await.is_err() {
            return Ok(Append::Indeterminate);
        }
        if head != batch.header().expected_previous_position {
            return Err(LogError::Conflict);
        }
        let slot = format!(
            "b:{}",
            jojobot_domain::request::digest_of(&batch.header().batch_id.0)
        );
        // A failed admission read can lose the connection too.
        session.uncertain = true;
        let connection = session.connection.as_mut().ok_or(LogError::Indeterminate)?;
        let stored: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT payload FROM event_log WHERE slot = ?")
                .bind(&slot)
                .fetch_optional(&mut *connection)
                .await
                .map_err(failed)?;
        if let Some(bytes) = stored {
            session.uncertain = false;
            return if StoredBatch::decode(&bytes)? == batch {
                Ok(Append::Committed(batch.range()))
            } else {
                Err(LogError::Conflict)
            };
        }
        let last: Option<u64> = sqlx::query_scalar(
            "SELECT MAX(last_position) FROM event_log WHERE first_position IS NOT NULL",
        )
        .fetch_one(&mut *connection)
        .await
        .map_err(failed)?;
        if head.0 != last.unwrap_or(0) {
            session.uncertain = false;
            return Err(LogError::Conflict);
        }
        let bytes = StoredBatch::encode(&batch)?;
        #[cfg(any(test, feature = "testing"))]
        let fault = self.0.fault.lock().unwrap().take();
        // Once BEGIN is issued, every failure must cross a terminal fence.
        session.uncertain = true;
        let connection = session.connection.as_mut().ok_or(LogError::Indeterminate)?;
        let result = within(async {
            sqlx::raw_sql("BEGIN").execute(&mut *connection).await?;
            sqlx::query("INSERT INTO event_log (slot, first_position, last_position, payload) VALUES (?, ?, ?, ?)")
                .bind(&slot).bind(batch.range().first.0).bind(batch.range().last.0).bind(&bytes)
                .execute(&mut *connection).await?;
            Ok::<_, sqlx::Error>(())
        }).await;
        if result.is_err() {
            return Ok(Append::Indeterminate);
        }
        #[cfg(any(test, feature = "testing"))]
        if matches!(fault, Some(Fault::BetweenMembers)) {
            sqlx::raw_sql("ROLLBACK")
                .execute(&mut *connection)
                .await
                .map_err(failed)?;
            session.uncertain = false;
            return Ok(Append::DefinitelyNotCommitted);
        }
        #[cfg(any(test, feature = "testing"))]
        if matches!(fault, Some(Fault::DelayedCommit)) {
            session.delayed = true;
            return Ok(Append::Indeterminate);
        }
        if within(sqlx::raw_sql("COMMIT").execute(&mut *connection))
            .await
            .is_err()
        {
            return Ok(Append::Indeterminate);
        }
        #[cfg(any(test, feature = "testing"))]
        if matches!(fault, Some(Fault::LostAcknowledgement)) {
            return Ok(Append::Indeterminate);
        }
        session.uncertain = false;
        Ok(Append::Committed(batch.range()))
    }
}

#[async_trait::async_trait]
impl LogStore for DoltLog {
    async fn append(&self, head: Position, batch: &SealedBatch) -> Result<Append, LogError> {
        let log = self.clone();
        let batch = batch.clone();
        #[cfg(any(test, feature = "testing"))]
        let hold_reply = matches!(
            *self.0.fault.lock().unwrap(),
            Some(Fault::CancelledAcknowledgement)
        );
        // The owner outlives a caller that stops awaiting its acknowledgement.
        let result = tokio::spawn(async move { log.append_owned(head, batch).await })
            .await
            .map_err(failed)?;
        #[cfg(any(test, feature = "testing"))]
        if hold_reply && matches!(result, Ok(Append::Committed(_))) {
            self.0.acknowledged.notify_one();
            std::future::pending::<()>().await;
        }
        result
    }
    async fn read_from(
        &self,
        position: Position,
        bound: usize,
    ) -> Result<Vec<SealedBatch>, LogError> {
        let mut session = self.0.session.lock().await;
        self.ready(&mut session).await?;
        let mut reader = Self::fresh(&self.0.pool).await?;
        let rows = sqlx::query("SELECT first_position, last_position, payload FROM event_log WHERE first_position IS NOT NULL AND last_position > ? ORDER BY first_position LIMIT ?")
            .bind(position.0).bind(bound as u64).fetch_all(&mut reader).await.map_err(failed)?;
        rows.into_iter()
            .map(|row| {
                let batch =
                    StoredBatch::decode(&row.try_get::<Vec<u8>, _>("payload").map_err(failed)?)?;
                if batch.range().first.0
                    != row.try_get::<u64, _>("first_position").map_err(failed)?
                    || batch.range().last.0
                        != row.try_get::<u64, _>("last_position").map_err(failed)?
                {
                    return Err(failed("event log range mismatch"));
                }
                Ok(batch)
            })
            .collect()
    }
    async fn load_snapshot(&self) -> Result<Option<ValidatedSnapshot>, LogError> {
        let mut session = self.0.session.lock().await;
        self.ready(&mut session).await?;
        let mut reader = Self::fresh(&self.0.pool).await?;
        let bytes: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT payload FROM event_log WHERE slot = 'snapshot'")
                .fetch_optional(&mut reader)
                .await
                .map_err(failed)?;
        bytes.map(|bytes| decode_snapshot(&bytes)).transpose()
    }
    async fn store_snapshot(&self, snapshot: &ValidatedSnapshot) -> Result<(), LogError> {
        let bytes = encode_snapshot(snapshot)?;
        let position = snapshot.covered_position.0;
        let log = self.clone();
        tokio::spawn(async move {
            let mut session = log.0.session.lock().await;
            log.ready(&mut session).await?;
            session.uncertain = true;
            within(sqlx::query("REPLACE INTO event_log (slot, first_position, last_position, payload) VALUES ('snapshot', NULL, ?, ?)")
                .bind(position).bind(bytes).execute(session.connection.as_mut().ok_or(LogError::Indeterminate)?)
                ).await?;
            session.uncertain = false;
            Ok(())
        }).await.map_err(failed)?
    }
}

fn encode_snapshot(s: &ValidatedSnapshot) -> Result<Vec<u8>, LogError> {
    serde_json::to_vec(&(
        s.snapshot_format_version,
        s.projection_version,
        &s.log_identity,
        s.representation_epoch,
        s.erasure_epoch,
        s.covered_position,
        s.batch_checksum,
        s.created_at,
        s.checksum,
        s.serialized_length,
        &s.blob,
    ))
    .map_err(failed)
}
fn decode_snapshot(bytes: &[u8]) -> Result<ValidatedSnapshot, LogError> {
    let (
        snapshot_format_version,
        projection_version,
        log_identity,
        representation_epoch,
        erasure_epoch,
        covered_position,
        batch_checksum,
        created_at,
        checksum,
        serialized_length,
        blob,
    ) = serde_json::from_slice(bytes).map_err(failed)?;
    Ok(ValidatedSnapshot {
        snapshot_format_version,
        projection_version,
        log_identity,
        representation_epoch,
        erasure_epoch,
        covered_position,
        batch_checksum,
        created_at,
        checksum,
        serialized_length,
        blob,
    })
}

#[cfg(any(test, feature = "testing"))]
#[async_trait::async_trait]
impl FaultLog for DoltLog {
    async fn inject(&self, fault: Fault) {
        *self.0.fault.lock().unwrap() = Some(fault);
    }
    async fn acknowledgement_pending(&self) {
        self.0.acknowledged.notified().await;
    }
    async fn finish_delayed(&self, commit: bool) {
        let mut session = self.0.session.lock().await;
        if !session.delayed {
            return;
        }
        if commit {
            sqlx::raw_sql("COMMIT")
                .execute(session.connection.as_mut().unwrap())
                .await
                .unwrap();
            session.delayed = false;
            session.uncertain = false;
        } else {
            self.fence(&mut session, true).await.unwrap();
        }
    }
    async fn restart(&self) {
        let mut session = self.0.session.lock().await;
        self.fence(&mut session, true).await.unwrap();
        *self.0.fault.lock().unwrap() = None;
    }
}

#[cfg(test)]
#[path = "dolt_log_tests.rs"]
mod tests;
