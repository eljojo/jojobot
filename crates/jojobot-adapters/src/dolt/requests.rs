//! The ledger of spent request ids, in the store. See [`jojobot_domain::request`].
//!
//! **A write spends its id in its own transaction.** The commit that stores the
//! write calls [`claim`] before it commits, and the row for the id goes in with
//! the write. A process that stops before the commit stores neither.
//!
//! **A row is keyed by the permanent identity of the bot**, found the way every
//! other lookup finds it: the entity's badge, or its id when it holds none, and a
//! former handle resolves through the badge it belonged to. A rename moves the
//! handle and leaves the key.

use crate::writer::owned;
pub(crate) use crate::writer::sql::commit as commit_native;
use jiff::Timestamp;
use jojobot_domain::clock::Clock;
use jojobot_domain::memory::EntityId;
use jojobot_domain::request::{
    KEPT_FOR, PartRow, Recorded, Request, RequestError, RequestId, Requests, Reservation, Stored,
    take,
};
use sqlx::{MySql, MySqlConnection, MySqlPool, Row, Transaction};

/// An instant as this table stores it: fixed width, so a text comparison is an
/// instant comparison.
fn stamp(at: Timestamp) -> String {
    at.strftime("%Y-%m-%dT%H:%M:%S%.9fZ").to_string()
}

fn store(e: sqlx::Error) -> RequestError {
    tracing::error!(error = %e, "the request store failed");
    crate::writer::sql::store_error(e)
}

/// **A call under an id that another call spent first.** The ledger holds one
/// row for an id, so a call that inserts a second is refused by the store as a
/// key held twice. That is not a rule the caller broke: the id is spent, and the
/// same call sent again finds the row and answers as a repeat. The error says
/// what the store says for a write that collided with another (SQLSTATE `40001`),
/// which every mapper of a store failure already tells apart and answers with a
/// retry.
#[derive(Debug)]
struct SpentByAnother;

impl std::fmt::Display for SpentByAnother {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the request id was spent by another call that committed first")
    }
}

impl std::error::Error for SpentByAnother {}

impl sqlx::error::DatabaseError for SpentByAnother {
    fn message(&self) -> &str {
        "the request id was spent by another call that committed first"
    }

    fn code(&self) -> Option<std::borrow::Cow<'_, str>> {
        Some("40001".into())
    }

    fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
        self
    }

    fn kind(&self) -> sqlx::error::ErrorKind {
        sqlx::error::ErrorKind::Other
    }
}

/// **The error of a write that carried an id, with a key held twice read as the
/// id having been spent.** Any other error is returned as it came.
fn read_as_spent(e: sqlx::Error) -> sqlx::Error {
    match e.as_database_error() {
        Some(db) if db.kind() == sqlx::error::ErrorKind::UniqueViolation => {
            sqlx::Error::Database(Box::new(SpentByAnother))
        }
        _ => e,
    }
}

/// The permanent identity of the bot a handle answers to, or `None` when the
/// handle answers to nothing.
async fn bot_key(
    conn: &mut MySqlConnection,
    handle: &EntityId,
) -> Result<Option<String>, sqlx::Error> {
    if let Some(row) = sqlx::query("SELECT id, badge FROM entity WHERE id = ?")
        .bind(handle.as_str())
        .fetch_optional(&mut *conn)
        .await?
    {
        let badge: Option<String> = row.try_get("badge")?;
        return Ok(Some(badge.unwrap_or_else(|| handle.0.clone())));
    }
    // **Newest first**, as every other read of the table is: a handle two things
    // have worn answers to the one that wore it last.
    sqlx::query_scalar(
        "SELECT badge FROM entity_former_handle WHERE former_handle = ? \
         ORDER BY ordinal DESC LIMIT 1",
    )
    .bind(handle.as_str())
    .fetch_optional(&mut *conn)
    .await
}

/// The ledger over the one table.
#[derive(Clone)]
pub struct DoltRequests {
    writer: crate::writer::Writer,
    pool: crate::writer::sql::Pool,
    /// The clock a lookup measures the keep window against.
    clock: Clock,
}

impl DoltRequests {
    pub fn new(pool: MySqlPool) -> Self {
        Self {
            writer: crate::writer::Writer::default(),
            pool: pool.into(),
            clock: Clock::default(),
        }
    }

