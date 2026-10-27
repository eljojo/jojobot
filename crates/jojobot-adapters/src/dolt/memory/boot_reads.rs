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

use jojobot_domain::memory::boot_reader::{BootReader, FieldsVersioned};
use jojobot_domain::memory::search::DocScan;
use jojobot_domain::memory::types::DeclaredType;
use jojobot_domain::memory::{
    ClaimWrite, EntitySnapshot, EntityVersion, Fact, FieldAges, FieldBacking, KeyTrails,
};

use super::*;

/// The writer and session of the first write of each claim, by its key and id.
type Writers = HashMap<(String, String), (Option<String>, Option<String>)>;

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
    /// Every handle the entity table lists, with the key its rows are filed under.
    key_of: HashMap<EntityId, EntityId>,
    /// Every handle the entity table lists.
    ids: HashSet<EntityId>,
    /// Every name a merge, or a forwarding row, touches.
    touched: HashSet<String>,
    /// Every handle some thing wore and left, with the badges it was worn by.
    former: HashMap<String, Vec<String>>,
    /// The handles each badge wore and left.
    left_by: HashMap<String, HashSet<String>>,
    /// The names a merge touches, as a stored value may spell them: each key, and
    /// the handle of each thing wearing one.
    merge_names: Vec<String>,
}

impl Context {
    fn marked_links(
        &self,
        stored: &BTreeMap<String, String>,
    ) -> Option<BTreeMap<String, Vec<EntityId>>> {
        jojobot_domain::memory::marked_field_links(stored, |id| {
            self.handle_of(&EntityId(id.to_string()))
        })
    }

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
        let left = sqlx::query("SELECT former_handle, badge FROM entity_former_handle")
            .fetch_all(&mut **tx)
            .await
            .map_err(store)?;
        let mut former: HashMap<String, Vec<String>> = HashMap::with_capacity(left.len());
        let mut left_by: HashMap<String, HashSet<String>> = HashMap::new();
        for row in &left {
            let handle: String = row.try_get("former_handle").map_err(store)?;
            let badge: String = row.try_get("badge").map_err(store)?;
            left_by
                .entry(badge.clone())
                .or_default()
                .insert(handle.clone());
            former.entry(handle).or_default().push(badge);
        }
        let ids: HashSet<EntityId> = keys.keys().cloned().collect();
        let key_of = keys.clone();
        let mut merge_names: Vec<String> = touched.iter().cloned().collect();
        merge_names.extend(
            keys.iter()
                .filter(|(_, key)| touched.contains(key.as_str()))
                .map(|(id, _)| id.as_str().to_string()),
        );
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
            key_of,
            ids,
            touched,
            former,
            left_by,
            merge_names,
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

    /// **Whether a stored value names a thing a merge touched**, in any spelling a
    /// link takes, so a value a merge would repoint is left to the read of one thing.
    fn names_a_merge(&self, value: &str) -> bool {
        self.merge_names.iter().any(|name| value.contains(name))
    }

