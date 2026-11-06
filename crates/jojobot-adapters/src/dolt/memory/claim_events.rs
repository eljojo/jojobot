//! **The claims events a claim write adds to its transaction's one batch.**
//!
//! The store keeps writing its rows, and a claim write also hands the events it
//! made to the writer's job (rule 9.3.3-d (a)). The events are built from the
//! row that was just written, with the keys and the writer decoded as the
//! import's reader decodes them, so a claim the State holds is the claim the
//! import would have made of the same row.

use jiff::Timestamp;
use jojobot_domain::log::claims::import::{EVENT_KIND, MEANING_VERSION, record_id};
use jojobot_domain::log::claims::{ClaimBody, ClaimEvent, FieldValue, KeyId};
use jojobot_domain::log::{Event, ObjectId, RunId};
use jojobot_domain::memory::WriteClass;
use jojobot_domain::memory::write_origin::{WriteOrigin, written_by};

use jojobot_domain::actor::Actor;

use super::*;

/// What the key writes of one claim write share.
pub(super) struct Emitting {
    record: jojobot_domain::log::RecordId,
    subject: ObjectId,
    run: Option<String>,
    actor: Option<Actor>,
}

/// Add one event to the batch of the transaction now open. Outside a writer job
/// there is no batch and the call fails, and nothing is kept.
fn emit_one(
    actor: Option<Actor>,
    run: Option<String>,
    written_at: Option<Timestamp>,
    payload: ClaimEvent,
) {
    let _ = crate::writer::sql::emit(vec![Event {
        event_kind: EVENT_KIND.into(),
        meaning_version: MEANING_VERSION,
        actor,
        run: run.map(RunId),
        session_card: None,
        request: None,
        part: None,
        occurred_at: None,
        written_at,
        imported_source_time: None,
        payload,
    }]);
}

