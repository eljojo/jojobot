//! **Today's stored rows, read once into the claims import's [`Rows`].**
//!
//! A read-only, stateless reader: it reads each table in one transaction and
//! decodes it with the owners the store already has (`fact_from` for a claim's
//! columns, `parse_stamp` for a moment, [`Field::of_token`] for a declaration),
//! so there is no second decoder to drift. It writes nothing, wires nothing and
//! changes no boot; the import and the State's parity table call it.
//!
//! **Nothing is invented.** A NULL stays NULL, an ordinal stays as stored and no
//! row is reordered by time. Every [`ObjectId`] is the permanent key a row is
//! filed under: an entity's badge when it has one, else its id. A stored name
//! that names no key (a pointer to a thing that is gone, a row filed under a
//! handle nobody wears) keeps its stored spelling and is reported as a
//! [`Finding`]; it is never repaired by guessing.

use std::collections::{BTreeMap, HashMap, HashSet};

use jiff::Timestamp;
use jojobot_domain::log::ObjectId;
use jojobot_domain::log::claims::import::{
    AdoptedRow, EntityRow, EntityWriteRow, FactRow, FactWriteRow, FieldWriteRow, FormerHandleRow,
    MergeRow, MetadataRow, Rows, SuppliedRow, TypeWriteRow, adopted_identity, record_id,
    supply_identity,
};
use jojobot_domain::log::claims::{ClaimBody, FieldValue, KeyId, ObjectSnapshot};
use jojobot_domain::memory::types::{DeclaredType, Origin};
use jojobot_domain::memory::{EntityId, FactId, WriteClass};

use super::*;

/// **A thing the reader could not place and did not guess.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub table: &'static str,
    /// The row, by its primary key as stored.
    pub row: String,
    pub what: String,
}

/// **The rows, and what the reader would not decide.**
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportRead {
    pub rows: Rows,
    pub findings: Vec<Finding>,
    /// **Homes whose claims the import skipped**, each with its claim count: a name
    /// that is no row, no former handle, no supplied record and no retired supplied
    /// handle, so the rows were removed by hand. They stay in the old tables.
    pub skipped_homes: Vec<(String, usize)>,
}

/// **Which key a stored name stands for.** A name is a badge, an entity's id, or
/// a handle some thing wore and left. Anything else is no key this store knows.
struct Keys {
    /// Every entity's id, with the key its rows are filed under.
    by_id: HashMap<String, String>,
    /// Every badge the entity table holds.
    badges: HashSet<String>,
    /// Every handle some thing wore and left, with the badges that left it.
    former: HashMap<String, Vec<String>>,
    /// Every handle the build supplies, with the supply identity its record has
    /// in the log. A stored row at the same handle is found first and wins
    /// (rule 508).
    supplied: HashMap<String, String>,
    /// Every handle a build once supplied and none supplies now, that claims are
    /// still filed under and no row or former handle wears, with the identity the
    /// import gives it. Found last.
    adopted: HashMap<String, String>,
}

/// **Where a stored name lands.** A name no key answers to is absent: the rows that
/// held it were removed by hand. A name that answers to more than one thing is not
/// absent, and nothing may guess which: it is ambiguous.
enum Placed {
    Key(String),
    Ambiguous,
    Absent,
}

impl Keys {
    /// **A current name answers before a former one**, in the order
    /// `DoltMemory::resolve` answers: badge, stored id, supplied handle, former
    /// handle, adoption.
    fn placed(&self, stored: &str) -> Placed {
        if self.badges.contains(stored) {
            return Placed::Key(stored.to_string());
        }
        if let Some(key) = self.by_id.get(stored) {
            return Placed::Key(key.clone());
        }
        if let Some(key) = self.supplied.get(stored) {
            return Placed::Key(key.clone());
        }
        match self.former.get(stored).map(Vec::as_slice) {
            Some([only]) => return Placed::Key(only.clone()),
            Some(_) => return Placed::Ambiguous,
            None => {}
        }
        match self.adopted.get(stored) {
            Some(key) => Placed::Key(key.clone()),
            None => Placed::Absent,
        }
    }
}

