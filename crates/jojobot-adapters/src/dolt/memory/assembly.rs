//! **Claims read back in batches, and pointers asked once.**
//!
//! A thing's claims are served with their fields, references and marks, every
//! pointer resolved to the handle it answers to now. Reading those per claim
//! costs a number of statements for each claim, so a store whose things hold
//! more claims boots more slowly for each thing. Here the rows beside the claims
//! are read once for the whole list, and a pointer, a handle or the declared
//! types is asked of the store once for the whole read.

use std::collections::{BTreeMap, HashMap};

use jojobot_domain::memory::types::DeclaredType;

use super::*;

/// How many claims one batched read names at most.
const CLAIMS_PER_READ: usize = 200;

/// **A claim's key**: the name it is stored under and its id there.
type ClaimKey = (EntityId, FactId);

/// What one assembling read has already asked the store, so a repeat is
/// answered from here. It lives for one transaction and is never shared.
#[derive(Default)]
pub(super) struct Asked {
    current: HashMap<EntityId, EntityId>,
    pointers: HashMap<EntityId, EntityId>,
    addresses: HashMap<FactAddress, FactAddress>,
    resolved: HashMap<EntityId, Option<(EntityId, EntityId)>>,
    declared: Option<Vec<DeclaredType>>,
}

type Fields = jojobot_domain::memory::stored::RecordFields;

/// **The rows beside a list of claims**, each read once for the whole list.
#[derive(Default)]
pub(super) struct Beside {
    fields: HashMap<ClaimKey, Fields>,
    refs: HashMap<ClaimKey, Vec<EntityId>>,
    stands_for: HashMap<ClaimKey, Vec<FactAddress>>,
    /// The bot of a claim's first write, and the bot behind that write's run,
    /// for a claim drawing a connection edge. A claim with no write is absent.
    first_writers: HashMap<ClaimKey, (Option<String>, Option<String>)>,
}

impl Beside {
    pub(super) fn fields(&mut self, key: &ClaimKey) -> Fields {
        self.fields.remove(key).unwrap_or_default()
    }

    pub(super) fn refs(&mut self, key: &ClaimKey) -> Vec<EntityId> {
        self.refs.remove(key).unwrap_or_default()
    }

    pub(super) fn stands_for(&mut self, key: &ClaimKey) -> Vec<FactAddress> {
        self.stands_for.remove(key).unwrap_or_default()
    }

    pub(super) fn first_writer(&self, key: &ClaimKey) -> Option<&(Option<String>, Option<String>)> {
        self.first_writers.get(key)
    }
}

/// `?, ?, ?` for a list of this many values.
fn placeholders(count: usize) -> String {
    vec!["?"; count].join(", ")
}

impl DoltMemory {
    /// **A claim's field rows read as writes and handed to the domain's fold**,
    /// which decides the fields, the keys set on the thing and the keys that
    /// reach it. This decodes the SQL rows and decides nothing. The rows are one
    /// claim's, in the order of `key, ordinal`; the single read and the batch both
    /// come here.
    pub(super) fn read_fields(rows: &[&sqlx::mysql::MySqlRow]) -> Result<Fields, MemoryError> {
        let writes = rows
            .iter()
            .map(|row| {
                let key: String = row.try_get("key").map_err(store)?;
                let value: Option<String> = row.try_get("value").map_err(store)?;
                let class = if value.is_some() {
                    let token: String = row.try_get("write_class").map_err(store)?;
                    jojobot_domain::memory::WriteClass::from_token(&token)
                } else {
                    jojobot_domain::memory::WriteClass::Legacy
                };
                Ok((key, value, class))
            })
            .collect::<Result<Vec<_>, MemoryError>>()?;
        Ok(jojobot_domain::memory::stored::fold_field_writes(writes))
    }

