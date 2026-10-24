//! **The fold's fill, read from the Dolt store a table at a time.**
//!
//! A [`DoltBootReader`] answers the reads the fill asks of every thing from a
//! few statements that do not follow the number of things: each table is read
//! once, in one transaction, and each thing's answer is worked out from its rows
//! by the same decoders and the same folds the read of a single thing uses.
//!
//! **It answers only for a plain thing**: one that is listed under its own
//! handle and that no merge touched. A thing a merge folded, or one that others
//! were folded into, is left out, and the fill reads it the way it always did,
//! because its rows are read across the keys a merge joined and that is the
//! per-thing read's to do.

use std::collections::{BTreeMap, HashMap, HashSet};

use jojobot_domain::memory::boot_reader::BootReader;
use jojobot_domain::memory::{ClaimWrite, FieldBacking};

use super::*;

/// The boot reader of a [`DoltMemory`].
pub struct DoltBootReader {
    memory: DoltMemory,
}

impl DoltMemory {
    /// **The reader the fold's fill takes its reads from.** It reads the same
    /// store this one does.
    pub fn boot_reader(&self) -> DoltBootReader {
        DoltBootReader {
            memory: self.clone(),
        }
    }
}

/// **What the grouped reads share**, read once in one transaction: which things are
/// plain and the key their rows are filed under, and the lookups a pointer in a
/// row needs. Every lookup answers only for a name no merge touched, and a name it
/// cannot place the way the read of a single thing would is `None`: the thing the
/// row belongs to is then left out.
struct Context {
    /// The plain things among those asked for, with their keys.
    plain: HashMap<EntityId, EntityId>,
    /// Who wears each badge now: the least handle, as `current_handle` takes it.
    by_badge: HashMap<String, EntityId>,
    /// Every handle the entity table lists.
    ids: HashSet<EntityId>,
    /// Every name a merge, or a forwarding row, touches.
    touched: HashSet<String>,
    /// Every handle some thing wore and left.
    former: HashSet<String>,
}

impl Context {
    /// Read the tables the lookups are answered from.
    async fn load(
        tx: &mut Transaction<'_, MySql>,
        things: &[EntityId],
    ) -> Result<Context, MemoryError> {
        let rows = sqlx::query("SELECT id, badge, merged_into FROM entity ORDER BY id")
            .fetch_all(&mut **tx)
            .await
            .map_err(store)?;
        let mut keys: HashMap<EntityId, EntityId> = HashMap::with_capacity(rows.len());
        let mut by_badge: HashMap<String, EntityId> = HashMap::new();
        let mut touched: HashSet<String> = HashSet::new();
        for row in &rows {
            let id = EntityId(row.try_get::<String, _>("id").map_err(store)?);
            let badge = row.try_get::<Option<String>, _>("badge").map_err(store)?;
            let key = badge.clone().map(EntityId).unwrap_or_else(|| id.clone());
            if let Some(badge) = badge {
                by_badge.entry(badge).or_insert_with(|| id.clone());
            }
            // A forwarding row folds this thing, and names where it went.
            if let Some(forwards_to) = row
                .try_get::<Option<String>, _>("merged_into")
                .map_err(store)?
            {
                touched.insert(key.as_str().to_string());
                touched.insert(forwards_to);
            }
            keys.insert(id, key);
        }
        let merged = sqlx::query("SELECT folded, survivor FROM merge_event WHERE ordinal = 0")
            .fetch_all(&mut **tx)
            .await
            .map_err(store)?;
        for row in &merged {
            touched.insert(row.try_get::<String, _>("folded").map_err(store)?);
            touched.insert(row.try_get::<String, _>("survivor").map_err(store)?);
        }
        let left = sqlx::query("SELECT former_handle FROM entity_former_handle")
            .fetch_all(&mut **tx)
            .await
            .map_err(store)?;
        let mut former: HashSet<String> = HashSet::with_capacity(left.len());
        for row in &left {
            former.insert(row.try_get::<String, _>("former_handle").map_err(store)?);
        }
        let ids: HashSet<EntityId> = keys.keys().cloned().collect();
        let plain = things
            .iter()
            .filter_map(|thing| {
                let key = keys.get(thing)?;
                (!touched.contains(key.as_str())).then(|| (thing.clone(), key.clone()))
            })
            .collect();
        Ok(Context {
            plain,
            by_badge,
            ids,
            touched,
            former,
        })
    }

