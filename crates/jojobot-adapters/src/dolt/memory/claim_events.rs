//! **The claims events a claim write adds to its transaction's one batch.**
//!
//! The store keeps writing its rows, and a claim write also hands the events it
//! made to the writer's job (rule 9.3.3-d (a)). The events are built from the
//! row that was just written, with the keys and the writer decoded as the
//! import's reader decodes them, so a claim the State holds is the claim the
//! import would have made of the same row.

use jiff::Timestamp;
use jojobot_domain::log::claims::emit::{self, Origin, Served};
use jojobot_domain::log::claims::import::record_id;
use jojobot_domain::log::claims::{ClaimBody, ClaimEvent, MovedRecord, MovedWrite};
use jojobot_domain::log::{Event, ObjectId};
use jojobot_domain::memory::WriteClass;
use jojobot_domain::memory::merge::Side;
use jojobot_domain::memory::write_origin::{WriteOrigin, written_by};

use jojobot_domain::actor::Actor;

use super::*;

/// **A key write as a merge places it**: the key, the place it holds, the moment
/// the store took it in and the claim that carried it.
#[derive(Clone)]
struct Placed {
    key: String,
    ordinal: i64,
    written_at: Option<Timestamp>,
    fact: String,
}

impl ServedWrite for Placed {
    fn at(&self) -> (&str, i64, Option<Timestamp>, &str) {
        (&self.key, self.ordinal, self.written_at, &self.fact)
    }
    fn place(&mut self, ordinal: i64, fact: Option<&str>) {
        self.ordinal = ordinal;
        if let Some(fact) = fact {
            self.fact = fact.to_string();
        }
    }
}

/// **Who was folded into whom, when, by which bot and in which run.**
pub(super) struct MergeMade<'a> {
    pub(super) folded: &'a EntityId,
    pub(super) survivor: &'a EntityId,
    pub(super) at: &'a str,
    pub(super) caller: &'a EntityId,
    pub(super) session: Option<&'a str>,
}

/// What the key writes of one claim write share.
pub(super) struct Emitting {
    record: jojobot_domain::log::RecordId,
    subject: ObjectId,
    /// The thing the key writes belong to now, when merges have placed them.
    merged: Option<String>,
    origin: Origin,
}

/// Add one event to the batch of the transaction now open. Outside a writer job
/// there is no batch and the call fails, and nothing is kept.
fn emit_one(event: Event<ClaimEvent>) {
    let _ = crate::writer::sql::emit(vec![event]);
}

