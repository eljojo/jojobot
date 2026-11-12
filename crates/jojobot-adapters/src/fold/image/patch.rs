//! A merge and a rename follow the image by reading the lists they change
//! (work: q-merge-patches-image, rule 498).
//!
//! A merge or a rename moves a handle, so the listing, the rename history and the
//! merge events are read again from the store. No document is held, so none is
//! read: the claims State has the write and builds a thing's document when a
//! read asks for it.
//!
//! **The things told to the index are the ones the write names and the ones that
//! point at the handle it moves**, worked out from the State's records and not
//! from a document held.
//!
//! **Where the lists cannot be read, the caller reads the whole image again.**
//! A read that fails leaves the image as it was, and the caller's fallback
//! replaces it.

use std::sync::atomic::Ordering;

use jojobot_domain::memory::{EntityId, MemoryError};

use crate::fold::Folded;

/// What the store has done, as the image has to follow it.
pub(in crate::fold) enum Change<'a> {
    /// `folded` was folded into `survivor`.
    Merged {
        folded: &'a EntityId,
        survivor: &'a EntityId,
    },
    /// The thing that wore `from` wears `to`.
    Renamed {
        from: &'a EntityId,
        to: &'a EntityId,
    },
}

impl Folded {
    /// **The patch, or the whole image read again where the patch cannot be
    /// made.** The error is the fallback's own, so the caller answers a write
    /// that landed with an image that could not follow it exactly as it did when
    /// every merge and rename read the whole image.
    pub(in crate::fold) async fn patched_or_reloaded(
        &self,
        change: Change<'_>,
    ) -> Result<(), MemoryError> {
        match self.patch(change).await {
            Ok(()) => Ok(()),
            Err(error) => {
                tracing::warn!(
                    %error,
                    "the image could not be patched, so it is read again whole"
                );
                self.reload().await
            }
        }
    }

    /// **The things a change names and the things that point at them**, each once.
    fn touched_by(&self, change: &Change<'_>) -> Vec<EntityId> {
        let named: Vec<&EntityId> = match change {
            Change::Merged { folded, survivor } => vec![folded, survivor],
            Change::Renamed { from, to } => vec![from, to],
        };
        let mut touched: Vec<EntityId> = named.iter().map(|id| (*id).clone()).collect();
        for id in named {
            if let Ok(holders) = self.claims(|state| state.holders_of(id)) {
                for holder in holders {
                    if !touched.contains(&holder) {
                        touched.push(holder);
                    }
                }
            }
        }
        touched
    }

    /// **Bring the image up to date with a merge or a rename the store has
    /// made**, reading the lists it changes and nothing else. The caller holds the
    /// write gate. An error leaves the image as it was.
    pub(in crate::fold) async fn patch(&self, change: Change<'_>) -> Result<(), MemoryError> {
        if !self.wanted.load(Ordering::SeqCst) {
            self.publish_things(Vec::new(), true);
            return Ok(());
        }
        if self.held(|_| ()).is_none() {
            return self.fill().await.map(|_| ());
        }
        let entities = self.inner.list_entities(None).await?;
        let former = self.inner.former_handles().await?;
        let events = self.inner.merge_events(None).await?;
        let mut slot = self.image.write().expect("fold lock");
        let Some(image) = slot.as_mut() else {
            return Err(MemoryError::Store(
                "the image went while a patch was read for it".into(),
            ));
        };
        image.patched(entities, former, events);
        self.generation.fetch_add(1, Ordering::SeqCst);
        drop(slot);
        let touched = self.touched_by(&change);
        self.publish_things(touched, true);
        Ok(())
    }
}

