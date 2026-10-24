//! The link decision of rule 646 in the Dolt store: the write, the boot pass and the
//! repoint of a merge. The store runs it when the process switch is on; until then
//! it only computes what it would do, for the log.

use super::*;

/// **The name the marking of every link in a stored field value is done under**
/// in `store_pass`, when the link rule is on. The log-only boot never marks it.
const FIELD_MARKS_PASS: &str = "field value links marked";

impl DoltMemory {
    /// **Every handle a field value names, lowered from whatever the caller
    /// wrote to the permanent id the store keeps**, behind the mark that tells a
    /// stored id from a word. The mark is the decision that the item is a link,
    /// taken here once and whatever the key (rule 646). Each item was already
    /// checked to exist by the caller of this function, so `resolve` cannot miss
    /// on one; loose items beside them are kept as written.
    pub(super) async fn lower_marked_fields(
        &self,
        tx: &mut Transaction<'_, MySql>,
        fields: &mut std::collections::BTreeMap<String, String>,
    ) -> Result<(), MemoryError> {
        let declared = Self::types_in(tx).await?;
        for (key, value) in fields.clone() {
            // A key a type declares a reference also takes a bare id of an existing
            // thing, as a write has always read it; that is read here, once, and
            // the mark it is given is what stays.
            let reference = declared
                .iter()
                .filter_map(|d| d.field(&key))
                .any(|f| f.holds == jojobot_domain::memory::types::ValueType::Reference);
            if !reference && jojobot_domain::memory::at_write::handles_in_value(&value).is_empty() {
                continue;
            }
            let mut lowered = Vec::new();
            let mut changed = false;
            for item in value.split(',').map(str::trim) {
                let id = EntityId(item.to_string());
                if jojobot_domain::memory::validate_subject(&id).is_err() {
                    if reference && self.resolve_by_badge(tx, item).await?.is_some() {
                        lowered.push(jojobot_domain::memory::marked_stored_handle(item));
                        changed = true;
                    } else {
                        lowered.push(item.to_string());
                    }
                    continue;
                }
                let stored = self.resolve(tx, &id).await?.map_or(id, |(key, _)| key);
                lowered.push(jojobot_domain::memory::marked_stored_handle(
                    stored.as_str(),
                ));
                changed = true;
            }
            if changed {
                fields.insert(key, lowered.join(", "));
            }
        }
        Ok(())
    }

