//! **Numbered collections, in the Dolt store.** The rules are the domain's
//! ([`jojobot_domain::memory::numbered`]); this reads what they need inside the
//! transaction that makes the write, so the check and the write commit
//! together, and serves the handles a number makes.
//!
//! The tree is walked with one-row queries: up from the subject by the parent
//! pointers to find collections, down from a collection by `parent = badge` to
//! find members. Nothing loads the whole roster.

use std::collections::BTreeSet;

use jojobot_domain::memory::guard::EntityMatch;
use jojobot_domain::memory::numbered::{
    Change, Founding, KeyChange, Member, NUMBERED_DAY, NUMBERS_BY, Numbered, ParentVersion, Reach,
    Retired, TakenBack, founded_at, issued_here, numbered_handle, numbering_change,
    parent_run_start, retired_numbers,
};

use super::*;

/// **One write of a key as the history of the key serves it**, off the record
/// that carried it.
pub(super) fn field_write_of(
    fact: &Fact,
    value: Option<String>,
    written_at: Option<jiff::Timestamp>,
) -> FieldWrite {
    FieldWrite {
        written_at,
        recorded_by: fact.app_stamp.clone(),
        value,
        fact: fact.address(),
        recorded_at: fact.recorded_at,
        status: fact.status,
        provenance: fact.provenance,
        standing: fact.standing,
        note: fact.details.clone(),
    }
}

/// How many keys one `IN (...)` list carries, so a collection of any size is read
/// in a few statements and none is long enough for the server to refuse.
const IN_LIST: usize = 500;

impl DoltMemory {
    /// Every descendant of the thing wearing `root` (a stored key), by the
    /// parent pointers, as `(handle, stored key)`. A cycle ends the walk and a
    /// folded thing is not a member.
    async fn descendants_of_key(
        &self,
        tx: &mut Transaction<'_, MySql>,
        root: &str,
    ) -> Result<Vec<(EntityId, EntityId)>, MemoryError> {
        let mut found = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::from([root.to_string()]);
        let mut frontier = vec![root.to_string()];
        // **Read once for the walk**, not once for each member it reaches.
        let bare = Self::folds_without_an_event(tx).await?;
        while let Some(parent) = frontier.pop() {
            // **The children a read serves under the thing**, which include those
            // of everything folded into it.
            let ids: Vec<String> = self
                .children_served_under(tx, &EntityId(parent.clone()), &bare)
                .await?
                .into_iter()
                .map(|id| id.as_str().to_string())
                .collect();
            let mut badges = Self::badges_of(tx, &ids).await?;
            let rows: Vec<(String, Option<String>)> = ids
                .into_iter()
                .map(|id| {
                    let badge = badges.remove(&id).flatten();
                    (id, badge)
                })
                .collect();
            for (id, badge) in rows {
                let key = badge.unwrap_or_else(|| id.clone());
                if seen.insert(key.clone()) {
                    found.push((EntityId(id), EntityId(key.clone())));
                    frontier.push(key);
                }
            }
        }
        Ok(found)
    }

    /// The badge of each stored key named, in one read. A key with no row, or with
    /// no badge, maps to nothing.
    async fn badges_of(
        tx: &mut Transaction<'_, MySql>,
        ids: &[String],
    ) -> Result<std::collections::HashMap<String, Option<String>>, MemoryError> {
        let mut found = std::collections::HashMap::new();
        for chunk in ids.chunks(IN_LIST) {
            let marks = vec!["?"; chunk.len()].join(", ");
            let sql = format!("SELECT id, badge FROM entity WHERE id IN ({marks})");
            let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
            for id in chunk {
                query = query.bind(id);
            }
            for row in query.fetch_all(&mut **tx).await.map_err(store)? {
                found.insert(
                    row.try_get::<String, _>("id").map_err(store)?,
                    row.try_get::<Option<String>, _>("badge").map_err(store)?,
                );
            }
        }
        Ok(found)
    }