    /// **Read what sits beside these claim rows**: fields, references, marks and,
    /// for a claim drawing a connection edge, the bot of its first write. One
    /// statement of each kind for every `CLAIMS_PER_READ` claims of a name.
    pub(super) async fn read_beside(
        tx: &mut Transaction<'_, MySql>,
        rows: &[sqlx::mysql::MySqlRow],
    ) -> Result<Beside, MemoryError> {
        let mut by_name: BTreeMap<EntityId, Vec<(FactId, bool)>> = BTreeMap::new();
        for row in rows {
            let name = EntityId(row.try_get::<String, _>("entity").map_err(store)?);
            let id = FactId(row.try_get::<String, _>("id").map_err(store)?);
            let connection = row
                .try_get::<Option<String>, _>("edge_shape")
                .map_err(store)?
                .and_then(|shape| EdgeShape::from_token(&shape))
                == Some(EdgeShape::Connection);
            by_name.entry(name).or_default().push((id, connection));
        }
        let mut beside = Beside::default();
        for (name, claims) in &by_name {
            for chunk in claims.chunks(CLAIMS_PER_READ) {
                let ids: Vec<&str> = chunk.iter().map(|(id, _)| id.as_str()).collect();
                let marks = placeholders(ids.len());
                let written = sqlx::query(AssertSqlSafe(format!(
                    "SELECT fact_id, `key`, value, write_class FROM field_write
                     WHERE entity = ? AND fact_id IN ({marks}) ORDER BY fact_id, `key`, ordinal"
                )))
                .bind(name.as_str());
                let written = ids
                    .iter()
                    .fold(written, |query, id| query.bind(*id))
                    .fetch_all(&mut **tx)
                    .await
                    .map_err(store)?;
                let mut grouped: BTreeMap<String, Vec<&sqlx::mysql::MySqlRow>> = BTreeMap::new();
                for row in &written {
                    let id: String = row.try_get("fact_id").map_err(store)?;
                    grouped.entry(id).or_default().push(row);
                }
                for (id, group) in grouped {
                    beside
                        .fields
                        .insert((name.clone(), FactId(id)), Self::read_fields(&group)?);
                }

                let refs = sqlx::query(AssertSqlSafe(format!(
                    "SELECT fact_id, entity FROM fact_event_ref
                     WHERE fact_home = ? AND fact_id IN ({marks}) ORDER BY fact_id, ordinal"
                )))
                .bind(name.as_str());
                for r in ids
                    .iter()
                    .fold(refs, |query, id| query.bind(*id))
                    .fetch_all(&mut **tx)
                    .await
                    .map_err(store)?
                {
                    let id = FactId(r.get::<String, _>("fact_id"));
                    beside
                        .refs
                        .entry((name.clone(), id))
                        .or_default()
                        .push(EntityId(r.get::<String, _>("entity")));
                }

                let marked = sqlx::query(AssertSqlSafe(format!(
                    "SELECT fact_id, source_home, source_id FROM fact_stands_for
                     WHERE fact_home = ? AND fact_id IN ({marks}) ORDER BY fact_id, ordinal"
                )))
                .bind(name.as_str());
                for r in ids
                    .iter()
                    .fold(marked, |query, id| query.bind(*id))
                    .fetch_all(&mut **tx)
                    .await
                    .map_err(store)?
                {
                    let id = FactId(r.get::<String, _>("fact_id"));
                    beside
                        .stands_for
                        .entry((name.clone(), id))
                        .or_default()
                        .push(FactAddress::new(
                            EntityId(r.get::<String, _>("source_home")),
                            FactId(r.get::<String, _>("source_id")),
                        ));
                }

                let connected: Vec<&str> = chunk
                    .iter()
                    .filter(|(_, connection)| *connection)
                    .map(|(id, _)| id.as_str())
                    .collect();
                if connected.is_empty() {
                    continue;
                }
                let marks = placeholders(connected.len());
                let first = sqlx::query(AssertSqlSafe(format!(
                    "SELECT w.fact_id, w.bot, s.bot AS run_bot
                     FROM fact_write w
                     JOIN (SELECT fact_id, MIN(ordinal) AS first FROM fact_write
                           WHERE entity = ? AND fact_id IN ({marks}) GROUP BY fact_id) f
                       ON f.fact_id = w.fact_id AND f.first = w.ordinal
                     LEFT JOIN session s ON s.sid = w.session
                     WHERE w.entity = ?"
                )))
                .bind(name.as_str());
                let first = connected
                    .iter()
                    .fold(first, |query, id| query.bind(*id))
                    .bind(name.as_str());
                for r in first.fetch_all(&mut **tx).await.map_err(store)? {
                    let id = FactId(r.get::<String, _>("fact_id"));
                    beside.first_writers.insert(
                        (name.clone(), id),
                        (
                            r.get::<Option<String>, _>("bot"),
                            r.get::<Option<String>, _>("run_bot"),
                        ),
                    );
                }
            }
        }
        Ok(beside)
    }