/// What a read carries while it decodes: the keys and the findings so far.
struct Read {
    keys: Keys,
    findings: Vec<Finding>,
    /// Homes whose claims are skipped, with their claim counts.
    skipped: BTreeMap<String, usize>,
}

impl Read {
    /// **The key a stored name stands for**, or its stored spelling with a
    /// finding when it stands for none.
    fn key(&mut self, table: &'static str, row: &str, column: &str, stored: &str) -> ObjectId {
        let what = match self.keys.placed(stored) {
            Placed::Key(key) => return ObjectId(key),
            Placed::Ambiguous => format!(
                "{column} names {stored:?}, which more than one thing once wore; \
                 the import will not guess which"
            ),
            Placed::Absent => {
                format!("{column} names {stored:?}, which is no key this store holds")
            }
        };
        self.findings.push(Finding {
            table,
            row: row.to_string(),
            what,
        });
        ObjectId(stored.to_string())
    }
}

fn text(row: &sqlx::mysql::MySqlRow, column: &str) -> Result<String, MemoryError> {
    row.try_get::<String, _>(column).map_err(store)
}

fn maybe(row: &sqlx::mysql::MySqlRow, column: &str) -> Result<Option<String>, MemoryError> {
    row.try_get::<Option<String>, _>(column).map_err(store)
}

/// **The records a build supplies, as the import is handed them.**
pub fn supplied_rows(supplied: &jojobot_domain::memory::owned::Provisions) -> Vec<SuppliedRow> {
    supplied
        .records_with_first_handles()
        .map(|(entity, first)| SuppliedRow {
            handle: entity.id.to_string(),
            first_handle: first.to_string(),
            kind: entity.kind.as_token().to_string(),
            name: entity.name.clone(),
            source: entity.source.clone(),
        })
        .collect()
}