    /// **What each of these things holds, as [`Self::held_by`] reads it, in two
    /// reads for the many and not two per thing.** The types are read once, and the
    /// writes of every thing that no merge touches come back in one query. A thing
    /// a merge touches, on either side, is read by `held_by` itself, because
    /// what it holds depends on the merge events.
    pub(super) async fn held_by_many(
        &self,
        tx: &mut Transaction<'_, MySql>,
        keys: &[EntityId],
    ) -> Result<
        std::collections::BTreeMap<EntityId, std::collections::BTreeMap<String, String>>,
        MemoryError,
    > {
        let declared = Self::types_in(tx).await?;
        let names: Vec<String> = keys.iter().map(|key| key.as_str().to_string()).collect();
        let mut merged: BTreeSet<String> = BTreeSet::new();
        for chunk in names.chunks(IN_LIST) {
            let marks = vec!["?"; chunk.len()].join(", ");
            for column in ["survivor", "folded"] {
                let sql = format!(
                    "SELECT DISTINCT {column} FROM merge_event WHERE ordinal = 0 AND {column} IN ({marks})"
                );
                let mut query = sqlx::query_scalar::<_, String>(AssertSqlSafe(sql.as_str()));
                for name in chunk {
                    query = query.bind(name);
                }
                merged.extend(query.fetch_all(&mut **tx).await.map_err(store)?);
            }
        }
        let plain: Vec<&String> = names
            .iter()
            .filter(|name| !merged.contains(*name))
            .collect();
        let mut writes: std::collections::HashMap<String, Vec<KeyWrite>> = Default::default();
        for chunk in plain.chunks(IN_LIST) {
            let marks = vec!["?"; chunk.len()].join(", ");
            let sql = format!(
                "SELECT w.entity, w.`key`, w.ordinal, w.value, w.fact_id, w.write_class, w.written_at,
                        f.status, f.provenance, f.standing, f.details, f.app_stamp
                 FROM field_write w
                 JOIN fact f ON f.entity = w.entity AND f.id = w.fact_id
                 WHERE w.entity IN ({marks})"
            );
            let mut query = sqlx::query(AssertSqlSafe(sql.as_str()));
            for name in chunk {
                query = query.bind(*name);
            }
            for row in query.fetch_all(&mut **tx).await.map_err(store)? {
                let entity: String = row.try_get("entity").map_err(store)?;
                if let Some(write) = Self::key_write_of(&row, &entity)? {
                    writes.entry(entity).or_default().push(write);
                }
            }
        }
        let mut held = std::collections::BTreeMap::new();
        for key in keys {
            let fields = if merged.contains(key.as_str()) {
                self.held_by(tx, key).await?
            } else {
                folded_fields(&writes.remove(key.as_str()).unwrap_or_default(), &declared)
            };
            held.insert(key.clone(), fields);
        }
        Ok(held)
    }

    /// **The whole numbers that the claims of these things held under `key` and
    /// that are no longer current**, in the same two reads as
    /// [`Self::held_by_many`]: the writes of every thing no merge touches come
    /// back in one query, and a thing a merge touches is read by `writes_on`,
    /// because which writes it serves depends on the merge events.
    async fn retired_of_many(
        &self,
        tx: &mut Transaction<'_, MySql>,
        keys: &[EntityId],
        key: &str,
    ) -> Result<std::collections::BTreeMap<EntityId, Vec<TakenBack>>, MemoryError> {
        let names: Vec<String> = keys.iter().map(|k| k.as_str().to_string()).collect();
        let mut merged: BTreeSet<String> = BTreeSet::new();
        for chunk in names.chunks(IN_LIST) {
            let marks = vec!["?"; chunk.len()].join(", ");
            for column in ["survivor", "folded"] {
                let sql = format!(
                    "SELECT DISTINCT {column} FROM merge_event WHERE ordinal = 0 AND {column} IN ({marks})"
                );
                let mut query = sqlx::query_scalar::<_, String>(AssertSqlSafe(sql.as_str()));
                for name in chunk {
                    query = query.bind(name);
                }
                merged.extend(query.fetch_all(&mut **tx).await.map_err(store)?);
            }
        }
        let plain: Vec<&String> = names.iter().filter(|n| !merged.contains(*n)).collect();
        let mut writes: std::collections::HashMap<String, Vec<KeyWrite>> = Default::default();
        for chunk in plain.chunks(IN_LIST) {
            let marks = vec!["?"; chunk.len()].join(", ");
            let sql = format!(
                "SELECT w.entity, w.`key`, w.ordinal, w.value, w.fact_id, w.write_class, w.written_at,
                        f.status, f.provenance, f.standing, f.details, f.app_stamp
                 FROM field_write w
                 JOIN fact f ON f.entity = w.entity AND f.id = w.fact_id
                 WHERE w.`key` = ? AND w.entity IN ({marks})"
            );
            let mut query = sqlx::query(AssertSqlSafe(sql.as_str())).bind(key);
            for name in chunk {
                query = query.bind(*name);
            }
            for row in query.fetch_all(&mut **tx).await.map_err(store)? {
                let entity: String = row.try_get("entity").map_err(store)?;
                if let Some(write) = Self::key_write_of(&row, &entity)? {
                    writes.entry(entity).or_default().push(write);
                }
            }
        }
        let mut retired = std::collections::BTreeMap::new();
        for member in keys {
            let mut own = if merged.contains(member.as_str()) {
                self.writes_on(tx, member).await?
            } else {
                writes.remove(member.as_str()).unwrap_or_default()
            };
            own.retain(|w| w.key == key);
            own.sort_by_key(|w| w.ordinal);
            let numbers = retired_numbers(own.iter().map(|w| {
                (
                    w.fact.0.clone(),
                    w.value.as_deref(),
                    w.status == FactStatus::Active,
                    w.written_at,
                )
            }));
            if !numbers.is_empty() {
                retired.insert(member.clone(), numbers);
            }
        }
        Ok(retired)
    }