impl super::Image {
    /// **Take in what a patch read.** The listing, the rename history and the
    /// merge events are put in place of the ones held.
    fn patched(
        &mut self,
        entities: Vec<jojobot_domain::memory::Entity>,
        former: Vec<jojobot_domain::memory::FormerHandle>,
        events: Vec<jojobot_domain::memory::MergeEvent>,
    ) {
        self.install(entities);
        self.former = former
            .into_iter()
            .filter(|f| jojobot_domain::memory::numbered::numbered_handle(&f.former).is_none())
            .collect();
        self.events = events;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use jiff::civil::date;
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::memory::{EntityId, EntityKind, Memory, NewEntity, NewFact, Override};

    use crate::counted::Counted;
    use crate::fold::Folded;

    #[tokio::test]
    async fn a_numbered_rename_refreshes_the_members_reference_holders() {
        let folded = Folded::new(Arc::new(InMemoryMemory::booted()));
        folded.rebuild().await.expect("the image fills");
        jojobot_domain::memory::testing::contract::typed_kinds::b5_numbered_rename(&folded).await;
    }

    /// A word of letters that depends on nothing but `n`, spread far enough
    /// apart that the write guard's near-miss screen takes no two for one.
    fn word(n: usize) -> String {
        let mut n = n * 7919 + 104_729;
        let mut letters = String::new();
        for _ in 0..8 {
            letters.push((b'a' + (n % 26) as u8) as char);
            n = n / 26 * 31 + 17;
        }
        letters
    }

    fn thing(n: usize) -> EntityId {
        EntityId::new(EntityKind::THING, word(n))
    }

    /// A fold over a store of `unrelated` things nothing points at, and the two
    /// persons a case merges or renames. Each holds a claim, and a third thing
    /// points at the first.
    async fn fold_of(unrelated: usize) -> (Arc<Counted>, Folded) {
        let counted = Arc::new(Counted::new(Arc::new(InMemoryMemory::booted())));
        let folded = Folded::new(counted.clone());
        folded.rebuild().await.expect("the fold fills");
        for id in [
            EntityId::person("person:alpha"),
            EntityId::person("person:beta"),
        ] {
            folded
                .add_entity(NewEntity::new(id.clone(), id.to_string(), "user-named"))
                .await
                .expect("an entity is added")
                .written()
                .expect("not blocked");
            folded
                .capture(NewFact::about(id.clone(), "a claim", date(2026, 9, 1)))
                .await
                .expect("a claim lands")
                .written()
                .expect("not blocked");
        }
        for n in 0..unrelated {
            folded
                .add_entity(NewEntity::new(thing(n), word(n + 1000), "user-named"))
                .await
                .expect("an entity is added")
                .written()
                .expect("not blocked");
        }
        (counted, folded)
    }

    /// **The reads a merge makes do not grow with the things it does not
    /// touch.** The same merge over a store twice as large asks the store for as
    /// many reads.
    #[tokio::test]
    async fn a_merge_reads_the_store_as_much_over_a_large_store_as_over_a_small_one() {
        let mut reads = Vec::new();
        for unrelated in [20, 40] {
            let (counted, folded) = fold_of(unrelated).await;
            counted.reset();
            folded
                .merge(
                    &EntityId::person("person:beta"),
                    &EntityId::person("person:alpha"),
                    Some("filed twice"),
                    date(2026, 9, 2),
                    &EntityId("bot:sigma".into()),
                    None,
                )
                .await
                .expect("the merge lands");
            reads.push((unrelated, counted.calls(), counted.seen()));
        }
        assert_eq!(
            reads[0].1, reads[1].1,
            "the merge's reads grew with the store: {reads:?}"
        );
        for (unrelated, _, seen) in &reads {
            assert_eq!(
                seen.get("scan"),
                None,
                "a merge over {unrelated} things scanned every document: {seen:?}"
            );
        }
    }

    /// **The same holds for a rename.**
    #[tokio::test]
    async fn a_rename_reads_the_store_as_much_over_a_large_store_as_over_a_small_one() {
        let mut reads = Vec::new();
        for unrelated in [20, 40] {
            let (counted, folded) = fold_of(unrelated).await;
            counted.reset();
            folded
                .rename_entity(
                    &EntityId::person("person:beta"),
                    &EntityId::person("person:gamma"),
                    None,
                    date(2026, 9, 2),
                    Override::none(),
                    None,
                )
                .await
                .expect("the rename lands")
                .written()
                .expect("not blocked");
            reads.push((unrelated, counted.calls(), counted.seen()));
        }
        assert_eq!(
            reads[0].1, reads[1].1,
            "the rename's reads grew with the store: {reads:?}"
        );
        for (unrelated, _, seen) in &reads {
            assert_eq!(
                seen.get("scan"),
                None,
                "a rename over {unrelated} things scanned every document: {seen:?}"
            );
        }
    }
}
