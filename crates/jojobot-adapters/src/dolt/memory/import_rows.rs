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
    EntityRow, EntityWriteRow, FactRow, FactWriteRow, FieldWriteRow, FormerHandleRow, MergeRow,
    MetadataRow, Rows, TypeWriteRow, record_id,
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
}

impl Keys {
    fn of(&self, stored: &str) -> Option<String> {
        if self.badges.contains(stored) {
            return Some(stored.to_string());
        }
        if let Some(key) = self.by_id.get(stored) {
            return Some(key.clone());
        }
        match self.former.get(stored).map(Vec::as_slice) {
            Some([only]) => Some(only.clone()),
            _ => None,
        }
    }
}

/// What a read carries while it decodes: the keys and the findings so far.
struct Read {
    keys: Keys,
    findings: Vec<Finding>,
}

impl Read {
    /// **The key a stored name stands for**, or its stored spelling with a
    /// finding when it stands for none.
    fn key(&mut self, table: &'static str, row: &str, column: &str, stored: &str) -> ObjectId {
        match self.keys.of(stored) {
            Some(key) => ObjectId(key),
            None => {
                self.findings.push(Finding {
                    table,
                    row: row.to_string(),
                    what: format!("{column} names {stored:?}, which is no key this store holds"),
                });
                ObjectId(stored.to_string())
            }
        }
    }
}

fn text(row: &sqlx::mysql::MySqlRow, column: &str) -> Result<String, MemoryError> {
    row.try_get::<String, _>(column).map_err(store)
}

fn maybe(row: &sqlx::mysql::MySqlRow, column: &str) -> Result<Option<String>, MemoryError> {
    row.try_get::<Option<String>, _>(column).map_err(store)
}

impl DoltMemory {
    /// **Every row the claims import names, read from this store.**
    pub async fn import_rows(&self) -> Result<ImportRead, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let read = Self::read_import_rows(&mut tx).await?;
        commit_native(tx).await.map_err(store)?;
        Ok(read)
    }

    async fn read_import_rows(tx: &mut Transaction<'_, MySql>) -> Result<ImportRead, MemoryError> {
        let entity_rows = sqlx::query(
            "SELECT id, kind, name, source, crm, parent, boot, prose, badge, merged_into, \
             archived_reason, archived_at FROM entity ORDER BY id",
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        let former_rows = sqlx::query(
            "SELECT former_handle, badge, ordinal FROM entity_former_handle \
             ORDER BY former_handle, badge, ordinal",
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        let mut keys = Keys {
            by_id: HashMap::with_capacity(entity_rows.len()),
            badges: HashSet::new(),
            former: HashMap::new(),
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
        let mut read = Read {
            keys,
            findings: Vec::new(),
        };
        let mut rows = Rows::default();

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
        let written = Self::entity_write_rows(tx, &mut read).await?;
        for row in &entity_rows {
            let handle = text(row, "id")?;
            let key = read.keys.by_id[&handle].clone();
            let parent =
                maybe(row, "parent")?.map(|stored| read.key("entity", &handle, "parent", &stored));
            let merged_into = maybe(row, "merged_into")?
                .map(|stored| read.key("entity", &handle, "merged_into", &stored));
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
    /// **Every `entity_write` row, filed to the thing it is a version of.** The row
    /// names a badge, and that badge's thing owns it; a row from before the
    /// snapshot names none, so the handle it was filed under decides: a thing that
    /// left the handle owns it, else the thing that wears it. A handle several
    /// things left, or one nobody holds, is a finding.
    async fn entity_write_rows(
        tx: &mut Transaction<'_, MySql>,
        read: &mut Read,
    ) -> Result<Written, MemoryError> {
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
            let owner = match maybe(row, "badge")? {
                Some(badge) => read.key("entity_write", &at, "badge", &badge),
                None => match read.keys.former.get(&filed_under).map(Vec::as_slice) {
                    Some([only]) => ObjectId(only.clone()),
                    Some(several) => {
                        read.findings.push(Finding {
                            table: "entity_write",
                            row: at.clone(),
                            what: format!(
                                "handle {filed_under:?} was left by {} things; the row is filed under the first",
                                several.len()
                            ),
                        });
                        ObjectId(several[0].clone())
                    }
                    None => read.key("entity_write", &at, "entity", &filed_under),
                },
            };
            let moment = parse_stamp(Some(text(row, "written_at")?), &filed_under);
            let snapshot = match maybe(row, "name")? {
                // A row an older build appended holds no name, and no other column.
                None => None,
                Some(name) => {
                    let aliases: Vec<String> = match maybe(row, "aliases")? {
                        None => Vec::new(),
                        Some(raw) => serde_json::from_str(&raw).map_err(|e| {
                            MemoryError::Store(format!("a stored alias set is not readable: {e}"))
                        })?,
                    };
                    let parent = maybe(row, "parent")?
                        .map(|stored| read.key("entity_write", &at, "parent", &stored));
                    let merged_into = maybe(row, "merged_into")?
                        .map(|stored| read.key("entity_write", &at, "merged_into", &stored));
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
                filed_under,
                ordinal,
                written_at: moment,
                session: maybe(row, "session")?,
                snapshot,
            });
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
        let runs: HashMap<String, String> = sqlx::query("SELECT id, bot FROM session")
            .fetch_all(&mut **tx)
            .await
            .map_err(store)?
            .iter()
            .map(|row| Ok((text(row, "id")?, text(row, "bot")?)))
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

        type Cells = (Vec<EntityId>, Vec<FactAddress>);
        let mut writes: BTreeMap<(String, String), Vec<(FactWriteRow, Cells)>> = BTreeMap::new();
        for row in &write_rows {
            let (stored, local) = (text(row, "entity")?, text(row, "id")?);
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
            let held = writes
                .remove(&(stored.clone(), local.clone()))
                .unwrap_or_default();
            let cells = held
                .last()
                .map(|(_, cells)| cells.clone())
                .unwrap_or_default();
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