    /// The collection a thing is, if it carries `numbers_by`: the key and
    /// every descendant with what it holds under that key.
    pub(super) async fn collection_at(
        &self,
        tx: &mut Transaction<'_, MySql>,
        handle: &EntityId,
        key: &EntityId,
    ) -> Result<Option<Numbered>, MemoryError> {
        let held = self.held_by(tx, key).await?;
        let Some(by) = held
            .get(NUMBERS_BY)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        else {
            return Ok(None);
        };
        Ok(Some(self.collection_numbering(tx, handle, key, by).await?))
    }

    /// A thing read as a collection numbering by `by`, whether or not it carries
    /// `numbers_by` yet: every descendant with what it holds under that key.
    async fn collection_numbering(
        &self,
        tx: &mut Transaction<'_, MySql>,
        handle: &EntityId,
        key: &EntityId,
        by: String,
    ) -> Result<Numbered, MemoryError> {
        let found = self.descendants_of_key(tx, key.as_str()).await?;
        let keys: Vec<EntityId> = found
            .iter()
            .map(|(_, member_key)| member_key.clone())
            .collect();
        let mut fields = self.held_by_many(tx, &keys).await?;
        let mut history = Retired::default();
        for (member, numbers) in self.retired_of_many(tx, &keys, &by).await? {
            history.taken_back.insert(
                member,
                std::collections::BTreeMap::from([(by.clone(), numbers)]),
            );
        }
        let mut own = self.writes_on(tx, key).await?;
        own.retain(|w| w.key == NUMBERS_BY);
        own.sort_by_key(|w| w.ordinal);
        history.founded.insert(
            handle.clone(),
            founded_at(own.iter().map(|w| {
                (
                    w.value.as_deref(),
                    w.status == FactStatus::Active,
                    w.written_at,
                )
            })),
        );
        let founded = history.founded.get(handle).copied().flatten();
        // **When a member began to sit in the collection** is worked out only for
        // one that holds a taken-back number, so a collection with none pays
        // nothing for it.
        for (member, member_key) in &found {
            if history.taken_back.contains_key(member_key) {
                let joined = self.joined_since(tx, key, member, member_key).await?;
                history.joined.insert(member_key.clone(), joined);
            }
        }
        let mut members = Vec::with_capacity(found.len());
        for (member, member_key) in found {
            let held = fields.remove(&member_key).unwrap_or_default();
            let joined = history.joined.get(&member_key).copied().flatten();
            members.push(Member {
                retired: issued_here(&history, &member_key, &by, founded.max(joined)),
                handle: member,
                value: held.get(&by).cloned(),
            });
        }
        Ok(Numbered {
            collection: handle.clone(),
            key: by,
            members,
        })
    }