    /// **The ledger, told which clock it measures the keep window with.** The
    /// real one unless the server acts out a day, and the same clock the writes
    /// stamp their rows with.
    pub fn on_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// Receipt writes use the same owner as the parts they close.
    pub fn with_writer(mut self, writer: crate::writer::Writer) -> Self {
        self.writer = writer;
        self
    }
}

#[async_trait::async_trait]
impl Requests for DoltRequests {
    async fn lookup(&self, bot: &EntityId, id: &RequestId) -> Result<Option<Stored>, RequestError> {
        let mut conn = self.pool.acquire().await.map_err(store)?;
        let Some(key) = bot_key(&mut conn, bot).await.map_err(store)? else {
            return Ok(None);
        };
        // **A row older than the keep window is not a spent id**, whether or not a
        // later write has removed it yet: the id answers as a repeat exactly while
        // the ledger keeps it.
        let row = sqlx::query(
            "SELECT verb, args_digest, landed, receipt FROM request_receipt \
             WHERE bot_key = ? AND request_id = ? AND written_at >= ?",
        )
        .bind(&key)
        .bind(id.as_str())
        .bind(stamp(self.clock.now() - KEPT_FOR))
        .fetch_optional(&mut *conn)
        .await
        .map_err(store)?;
        row.map(|row| {
            Ok(Stored {
                verb: row.try_get("verb").map_err(store)?,
                digest: row.try_get("args_digest").map_err(store)?,
                landed: row.try_get("landed").map_err(store)?,
                receipt: row.try_get("receipt").map_err(store)?,
            })
        })
        .transpose()
    }

    async fn fill(&self, request: &Request, receipt: &str) -> Result<(), RequestError> {
        owned!(
            self,
            this,
            [request = request.clone(), receipt = receipt.to_string()],
            this.fill(&request, &receipt).await
        );
        let mut conn = self.pool.acquire().await.map_err(store)?;
        let Some(key) = bot_key(&mut conn, &request.bot).await.map_err(store)? else {
            return Ok(());
        };
        sqlx::query("UPDATE request_receipt SET receipt = ? WHERE bot_key = ? AND request_id = ?")
            .bind(receipt)
            .bind(&key)
            .bind(request.id.as_str())
            .execute(&mut *conn)
            .await
            .map_err(store)?;
        Ok(())
    }

    async fn parts(&self, bot: &EntityId, id: &RequestId) -> Result<Vec<PartRow>, RequestError> {
        let mut conn = self.pool.acquire().await.map_err(store)?;
        let Some(key) = bot_key(&mut conn, bot).await.map_err(store)? else {
            return Ok(Vec::new());
        };
        // **Oldest first, in the order the parts were written**, and the keep
        // window applies as it does to the whole write's row.
        let rows = sqlx::query(
            "SELECT part, verb, args_digest, landed, note FROM request_part \
             WHERE bot_key = ? AND request_id = ? AND written_at >= ? \
             ORDER BY written_at, part",
        )
        .bind(&key)
        .bind(id.as_str())
        .bind(stamp(self.clock.now() - KEPT_FOR))
        .fetch_all(&mut *conn)
        .await
        .map_err(store)?;
        rows.into_iter()
            .map(|row| {
                Ok(PartRow {
                    part: row.try_get("part").map_err(store)?,
                    verb: row.try_get("verb").map_err(store)?,
                    digest: row.try_get("args_digest").map_err(store)?,
                    recorded: Recorded {
                        landed: row.try_get("landed").map_err(store)?,
                        note: row.try_get("note").map_err(store)?,
                    },
                })
            })
            .collect()
    }

    async fn finish(&self, request: &Request, receipt: &str) -> Result<(), RequestError> {
        owned!(
            self,
            this,
            [request = request.clone(), receipt = receipt.to_string()],
            this.finish(&request, &receipt).await
        );
        let parts = self.parts(&request.bot, &request.id).await?;
        let mut conn = self.pool.acquire().await.map_err(store)?;
        let Some(key) = bot_key(&mut conn, &request.bot).await.map_err(store)? else {
            return Ok(());
        };
        let names = parts
            .iter()
            .map(|row| format!("{:?}", row.part))
            .collect::<Vec<_>>()
            .join(",");
        // **One row for one id**: a second finish keeps the row and takes the
        // newer receipt.
        sqlx::query(
            "INSERT INTO request_receipt \
             (bot_key, request_id, verb, args_digest, landed, receipt, written_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?) \
             ON DUPLICATE KEY UPDATE receipt = VALUES(receipt)",
        )
        .bind(&key)
        .bind(request.id.as_str())
        .bind(&request.verb)
        .bind(&request.digest)
        .bind(format!("{{\"parts\":[{names}]}}"))
        .bind(receipt)
        .bind(stamp(self.clock.now()))
        .execute(&mut *conn)
        .await
        .map_err(store)?;
        Ok(())
    }
}

