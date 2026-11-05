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
use jojobot_domain::log::claims::ClaimEvent;
use jojobot_domain::log::{BatchId, Event, ObjectId, Position, RequestId, SealedBatch};
use jojobot_domain::request::events::{LedgerEvent, LegacyRow, State};
use sqlx::{MySql, Row, Transaction};

use crate::dolt_log::{DoltLog, Staged};
use crate::ledger_log::{
    LedgerError, READ_BOUND, REPRESENTATION_VERSION, codec, refused, unreadable,
};
use crate::writer::{Writer, sql};

#[cfg(test)]
mod claims_tests;

#[cfg(test)]
mod kind_set_tests;

#[cfg(test)]
mod two_families_tests;

/// The generation the import gives every retained row. The old tables hold one
/// row per id, so they never held two attempts under one key.
const IMPORTED_GENERATION: u64 = 1;

struct Held {
    state: State,
    /// **The claims family's state, held beside the ledgers'.** One replay of the
    /// log fills both, because the log holds every family's members and a batch
    /// may carry both (one batch per transaction).
    claims: jojobot_domain::log::claims::State,
    /// The position of the last batch applied to `state` and `claims`.
    applied: Position,
    /// A commit's outcome was not known: the log may hold a batch `state` has not
    /// applied.
    behind: bool,
    /// The log holds a ledger event. The state cannot say, because it evicts, and
    /// the import runs only into a log that holds none.
    seen: bool,
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
    /// The claims events of the batch, as the log stores them.
    claims: Vec<Event<ClaimEvent>>,
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
        for claim in &self.claims {
            held.claims.apply(claim);
        }
        // Only a ledger event says the log holds one.
        held.seen |= !self.events.is_empty();
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
                claims: jojobot_domain::log::claims::State::default(),
                applied: Position::default(),
                behind: false,
                seen: false,
            }),
        });
        seam.catch_up().await?;
        if !seam.lock().seen {
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
                    // A member of the ledger family is applied to the ledgers; a
                    // member of the claims family, to the claims; any other
                    // family's member is that family's to read.
                    let Some(event) = codec::decode_own(member).map_err(unreadable)? else {
                        if let Some(claim) = crate::claims_log::codec::decode_own(member)
                            .map_err(|error| LedgerError::Store(error.to_string()))?
                        {
                            held.claims.apply(&claim);
                        }
                        continue;
                    };
                    held.state
                        .apply(&event)
                        .map_err(|error| LedgerError::Store(error.to_string()))?;
                    held.seen = true;
                }
                held.applied = batch.range().last;
            }
        }
    }

    /// **The one-time import of the old tables.** Each readable row becomes an
    /// `Imported` event with its source time, when the log holds no ledger event
    /// yet. A log that holds one is never imported into again.
    ///
    /// **The events go in batches of at most [`READ_BOUND`], all in one
    /// transaction.** The old `app_write` table is never pruned, so the import can
    /// be large, and a batch is one stored row. One transaction keeps the import
    /// whole: a failure leaves the log with no ledger event, so the next write
    /// imports again, and no half of it is ever taken for all of it.
    async fn import(self: &Arc<Self>, pool: &sql::Pool) -> Result<(), LedgerError> {
        let rows = legacy_rows(pool).await?;
        if rows.is_empty() {
            return Ok(());
        }
        let mut chunks: Vec<Vec<LedgerEvent>> = Vec::new();
        for row in rows {
            let event = row.into_event(IMPORTED_GENERATION);
            match chunks.last_mut() {
                Some(chunk) if chunk.len() < READ_BOUND => chunk.push(event),
                _ => chunks.push(vec![event]),
            }
        }
        let at = Timestamp::now();
        let mut tx = pool
            .writer()
            .begin()
            .await
            .map_err(|e| LedgerError::Store(e.to_string()))?;
        let mut pending: Vec<Pending> = Vec::new();
        for chunk in chunks {
            let next = match pending.last() {
                None => self.stage(&mut tx, chunk, at).await?,
                Some(previous) => self.stage_after(&mut tx, previous.last, chunk, at).await?,
            };
            pending.push(next);
        }
        match sql::commit(tx).await {
            Ok(()) => {
                for staged in pending {
                    staged.committed();
                }
                Ok(())
            }
            Err(error) => {
                for staged in pending {
                    staged.unknown();
                }
                Err(sql::store_error::<LedgerError>(error))
            }
        }
    }

    /// Stage `events` as one batch in `tx`, against the head the log holds.
    pub(crate) async fn stage(
        self: &Arc<Self>,
        tx: &mut Transaction<'_, MySql>,
        events: Vec<LedgerEvent>,
        at: Timestamp,
    ) -> Result<Pending, LedgerError> {
        self.stage_with(tx, events, Vec::new(), at).await
    }

    /// **Stage the events of two families as one batch in `tx`**: one batch per
    /// transaction, the ledger members first and the claims members after them.
    /// The claims events are read back through their codec before anything is
    /// staged, so an event the log could not read again is refused here, before
    /// the transaction commits, and never after it.
    pub(crate) async fn stage_with(
        self: &Arc<Self>,
        tx: &mut Transaction<'_, MySql>,
        events: Vec<LedgerEvent>,
        claims: Vec<Event<ClaimEvent>>,
        at: Timestamp,
    ) -> Result<Pending, LedgerError> {
        if self.lock().behind {
            self.catch_up().await?;
        }
        // **A batch the state cannot apply never reaches the log.** Boot replays
        // the log and nothing takes an event back out of it.
        self.lock()
            .state
            .check(&events)
            .map_err(|error| LedgerError::Refused(error.to_string()))?;
        let claims: Vec<Event<ClaimEvent>> = claims
            .iter()
            .map(|event| {
                crate::claims_log::codec::decode(&crate::claims_log::codec::encode(event))
                    .map_err(|error| LedgerError::Refused(error.to_string()))
            })
            .collect::<Result<_, _>>()?;
        let head = self.log.head_in(tx).await.map_err(refused)?;
        let batch = Self::seal(head, &events, &claims, at)?;
        let staged = self
            .log
            .stage(tx, Position(head), &batch)
            .await
            .map_err(refused)?;
        Ok(Pending {
            seam: Arc::clone(self),
            staged,
            events,
            claims,
            last: batch.range().last,
        })
    }

    /// Stage `events` as one more batch in `tx`, after the batch this transaction
    /// staged last, which ended at `after`. The import stages its chunks this way.
    async fn stage_after(
        self: &Arc<Self>,
        tx: &mut Transaction<'_, MySql>,
        after: Position,
        events: Vec<LedgerEvent>,
        at: Timestamp,
    ) -> Result<Pending, LedgerError> {
        let batch = Self::seal(after.0, &events, &[], at)?;
        let staged = self
            .log
            .stage_after(tx, after, &batch)
            .await
            .map_err(refused)?;
        Ok(Pending {
            seam: Arc::clone(self),
            staged,
            events,
            claims: Vec::new(),
            last: batch.range().last,
        })
    }

    fn seal(
        head: u64,
        events: &[LedgerEvent],
        claims: &[Event<ClaimEvent>],
        at: Timestamp,
    ) -> Result<SealedBatch, LedgerError> {
        let members: Vec<Vec<u8>> = events
            .iter()
            .map(codec::encode)
            .chain(claims.iter().map(crate::claims_log::codec::encode))
            .collect();
        let identity = jojobot_domain::request::digest_of(&format!(
            "{head}:{}",
            members
                .iter()
                .map(|member| String::from_utf8_lossy(member).into_owned())
                .collect::<Vec<_>>()
                .join("\n")
        ));
        SealedBatch::seal(
            BatchId(format!("ledger:{identity}")),
            Position(head),
            at,
            REPRESENTATION_VERSION,
            members,
        )
        .map_err(refused)
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
            Err(error) => {
                pending.unknown();
                // **The error goes through the writer's own classification**, which
                // marks the native connection uncertain: the job then closes it at a
                // terminal boundary before any read of the tail, as for every other
                // commit of this writer.
                Err(sql::store_error::<LedgerError>(error))
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

    /// Read the held claims state. The capture job reads the State it prepares
    /// against through this, and it is the next commit; until then only the case
    /// that replays the log reads it.
    #[allow(dead_code)]
    pub(crate) fn read_claims<T>(
        &self,
        read: impl FnOnce(&jojobot_domain::log::claims::State) -> T,
    ) -> T {
        read(&self.lock().claims)
    }
}

/// Every retained row of the three old tables, as the events that import them.
async fn legacy_rows(pool: &sql::Pool) -> Result<Vec<LegacyRow>, LedgerError> {
    let store = |e: sqlx::Error| LedgerError::Store(e.to_string());
    // **A row whose stamp cannot be read is named and left out.** The columns are
    // plain text, and one such row must not stop the import, and with it every
    // write of the writer.
    let time = |table: &str, key: String, text: String| match text.parse::<Timestamp>() {
        Ok(at) => Some(at),
        Err(error) => {
            tracing::warn!(
                table,
                key,
                %error,
                "an old ledger row has an unreadable time and is not imported"
            );
            None
        }
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
        let bot_key: String = row.try_get("bot_key").map_err(store)?;
        let request_id: String = row.try_get("request_id").map_err(store)?;
        let Some(written_at) = time(
            "request_receipt",
            format!("{bot_key}/{request_id}"),
            row.try_get("written_at").map_err(store)?,
        ) else {
            continue;
        };
        rows.push(LegacyRow::RequestReceipt {
            bot_key: ObjectId(bot_key),
            request_id: RequestId(request_id),
            verb: row.try_get("verb").map_err(store)?,
            args_digest: row.try_get("args_digest").map_err(store)?,
            landed: row.try_get("landed").map_err(store)?,
            receipt: row.try_get("receipt").map_err(store)?,
            written_at,
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
        let bot_key: String = row.try_get("bot_key").map_err(store)?;
        let request_id: String = row.try_get("request_id").map_err(store)?;
        let part: String = row.try_get("part").map_err(store)?;
        let Some(written_at) = time(
            "request_part",
            format!("{bot_key}/{request_id}/{part}"),
            row.try_get("written_at").map_err(store)?,
        ) else {
            continue;
        };
        rows.push(LegacyRow::RequestPart {
            bot_key: ObjectId(bot_key),
            request_id: RequestId(request_id),
            part,
            verb: row.try_get("verb").map_err(store)?,
            args_digest: row.try_get("args_digest").map_err(store)?,
            landed: row.try_get("landed").map_err(store)?,
            note: row.try_get("note").map_err(store)?,
            written_at,
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
        let app_id: String = row.try_get("app_id").map_err(store)?;
        let event_key: String = row.try_get("event_key").map_err(store)?;
        let Some(written_at) = time(
            "app_write",
            format!("{app_id}/{event_key}"),
            row.try_get("written_at").map_err(store)?,
        ) else {
            continue;
        };
        rows.push(LegacyRow::AppWrite {
            app_id,
            event_key,
            digest: row.try_get("digest").map_err(store)?,
            landed: row.try_get("landed").map_err(store)?,
            written_at,
        });
    }
    Ok(rows)
}