    /// **The parent each write of a thing's own row left**, oldest first, as the
    /// stored key of that parent. A write kept before the snapshot existed names
    /// none, and reads as not known.
    async fn parent_versions_of(
        &self,
        tx: &mut Transaction<'_, MySql>,
        handle: &EntityId,
        key: &EntityId,
    ) -> Result<Vec<ParentVersion>, MemoryError> {
        let rows = entity_writes_of(tx, handle, key, "written_at, name, parent").await?;
        let mut kept: Vec<(Option<jiff::Timestamp>, ParentVersion)> = Vec::new();
        for row in &rows {
            let stamp: String = row.try_get("written_at").map_err(store)?;
            let at = parse_stamp(Some(stamp), key.as_str());
            let name: Option<String> = row.try_get("name").map_err(store)?;
            let parent: Option<String> = row.try_get("parent").map_err(store)?;
            kept.push((at, name.map(|_| (parent.map(EntityId), at))));
        }
        kept.sort_by_key(|(at, _)| *at);
        Ok(kept.into_iter().map(|(_, version)| version).collect())
    }

    /// **When a member began to sit under the collection** wearing `collection`:
    /// see [`joined_at`]. The way up is the parent pointers, read a row at a time
    /// for the few members that hold a taken-back number.
    async fn joined_since(
        &self,
        tx: &mut Transaction<'_, MySql>,
        collection: &EntityId,
        member: &EntityId,
        member_key: &EntityId,
    ) -> Result<Option<jiff::Timestamp>, MemoryError> {
        let mut since = self
            .chain_joined(tx, collection, member, member_key)
            .await?;
        // **A claim the member serves from a thing folded into it was written while
        // that thing was a member**: the earliest of their joinings stands, and one
        // not known keeps every number.
        for step in Self::merges_into(tx, member_key).await? {
            let folded_key = EntityId(step.folded);
            let folded = self.current_handle(tx, &folded_key).await?;
            let other = self
                .chain_joined(tx, collection, &folded, &folded_key)
                .await?;
            since = since.zip(other).map(|(own, other)| own.min(other));
        }
        Ok(since)
    }

    /// **When one thing began to sit under the collection**: the latest moment any
    /// thing from it up to the collection began to have its parent.
    async fn chain_joined(
        &self,
        tx: &mut Transaction<'_, MySql>,
        collection: &EntityId,
        member: &EntityId,
        member_key: &EntityId,
    ) -> Result<Option<jiff::Timestamp>, MemoryError> {
        let (mut handle, mut key) = (member.clone(), member_key.clone());
        let mut since: Option<jiff::Timestamp> = None;
        let mut seen: BTreeSet<EntityId> = BTreeSet::new();
        loop {
            if !seen.insert(key.clone()) {
                return Ok(None);
            }
            let versions = self.parent_versions_of(tx, &handle, &key).await?;
            let Some(run) = parent_run_start(&versions) else {
                return Ok(None);
            };
            since = Some(since.map_or(run, |held| held.max(run)));
            let parent: Option<Option<String>> =
                sqlx::query_scalar("SELECT parent FROM entity WHERE id = ?")
                    .bind(handle.as_str())
                    .fetch_optional(&mut **tx)
                    .await
                    .map_err(store)?;
            let Some(parent) = parent.flatten() else {
                return Ok(None);
            };
            if parent == collection.as_str() {
                return Ok(since);
            }
            let Some((next_key, next_handle)) = self.resolve_stored_key(tx, &parent).await? else {
                return Ok(None);
            };
            (handle, key) = (next_handle, next_key);
        }
    }

    /// A stored key (a badge, or the handle of a thing that wears none) read
    /// back as `(key, handle)`.
    async fn resolve_stored_key(
        &self,
        tx: &mut Transaction<'_, MySql>,
        stored: &str,
    ) -> Result<Option<(EntityId, EntityId)>, MemoryError> {
        if let Some(found) = self.resolve_by_badge(tx, stored).await? {
            return Ok(Some(found));
        }
        self.resolve_by_id(tx, &EntityId(stored.to_string())).await
    }