    /// **The handle a handle written in a value answers to now**, for a handle the
    /// entity table lists; one it does not is left as it was written when no handle
    /// was ever worn under that name, and to the read of one thing otherwise.
    fn handle_written(&self, written: &EntityId) -> Option<String> {
        if self.ids.contains(written) {
            return Some(written.to_string());
        }
        let numbered = written
            .as_str()
            .rsplit_once('/')
            .is_some_and(|(_, tail)| !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()));
        (!self.former.contains_key(written.as_str()) && !numbered).then(|| written.to_string())
    }

    /// **A thing's fields, every value naming a thing served as the handle it
    /// answers to now**, as the read of one thing serves them; `None` when a name in
    /// them is one the grouped reads cannot place.
    fn composed(
        &self,
        declared: &[DeclaredType],
        mut fields: BTreeMap<String, String>,
    ) -> Option<BTreeMap<String, String>> {
        if jojobot_domain::memory::link_rule() == jojobot_domain::memory::LinkRule::On {
            return self.composed_marked(fields);
        }
        let referenced = jojobot_domain::memory::reference_field_values(&fields, declared);
        for (key, items) in &referenced {
            let mut composed = Vec::with_capacity(items.len());
            for item in items {
                let stored = jojobot_domain::memory::stored_handle_id(item).unwrap_or(item);
                composed.push(self.handle_of(&EntityId(stored.to_string()))?);
            }
            fields.insert(key.clone(), DoltMemory::serve_list(composed));
        }
        for (key, value) in fields.clone() {
            if referenced.iter().any(|(held, _)| held == &key) {
                continue;
            }
            let items: Vec<&str> = value.split(',').collect();
            if items
                .iter()
                .all(|item| jojobot_domain::memory::stored_handle_id(item).is_some())
            {
                let mut composed = Vec::with_capacity(items.len());
                for item in items {
                    if let Some(id) = jojobot_domain::memory::stored_handle_id(item) {
                        composed.push(self.handle_of(&EntityId(id.to_string()))?);
                    }
                }
                fields.insert(key, DoltMemory::serve_list(composed));
            } else if items
                .iter()
                .any(|item| jojobot_domain::memory::stored_handle_id(item).is_some())
            {
                // A list that mixes words with marked items is served as the rule
                // that is on serves it; see the per-thing reader.
                let one = BTreeMap::from([(key.clone(), value.clone())]);
                let mut one = self.composed_marked(one)?;
                if let Some(served) = one.remove(&key) {
                    fields.insert(key, served);
                }
            } else {
                let handles = jojobot_domain::memory::handles_in_value(&value);
                if handles.is_empty() {
                    continue;
                }
                let mut composed = Vec::with_capacity(handles.len());
                for id in handles {
                    composed.push(self.handle_written(&id)?);
                }
                fields.insert(key, composed.join(", "));
            }
        }
        Some(fields)
    }

    /// **[`Self::composed`] for the link rule that is on**: each comma item is served
    /// by what it wears. A mark is the thing's id, served as its handle; unmarked
    /// text, including a handle spelling, is served as written. A list names each
    /// marked thing once. `None` when a marked id is one the grouped reads cannot
    /// place, so the per-thing reader can resolve it.
    fn composed_marked(
        &self,
        mut fields: BTreeMap<String, String>,
    ) -> Option<BTreeMap<String, String>> {
        for (key, value) in fields.clone() {
            let items: Vec<&str> = value.split(',').map(str::trim).collect();
            if !items
                .iter()
                .any(|item| jojobot_domain::memory::stored_handle_id(item).is_some())
            {
                continue;
            }
            let mut composed: Vec<(Option<EntityId>, String)> = Vec::with_capacity(items.len());
            for item in items {
                match jojobot_domain::memory::stored_handle_id(item) {
                    Some(id) => {
                        let handle = self.handle_of(&EntityId(id.to_string()))?;
                        composed.push((Some(handle.clone()), handle.to_string()));
                    }
                    None => composed.push((None, item.to_string())),
                }
            }
            fields.insert(key, jojobot_domain::memory::list::serve_values(composed));
        }
        Some(fields)
    }

    /// **The version rows of a thing**: the `entity_write` rows that are this
    /// thing's, as `entity_writes_of` selects them. A row filed under a handle the
    /// thing wears or wore, or under its key, that names the thing's badge, or
    /// names none and was not filed under a handle someone else left.
    fn versions_of<'a>(
        &self,
        written: &'a HashMap<String, Vec<VersionRow>>,
        handle: &EntityId,
        key: &EntityId,
    ) -> Vec<&'a VersionRow> {
        let left = self.left_by.get(key.as_str());
        let mut filed: Vec<&str> = vec![handle.as_str(), key.as_str()];
        if let Some(left) = left {
            filed.extend(left.iter().map(String::as_str));
        }
        filed.sort_unstable();
        filed.dedup();
        let mut mine = Vec::new();
        for under in filed {
            let left_it = self
                .former
                .get(under)
                .is_some_and(|badges| badges.iter().any(|b| b != key.as_str()));
            let named_here = left.is_some_and(|left| left.contains(under));
            for row in written.get(under).into_iter().flatten() {
                let belongs = match &row.badge {
                    Some(badge) => badge == key.as_str(),
                    None => named_here || !left_it,
                };
                if belongs {
                    mine.push(row);
                }
            }
        }
        mine
    }

    /// **Whether a recorded writer is the thing filed under `against`**, for a writer
    /// the entity table lists or one no handle was ever worn under; any other is
    /// left to the read of one thing.
    fn writer_is(&self, writer: &EntityId, against: &EntityId) -> Option<bool> {
        if writer == against {
            return Some(true);
        }
        if let Some(key) = self.key_of.get(writer) {
            return Some(key == against);
        }
        let numbered = writer
            .as_str()
            .rsplit_once('/')
            .is_some_and(|(_, tail)| !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()));
        (!self.former.contains_key(writer.as_str()) && !numbered).then_some(false)
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
        (self.ids.contains(&named) && !self.former.contains_key(named.as_str())).then_some(named)
    }
}

