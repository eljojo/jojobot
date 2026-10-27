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

use jojobot_domain::clock::Clock;
use jojobot_domain::memory::owned::{
    Provisions, extended, guard_extension, is_overlay_key, overlaid, overlay_fields,
    without_overlay,
};
use jojobot_domain::memory::{
    Archived, ClaimWrite, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactId,
    FactPatch, FieldBacking, FieldWrite, FormerHandle, Guarded, Memory, MemoryError, Merge,
    NewEntity, NewFact, Retraction, WriteSummary, guard, screen_entity_patch, search, types,
    validate_aliases, validate_field,
};

/// A store, plus what this build supplies over it.
pub struct Provisioned<M> {
    inner: M,
    provisions: Provisions,
    /// The clock the day of an overlay's claim is read from.
    clock: Clock,
}

impl<M: Memory + Send + Sync> Provisioned<M> {
    /// Wrap a store with what this build supplies.
    ///
    /// **An empty set is the ordinary case for every row**, so a store with no
    /// provisions over it behaves exactly as the store does.
    pub fn new(inner: M, provisions: Provisions) -> Self {
        Provisioned {
            inner,
            provisions,
            clock: Clock::default(),
        }
    }

    /// **The layer, told which clock it dates the claims it writes with.** The
    /// real one unless the server acts out a day, and the same clock the store
    /// beneath stamps with.
    #[must_use]
    pub fn on_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// **The one place a supplied record meets the operator's name and aliases**
    /// (rule 498): the record as the build ships it, with what the store holds
    /// under the overlay's keys laid over it. Every read that answers for a
    /// supplied record and every edit that screens against one asks this.
    async fn with_overlay(&self, shipped: &Entity) -> Result<Entity, MemoryError> {
        let held = self.inner.fields(&shipped.id).await?;
        Ok(overlaid(shipped, &held))
    }