    /// **The handle a stored name answers to now**, as a pointer is served.
    fn handle_of(&self, stored: &EntityId) -> Option<EntityId> {
        if self.touched.contains(stored.as_str()) {
            return None;
        }
        Some(
            self.by_badge
                .get(stored.as_str())
                .cloned()
                .unwrap_or_else(|| stored.clone()),
        )
    }

    /// **The address a pointer to a claim is served at.**
    fn address_of(&self, stored: &FactAddress) -> Option<FactAddress> {
        Some(FactAddress::new(
            self.handle_of(&stored.home)?,
            stored.local.clone(),
        ))
    }

    /// **The handle a recorded writer answers to now**, for a writer the entity
    /// table lists; any other is left to the read of one thing.
    fn writer_of(&self, recorded: &EntityId) -> Option<EntityId> {
        let named = self.handle_of(recorded)?;
        (self.ids.contains(&named) && !self.former.contains(named.as_str())).then_some(named)
    }
}

impl DoltBootReader {
    /// **Every field write of every key, joined to its record and decoded by the
    /// reader a single thing's writes go through**, grouped by the key it is filed
    /// under.
    async fn writes_by_key(
        tx: &mut Transaction<'_, MySql>,
    ) -> Result<HashMap<String, Vec<KeyWrite>>, MemoryError> {
        let rows = sqlx::query(
            "SELECT w.entity, w.`key`, w.ordinal, w.value, w.fact_id, w.write_class, \
                    w.written_at, f.status, f.provenance, f.standing, f.details, f.app_stamp
             FROM field_write w
             JOIN fact f ON f.entity = w.entity AND f.id = w.fact_id",
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        let mut by_key: HashMap<String, Vec<KeyWrite>> = HashMap::new();
        for row in &rows {
            let entity: String = row.try_get("entity").map_err(store)?;
            if let Some(write) = DoltMemory::key_write_of(row, &entity)? {
                by_key.entry(entity).or_default().push(write);
            }
        }
        Ok(by_key)
    }

    /// **One claim write, served as the read of a single thing serves it**, or
    /// `None` when a name in it is one the grouped reads cannot place.
    fn claim_write_of(
        context: &Context,
        key: &EntityId,
        row: &sqlx::mysql::MySqlRow,
    ) -> Result<Option<(FactId, ClaimWrite)>, MemoryError> {
        let id = FactId(row.try_get::<String, _>("id").map_err(store)?);
        let ordinal: i64 = row.try_get("ordinal").map_err(store)?;
        let written_at = row
            .try_get::<Option<String>, _>("written_at")
            .map_err(store)?
            .and_then(|at| at.parse::<jiff::Timestamp>().ok());
        let mut fact = fact_from(
            row,
            key.clone(),
            id.clone(),
            Default::default(),
            Vec::new(),
            Vec::new(),
        )?;
        if let Some(source) = &fact.derived_from {
            let Some(at) = context.address_of(source) else {
                return Ok(None);
            };
            fact.derived_from = Some(at);
        }
        if let Some(edge) = &fact.edge {
            let Some(object) = context.handle_of(&edge.object) else {
                return Ok(None);
            };
            fact.edge = Some(Edge {
                shape: edge.shape,
                object,
            });
        }
        let mut write = ClaimWrite::of(&fact, ordinal.max(0) as usize, written_at);
        write.session = row.try_get::<Option<String>, _>("session").map_err(store)?;
        write.bot = match row.try_get::<Option<String>, _>("bot").map_err(store)? {
            Some(recorded) => {
                let Some(handle) = context.writer_of(&EntityId(recorded)) else {
                    return Ok(None);
                };
                Some(handle)
            }
            None => None,
        };
        let damaged = |what: &str| {
            unreadable(&format!(
                "its kept {what} cannot be read as the list this store wrote"
            ))
        };
        write.refs = match row
            .try_get::<Option<String>, _>("kept_refs")
            .map_err(store)?
        {
            None => None,
            Some(cell) => {
                let keys: Vec<String> = serde_json::from_str(&cell).map_err(|_| damaged("refs"))?;
                let mut refs = Vec::with_capacity(keys.len());
                for key in keys {
                    let Some(handle) = context.handle_of(&EntityId(key)) else {
                        return Ok(None);
                    };
                    refs.push(handle);
                }
                Some(refs)
            }
        };
        write.stands_for = match row
            .try_get::<Option<String>, _>("kept_stands_for")
            .map_err(store)?
        {
            None => None,
            Some(cell) => {
                let pairs: Vec<[String; 2]> =
                    serde_json::from_str(&cell).map_err(|_| damaged("mark"))?;
                let mut marked = Vec::with_capacity(pairs.len());
                for [home, local] in pairs {
                    let at = FactAddress::new(EntityId(home), FactId(local));
                    let Some(served) = context.address_of(&at) else {
                        return Ok(None);
                    };
                    marked.push(served);
                }
                Some(marked)
            }
        };
        Ok(Some((id, write)))
    }
}

#[async_trait]
impl BootReader for DoltBootReader {
    async fn backing_many(
        &self,
        things: &[EntityId],
    ) -> Result<HashMap<EntityId, BTreeMap<String, FieldBacking>>, MemoryError> {
        let mut tx = self.memory.pool.begin().await.map_err(store)?;
        let context = Context::load(&mut tx, things).await?;
        let mut by_key = Self::writes_by_key(&mut tx).await?;
        let declared = DoltMemory::types_in(&mut tx).await?;
        tx.commit().await.map_err(store)?;
        Ok(context
            .plain
            .into_iter()
            .map(|(thing, key)| {
                let writes = by_key.get_mut(key.as_str()).map(std::mem::take);
                let backing =
                    jojobot_domain::memory::folded_backing(&writes.unwrap_or_default(), &declared);
                (thing, backing)
            })
            .collect())
    }