impl DoltMemory {
    /// **The permanent key a stored name stands for**: a badge, an entity's id
    /// (filed under its badge when it has one), or a handle some thing wore and
    /// left. A name that stands for none keeps its stored spelling, as the
    /// import's reader keeps it.
    pub(super) async fn object_key(
        tx: &mut Transaction<'_, MySql>,
        stored: &str,
    ) -> Result<ObjectId, MemoryError> {
        let held: Option<String> = sqlx::query_scalar(
            "SELECT COALESCE(badge, id) FROM entity WHERE badge = ? OR id = ?
             ORDER BY badge = ? DESC LIMIT 1",
        )
        .bind(stored)
        .bind(stored)
        .bind(stored)
        .fetch_optional(&mut **tx)
        .await
        .map_err(store)?;
        if let Some(key) = held {
            return Ok(ObjectId(key));
        }
        let former: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT badge FROM entity_former_handle WHERE former_handle = ?",
        )
        .bind(stored)
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        Ok(ObjectId(match former.as_slice() {
            [only] => only.clone(),
            _ => stored.to_string(),
        }))
    }

    /// **A claim as the State holds it**, from the claim as it was written.
    async fn claim_body_of(
        tx: &mut Transaction<'_, MySql>,
        fact: &Fact,
    ) -> Result<ClaimBody, MemoryError> {
        let home = Self::object_key(tx, fact.home.as_str()).await?;
        let edge = match &fact.edge {
            Some(edge) => Some((
                edge.shape.to_string(),
                Self::object_key(tx, edge.object.as_str()).await?,
            )),
            None => None,
        };
        let derived_from = match &fact.derived_from {
            Some(from) => Some(record_id(
                &Self::object_key(tx, from.home.as_str()).await?,
                from.local.as_str(),
            )),
            None => None,
        };
        let mut refs = Vec::with_capacity(fact.refs.len());
        for named in &fact.refs {
            refs.push(Self::object_key(tx, named.as_str()).await?);
        }
        let mut stands_for = Vec::with_capacity(fact.stands_for.len());
        for named in &fact.stands_for {
            stands_for.push(record_id(
                &Self::object_key(tx, named.home.as_str()).await?,
                named.local.as_str(),
            ));
        }
        Ok(ClaimBody {
            label: fact.id.as_str().to_string(),
            home,
            content: fact.content.clone(),
            details: fact.details.clone(),
            provenance: fact.provenance,
            standing: fact.standing,
            status: fact.status,
            recorded_at: fact.recorded_at,
            happened_at: fact.happened_at,
            happened_through: fact.happened_through,
            stale_after: fact.stale_after,
            edge,
            derived_from,
            refs,
            stands_for,
            receipt: fact.receipt.clone(),
            app: fact.app_stamp.clone(),
            inserted_at: fact.inserted_at,
        })
    }

    /// **The moment, the run and the writer of one claim write**, read back from
    /// the row that holds them, so the event and the row never disagree.
    async fn claim_write_origin(
        tx: &mut Transaction<'_, MySql>,
        home: &EntityId,
        id: &FactId,
        ordinal: i64,
    ) -> Result<(Option<Timestamp>, Option<String>, Option<Actor>), MemoryError> {
        let row = sqlx::query(
            "SELECT w.written_at, w.session, w.bot, w.app_stamp,
                    (SELECT s.bot FROM session s WHERE s.id = w.session) AS run_bot
             FROM fact_write w WHERE w.entity = ? AND w.fact_id = ? AND w.ordinal = ?",
        )
        .bind(home.as_str())
        .bind(id.as_str())
        .bind(ordinal)
        .fetch_one(&mut **tx)
        .await
        .map_err(store)?;
        let maybe = |column: &str| -> Result<Option<String>, MemoryError> {
            row.try_get::<Option<String>, _>(column).map_err(store)
        };
        let app_stamp: Option<jojobot_domain::app::AppStamp> = match maybe("app_stamp")? {
            None => None,
            Some(cell) => Some(
                serde_json::from_str(&cell)
                    .map_err(|_| unreadable("its app mark cannot be read"))?,
            ),
        };
        let run = maybe("session")?;
        let by = written_by(
            Some(WriteOrigin {
                bot: maybe("bot")?.map(EntityId),
                run: run.clone(),
                run_bot: maybe("run_bot")?.map(EntityId),
                app_stamp: app_stamp.clone(),
            }),
            app_stamp.as_ref(),
        );
        Ok((
            parse_stamp(maybe("written_at")?, home.as_str()),
            run,
            by.writer,
        ))
    }

    /// **Add the event of one claim write to the open transaction's batch**: the
    /// first write of a claim creates its record and every later one revises it.
    /// A write outside a writer job has no batch to join and adds nothing, which
    /// is also how a boot row repair stays off the log (rule 9.3.3-d (c)).
    pub(super) async fn emit_claim_write(
        tx: &mut Transaction<'_, MySql>,
        fact: &Fact,
        ordinal: i64,
    ) -> Result<(), MemoryError> {
        let claim = Self::claim_body_of(tx, fact).await?;
        let (written_at, run, actor) =
            Self::claim_write_origin(tx, &fact.home, &fact.id, ordinal).await?;
        let record = record_id(&claim.home, fact.id.as_str());
        let payload = if ordinal == 1 {
            ClaimEvent::RecordCreated { record, claim }
        } else {
            ClaimEvent::RecordRevised { record, claim }
        };
        emit_one(actor, run, written_at, payload);
        Ok(())
    }

    /// **What the key writes of one claim write share**: the claim's record, the
    /// key the thing is filed under, and who wrote, in which run.
    pub(super) async fn emitting(
        tx: &mut Transaction<'_, MySql>,
        entity: &EntityId,
        fact: &FactId,
        claim_write: i64,
    ) -> Result<Emitting, MemoryError> {
        let subject = Self::object_key(tx, entity.as_str()).await?;
        let (_, run, actor) = Self::claim_write_origin(tx, entity, fact, claim_write).await?;
        Ok(Emitting {
            record: record_id(&subject, fact.as_str()),
            subject,
            run,
            actor,
        })
    }

    /// **Add the event of one key write to the open transaction's batch**, with
    /// the things its value points at as the keys they stand for.
    pub(super) async fn emit_key_write(
        tx: &mut Transaction<'_, MySql>,
        emitting: &Emitting,
        at: &str,
        key: &str,
        ordinal: i64,
        class: WriteClass,
        value: &Option<String>,
    ) -> Result<(), MemoryError> {
        let mut targets = Vec::new();
        if let Some(value) = value {
            for target in jojobot_domain::memory::field_link_targets(value) {
                targets.push(Self::object_key(tx, &target).await?);
            }
        }
        emit_one(
            emitting.actor.clone(),
            emitting.run.clone(),
            parse_stamp(Some(at.to_string()), emitting.subject.0.as_str()),
            ClaimEvent::FieldWritten {
                record: emitting.record.clone(),
                subject: emitting.subject.clone(),
                key: KeyId(key.to_string()),
                ordinal: ordinal as u64,
                class,
                write: match value {
                    Some(value) => FieldValue::Set(value.clone()),
                    None => FieldValue::Clear,
                },
                targets,
            },
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests;