/// One `entity_write` row, as much of it as the grouped reads of versions use.
struct VersionRow {
    badge: Option<String>,
    written_at: Option<String>,
    /// The row itself, for the read of a thing's versions.
    snapshot: Option<Snapshot>,
}

/// What a version row holds beyond the moment: the thing as it stood.
struct Snapshot {
    kind: Option<String>,
    name: Option<String>,
    source: Option<String>,
    crm: Option<String>,
    parent: Option<String>,
    boot: Option<String>,
    archived_reason: Option<String>,
    archived_at: Option<String>,
    merged_into: Option<String>,
    aliases: Option<String>,
    prose: Option<String>,
    session: Option<String>,
}

impl DoltBootReader {
    /// **The version rows of every thing**, by the handle each was filed under.
    /// `whole` also reads what each row holds of the thing; the reads that only
    /// count versions or take their moment leave it out.
    async fn versions_written(
        tx: &mut Transaction<'_, MySql>,
        whole: bool,
    ) -> Result<HashMap<String, Vec<VersionRow>>, MemoryError> {
        let columns = if whole {
            "entity, badge, written_at, kind, name, source, crm, parent, boot, \
             archived_reason, archived_at, merged_into, aliases, prose, session"
        } else {
            "entity, badge, written_at"
        };
        let rows = sqlx::query(AssertSqlSafe(format!("SELECT {columns} FROM entity_write")))
            .fetch_all(&mut **tx)
            .await
            .map_err(store)?;
        let mut written: HashMap<String, Vec<VersionRow>> = HashMap::new();
        for row in &rows {
            let filed: String = row.try_get("entity").map_err(store)?;
            let text = |column: &str| -> Result<Option<String>, MemoryError> {
                row.try_get(column).map_err(store)
            };
            written.entry(filed).or_default().push(VersionRow {
                badge: text("badge")?,
                written_at: text("written_at")?,
                snapshot: if whole {
                    Some(Snapshot {
                        kind: text("kind")?,
                        name: text("name")?,
                        source: text("source")?,
                        crm: text("crm")?,
                        parent: text("parent")?,
                        boot: text("boot")?,
                        archived_reason: text("archived_reason")?,
                        archived_at: text("archived_at")?,
                        merged_into: text("merged_into")?,
                        aliases: text("aliases")?,
                        prose: text("prose")?,
                        session: text("session")?,
                    })
                } else {
                    None
                },
            });
        }
        Ok(written)
    }

    /// Field writes joined to their records and decoded by the per-thing reader.
    /// A supplied-only count selects its homes so unrelated claim details are
    /// not materialized; the full fold fill passes no selection.
    pub(super) async fn writes_by_key(
        tx: &mut Transaction<'_, MySql>,
        keys: Option<&[String]>,
    ) -> Result<HashMap<String, Vec<KeyWrite>>, MemoryError> {
        let mut query = sqlx::QueryBuilder::<MySql>::new(
            "SELECT w.entity, w.`key`, w.ordinal, w.value, w.fact_id, w.write_class, \
                    w.written_at, f.status, f.provenance, f.standing, f.details, f.app_stamp
             FROM field_write w
             JOIN fact f ON f.entity = w.entity AND f.id = w.fact_id",
        );
        if let Some(keys) = keys {
            if keys.is_empty() {
                query.push(" WHERE 1 = 0");
            } else {
                query.push(" WHERE w.entity IN (");
                let mut values = query.separated(", ");
                for key in keys {
                    values.push_bind(key);
                }
                values.push_unseparated(")");
            }
        }
        let rows = query.build().fetch_all(&mut **tx).await.map_err(store)?;
        let mut by_key: HashMap<String, Vec<KeyWrite>> = HashMap::new();
        for row in &rows {
            let entity: String = row.try_get("entity").map_err(store)?;
            if let Some(write) = DoltMemory::key_write_of(row, &entity)? {
                by_key.entry(entity).or_default().push(write);
            }
        }
        Ok(by_key)
    }

