//! The fake's numbered-collection helpers: what each thing holds, what a
//! retraction took back, and the guard every write path asks.

use super::*;

impl InMemoryMemory {
    /// **Whether any thing here numbers its members.** A scan of the writes for
    /// the key, so a store that never wrote it pays nothing for the feature.
    pub(super) fn carries_numbering(&self) -> bool {
        self.with_writes(|writes| {
            writes
                .iter()
                .any(|w| w.key == super::super::super::numbered::NUMBERS_BY && w.value.is_some())
        })
    }

    /// **Each thing's folded fields, by handle**, over the claims given.
    pub(super) fn fields_by_handle(
        &self,
        entities: &[Entity],
        facts: &[Fact],
    ) -> std::collections::BTreeMap<EntityId, std::collections::BTreeMap<String, String>> {
        let declared = self.declarations();
        entities
            .iter()
            .map(|e| {
                let key = match &e.badge {
                    Some(badge) => EntityId(badge.clone()),
                    None => e.id.clone(),
                };
                (
                    e.id.clone(),
                    super::super::super::folded_fields(&self.writes_on(&key, facts), &declared),
                )
            })
            .collect()
    }

    /// **The numbers each thing's claims held that are no longer current**, by
    /// key, over the claims given — see
    /// [`numbered::retired_of_writes`](super::super::super::numbered::retired_of_writes).
    pub(super) fn retired_by_handle(
        &self,
        entities: &[Entity],
        facts: &[Fact],
    ) -> super::super::super::numbered::Retired {
        let mut all = super::super::super::numbered::Retired::default();
        for e in entities {
            let key = e.badge.clone().map_or_else(|| e.id.clone(), EntityId);
            let held = super::super::super::numbered::retired_of_writes(
                &e.id,
                &self.writes_on(&key, facts),
            );
            all.taken_back.extend(held.taken_back);
            all.founded.extend(held.founded);
        }
        // **The parent each write of a thing's own row left**, for a thing that
        // holds a taken-back number, for everything above it, and for what was
        // folded into any of them.
        let numbered = super::super::super::numbered::last_parent;
        let held = self.entity_versions.lock().expect("fake mutex poisoned");
        // **A version names its parent as the row held it**, a handle when the
        // thing was made and the key it wears since: both read as the handle that
        // key answers to now, so one parent is one value.
        let handle_of = |parent: &EntityId| {
            entities
                .iter()
                .find(|e| e.badge.as_deref() == Some(parent.as_str()))
                .map_or_else(|| parent.clone(), |e| e.id.clone())
        };
        let versions_of = |key: &EntityId| -> Vec<super::super::super::numbered::ParentVersion> {
            held.iter()
                .filter(|(home, _)| home == key)
                .map(|(_, version)| {
                    version.snapshot.as_ref().map(|snapshot| {
                        (snapshot.parent.as_ref().map(&handle_of), version.written_at)
                    })
                })
                .collect()
        };
        let mut work: Vec<EntityId> = all.taken_back.keys().cloned().collect();
        while let Some(node) = work.pop() {
            if all.versions.contains_key(&node) {
                continue;
            }
            let Some(entity) = entities.iter().find(|e| e.id == node) else {
                continue;
            };
            let key = entity.badge.clone().map_or_else(|| node.clone(), EntityId);
            let own = versions_of(&key);
            work.extend(entity.parent.clone());
            for folded in self.keys_folded_into(&key).into_iter().flatten() {
                let handle = handle_of(&folded);
                let versions = versions_of(&folded);
                work.extend(numbered(&versions));
                all.folded
                    .entry(node.clone())
                    .or_default()
                    .push(handle.clone());
                all.versions.insert(handle, versions);
            }
            all.versions.insert(node, own);
        }
        all
    }

    /// The handles a number makes, served as former handles — see
    /// [`numbered::derived_formers`](super::super::super::numbered::derived_formers).
    pub(super) fn numbered_formers(&self) -> Vec<FormerHandle> {
        if !self.carries_numbering() {
            return Vec::new();
        }
        let known = self.known();
        let facts = self.facts_view();
        super::super::super::numbered::derived_formers(
            &known,
            &self.fields_by_handle(&known, &facts),
        )
    }

    /// **What a miss offers**: the numbers a collection holds when the handle
    /// asked for is `<collection>/<N>`, else the ordinary near-miss screen.
    pub(super) fn nearest_for(
        &self,
        asked: &EntityId,
        index: &[Entity],
    ) -> Vec<guard::EntityMatch> {
        if super::super::super::numbered::numbered_handle(asked).is_some()
            && self.carries_numbering()
        {
            let facts = self.facts_view();
            let fields = self.fields_by_handle(index, &facts);
            if let Some(held) =
                super::super::super::numbered::nearest_numbers(asked, index, &fields)
                && !held.is_empty()
            {
                return held;
            }
        }
        guard::screen(asked, &[], index)
    }

    /// **Refuse a write that would give a numbered member a bad number, one
    /// another member holds, or would leave a number nobody holds**, over the
    /// claims and the roster the write sees. `under` is the parent a moved thing
    /// takes. The one function every write path asks.
    pub(super) fn guards_numbering(
        &self,
        change: &super::super::super::numbered::Change,
        under: Option<&EntityId>,
        rows: &[Entity],
        facts: &[Fact],
    ) -> Result<(), MemoryError> {
        if !self.carries_numbering() && !change.sets_numbering() {
            return Ok(());
        }
        let fields = self.fields_by_handle(rows, facts);
        let retired = self.retired_by_handle(rows, facts);
        let reach = super::super::super::numbered::reach_of(change, under, rows, &fields, &retired);
        match reach.refuses(change) {
            Some(refusal) => Err(refusal),
            None => Ok(()),
        }
    }

    /// The writes on one thing that reach it, as the fold reads them.
    pub(super) fn writes_on(
        &self,
        entity: &EntityId,
        facts: &[Fact],
    ) -> Vec<super::super::super::KeyWrite> {
        self.with_writes(|writes| Self::key_writes_on(writes, entity, facts))
    }
}