impl DoltMemory {
    /// **The bot a write names**: the caller, filed under the id the store keeps
    /// for it, when the caller is a bot. A write with no session still says who
    /// wrote it.
    pub(super) async fn writing_bot(
        &self,
        tx: &mut Transaction<'_, MySql>,
        caller: &EntityId,
    ) -> Result<Option<EntityId>, MemoryError> {
        if caller.kind() != Some(jojobot_domain::memory::EntityKind::BOT) {
            return Ok(None);
        }
        Ok(Some(self.stored_writer(tx, caller).await?))
    }

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
        // **A claim a merge moved is served at the survivor under the id the
        // merge gave it**, wherever it is stored.
        let (served_home, label) =
            Self::served_address(tx, fact.home.as_str(), fact.id.as_str()).await?;
        let home = Self::object_key(tx, &served_home).await?;
        let edge = match &fact.edge {
            Some(edge) => Some((
                edge.shape.to_string(),
                Self::object_key(tx, edge.object.as_str()).await?,
            )),
            None => None,
        };
        let derived_from = match &fact.derived_from {
            Some(from) => Some(Self::stored_record(tx, &from.home, from.local.as_str()).await?),
            None => None,
        };
        let mut refs = Vec::with_capacity(fact.refs.len());
        for named in &fact.refs {
            refs.push(Self::object_key(tx, named.as_str()).await?);
        }
        let mut stands_for = Vec::with_capacity(fact.stands_for.len());
        for named in &fact.stands_for {
            stands_for.push(Self::stored_record(tx, &named.home, named.local.as_str()).await?);
        }
        Ok(emit::claim_body(
            fact,
            Served {
                home,
                label,
                edge,
                derived_from,
                refs,
                stands_for,
            },
        ))
    }

    /// **The moment, the run and the writer of one claim write**, read back from
    /// the row that holds them, so the event and the row never disagree.
    async fn claim_write_origin(
        tx: &mut Transaction<'_, MySql>,
        home: &EntityId,
        id: &FactId,
        ordinal: i64,
    ) -> Result<Origin, MemoryError> {
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
        Ok(Origin {
            actor: by.writer,
            run,
            written_at: parse_stamp(maybe("written_at")?, home.as_str()),
        })
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
        let origin = Self::claim_write_origin(tx, &fact.home, &fact.id, ordinal).await?;
        let record = record_id(
            &Self::object_key(tx, fact.home.as_str()).await?,
            fact.id.as_str(),
        );
        emit_one(emit::claim_write(record, ordinal, claim, &origin));
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
        let stored = Self::object_key(tx, entity.as_str()).await?;
        let origin = Self::claim_write_origin(tx, entity, fact, claim_write).await?;
        // **Writes of a merged thing are numbered among the survivor's**, and
        // belong to it.
        let survivor = Self::final_survivor(tx, entity.as_str()).await?;
        let merged = Self::has_merges(tx, &survivor).await?;
        let subject = if survivor == entity.as_str() {
            stored.clone()
        } else {
            Self::object_key(tx, &survivor).await?
        };
        Ok(Emitting {
            record: record_id(&stored, fact.as_str()),
            subject,
            merged: merged.then_some(survivor),
            origin,
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
        // **The place a key write holds is the one the merges give it**, which is
        // the count of the key's writes on the thing it belongs to now: a write is
        // the newest of them.
        let ordinal = match &emitting.merged {
            Some(survivor) => Self::served_count(tx, survivor, key).await?,
            None => ordinal,
        };
        let mut targets = Vec::new();
        if let Some(value) = value {
            for target in jojobot_domain::memory::field_link_targets(value) {
                targets.push(Self::object_key(tx, &target).await?);
            }
        }
        let origin = Origin {
            written_at: parse_stamp(Some(at.to_string()), emitting.subject.0.as_str()),
            ..emitting.origin.clone()
        };
        emit_one(emit::key_write(
            emitting.record.clone(),
            emitting.subject.clone(),
            key,
            ordinal as u64,
            class,
            value,
            targets,
            &origin,
        ));
        Ok(())
    }

    /// The thing a key was folded into, through every merge, or the key itself.
    async fn final_survivor(
        tx: &mut Transaction<'_, MySql>,
        key: &str,
    ) -> Result<String, MemoryError> {
        let mut at = key.to_string();
        for _ in 0..64 {
            let next: Option<String> = sqlx::query_scalar(
                "SELECT survivor FROM merge_event WHERE ordinal = 0 AND folded = ?
                 ORDER BY merge_no DESC LIMIT 1",
            )
            .bind(&at)
            .fetch_optional(&mut **tx)
            .await
            .map_err(store)?;
            match next {
                Some(next) => at = next,
                None => break,
            }
        }
        Ok(at)
    }

    /// Whether any merge folded a thing into this key.
    async fn has_merges(tx: &mut Transaction<'_, MySql>, key: &str) -> Result<bool, MemoryError> {
        let found: Option<i64> = sqlx::query_scalar(
            "SELECT merge_no FROM merge_event WHERE ordinal = 0 AND survivor = ? LIMIT 1",
        )
        .bind(key)
        .fetch_optional(&mut **tx)
        .await
        .map_err(store)?;
        Ok(found.is_some())
    }

    /// **How many writes of a key a thing holds once the merges placed them**,
    /// the folded things' writes interleaved with its own.
    async fn served_count(
        tx: &mut Transaction<'_, MySql>,
        survivor: &str,
        key: &str,
    ) -> Result<i64, MemoryError> {
        let steps = Self::merges_into(tx, &EntityId(survivor.to_string())).await?;
        let mut held = std::collections::HashMap::new();
        for name in std::iter::once(survivor.to_string())
            .chain(steps.iter().map(|step| step.folded.clone()))
        {
            let rows = sqlx::query(
                "SELECT `key`, ordinal, fact_id, written_at FROM field_write
                 WHERE entity = ? ORDER BY ordinal",
            )
            .bind(&name)
            .fetch_all(&mut **tx)
            .await
            .map_err(store)?;
            let mut writes = Vec::with_capacity(rows.len());
            for row in &rows {
                writes.push(Placed {
                    key: row.try_get("key").map_err(store)?,
                    ordinal: row.try_get("ordinal").map_err(store)?,
                    fact: row.try_get("fact_id").map_err(store)?,
                    written_at: parse_stamp(
                        row.try_get::<Option<String>, _>("written_at")
                            .map_err(store)?,
                        &name,
                    ),
                });
            }
            held.insert(name, writes);
        }
        apply_merges(&mut held, &steps);
        Ok(held
            .remove(survivor)
            .unwrap_or_default()
            .iter()
            .filter(|write| write.key == key)
            .count() as i64)
    }

    /// **Where a stored claim is served now**: the thing and the id a merge gave
    /// it, followed through every merge that moved it on.
    async fn served_address(
        tx: &mut Transaction<'_, MySql>,
        home: &str,
        local: &str,
    ) -> Result<(String, String), MemoryError> {
        let mut at = (home.to_string(), local.to_string());
        for _ in 0..64 {
            let next: Option<(String, String)> = sqlx::query_as(
                "SELECT survivor, survivor_id FROM merge_event
                 WHERE folded = ? AND folded_id = ? AND ordinal > 0
                 ORDER BY merge_no DESC LIMIT 1",
            )
            .bind(&at.0)
            .bind(&at.1)
            .fetch_optional(&mut **tx)
            .await
            .map_err(store)?;
            match next {
                Some(next) => at = next,
                None => break,
            }
        }
        Ok(at)
    }

    /// **The record a claim is held in, named by the row it is stored in.** An
    /// address names a claim by the home and id it is served under, and a merge
    /// leaves a claim in the row it was written to, so the address is walked back
    /// along the merge events to the row that holds the claim's writes. Every
    /// pointer an event holds to a claim names it through this one function.
    async fn stored_record(
        tx: &mut Transaction<'_, MySql>,
        home: &EntityId,
        local: &str,
    ) -> Result<jojobot_domain::log::RecordId, MemoryError> {
        let served = FactAddress::new(home.clone(), FactId(local.to_string()));
        let at = Self::written_at_or_before(tx, &served)
            .await?
            .unwrap_or(served);
        Ok(record_id(
            &Self::object_key(tx, at.home.as_str()).await?,
            at.local.as_str(),
        ))
    }

    /// **Add the event of a merge to the open transaction's batch**: who was
    /// folded into whom, which claims it gave new ids, and the place each key
    /// write takes among the survivor's. The places are the domain's one rule
    /// ([`interleave`]) over the writes as they are served before the merge.
    pub(super) async fn emit_merge(
        &self,
        tx: &mut Transaction<'_, MySql>,
        made: MergeMade<'_>,
        address_map: &[(String, String)],
    ) -> Result<(), MemoryError> {
        let MergeMade {
            folded,
            survivor,
            at,
            caller,
            session,
        } = made;
        let claims: std::collections::HashMap<&str, &str> = address_map
            .iter()
            .map(|(was, now)| (was.as_str(), now.as_str()))
            .collect();
        let mut records = Vec::with_capacity(address_map.len());
        for (was, now) in address_map {
            records.push(MovedRecord {
                record: Self::stored_record(tx, folded, was).await?,
                label: now.clone(),
            });
        }
        let theirs = self.raw_writes_on(tx, folded).await?;
        let ours = self.raw_writes_on(tx, survivor).await?;
        let mut writes = Vec::new();
        for placed in emit::placements(theirs, ours, &claims) {
            let home = if placed.side == Side::Folded {
                folded
            } else {
                survivor
            };
            writes.push(MovedWrite {
                record: Self::stored_record(tx, home, &placed.write.fact).await?,
                key: jojobot_domain::log::claims::KeyId(placed.write.key.clone()),
                from_ordinal: placed.write.ordinal as u64,
                ordinal: placed.ordinal as u64,
            });
        }
        // **The bot that made the fold is the event's actor**, filed under the id
        // it is stored by, as every claim write names its bot.
        let origin = Origin {
            actor: Some(Actor::Bot(self.stored_writer(tx, caller).await?)),
            run: session.map(str::to_string),
            written_at: parse_stamp(Some(at.to_string()), folded.as_str()),
        };
        emit_one(emit::merge(
            Self::object_key(tx, folded.as_str()).await?,
            Self::object_key(tx, survivor.as_str()).await?,
            records,
            writes,
            &origin,
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests;
