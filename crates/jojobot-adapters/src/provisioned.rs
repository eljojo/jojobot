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

use jojobot_domain::memory::owned::{Provisions, Slot, extended, guard_extension};
use jojobot_domain::memory::{
    Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactPatch, FieldBacking,
    FieldWrite, Guarded, Memory, MemoryError, NewEntity, NewFact, Retraction, search, types,
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
        if let Some(shipped) = self.provisions.at(&entity.id, &Slot::Prose) {
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
        if !self.provisions.is_empty() {
            for doc in &mut scanned {
                self.resolve(doc);
            }
        }
        Ok(scanned)
    }

    async fn scan_entity(&self, entity: &EntityId) -> Result<Option<search::DocScan>, MemoryError> {
        let mut scanned = self.inner.scan_entity(entity).await?;
        if let Some(doc) = scanned.as_mut() {
            self.resolve(doc);
        }
        Ok(scanned)
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
        if let Some(shipped) = self.provisions.at(entity, &Slot::Prose) {
            guard_extension(shipped, prose)?;
        }
        self.inner.set_prose(entity, prose).await
    }

    // ── everything else is the store's, unchanged ───────────────────────────

    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        self.inner.add_entity(new).await
    }
    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        self.inner.list_entities(kind).await
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
    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.inner.update_entity(handle, patch).await
    }
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        self.inner.capture(fact).await
    }
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.inner.recall(subject).await
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
    async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        self.inner.fields(entity).await
    }
    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
    ) -> Result<Retraction, MemoryError> {
        self.inner.retract(address, reason, date).await
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
        let bot = EntityId::new(EntityKind::BOT, "gamma");
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
        let other = EntityId::new(EntityKind::BOT, "delta");
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
