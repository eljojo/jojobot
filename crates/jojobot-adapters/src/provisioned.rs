//! **The layer that resolves what the build supplies** — a [`Memory`]
//! decorator, and the one place shipped-ness exists.
//!
//! A capability ships data by declaring a [`Provision`]: a value, and where in
//! the store it goes. **Nothing above this layer knows.** A verb writes a
//! column and reads a column; the answer it gets already carries what the build
//! supplies, and the write it sends is kept from storing that half back.
//!
//! **The seam is a decorator for the reason the index is one.**
//! [`IndexedMemory`](crate::search::IndexedMemory) wraps the same port to keep
//! the search projection current and nothing above it has a word for an index.
//! This wraps it to resolve what the build supplies, and nothing above it has a
//! word for a provision. One implementation, over every store — a rule each
//! store implemented for itself is a rule they eventually disagree about.
//!
//! # Where it sits, and why the index is inside it
//!
//! `DoltMemory` → `Provisioned` → `IndexedMemory` → served.
//!
//! **The index scans through this layer, so search reads what every other
//! reader reads.** That is deliberate: the index is a READER, and a reader that
//! sees something no other reader sees is a second seam. A session that reads a
//! charter, searches a phrase out of it and gets nothing would conclude the
//! text is not there.
//!
//! # What it does NOT do
//!
//! It stores nothing and reclaims nothing. The build carries the value; a build
//! that stops supplying one stops supplying it, and the operator's half was
//! never touched. **Materialized provisions — the kinds — are a different
//! mechanism and are not here**: those are rows because the store enforces a
//! kind's required keys by reading what it holds.

use std::collections::BTreeMap;

use async_trait::async_trait;
use jiff::civil::Date;

use jojobot_domain::memory::owned::{Provisions, extended, guard_extension};
use jojobot_domain::memory::{
    Archived, ClaimWrite, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactId,
    FactPatch, FieldBacking, FieldWrite, FormerHandle, Guarded, Memory, MemoryError, Merge,
    NewEntity, NewFact, Retraction, WriteSummary, guard, search, types,
};

/// A store, plus what this build supplies over it.
pub struct Provisioned<M> {
    inner: M,
    provisions: Provisions,
}

impl<M> Provisioned<M> {
    /// Wrap a store with what this build supplies.
    ///
    /// **An empty set is the ordinary case for every row**, so a store with no
    /// provisions over it behaves exactly as the store does.
    pub fn new(inner: M, provisions: Provisions) -> Self {
        Provisioned { inner, provisions }
    }

    /// Resolve a scanned document's prose, if the build supplies any for it.
    fn resolve(&self, doc: &mut search::DocScan) {
        let Some(entity) = doc.entity.as_ref() else {
            return;
        };
        if let Some(shipped) = self.provisions.prose_for(&entity.id) {
            doc.prose = extended(shipped, &doc.prose);
        }
    }

    /// **What this entity's fields would fold to if the store held nothing
    /// of its own** — a whole shipped record's own fields, narrowed by any
    /// per-key default over the same entity, exactly as [`fields`](Self::fields)
    /// folds them before the store's own writes join. Shared so a read and
    /// the echo check can never drift apart about what counts as a default:
    /// [`fields`](Self::fields) extends this with what the store holds;
    /// [`echoed_defaults`](Self::echoed_defaults) compares an incoming write
    /// against it directly.
    fn unwritten_defaults(&self, entity: &EntityId) -> BTreeMap<String, String> {
        let mut defaults = match self.provisions.record_for(entity) {
            Some((_, supplied)) => supplied.clone(),
            None => BTreeMap::new(),
        };
        defaults.extend(self.provisions.fields_for(entity));
        defaults
    }
}

impl<M: Memory + Send + Sync> Provisioned<M> {
    /// A record the build supplies, as the document a reader would have got had
    /// the store held it. **Its facts are the claims the store holds about
    /// it**: the build ships no claim, but a caller may write one on a supplied
    /// record, and the store keeps it under the handle with no entity row to
    /// make a document of its own. Without them the index never holds the
    /// claim's words, and a search for them finds nothing.
    ///
    /// **Its fields are what a read of the record folds to**: the fields the
    /// build ships, under whatever keys the store holds for the handle. The
    /// index is built from this document, so a key written onto a supplied
    /// record has to be in it, or the key finds the stored records that carry it
    /// and never this one.
    async fn supplied_scan(
        &self,
        entity: &EntityId,
    ) -> Result<Option<search::DocScan>, MemoryError> {
        let Some((held, _)) = self.provisions.record_for(entity) else {
            return Ok(None);
        };
        Ok(Some(search::DocScan {
            doc_id: held.id.to_string(),
            title: held.name.clone(),
            prose: self
                .provisions
                .prose_for(entity)
                .unwrap_or_default()
                .to_string(),
            entity: Some(held.clone()),
            facts: self.inner.recall(entity).await?,
            fields: self.fields(entity).await?,
            owner: None,
        }))
    }
}

#[async_trait]
impl<M: Memory + Send + Sync> Memory for Provisioned<M> {
    // ── the two reads prose leaves the store by ─────────────────────────────
    //
    // Every reader above — the graph query, the orientation door, the search
    // index — takes prose from one of these two, so resolving here reaches all
    // of them and none of them gains a branch.

    async fn scan(&self) -> Result<Vec<search::DocScan>, MemoryError> {
        let mut scanned = self.inner.scan().await?;
        if self.provisions.is_empty() {
            return Ok(scanned);
        }
        for doc in &mut scanned {
            self.resolve(doc);
        }
        for (entity, _) in self.provisions.records() {
            let stored = scanned
                .iter()
                .any(|d| d.entity.as_ref().is_some_and(|e| e.id == entity.id));
            if !stored && let Some(doc) = self.supplied_scan(&entity.id).await? {
                scanned.push(doc);
            }
        }
        Ok(scanned)
    }

    /// **What a boot will serve for this charter**, the build's layer joined to
    /// `own` by the same function [`resolve`](Self::resolve) joins a stored
    /// doc's prose with.
    async fn composed_prose(&self, entity: &EntityId, own: &str) -> Result<String, MemoryError> {
        match self.provisions.prose_for(entity) {
            Some(shipped) => Ok(extended(shipped, own)),
            None => self.inner.composed_prose(entity, own).await,
        }
    }

    async fn scan_entity(&self, entity: &EntityId) -> Result<Option<search::DocScan>, MemoryError> {
        match self.inner.scan_entity(entity).await? {
            Some(mut doc) => {
                self.resolve(&mut doc);
                Ok(Some(doc))
            }
            // The store holds no row, so a supplied record is the whole answer.
            None => self.supplied_scan(entity).await,
        }
    }

    /// **A supplied record is never stored and never changes while this
    /// process runs** (rule 234's own build/store split), so it cannot move
    /// this signal — the store underneath is the whole answer.
    async fn write_summary(&self) -> Result<Option<WriteSummary>, MemoryError> {
        self.inner.write_summary().await
    }

    // ── and the one write that reaches it ───────────────────────────────────