    /// **Every numbered collection a thing sits under**, nearest first. A thing
    /// with no parent costs one row.
    pub(super) async fn numbering_scopes(
        &self,
        tx: &mut Transaction<'_, MySql>,
        subject: &EntityId,
    ) -> Result<Vec<Numbered>, MemoryError> {
        let mut scopes = Vec::new();
        let mut seen: BTreeSet<EntityId> = BTreeSet::from([subject.clone()]);
        let mut at = subject.clone();
        loop {
            let parent: Option<Option<String>> =
                sqlx::query_scalar("SELECT parent FROM entity WHERE id = ?")
                    .bind(at.as_str())
                    .fetch_optional(&mut **tx)
                    .await
                    .map_err(store)?;
            let Some(parent) = parent.flatten() else {
                break;
            };
            // A merge moves no row, so a child of a folded thing still holds the
            // folded thing's key; the reads serve it under the survivor.
            let parent = self.forward_key(tx, &EntityId(parent)).await?;
            let Some((key, handle)) = self.resolve_stored_key(tx, parent.as_str()).await? else {
                break;
            };
            if !seen.insert(handle.clone()) {
                break;
            }
            if let Some(scope) = self.collection_at(tx, &handle, &key).await? {
                scopes.push(scope);
            }
            at = handle;
        }
        Ok(scopes)
    }

    /// **The numbered collections a write reaches, as they stand before it**:
    /// those above any thing it touches, and for a move those at or above the new
    /// parent. Read in the transaction of the write, so the answer and the write
    /// commit together.
    pub(super) async fn numbering_reach(
        &self,
        tx: &mut Transaction<'_, MySql>,
        change: &Change,
        under: Option<&EntityId>,
    ) -> Result<Reach, MemoryError> {
        let mut reach = Reach::default();
        let push = |scope: Numbered, reach: &mut Reach| {
            if !reach
                .scopes
                .iter()
                .any(|s| s.collection == scope.collection)
            {
                reach.scopes.push(scope);
            }
        };
        for handle in change.touched() {
            for scope in self.numbering_scopes(tx, handle).await? {
                push(scope, &mut reach);
            }
        }
        // **A write that gives a thing `numbers_by`, another one or none makes it
        // a collection over the descendants it already has, or ends it as one**,
        // which the scopes above cannot see: the thing is never its own scope.
        // Asked of a change whose thing holds the key after the write, or has
        // written it before; any other write pays one indexed lookup.
        for (handle, after) in change.founds() {
            let Some((key, handle)) = self.resolve(tx, handle).await? else {
                continue;
            };
            if !after.contains_key(NUMBERS_BY) {
                let wrote: Option<i64> = sqlx::query_scalar(
                    "SELECT 1 FROM field_write WHERE entity = ? AND `key` = ? LIMIT 1",
                )
                .bind(key.as_str())
                .bind(NUMBERS_BY)
                .fetch_optional(&mut **tx)
                .await
                .map_err(store)?;
                if wrote.is_none() {
                    continue;
                }
            }
            let now = self.held_by(tx, &key).await?;
            let new = match numbering_change(Some(&now), after) {
                Some(KeyChange::By(by)) => {
                    Some(self.collection_numbering(tx, &handle, &key, by).await?)
                }
                Some(KeyChange::Off) => None,
                None => continue,
            };
            reach.founded.push(Founding {
                old: self.collection_at(tx, &handle, &key).await?,
                new,
            });
        }
        if let Some(parent) = under
            && let Some((key, handle)) = self.resolve(tx, parent).await?
        {
            let mut above = Vec::new();
            if let Some(own) = self.collection_at(tx, &handle, &key).await? {
                above.push(own);
            }
            above.extend(self.numbering_scopes(tx, &handle).await?);
            for scope in above {
                reach.chain.insert(scope.collection.clone());
                push(scope, &mut reach);
            }
        }
        Ok(reach)
    }

    /// **Refuse a write that would give a numbered member a bad number, one
    /// another member holds, or leave a number nobody holds**, in the transaction
    /// the write is made in. The one function a write whose result is known
    /// before it lands asks (a capture, an edit, a retraction, a move).
    pub(super) async fn guard_numbering(
        &self,
        tx: &mut Transaction<'_, MySql>,
        change: &Change,
        under: Option<&EntityId>,
    ) -> Result<(), MemoryError> {
        match self
            .numbering_reach(tx, change, under)
            .await?
            .refuses(change)
        {
            Some(refusal) => Err(refusal),
            None => Ok(()),
        }
    }