    async fn claim_histories_many(
        &self,
        things: &[EntityId],
    ) -> Result<HashMap<EntityId, HashMap<FactId, Vec<ClaimWrite>>>, MemoryError> {
        let mut tx = self.memory.pool.begin().await.map_err(store)?;
        let context = Context::load(&mut tx, things).await?;
        let rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT w.ordinal, w.written_at, w.session, w.bot, w.refs AS kept_refs, \
                    w.stands_for AS kept_stands_for, {FACT_WRITE_COLUMNS} FROM fact_write w \
             ORDER BY w.entity, w.fact_id, w.ordinal"
        )))
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        tx.commit().await.map_err(store)?;
        // The rows of each key, in the order they were read.
        let mut by_key: HashMap<String, Vec<&sqlx::mysql::MySqlRow>> = HashMap::new();
        for row in &rows {
            let key: String = row.try_get("entity").map_err(store)?;
            by_key.entry(key).or_default().push(row);
        }
        let mut answered = HashMap::with_capacity(context.plain.len());
        for (thing, key) in &context.plain {
            let mut histories: HashMap<FactId, Vec<ClaimWrite>> = HashMap::new();
            let mut whole = true;
            for row in by_key.get(key.as_str()).into_iter().flatten() {
                match Self::claim_write_of(&context, key, row) {
                    Ok(Some((id, write))) => histories.entry(id).or_default().push(write),
                    // A name it cannot place, or a row that does not read: the read
                    // of this thing on its own answers, or names the damage.
                    Ok(None) | Err(_) => {
                        whole = false;
                        break;
                    }
                }
            }
            if whole {
                answered.insert(thing.clone(), histories);
            }
        }
        Ok(answered)
    }
}
