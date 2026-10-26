//! **The push outbox rows**: the moment an outbox row is stamped with, the
//! failures it can meet, a row read back as a push, and the account of one
//! attempt kept in the transaction that changed it.

use super::*;

/// **A moment as the outbox writes it**, every fractional digit kept so two
/// stamps read back as the moments they were.
pub(super) fn push_stamp(at: Timestamp) -> String {
    at.strftime("%Y-%m-%dT%H:%M:%S%.9fZ").to_string()
}

pub(super) fn push_store(e: sqlx::Error) -> PushError {
    PushError::Store(e.to_string())
}

pub(super) fn push_moment(text: &str) -> Result<Timestamp, PushError> {
    text.parse().map_err(|e| {
        PushError::Store(format!(
            "a push row holds a moment that is no moment ({text:?}): {e}"
        ))
    })
}

pub(super) fn push_from(row: &sqlx::mysql::MySqlRow) -> Result<Push, PushError> {
    Ok(Push {
        message: MessageId(row.get::<String, _>("message_id")),
        enqueued_at: push_moment(&row.get::<String, _>("enqueued_at"))?,
        attempts: u32::try_from(row.get::<i32, _>("attempts")).unwrap_or(0),
        next_attempt_at: push_moment(&row.get::<String, _>("next_attempt_at"))?,
    })
}

#[async_trait]
impl Outbox for DoltMailboxes {
    async fn due(&self, now: Timestamp) -> Result<Vec<Push>, PushError> {
        // **Filtered here and not in SQL**: the pending rows are the few that
        // are inside their window, and a moment compared as text is only as
        // right as the width it was written at.
        let rows = sqlx::query(
            "SELECT message_id, enqueued_at, attempts, next_attempt_at FROM push_outbox
             WHERE state = 'pending' ORDER BY enqueued_at, message_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(push_store)?;
        let mut due = Vec::new();
        for row in &rows {
            let push = push_from(row)?;
            if push.next_attempt_at <= now {
                due.push(push);
            }
        }
        Ok(due)
    }

    async fn next_due(&self) -> Result<Option<Timestamp>, PushError> {
        let rows = sqlx::query(
            "SELECT message_id, enqueued_at, attempts, next_attempt_at FROM push_outbox
             WHERE state = 'pending'",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(push_store)?;
        let mut earliest: Option<Timestamp> = None;
        for row in &rows {
            let at = push_from(row)?.next_attempt_at;
            earliest = Some(earliest.map_or(at, |e| e.min(at)));
        }
        Ok(earliest)
    }

    async fn sent(&self, message: &MessageId, at: Timestamp) -> Result<(), PushError> {
        owned!(
            self,
            this,
            [message = message.clone()],
            this.sent(&message, at).await
        );
        self.finish(message, "sent", at).await
    }

    async fn failed(
        &self,
        message: &MessageId,
        attempts: u32,
        next_attempt_at: Timestamp,
    ) -> Result<(), PushError> {
        owned!(
            self,
            this,
            [message = message.clone()],
            this.failed(&message, attempts, next_attempt_at).await
        );
        let mut tx = self.pool.begin().await.map_err(push_store)?;
        let changed = sqlx::query(
            "UPDATE push_outbox SET attempts = ?, next_attempt_at = ?
             WHERE message_id = ? AND state = 'pending'",
        )
        .bind(i32::try_from(attempts).unwrap_or(i32::MAX))
        .bind(push_stamp(next_attempt_at))
        .bind(message.as_str())
        .execute(&mut *tx)
        .await
        .map_err(push_store)?
        .rows_affected();
        if changed > 0 {
            append_push_attempt(
                &mut tx,
                message,
                "failed",
                Timestamp::now(),
                Some(next_attempt_at),
            )
            .await?;
        }
        tx.commit().await.map_err(push_store)?;
        Ok(())
    }

    async fn abandoned(&self, message: &MessageId, at: Timestamp) -> Result<(), PushError> {
        owned!(
            self,
            this,
            [message = message.clone()],
            this.abandoned(&message, at).await
        );
        self.finish(message, "abandoned", at).await
    }
}

impl DoltMailboxes {
    /// End a pending row in `state`. A row that is not pending is left as it is,
    /// so a push that was sent is never marked abandoned afterwards.
    async fn finish(
        &self,
        message: &MessageId,
        state: &str,
        at: Timestamp,
    ) -> Result<(), PushError> {
        let mut tx = self.pool.begin().await.map_err(push_store)?;
        let changed = sqlx::query(
            "UPDATE push_outbox SET state = ?, finished_at = ?
             WHERE message_id = ? AND state = 'pending'",
        )
        .bind(state)
        .bind(push_stamp(at))
        .bind(message.as_str())
        .execute(&mut *tx)
        .await
        .map_err(push_store)?
        .rows_affected();
        if changed > 0 {
            append_push_attempt(&mut tx, message, state, at, None).await?;
        }
        tx.commit().await.map_err(push_store)?;
        Ok(())
    }
}

/// **Keep one attempt at a push, in the transaction that changed the row it
/// belongs to.** The outbox row and its account of how it got there land
/// together or not at all, for the reason `message_event` is written that way.
/// The attempt takes the next ordinal the message holds.
pub(super) async fn append_push_attempt(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    message: &MessageId,
    outcome: &str,
    attempted_at: Timestamp,
    next_attempt_at: Option<Timestamp>,
) -> Result<(), PushError> {
    let ordinal = ordinal::next(tx, "push_attempt", &[("message_id", message.as_str())])
        .await
        .map_err(push_store)?;
    sqlx::query(
        "INSERT INTO push_attempt (message_id, ordinal, attempted_at, outcome, next_attempt_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(message.as_str())
    .bind(ordinal)
    .bind(push_stamp(attempted_at))
    .bind(outcome)
    .bind(next_attempt_at.map(push_stamp))
    .execute(&mut **tx)
    .await
    .map_err(push_store)?;
    Ok(())
}