    /// **The numbered collections whose numbers a declaration of these keys can
    /// change**, as they stand now: those that number by one of the keys. A key's
    /// fold is part of what its writes come to, so declaring it a counter changes
    /// what every member holds under it without a write to any member.
    pub(super) async fn collections_numbering_by(
        &self,
        tx: &mut Transaction<'_, MySql>,
        keys: &BTreeSet<&str>,
    ) -> Result<Vec<(EntityId, Numbered)>, MemoryError> {
        let stored: Vec<String> =
            sqlx::query_scalar("SELECT DISTINCT entity FROM field_write WHERE `key` = ?")
                .bind(NUMBERS_BY)
                .fetch_all(&mut **tx)
                .await
                .map_err(store)?;
        let mut found: Vec<(EntityId, Numbered)> = Vec::new();
        for stored in stored {
            let Some((key, handle)) = self.resolve_stored_key(tx, &stored).await? else {
                continue;
            };
            if found.iter().any(|(_, held)| held.collection == handle) {
                continue;
            }
            if let Some(scope) = self.collection_at(tx, &handle, &key).await?
                && keys.contains(scope.key.as_str())
            {
                found.push((key, scope));
            }
        }
        Ok(found)
    }

    /// **Refuse a declaration that leaves a member with a number it did not hold
    /// before**, given the collections as they stood before it
    /// ([`Self::collections_numbering_by`]). Asked after the declaration's rows
    /// are written in the same transaction, so each member's fold is read under the
    /// declaration being made. One collection at a time: a member of two
    /// collections that number by different keys is judged in each by its own key.
    pub(super) async fn guard_declared_numbering(
        &self,
        tx: &mut Transaction<'_, MySql>,
        before: Vec<(EntityId, Numbered)>,
    ) -> Result<(), MemoryError> {
        for (key, scope) in before {
            let Some(after) = self.collection_at(tx, &scope.collection, &key).await? else {
                continue;
            };
            let change = Change {
                holds: after
                    .members
                    .iter()
                    .map(|member| {
                        let held = member
                            .value
                            .iter()
                            .map(|value| (scope.key.clone(), value.clone()))
                            .collect();
                        (member.handle.clone(), held)
                    })
                    .collect(),
                ..Default::default()
            };
            let reach = Reach {
                scopes: vec![scope],
                ..Default::default()
            };
            if let Some(refusal) = reach.refuses(&change) {
                return Err(refusal);
            }
        }
        Ok(())
    }

    /// **A move changes which collections the thing and everything under it sit
    /// in.** Each of them is held to the numbers of the collections it leaves and
    /// of those it joins.
    pub(super) async fn guard_reparent(
        &self,
        tx: &mut Transaction<'_, MySql>,
        thing: &EntityId,
        thing_key: &EntityId,
        new_parent: &EntityId,
    ) -> Result<(), MemoryError> {
        let moved = self.subtree_holding(tx, thing, thing_key).await?;
        self.guard_numbering(
            tx,
            &Change {
                moved,
                ..Default::default()
            },
            Some(new_parent),
        )
        .await
    }

    /// **A thing and every thing under it, with what each holds.** What a move
    /// carries with it: a read serves the children of everything folded into a
    /// thing as its own ([`Self::descendants_of_key`]), so a folded thing's
    /// children are in it.
    pub(super) async fn subtree_holding(
        &self,
        tx: &mut Transaction<'_, MySql>,
        thing: &EntityId,
        thing_key: &EntityId,
    ) -> Result<Vec<(EntityId, std::collections::BTreeMap<String, String>)>, MemoryError> {
        let mut subtree = vec![(thing.clone(), self.held_by(tx, thing_key).await?)];
        for (handle, key) in self.descendants_of_key(tx, thing_key.as_str()).await? {
            subtree.push((handle, self.held_by(tx, &key).await?));
        }
        Ok(subtree)
    }

    /// **`<collection>/<N>`, resolved**: the member of the collection holding N,
    /// as `(key, handle)`. The tier after a former handle, tried only for a
    /// handle that ends in a number.
    pub(super) async fn resolve_numbered(
        &self,
        tx: &mut Transaction<'_, MySql>,
        id: &EntityId,
    ) -> Result<Option<(EntityId, EntityId)>, MemoryError> {
        let Some((collection, number)) = numbered_handle(id) else {
            return Ok(None);
        };
        let Some((key, handle)) = Box::pin(self.resolve(tx, &collection)).await? else {
            return Ok(None);
        };
        let Some(scope) = self.collection_at(tx, &handle, &key).await? else {
            return Ok(None);
        };
        match scope.holder_of(number) {
            Some(holder) => self.resolve_by_id(tx, &holder.handle).await,
            None => Ok(None),
        }
    }