    /// **The read-modify-write trap, closed underneath.**
    ///
    /// A caller reads the resolved value, adds a line and sends the whole thing
    /// back. The caller has no way to know it is doing anything unusual — the
    /// half it is echoing looks exactly like the half it wrote — so the guard
    /// cannot be the caller's, and it cannot be a warning either.
    ///
    /// **What comes back is the stored half**, which is the receipt for what
    /// this call put in the store rather than what a later read will answer
    /// with.
    async fn set_prose(&self, entity: &EntityId, prose: &str) -> Result<String, MemoryError> {
        if let Some(shipped) = self.provisions.prose_for(entity) {
            guard_extension(shipped, prose)?;
        }
        self.inner.set_prose(entity, prose).await
    }

    // ── everything else is the store's, unchanged ───────────────────────────

    /// **A handle the build supplies is taken, exactly as a stored one is.**
    ///
    /// What the build supplies behaves like a stored row (rule 234), and that
    /// is not only true of reads: a guard that consults the store to DECIDE
    /// something has to see the supplied half too. **The guard is the half that
    /// fails silently** — a read resolving a supplied record while the screen
    /// beside it sees nothing lets a second thing answer to one name, and
    /// nothing reports it.
    ///
    /// **Exact handle, so it is never overridable** — a token answers *these
    /// are two different things*, and there is only one handle here.
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        if let Some((supplied, _)) = self.provisions.record_for(&new.id) {
            return Ok(Guarded::Blocked {
                attempted: new.id.clone(),
                candidates: vec![guard::EntityMatch {
                    handle: supplied.id.clone(),
                    kind: supplied.kind,
                    name: supplied.name.clone(),
                    source: supplied.source.clone(),
                    reason: guard::MatchReason::ExactHandle,
                }],
            });
        }
        self.inner.add_entity(new).await
    }

    /// **A handle the build supplies is taken here too**, for the reason
    /// `add_entity` refuses it: the creation that writes its first claim must not
    /// be a way past the screen.
    async fn add_entity_with_first_claim(
        &self,
        new: NewEntity,
        first: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        if let Some((supplied, _)) = self.provisions.record_for(&new.id) {
            return Ok(Guarded::Blocked {
                attempted: new.id.clone(),
                candidates: vec![guard::EntityMatch {
                    handle: supplied.id.clone(),
                    kind: supplied.kind,
                    name: supplied.name.clone(),
                    source: supplied.source.clone(),
                    reason: guard::MatchReason::ExactHandle,
                }],
            });
        }
        self.inner.add_entity_with_first_claim(new, first).await
    }

    /// **A supplied record is not edited by the edit that writes a claim**, for the
    /// reason `update_entity` refuses it: the claim must not be a way past the
    /// screen.
    ///
    /// **An edit that names nothing of the record itself is a capture.** What the
    /// call sets is written on top of what the build ships, exactly as `capture`
    /// writes it, and the record answered with is the supplied one. A name,
    /// aliases, source or crm in the same call refuses the whole call, so the
    /// claim is not written either.
    async fn update_entity_with_claim(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
        claim: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        if let Some((supplied, _)) = self.provisions.record_for(handle) {
            let edits_the_record = patch.name.is_some()
                || patch.aliases.is_some()
                || patch.source.is_some()
                || patch.crm.is_some();
            if !edits_the_record {
                // The same check the store makes before it writes the pair.
                if claim.subject != *handle {
                    return Err(MemoryError::InvalidFact(format!(
                        "the claim is about {}, and the thing being edited is {handle}",
                        claim.subject
                    )));
                }
                return Ok(match self.inner.capture(claim).await? {
                    Guarded::Written(fact) => Guarded::Written((supplied.clone(), fact)),
                    Guarded::Blocked {
                        attempted,
                        candidates,
                    } => Guarded::Blocked {
                        attempted,
                        candidates,
                    },
                });
            }
            return Ok(Guarded::Blocked {
                attempted: handle.clone(),
                candidates: vec![guard::EntityMatch {
                    handle: supplied.id.clone(),
                    kind: supplied.kind,
                    name: supplied.name.clone(),
                    source: supplied.source.clone(),
                    reason: guard::MatchReason::ExactHandle,
                }],
            });
        }
        self.inner
            .update_entity_with_claim(handle, patch, claim)
            .await
    }

    // ── the reads a WHOLE supplied record has to answer ─────────────────────
    //
    // A record the build supplies is in no table, so every read that would
    // have found a stored one has to find this one instead. These four are
    // what the graph query reads through, which is what makes a supplied
    // record answerable by the same code that answers a stored one.

    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        let mut held = self.inner.list_entities(kind).await?;
        for (entity, _) in self.provisions.records() {
            // **The store's row wins where both exist.** What the operator
            // declared is theirs, and a build that supplies the same handle
            // does not overwrite it in the answer.
            if kind.is_none_or(|k| entity.kind == k) && !held.iter().any(|e| e.id == entity.id) {
                held.push(entity.clone());
            }
        }
        Ok(held)
    }

    /// A build-supplied record is not a row, so it has no rename history —
    /// this is the store's own, unchanged.
    async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
        self.inner.former_handles().await
    }

    /// **What the store holds under the build's keys.**
    ///
    /// ⛔️ **No rescue here.** The store's own existence gate reads what the
    /// build supplies, so a supplied record is not a miss underneath and there
    /// is nothing to turn into an empty answer — and an arm that did would hide
    /// the keys a caller really wrote on that handle.
    ///
    /// **Two supplied halves fold in, both under what the store holds**: a
    /// whole record's own fields (for an address the store holds nothing
    /// of), and per-key defaults (for a row the store already holds — see
    /// [`Supplies::Field`](jojobot_domain::memory::owned::Supplies::Field)).
    /// Either way the store's own write of the same key is what a caller
    /// reads; a supplied default only ever answers where the operator wrote
    /// nothing.
    async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        let held = self.inner.fields(entity).await?;
        let mut folded = self.unwritten_defaults(entity);
        folded.extend(held);
        Ok(folded)
    }

    /// **Straight through.** A supplied default is not a write, so it has no
    /// moment and no age: what the store took in is all there is to date.
    async fn field_ages(
        &self,
        entity: &EntityId,
    ) -> Result<jojobot_domain::memory::FieldAges, MemoryError> {
        self.inner.field_ages(entity).await
    }

    /// **The same fold as [`fields`](Self::fields), under the store's own
    /// marker.** `Folded` caches what this answers, so the shipped defaults
    /// have to be in it, and its stale-read guard compares the marker, so it
    /// has to be the store's rather than the trait default's `0`.
    async fn fields_versioned(
        &self,
        entity: &EntityId,
    ) -> Result<(BTreeMap<String, String>, u64), MemoryError> {
        let (held, version) = self.inner.fields_versioned(entity).await?;
        let mut folded = self.unwritten_defaults(entity);
        folded.extend(held);
        Ok((folded, version))
    }

    /// **The one comparison this layer can make that the store underneath
    /// cannot**: only `Provisioned` holds what the build ships, so only here
    /// can a write be compared against it. Per-key exact-value equality
    /// against [`unwritten_defaults`](Self::unwritten_defaults) — never the
    /// line-window prose comparison [`guard_extension`] runs for
    /// [`set_prose`](Self::set_prose), which is the wrong shape for a bag of
    /// independent scalars. Nothing here changes what gets written; a caller
    /// (`capture` and `update_fact`, in this build) reads this beside the
    /// write to build its own receipt.
    async fn echoed_defaults(
        &self,
        entity: &EntityId,
        fields: &BTreeMap<String, String>,
    ) -> Vec<String> {
        let defaults = self.unwritten_defaults(entity);
        let mut echoed: Vec<String> = fields
            .iter()
            .filter(|(key, value)| defaults.get(key.as_str()) == Some(*value))
            .map(|(key, _)| key.clone())
            .collect();
        echoed.sort();
        echoed
    }

    async fn backing(
        &self,
        entity: &EntityId,
    ) -> Result<BTreeMap<String, FieldBacking>, MemoryError> {
        self.inner.backing(entity).await
    }
    async fn built_on(&self, source: &FactAddress) -> Result<Vec<Fact>, MemoryError> {
        self.inner.built_on(source).await
    }
    async fn referring_to(&self, target: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.inner.referring_to(target).await
    }
    async fn thought_writers(&self, thread: &EntityId) -> Result<Vec<EntityId>, MemoryError> {
        self.inner.thought_writers(thread).await
    }
    // **`children` is deliberately NOT overridden here** (rule 234). The port
    // derives it from `list_entities`, and this layer's `list_entities` already
    // resolves what the build supplies — so the default asks the right question
    // by construction. Forwarding it to the inner store asked the store's own
    // rows, which hold no supplied record, and a read that resolves the record
    // while the read beside it says the handle names nothing is the two halves
    // disagreeing about what exists.
    //
    // ⛔️ **Not a rescue either.** An arm turning the miss into an empty answer
    // would hide a child the operator really filed under a supplied record; the
    // default returns it.
    /// **A supplied record is not the caller's to rename.**
    ///
    /// It is refused the way creating one under that handle is refused, and for
    /// the same reason: the handle is taken, by a record the build supplies.
    ///
    /// ⚠️ **The failure this closes is a guard that could not see what the
    /// build supplies** (rule 234). The inner store holds no row for the
    /// handle, so it answered that the handle names nothing — and offered no
    /// candidates, which says nothing even resembles it. **Every other read
    /// returns the record.** A caller met "there is no such thing" from one
    /// path and the record itself from every other, with no way to tell which
    /// was lying.
    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        if let Some((supplied, _)) = self.provisions.record_for(handle) {
            return Ok(Guarded::Blocked {
                attempted: handle.clone(),
                candidates: vec![guard::EntityMatch {
                    handle: supplied.id.clone(),
                    kind: supplied.kind,
                    name: supplied.name.clone(),
                    source: supplied.source.clone(),
                    reason: guard::MatchReason::ExactHandle,
                }],
            });
        }
        self.inner.update_entity(handle, patch).await
    }
    /// **The build's own half is not a rename's to move.** Refused before the
    /// write ever reaches the store underneath, with
    /// [`MemoryError::SuppliedHandle`] — the answer `archive_entity` and
    /// `merge` give for the same fact.
    ///
    /// ⚠️ **Not a `Guarded::Blocked`.** That shape is a rename's answer about
    /// its destination or its parent, and the verb words every blocked answer
    /// that is neither a self-parent nor the destination as "the parent this
    /// call named, which jojobot does not know": a record that exists and was
    /// never a parent read as one that does not.
    async fn rename_entity(
        &self,
        from: &EntityId,
        to: &EntityId,
        parent: Option<EntityId>,
        date: Date,
        override_token: Option<&str>,
    ) -> Result<Guarded<Entity>, MemoryError> {
        if self.provisions.record_for(from).is_some() {
            return Err(MemoryError::SuppliedHandle {
                attempted: from.to_string(),
            });
        }
        self.inner
            .rename_entity(from, to, parent, date, override_token)
            .await
    }
    /// **The build's own half has no row to mark.** Same shape as
    /// [`rename_entity`](Self::rename_entity), but there is no candidate to
    /// hand back: archiving carries no near-miss screening, so
    /// [`MemoryError::SuppliedHandle`] is the whole of the answer.
    async fn archive_entity(&self, id: &EntityId, reason: &str) -> Result<Entity, MemoryError> {
        if self.provisions.record_for(id).is_some() {
            return Err(MemoryError::SuppliedHandle {
                attempted: id.to_string(),
            });
        }
        self.inner.archive_entity(id, reason).await
    }
    async fn restore_entity(&self, id: &EntityId) -> Result<(Entity, Archived), MemoryError> {
        if self.provisions.record_for(id).is_some() {
            return Err(MemoryError::SuppliedHandle {
                attempted: id.to_string(),
            });
        }
        self.inner.restore_entity(id).await
    }
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        self.inner.capture(fact).await
    }
    /// **Straight through.** The store's existence gate reads what the build
    /// supplies, so a supplied record answers with the claims written on it —
    /// none, usually, and whatever a caller wrote when there are some.
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.inner.recall(subject).await
    }
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        self.inner.update_fact(address, patch, caller).await
    }
    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        self.inner.history(entity, key).await
    }
    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        self.inner.claim_history(address).await
    }
    async fn keys_kept_off_the_thing(
        &self,
        address: &FactAddress,
    ) -> Result<std::collections::BTreeSet<String>, MemoryError> {
        self.inner.keys_kept_off_the_thing(address).await
    }
    async fn claim_histories(
        &self,
        entity: &EntityId,
    ) -> Result<std::collections::HashMap<FactId, Vec<ClaimWrite>>, MemoryError> {
        self.inner.claim_histories(entity).await
    }
    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Retraction, MemoryError> {
        self.inner
            .retract(address, reason, date, caller, session)
            .await
    }
    /// **A handle the build supplies is taken, on either side of a fold —
    /// exactly as it is for add, update and rename** (rule 234). Checked
    /// here, before the store underneath is ever asked, for the reason
    /// those three are: a guard that consults the store to decide something
    /// has to see the supplied half too, or a handle that reads, lists and
    /// searches as existing comes back from a row-only existence check as
    /// though it names nothing.
    ///
    /// ⚠️ **This returns an `Err`, not `Guarded::Blocked`** — unlike its
    /// three siblings. `merge`'s own signature carries no blocked shape to
    /// answer into: there is no `Guarded<Merge>`. `MemoryError::SuppliedHandle`
    /// is the one this layer already has for exactly this fact — real
    /// handle, no row underneath — so this reuses it rather than growing a
    /// second shape that says the same thing.
    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Merge, MemoryError> {
        for side in [folded, survivor] {
            if self.provisions.record_for(side).is_some() {
                return Err(MemoryError::SuppliedHandle {
                    attempted: side.to_string(),
                });
            }
        }
        self.inner
            .merge(folded, survivor, reason, date, caller, session)
            .await
    }
    async fn declare_type(
        &self,
        declared: types::DeclaredType,
    ) -> Result<types::DeclaredType, MemoryError> {
        self.inner.declare_type(declared).await
    }
    async fn declared_types(&self) -> Result<Vec<types::DeclaredType>, MemoryError> {
        self.inner.declared_types().await
    }
    async fn displaced_type(&self, name: &str) -> Result<Option<types::Displaced>, MemoryError> {
        self.inner.displaced_type(name).await
    }
    async fn declare_kind(
        &self,
        token: &str,
        origin: types::Origin,
        fields: Vec<types::Field>,
    ) -> Result<(), MemoryError> {
        self.inner.declare_kind(token, origin, fields).await
    }
    async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
        self.inner.reclaim_kind(token).await
    }
    async fn declared_kinds(&self) -> Result<Vec<(String, types::Origin)>, MemoryError> {
        self.inner.declared_kinds().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jojobot_domain::memory::owned::Provision;
    use jojobot_domain::memory::testing::InMemoryMemory;

    const CORE: &str = "You answer in one line unless asked otherwise.";

    async fn provisioned() -> (Provisioned<InMemoryMemory>, EntityId) {
        let bot = EntityId("bot:gamma".into());
        let store = InMemoryMemory::booted();
        store
            .add_entity(NewEntity::new(bot.clone(), "Gamma", "jojobot"))
            .await
            .expect("the identity is created")
            .written()
            .expect("an empty board blocks nothing");
        let over = Provisioned::new(
            store,
            Provisions::new(vec![Provision::prose(bot.clone(), CORE)]),
        );
        (over, bot)
    }

    /// The layer forwards which keys of a record the store holds off the thing.
    #[tokio::test]
    async fn the_layer_forwards_the_keys_a_switched_store_holds_off_a_thing() {
        let over = Provisioned::new(InMemoryMemory::booted(), Provisions::new(vec![]));
        jojobot_domain::memory::testing::contract::own_fields_stay_while_settings_and_computed_keys_reach(&over).await;
    }

    /// The prose a reader gets for that entity, through the read every reader
    /// above this layer takes it from.
    async fn read(store: &Provisioned<InMemoryMemory>, bot: &EntityId) -> String {
        store
            .scan_entity(bot)
            .await
            .expect("the scan reads")
            .expect("the entity is there")
            .prose
    }

    /// **A read carries what the build supplies, and the operator's own text
    /// under it.**
    ///
    /// Both halves out of one read: the build's alone passes on a layer that
    /// throws the operator's text away, and the operator's alone passes on a
    /// layer that resolves nothing.
    #[tokio::test]
    async fn a_read_carries_what_the_build_supplies_and_what_the_operator_wrote() {
        let (store, bot) = provisioned().await;
        store
            .set_prose(&bot, "Gamma also files the weekly note.")
            .await
            .expect("the operator's own layer lands");

        let read = read(&store, &bot).await;
        assert!(read.contains(CORE), "the build's half is there: {read}");
        assert!(
            read.contains("files the weekly note"),
            "…and the operator's is too: {read}",
        );
        assert!(
            read.find(CORE) < read.find("files the weekly note"),
            "…with the build's half first, because the second narrows it: {read}",
        );
    }

    /// **An entity the build supplies nothing for is untouched** — which is
    /// every entity in the store but one.
    ///
    /// The positive the case above rests on: without it, a layer that pasted
    /// the same text onto everything would pass.
    #[tokio::test]
    async fn an_entity_the_build_supplies_nothing_for_reads_exactly_what_was_stored() {
        let (store, _) = provisioned().await;
        let other = EntityId("bot:delta".into());
        store
            .add_entity(NewEntity::new(other.clone(), "Delta", "jojobot"))
            .await
            .expect("a second identity")
            .written()
            .expect("an empty board blocks nothing");
        store
            .set_prose(&other, "Delta writes its own charter.")
            .await
            .expect("prose lands");

        assert_eq!(read(&store, &other).await, "Delta writes its own charter.");
    }

    /// **A bot nobody has written for still answers with what the build
    /// supplies**, and with nothing bolted onto it.
    #[tokio::test]
    async fn an_unwritten_entity_answers_with_the_build_alone() {
        let (store, bot) = provisioned().await;
        assert_eq!(read(&store, &bot).await, CORE);
    }

    /// **THE READ-MODIFY-WRITE TRAP.** A caller reads the resolved value, adds
    /// a line, sends it back — and the build's own words would become this
    /// instance's, where they stop moving when the software does.
    ///
    /// **Both halves in one read of the store**: the echoed write is refused,
    /// and the caller's own text lands. Without the second, the case passes on
    /// a layer that refuses every write there is.
    #[tokio::test]
    async fn sending_a_read_back_is_refused_and_the_callers_own_text_lands() {
        let (store, bot) = provisioned().await;
        let round_trip = format!(
            "{}\n\nAnd Gamma files the weekly note.",
            read(&store, &bot).await
        );

        let refused = store
            .set_prose(&bot, &round_trip)
            .await
            .expect_err("a write carrying the build's own half is refused");
        assert!(
            matches!(refused, MemoryError::RepeatsShipped),
            "the refusal says what is wrong rather than failing the store: {refused:?}",
        );

        store
            .set_prose(&bot, "Gamma files the weekly note.")
            .await
            .expect("the same intent, sent as what is being added, lands");

        let read = read(&store, &bot).await;
        assert!(
            read.contains("files the weekly note"),
            "the caller's own text is stored: {read}",
        );
        assert_eq!(
            read.matches(CORE).count(),
            1,
            "…and the build's half is there exactly once, not stored a second time: {read}",
        );
    }

    /// **An operator's extension survives an upgrade that changes what the
    /// build supplies** — paired, in one read, with the build's half having
    /// actually changed.
    ///
    /// Without the pairing this passes on a build where nothing upgrades: the
    /// extension surviving is only interesting if the half beside it moved.
    #[tokio::test]
    async fn an_extension_survives_an_upgrade_that_changes_what_the_build_ships() {
        let (store, bot) = provisioned().await;
        store
            .set_prose(&bot, "Gamma files the weekly note.")
            .await
            .expect("the operator writes their own layer");

        // The upgrade: a later build, over the same store, supplying different
        // text at the same address. Nothing migrates and nothing runs.
        const IMPROVED: &str = "You answer in one line, and you say when you are unsure.";
        let upgraded = Provisioned::new(
            store.inner,
            Provisions::new(vec![Provision::prose(bot.clone(), IMPROVED)]),
        );

        let read = read(&upgraded, &bot).await;
        assert!(
            read.contains(IMPROVED),
            "the later build's half reached an instance that was already running: {read}",
        );
        assert!(
            !read.contains(CORE),
            "…and the older build's half is gone rather than accumulated: {read}",
        );
        assert!(
            read.contains("files the weekly note"),
            "…while what the operator wrote is exactly where they left it: {read}",
        );
    }

    /// **Renaming a supplied record is refused the way creating one is, and
    /// the refusal says the record is there.**
    ///
    /// `add_entity` on a supplied handle comes back blocked, naming the
    /// collision. `update_entity` reached the inner store, which holds no row
    /// for that handle and answered that the handle names nothing — so one path
    /// resolved the record, one blocked on it, and a third could not see it.
    /// **A caller was told a thing does not exist while every read returns it**,
    /// and the candidates offered came from an index the record is not in.
    ///
    /// **Both halves in one read.** The supplied record is refused with the
    /// collision named, and the identical call on a STORED entity still
    /// renames it — without the second, a decorator that blocked every rename
    /// would pass the first.
    #[tokio::test]
    async fn renaming_a_supplied_record_is_blocked_and_a_stored_one_still_renames() {
        let store = InMemoryMemory::booted();
        store
            .add_entity(jojobot_domain::memory::NewEntity::new(
                EntityId("view:my-week".into()),
                "The week",
                "the operator",
            ))
            .await
            .expect("the operator's own view is created")
            .written()
            .expect("an empty board blocks nothing");
        let over = Provisioned::new(store, Provisions::new(vec![shipped_record("loops")]));

        let renamed = over
            .update_entity(
                &EntityId("view:loops".into()),
                jojobot_domain::memory::EntityPatch {
                    name: Some("Mine now".into()),
                    ..Default::default()
                },
            )
            .await;

        match renamed {
            Ok(jojobot_domain::memory::Guarded::Blocked {
                attempted,
                candidates,
            }) => {
                assert_eq!(attempted, EntityId("view:loops".into()));
                assert!(
                    candidates
                        .iter()
                        .any(|c| c.handle == EntityId("view:loops".into())),
                    "the refusal names the record it collided with: {candidates:?}",
                );
            }
            other => panic!(
                "a supplied record is there, so renaming it is a refusal that names it — \
                 not an answer that it does not exist: {other:?}"
            ),
        }

        let stored = over
            .update_entity(
                &EntityId("view:my-week".into()),
                jojobot_domain::memory::EntityPatch {
                    name: Some("The week ahead".into()),
                    ..Default::default()
                },
            )
            .await
            .expect("the operator's own record is theirs to rename");
        assert!(
            matches!(stored, jojobot_domain::memory::Guarded::Written(ref e) if e.name == "The week ahead"),
            "…and a stored record still renames exactly as it did: {stored:?}",
        );
    }

    /// **An edit that writes a claim and names nothing of the record itself is a
    /// capture on a supplied record**, and a name beside it refuses the whole
    /// call.
    ///
    /// Three reads in one case. The claim lands and the supplied record is what
    /// comes back, with what the build ships still under what was set; the same
    /// call with a name refuses and leaves the claim unwritten, so the pair is
    /// whole or not at all; and a claim about another record than the one edited
    /// is the malformed call it is on a stored record. Without the second, a
    /// layer that ignored the patch and wrote the claim would pass the first.
    #[tokio::test]
    async fn an_edit_that_only_sets_lands_on_a_supplied_record_and_one_that_renames_does_not() {
        let supplied = Provisions::new(vec![shipped_record("loops")]);
        let over = Provisioned::new(InMemoryMemory::booted().knowing(supplied.clone()), supplied);
        let id = EntityId("view:loops".into());
        let claim = |content: &str, key: &str| {
            let mut fact = NewFact::about(id.clone(), content, Date::constant(2026, 10, 12));
            fact.fields
                .insert(key.to_string(), "kept on top".to_string());
            fact.sets.insert(key.to_string());
            fact
        };

        let landed = over
            .update_entity_with_claim(&id, EntityPatch::default(), claim("a note", "note"))
            .await
            .expect("a sets-only edit of a supplied record answers");
        match landed {
            Guarded::Written((entity, _)) => assert_eq!(entity.id, id),
            other => panic!("a sets-only edit lands on a supplied record: {other:?}"),
        }
        let held = over.fields(&id).await.expect("fields read");
        assert_eq!(
            held.get("note").map(String::as_str),
            Some("kept on top"),
            "{held:?}"
        );
        assert_eq!(
            held.get("selects").map(String::as_str),
            Some("rhythm"),
            "what the build ships is still under it: {held:?}",
        );

        let renamed = over
            .update_entity_with_claim(
                &id,
                EntityPatch {
                    name: Some("Mine".into()),
                    ..Default::default()
                },
                claim("another note", "other"),
            )
            .await
            .expect("the refusal is an answer");
        assert!(
            matches!(renamed, Guarded::Blocked { ref attempted, .. } if attempted == &id),
            "a name beside the sets refuses the call: {renamed:?}",
        );
        let held = over.fields(&id).await.expect("fields read");
        assert!(
            !held.contains_key("other"),
            "a refused edit writes none of its claim: {held:?}",
        );

        let elsewhere = over
            .update_entity_with_claim(
                &id,
                EntityPatch::default(),
                NewFact::about(
                    EntityId("view:ready".into()),
                    "about another record",
                    Date::constant(2026, 10, 12),
                ),
            )
            .await;
        assert!(
            matches!(elsewhere, Err(MemoryError::InvalidFact(_))),
            "a claim about another record is a malformed call: {elsewhere:?}",
        );
    }

    /// **Folding a supplied record away, or into it, is refused before the
    /// store is ever asked — the fourth sibling merge was missing.**
    ///
    /// `add_entity`, `update_entity` and `rename_entity` all short-circuit on
    /// what the build supplies; `merge` forwarded straight through. Checked
    /// on both sides, because a supplied handle is equally untouchable as
    /// the duplicate and as the survivor.
    ///
    /// **Paired with a genuinely stored fold, which still lands** — without
    /// it, a decorator that refused every fold would pass the first two
    /// assertions identically.
    #[tokio::test]
    async fn merging_a_supplied_record_is_refused_and_a_stored_pair_still_merges() {
        let store = InMemoryMemory::booted();
        for id in ["thing:merge-spare", "thing:merge-kept"] {
            store
                .add_entity(jojobot_domain::memory::NewEntity::new(
                    EntityId(id.into()),
                    id,
                    "the operator",
                ))
                .await
                .expect("the operator's own thing is created")
                .written()
                .expect("an empty board blocks nothing");
        }
        let over = Provisioned::new(store, Provisions::new(vec![shipped_record("loops")]));

        let as_folded = over
            .merge(
                &EntityId("view:loops".into()),
                &EntityId("thing:merge-kept".into()),
                None,
                Date::constant(2026, 6, 13),
                &EntityId("bot:sigma".into()),
                None,
            )
            .await
            .expect_err("folding a supplied record away must be refused");
        assert!(
            matches!(&as_folded, MemoryError::SuppliedHandle { attempted } if attempted == "view:loops"),
            "the refusal says the supplied handle exists and has no row to move: {as_folded:?}",
        );

        let as_survivor = over
            .merge(
                &EntityId("thing:merge-spare".into()),
                &EntityId("view:loops".into()),
                None,
                Date::constant(2026, 6, 13),
                &EntityId("bot:sigma".into()),
                None,
            )
            .await
            .expect_err("folding a stored thing into a supplied record must be refused");
        assert!(
            matches!(&as_survivor, MemoryError::SuppliedHandle { attempted } if attempted == "view:loops"),
            "the refusal says the same when the supplied handle is the survivor: {as_survivor:?}",
        );

        over.merge(
            &EntityId("thing:merge-spare".into()),
            &EntityId("thing:merge-kept".into()),
            None,
            Date::constant(2026, 6, 13),
            &EntityId("bot:sigma".into()),
            None,
        )
        .await
        .expect("a fold of two stored things is still the decorator's to pass through");
    }

    /// **A shipped default for one key on a row the store already holds**,
    /// under what the operator wrote — same rule prose narrows by, on a
    /// single field instead of a paragraph.
    ///
    /// The missing shape `Provision::record` cannot fill: that one is for an
    /// address the store holds nothing of, and `bot:assistant` (and any
    /// other real identity) always has a row. This is the mechanism
    /// `orientation/charter.rs`'s comment says does not exist yet.
    ///
    /// **Both halves in one read**: the default sits where the operator
    /// wrote nothing, and it stops sitting there — replaced, not
    /// accumulated — the moment the operator writes their own value for the
    /// same key. Without the second half, a decorator that just always
    /// showed the default would pass the first half alone.
    #[tokio::test]
    async fn a_supplied_field_default_sits_under_what_the_operator_wrote() {
        let store = InMemoryMemory::booted();
        let bot = EntityId("bot:gamma".into());
        store
            .add_entity(NewEntity::new(bot.clone(), "Gamma", "jojobot"))
            .await
            .expect("the identity is created")
            .written()
            .expect("an empty board blocks nothing");
        let over = Provisioned::new(
            store,
            Provisions::new(vec![Provision::field(
                bot.clone(),
                "one_liner",
                "The disposable implementer.",
            )]),
        );

        let before = over.fields(&bot).await.expect("fields read");
        assert_eq!(
            before.get("one_liner").map(String::as_str),
            Some("The disposable implementer."),
            "the shipped default answers when the operator wrote nothing: {before:?}",
        );

        over.inner
            .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                fields: [(
                    "one_liner".to_string(),
                    "Files the weekly note.".to_string(),
                )]
                .into_iter()
                .collect(),
                ..NewFact::about(
                    bot.clone(),
                    "gamma's own one-liner",
                    Date::constant(2026, 4, 18),
                )
            }))
            .await
            .expect("capture ok")
            .written()
            .expect("nothing collides");

        let after = over.fields(&bot).await.expect("fields read");
        assert_eq!(
            after.get("one_liner").map(String::as_str),
            Some("Files the weekly note."),
            "the operator's own write replaces the default rather than sitting beside it: {after:?}",
        );
    }

    /// **The three answers an echoed default has to keep apart, in one read
    /// of the store**: a written value equal to today's default is named and
    /// then frozen at exactly what was written, through an upgrade that
    /// changes the default; a key nobody ever wrote keeps following the
    /// default wherever it moves; and a real customisation — a value that
    /// never matched any default — is never named and is unaffected either
    /// way.
    ///
    /// All three from one setup, because each covers how the others pass on
    /// a build that treats every field alike: only the first would pass on a
    /// build that names every key; only the second on a build that freezes
    /// everything at capture time; only the third on a build that names
    /// every write, customisation included.
    #[tokio::test]
    async fn an_echoed_default_is_named_and_frozen_while_an_unwritten_key_still_follows_it() {
        let store = InMemoryMemory::booted();
        let bot = EntityId("bot:gamma".into());
        store
            .add_entity(NewEntity::new(bot.clone(), "Gamma", "jojobot"))
            .await
            .expect("the identity is created")
            .written()
            .expect("an empty board blocks nothing");
        let over = Provisioned::new(
            store,
            Provisions::new(vec![
                Provision::field(bot.clone(), "one_liner", "The disposable implementer."),
                Provision::field(bot.clone(), "boot", "on-demand"),
                Provision::field(bot.clone(), "role", "dev"),
            ]),
        );

        // The caller sends one_liner equal to today's default (the echo,
        // whether deliberate or an accidental round trip) and boot as a real
        // customisation that happens to differ from today's default.
        let sent = BTreeMap::from([
            (
                "one_liner".to_string(),
                "The disposable implementer.".to_string(),
            ),
            ("boot".to_string(), "always-on".to_string()),
        ]);
        let echoed = over.echoed_defaults(&bot, &sent).await;
        assert_eq!(
            echoed,
            vec!["one_liner".to_string()],
            "only the key that equals today's default is named, never the customisation: \
             {echoed:?}",
        );

        over.capture(jojobot_domain::memory::testing::contract::state(NewFact {
            fields: sent,
            ..NewFact::about(
                bot.clone(),
                "gamma's own fields, one of them a deliberate match on today's default",
                Date::constant(2026, 4, 18),
            )
        }))
        .await
        .expect("capture ok")
        .written()
        .expect("nothing collides");

        // The upgrade: a later build, over the same store, shipping different
        // defaults for all three keys. Nothing migrates and nothing runs.
        let upgraded = Provisioned::new(
            over.inner,
            Provisions::new(vec![
                Provision::field(bot.clone(), "one_liner", "An upgraded one-liner."),
                Provision::field(bot.clone(), "boot", "always-on-by-default"),
                Provision::field(bot.clone(), "role", "senior-dev"),
            ]),
        );

        let after = upgraded.fields(&bot).await.expect("fields read");
        assert_eq!(
            after.get("one_liner").map(String::as_str),
            Some("The disposable implementer."),
            "the echoed value stays exactly what was written, through an upgrade that changed \
             the default: {after:?}",
        );
        assert_eq!(
            after.get("boot").map(String::as_str),
            Some("always-on"),
            "the real customisation is unaffected by the upgrade: {after:?}",
        );
        assert_eq!(
            after.get("role").map(String::as_str),
            Some("senior-dev"),
            "a key nobody ever wrote keeps following the default, wherever it moves: {after:?}",
        );

        // The comparison is live, not a memory taken at capture time: sending
        // the ONE echoed value again after the upgrade no longer names it,
        // because it no longer equals what this build ships today.
        let sent_again = BTreeMap::from([(
            "one_liner".to_string(),
            "The disposable implementer.".to_string(),
        )]);
        let echoed_after_upgrade = upgraded.echoed_defaults(&bot, &sent_again).await;
        assert!(
            echoed_after_upgrade.is_empty(),
            "a value that no longer matches any current default must not be named: \
             {echoed_after_upgrade:?}",
        );
    }

    /// **The same echo, over a whole shipped record's own fields — not only a
    /// per-key [`Provision::field`] default.** A `Supplies::Record` carries
    /// its own fields map, folded in at [`fields`](Provisioned::fields)'s own
    /// starting point exactly as a `Field` default is; `echoed_defaults`
    /// shares that same fold now, through `unwritten_defaults`, so it must
    /// see this shape too.
    ///
    /// Paired: an echo of the record's own value is named, and a
    /// customisation of the same key is not — without the second half this
    /// passes on a check that names every write to a supplied record's key.
    #[tokio::test]
    async fn an_echo_of_a_shipped_records_own_field_is_named_and_a_customisation_is_not() {
        let record = shipped_record("my-week");
        let id = EntityId("view:my-week".into());
        let store = InMemoryMemory::booted();
        let over = Provisioned::new(store, Provisions::new(vec![record]));

        let echoing = BTreeMap::from([("selects".to_string(), "rhythm".to_string())]);
        let echoed = over.echoed_defaults(&id, &echoing).await;
        assert_eq!(
            echoed,
            vec!["selects".to_string()],
            "a value equal to the shipped record's own field must be named: {echoed:?}",
        );

        let customised = BTreeMap::from([("selects".to_string(), "person".to_string())]);
        let not_echoed = over.echoed_defaults(&id, &customised).await;
        assert!(
            not_echoed.is_empty(),
            "a value that differs from the shipped record's own field must not be named: \
             {not_echoed:?}",
        );
    }

    /// A record the build ships: kind, handle, and the keys it carries.
    fn shipped_record(slug: &str) -> jojobot_domain::memory::owned::Provision {
        let id = EntityId::new(EntityKind::VIEW, slug);
        jojobot_domain::memory::owned::Provision::record(
            Entity {
                id: id.clone(),
                kind: EntityKind::VIEW,
                name: format!("The {slug} view"),
                aliases: Vec::new(),
                source: "jojobot".into(),
                crm: None,
                parent: None,
                boot: Default::default(),
                merged_into: None,
                badge: None,
                archived: None,
            },
            BTreeMap::from([("selects".to_string(), "rhythm".to_string())]),
        )
    }

    /// **A record the build ships answers every read a stored one answers, and
    /// the store holds nothing of it.**
    ///
    /// Four reads because those are the four the graph query goes through: a
    /// supplied record that answered three of them would be a thing you could
    /// list and not read. **And the store is asked directly at the end** —
    /// without that, a layer that quietly wrote the record down would pass, and
    /// the instance would be frozen on the build that wrote it.
    /// 🚨 **A claim may point at a record the build ships.**
    ///
    /// A read resolves what the build supplies and the write guard read only
    /// the stored rows, so the two halves disagreed about what exists: `recall`
    /// answered for `view:loops` and a claim drawing an edge at it was refused
    /// as an entity nobody knows. **And the way out the refusal offered made it
    /// worse** — a caller doing what it said would mint a stored row for a name
    /// the build already owns, which is the collision the guard exists to
    /// prevent.
    ///
    /// ⚠️ **The pair is that the edge LANDS and resolves**, not merely that the
    /// refusal is gone: a build that waved the write through and dropped the
    /// edge would pass a check for the absence of a block.
    #[tokio::test]
    async fn a_claim_may_point_at_a_record_the_build_ships() {
        let store = InMemoryMemory::booted();
        let supplied = Provisions::new(vec![shipped_record("loops")]);
        let over = Provisioned::new(store.knowing(supplied.clone()), supplied);
        over.add_entity(jojobot_domain::memory::NewEntity::new(
            EntityId::person("person:milhouse"),
            "Milhouse",
            "the operator",
        ))
        .await
        .expect("add_entity ok")
        .written()
        .expect("an empty board blocks nothing");

        let written = over
            .capture(NewFact {
                edge: Some(jojobot_domain::memory::Edge {
                    shape: jojobot_domain::memory::EdgeShape::About,
                    object: EntityId("view:loops".into()),
                }),
                ..NewFact::about(
                    EntityId::person("person:milhouse"),
                    "asked for that view twice this week",
                    Date::constant(2026, 4, 18),
                )
            })
            .await
            .expect("capture ok")
            .written()
            .expect("a claim may point at a record the build ships");

        let edge = written
            .edge
            .expect("the edge landed rather than being dropped on the way in");
        assert_eq!(edge.object, EntityId("view:loops".into()));
        assert_eq!(edge.shape, jojobot_domain::memory::EdgeShape::About);
    }

    #[tokio::test]
    async fn a_record_the_build_ships_answers_like_a_stored_one_and_is_stored_nowhere() {
        // **Both halves told the same set**, as the binary wires it: the store
        // below has to see the supplied record when its own gate asks what
        // exists, and a fixture telling only the layer above is a fixture
        // poorer than the deployment it stands for.
        let supplied = Provisions::new(vec![shipped_record("loops")]);
        let over = Provisioned::new(InMemoryMemory::booted().knowing(supplied.clone()), supplied);
        let id = EntityId("view:loops".into());

        assert!(
            over.list_entities(Some(EntityKind::VIEW))
                .await
                .expect("the listing reads")
                .iter()
                .any(|e| e.id == id),
            "a supplied record is in the listing its kind answers",
        );
        assert_eq!(
            over.fields(&id)
                .await
                .expect("the keys read")
                .get("selects"),
            Some(&"rhythm".to_string()),
            "…and it carries the keys the build gave it",
        );
        assert_eq!(
            over.scan_entity(&id)
                .await
                .expect("the scan reads")
                .expect("the record is there")
                .entity
                .map(|e| e.name),
            Some("The loops view".to_string()),
            "…and it scans like a document",
        );
        assert!(
            over.recall(&id)
                .await
                .expect("recall is not a miss")
                .is_empty(),
            "…and recalling it is an empty page rather than an absence",
        );

        // **The store holds nothing of it.** The claim the whole design rests
        // on, asked of the store itself rather than through the layer that
        // would answer for it either way.
        assert!(
            !over
                .inner
                .list_entities(None)
                .await
                .expect("the store reads")
                .iter()
                .any(|e| e.id == id),
            "the build's record was written down, so this instance is frozen on this build",
        );
    }

    /// **A stored thing's scan answers alike a supplied one's, through the one
    /// layer that resolves both.**
    ///
    /// [`a_record_the_build_ships_answers_like_a_stored_one_and_is_stored_nowhere`]
    /// names the claim in its own title and proves only the supplied half;
    /// this is the stored half beside it, in one call, so the two cannot
    /// drift the way the listing question already had. `scan_entity` derives
    /// from `list_entities`, so this is asked here — the one place it
    /// actually resolves what the build supplies — rather than of the bare
    /// store, which answers rows alone by design.
    #[tokio::test]
    async fn a_stored_things_scan_answers_like_a_supplied_ones() {
        let supplied = Provisions::new(vec![shipped_record("loops")]);
        let over = Provisioned::new(InMemoryMemory::booted().knowing(supplied.clone()), supplied);

        let stored_id = EntityId("thing:contract-scan-stored".into());
        over.add_entity(NewEntity::new(
            stored_id.clone(),
            "Scan Stored",
            "user-named",
        ))
        .await
        .expect("add_entity should succeed")
        .written()
        .expect("nothing collides with it");
        assert_eq!(
            over.scan_entity(&stored_id)
                .await
                .expect("the scan reads")
                .expect("the record is there")
                .entity
                .map(|e| e.name),
            Some("Scan Stored".to_string()),
            "a stored thing must scan through this layer exactly as a supplied one does",
        );

        let supplied_id = EntityId("view:loops".into());
        assert_eq!(
            over.scan_entity(&supplied_id)
                .await
                .expect("the scan reads")
                .expect("the record is there")
                .entity
                .map(|e| e.name),
            Some("The loops view".to_string()),
            "…and the supplied record still scans beside it",
        );
    }

    /// **A key set on a record is in that record's scanned document, whether the
    /// record is stored or the build supplies it.** The index is built from this
    /// scan, and a question that asks by a key and no kind is answered from the
    /// index. A supplied record must therefore scan as ONE document that holds
    /// both the fields the build ships and the keys written onto it.
    #[tokio::test]
    async fn a_key_set_on_a_supplied_record_scans_beside_the_fields_the_build_ships() {
        let supplied = Provisions::new(vec![shipped_record("loops")]);
        let over = Provisioned::new(InMemoryMemory::booted().knowing(supplied.clone()), supplied);
        let stored_id = EntityId("thing:contract-scan-stored".into());
        over.add_entity(NewEntity::new(
            stored_id.clone(),
            "Scan Stored",
            "user-named",
        ))
        .await
        .expect("add_entity should succeed")
        .written()
        .expect("nothing collides with it");
        let supplied_id = EntityId("view:loops".into());

        for subject in [&stored_id, &supplied_id] {
            let mut fact = NewFact::about(
                subject.clone(),
                "a measurement",
                jiff::civil::date(2026, 10, 12),
            );
            fact.fields
                .insert("answer_99".to_string(), "found me".to_string());
            fact.sets.insert("answer_99".to_string());
            over.capture(fact)
                .await
                .expect("the capture lands")
                .written()
                .expect("nothing blocks it");
        }

        let scanned = over.scan().await.expect("the scan reads");
        for subject in [&stored_id, &supplied_id] {
            let docs: Vec<&search::DocScan> = scanned
                .iter()
                .filter(|doc| {
                    doc.entity.as_ref().map(|e| &e.id) == Some(subject)
                        || doc.doc_id == subject.to_string()
                })
                .collect();
            assert_eq!(docs.len(), 1, "{subject} scans as one document: {docs:?}");
            assert_eq!(
                docs[0].entity.as_ref().map(|e| &e.id),
                Some(subject),
                "{subject}'s document names its entity"
            );
            assert_eq!(
                docs[0].fields.get("answer_99").map(String::as_str),
                Some("found me"),
                "{subject}'s document holds the key written onto it: {:?}",
                docs[0].fields
            );
        }
        assert_eq!(
            scanned
                .iter()
                .find(|doc| doc.entity.as_ref().map(|e| &e.id) == Some(&supplied_id))
                .and_then(|doc| doc.fields.get("selects"))
                .map(String::as_str),
            Some("rhythm"),
            "…and the supplied record keeps the fields the build ships"
        );
    }

    /// The shared contract's listing case (rule 234), asked of a stored row
    /// and, over this layer — the one place `list_entities` actually
    /// resolves what the build supplies — a record the build ships, in one
    /// call. Not a rewrite of
    /// [`a_record_the_build_ships_answers_like_a_stored_one_and_is_stored_nowhere`]
    /// above, which is the wider proof over every read; this is the doubled
    /// contract's own case, so the listing question cannot drift from the
    /// stored one the same way the add_entity guard already had.
    #[tokio::test]
    async fn an_existing_thing_appears_in_its_kinds_listing_stored_and_supplied() {
        let supplied = Provisions::new(vec![shipped_record("loops")]);
        let over = Provisioned::new(InMemoryMemory::booted().knowing(supplied.clone()), supplied);
        jojobot_domain::memory::testing::contract::an_existing_thing_appears_in_its_kinds_listing_stored_and_supplied(
            &InMemoryMemory::booted(),
            &over,
        )
        .await;
    }

    /// 🚨 **Asking what sits under a supplied record is an answer, not a
    /// miss.**
    ///
    /// A read that resolves the record while the read beside it says the handle
    /// names nothing is the two halves disagreeing about what exists — the
    /// failure rule 234 is about, met a second time at a second read.
    ///
    /// **Three answers in one case, because the middle one is what a rescue
    /// gets wrong.** A supplied record nothing sits under answers with none; a
    /// supplied record something DOES sit under answers with that child, which
    /// an empty rescue would hide; and a handle nobody has is still a miss.
    #[tokio::test]
    async fn what_sits_under_a_supplied_record_is_an_answer_and_a_missing_handle_is_still_a_miss() {
        let shipped = EntityId("view:loops".into());
        let under = EntityId("view:my-week".into());
        // **Both halves told the same set**, as the binary wires it: the store
        // below has to see the supplied record when its guard asks whether the
        // parent of this write exists.
        let supplied = Provisions::new(vec![shipped_record("loops")]);
        let store = InMemoryMemory::booted().knowing(supplied.clone());
        let over = Provisioned::new(store, supplied);

        assert_eq!(
            over.children(&shipped).await.expect("the read answers"),
            Vec::<EntityId>::new(),
            "a supplied record nothing sits under answers with none rather than a miss",
        );

        // **A child filed under it**, which is what tells a rescue from a read.
        let mut child = NewEntity::new(under.clone(), "My Week", "user-named");
        child.parent = Some(shipped.clone());
        over.add_entity(child)
            .await
            .expect("the operator files one under it")
            .written()
            .expect("nothing collides with it");
        assert_eq!(
            over.children(&shipped).await.expect("the read answers"),
            vec![under],
            "the child under a supplied record came back empty, so the read is a rescue \
             rather than an answer",
        );

        // …and the absence this read exists to report still reports.
        assert!(
            matches!(
                over.children(&EntityId("view:nobody-has-this".into()))
                    .await,
                Err(jojobot_domain::memory::MemoryError::UnknownEntity { .. }),
            ),
            "a handle nobody has stopped being a miss",
        );
    }

    /// **A claim written about a supplied record is in its one document**,
    /// over the fake, with the stored half asked beside it.
    #[tokio::test]
    async fn a_claim_on_a_supplied_record_is_in_its_one_document_over_the_fake() {
        let supplied = Provisions::new(vec![shipped_record("loops")]);
        let over = Provisioned::new(InMemoryMemory::booted().knowing(supplied.clone()), supplied);
        jojobot_domain::memory::testing::contract::a_claim_on_a_supplied_record_is_in_its_one_document(&over)
            .await;
    }

    /// **A stored row for a handle the build later ships stays the one
    /// document**, claims and all: the supplied record neither shadows it nor
    /// joins it as a second.
    #[tokio::test]
    async fn a_stored_row_under_a_supplied_handle_stays_one_document_with_its_claims() {
        let shipped = EntityId("view:loops".into());
        let store = InMemoryMemory::booted();
        store
            .add_entity(NewEntity::new(
                shipped.clone(),
                "Stored Loops",
                "user-named",
            ))
            .await
            .expect("the operator wrote it first")
            .written()
            .expect("nothing shipped it yet");
        let claim = store
            .capture(jojobot_domain::memory::testing::contract::state(
                NewFact::about(
                    shipped.clone(),
                    "a stored zarbleflux habit",
                    jiff::civil::date(2026, 6, 2),
                ),
            ))
            .await
            .expect("the claim lands")
            .written()
            .expect("nothing blocks it");
        let over = Provisioned::new(store, Provisions::new(vec![shipped_record("loops")]));

        let whole = over.scan().await.expect("the scan reads");
        let mine: Vec<_> = whole
            .iter()
            .filter(|d| d.entity.as_ref().is_some_and(|e| e.id == shipped))
            .collect();
        assert_eq!(
            mine.len(),
            1,
            "one document, not a stored one and a supplied one"
        );
        assert_eq!(mine[0].title, "Stored Loops", "the stored row wins");
        assert!(
            mine[0].facts.iter().any(|f| f.address() == claim.address()),
            "the stored document keeps its claim: {:?}",
            mine[0].facts,
        );
        let one = over
            .scan_entity(&shipped)
            .await
            .expect("the one-document read answers")
            .expect("the document exists");
        assert_eq!(one.title, "Stored Loops", "scan_entity agrees");
    }

    /// **A build that stops shipping a record loses it, and what the operator
    /// declared under the same kind is untouched.**
    ///
    /// Paired in one read, or the case passes on a build that empties the
    /// listing.
    #[tokio::test]
    async fn a_record_the_build_stops_shipping_goes_and_the_operators_stays() {
        let store = InMemoryMemory::booted();
        let theirs = EntityId("view:my-week".into());
        store
            .add_entity(NewEntity::new(theirs.clone(), "My Week", "user-named"))
            .await
            .expect("the operator declares their own")
            .written()
            .expect("an empty board blocks nothing");

        let dropped = Provisioned::new(store, Provisions::default());
        let listed = dropped
            .list_entities(Some(EntityKind::VIEW))
            .await
            .expect("the listing reads");
        assert!(
            !listed.iter().any(|e| e.id == EntityId("view:loops".into())),
            "the build stopped shipping it, so the instance stops holding it: {listed:?}",
        );
        assert!(
            listed.iter().any(|e| e.id == theirs),
            "…and what the operator declared is exactly where they left it: {listed:?}",
        );
    }

    /// **A build that stops supplying loses its half, and the operator keeps
    /// theirs.** No reclaim runs, because nothing of the build's was ever
    /// stored.
    #[tokio::test]
    async fn a_build_that_stops_supplying_leaves_the_operators_text_alone() {
        let (store, bot) = provisioned().await;
        store
            .set_prose(&bot, "Gamma files the weekly note.")
            .await
            .expect("the operator writes their own layer");

        let dropped = Provisioned::new(store.inner, Provisions::default());
        assert_eq!(read(&dropped, &bot).await, "Gamma files the weekly note.");
    }
}