    /// **Whether another bot wrote a claim**, from the first write read in
    /// [`read_beside`](Self::read_beside). The same answer as
    /// [`another_bots_claim`](Self::another_bots_claim) gives from the store.
    pub(super) async fn another_bots_claim_asked(
        &self,
        tx: &mut Transaction<'_, MySql>,
        asked: &mut Asked,
        first: Option<&(Option<String>, Option<String>)>,
        stored_home: &EntityId,
    ) -> Result<bool, MemoryError> {
        let Some((recorded, of_the_run)) = first else {
            return Ok(false);
        };
        let Some(bot) = jojobot_domain::memory::authorship::first_writer(
            recorded.clone().map(EntityId),
            || of_the_run.clone().map(EntityId),
        ) else {
            return Ok(false);
        };
        let handle = self.current_handle_asked(tx, asked, stored_home).await?;
        let names_home = self.writer_names(tx, &bot, stored_home).await?;
        Ok(jojobot_domain::memory::authorship::another_bots_claim(
            &handle,
            Some(&bot),
            |_| names_home,
        ))
    }

    /// [`current_handle`](Self::current_handle), asked once per name.
    pub(super) async fn current_handle_asked(
        &self,
        tx: &mut Transaction<'_, MySql>,
        asked: &mut Asked,
        stored: &EntityId,
    ) -> Result<EntityId, MemoryError> {
        if let Some(held) = asked.current.get(stored) {
            return Ok(held.clone());
        }
        let handle = self.current_handle(tx, stored).await?;
        asked.current.insert(stored.clone(), handle.clone());
        Ok(handle)
    }

    /// [`pointer_handle`](Self::pointer_handle), asked once per name.
    pub(super) async fn pointer_handle_asked(
        &self,
        tx: &mut Transaction<'_, MySql>,
        asked: &mut Asked,
        stored: &EntityId,
    ) -> Result<EntityId, MemoryError> {
        if let Some(held) = asked.pointers.get(stored) {
            return Ok(held.clone());
        }
        let key = self.forward_key(tx, stored).await?;
        let handle = self.current_handle_asked(tx, asked, &key).await?;
        asked.pointers.insert(stored.clone(), handle.clone());
        Ok(handle)
    }

    /// [`pointer_address`](Self::pointer_address), asked once per address.
    pub(super) async fn pointer_address_asked(
        &self,
        tx: &mut Transaction<'_, MySql>,
        asked: &mut Asked,
        stored: &FactAddress,
    ) -> Result<FactAddress, MemoryError> {
        if let Some(held) = asked.addresses.get(stored) {
            return Ok(held.clone());
        }
        let at = Self::forward_address(tx, stored).await?;
        let home = self.pointer_handle_asked(tx, asked, &at.home).await?;
        let served = FactAddress::new(home, at.local);
        asked.addresses.insert(stored.clone(), served.clone());
        Ok(served)
    }