/// **The claim a commit holds on the id of the call it is storing.** Empty when
/// the call carries no id.
///
/// Call [`Claim::committed`] after the transaction commits. A claim dropped any
/// other way (the commit failed, or an earlier step returned) leaves the id
/// unspent, because the row went in the same transaction and the transaction did
/// not commit.
pub(crate) struct Claim {
    reservation: Option<Reservation>,
    /// What the store returned for a part, as the row holds it. Empty for the
    /// whole write.
    landed: String,
}

impl Claim {
    pub(crate) fn committed(mut self) {
        if let Some(reservation) = self.reservation.take() {
            match reservation.part() {
                Some(_) => reservation.spent_with(std::mem::take(&mut self.landed)),
                None => reservation.spent(),
            }
        }
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        if let Some(reservation) = self.reservation.take() {
            reservation.failed();
        }
    }
}

/// **Put the id's row into the transaction that stores the write**, when the
/// call carries an id. `landed` says what the write returned, as JSON text, and
/// is only built for a call that carries one.
///
/// Rows older than [`KEPT_FOR`] go in the same transaction.
pub(crate) async fn claim(
    tx: &mut Transaction<'_, MySql>,
    clock: &Clock,
    landed: impl FnOnce() -> String,
) -> Result<Claim, sqlx::Error> {
    let Some(reservation) = take() else {
        return Ok(Claim {
            reservation: None,
            landed: String::new(),
        });
    };
    // **Held by the claim from here on**, so every way out of this function that
    // is not the committed write hands the request back (see `Claim`'s drop).
    let mut claim = Claim {
        reservation: Some(reservation),
        landed: String::new(),
    };
    let request = claim
        .reservation
        .as_ref()
        .expect("the claim was just made with one")
        .request()
        .clone();
    let Some(key) = bot_key(tx, &request.bot).await? else {
        return Err(sqlx::Error::RowNotFound);
    };
    let now = clock.now();
    sqlx::query("DELETE FROM request_receipt WHERE written_at < ?")
        .bind(stamp(now - KEPT_FOR))
        .execute(&mut **tx)
        .await?;
    // **A part of a write that commits in several has a row of its own** in the
    // same transaction, and the whole write's row waits for the verb to finish.
    let part = claim.reservation.as_ref().and_then(|held| {
        held.part()
            .map(|name| (name.to_string(), held.note().map(str::to_string)))
    });
    if let Some((name, note)) = part {
        sqlx::query("DELETE FROM request_part WHERE written_at < ?")
            .bind(stamp(now - KEPT_FOR))
            .execute(&mut **tx)
            .await?;
        claim.landed = landed();
        sqlx::query(
            "INSERT INTO request_part \
             (bot_key, request_id, part, verb, args_digest, landed, note, written_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&key)
        .bind(request.id.as_str())
        .bind(&name)
        .bind(&request.verb)
        .bind(&request.digest)
        .bind(&claim.landed)
        .bind(note)
        .bind(stamp(now))
        .execute(&mut **tx)
        .await
        .map_err(read_as_spent)?;
        return Ok(claim);
    }
    sqlx::query(
        "INSERT INTO request_receipt (bot_key, request_id, verb, args_digest, landed, written_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&key)
    .bind(request.id.as_str())
    .bind(&request.verb)
    .bind(&request.digest)
    .bind(landed())
    .bind(stamp(now))
    .execute(&mut **tx)
    .await
    .map_err(read_as_spent)?;
    Ok(claim)
}

/// **Commit the transaction that stores a write, with the id's row in it** when
/// the call carries an id. This is the whole of what a covered write does at its
/// commit point.
pub(crate) async fn commit(
    mut tx: Transaction<'_, MySql>,
    clock: &Clock,
    landed: impl FnOnce() -> String,
) -> Result<(), sqlx::Error> {
    let claim = claim(&mut tx, clock, landed).await?;
    crate::writer::sql::commit(tx).await?;
    claim.committed();
    Ok(())
}
