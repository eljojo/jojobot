//! Atomic event batches and a connection-lifetime commit fence.
use jojobot_domain::log::*;
use serde::{Deserialize, Serialize};
use sqlx::{Connection, MySqlConnection, MySqlPool, Row};
use std::{sync::Arc, time::Duration};

#[derive(Clone)]
pub struct DoltLog(Arc<Inner>);
struct Inner {
    pool: MySqlPool,
    /// **The last position the log holds, kept in memory by its one writer.**
    /// Read from the store once, when first needed and again after a fence;
    /// moved only after a COMMIT is acknowledged. A write never reads the tail.
    head: std::sync::Mutex<Option<u64>>,
    #[cfg(test)]
    tail_reads: std::sync::atomic::AtomicU64,
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
    /// Open the log's table over a migrated store. The head is read once, here,
    /// so no write after it reads the tail. The log takes no connection and no
    /// lock of its own: its owner, the writer, holds both.
    pub async fn open(pool: MySqlPool) -> Result<Self, LogError> {
        let log = Self::unread(pool);
        log.create_validity().await?;
        let mut connection = Self::fresh(&log.0.pool).await?;
        log.known_head(&mut connection).await?;
        Ok(log)
    }

    /// The first opener gives this log its identity. Reopens preserve it; the
    /// primary key chooses one identity if two initializers arrive together.
    pub(crate) async fn create_validity(&self) -> Result<(), LogError> {
        // This creation is one autocommitted statement, independent of the
        // writer's append transaction and its commit-acknowledgement fence.
        let mut connection = Self::fresh(&self.0.pool).await?;
        let validity = Validity {
            log_identity: jojobot_domain::handle::draw(26),
            representation_epoch: 0,
            erasure_epoch: 0,
            earliest_affected_position: None,
        };
        let bytes = serde_json::to_vec(&validity).map_err(failed)?;
        sqlx::query("INSERT IGNORE INTO event_log (slot, first_position, last_position, payload) VALUES ('validity', NULL, 0, ?)")
            .bind(bytes)
            .execute(&mut connection)
            .await
            .map_err(failed)?;
        Ok(())
    }

    pub(crate) async fn validity(&self) -> Result<Validity, LogError> {
        let mut reader = self.reader().await?;
        Self::read_validity(reader.connection()).await
    }

    async fn read_validity(connection: &mut MySqlConnection) -> Result<Validity, LogError> {
        let bytes: Vec<u8> =
            sqlx::query_scalar("SELECT payload FROM event_log WHERE slot = 'validity' FOR UPDATE")
                .fetch_one(connection)
                .await
                .map_err(failed)?;
        serde_json::from_slice(&bytes).map_err(failed)
    }

    /// The log's table with no head held. The owner reads the tail once, on a
    /// connection that already holds its lock, through [`DoltLog::settle_on`].
    pub(crate) fn unread(pool: MySqlPool) -> Self {
        Self(Arc::new(Inner {
            pool,
            head: std::sync::Mutex::new(None),
            #[cfg(test)]
            tail_reads: std::sync::atomic::AtomicU64::new(0),
        }))
    }

    /// Read the tail once on `connection`, which the caller has put behind the
    /// owner's lock, and hold it as the head.
    pub(crate) async fn settle_on(&self, connection: &mut MySqlConnection) -> Result<(), LogError> {
        self.forget_head();
        self.known_head(connection).await?;
        Ok(())
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
        Ok(self.hold_read(last.unwrap_or(0)))
    }

    /// The head the owner holds, read on `connection` only when none is held. A
    /// caller that stages a batch of its own asks for it here.
    pub(crate) async fn head_in(&self, connection: &mut MySqlConnection) -> Result<u64, LogError> {
        self.known_head(connection).await
    }

    /// **A read of the tail becomes the held head only if it is not behind it.**
    /// The read was made before an `await` returned, and an append can commit and
    /// move the head in that time. Storing the read as it came would set the head
    /// back, and every later append would then be refused as a conflict until a
    /// restart. Returns the head now held.
    fn hold_read(&self, read: u64) -> u64 {
        let mut held = self.0.head.lock().expect("the head is not poisoned");
        let head = held.map_or(read, |now| now.max(read));
        *held = Some(head);
        head
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

impl DoltLog {
    /// **Put one more batch into a transaction that already staged one**, which
    /// ended at `after`. The head held is the head before that first batch, so
    /// this admits the batch against `after` alone, which the caller took from the
    /// batch it staged. Report each [`Staged`] in order after the COMMIT.
    pub async fn stage_after(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
        after: Position,
        batch: &SealedBatch,
    ) -> Result<Staged, LogError> {
        if after != batch.header().expected_previous_position {
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
    /// The batches after `position`, at most `bound`, read on the connection a
    /// read runs on (see [`DoltLog::reader`]).
    pub(crate) async fn read_from(
        &self,
        position: Position,
        bound: usize,
    ) -> Result<Vec<SealedBatch>, LogError> {
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

    pub(crate) async fn load_snapshot(&self) -> Result<Option<ValidatedSnapshot>, LogError> {
        let mut reader = self.reader().await?;
        let bytes: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT payload FROM event_log WHERE slot = 'snapshot'")
                .fetch_optional(reader.connection())
                .await
                .map_err(failed)?;
        bytes.map(|bytes| decode_snapshot(&bytes)).transpose()
    }
    /// **Put the snapshot into a transaction the caller owns.**
    pub(crate) async fn put_snapshot(
        connection: &mut MySqlConnection,
        snapshot: &ValidatedSnapshot,
    ) -> Result<(), LogError> {
        if !Self::read_validity(connection)
            .await?
            .permits_publication(snapshot)
        {
            return Err(LogError::Conflict);
        }
        let bytes = encode_snapshot(snapshot)?;
        sqlx::query("REPLACE INTO event_log (slot, first_position, last_position, payload) VALUES ('snapshot', NULL, ?, ?)")
            .bind(snapshot.covered_position.0)
            .bind(bytes)
            .execute(connection)
            .await
            .map_err(failed)?;
        Ok(())
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

#[cfg(test)]
#[path = "dolt_log_tests.rs"]
pub(crate) mod tests;