    /// [`resolve`](Self::resolve), asked once per handle.
    async fn resolve_asked(
        &self,
        tx: &mut Transaction<'_, MySql>,
        asked: &mut Asked,
        id: &EntityId,
    ) -> Result<Option<(EntityId, EntityId)>, MemoryError> {
        if let Some(held) = asked.resolved.get(id) {
            return Ok(held.clone());
        }
        let found = self.resolve(tx, id).await?;
        asked.resolved.insert(id.clone(), found.clone());
        Ok(found)
    }

    /// **Every reference-typed field value, composed from the permanent id it is
    /// stored as to the handle it answers to today** — the mirror of
    /// [`Self::lower_reference_fields`], run once, right before a fields map
    /// reaches whoever asked for it. The declared types are read on the first
    /// call of a read and held in `asked`.
    pub(super) async fn compose_reference_fields_asked(
        &self,
        tx: &mut Transaction<'_, MySql>,
        asked: &mut Asked,
        fields: &mut BTreeMap<String, String>,
    ) -> Result<(), MemoryError> {
        if jojobot_domain::memory::link_rule() == jojobot_domain::memory::LinkRule::On {
            return self.compose_marked_asked(tx, asked, fields).await;
        }
        if asked.declared.is_none() {
            asked.declared = Some(Self::types_in(tx).await?);
        }
        let declared = asked.declared.clone().unwrap_or_default();
        let referenced = jojobot_domain::memory::reference_field_values(fields, &declared);
        for (key, items) in &referenced {
            let mut composed = Vec::with_capacity(items.len());
            for item in items {
                // A key declared a reference AFTER a handle was stored under it
                // as an undeclared one holds that value behind the mark.
                let stored = jojobot_domain::memory::stored_handle_id(item).unwrap_or(item);
                composed.push(
                    self.pointer_handle_asked(tx, asked, &EntityId(stored.to_string()))
                        .await?,
                );
            }
            fields.insert(key.clone(), Self::serve_list(composed));
        }
        // **Every other value, served as the handle it answers to now.** A value
        // stored behind the mark is the thing's id. One stored as plain handle
        // text, from before ids were kept, is resolved through the thing's
        // rename history, so it keeps pointing at the thing it named and is
        // lowered the next time the key is written.
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
                        composed.push(
                            self.pointer_handle_asked(tx, asked, &EntityId(id.to_string()))
                                .await?,
                        );
                    }
                }
                fields.insert(key, Self::serve_list(composed));
            } else if items
                .iter()
                .any(|item| jojobot_domain::memory::stored_handle_id(item).is_some())
            {
                // **A list that mixes words with marked items**, which the rule
                // that is on marks. Each marked item is served as the handle it
                // answers to and each word as written, so a rollback reads what
                // it read before the marks and no mark reaches a caller.
                let mut one = BTreeMap::from([(key.clone(), value.clone())]);
                self.compose_marked_asked(tx, asked, &mut one).await?;
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
                    composed.push(match self.resolve_asked(tx, asked, &id).await? {
                        Some((_, current)) => current.to_string(),
                        None => id.to_string(),
                    });
                }
                fields.insert(key, composed.join(", "));
            }
        }
        Ok(())
    }

    /// **What each key's stored value links to, by its marks**, as the handles the
    /// marked things answer to now: the answer a reader asks of a record and does not
    /// work out again from the served words. Empty unless the link rule is on,
    /// because the rule before rule 646 draws links from the words.
    pub(super) async fn marked_links_asked(
        &self,
        tx: &mut Transaction<'_, MySql>,
        asked: &mut Asked,
        stored: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, Vec<EntityId>>, MemoryError> {
        let mut links = BTreeMap::new();
        if jojobot_domain::memory::link_rule() != jojobot_domain::memory::LinkRule::On {
            return Ok(links);
        }
        let targets =
            jojobot_domain::memory::marked_field_links(stored, |id| Some(EntityId(id.to_string())))
                .expect("a stored id needs no resolution");
        for (key, targets) in targets {
            let mut named: Vec<EntityId> = Vec::new();
            for id in targets {
                let handle = self.pointer_handle_asked(tx, asked, &id).await?;
                if !named.contains(&handle) {
                    named.push(handle);
                }
            }
            if !named.is_empty() {
                links.insert(key, named);
            }
        }
        Ok(links)
    }

    /// **Every handle a field value names, composed from the permanent id it is
    /// stored as to the handle it answers to today** — the mirror of
    /// [`Self::lower_reference_fields`], run once, right before a fields map
    /// reaches whoever asked for it.
    ///
    /// **Each item is served by what it wears.** One behind the mark is the
    /// thing's id. One stored as plain handle text, from before ids were kept, is
    /// resolved through the thing's rename history, so it keeps pointing at the
    /// thing it named and is lowered the next time the key is written. Loose text
    /// is served as written.
    async fn compose_marked_asked(
        &self,
        tx: &mut Transaction<'_, MySql>,
        asked: &mut Asked,
        fields: &mut BTreeMap<String, String>,
    ) -> Result<(), MemoryError> {
        for (key, value) in fields.clone() {
            let items: Vec<&str> = value.split(',').map(str::trim).collect();
            if !items
                .iter()
                .any(|item| jojobot_domain::memory::stored_handle_id(item).is_some())
            {
                continue;
            }
            // Each item with the thing it names, loose text naming none. Only a mark
            // names one: text that wears none is served as written, even when it
            // spells a handle.
            let mut composed: Vec<(Option<EntityId>, String)> = Vec::with_capacity(items.len());
            for item in items {
                let id = match jojobot_domain::memory::stored_handle_id(item) {
                    Some(stored) => Some(
                        self.pointer_handle_asked(tx, asked, &EntityId(stored.to_string()))
                            .await?,
                    ),
                    None => None,
                };
                let text = id
                    .as_ref()
                    .map_or_else(|| item.to_string(), |id| id.to_string());
                composed.push((id, text));
            }
            fields.insert(key, jojobot_domain::memory::list::serve_values(composed));
        }
        Ok(())
    }

    /// **Read what a write kept of the claim's references and mark, onto the
    /// write**, served under the handles the keys answer to today and, for a
    /// thing a merge folded away, under the survivor and the id the merge gave
    /// the marked claim. The stored snapshot is not rewritten.
    ///
    /// A write kept before the columns existed holds NULL in both, and reads as
    /// `None` rather than as a write that pointed at or stood for nothing. A
    /// cell that is not the JSON this store wrote is damage and is named, not
    /// read as empty.
    pub(super) async fn read_kept_marks(
        &self,
        tx: &mut Transaction<'_, MySql>,
        row: &sqlx::mysql::MySqlRow,
        write: &mut ClaimWrite,
        asked: &mut Asked,
    ) -> Result<(), MemoryError> {
        let damaged = |what: &str| {
            unreadable(&format!(
                "its kept {what} cannot be read as the list this store wrote"
            ))
        };
        if let Some(cell) = row
            .try_get::<Option<String>, _>("kept_refs")
            .map_err(store)?
        {
            let keys: Vec<String> = serde_json::from_str(&cell).map_err(|_| damaged("refs"))?;
            let mut refs = Vec::with_capacity(keys.len());
            for key in keys {
                refs.push(self.pointer_handle_asked(tx, asked, &EntityId(key)).await?);
            }
            write.refs = Some(refs);
        } else {
            write.refs = None;
        }
        if let Some(cell) = row
            .try_get::<Option<String>, _>("kept_stands_for")
            .map_err(store)?
        {
            let pairs: Vec<[String; 2]> =
                serde_json::from_str(&cell).map_err(|_| damaged("mark"))?;
            let mut marked = Vec::with_capacity(pairs.len());
            for [home, local] in pairs {
                let at = self
                    .pointer_address_asked(
                        tx,
                        asked,
                        &FactAddress::new(EntityId(home), FactId(local)),
                    )
                    .await?;
                marked.push(at);
            }
            write.stands_for = Some(marked);
        } else {
            write.stands_for = None;
        }
        Ok(())
    }
}