    /// **The handles the held numbers make, served as former handles** — see
    /// [`numbered::derived_formers`]. Read for the list of former handles a
    /// caller asks for; the rename history the store itself consults is left as
    /// it was.
    pub(super) async fn derived_formers_in(
        &self,
        tx: &mut Transaction<'_, MySql>,
    ) -> Result<Vec<FormerHandle>, MemoryError> {
        let carriers: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT entity FROM field_write WHERE `key` = ? AND value IS NOT NULL",
        )
        .bind(NUMBERS_BY)
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        let mut derived = Vec::new();
        for stored in carriers {
            let Some((key, handle)) = self.resolve_stored_key(tx, &stored).await? else {
                continue;
            };
            let Some(scope) = self.collection_at(tx, &handle, &key).await? else {
                continue;
            };
            for former in scope.handles() {
                let Some((_, number)) = numbered_handle(&former) else {
                    continue;
                };
                let Some(holder) = scope.holder_of(number) else {
                    continue;
                };
                let Some((badge, _)) = self.resolve_by_id(tx, &holder.handle).await? else {
                    continue;
                };
                derived.push(FormerHandle {
                    former,
                    badge: badge.to_string(),
                    changed_at: NUMBERED_DAY,
                });
            }
        }
        Ok(derived)
    }

    /// **What a miss offers**: for `<collection>/<N>` over a numbered
    /// collection, the numbers it holds; otherwise the ordinary near-miss
    /// screen.
    pub(super) async fn nearest(
        &self,
        tx: &mut Transaction<'_, MySql>,
        asked: &EntityId,
        index: &[Entity],
    ) -> Result<Vec<EntityMatch>, MemoryError> {
        if let Some((collection, _)) = numbered_handle(asked)
            && let Some((key, handle)) = Box::pin(self.resolve(tx, &collection)).await?
            && let Some(scope) = self.collection_at(tx, &handle, &key).await?
        {
            let held: Vec<EntityMatch> = scope
                .handles()
                .into_iter()
                .filter_map(|former| {
                    let (_, number) = numbered_handle(&former)?;
                    let holder = scope.holder_of(number)?;
                    let member = index.iter().find(|e| e.id == holder.handle)?;
                    Some(EntityMatch {
                        handle: former,
                        kind: member.kind,
                        name: member.name.clone(),
                        source: member.source.clone(),
                        reason: guard::MatchReason::HeldNumber,
                        told_apart_by: None,
                    })
                })
                .collect();
            if !held.is_empty() {
                return Ok(held);
            }
        }
        Ok(guard::screen(asked, &[], index))
    }

    pub(super) async fn current_handle(
        &self,
        tx: &mut Transaction<'_, MySql>,
        stored: &EntityId,
    ) -> Result<EntityId, MemoryError> {
        let wearing: Option<String> =
            sqlx::query_scalar("SELECT id FROM entity WHERE badge = ? ORDER BY id LIMIT 1")
                .bind(stored.as_str())
                .fetch_optional(&mut **tx)
                .await
                .map_err(store)?;
        if let Some(id) = wearing {
            return Ok(EntityId(id));
        }
        for (entity, _) in self.supplied.records() {
            if entity.badge.as_deref() == Some(stored.as_str()) {
                return Ok(entity.id.clone());
            }
        }
        Ok(stored.clone())
    }

    /// **The handle of the parent a write names**, as [`Self::row_handle`] reads
    /// it, or none when the write names none.
    pub(super) async fn row_parent(
        &self,
        tx: &mut Transaction<'_, MySql>,
        named: Option<&EntityId>,
    ) -> Result<Option<EntityId>, MemoryError> {
        match named {
            Some(named) => Ok(Some(self.row_handle(tx, named).await?)),
            None => Ok(None),
        }
    }

    /// **The entity as its row stores it**: the parent resolved back to the key
    /// the row holds, so a write of the row never turns a badge-keyed parent
    /// back into a handle.
    pub(super) async fn stored_form(
        &self,
        tx: &mut Transaction<'_, MySql>,
        entity: &Entity,
    ) -> Result<Entity, MemoryError> {
        let Some(parent) = &entity.parent else {
            return Ok(entity.clone());
        };
        let key = self
            .resolve(tx, parent)
            .await?
            .map_or_else(|| parent.clone(), |(key, _)| key);
        Ok(Entity {
            parent: Some(key),
            ..entity.clone()
        })
    }

    /// **The handle of the row an edit, an archive, a restore or a parent
    /// names**: a member's number is the member that holds it, and every other
    /// name is kept as sent, a former handle included, which these writes refuse.
    /// A number nobody holds is kept too, so the miss is reported against what
    /// the caller sent.
    pub(super) async fn row_handle(
        &self,
        tx: &mut Transaction<'_, MySql>,
        id: &EntityId,
    ) -> Result<EntityId, MemoryError> {
        if jojobot_domain::memory::numbered::numbered_handle(id).is_none() {
            return Ok(id.clone());
        }
        Ok(self
            .resolve(tx, id)
            .await?
            .map_or_else(|| id.clone(), |(_, handle)| handle))
    }

    /// **What a handle is stored as, and what it is called today, together**
    /// — the pair every write and every miss-report needs. Mirrors the fake's
    /// `resolve`, over a real store's own rows and its own rename history.
    ///
    /// **Builds no full listing.** Both tiers below are one targeted row
    /// each; only a caller who gets `None` back needs [`known`](Self::known)
    /// at all, to say what a name that answered to nothing resembles.
    pub(super) async fn resolve(
        &self,
        tx: &mut Transaction<'_, MySql>,
        id: &EntityId,
    ) -> Result<Option<(EntityId, EntityId)>, MemoryError> {
        if let Some(found) = self.resolve_by_id(tx, id).await? {
            return Ok(Some(found));
        }
        let former = Self::former_handles_in(tx).await?;
        let Some(badge) = former
            .iter()
            .find(|f| &f.former == id)
            .map(|f| f.badge.clone())
        else {
            // **The last tier: a number a member holds.** Tried only for a
            // handle that ends in a slash and a whole number.
            return self.resolve_numbered(tx, id).await;
        };
        self.resolve_by_badge(tx, &badge).await
    }

    /// **Tier one of [`resolve`](Self::resolve): an id that already answers
    /// directly, stored or supplied.** One row by primary key — mirrors
    /// [`jojobot_domain::memory::resolve_handle`]'s own first tier
    /// (`known.iter().find(|e| &e.id == id)`), expressed as SQL instead of an
    /// in-memory scan, so the common case never has to build the list that
    /// scan runs over.
    pub(super) async fn resolve_by_id(
        &self,
        tx: &mut Transaction<'_, MySql>,
        id: &EntityId,
    ) -> Result<Option<(EntityId, EntityId)>, MemoryError> {
        let row = sqlx::query("SELECT badge FROM entity WHERE id = ?")
            .bind(id.as_str())
            .fetch_optional(&mut **tx)
            .await
            .map_err(store)?;
        if let Some(row) = row {
            let badge = row.try_get::<Option<String>, _>("badge").map_err(store)?;
            let key = badge.map(EntityId).unwrap_or_else(|| id.clone());
            return Ok(Some((key, id.clone())));
        }
        if let Some((entity, _)) = self.supplied.record_for(id) {
            let key = entity
                .badge
                .clone()
                .map(EntityId)
                .unwrap_or_else(|| entity.id.clone());
            return Ok(Some((key, entity.id.clone())));
        }
        Ok(None)
    }

    /// **Tier two of [`resolve`](Self::resolve): a former handle, resolved to
    /// whoever wears its badge today.** One row by badge — the same query
    /// [`current_handle`](Self::current_handle) already runs for the same
    /// reason, and the same second tier `resolve_handle` walks in memory
    /// (`known.iter().find(|e| e.badge == Some(badge))`).
    pub(super) async fn resolve_by_badge(
        &self,
        tx: &mut Transaction<'_, MySql>,
        badge: &str,
    ) -> Result<Option<(EntityId, EntityId)>, MemoryError> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT id FROM entity WHERE badge = ? ORDER BY id LIMIT 1")
                .bind(badge)
                .fetch_optional(&mut **tx)
                .await
                .map_err(store)?;
        if let Some((handle,)) = row {
            return Ok(Some((EntityId(badge.to_string()), EntityId(handle))));
        }
        for (entity, _) in self.supplied.records() {
            if entity.badge.as_deref() == Some(badge) {
                return Ok(Some((EntityId(badge.to_string()), entity.id.clone())));
            }
        }
        Ok(None)
    }
}
