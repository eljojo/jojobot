//! **The request and app ledgers on the commit path.** A write that carries a
//! request id, and an app write that carries an attempt, append their ledger
//! events to the event log inside the transaction that stores the write, so the
//! write and the record that it happened commit or roll back as one. A read of
//! either ledger is answered from the [`State`] this type holds (decision log
//! 292); the old tables are not read after [`Seam::open`], and not written at all.
//!
//! **The seam belongs to the writer** (see [`Writer::ensure_seam`]): every job of
//! the one writer reaches it through the job's own context, so the 62 commit
//! sites keep their signature. A covered commit that finds no seam fails; it
//! never falls back to the old tables.
//!
//! **No tail read on the write path.** The log's head is the one the log handle
//! holds in memory ([`DoltLog::head_in`]). After a commit whose outcome is not
//! known, the seam is `behind`: the next use replays the log from the position it
//! applied through, which settles the unknown batch before anything is built on
//! the held state.
//!
//! **A batch is a list of members**, so a later family can add its events to the
//! same transaction's batch before it is sealed.

use std::sync::{Arc, Mutex, MutexGuard};

use jiff::Timestamp;
use jojobot_domain::log::{BatchId, ObjectId, Position, RequestId, SealedBatch};
use jojobot_domain::request::events::{LedgerEvent, LegacyRow, State};
use sqlx::{MySql, Row, Transaction};

use crate::dolt_log::{DoltLog, Staged};
use crate::ledger_log::{
    LedgerError, READ_BOUND, REPRESENTATION_VERSION, codec, refused, unreadable,
};
use crate::writer::{Writer, sql};

/// The generation the import gives every retained row. The old tables hold one
/// row per id, so they never held two attempts under one key.
const IMPORTED_GENERATION: u64 = 1;

struct Held {
    state: State,
    /// The position of the last batch applied to `state`.
    applied: Position,
    /// A commit's outcome was not known: the log may hold a batch `state` has not
    /// applied.
    behind: bool,
}

pub(crate) struct Seam {
    log: DoltLog,
    held: Mutex<Held>,
}

/// **A batch staged in the caller's transaction.** Call [`Pending::committed`]
/// after the COMMIT is acknowledged and [`Pending::unknown`] when its outcome is
/// not known. Dropped any other way, the transaction rolled back and nothing
/// changed.
pub(crate) struct Pending {
    seam: Arc<Seam>,
    staged: Staged,
    events: Vec<LedgerEvent>,
    last: Position,
}

impl Pending {
    pub(crate) fn committed(self) {
        self.staged.committed();
        let mut held = self.seam.lock();
        for event in &self.events {
            if let Err(error) = held.state.apply(event) {
                // The events were built from this state, so this does not happen;
                // the log holds them and a restart replays them the same way.
                tracing::error!(%error, "a committed ledger event could not be applied");
            }
        }
        held.applied = self.last;
    }

    pub(crate) fn unknown(self) {
        self.staged.unknown();
        self.seam.lock().behind = true;
    }
}

