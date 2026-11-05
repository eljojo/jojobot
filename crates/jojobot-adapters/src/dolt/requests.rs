//! The ledger of spent request ids. See [`jojobot_domain::request`].
//!
//! **A write spends its id in its own transaction.** The commit that stores the
//! write calls [`claim`] before it commits, and the ledger event for the id goes
//! into the event log in that same transaction. A process that stops before the
//! commit stores neither. A read of the ledger is answered from the state the
//! writer's ledger holds ([`crate::ledger_seam`]); the old tables are not read
//! or written after the writer opened it.
//!
//! **A row is keyed by the permanent identity of the bot**, found the way every
//! other lookup finds it: the entity's badge, or its id when it holds none, and a
//! former handle resolves through the badge it belonged to. A rename moves the
//! handle and leaves the key.

use std::sync::Arc;

use crate::ledger_log::LedgerError;
use crate::ledger_seam::{Pending, Seam};
pub(crate) use crate::writer::sql::commit as commit_native;
use crate::writer::{Writer, owned};
use jojobot_domain::clock::Clock;
use jojobot_domain::log::{ObjectId, RequestId as LogRequestId};
use jojobot_domain::memory::EntityId;
use jojobot_domain::request::events::{self, LedgerEvent, RequestKey};
use jojobot_domain::request::{
    KEPT_FOR, PartRow, Recorded, Request, RequestError, RequestId, Requests, Reservation, Stored,
    take,
};
use sqlx::{MySql, MySqlConnection, MySqlPool, Row, Transaction};

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

/// The ledger of spent ids.
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

fn ledger_failed(e: LedgerError) -> RequestError {
    match e {
        LedgerError::Indeterminate => RequestError::Indeterminate,
        other => {
            tracing::error!(error = %other, "the request ledger failed");
            RequestError::Store(other.to_string())
        }
    }
}

impl DoltRequests {
    /// **The ledger, open and settled.** Opened on the first call as a job of the
    /// writer (a replay of the log, and the import of the old tables into a log
    /// that holds no ledger event), and brought up to the log again when a commit
    /// left its outcome unknown. After that a read touches no store.
    async fn seam(&self) -> Result<Arc<Seam>, RequestError> {
        if let Some(seam) = self.writer.seam()
            && !seam.behind()
        {
            return Ok(seam);
        }
        let (writer, pool) = (self.writer.clone(), self.pool.clone());
        self.writer
            .run(async move {
                let seam = writer.ensure_seam(&pool).await?;
                seam.settle().await?;
                Ok(seam)
            })
            .await
            .map_err(ledger_failed)
    }

    /// The identity a handle answers to, read from the entity tables.
    async fn actor(&self, bot: &EntityId) -> Result<Option<ObjectId>, RequestError> {
        let mut conn = self.pool.acquire().await.map_err(store)?;
        Ok(bot_key(&mut conn, bot).await.map_err(store)?.map(ObjectId))
    }

    /// Append events to the log as a batch of their own, in a job of the writer.
    async fn append(&self, events: Vec<LedgerEvent>) -> Result<(), RequestError> {
        let (writer, pool, now) = (self.writer.clone(), self.pool.clone(), self.clock.now());
        self.writer
            .run(async move {
                let seam = writer.ensure_seam(&pool).await?;
                seam.settle().await?;
                seam.append_event_batch(&pool, events, now).await
            })
            .await
            .map_err(ledger_failed)
    }
}

#[async_trait::async_trait]
impl Requests for DoltRequests {
    async fn lookup(&self, bot: &EntityId, id: &RequestId) -> Result<Option<Stored>, RequestError> {
        let Some(actor) = self.actor(bot).await? else {
            return Ok(None);
        };
        let seam = self.seam().await?;
        let id = LogRequestId(id.as_str().to_string());
        // **A row older than the keep window is not a spent id**, whether or not a
        // later write has removed it yet: the id answers as a repeat exactly while
        // the ledger keeps it.
        let now = self.clock.now();
        Ok(seam.read(|state| {
            state.lookup(&actor, &id, now).map(|commit| Stored {
                verb: commit.request.verb.clone(),
                digest: commit.request.digest.clone(),
                landed: commit.landed.clone(),
                receipt: commit.receipt.clone(),
            })
        }))
    }

