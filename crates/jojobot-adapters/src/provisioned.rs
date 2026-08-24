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
    ClaimWrite, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactPatch,
    FieldBacking, FieldWrite, Guarded, Memory, MemoryError, Merge, NewEntity, NewFact, Retraction,
    guard, search, types,
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

    /// A record the build supplies, as the document a reader would have got had
    /// the store held it. **No facts**: a supplied record is a thing the
    /// software ships, and a claim about it is somebody's to make.
    fn supplied_scan(&self, entity: &EntityId) -> Option<search::DocScan> {
        let (held, fields) = self.provisions.record_for(entity)?;
        Some(search::DocScan {
            doc_id: held.id.to_string(),
            title: held.name.clone(),
            prose: self
                .provisions
                .prose_for(entity)
                .unwrap_or_default()
                .to_string(),
            entity: Some(held.clone()),
            facts: Vec::new(),
            fields: fields.clone(),
            owner: None,
        })
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
            if !stored && let Some(doc) = self.supplied_scan(&entity.id) {
                scanned.push(doc);
            }
        }
        Ok(scanned)
    }

    async fn scan_entity(&self, entity: &EntityId) -> Result<Option<search::DocScan>, MemoryError> {
        match self.inner.scan_entity(entity).await? {
            Some(mut doc) => {
                self.resolve(&mut doc);
                Ok(Some(doc))
            }
            // The store holds no row, so a supplied record is the whole answer.
            None => Ok(self.supplied_scan(entity)),
        }
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

    async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        // A handle the store does not hold is a miss, which is right for a
        // handle nobody wrote and wrong for a record the software ships.
        let held = match self.inner.fields(entity).await {
            Err(MemoryError::UnknownEntity { .. })
                if self.provisions.record_for(entity).is_some() =>
            {
                BTreeMap::new()
            }
            answer => answer?,
        };
        match self.provisions.record_for(entity) {
            // Supplied keys sit UNDER what the store holds, for the reason
            // prose does: the operator's write is the narrowing one.
            Some((_, supplied)) => {
                let mut folded = supplied.clone();
                folded.extend(held);
                Ok(folded)
            }
            None => Ok(held),
        }
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
    async fn children(&self, parent: &EntityId) -> Result<Vec<EntityId>, MemoryError> {
        self.inner.children(parent).await
    }
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
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        self.inner.capture(fact).await
    }
    /// **A supplied record is not a miss.** Recall of a handle the store does
    /// not hold is an absence rather than an empty page — which is right, and
    /// wrong for a record the software ships: it is there, and nobody has made
    /// a claim about it yet.
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        match self.inner.recall(subject).await {
            Err(MemoryError::UnknownEntity { .. })
                if self.provisions.record_for(subject).is_some() =>
            {
                Ok(Vec::new())
            }
            answer => answer,
        }
    }
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
    ) -> Result<Guarded<Fact>, MemoryError> {
        self.inner.update_fact(address, patch).await
    }
    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        self.inner.history(entity, key).await
    }
    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        self.inner.claim_history(address).await
    }
    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
    ) -> Result<Retraction, MemoryError> {
        self.inner.retract(address, reason, date).await
    }
    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: Date,
    ) -> Result<Merge, MemoryError> {
        self.inner.merge(folded, survivor, reason, date).await
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
        let store = InMemoryMemory::booted();
        let over = Provisioned::new(store, Provisions::new(vec![shipped_record("loops")]));
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