    /// **The refusal of an overlay key sent as an ordinary field.** The keys are
    /// written by the update that sets a name or aliases, which screens them; a
    /// capture that set them would skip the screen.
    fn refuse_overlay_keys<'a>(
        keys: impl IntoIterator<Item = &'a String>,
    ) -> Result<(), MemoryError> {
        match keys.into_iter().find(|key| is_overlay_key(key)) {
            Some(key) => Err(MemoryError::InvalidFact(format!(
                "'{key}' is a key jojobot keeps a supplied record's name and aliases under, and a \
                 write cannot set it: edit the name or the aliases with update_entity, which \
                 screens them like any other name"
            ))),
            None => Ok(()),
        }
    }

    /// **What a name or aliases edit of a supplied record comes to, once it has
    /// faced the checks and the screen a rename of a stored row faces.** `Ok` is
    /// the fields to store under the overlay's keys and the record as it will
    /// read afterwards; `Err` is the candidates the screen found, and nothing is
    /// written. The record is screened against everything that answers to a name,
    /// the other supplied records with their operator's names included.
    async fn screened_overlay(
        &self,
        shipped: &Entity,
        patch: &EntityPatch,
    ) -> Result<Result<(BTreeMap<String, String>, Entity), Vec<guard::EntityMatch>>, MemoryError>
    {
        if let Some(name) = &patch.name {
            validate_field("name", name)?;
        }
        if let Some(aliases) = &patch.aliases {
            validate_aliases(aliases)?;
        }
        let current = self.with_overlay(shipped).await?;
        let known = self.list_entities(None).await?;
        if let guard::Decision::Block(candidates) = screen_entity_patch(&current, patch, &known) {
            return Ok(Err(candidates));
        }
        let new = overlay_fields(patch.name.as_deref(), patch.aliases.as_deref());
        let mut merged = self.inner.fields(&shipped.id).await?;
        merged.extend(new.clone());
        Ok(Ok((new, overlaid(shipped, &merged))))
    }

    /// **The claim that carries an overlay.** It says what was set and holds the
    /// overlay's keys in its setting bag, so retracting it takes the operator's
    /// name off the record.
    fn overlay_claim(
        &self,
        handle: &EntityId,
        patch: &EntityPatch,
        new: &BTreeMap<String, String>,
    ) -> NewFact {
        let what = match (patch.name.is_some(), patch.aliases.is_some()) {
            (true, true) => "name and aliases",
            (true, false) => "name",
            _ => "aliases",
        };
        let mut fact = NewFact::about(
            handle.clone(),
            format!("Set the {what} of this record."),
            self.clock.today_in(&jiff::tz::TimeZone::UTC),
        );
        fact.session = patch.session.clone();
        fact.fields = new.clone();
        fact.sets = new.keys().cloned().collect();
        fact
    }

    /// The answer that names the supplied record a refused edit collided with.
    fn blocked_by<T>(attempted: &EntityId, supplied: &Entity) -> Guarded<T> {
        Guarded::Blocked {
            attempted: attempted.clone(),
            candidates: vec![guard::EntityMatch {
                handle: supplied.id.clone(),
                kind: supplied.kind,
                name: supplied.name.clone(),
                source: supplied.source.clone(),
                reason: guard::MatchReason::ExactHandle,
                told_apart_by: None,
            }],
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
    ///
    /// **Empty under a stored row at a handle the build ships whole**: the
    /// operator's record is fully theirs and reads back only the keys it
    /// holds (rule 508).
    async fn unwritten_defaults(
        &self,
        entity: &EntityId,
    ) -> Result<BTreeMap<String, String>, MemoryError> {
        if self.provisions.record_for(entity).is_none() {
            return Ok(self.provisions.fields_for(entity));
        }
        let Some((_, supplied)) = self.supplied_only(entity).await? else {
            return Ok(BTreeMap::new());
        };
        let mut defaults = supplied.clone();
        defaults.extend(self.provisions.fields_for(entity));
        Ok(defaults)
    }

    /// **The one "a stored row is here" check** (rule 498). The record the
    /// build ships at this handle, but only while the store holds no row of
    /// its own there: once the operator's row exists, the handle is a stored
    /// thing and every verb treats it as one. Archived and folded rows count
    /// as rows, so a restore still finds the row it restores.
    async fn supplied_only(
        &self,
        handle: &EntityId,
    ) -> Result<Option<(&Entity, &BTreeMap<String, String>)>, MemoryError> {
        let Some(supplied) = self.provisions.record_for(handle) else {
            return Ok(None);
        };
        let stored = self
            .inner
            .list_entities(None)
            .await?
            .iter()
            .any(|row| row.id == *handle);
        Ok((!stored).then_some(supplied))
    }
}

/// Resolve a scanned document's prose and fields, if the build supplies any for
/// it. **The fields fold as [`Provisioned::fields`] folds them**: the shipped
/// per-key defaults sit under what the store holds, because the graph takes a
/// thing's fields from its scan. A stored row at a handle the build ships whole
/// reads back only its own keys (rule 508), so it takes none.
pub(crate) fn resolve_scan(provisions: &Provisions, doc: &mut search::DocScan) {
    let Some(entity) = doc.entity.as_ref() else {
        return;
    };
    if let Some(shipped) = provisions.prose_for(&entity.id) {
        doc.prose = extended(shipped, &doc.prose);
    }
    if provisions.record_for(&entity.id).is_none() {
        let mut folded = provisions.fields_for(&entity.id);
        if !folded.is_empty() {
            folded.extend(std::mem::take(&mut doc.fields));
            doc.fields = without_overlay(folded);
        }
    }
}

mod scans;

#[async_trait]
impl<M: Memory + Send + Sync> Memory for Provisioned<M> {
    fn image_field(&self, entity: &EntityId, key: &str) -> Option<Option<String>> {
        self.inner.image_field(entity, key)
    }

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
            resolve_scan(&self.provisions, doc);
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
    /// `own` by the same function [`resolve_scan`] joins a stored
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
                resolve_scan(&self.provisions, &mut doc);
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
    async fn set_prose(
        &self,
        entity: &EntityId,
        prose: &str,
        session: Option<&str>,
    ) -> Result<String, MemoryError> {
        if let Some(shipped) = self.provisions.prose_for(entity) {
            guard_extension(shipped, prose)?;
        }
        self.inner.set_prose(entity, prose, session).await
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
                    told_apart_by: None,
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
                    told_apart_by: None,
                }],
            });
        }
        Self::refuse_overlay_keys(first.fields.keys())?;
        self.inner.add_entity_with_first_claim(new, first).await
    }

    /// **An app's event faces the screen a creation faces.** The handle is drawn
    /// by the server, but the supplied-handle rule is stated once and an app's
    /// write does not get a way around it. Forwarded and not left to the port's
    /// default, which refuses.
    async fn record_app_event(
        &self,
        event: jojobot_domain::app::AppEvent,
    ) -> Result<jojobot_domain::app::AppOutcome, MemoryError> {
        if let Some((supplied, _)) = self.provisions.record_for(&event.entity.id) {
            return Ok(jojobot_domain::app::AppOutcome::Landed(Box::new(
                Guarded::Blocked {
                    attempted: event.entity.id.clone(),
                    candidates: vec![guard::EntityMatch {
                        handle: supplied.id.clone(),
                        kind: supplied.kind,
                        name: supplied.name.clone(),
                        source: supplied.source.clone(),
                        reason: guard::MatchReason::ExactHandle,
                        told_apart_by: None,
                    }],
                },
            )));
        }
        self.inner.record_app_event(event).await
    }

    /// **A supplied record's name and aliases are the operator's to set, in the
    /// edit that writes a claim as in the edit that does not.**
    ///
    /// **An edit that names nothing of the record itself is a capture.** What the
    /// call sets is written on top of what the build ships, exactly as `capture`
    /// writes it, and the record answered with is the supplied one with the
    /// operator's name laid over it. **A name or aliases in the call are screened
    /// like a rename, and stored in the same claim as the rest of the call**, so
    /// the pair is whole or not at all. A source or crm refuses the whole call,
    /// so the claim is not written either.
    async fn update_entity_with_claim(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
        mut claim: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        Self::refuse_overlay_keys(claim.fields.keys())?;
        if let Some((supplied, _)) = self.supplied_only(handle).await? {
            if patch.source.is_some() || patch.crm.is_some() {
                return Ok(Self::blocked_by(handle, supplied));
            }
            // The same check the store makes before it writes the pair.
            if claim.subject != *handle {
                return Err(MemoryError::InvalidFact(format!(
                    "the claim is about {}, and the thing being edited is {handle}",
                    claim.subject
                )));
            }
            let supplied = supplied.clone();
            let after = if patch.name.is_some() || patch.aliases.is_some() {
                match self.screened_overlay(&supplied, &patch).await? {
                    Ok((new, after)) => {
                        claim.sets.extend(new.keys().cloned());
                        claim.fields.extend(new);
                        after
                    }
                    Err(candidates) => {
                        return Ok(Guarded::Blocked {
                            attempted: handle.clone(),
                            candidates,
                        });
                    }
                }
            } else {
                self.with_overlay(&supplied).await?
            };
            return Ok(match self.inner.capture(claim).await? {
                Guarded::Written(fact) => Guarded::Written((after, fact)),
                Guarded::Blocked {
                    attempted,
                    candidates,
                } => Guarded::Blocked {
                    attempted,
                    candidates,
                },
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
                held.push(self.with_overlay(entity).await?);
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
        let mut folded = self.unwritten_defaults(entity).await?;
        folded.extend(held);
        Ok(without_overlay(folded))
    }

    /// **What the store took in, without the overlay's keys.** A supplied
    /// default is not a write, so it has no moment and no age: what the store
    /// took in is all there is to date.
    async fn field_ages(
        &self,
        entity: &EntityId,
    ) -> Result<jojobot_domain::memory::FieldAges, MemoryError> {
        let mut ages = self.inner.field_ages(entity).await?;
        ages.keys.retain(|key, _| !is_overlay_key(key));
        Ok(ages)
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
        let mut folded = self.unwritten_defaults(entity).await?;
        folded.extend(held);
        Ok((without_overlay(folded), version))
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
        // The trait gives this no error to return. A store that cannot be read names
        // nothing as echoed, and the write itself is unaffected.
        let defaults = self.unwritten_defaults(entity).await.unwrap_or_default();
        let mut echoed: Vec<String> = fields
            .iter()
            .filter(|(key, value)| defaults.get(key.as_str()) == Some(*value))
            .map(|(key, _)| key.clone())
            .collect();
        echoed.sort();
        echoed
    }

    /// **Without the overlay's keys**, like every other read of the fields.
    async fn backing(
        &self,
        entity: &EntityId,
    ) -> Result<BTreeMap<String, FieldBacking>, MemoryError> {
        let mut backing = self.inner.backing(entity).await?;
        backing.retain(|key, _| !is_overlay_key(key));
        Ok(backing)
    }
    async fn built_on(&self, source: &FactAddress) -> Result<Vec<Fact>, MemoryError> {
        self.inner.built_on(source).await
    }
    async fn referring_to(&self, target: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.inner.referring_to(target).await
    }
    fn read_width(&self) -> usize {
        self.inner.read_width()
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
    /// **A supplied record's name and aliases are the operator's to set; its
    /// source and crm stay the build's** (rule 508).
    ///
    /// A name or aliases go through the screen a rename of a stored row faces,
    /// and are kept as the overlay's keys on the record's handle; no row is
    /// made. A source or crm, or an edit that sets nothing, is refused the way
    /// creating a record under that handle is refused, and for the same reason:
    /// the handle is taken, by a record the build supplies.
    ///
    /// ⚠️ **The failure the refusal closes is a guard that could not see what the
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
        if let Some((supplied, _)) = self.supplied_only(handle).await? {
            let names = patch.name.is_some() || patch.aliases.is_some();
            if !names || patch.source.is_some() || patch.crm.is_some() {
                return Ok(Self::blocked_by(handle, supplied));
            }
            let supplied = supplied.clone();
            let (new, after) = match self.screened_overlay(&supplied, &patch).await? {
                Ok(composed) => composed,
                Err(candidates) => {
                    return Ok(Guarded::Blocked {
                        attempted: handle.clone(),
                        candidates,
                    });
                }
            };
            return Ok(
                match self
                    .inner
                    .capture(self.overlay_claim(handle, &patch, &new))
                    .await?
                {
                    Guarded::Written(_) => Guarded::Written(after),
                    Guarded::Blocked {
                        attempted,
                        candidates,
                    } => Guarded::Blocked {
                        attempted,
                        candidates,
                    },
                },
            );
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
        resemblance: jojobot_domain::memory::Override<'_>,
        session: Option<&str>,
    ) -> Result<Guarded<Entity>, MemoryError> {
        if self.supplied_only(from).await?.is_some() {
            return Err(MemoryError::SuppliedHandle {
                attempted: from.to_string(),
            });
        }
        self.inner
            .rename_entity(from, to, parent, date, resemblance, session)
            .await
    }
    /// **The build's own half has no row to mark.** Same shape as
    /// [`rename_entity`](Self::rename_entity), but there is no candidate to
    /// hand back: archiving carries no near-miss screening, so
    /// [`MemoryError::SuppliedHandle`] is the whole of the answer.
    async fn archive_entity(
        &self,
        id: &EntityId,
        reason: &str,
        session: Option<&str>,
    ) -> Result<Entity, MemoryError> {
        if self.supplied_only(id).await?.is_some() {
            return Err(MemoryError::SuppliedHandle {
                attempted: id.to_string(),
            });
        }
        self.inner.archive_entity(id, reason, session).await
    }
    async fn restore_entity(
        &self,
        id: &EntityId,
        session: Option<&str>,
    ) -> Result<(Entity, Archived), MemoryError> {
        if self.supplied_only(id).await?.is_some() {
            return Err(MemoryError::SuppliedHandle {
                attempted: id.to_string(),
            });
        }
        self.inner.restore_entity(id, session).await
    }
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        Self::refuse_overlay_keys(fact.fields.keys())?;
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
        Self::refuse_overlay_keys(patch.fields.keys())?;
        self.inner.update_fact(address, patch, caller).await
    }
    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        self.inner.history(entity, key).await
    }
    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        self.inner.claim_history(address).await
    }
    async fn entity_history(
        &self,
        entity: &EntityId,
    ) -> Result<Vec<jojobot_domain::memory::EntityVersion>, MemoryError> {
        self.inner.entity_history(entity).await
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
    /// **Straight through**: a left-over default here answers no event.
    async fn merge_events(
        &self,
        entity: Option<&EntityId>,
    ) -> Result<Vec<jojobot_domain::memory::MergeEvent>, MemoryError> {
        self.inner.merge_events(entity).await
    }
    /// **Straight through**: a left-over default here answers that nothing moved.
    async fn moved_address(
        &self,
        address: &FactAddress,
    ) -> Result<Option<FactAddress>, MemoryError> {
        self.inner.moved_address(address).await
    }
    /// **Straight through**: a left-over default here answers that nothing is unserved.
    async fn unmapped_claims(
        &self,
        entity: &EntityId,
    ) -> Result<Vec<jojobot_domain::memory::unmapped::UnmappedClaim>, MemoryError> {
        self.inner.unmapped_claims(entity).await
    }
    /// **Straight through**: a left-over default here would answer from this
    /// layer's own `history` and walk the store one key at a time.
    async fn key_trails(
        &self,
        entity: &EntityId,
    ) -> Result<jojobot_domain::memory::KeyTrails, MemoryError> {
        self.inner.key_trails(entity).await
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
            if self.supplied_only(side).await?.is_some() {
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
        by_session: Option<&str>,
    ) -> Result<types::DeclaredType, MemoryError> {
        self.inner.declare_type(declared, by_session).await
    }
    async fn declared_types(&self) -> Result<Vec<types::DeclaredType>, MemoryError> {
        self.inner.declared_types().await
    }
    async fn declared_types_shared(
        &self,
    ) -> Result<std::sync::Arc<Vec<types::DeclaredType>>, MemoryError> {
        self.inner.declared_types_shared().await
    }
    async fn displaced_type(&self, name: &str) -> Result<Option<types::Displaced>, MemoryError> {
        self.inner.displaced_type(name).await
    }
    async fn declare_kind(
        &self,
        token: &str,
        origin: types::Origin,
        fields: Vec<types::Field>,
    ) -> Result<bool, MemoryError> {
        self.inner.declare_kind(token, origin, fields).await
    }
    async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
        self.inner.reclaim_kind(token).await
    }
    async fn declared_kinds(&self) -> Result<Vec<(String, types::Origin)>, MemoryError> {
        self.inner.declared_kinds().await
    }
}

/// **The overlay cases, held to any store.** The unit cases run them over the
/// fake and the real-store suite runs them over Dolt, so a store that composed
/// the operator's name for a read and not for its own screen would fail here
/// (rule 234).
#[cfg(any(test, feature = "testing"))]
pub mod overlay_contract {
    use super::*;
    use jojobot_domain::memory::owned::Provision;

    /// **A record the build ships**: a view with one key.
    pub fn shipped_record(slug: &str) -> Provision {
        shipped_record_as(slug, &format!("The {slug} view"))
    }

    /// **The same record under the name a later build gives it.**
    pub fn shipped_record_as(slug: &str, name: &str) -> Provision {
        let id = EntityId::new(EntityKind::VIEW, slug);
        Provision::record(
            Entity {
                folded_names: Vec::new(),
                id: id.clone(),
                kind: EntityKind::VIEW,
                name: name.to_string(),
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

    /// What the build supplies in the overlay cases: two views.
    pub fn supplied() -> Provisions {
        Provisions::new(vec![shipped_record("loops"), shipped_record("ready")])
    }

    /// The operator's own thing, which a supplied record's name may collide with.
    pub async fn the_operators_own_view<M: Memory>(store: &M) {
        store
            .add_entity(NewEntity::new(
                EntityId("view:my-week".into()),
                "The week",
                "the operator",
            ))
            .await
            .expect("the operator's own view is created")
            .written()
            .expect("an empty board blocks nothing");
    }

    /// **The operator names a supplied record and gives it more names, and every
    /// read shows it.** One update, then the reads that name a record (the
    /// listing, the document, the scan the index ingests and the fields), the
    /// screen a new creation faces and the screen a rename of a stored thing
    /// faces, and the claim that carries the overlay. Paired with the record
    /// before the update, which is the build's own.
    pub async fn the_operator_names_a_supplied_record_and_every_read_shows_it<M: Memory>(over: &M) {
        let id = EntityId("view:loops".into());
        let before = over.list_entities(None).await.expect("lists");
        assert_eq!(
            before.iter().find(|e| e.id == id).map(|e| e.name.as_str()),
            Some("The loops view"),
            "before any update the record is the build's"
        );

        let written = over
            .update_entity(
                &id,
                EntityPatch {
                    name: Some("Weekly rounds".into()),
                    aliases: Some(vec!["Rounds".into()]),
                    ..Default::default()
                },
            )
            .await
            .expect("a name and aliases on a supplied record answer");
        match written {
            Guarded::Written(entity) => {
                assert_eq!(entity.name, "Weekly rounds");
                assert_eq!(entity.aliases, vec!["Rounds".to_string()]);
                assert_eq!(entity.source, "jojobot", "source stays the build's");
            }
            other => panic!("the operator's name lands on a supplied record: {other:?}"),
        }

        let listed = over.list_entities(None).await.expect("lists");
        let there = listed.iter().find(|e| e.id == id).expect("still listed");
        assert_eq!(there.name, "Weekly rounds", "the listing shows it");
        assert_eq!(there.aliases, vec!["Rounds".to_string()]);
        let document = over
            .scan_entity(&id)
            .await
            .expect("scan_entity reads")
            .expect("the supplied record has a document");
        assert_eq!(document.title, "Weekly rounds", "the document's title");
        assert_eq!(
            document.entity.as_ref().map(|e| e.aliases.clone()),
            Some(vec!["Rounds".to_string()]),
            "the document's entity"
        );
        let scanned = over.scan().await.expect("scan reads");
        assert!(
            scanned.iter().any(|d| d
                .entity
                .as_ref()
                .is_some_and(|e| e.id == id && e.name == "Weekly rounds")),
            "the scan the index ingests carries it"
        );
        let held = over.fields(&id).await.expect("fields read");
        assert!(
            held.keys()
                .all(|key| !jojobot_domain::memory::owned::is_overlay_key(key)),
            "a reader never sees the keys the overlay is stored under: {held:?}"
        );
        assert_eq!(held.get("selects").map(String::as_str), Some("rhythm"));
        let backing = over.backing(&id).await.expect("backing reads");
        let ages = over.field_ages(&id).await.expect("ages read");
        assert!(
            backing
                .keys()
                .chain(ages.keys.keys())
                .all(|key| !jojobot_domain::memory::owned::is_overlay_key(key)),
            "no read of the fields shows the keys the overlay is stored under: {backing:?} {ages:?}"
        );
        assert_eq!(
            over.recall(&id).await.expect("recall reads").len(),
            1,
            "the overlay is one claim on the record"
        );

        // The near-miss screen reads the same names a listing does.
        let created = over
            .add_entity(jojobot_domain::memory::NewEntity::new(
                EntityId("view:rounds".into()),
                "Rounds",
                "the operator",
            ))
            .await
            .expect("a creation answers");
        assert!(
            matches!(created, Guarded::Blocked { ref candidates, .. } if candidates.iter().any(|c| c.handle == id)),
            "a new thing under the operator's alias is blocked, naming the record: {created:?}"
        );
        let old = over
            .add_entity(jojobot_domain::memory::NewEntity::new(
                EntityId("view:lantern".into()),
                "The loops view",
                "the operator",
            ))
            .await
            .expect("a creation answers");
        assert!(
            matches!(old, Guarded::Written(_)),
            "the shipped name no longer belongs to the record once the operator named it: {old:?}"
        );

        // The screen an edit of a stored thing faces reads the same names.
        let taken = over
            .update_entity(
                &EntityId("view:my-week".into()),
                EntityPatch {
                    name: Some("Rounds".into()),
                    ..Default::default()
                },
            )
            .await
            .expect("an edit answers");
        assert!(
            matches!(taken, Guarded::Blocked { ref candidates, .. } if candidates.iter().any(|c| c.handle == id)),
            "a stored thing renamed onto the operator's alias is blocked, naming the record: {taken:?}"
        );

        // A second update moves the name and leaves the aliases.
        over.update_entity(
            &id,
            EntityPatch {
                name: Some("Rounds weekly".into()),
                ..Default::default()
            },
        )
        .await
        .expect("answers")
        .written()
        .expect("lands");
        let again = over.list_entities(None).await.expect("lists");
        let there = again.iter().find(|e| e.id == id).expect("listed");
        assert_eq!(
            (there.name.as_str(), there.aliases.clone()),
            ("Rounds weekly", vec!["Rounds".to_string()])
        );
    }
}

#[cfg(test)]
mod tests;