    async fn fill(&self, request: &Request, receipt: &str) -> Result<(), RequestError> {
        owned!(
            self,
            this,
            [request = request.clone(), receipt = receipt.to_string()],
            this.fill(&request, &receipt).await
        );
        let Some(actor) = self.actor(&request.bot).await? else {
            return Ok(());
        };
        let seam = self.seam().await?;
        let id = LogRequestId(request.id.as_str().to_string());
        // Filling an id nobody spent changes nothing, and appends nothing.
        let Some(key) = seam.read(|state| {
            state
                .current(&actor, &id)
                .and_then(|held| held.committed.as_ref())
                .map(|commit| commit.request.key.clone())
        }) else {
            return Ok(());
        };
        self.append(vec![LedgerEvent::RequestReceiptRecorded {
            key,
            receipt: receipt.to_string(),
        }])
        .await
    }

    async fn parts(&self, bot: &EntityId, id: &RequestId) -> Result<Vec<PartRow>, RequestError> {
        let Some(actor) = self.actor(bot).await? else {
            return Ok(Vec::new());
        };
        let seam = self.seam().await?;
        let id = LogRequestId(id.as_str().to_string());
        let now = self.clock.now();
        // **Oldest first, in the order the parts were written**, and the keep
        // window applies as it does to the whole write's row.
        Ok(seam.read(|state| {
            state
                .parts(&actor, &id, now)
                .into_iter()
                .map(|part| PartRow {
                    part: part.name.clone(),
                    verb: part.request.verb.clone(),
                    digest: part.request.digest.clone(),
                    recorded: part.recorded.clone(),
                })
                .collect()
        }))
    }

    async fn finish(&self, request: &Request, receipt: &str) -> Result<(), RequestError> {
        owned!(
            self,
            this,
            [request = request.clone(), receipt = receipt.to_string()],
            this.finish(&request, &receipt).await
        );
        let Some(actor) = self.actor(&request.bot).await? else {
            return Ok(());
        };
        let seam = self.seam().await?;
        let id = LogRequestId(request.id.as_str().to_string());
        let now = self.clock.now();
        let (generation, parts, spent) = seam.read(|state| {
            (
                state.generation_for(&actor, &id, now),
                state
                    .parts(&actor, &id, now)
                    .iter()
                    .map(|part| part.name.clone())
                    .collect::<Vec<_>>(),
                state
                    .lookup(&actor, &id, now)
                    .map(|commit| commit.request.key.clone()),
            )
        });
        // **One row for one id**: a second finish keeps the row and takes the
        // newer receipt.
        if let Some(key) = spent {
            return self
                .append(vec![LedgerEvent::RequestReceiptRecorded {
                    key,
                    receipt: receipt.to_string(),
                }])
                .await;
        }
        let names = parts
            .iter()
            .map(|name| format!("{name:?}"))
            .collect::<Vec<_>>()
            .join(",");
        self.append(vec![LedgerEvent::RequestCompleted {
            commit: events::Commit {
                request: events::Request {
                    key: RequestKey {
                        actor,
                        id,
                        generation,
                    },
                    verb: request.verb.clone(),
                    digest: request.digest.clone(),
                },
                landed: format!("{{\"parts\":[{names}]}}"),
                receipt: Some(receipt.to_string()),
                written_at: now,
                expires_at: now + KEPT_FOR,
            },
            required_parts: parts,
        }])
        .await
    }
}

/// **The claim a commit holds on the id of the call it is storing.** Empty when
/// the call carries no id.
///
/// Call [`Claim::committed`] after the transaction commits. A claim dropped any
/// other way (the commit failed, or an earlier step returned) leaves the id
/// unspent, because the ledger event went into the same transaction and the
/// transaction did not commit.
pub(crate) struct Claim {
    reservation: Option<Reservation>,
    /// What the store returned for a part, as the event holds it. Empty for the
    /// whole write.
    landed: String,
    /// The ledger event, staged in the transaction of the write.
    pending: Option<Pending>,
}