    /// **One claim as it stands, served as the read of a single thing serves it**,
    /// or `None` when a name in it is one the grouped reads cannot place.
    #[allow(clippy::too_many_arguments)]
    fn claim_of(
        &self,
        context: &Context,
        declared: &[DeclaredType],
        handle: &EntityId,
        key: &EntityId,
        row: &sqlx::mysql::MySqlRow,
        folded: &HashMap<(String, String), Vec<&sqlx::mysql::MySqlRow>>,
        referred: &HashMap<(String, String), Vec<EntityId>>,
        marked: &HashMap<(String, String), Vec<FactAddress>>,
        writers: &Writers,
    ) -> Result<Option<Fact>, MemoryError> {
        let id = FactId(row.try_get::<String, _>("id").map_err(store)?);
        let claim = (key.as_str().to_string(), id.as_str().to_string());
        let (fields, sets, reaching) = match folded.get(&claim) {
            Some(rows) => DoltMemory::read_fields(rows)?,
            None => Default::default(),
        };
        let refs = referred.get(&claim).cloned().unwrap_or_default();
        let stands_for = marked.get(&claim).cloned().unwrap_or_default();
        let mut served = fact_from(row, key.clone(), id, fields, refs, stands_for)?;
        // Whether another bot wrote a claim that could be a thought: one drawing a
        // connection edge.
        if jojobot_domain::memory::authorship::is_connection(served.edge.as_ref()) {
            let by_another = match writers.get(&claim) {
                None => false,
                Some((recorded, of_the_run)) => {
                    match jojobot_domain::memory::authorship::first_writer(
                        recorded.clone().map(EntityId),
                        || of_the_run.clone().map(EntityId),
                    ) {
                        None => false,
                        Some(bot) => {
                            if self.memory.supplied.record_for(&bot).is_some() {
                                return Ok(None);
                            }
                            let Some(names_home) = context.writer_is(&bot, key) else {
                                return Ok(None);
                            };
                            jojobot_domain::memory::authorship::another_bots_claim(
                                handle,
                                Some(&bot),
                                |_| names_home,
                            )
                        }
                    }
                }
            };
            served.written_by_another = by_another;
        }
        served.sets = sets;
        served.reaching = reaching;
        served.home = handle.clone();
        served.subject = handle.clone();
        if let Some(source) = &served.derived_from {
            let Some(at) = context.address_of(source) else {
                return Ok(None);
            };
            served.derived_from = Some(at);
        }
        if let Some(edge) = &served.edge {
            let Some(object) = context.handle_of(&edge.object) else {
                return Ok(None);
            };
            served.edge = Some(Edge {
                shape: edge.shape,
                object,
            });
        }
        let mut resolved_refs = Vec::with_capacity(served.refs.len());
        for object in &served.refs {
            let Some(at) = context.handle_of(object) else {
                return Ok(None);
            };
            resolved_refs.push(at);
        }
        served.refs = resolved_refs;
        let mut resolved_marks = Vec::with_capacity(served.stands_for.len());
        for named in &served.stands_for {
            let Some(at) = context.address_of(named) else {
                return Ok(None);
            };
            resolved_marks.push(at);
        }
        served.stands_for = resolved_marks;
        let Some(field_links) = context.marked_links(&served.fields) else {
            return Ok(None);
        };
        served.field_links = field_links;
        let Some(composed) = context.composed(declared, std::mem::take(&mut served.fields)) else {
            return Ok(None);
        };
        served.fields = composed;
        Ok(Some(served))
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
        let mut by_key = Self::writes_by_key(&mut tx, None).await?;
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
    async fn key_trails_many(
        &self,
        things: &[EntityId],
    ) -> Result<HashMap<EntityId, KeyTrails>, MemoryError> {
        let mut tx = self.memory.pool.begin().await.map_err(store)?;
        let context = Context::load(&mut tx, things).await?;
        let writes = sqlx::query(
            "SELECT entity, `key`, ordinal, value, fact_id, write_class, written_at \
             FROM field_write ORDER BY entity, ordinal",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        // The newest write of each claim: the claim as it stands.
        let claims = sqlx::query(AssertSqlSafe(format!(
            "SELECT {FACT_WRITE_COLUMNS} FROM fact_write w \
             JOIN (SELECT entity, fact_id, MAX(ordinal) AS newest FROM fact_write \
                   GROUP BY entity, fact_id) n \
               ON n.entity = w.entity AND n.fact_id = w.fact_id AND n.newest = w.ordinal \
             ORDER BY w.entity, w.fact_id"
        )))
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        tx.commit().await.map_err(store)?;
        let mut raw: HashMap<String, Vec<RawWrite>> = HashMap::new();
        for row in &writes {
            let entity: String = row.try_get("entity").map_err(store)?;
            raw.entry(entity.clone()).or_default().push(RawWrite {
                key: row.try_get("key").map_err(store)?,
                ordinal: row.try_get("ordinal").map_err(store)?,
                value: row.try_get("value").map_err(store)?,
                fact: row.try_get("fact_id").map_err(store)?,
                class: jojobot_domain::memory::WriteClass::from_token(
                    &row.try_get::<String, _>("write_class").map_err(store)?,
                ),
                written_at: parse_stamp(
                    row.try_get::<Option<String>, _>("written_at")
                        .map_err(store)?,
                    &entity,
                ),
            });
        }
        let mut standing: HashMap<String, Vec<&sqlx::mysql::MySqlRow>> = HashMap::new();
        for row in &claims {
            let entity: String = row.try_get("entity").map_err(store)?;
            standing.entry(entity).or_default().push(row);
        }
        let mut answered = HashMap::with_capacity(context.plain.len());
        for (thing, key) in &context.plain {
            let rows = raw.get(key.as_str()).map(Vec::as_slice).unwrap_or(&[]);
            if rows.iter().any(|write| {
                write
                    .value
                    .as_deref()
                    .is_some_and(|v| context.names_a_merge(v))
            }) {
                continue;
            }
            let Some(handle) = context.handle_of(key) else {
                continue;
            };
            let mut facts = Vec::new();
            let mut whole = true;
            for row in standing.get(key.as_str()).into_iter().flatten() {
                let id = FactId(row.try_get::<String, _>("id").map_err(store)?);
                match fact_from(
                    row,
                    key.clone(),
                    id,
                    Default::default(),
                    Vec::new(),
                    Vec::new(),
                ) {
                    Ok(mut fact) => {
                        fact.home = handle.clone();
                        fact.subject = handle.clone();
                        facts.push(fact);
                    }
                    Err(_) => {
                        whole = false;
                        break;
                    }
                }
            }
            if whole {
                answered.insert(thing.clone(), trails_from(rows, &facts));
            }
        }
        Ok(answered)
    }
    async fn fields_versioned_many(
        &self,
        things: &[EntityId],
    ) -> Result<HashMap<EntityId, FieldsVersioned>, MemoryError> {
        let mut tx = self.memory.pool.begin().await.map_err(store)?;
        let context = Context::load(&mut tx, things).await?;
        let mut by_key = Self::writes_by_key(&mut tx, None).await?;
        let declared = DoltMemory::types_in(&mut tx).await?;
        let counts =
            sqlx::query("SELECT entity, COUNT(*) AS written FROM fact_write GROUP BY entity")
                .fetch_all(&mut *tx)
                .await
                .map_err(store)?;
        let versions = Self::versions_written(&mut tx, false).await?;
        tx.commit().await.map_err(store)?;
        let mut claims: HashMap<String, i64> = HashMap::with_capacity(counts.len());
        for row in &counts {
            claims.insert(
                row.try_get("entity").map_err(store)?,
                row.try_get("written").map_err(store)?,
            );
        }
        let mut answered = HashMap::with_capacity(context.plain.len());
        for (thing, key) in &context.plain {
            let writes = by_key.get_mut(key.as_str()).map(std::mem::take);
            let held =
                jojobot_domain::memory::folded_fields(&writes.unwrap_or_default(), &declared);
            let Some(held) = context.composed(&declared, held) else {
                continue;
            };
            let written = claims.get(key.as_str()).copied().unwrap_or(0);
            let signalled = context.versions_of(&versions, thing, key).len() as i64;
            answered.insert(thing.clone(), (held, (written + signalled) as u64));
        }
        Ok(answered)
    }

    async fn field_ages_many(
        &self,
        things: &[EntityId],
    ) -> Result<HashMap<EntityId, FieldAges>, MemoryError> {
        let mut tx = self.memory.pool.begin().await.map_err(store)?;
        let context = Context::load(&mut tx, things).await?;
        let mut by_key = Self::writes_by_key(&mut tx, None).await?;
        let declared = DoltMemory::types_in(&mut tx).await?;
        let versions = Self::versions_written(&mut tx, false).await?;
        let fields =
            sqlx::query("SELECT entity, `key`, value, fact_id, written_at FROM field_write")
                .fetch_all(&mut *tx)
                .await
                .map_err(store)?;
        let facts = sqlx::query("SELECT entity, fact_id, written_at FROM fact_write")
            .fetch_all(&mut *tx)
            .await
            .map_err(store)?;
        tx.commit().await.map_err(store)?;
        type Field = (String, Option<String>, String, Option<String>);
        let mut written: HashMap<String, Vec<Field>> = HashMap::new();
        for row in &fields {
            written
                .entry(row.try_get("entity").map_err(store)?)
                .or_default()
                .push((
                    row.try_get("key").map_err(store)?,
                    row.try_get("value").map_err(store)?,
                    row.try_get("fact_id").map_err(store)?,
                    row.try_get("written_at").map_err(store)?,
                ));
        }
        let mut claimed: HashMap<String, Vec<(String, Option<String>)>> = HashMap::new();
        for row in &facts {
            claimed
                .entry(row.try_get("entity").map_err(store)?)
                .or_default()
                .push((
                    row.try_get("fact_id").map_err(store)?,
                    row.try_get("written_at").map_err(store)?,
                ));
        }
        let mut answered = HashMap::with_capacity(context.plain.len());
        for (thing, key) in &context.plain {
            let writes = by_key.get_mut(key.as_str()).map(std::mem::take);
            let keys = jojobot_domain::memory::folded_ages(&writes.unwrap_or_default(), &declared);
            let mut newest: Option<jiff::Timestamp> = None;
            for row in context.versions_of(&versions, thing, key) {
                newest = newest.max(parse_stamp(row.written_at.clone(), key.as_str()));
            }
            // The record of the release notes a bot was told is no change of the thing.
            let own = written.get(key.as_str()).map(Vec::as_slice).unwrap_or(&[]);
            let ledgers = jojobot_domain::memory::review::ledger_records(
                own.iter()
                    .map(|(held, value, fact, _)| (held.as_str(), value.as_deref(), fact.clone())),
            );
            for (held, _, fact, stamp) in own {
                if jojobot_domain::memory::review::counts_as_a_change(Some(held), fact, &ledgers) {
                    newest = newest.max(parse_stamp(stamp.clone(), key.as_str()));
                }
            }
            for (fact, stamp) in claimed.get(key.as_str()).into_iter().flatten() {
                if jojobot_domain::memory::review::counts_as_a_change(None, fact, &ledgers) {
                    newest = newest.max(parse_stamp(stamp.clone(), key.as_str()));
                }
            }
            answered.insert(thing.clone(), FieldAges { newest, keys });
        }
        Ok(answered)
    }
    async fn entity_history_many(
        &self,
        things: &[EntityId],
    ) -> Result<HashMap<EntityId, Vec<EntityVersion>>, MemoryError> {
        let mut tx = self.memory.pool.begin().await.map_err(store)?;
        let context = Context::load(&mut tx, things).await?;
        let versions = Self::versions_written(&mut tx, true).await?;
        tx.commit().await.map_err(store)?;
        let mut answered = HashMap::with_capacity(context.plain.len());
        'things: for (thing, key) in &context.plain {
            let Some(handle) = context.handle_of(key) else {
                continue;
            };
            let mut kept: Vec<(Option<jiff::Timestamp>, EntityVersion)> = Vec::new();
            for row in context.versions_of(&versions, &handle, key) {
                let Some(held) = &row.snapshot else {
                    continue 'things;
                };
                let Some(stamp) = row.written_at.clone() else {
                    continue 'things;
                };
                let written_at = parse_stamp(Some(stamp), key.as_str());
                let snapshot = match &held.name {
                    // A row an older build appended holds no name, and no other
                    // column either: it says that a write happened and when.
                    None => None,
                    Some(name) => {
                        let aliases: Vec<String> = match &held.aliases {
                            None => Vec::new(),
                            Some(raw) => match serde_json::from_str(raw) {
                                Ok(aliases) => aliases,
                                Err(_) => continue 'things,
                            },
                        };
                        // A pointer to a thing a merge touched is the read of one
                        // thing's to place.
                        let pointed = |stored: &Option<String>| match stored {
                            None => Some(None),
                            Some(stored) => context.handle_of(&EntityId(stored.clone())).map(Some),
                        };
                        let (Some(parent), Some(merged_into)) =
                            (pointed(&held.parent), pointed(&held.merged_into))
                        else {
                            continue 'things;
                        };
                        Some(EntitySnapshot {
                            kind: held.kind.clone().unwrap_or_default(),
                            name: name.clone(),
                            source: held.source.clone().unwrap_or_default(),
                            crm: held.crm.clone(),
                            parent,
                            boot: held.boot.clone().unwrap_or_default(),
                            archived_reason: held.archived_reason.clone(),
                            archived_at: held.archived_at.clone(),
                            merged_into,
                            aliases,
                            prose: held.prose.clone().unwrap_or_default(),
                        })
                    }
                };
                kept.push((
                    written_at,
                    EntityVersion {
                        ordinal: 0,
                        thing: handle.clone(),
                        written_at,
                        session: held.session.clone(),
                        snapshot,
                    },
                ));
            }
            // In the order they happened; a version nobody stamped sorts first.
            kept.sort_by_key(|(at, _)| *at);
            answered.insert(
                thing.clone(),
                kept.into_iter()
                    .enumerate()
                    .map(|(at, (_, mut version))| {
                        version.ordinal = at + 1;
                        version
                    })
                    .collect(),
            );
        }
        Ok(answered)
    }
    async fn scans_many(
        &self,
        things: &[EntityId],
    ) -> Result<HashMap<EntityId, DocScan>, MemoryError> {
        let mut tx = self.memory.pool.begin().await.map_err(store)?;
        let context = Context::load(&mut tx, things).await?;
        let listing = self.memory.index(&mut tx).await?;
        let own = sqlx::query("SELECT id, prose, badge FROM entity")
            .fetch_all(&mut *tx)
            .await
            .map_err(store)?;
        let mut by_key = Self::writes_by_key(&mut tx, None).await?;
        let declared = DoltMemory::types_in(&mut tx).await?;
        let claims = sqlx::query(AssertSqlSafe(format!(
            "SELECT {FACT_WRITE_COLUMNS} FROM fact_write w \
             JOIN (SELECT entity, fact_id, MAX(ordinal) AS newest FROM fact_write \
                   GROUP BY entity, fact_id) n \
               ON n.entity = w.entity AND n.fact_id = w.fact_id AND n.newest = w.ordinal \
             ORDER BY w.entity, w.fact_id"
        )))
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        let written = sqlx::query(
            "SELECT entity, fact_id, `key`, value, write_class FROM field_write \
             ORDER BY entity, fact_id, `key`, ordinal",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        let refs = sqlx::query(
            "SELECT fact_home, fact_id, entity FROM fact_event_ref \
             ORDER BY fact_home, fact_id, ordinal",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        let marks = sqlx::query(
            "SELECT fact_home, fact_id, source_home, source_id FROM fact_stands_for \
             ORDER BY fact_home, fact_id, ordinal",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        let first = sqlx::query(
            "SELECT w.entity, w.fact_id, w.bot, s.bot AS run_bot FROM fact_write w \
             JOIN (SELECT entity, fact_id, MIN(ordinal) AS first FROM fact_write \
                   GROUP BY entity, fact_id) f \
               ON f.entity = w.entity AND f.fact_id = w.fact_id AND f.first = w.ordinal \
             LEFT JOIN session s ON s.sid = w.session",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        tx.commit().await.map_err(store)?;

        // The rows beside the claims, by the claim they belong to.
        type Claim = (String, String);
        let mut folded: HashMap<Claim, Vec<&sqlx::mysql::MySqlRow>> = HashMap::new();
        for row in &written {
            let claim = (
                row.try_get::<String, _>("entity").map_err(store)?,
                row.try_get::<String, _>("fact_id").map_err(store)?,
            );
            folded.entry(claim).or_default().push(row);
        }
        let mut referred: HashMap<Claim, Vec<EntityId>> = HashMap::new();
        for row in &refs {
            let claim = (
                row.try_get::<String, _>("fact_home").map_err(store)?,
                row.try_get::<String, _>("fact_id").map_err(store)?,
            );
            referred
                .entry(claim)
                .or_default()
                .push(EntityId(row.try_get::<String, _>("entity").map_err(store)?));
        }
        let mut marked: HashMap<Claim, Vec<FactAddress>> = HashMap::new();
        for row in &marks {
            let claim = (
                row.try_get::<String, _>("fact_home").map_err(store)?,
                row.try_get::<String, _>("fact_id").map_err(store)?,
            );
            marked.entry(claim).or_default().push(FactAddress::new(
                EntityId(row.try_get::<String, _>("source_home").map_err(store)?),
                FactId(row.try_get::<String, _>("source_id").map_err(store)?),
            ));
        }
        let mut writers: Writers = HashMap::new();
        for row in &first {
            let claim = (
                row.try_get::<String, _>("entity").map_err(store)?,
                row.try_get::<String, _>("fact_id").map_err(store)?,
            );
            writers.insert(
                claim,
                (
                    row.try_get::<Option<String>, _>("bot").map_err(store)?,
                    row.try_get::<Option<String>, _>("run_bot").map_err(store)?,
                ),
            );
        }
        let mut standing: HashMap<String, Vec<&sqlx::mysql::MySqlRow>> = HashMap::new();
        for row in &claims {
            standing
                .entry(row.try_get::<String, _>("entity").map_err(store)?)
                .or_default()
                .push(row);
        }
        // Each thing's prose and the badge its document goes by.
        let mut prose: HashMap<EntityId, (String, Option<String>)> =
            HashMap::with_capacity(own.len());
        for row in &own {
            prose.insert(
                EntityId(row.try_get::<String, _>("id").map_err(store)?),
                (
                    row.try_get::<String, _>("prose").map_err(store)?,
                    row.try_get::<Option<String>, _>("badge").map_err(store)?,
                ),
            );
        }
        let listed: HashMap<&EntityId, &Entity> = listing.iter().map(|e| (&e.id, e)).collect();

        let mut answered = HashMap::with_capacity(context.plain.len());
        for (thing, key) in &context.plain {
            let Some(entity) = listed.get(thing) else {
                continue;
            };
            // A thing with no badge is the read of one thing's to refuse.
            let (Some((prose, Some(badge))), Some(handle)) =
                (prose.get(thing), context.handle_of(key))
            else {
                continue;
            };
            let stored = jojobot_domain::memory::folded_fields(
                &by_key
                    .get_mut(key.as_str())
                    .map(std::mem::take)
                    .unwrap_or_default(),
                &declared,
            );
            let Some(field_links) = context.marked_links(&stored) else {
                continue;
            };
            let Some(fields) = context.composed(&declared, stored) else {
                continue;
            };
            let mut facts = Vec::new();
            let mut whole = true;
            for row in standing.get(key.as_str()).into_iter().flatten() {
                match self.claim_of(
                    &context, &declared, &handle, key, row, &folded, &referred, &marked, &writers,
                ) {
                    Ok(Some(fact)) => facts.push(fact),
                    Ok(None) | Err(_) => {
                        whole = false;
                        break;
                    }
                }
            }
            if !whole {
                continue;
            }
            answered.insert(
                thing.clone(),
                DocScan {
                    doc_id: badge.clone(),
                    title: entity.name.clone(),
                    prose: prose.clone(),
                    facts,
                    fields,
                    field_links,
                    entity: Some((*entity).clone()),
                    owner: None,
                    unmapped: Vec::new(),
                },
            );
        }
        Ok(answered)
    }
}

/// **The trails of one thing, from every write on it and the records they were
/// written in.** The one fold the read of a single thing and the boot reader run:
/// a key's trail is its writes that reach the thing, a key that stays on its record
/// keeps its writes with their moments, and a record's key is kept off the thing
/// when its newest write of that key, unless a later one took the key off, was made
/// under a class that does not reach it.
pub(super) fn trails_from(rows: &[RawWrite], facts: &[Fact]) -> KeyTrails {
    let by_id: std::collections::HashMap<&FactId, &Fact> =
        facts.iter().map(|f| (&f.id, f)).collect();
    let mut trails = jojobot_domain::memory::KeyTrails::default();
    let mut held: std::collections::BTreeMap<(FactId, String), jojobot_domain::memory::WriteClass> =
        std::collections::BTreeMap::new();
    for row in rows {
        let key = row.key.clone();
        let value = row.value.clone();
        let carried = FactId(row.fact.clone());
        let class = row.class;
        match &value {
            Some(_) => {
                held.insert((carried.clone(), key.clone()), class);
            }
            None => {
                held.remove(&(carried.clone(), key.clone()));
            }
        }
        if !jojobot_domain::memory::reaches_the_thing(class) {
            // **A key that stays on its record keeps its writes here**, with
            // their moments, because the history of a key on a thing leaves
            // them out.
            if by_id.contains_key(&carried) {
                trails
                    .record_writes
                    .push(jojobot_domain::memory::RecordFieldWrite {
                        fact: carried,
                        key,
                        ordinal: row.ordinal.max(0) as u64,
                        value,
                        written_at: row.written_at,
                    });
            }
            continue;
        }
        let Some(fact) = by_id.get(&carried) else {
            continue;
        };
        trails
            .history
            .entry(key)
            .or_default()
            .push(super::field_write_of(fact, value, row.written_at));
    }
    for ((fact, key), class) in held {
        if !jojobot_domain::memory::reaches_the_thing(class) {
            trails.kept_off.entry(fact).or_default().insert(key);
        }
    }
    trails
        .record_writes
        .sort_by(|a, b| (&a.key, a.ordinal).cmp(&(&b.key, b.ordinal)));
    trails
}
