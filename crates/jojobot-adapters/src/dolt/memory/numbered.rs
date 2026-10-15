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
    Change, Founding, Member, NUMBERED_DAY, NUMBERS_BY, Numbered, Reach, new_numbers_by,
    numbered_handle,
};

use super::*;

/// How many keys one `IN (...)` list carries, so a collection of any size is read
/// in a few statements and none is long enough for the server to refuse.
const IN_LIST: usize = 500;

impl DoltMemory {
    /// Every descendant of the thing wearing `root` (a stored key), by the
    /// parent pointers, as `(handle, stored key)`. A cycle ends the walk and a
    /// folded thing is not a member.
    async fn descendants_of_key(
        tx: &mut Transaction<'_, MySql>,
        root: &str,
    ) -> Result<Vec<(EntityId, EntityId)>, MemoryError> {
        let mut found = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::from([root.to_string()]);
        let mut frontier = vec![root.to_string()];
        while let Some(parent) = frontier.pop() {
            // **The children a read serves under the thing**, which include those
            // of everything folded into it.
            let ids: Vec<String> = Self::children_served_under(tx, &EntityId(parent.clone()))
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
            let mut query = sqlx::query(&sql);
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
                let mut query = sqlx::query_scalar::<_, String>(&sql);
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
                        f.status, f.provenance, f.standing, f.details
                 FROM field_write w
                 JOIN fact f ON f.entity = w.entity AND f.id = w.fact_id
                 WHERE w.entity IN ({marks})"
            );
            let mut query = sqlx::query(&sql);
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
                Self::held_by(tx, key).await?
            } else {
                folded_fields(&writes.remove(key.as_str()).unwrap_or_default(), &declared)
            };
            held.insert(key.clone(), fields);
        }
        Ok(held)
    }

    /// The collection a thing is, if it carries `numbers_by`: the key and
    /// every descendant with what it holds under that key.
    pub(super) async fn collection_at(
        tx: &mut Transaction<'_, MySql>,
        handle: &EntityId,
        key: &EntityId,
    ) -> Result<Option<Numbered>, MemoryError> {
        let held = Self::held_by(tx, key).await?;
        let Some(by) = held
            .get(NUMBERS_BY)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        else {
            return Ok(None);
        };
        Ok(Some(Self::collection_numbering(tx, handle, key, by).await?))
    }

    /// A thing read as a collection numbering by `by`, whether or not it carries
    /// `numbers_by` yet: every descendant with what it holds under that key.
    async fn collection_numbering(
        tx: &mut Transaction<'_, MySql>,
        handle: &EntityId,
        key: &EntityId,
        by: String,
    ) -> Result<Numbered, MemoryError> {
        let found = Self::descendants_of_key(tx, key.as_str()).await?;
        let keys: Vec<EntityId> = found
            .iter()
            .map(|(_, member_key)| member_key.clone())
            .collect();
        let mut fields = Self::held_by_many(tx, &keys).await?;
        let mut members = Vec::with_capacity(found.len());
        for (member, member_key) in found {
            let held = fields.remove(&member_key).unwrap_or_default();
            members.push(Member {
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
            let parent = Self::forward_key(tx, &EntityId(parent)).await?;
            let Some((key, handle)) = self.resolve_stored_key(tx, parent.as_str()).await? else {
                break;
            };
            if !seen.insert(handle.clone()) {
                break;
            }
            if let Some(scope) = Self::collection_at(tx, &handle, &key).await? {
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
        // **A write that gives a thing `numbers_by`, or another one, makes it a
        // collection over the descendants it already has**, which the scopes above
        // cannot see: the thing is never its own scope. Asked only of a change that
        // holds the key, so a write on anything else pays no query.
        for (handle, after) in change.founds() {
            if !after.contains_key(NUMBERS_BY) {
                continue;
            }
            let Some((key, handle)) = self.resolve(tx, handle).await? else {
                continue;
            };
            let now = Self::held_by(tx, &key).await?;
            if let Some(by) = new_numbers_by(Some(&now), after) {
                reach.founded.push(Founding {
                    old: Self::collection_at(tx, &handle, &key).await?,
                    new: Self::collection_numbering(tx, &handle, &key, by).await?,
                });
            }
        }
        if let Some(parent) = under
            && let Some((key, handle)) = self.resolve(tx, parent).await?
        {
            let mut above = Vec::new();
            if let Some(own) = Self::collection_at(tx, &handle, &key).await? {
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
            if let Some(scope) = Self::collection_at(tx, &handle, &key).await?
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
            let Some(after) = Self::collection_at(tx, &scope.collection, &key).await? else {
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
        let mut subtree = vec![(thing.clone(), Self::held_by(tx, thing_key).await?)];
        for (handle, key) in Self::descendants_of_key(tx, thing_key.as_str()).await? {
            subtree.push((handle, Self::held_by(tx, &key).await?));
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
        let Some(scope) = Self::collection_at(tx, &handle, &key).await? else {
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
            let Some(scope) = Self::collection_at(tx, &handle, &key).await? else {
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
            && let Some(scope) = Self::collection_at(tx, &handle, &key).await?
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
}