impl Seam {
    fn lock(&self) -> MutexGuard<'_, Held> {
        self.held.lock().expect("the ledger state is not poisoned")
    }

    /// **Replay the log, and fill an empty ledger once from the old tables.** The
    /// head is read on the writer's connection, which takes the one-writer lock
    /// first.
    pub(crate) async fn open(writer: &Writer, pool: &sql::Pool) -> Result<Arc<Self>, LedgerError> {
        let log = writer.shared_log(pool);
        let mut tx = pool
            .writer()
            .begin()
            .await
            .map_err(|e| LedgerError::Store(e.to_string()))?;
        let settled = log.settle_on(&mut tx).await;
        let _ = tx.rollback().await;
        settled.map_err(refused)?;
        let seam = Arc::new(Self {
            log,
            held: Mutex::new(Held {
                state: State::default(),
                applied: Position::default(),
                behind: false,
            }),
        });
        seam.catch_up().await?;
        if seam.lock().state.history().is_empty() {
            seam.import(pool).await?;
        }
        Ok(seam)
    }

    /// Apply every committed batch after the position applied through.
    async fn catch_up(&self) -> Result<(), LedgerError> {
        loop {
            let from = self.lock().applied;
            let batches = self
                .log
                .read_from(from, READ_BOUND)
                .await
                .map_err(refused)?;
            if batches.is_empty() {
                self.lock().behind = false;
                return Ok(());
            }
            let mut held = self.lock();
            for batch in batches {
                for member in batch.members() {
                    // Another family's member is that family's to read.
                    let Some(event) = codec::decode_own(member).map_err(unreadable)? else {
                        continue;
                    };
                    held.state
                        .apply(&event)
                        .map_err(|error| LedgerError::Store(error.to_string()))?;
                }
                held.applied = batch.range().last;
            }
        }
    }

    /// **The one-time import of the old tables.** Each retained row becomes an
    /// `Imported` event with its source time, all in one batch, when the log holds
    /// no ledger event yet. A log that holds one is never imported into again.
    async fn import(self: &Arc<Self>, pool: &sql::Pool) -> Result<(), LedgerError> {
        let rows = legacy_rows(pool).await?;
        if rows.is_empty() {
            return Ok(());
        }
        let events = rows
            .into_iter()
            .map(|row| row.into_event(IMPORTED_GENERATION))
            .collect();
        self.append_event_batch(pool, events, Timestamp::now())
            .await
    }

    /// Stage `events` as one batch in `tx`, against the head the log holds.
    pub(crate) async fn stage(
        self: &Arc<Self>,
        tx: &mut Transaction<'_, MySql>,
        events: Vec<LedgerEvent>,
        at: Timestamp,
    ) -> Result<Pending, LedgerError> {
        if self.lock().behind {
            self.catch_up().await?;
        }
        let head = self.log.head_in(tx).await.map_err(refused)?;
        let members: Vec<Vec<u8>> = events.iter().map(codec::encode).collect();
        let identity = jojobot_domain::request::digest_of(&format!(
            "{head}:{}",
            members
                .iter()
                .map(|member| String::from_utf8_lossy(member).into_owned())
                .collect::<Vec<_>>()
                .join("\n")
        ));
        let batch = SealedBatch::seal(
            BatchId(format!("ledger:{identity}")),
            Position(head),
            at,
            REPRESENTATION_VERSION,
            members,
        )
        .map_err(refused)?;
        let staged = self
            .log
            .stage(tx, Position(head), &batch)
            .await
            .map_err(refused)?;
        Ok(Pending {
            seam: Arc::clone(self),
            staged,
            events,
            last: batch.range().last,
        })
    }

    /// Append `events` as a batch of their own, in a transaction of their own.
    pub(crate) async fn append_event_batch(
        self: &Arc<Self>,
        pool: &sql::Pool,
        events: Vec<LedgerEvent>,
        at: Timestamp,
    ) -> Result<(), LedgerError> {
        let mut tx = pool
            .writer()
            .begin()
            .await
            .map_err(|e| LedgerError::Store(e.to_string()))?;
        let pending = self.stage(&mut tx, events, at).await?;
        match sql::commit(tx).await {
            Ok(()) => {
                pending.committed();
                Ok(())
            }
            Err(_) => {
                pending.unknown();
                Err(LedgerError::Indeterminate)
            }
        }
    }

    /// Bring the held state up to the log when a commit's outcome was not known.
    pub(crate) async fn settle(&self) -> Result<(), LedgerError> {
        if self.lock().behind {
            self.catch_up().await?;
        }
        Ok(())
    }

    pub(crate) fn behind(&self) -> bool {
        self.lock().behind
    }

    /// Read the held state.
    pub(crate) fn read<T>(&self, read: impl FnOnce(&State) -> T) -> T {
        read(&self.lock().state)
    }
}

/// Every retained row of the three old tables, as the events that import them.
async fn legacy_rows(pool: &sql::Pool) -> Result<Vec<LegacyRow>, LedgerError> {
    let store = |e: sqlx::Error| LedgerError::Store(e.to_string());
    let time = |text: String| {
        text.parse::<Timestamp>()
            .map_err(|e| LedgerError::Store(format!("a ledger row's time is unreadable: {e}")))
    };
    let mut rows = Vec::new();
    for row in sqlx::query(
        "SELECT bot_key, request_id, verb, args_digest, landed, receipt, written_at \
         FROM request_receipt ORDER BY written_at, bot_key, request_id",
    )
    .fetch_all(&**pool)
    .await
    .map_err(store)?
    {
        rows.push(LegacyRow::RequestReceipt {
            bot_key: ObjectId(row.try_get("bot_key").map_err(store)?),
            request_id: RequestId(row.try_get("request_id").map_err(store)?),
            verb: row.try_get("verb").map_err(store)?,
            args_digest: row.try_get("args_digest").map_err(store)?,
            landed: row.try_get("landed").map_err(store)?,
            receipt: row.try_get("receipt").map_err(store)?,
            written_at: time(row.try_get("written_at").map_err(store)?)?,
        });
    }
    for row in sqlx::query(
        "SELECT bot_key, request_id, part, verb, args_digest, landed, note, written_at \
         FROM request_part ORDER BY written_at, bot_key, request_id, part",
    )
    .fetch_all(&**pool)
    .await
    .map_err(store)?
    {
        rows.push(LegacyRow::RequestPart {
            bot_key: ObjectId(row.try_get("bot_key").map_err(store)?),
            request_id: RequestId(row.try_get("request_id").map_err(store)?),
            part: row.try_get("part").map_err(store)?,
            verb: row.try_get("verb").map_err(store)?,
            args_digest: row.try_get("args_digest").map_err(store)?,
            landed: row.try_get("landed").map_err(store)?,
            note: row.try_get("note").map_err(store)?,
            written_at: time(row.try_get("written_at").map_err(store)?)?,
        });
    }
    for row in sqlx::query(
        "SELECT app_id, event_key, digest, landed, written_at FROM app_write \
         ORDER BY written_at, app_id, event_key",
    )
    .fetch_all(&**pool)
    .await
    .map_err(store)?
    {
        rows.push(LegacyRow::AppWrite {
            app_id: row.try_get("app_id").map_err(store)?,
            event_key: row.try_get("event_key").map_err(store)?,
            digest: row.try_get("digest").map_err(store)?,
            landed: row.try_get("landed").map_err(store)?,
            written_at: time(row.try_get("written_at").map_err(store)?)?,
        });
    }
    Ok(rows)
}