impl DoltMemory {
    /// **Every row the claims import names, read from this store.**
    pub async fn import_rows(&self) -> Result<ImportRead, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let read = self.read_import_rows(&mut tx).await?;
        commit_native(tx).await.map_err(store)?;
        Ok(read)
    }

    async fn read_import_rows(
        &self,
        tx: &mut Transaction<'_, MySql>,
    ) -> Result<ImportRead, MemoryError> {
        let entity_rows = sqlx::query(
            "SELECT id, kind, name, source, crm, parent, boot, prose, badge, merged_into, \
             archived_reason, archived_at FROM entity ORDER BY id",
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        let former_rows = sqlx::query(
            "SELECT former_handle, badge, ordinal, changed_at FROM entity_former_handle \
             ORDER BY former_handle, badge, ordinal",
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        let mut keys = Keys {
            by_id: HashMap::with_capacity(entity_rows.len()),
            badges: HashSet::new(),
            former: HashMap::new(),
            supplied: self
                .supplied
                .records_with_first_handles()
                .map(|(entity, first)| (entity.id.to_string(), supply_identity(first).0))
                .collect(),
            adopted: HashMap::new(),
        };
        for row in &entity_rows {
            let id = text(row, "id")?;
            let key = match maybe(row, "badge")? {
                Some(badge) => {
                    keys.badges.insert(badge.clone());
                    badge
                }
                None => id.clone(),
            };
            keys.by_id.insert(id, key);
        }
        for row in &former_rows {
            keys.former
                .entry(text(row, "former_handle")?)
                .or_default()
                .push(text(row, "badge")?);
        }
        // **A handle the build stopped supplying, with writes filed under its text
        // and no row**, by the one test the boot's adoption pass uses
        // ([`super::adopt::retired_supplied_handles`]), less a record this build
        // supplies now.
        let retired = super::adopt::retired_supplied_handles(&mut **tx).await?;
        let mut adopted = Vec::new();
        for handle in retired {
            let id = EntityId(handle.clone());
            if self.supplied.record_for(&id).is_some()
                || id.kind().is_none()
                || id.slug().is_empty()
            {
                continue;
            }
            keys.adopted
                .insert(handle.clone(), adopted_identity(&handle).0);
            adopted.push(AdoptedRow { handle });
        }
        let mut read = Read {
            keys,
            findings: Vec::new(),
            skipped: BTreeMap::new(),
        };
        let mut rows = Rows {
            supplied: supplied_rows(&self.supplied),
            adopted,
            ..Rows::default()
        };

        // ── entity, with its aliases ────────────────────────────────────────
        let alias_rows =
            sqlx::query("SELECT entity, alias FROM entity_alias ORDER BY entity, ordinal")
                .fetch_all(&mut **tx)
                .await
                .map_err(store)?;
        let mut aliases: HashMap<String, Vec<String>> = HashMap::new();
        for row in &alias_rows {
            aliases
                .entry(text(row, "entity")?)
                .or_default()
                .push(text(row, "alias")?);
        }
        let written = self.entity_write_rows(tx, &mut read).await?;
        for row in &entity_rows {
            let handle = text(row, "id")?;
            let key = read.keys.by_id[&handle].clone();
            let parent =
                maybe(row, "parent")?.map(|stored| read.key("entity", &handle, "parent", &stored));
            // A forward is placed by the one owner of that decision: the merge
            // event of the fold, else the wearer of the handle the row forwards to.
            let merged_into = match maybe(row, "merged_into")? {
                None => None,
                Some(stored) => match self.survivor_of(tx, &EntityId(key.clone())).await? {
                    Some(survivor) => Some(ObjectId(survivor.0)),
                    None => {
                        read.findings.push(Finding {
                                table: "entity",
                                row: handle.clone(),
                                what: format!(
                                    "forwards to {stored:?}, which no merge event and no wearer of that handle places"
                                ),
                            });
                        Some(ObjectId(stored))
                    }
                },
            };
            // Both halves or neither, as the store reads an archive.
            let archived = match (maybe(row, "archived_reason")?, maybe(row, "archived_at")?) {
                (Some(reason), Some(at)) => at.parse::<Timestamp>().ok().map(|at| (reason, at)),
                _ => None,
            };
            rows.entities.push(EntityRow {
                // The moment of the thing's own first write, the unstamped first.
                taken_in: written.first_moment(&key),
                aliases: aliases.remove(&key).unwrap_or_default(),
                key: ObjectId(key),
                handle,
                kind: text(row, "kind")?,
                name: text(row, "name")?,
                source: text(row, "source")?,
                boot: Some(text(row, "boot")?),
                parent,
                crm: maybe(row, "crm")?,
                prose: text(row, "prose")?,
                badge: maybe(row, "badge")?,
                archived,
                merged_into,
            });
        }
        rows.entity_writes = written.rows;

        // ── entity_former_handle ────────────────────────────────────────────
        for row in &former_rows {
            let former = text(row, "former_handle")?;
            rows.former_handles.push(FormerHandleRow {
                object: ObjectId(text(row, "badge")?),
                ordinal: row.try_get::<i32, _>("ordinal").map_err(store)? as u32,
                former,
                day: text(row, "changed_at")?.parse().map_err(|_| {
                    MemoryError::Store(
                        "a former handle's changed-on day cannot be read as a date".to_string(),
                    )
                })?,
            });
        }

        Self::claim_rows(tx, &mut read, &mut rows).await?;
        Self::field_write_rows(tx, &mut read, &mut rows).await?;
        Self::merge_rows(tx, &mut read, &mut rows).await?;
        Self::type_write_rows(tx, &mut rows).await?;

        let metadata =
            sqlx::query("SELECT fact_home, fact_id, `key`, value FROM fact_event_metadata ORDER BY fact_home, fact_id, `key`")
                .fetch_all(&mut **tx)
                .await
                .map_err(store)?;
        for row in &metadata {
            let local = text(row, "fact_id")?;
            let stored = text(row, "fact_home")?;
            if read.skipped.contains_key(&stored) {
                continue;
            }
            rows.metadata.push(MetadataRow {
                home: read.key("fact_event_metadata", &local, "fact_home", &stored),
                local,
                key: text(row, "key")?,
                value: text(row, "value")?,
            });
        }
        Ok(ImportRead {
            rows,
            findings: read.findings,
            skipped_homes: read.skipped.into_iter().collect(),
        })
    }
}

/// The `entity_write` rows as the import takes them, and the first moment of
/// each thing's own.
struct Written {
    rows: Vec<EntityWriteRow>,
    first: HashMap<String, Option<Timestamp>>,
}

impl Written {
    /// **The moment of a thing's first write**, the unstamped first: a thing whose
    /// first write nobody stamped has none, never a later write's moment.
    fn first_moment(&self, key: &str) -> Option<Timestamp> {
        self.first.get(key).copied().flatten()
    }
}

impl DoltMemory {
    /// **Every `entity_write` row, attributed by the history's one owner.** A
    /// shared old row stays in every history that reads it. Several owners, or
    /// none, leave a finding; a finding never selects one history over another.
    async fn entity_write_rows(
        &self,
        tx: &mut Transaction<'_, MySql>,
        read: &mut Read,
    ) -> Result<Written, MemoryError> {
        let mut owners: HashMap<(String, u64), Vec<ObjectId>> = HashMap::new();
        for (handle, key) in &read.keys.by_id {
            for row in entity_writes_of(
                tx,
                &EntityId(handle.clone()),
                &EntityId(key.clone()),
                "entity, ordinal",
            )
            .await?
            {
                let filed_under = text(&row, "entity")?;
                let ordinal = row.try_get::<i64, _>("ordinal").map_err(store)? as u64;
                owners
                    .entry((filed_under, ordinal))
                    .or_default()
                    .push(ObjectId(key.clone()));
            }
        }
        let rows = sqlx::query(
            "SELECT entity, ordinal, written_at, aliases, archived_at, archived_reason, badge, \
             boot, crm, kind, merged_into, name, parent, prose, session, source \
             FROM entity_write ORDER BY entity, ordinal",
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        let mut written = Written {
            rows: Vec::with_capacity(rows.len()),
            first: HashMap::new(),
        };
        for row in &rows {
            let filed_under = text(row, "entity")?;
            let ordinal = row.try_get::<i64, _>("ordinal").map_err(store)? as u64;
            let at = format!("{filed_under}#{ordinal}");
            let mut owners = owners
                .remove(&(filed_under.clone(), ordinal))
                .unwrap_or_default();
            if owners.len() != 1 {
                read.findings.push(Finding {
                    table: "entity_write",
                    row: at.clone(),
                    what: format!("{} entity histories own this row", owners.len()),
                });
            }
            if owners.is_empty() {
                // No history owns it: keep the stored spelling, not a guessed key.
                owners.push(ObjectId(
                    maybe(row, "badge")?.unwrap_or_else(|| filed_under.clone()),
                ));
            }
            owners.sort();
            for owner in owners {
                let moment = parse_stamp(Some(text(row, "written_at")?), &filed_under);
                let snapshot = match maybe(row, "name")? {
                    // A row an older build appended holds no name, and no other column.
                    None => None,
                    Some(name) => {
                        let aliases: Vec<String> = match maybe(row, "aliases")? {
                            None => Vec::new(),
                            Some(raw) => serde_json::from_str(&raw).map_err(|e| {
                                MemoryError::Store(format!(
                                    "a stored alias set is not readable: {e}"
                                ))
                            })?,
                        };
                        let parent = maybe(row, "parent")?
                            .map(|stored| read.key("entity_write", &at, "parent", &stored));
                        // The survivor is found by the merge, as entity_history finds it:
                        // the version kept the handle the survivor wore at the fold.
                        let merged_into = match maybe(row, "merged_into")? {
                            None => None,
                            Some(stored) => {
                                match self.survivor_of(tx, &EntityId(owner.0.clone())).await? {
                                    Some(survivor) => Some(ObjectId(survivor.0)),
                                    None => {
                                        Some(read.key("entity_write", &at, "merged_into", &stored))
                                    }
                                }
                            }
                        };
                        Some(ObjectSnapshot {
                            kind: maybe(row, "kind")?.unwrap_or_default(),
                            name,
                            source: maybe(row, "source")?.unwrap_or_default(),
                            crm: maybe(row, "crm")?,
                            parent,
                            boot: maybe(row, "boot")?.unwrap_or_default(),
                            archived_reason: maybe(row, "archived_reason")?,
                            archived_at: maybe(row, "archived_at")?,
                            merged_into,
                            aliases,
                            prose: maybe(row, "prose")?.unwrap_or_default(),
                        })
                    }
                };
                written
                    .first
                    .entry(owner.0.clone())
                    .and_modify(|first| *first = (*first).min(moment))
                    .or_insert(moment);
                written.rows.push(EntityWriteRow {
                    thing: owner,
                    filed_under: filed_under.clone(),
                    ordinal,
                    written_at: moment,
                    session: maybe(row, "session")?,
                    snapshot,
                });
            }
        }
        Ok(written)
    }

    /// **Every claim and every write of it.** A claim's head is its own row; the
    /// references and the mark it stands for sit on the write table only, so the
    /// head takes those of its newest write.
    async fn claim_rows(
        tx: &mut Transaction<'_, MySql>,
        read: &mut Read,
        rows: &mut Rows,
    ) -> Result<(), MemoryError> {
        let runs: HashMap<String, String> = sqlx::query(super::write_origin::RUN_BOTS)
            .fetch_all(&mut **tx)
            .await
            .map_err(store)?
            .iter()
            .map(|row| Ok((text(row, "sid")?, text(row, "bot")?)))
            .collect::<Result<_, MemoryError>>()?;
        let write_rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT {FACT_WRITE_COLUMNS}, w.ordinal, w.written_at, w.session, w.refs, \
             w.stands_for, w.bot FROM fact_write w ORDER BY w.entity, w.fact_id, w.ordinal"
        )))
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        let head_rows = sqlx::query(AssertSqlSafe(format!(
            "SELECT {FACT_COLUMNS}, event_kind FROM fact ORDER BY entity, id"
        )))
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;

        // **A claim whose home is none of the keys this store holds is skipped, not
        // refused**: the rows were removed by hand, so adopting them would invent a
        // thing and refusing would stop the import altogether. They stay in the old
        // tables, and the import names each home with its claim count.
        for row in &head_rows {
            let stored = text(row, "entity")?;
            // Only an absent home is skipped; an ambiguous one is refused by name.
            if matches!(read.keys.placed(&stored), Placed::Absent) {
                *read.skipped.entry(stored).or_default() += 1;
            }
        }
        // The references and the mark a claim holds NOW sit in two tables of their
        // own, which the store's reads take a claim's head from. A write made before
        // its columns existed holds NULL there and nothing is backfilled, so the head
        // never takes them from its newest write.
        let mut now_refs: HashMap<(String, String), Vec<EntityId>> = HashMap::new();
        for row in &sqlx::query(
            "SELECT fact_home, fact_id, entity FROM fact_event_ref \
             ORDER BY fact_home, fact_id, ordinal",
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?
        {
            now_refs
                .entry((text(row, "fact_home")?, text(row, "fact_id")?))
                .or_default()
                .push(EntityId(text(row, "entity")?));
        }
        let mut now_stands: HashMap<(String, String), Vec<FactAddress>> = HashMap::new();
        for row in &sqlx::query(
            "SELECT fact_home, fact_id, source_home, source_id FROM fact_stands_for \
             ORDER BY fact_home, fact_id, ordinal",
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?
        {
            now_stands
                .entry((text(row, "fact_home")?, text(row, "fact_id")?))
                .or_default()
                .push(FactAddress::new(
                    EntityId(text(row, "source_home")?),
                    FactId(text(row, "source_id")?),
                ));
        }

        type Cells = (Vec<EntityId>, Vec<FactAddress>);
        let mut writes: BTreeMap<(String, String), Vec<(FactWriteRow, Cells)>> = BTreeMap::new();
        for row in &write_rows {
            let (stored, local) = (text(row, "entity")?, text(row, "id")?);
            if read.skipped.contains_key(&stored) {
                continue;
            }
            let at = format!(
                "{stored}/{local}#{}",
                row.try_get::<i64, _>("ordinal").map_err(store)?
            );
            let refs: Vec<String> = match maybe(row, "refs")? {
                None => Vec::new(),
                Some(raw) => serde_json::from_str(&raw)
                    .map_err(|_| unreadable("its references cannot be read"))?,
            };
            let stands: Vec<[String; 2]> = match maybe(row, "stands_for")? {
                None => Vec::new(),
                Some(raw) => {
                    serde_json::from_str(&raw).map_err(|_| unreadable("its mark cannot be read"))?
                }
            };
            let cells: Cells = (
                refs.into_iter().map(EntityId).collect(),
                stands
                    .into_iter()
                    .map(|[home, local]| FactAddress {
                        home: EntityId(home),
                        local: FactId(local),
                    })
                    .collect(),
            );
            let claim = Self::claim_body(read, "fact_write", &at, row, &stored, &local, &cells)?;
            let session = maybe(row, "session")?;
            let write = FactWriteRow {
                ordinal: row.try_get::<i64, _>("ordinal").map_err(store)? as u64,
                run_bot: session
                    .as_ref()
                    .and_then(|run| runs.get(run))
                    .map(|bot| EntityId(bot.clone())),
                run: session,
                bot: maybe(row, "bot")?.map(EntityId),
                written_at: maybe(row, "written_at")?
                    .and_then(|stamp| parse_stamp(Some(stamp), &stored)),
                claim,
            };
            writes
                .entry((stored, local))
                .or_default()
                .push((write, cells));
        }
        for row in &head_rows {
            let (stored, local) = (text(row, "entity")?, text(row, "id")?);
            if read.skipped.contains_key(&stored) {
                continue;
            }
            let held = writes
                .remove(&(stored.clone(), local.clone()))
                .unwrap_or_default();
            let key = (stored.clone(), local.clone());
            let cells: Cells = (
                now_refs.remove(&key).unwrap_or_default(),
                now_stands.remove(&key).unwrap_or_default(),
            );
            let at = format!("{stored}/{local}");
            let head = Self::claim_body(read, "fact", &at, row, &stored, &local, &cells)?;
            rows.facts.push(FactRow {
                home: head.home.clone(),
                local,
                head,
                writes: held.into_iter().map(|(write, _)| write).collect(),
                event_kind: maybe(row, "event_kind")?,
            });
        }
        // Writes of a claim whose head row is gone are findings, kept as claims of
        // their own so no recorded row is lost.
        for ((stored, local), held) in writes {
            read.findings.push(Finding {
                table: "fact_write",
                row: format!("{stored}/{local}"),
                what: "writes of a claim that has no fact row".to_string(),
            });
            let head = held
                .last()
                .map(|(write, _)| write.claim.clone())
                .expect("a group holds a write");
            rows.facts.push(FactRow {
                home: head.home.clone(),
                local,
                head,
                writes: held.into_iter().map(|(write, _)| write).collect(),
                event_kind: None,
            });
        }
        Ok(())
    }

    /// **A claim's columns as the import's body**, decoded by the one decoder the
    /// store reads a claim with, and every name in it as the key it stands for.
    fn claim_body(
        read: &mut Read,
        table: &'static str,
        at: &str,
        row: &sqlx::mysql::MySqlRow,
        stored: &str,
        local: &str,
        (refs, stands_for): &(Vec<EntityId>, Vec<FactAddress>),
    ) -> Result<ClaimBody, MemoryError> {
        let fact = fact_from(
            row,
            EntityId(stored.to_string()),
            FactId(local.to_string()),
            BTreeMap::new(),
            refs.clone(),
            stands_for.clone(),
        )?;
        let edge = match fact.edge {
            Some(edge) => Some((
                edge.shape.to_string(),
                read.key(table, at, "edge_object", edge.object.as_str()),
            )),
            None => None,
        };
        let derived_from = match fact.derived_from {
            Some(from) => Some(record_id(
                &read.key(table, at, "derived_from", from.home.as_str()),
                from.local.as_str(),
            )),
            None => None,
        };
        let refs = fact
            .refs
            .iter()
            .map(|named| read.key(table, at, "refs", named.as_str()))
            .collect();
        let stands_for = fact
            .stands_for
            .iter()
            .map(|named| {
                record_id(
                    &read.key(table, at, "stands_for", named.home.as_str()),
                    named.local.as_str(),
                )
            })
            .collect();
        Ok(ClaimBody {
            label: local.to_string(),
            home: read.key(table, at, "entity", stored),
            content: fact.content,
            details: fact.details,
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
            receipt: fact.receipt,
            app: fact.app_stamp,
            inserted_at: fact.inserted_at,
        })
    }

    /// **Every key write, with the things its value pointed at.**
    async fn field_write_rows(
        tx: &mut Transaction<'_, MySql>,
        read: &mut Read,
        rows: &mut Rows,
    ) -> Result<(), MemoryError> {
        let links = sqlx::query(
            "SELECT entity, `key`, ordinal, target FROM field_link \
             ORDER BY entity, `key`, ordinal, target",
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        let mut targets: HashMap<(String, String, i64), Vec<String>> = HashMap::new();
        for row in &links {
            targets
                .entry((
                    text(row, "entity")?,
                    text(row, "key")?,
                    row.try_get::<i64, _>("ordinal").map_err(store)?,
                ))
                .or_default()
                .push(text(row, "target")?);
        }
        let writes = sqlx::query(
            "SELECT entity, `key`, ordinal, value, fact_id, write_class, written_at, \
             fact_write_ordinal FROM field_write ORDER BY entity, `key`, ordinal",
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        for row in &writes {
            let (stored, key) = (text(row, "entity")?, text(row, "key")?);
            if read.skipped.contains_key(&stored) {
                targets.retain(|(entity, _, _), _| entity != &stored);
                continue;
            }
            let ordinal = row.try_get::<i64, _>("ordinal").map_err(store)?;
            let at = format!("{stored}/{key}#{ordinal}");
            let pointed = targets
                .remove(&(stored.clone(), key.clone(), ordinal))
                .unwrap_or_default()
                .iter()
                .map(|target| read.key("field_link", &at, "target", target))
                .collect();
            let home = read.key("field_write", &at, "entity", &stored);
            rows.field_writes.push(FieldWriteRow {
                entity: home.clone(),
                key: KeyId(key),
                ordinal: ordinal as u64,
                class: WriteClass::from_token(&text(row, "write_class")?),
                write: match maybe(row, "value")? {
                    Some(value) => FieldValue::Set(value),
                    None => FieldValue::Clear,
                },
                fact_home: home,
                fact_local: text(row, "fact_id")?,
                fact_write_ordinal: row
                    .try_get::<Option<i64>, _>("fact_write_ordinal")
                    .map_err(store)?
                    .map(|ordinal| ordinal as u64),
                written_at: maybe(row, "written_at")?
                    .and_then(|stamp| parse_stamp(Some(stamp), &stored)),
                targets: pointed,
            });
        }
        // A link no write names is a finding, not a dropped row.
        for ((entity, key, ordinal), _) in targets {
            read.findings.push(Finding {
                table: "field_link",
                row: format!("{entity}/{key}#{ordinal}"),
                what: "links of a key write that has no field_write row".to_string(),
            });
        }
        Ok(())
    }

    /// **Every merge, with the claims it gave a new id.** Ordinal 0 is the merge
    /// itself; each later ordinal is one claim it moved.
    async fn merge_rows(
        tx: &mut Transaction<'_, MySql>,
        read: &mut Read,
        rows: &mut Rows,
    ) -> Result<(), MemoryError> {
        let events = sqlx::query(
            "SELECT merge_no, ordinal, folded, survivor, merged_at, session, folded_id, \
             survivor_id FROM merge_event ORDER BY merge_no, ordinal",
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        let mut merges: BTreeMap<i64, MergeRow> = BTreeMap::new();
        let mut moves: BTreeMap<i64, Vec<(String, String)>> = BTreeMap::new();
        for row in &events {
            let no = row.try_get::<i64, _>("merge_no").map_err(store)?;
            let ordinal = row.try_get::<i64, _>("ordinal").map_err(store)?;
            if ordinal == 0 {
                let at = format!("merge {no}");
                let merged_at: Timestamp = text(row, "merged_at")?
                    .parse()
                    .map_err(|_| unreadable("its merge moment cannot be read"))?;
                merges.insert(
                    no,
                    MergeRow {
                        no,
                        folded: read.key("merge_event", &at, "folded", &text(row, "folded")?),
                        survivor: read.key("merge_event", &at, "survivor", &text(row, "survivor")?),
                        merged_at,
                        session: maybe(row, "session")?,
                        claims: Vec::new(),
                    },
                );
            } else if let (Some(old), Some(new)) =
                (maybe(row, "folded_id")?, maybe(row, "survivor_id")?)
            {
                moves.entry(no).or_default().push((old, new));
            } else {
                read.findings.push(Finding {
                    table: "merge_event",
                    row: format!("merge {no} claim {ordinal}"),
                    what: "a moved claim row holds no old or no new id".to_string(),
                });
            }
        }
        for (no, claims) in moves {
            match merges.get_mut(&no) {
                Some(merge) => merge.claims = claims,
                None => read.findings.push(Finding {
                    table: "merge_event",
                    row: format!("merge {no}"),
                    what: "claim rows of a merge that has no merge row".to_string(),
                }),
            }
        }
        rows.merges = merges.into_values().collect();
        Ok(())
    }

    /// **Every account of a declaration**, each key decoded by the same reader the
    /// roster's rows are. The account keeps neither the origin nor the rule of the
    /// declaration, so they read as a caller's and none.
    async fn type_write_rows(
        tx: &mut Transaction<'_, MySql>,
        rows: &mut Rows,
    ) -> Result<(), MemoryError> {
        let written = sqlx::query(
            "SELECT name, ordinal, written_at, by_session, fields FROM type_write \
             ORDER BY name, ordinal",
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        for row in &written {
            let name = text(row, "name")?;
            let cells: Vec<serde_json::Value> = serde_json::from_str(&text(row, "fields")?)
                .map_err(|_| unreadable("its declared keys cannot be read"))?;
            let mut fields = Vec::with_capacity(cells.len());
            for cell in &cells {
                let said = |key: &str| cell.get(key).and_then(serde_json::Value::as_str);
                fields.push(field_of_cells(
                    said("key").ok_or_else(|| unreadable("a declared key has no name"))?,
                    said("holds").ok_or_else(|| unreadable("a declared key holds nothing"))?,
                    said("folds").unwrap_or(""),
                    cell.get("required")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                    said("one_of"),
                )?);
            }
            rows.type_writes.push(TypeWriteRow {
                ordinal: row.try_get::<i32, _>("ordinal").map_err(store)? as u32,
                session: maybe(row, "by_session")?,
                written_at: parse_stamp(Some(text(row, "written_at")?), &name),
                declaration: DeclaredType {
                    origin: Origin::Declared,
                    ..DeclaredType::new(&name, fields)
                },
            });
        }
        Ok(())
    }
}
