//! A merge and a rename patch the image, and read only the things they touch
//! (work: q-merge-patches-image, rule 498).
//!
//! A merge or a rename can change the document of any thing that spells the
//! handle it moves, so the old code read the whole image again behind the write
//! gate. On a store of some hundreds of things that is thousands of
//! transactions, and every other write and every poll waits for them.
//!
//! **The touched set is worked out from the image, not from the store.** Every
//! document the image holds spells the handles of the things it names, as they
//! were served before the write. A document that spells the handle a write moves
//! is a document the write can change; one that does not, cannot. The set is
//! those documents, the things the write names, and nothing else. It is read
//! from the store in the way the whole image is, one document at a time, so the
//! image still holds what the store answers and never arithmetic of its own.
//!
//! **Where the patch cannot be made, the caller reads the whole image again.**
//! A read that fails leaves the image as it was, and the caller's fallback
//! replaces it.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::sync::atomic::Ordering;

use jojobot_domain::memory::numbered::numbered_handle;
use jojobot_domain::memory::{Entity, EntityId, Fact, FormerHandle, MemoryError, MergeEvent};

use super::{Doc, Image, ask_referring};
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

/// The handles the image held before the write that a change can reach, and
/// the handle each is read under now.
struct Plan {
    /// `(handle held, handle to read)` for each document to read again.
    touched: Vec<(EntityId, EntityId)>,
    /// The names to ask `referring_to` about again, besides those the touched
    /// documents point at.
    names: Vec<String>,
}

/// Everything read from the store for one patch.
struct Reads {
    entities: Vec<Entity>,
    former: Vec<FormerHandle>,
    events: Vec<MergeEvent>,
    docs: Vec<Doc>,
    /// The `referring_to` answers asked again.
    asked: Vec<(String, Vec<Fact>)>,
    /// The names that were asked, answered or not.
    names: Vec<String>,
}

impl Image {
    /// **The documents that spell any of `needles`**, in every part the image
    /// keeps of them. A part is spelled out whole so a handle in a claim's words,
    /// an edge, a field, a pointer or an earlier wording is found wherever it sits.
    fn spelling(&self, needles: &[String]) -> Vec<EntityId> {
        let mut spelled = String::new();
        let mut found = Vec::new();
        for (handle, doc) in &self.docs {
            spelled.clear();
            let _ = write!(
                spelled,
                "{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}",
                doc.scan,
                doc.fields,
                doc.ages,
                doc.histories,
                doc.backing,
                doc.trails,
                doc.unmapped_histories,
                doc.entity_versions,
            );
            if needles
                .iter()
                .any(|needle| spelled.contains(needle.as_str()))
            {
                found.push(handle.clone());
            }
        }
        found.sort();
        found
    }

    fn plan(&self, change: &Change<'_>) -> Plan {
        let (needles, named): (Vec<String>, Vec<(EntityId, EntityId)>) = match change {
            Change::Merged { folded, survivor } => (
                vec![folded.to_string(), survivor.to_string()],
                vec![
                    ((*folded).clone(), (*folded).clone()),
                    ((*survivor).clone(), (*survivor).clone()),
                ],
            ),
            Change::Renamed { from, to } => (
                vec![from.to_string()],
                vec![((*from).clone(), (*to).clone())],
            ),
        };
        let mut touched = named;
        for held in self.spelling(&needles) {
            if !touched.iter().any(|(was, _)| *was == held) {
                touched.push((held.clone(), held));
            }
        }
        Plan {
            touched,
            names: needles,
        }
    }

    /// **Take in what a patch read.** The documents, the listing, the rename
    /// history and the merge events are put in place of the ones held, and each
    /// answer of `referring_to` asked again replaces its entry, or takes it off
    /// when no record points at the name any more.
    fn patched(&mut self, reads: Reads) {
        for doc in reads.docs {
            self.take_in_doc(doc);
        }
        self.take_in_listing(reads.entities);
        self.former = reads
            .former
            .into_iter()
            .filter(|f| numbered_handle(&f.former).is_none())
            .collect();
        self.events = reads.events;
        self.derive_numbered();
        let named: HashSet<String> = self
            .names_in(&self.docs.values().collect::<Vec<_>>())
            .into_iter()
            .collect();
        for name in &reads.names {
            self.referring.remove(name);
        }
        for (name, facts) in reads.asked {
            if named.contains(&name) {
                self.referring.insert(name, facts);
            }
        }
    }
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

    /// **Bring the image up to date with a merge or a rename the store has
    /// made**, reading the touched things and nothing else. The caller holds the
    /// write gate. An error leaves the image as it was.
    pub(in crate::fold) async fn patch(&self, change: Change<'_>) -> Result<(), MemoryError> {
        if !self.wanted.load(Ordering::SeqCst) {
            return Ok(());
        }
        let Some((plan, old_names)) = self.held(|image| {
            let plan = image.plan(&change);
            let old: Vec<&Doc> = plan
                .touched
                .iter()
                .filter_map(|(held, _)| image.docs.get(held))
                .collect();
            let names = image.names_in(&old);
            (plan, names)
        }) else {
            return self.fill().await.map(|_| ());
        };
        let entities = self.inner.list_entities(None).await?;
        let former = self.inner.former_handles().await?;
        let events = self.inner.merge_events(None).await?;
        let mut docs = Vec::with_capacity(plan.touched.len());
        let width = self.inner.read_width().max(1);
        let mut pending = plan.touched.iter();
        loop {
            let mut batch = tokio::task::JoinSet::new();
            for (_, read) in pending.by_ref().take(width) {
                let inner = self.inner.clone();
                let read = read.clone();
                batch.spawn(async move { Doc::load(&*inner, &read).await });
            }
            if batch.is_empty() {
                break;
            }
            while let Some(done) = batch.join_next().await {
                docs.push(done.map_err(|e| MemoryError::Store(e.to_string()))??);
            }
        }
        let mut names = old_names;
        names.extend(plan.names);
        if let Some(reread) = self.held(|image| image.names_in(&docs.iter().collect::<Vec<_>>())) {
            names.extend(reread);
        }
        names.sort();
        names.dedup();
        let asked = ask_referring(&self.inner, names.clone()).await?;
        let reads = Reads {
            entities,
            former,
            events,
            docs,
            asked,
            names,
        };
        let mut slot = self.image.write().expect("fold lock");
        let Some(image) = slot.as_mut() else {
            return Err(MemoryError::Store(
                "the image went while a patch was read for it".into(),
            ));
        };
        image.patched(reads);
        self.generation.fetch_add(1, Ordering::SeqCst);
        Ok(())
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