impl Claim {
    pub(crate) fn committed(mut self) {
        if let Some(pending) = self.pending.take() {
            pending.committed();
        }
        if let Some(reservation) = self.reservation.take() {
            match reservation.part() {
                Some(_) => reservation.spent_with(std::mem::take(&mut self.landed)),
                None => reservation.spent(),
            }
        }
    }

    /// The COMMIT's outcome is not known. The id stays unspent for this call; the
    /// ledger settles from the log before anything else is built on it.
    fn unknown(mut self) {
        if let Some(pending) = self.pending.take() {
            pending.unknown();
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

fn ledger(e: LedgerError) -> sqlx::Error {
    sqlx::Error::Protocol(e.to_string())
}

/// **Stage the id's ledger event in the transaction that stores the write**, when
/// the call carries an id. `landed` says what the write returned, as JSON text,
/// and is only built for a call that carries one.
///
/// The ledger is the one the writer opened (see
/// [`crate::writer::Writer::ensure_seam`]); a commit that finds none fails and
/// never writes the old tables.
pub(crate) async fn claim(
    tx: &mut Transaction<'_, MySql>,
    clock: &Clock,
    landed: impl FnOnce() -> String,
) -> Result<Claim, sqlx::Error> {
    let Some(reservation) = take() else {
        return Ok(Claim {
            reservation: None,
            landed: String::new(),
            pending: None,
        });
    };
    // **Held by the claim from here on**, so every way out of this function that
    // is not the committed write hands the request back (see `Claim`'s drop).
    let mut claim = Claim {
        reservation: Some(reservation),
        landed: String::new(),
        pending: None,
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
    let seam = Writer::current()
        .and_then(|writer| writer.seam())
        .ok_or_else(|| {
            sqlx::Error::Protocol(
                "the request ledger is not open on this writer; a write that carries a request \
                 id cannot commit without it"
                    .into(),
            )
        })?;
    if seam.behind() {
        seam.settle().await.map_err(ledger)?;
    }
    let now = clock.now();
    let actor = ObjectId(key);
    let id = LogRequestId(request.id.as_str().to_string());
    let part = claim.reservation.as_ref().and_then(|held| {
        held.part()
            .map(|name| (name.to_string(), held.note().map(str::to_string)))
    });
    let (generation, spent) = seam.read(|state| {
        let spent = match &part {
            // **A part has a row of its own**; the whole write's row waits for the
            // verb to finish.
            Some((name, _)) => state
                .parts(&actor, &id, now)
                .iter()
                .any(|held| &held.name == name),
            None => state.lookup(&actor, &id, now).is_some(),
        };
        (state.generation_for(&actor, &id, now), spent)
    });
    if spent {
        return Err(sqlx::Error::Database(Box::new(SpentByAnother)));
    }
    let request = events::Request {
        key: RequestKey {
            actor,
            id,
            generation,
        },
        verb: request.verb,
        digest: request.digest,
    };
    let (written_at, expires_at) = (now, now + KEPT_FOR);
    let event = match part {
        Some((name, note)) => {
            claim.landed = landed();
            LedgerEvent::RequestPartCommitted(events::Part {
                request,
                name,
                recorded: Recorded {
                    landed: claim.landed.clone(),
                    note,
                },
                written_at,
                expires_at,
            })
        }
        None => LedgerEvent::RequestCommitted(events::Commit {
            request,
            landed: landed(),
            receipt: None,
            written_at,
            expires_at,
        }),
    };
    claim.pending = Some(seam.stage(tx, vec![event], now).await.map_err(ledger)?);
    Ok(claim)
}

/// **Commit the transaction that stores a write, with the id's ledger event in
/// it** when the call carries an id. This is the whole of what a covered write
/// does at its commit point.
pub(crate) async fn commit(
    mut tx: Transaction<'_, MySql>,
    clock: &Clock,
    landed: impl FnOnce() -> String,
) -> Result<(), sqlx::Error> {
    let claim = claim(&mut tx, clock, landed).await?;
    match crate::writer::sql::commit(tx).await {
        Ok(()) => {
            claim.committed();
            Ok(())
        }
        Err(e) => {
            claim.unknown();
            Err(e)
        }
    }
}