    /// **Put the mark on every item of a stored field value that is a link, and
    /// bring the link table into agreement with the field writes — in one pass.**
    ///
    /// Whether a value links is decided once, when it is written, and stored as
    /// the permanent id of the thing behind the mark (rule 646). A value stored
    /// before that held a link in one of two other forms: plain handle text, which
    /// names whatever answers to that handle now unless a rename has moved it, and
    /// under a key a type declares a reference, a bare id. This reads every
    /// comma item of every value through the entity list, the rename history and
    /// the records the build supplies. **An item that is a handle naming exactly
    /// one thing is stored as that thing's id behind the mark, and so is a bare id
    /// under a declared reference key. An item that is a handle naming none, or
    /// more than one, stays as written** and is listed in the report, one line
    /// each: the pass does not guess. Loose text beside a link is never touched.
    ///
    /// **Then the link table is made to equal what the writes imply**: a row for
    /// each marked item, from the mark alone. A row the writes imply and the table
    /// lacks is added, and a row the table holds that no write implies is removed.
    /// The table is derived, so this is also the repair for one that was lost or
    /// filled by a build that did not keep it. No declaration is read for it.
    ///
    /// **The marking runs once on a store.** It is marked finished after it has
    /// examined every write, and a later run reads none of them for it: the items
    /// left as text are listed in the run that left them and never again, because
    /// a handle that named nothing when it was written must not be linked to
    /// whatever takes the name later. **The link table is still checked on every
    /// run**, cheaply and inside the store; the reconcile above runs again only
    /// when the check finds a disagreement, and says so in the report.
    pub async fn migrate_field_links_marked(&self) -> Result<FieldLinkReport, MemoryError> {
        use std::collections::BTreeSet;

        let mut report = FieldLinkReport::default();
        let marked_before = crate::dolt::finished_passes(&self.pool)
            .await
            .contains(FIELD_MARKS_PASS);
        if marked_before {
            if !self.link_table_disagrees_marked().await? {
                report.settled = true;
                return Ok(report);
            }
            report.disagreed = true;
        }
        let mut tx = self.pool.begin().await.map_err(store)?;
        let mut directory = self.directory_in(&mut tx).await?;
        directory.keep_older = crate::dolt::finished_passes(&self.pool)
            .await
            .contains(FIELD_LOWERING_PASS);

        let mut writes: Vec<(String, String, i64, String)> = sqlx::query_as(
            "SELECT entity, `key`, ordinal, value FROM field_write WHERE value IS NOT NULL
             ORDER BY entity, `key`, ordinal",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;

        let mut marked: Vec<(usize, String)> = Vec::new();
        for (at, (entity, key, _, value)) in writes.iter().enumerate() {
            if marked_before {
                break;
            }
            let marking = directory.mark(key, value);
            for (item, ambiguous) in &marking.left {
                let holder = directory.handle_of.get(entity.as_str()).unwrap_or(entity);
                report.left_as_text.push(format!(
                    "{holder} key '{key}' item '{item}': {}",
                    match ambiguous {
                        true => "the handle has named more than one thing",
                        false => "the handle names nothing this store has held",
                    }
                ));
            }
            for item in &marking.older {
                let holder = directory.handle_of.get(entity.as_str()).unwrap_or(entity);
                report.left_as_text.push(format!(
                    "{holder} key '{key}' item '{item}': an older pass left it as text, and a \
                     thing wears the spelling now"
                ));
            }
            if marking.changed() {
                marked.push((at, marking.value));
            }
        }
        for (at, lowered) in marked {
            let (entity, key, ordinal, value) = &mut writes[at];
            sqlx::query(
                "UPDATE field_write SET value = ? WHERE entity = ? AND `key` = ? AND ordinal = ?",
            )
            .bind(&lowered)
            .bind(&*entity)
            .bind(&*key)
            .bind(*ordinal)
            .execute(&mut *tx)
            .await
            .map_err(store)?;
            *value = lowered;
            report.lowered += 1;
        }

        let mut expected: BTreeSet<(String, String, i64, String)> = BTreeSet::new();
        for (entity, key, ordinal, value) in &writes {
            for target in jojobot_domain::memory::at_write::field_link_targets(value) {
                expected.insert((entity.clone(), key.clone(), *ordinal, target));
            }
        }
        let held: BTreeSet<(String, String, i64, String)> =
            sqlx::query_as::<_, (String, String, i64, String)>(
                "SELECT entity, `key`, ordinal, target FROM field_link",
            )
            .fetch_all(&mut *tx)
            .await
            .map_err(store)?
            .into_iter()
            .collect();
        for (entity, key, ordinal, target) in held.difference(&expected) {
            sqlx::query(
                "DELETE FROM field_link
                 WHERE entity = ? AND `key` = ? AND ordinal = ? AND target = ?",
            )
            .bind(entity)
            .bind(key)
            .bind(ordinal)
            .bind(target)
            .execute(&mut *tx)
            .await
            .map_err(store)?;
            report.unlinked += 1;
        }
        for (entity, key, ordinal, target) in expected.difference(&held) {
            sqlx::query(
                "INSERT INTO field_link (entity, `key`, ordinal, target) VALUES (?, ?, ?, ?)",
            )
            .bind(entity)
            .bind(key)
            .bind(ordinal)
            .bind(target)
            .execute(&mut *tx)
            .await
            .map_err(store)?;
            report.linked += 1;
        }
        tx.commit().await.map_err(store)?;
        if !marked_before {
            crate::dolt::finish_pass(&self.pool, FIELD_MARKS_PASS).await;
        }
        Ok(report)
    }

    /// **Whether the link table disagrees with the field writes**, by membership:
    /// the set of `(write, target)` the marks imply against the set of rows the
    /// table holds. A write that lost one of its rows, a row no mark implies and a
    /// row whose write is gone each make the sets differ, so a half-filled set is
    /// not called settled because each write still has some row.
    async fn link_table_disagrees_marked(&self) -> Result<bool, MemoryError> {
        use std::collections::BTreeSet;
        let mark = jojobot_domain::memory::marked_stored_handle("");
        let writes: Vec<(String, String, i64, String)> = sqlx::query_as(
            "SELECT entity, `key`, ordinal, value FROM field_write WHERE INSTR(value, ?) > 0",
        )
        .bind(&mark)
        .fetch_all(&self.pool)
        .await
        .map_err(store)?;
        let mut expected: BTreeSet<(String, String, i64, String)> = BTreeSet::new();
        for (entity, key, ordinal, value) in &writes {
            for target in jojobot_domain::memory::at_write::field_link_targets(value) {
                expected.insert((entity.clone(), key.clone(), *ordinal, target));
            }
        }
        let held: BTreeSet<(String, String, i64, String)> =
            sqlx::query_as::<_, (String, String, i64, String)>(
                "SELECT entity, `key`, ordinal, target FROM field_link",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(store)?
            .into_iter()
            .collect();
        Ok(held != expected)
    }

    /// **Every field write whose value links to `from` is rewritten to link to
    /// `to`**, and its link rows with it (rule 374). A field naming a thing is
    /// stored as the thing's permanent id, and a folded thing only forwards, so a
    /// link left on it points at a husk. A list that already named `to` keeps one
    /// item for it. Answers how many writes it rewrote.
    ///
    /// Runs inside the caller's transaction, over the rows that transaction can
    /// see, so a fold's own moved writes are swept with the rest.
    pub(super) async fn repoint_field_links(
        tx: &mut Transaction<'_, MySql>,
        declared: &[DeclaredType],
        from: &str,
        to: &str,
    ) -> Result<usize, MemoryError> {
        Self::repoint_links_by(tx, declared, from, to, false).await
    }

    /// **[`repoint_field_links`] by the rule before rule 646**, whatever the switch
    /// says: the repair of a bare name reads the rows that rule wrote, so it
    /// rewrites them by it and the marking pass then finds the id it left.
    pub(super) async fn repoint_field_links_legacy(
        tx: &mut Transaction<'_, MySql>,
        declared: &[DeclaredType],
        from: &str,
        to: &str,
    ) -> Result<usize, MemoryError> {
        Self::repoint_links_by(tx, declared, from, to, true).await
    }

    async fn repoint_links_by(
        tx: &mut Transaction<'_, MySql>,
        declared: &[DeclaredType],
        from: &str,
        to: &str,
        legacy: bool,
    ) -> Result<usize, MemoryError> {
        let held: Vec<(String, String, i64)> = sqlx::query_as(
            "SELECT DISTINCT entity, `key`, ordinal FROM field_link WHERE target = ?",
        )
        .bind(from)
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        let mut repointed = 0;
        for (entity, key, ordinal) in held {
            let value: Option<String> = sqlx::query_scalar(
                "SELECT value FROM field_write WHERE entity = ? AND `key` = ? AND ordinal = ?",
            )
            .bind(&entity)
            .bind(&key)
            .bind(ordinal)
            .fetch_optional(&mut **tx)
            .await
            .map_err(store)?
            .flatten();
            let Some(value) = value else { continue };
            let repointed_value = match legacy {
                true => jojobot_domain::memory::legacy_repoint_field_value(
                    &key, &value, declared, from, to,
                ),
                false => {
                    jojobot_domain::memory::repoint_field_value(&key, &value, declared, from, to)
                }
            };
            let Some(now) = repointed_value else {
                continue;
            };
            sqlx::query(
                "UPDATE field_write SET value = ? WHERE entity = ? AND `key` = ? AND ordinal = ?",
            )
            .bind(&now)
            .bind(&entity)
            .bind(&key)
            .bind(ordinal)
            .execute(&mut **tx)
            .await
            .map_err(store)?;
            sqlx::query("DELETE FROM field_link WHERE entity = ? AND `key` = ? AND ordinal = ?")
                .bind(&entity)
                .bind(&key)
                .bind(ordinal)
                .execute(&mut **tx)
                .await
                .map_err(store)?;
            let targets = match legacy {
                true => jojobot_domain::memory::legacy_field_link_targets(&key, &now, declared),
                false => jojobot_domain::memory::field_link_targets(&key, &now, declared),
            };
            for target in targets {
                sqlx::query(
                    "INSERT INTO field_link (entity, `key`, ordinal, target) VALUES (?, ?, ?, ?)",
                )
                .bind(&entity)
                .bind(&key)
                .bind(ordinal)
                .bind(&target)
                .execute(&mut **tx)
                .await
                .map_err(store)?;
            }
            repointed += 1;
        }
        Ok(repointed)
    }
}

/// **What the store holds to decide which thing a stored item names**: the ids
/// each handle has answered to, the handle of each id, every id, and the keys any
/// type declares a reference.
pub(super) struct Directory {
    names: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    handle_of: std::collections::BTreeMap<String, String>,
    ids: std::collections::BTreeSet<String>,
    reference_keys: std::collections::BTreeSet<String>,
    /// Whether an older pass already decided the plain handle text of a value of
    /// only handles. Then a handle it left as text stays text: a thing that wears
    /// the spelling today is not evidence of what the value meant.
    pub(super) keep_older: bool,
}

/// **What the marking decides for one stored value**, item by item.
pub(super) struct Marking {
    /// The value with every item the rule would mark, marked.
    pub(super) value: String,
    /// Handles that name exactly one thing and are not yet marked.
    pub(super) handles: usize,
    /// Bare ids under a declared reference key.
    pub(super) bare: usize,
    /// Whether the value has an item that is no handle and no mark.
    pub(super) mixed: bool,
    /// Handles left as text, each with whether it names more than one thing.
    pub(super) left: Vec<(String, bool)>,
    /// Handles of such a value that an older pass left as text and that are kept so.
    pub(super) older: Vec<String>,
    /// Unmarked handles of a value that is only handles and marks, which name one
    /// thing today: what an older pass left as text because it named none or
    /// two then.
    pub(super) owned_now: usize,
}

impl Marking {
    pub(super) fn changed(&self) -> bool {
        self.handles + self.bare > 0
    }
}

impl Directory {
    /// The classification of one value; it reads nothing and writes nothing.
    pub(super) fn mark(&self, key: &str, value: &str) -> Marking {
        let mut marking = Marking {
            value: String::new(),
            handles: 0,
            bare: 0,
            mixed: false,
            left: Vec::new(),
            older: Vec::new(),
            owned_now: 0,
        };
        let items: Vec<&str> = value.split(',').map(str::trim).collect();
        let only_handles = items.iter().all(|item| {
            jojobot_domain::memory::stored_handle_id(item).is_some()
                || jojobot_domain::memory::validate_subject(&EntityId((*item).to_string())).is_ok()
        });
        let reference = self.reference_keys.contains(key);
        let marked: Vec<String> = items
            .iter()
            .map(|item| {
                if jojobot_domain::memory::stored_handle_id(item).is_some() {
                    return (*item).to_string();
                }
                let id = EntityId((*item).to_string());
                if jojobot_domain::memory::validate_subject(&id).is_err() {
                    if reference && self.ids.contains(*item) {
                        marking.bare += 1;
                        return jojobot_domain::memory::marked_stored_handle(item);
                    }
                    marking.mixed = true;
                    return (*item).to_string();
                }
                match self.names.get(*item).map(|named| named.len()) {
                    Some(1) if self.keep_older && only_handles => {
                        marking.owned_now += 1;
                        marking.older.push((*item).to_string());
                        (*item).to_string()
                    }
                    Some(1) => {
                        marking.handles += 1;
                        if only_handles {
                            marking.owned_now += 1;
                        }
                        self.names[*item]
                            .iter()
                            .map(|id| jojobot_domain::memory::marked_stored_handle(id))
                            .collect()
                    }
                    why => {
                        marking.left.push(((*item).to_string(), why.is_some()));
                        (*item).to_string()
                    }
                }
            })
            .collect();
        marking.value = marked.join(", ");
        marking
    }
}

impl DoltMemory {
    /// The directory of ids and handles, read in the caller's transaction.
    pub(super) async fn directory_in(
        &self,
        tx: &mut Transaction<'_, MySql>,
    ) -> Result<Directory, MemoryError> {
        use std::collections::{BTreeMap, BTreeSet};
        let declared = Self::types_in(tx).await?;
        let reference_keys: BTreeSet<String> = declared
            .iter()
            .flat_map(|d| &d.fields)
            .filter(|f| f.holds == jojobot_domain::memory::types::ValueType::Reference)
            .map(|f| f.key.clone())
            .collect();
        let entities: Vec<(String, Option<String>)> =
            sqlx::query_as("SELECT id, badge FROM entity")
                .fetch_all(&mut **tx)
                .await
                .map_err(store)?;
        let mut names: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut handle_of: BTreeMap<String, String> = BTreeMap::new();
        for (handle, badge) in &entities {
            if let Some(badge) = badge.as_deref().filter(|badge| !badge.is_empty()) {
                names
                    .entry(handle.clone())
                    .or_default()
                    .insert(badge.to_string());
                handle_of.insert(badge.to_string(), handle.clone());
            }
        }
        let former: Vec<(String, String)> =
            sqlx::query_as("SELECT former_handle, badge FROM entity_former_handle")
                .fetch_all(&mut **tx)
                .await
                .map_err(store)?;
        for (handle, badge) in former {
            names.entry(handle).or_default().insert(badge);
        }
        // A record the build supplies wears no badge, and a write stores its
        // handle as its id.
        for (entity, _) in self.supplied.records() {
            let id = entity
                .badge
                .clone()
                .filter(|badge| !badge.is_empty())
                .unwrap_or_else(|| entity.id.to_string());
            names.entry(entity.id.to_string()).or_default().insert(id);
        }
        let ids = names.values().flatten().cloned().collect();
        Ok(Directory {
            names,
            handle_of,
            ids,
            reference_keys,
            keep_older: false,
        })
    }
}

/// **What the link rule would do to a store, counted.** Every count is of items
/// or values and none names a value's text. Read from the stored writes alone,
/// by [`DoltMemory::link_rule_summary`]; nothing is written.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LinkRuleSummary {
    /// Stored writes with a value, all read.
    pub values_read: usize,
    /// Bare ids under a declared reference key that would be marked.
    pub bare_ids: usize,
    /// Values with a loose item among them whose handle items would newly link.
    pub mixed_lists: usize,
    /// Handles that name exactly one thing and would be marked, all shapes.
    pub handles: usize,
    /// Handles that name nothing the store has held; they stay text.
    pub unresolved: usize,
    /// Handles that name more than one thing; they stay text.
    pub ambiguous: usize,
    /// **R1.** Handles in a value of only handles and marks that an older pass
    /// left as text and that exactly one thing owns today. The rule would link
    /// them to today's owner, who may not be the thing they meant.
    pub owned_now: usize,
    /// Link rows the table holds that the rule would not draw.
    pub links_dropped: usize,
    /// Link rows the rule would draw that the table lacks.
    pub links_added: usize,
}

impl LinkRuleSummary {
    /// **Which counts could be inflated, and by what.** `owned_now` is the R1
    /// population: handles an older pass left as text that one thing wears now. The
    /// marking pass leaves them as text, so they are in neither `handles` nor
    /// `links_added`. Writes that continue under log-only store a bare id under a
    /// declared reference key, so `bare_ids` can grow until the rule is on. The
    /// review findings on reads (R2), on the bare id at the write (R3) and on the
    /// settle check (R4) are fixed with the rule on and inflate none of them.
    pub const INFLATED_BY: &'static str = "owned_now: r1 items left as text, in neither handles nor links_added; bare_ids: grows with \
         log-only writes; r2, r3, r4: none";
}

impl DoltMemory {
    /// **Read-only: what the link rule would store and link over the writes held
    /// now.** Begins a transaction, reads, and drops it; no byte, link row or pass
    /// marker changes.
    pub async fn link_rule_summary(&self) -> Result<LinkRuleSummary, MemoryError> {
        use std::collections::BTreeSet;
        let mut tx = self.pool.begin().await.map_err(store)?;
        let mut directory = self.directory_in(&mut tx).await?;
        // Counted as the marking pass will do it: it keeps what an older pass left.
        directory.keep_older = crate::dolt::finished_passes(&self.pool)
            .await
            .contains(FIELD_LOWERING_PASS);
        let writes: Vec<(String, String, i64, String)> = sqlx::query_as(
            "SELECT entity, `key`, ordinal, value FROM field_write WHERE value IS NOT NULL",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        let held: BTreeSet<(String, String, i64, String)> =
            sqlx::query_as::<_, (String, String, i64, String)>(
                "SELECT entity, `key`, ordinal, target FROM field_link",
            )
            .fetch_all(&mut *tx)
            .await
            .map_err(store)?
            .into_iter()
            .collect();
        let mut summary = LinkRuleSummary::default();
        let mut expected: BTreeSet<(String, String, i64, String)> = BTreeSet::new();
        for (entity, key, ordinal, value) in &writes {
            summary.values_read += 1;
            let marking = directory.mark(key, value);
            summary.bare_ids += marking.bare;
            summary.handles += marking.handles;
            summary.owned_now += marking.owned_now;
            if marking.mixed && marking.handles > 0 {
                summary.mixed_lists += 1;
            }
            for (_, ambiguous) in &marking.left {
                match ambiguous {
                    true => summary.ambiguous += 1,
                    false => summary.unresolved += 1,
                }
            }
            for target in jojobot_domain::memory::at_write::field_link_targets(&marking.value) {
                expected.insert((entity.clone(), key.clone(), *ordinal, target));
            }
        }
        summary.links_dropped = held.difference(&expected).count();
        summary.links_added = expected.difference(&held).count();
        Ok(summary)
    }

    /// **The log line of a write the rule would store differently.** Counts the
    /// keys whose lowered value would differ; it names neither key nor value, and
    /// it reads and writes nothing but a debug line.
    pub(super) async fn log_would_store(
        &self,
        tx: &mut Transaction<'_, MySql>,
        sent: &std::collections::BTreeMap<String, String>,
        stored: &std::collections::BTreeMap<String, String>,
    ) -> Result<(), MemoryError> {
        if jojobot_domain::memory::at_write::referenced_by(sent).is_empty() {
            return Ok(());
        }
        let mut would = sent.clone();
        self.lower_marked_fields(tx, &mut would).await?;
        let differing = would
            .iter()
            .filter(|(key, value)| stored.get(*key) != Some(*value))
            .count();
        if differing > 0 {
            tracing::debug!(
                differing,
                "link rule (log-only): the write would be stored differently"
            );
        }
        Ok(())
    }
}
