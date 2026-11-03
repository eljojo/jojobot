//! Atomic event batches and a connection-lifetime commit fence.
use crate::dolt::teaching::RowChange;
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
    /// **The last position the log holds, kept in memory by its one writer.**
    /// Read from the store once, when first needed and again after a fence;
    /// moved only after a COMMIT is acknowledged. A write never reads the tail.
    head: std::sync::Mutex<Option<u64>>,
    #[cfg(test)]
    tail_reads: std::sync::atomic::AtomicU64,
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

enum Reader {
    Fenced(sqlx::pool::PoolConnection<sqlx::MySql>),
    Fresh(MySqlConnection),
}

impl Reader {
    fn connection(&mut self) -> &mut MySqlConnection {
        match self {
            Self::Fenced(connection) => connection,
            Self::Fresh(connection) => connection,
        }
    }
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
        let log = Self(Arc::new(Inner {
            pool,
            lock,
            session: Mutex::new(Session {
                connection: Some(connection),
                id,
                uncertain: false,
                #[cfg(any(test, feature = "testing"))]
                delayed: false,
            }),
            head: std::sync::Mutex::new(None),
            #[cfg(test)]
            tail_reads: std::sync::atomic::AtomicU64::new(0),
            #[cfg(any(test, feature = "testing"))]
            fault: std::sync::Mutex::new(None),
            #[cfg(any(test, feature = "testing"))]
            acknowledged: tokio::sync::Notify::new(),
        }));
        // **The head is read when the log opens**, so no write after it reads
        // the tail.
        {
            let mut session = log.0.session.lock().await;
            let connection = session.connection.as_mut().ok_or(LogError::Indeterminate)?;
            log.known_head(connection).await?;
        }
        Ok(log)
    }

    /// **Settle the head after a commit whose outcome was not known**, by one
    /// read of the log's tail on a connection the caller has fenced (the
    /// writer's reader, taken after the old connection's terminal boundary).
    pub async fn settle(&self, reader: &MySqlPool) -> Result<(), LogError> {
        self.forget_head();
        let mut connection = reader.acquire().await.map_err(failed)?;
        self.known_head(&mut connection).await?;
        Ok(())
    }

    /// **Settle the head inside a recovery's fence**: one read of the tail on the
    /// reader that fence opened (see [`DoltLog::reader`]).
    pub(crate) async fn settle_in_fence(&self) -> Result<(), LogError> {
        self.forget_head();
        let mut reader = self.reader().await?;
        self.known_head(reader.connection()).await?;
        Ok(())
    }

    /// The slot a batch is stored under: its identity, hashed.
    pub(crate) fn slot_of(batch: &SealedBatch) -> String {
        format!(
            "b:{}",
            jojobot_domain::request::digest_of(&batch.header().batch_id.0)
        )
    }

    /// A stored batch read back, its checksum checked.
    pub(crate) fn decode_stored(bytes: &[u8]) -> Result<SealedBatch, LogError> {
        StoredBatch::decode(bytes)
    }

    /// The head the owner holds. The one read of the log's tail is made here,
    /// when no head is held.
    async fn known_head(&self, connection: &mut MySqlConnection) -> Result<u64, LogError> {
        if let Some(head) = *self.0.head.lock().expect("the head is not poisoned") {
            return Ok(head);
        }
        #[cfg(test)]
        self.0
            .tail_reads
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let last: Option<u64> = sqlx::query_scalar(
            "SELECT MAX(last_position) FROM event_log WHERE first_position IS NOT NULL",
        )
        .fetch_one(connection)
        .await
        .map_err(failed)?;
        let head = last.unwrap_or(0);
        *self.0.head.lock().expect("the head is not poisoned") = Some(head);
        Ok(head)
    }

    /// The head after a COMMIT the store acknowledged.
    fn moved_to(&self, head: u64) {
        *self.0.head.lock().expect("the head is not poisoned") = Some(head);
    }

    /// An outcome that is not known leaves no head to trust: the next write
    /// reads the tail once, after the fence.
    fn forget_head(&self) {
        *self.0.head.lock().expect("the head is not poisoned") = None;
    }

    /// **The connection a read runs on.** Inside a recovery's fence the read uses
    /// the reader that fence opened, and no other: a read made on any other
    /// connection could meet a commit the fence has not yet settled.
    async fn reader(&self) -> Result<Reader, LogError> {
        if let Some(pool) = crate::writer::sql::fenced_reader() {
            return Ok(Reader::Fenced(pool.acquire().await.map_err(failed)?));
        }
        Ok(Reader::Fresh(Self::fresh(&self.0.pool).await?))
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
        let (mut connection, id) = Self::take(&self.0.pool, &self.0.lock)
            .await
            .map_err(|_| LogError::Indeterminate)?;
        // The fence is also where the head is settled: one read of the tail,
        // on the connection that now holds the lock.
        self.forget_head();
        self.known_head(&mut connection)
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

    async fn append_owned(
        &self,
        head: Position,
        batch: SealedBatch,
        rows: Vec<RowChange>,
    ) -> Result<Append, LogError> {
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
        let last = self.known_head(&mut *connection).await?;
        if head.0 != last {
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
        for change in &rows {
            match within(change.apply(&mut *connection)).await {
                Ok(_) => {}
                Err(LogError::Indeterminate) => return Ok(Append::Indeterminate),
                Err(_) => {
                    // The server answered with a refusal, so the transaction is
                    // still open on this connection and rolls back whole.
                    return match sqlx::raw_sql("ROLLBACK").execute(&mut *connection).await {
                        Ok(_) => {
                            session.uncertain = false;
                            Ok(Append::DefinitelyNotCommitted)
                        }
                        Err(_) => Ok(Append::Indeterminate),
                    };
                }
            }
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
        self.moved_to(batch.range().last.0);
        Ok(Append::Committed(batch.range()))
    }

    /// **Put a batch into a transaction the caller owns**, so its rows and the
    /// batch commit or roll back as one. The batch is admitted against the head
    /// the owner holds, with no read of the log. The head moves only when the
    /// caller reports the COMMIT acknowledged through [`Staged::committed`]; a
    /// transaction that rolls back, or a [`Staged`] dropped, leaves it where it
    /// was.
    pub async fn stage(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
        head: Position,
        batch: &SealedBatch,
    ) -> Result<Staged, LogError> {
        if head != batch.header().expected_previous_position {
            return Err(LogError::Conflict);
        }
        if head.0 != self.known_head(tx).await? {
            return Err(LogError::Conflict);
        }
        let slot = format!(
            "b:{}",
            jojobot_domain::request::digest_of(&batch.header().batch_id.0)
        );
        let bytes = StoredBatch::encode(batch)?;
        sqlx::query("INSERT INTO event_log (slot, first_position, last_position, payload) VALUES (?, ?, ?, ?)")
            .bind(&slot)
            .bind(batch.range().first.0)
            .bind(batch.range().last.0)
            .bind(&bytes)
            .execute(&mut **tx)
            .await
            .map_err(failed)?;
        Ok(Staged {
            log: self.clone(),
            last: batch.range().last.0,
        })
    }
}

/// **A batch inside a caller's transaction that has not committed.** Call
/// [`Staged::committed`] after the COMMIT is acknowledged; dropping it any other
/// way leaves the head where it was.
pub struct Staged {
    log: DoltLog,
    last: u64,
}

impl Staged {
    pub fn committed(self) {
        self.log.moved_to(self.last);
    }

    /// The COMMIT's outcome is not known: the head held is not trusted until
    /// [`DoltLog::settle`] reads the tail.
    pub fn unknown(self) {
        self.log.forget_head();
    }
}

impl DoltLog {
    /// **Append a batch and the changes of the legacy rows as one transaction**,
    /// on the same lock-holding connection and under the same fence as
    /// [`LogStore::append`]. A change the server refuses rolls the whole
    /// transaction back and answers `DefinitelyNotCommitted`. The legacy rows of
    /// a family that has not left the old path use this, so the rows and the
    /// events never disagree.
    pub async fn append_with_rows(
        &self,
        head: Position,
        batch: &SealedBatch,
        rows: Vec<RowChange>,
    ) -> Result<Append, LogError> {
        let log = self.clone();
        let batch = batch.clone();
        tokio::spawn(async move { log.append_owned(head, batch, rows).await })
            .await
            .map_err(failed)?
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
        let result = tokio::spawn(async move { log.append_owned(head, batch, Vec::new()).await })
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
        let mut reader = self.reader().await?;
        let rows = sqlx::query("SELECT first_position, last_position, payload FROM event_log WHERE first_position IS NOT NULL AND last_position > ? ORDER BY first_position LIMIT ?")
            .bind(position.0).bind(bound as u64).fetch_all(reader.connection()).await.map_err(failed)?;
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
        let mut reader = self.reader().await?;
        let bytes: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT payload FROM event_log WHERE slot = 'snapshot'")
                .fetch_optional(reader.connection())
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
