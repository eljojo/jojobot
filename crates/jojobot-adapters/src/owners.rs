//! **The owner question, answered against Memory** — the production
//! [`OwnerIndex`].
//!
//! A mailbox is created FOR somebody, so the mail store has to ask the entity
//! world one question: does this handle resolve. When both contexts sat in one
//! store the mail adapter answered it by reading the index it already had; they
//! no longer do, so the question crosses a port and this is what stands behind
//! it.
//!
//! **It reads the port, not a store.** Which store holds entities is a wiring
//! decision that has already changed once and will change again — nothing here
//! knows or cares. That is also why this is not in [`crate::outline`]: the
//! entity world sits there today, and the day it moves this file does not.
//!
//! **The whole surface is existence plus what is near it.** It cannot fetch a
//! fact, a kind, an edge or a record, and [`OwnerLookup`] gives it nowhere to
//! put one. Something that wants more than that through here is a different
//! design, and it stops rather than widening this.

use std::sync::Arc;

use async_trait::async_trait;
use jojobot_domain::mailbox::{MailboxError, OwnerIndex, OwnerLookup};
use jojobot_domain::memory::{EntityId, Memory, MemoryError, guard};

/// The [`OwnerIndex`] the server runs: whatever Memory is wired, asked whether
/// a handle is in it.
pub struct MemoryOwners {
    memory: Arc<dyn Memory>,
}

impl MemoryOwners {
    /// Answer the owner question against this Memory.
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        MemoryOwners { memory }
    }
}

#[async_trait]
impl OwnerIndex for MemoryOwners {
    async fn look_up(&self, owner: &EntityId) -> Result<OwnerLookup, MailboxError> {
        // **Every kind, because an owner is any of them.** A box belongs to a
        // bot today and the port says nothing about kind; filtering to the one
        // that happens to own boxes now would answer "no such owner" about an
        // entity that is sitting right there.
        //
        // The read comes first and its failure is returned as one: a store
        // that cannot be reached says nothing about who exists, and reporting
        // that silence as absence refuses a legitimate creation.
        let index = self.memory.list_entities(None).await.map_err(store)?;
        if index.iter().any(|e| &e.id == owner) {
            return Ok(OwnerLookup::Known);
        }
        // Memory's own screen, so the mail context and the entity world can
        // never disagree about what "you probably meant this one" is. No
        // labels: a box names a handle and nothing else, and the slug channels
        // are what a typo travels through.
        Ok(OwnerLookup::Unknown(guard::screen(owner, &[], &index)))
    }
}

/// A Memory failure, as the mail context's own error.
///
/// **One class, because there is one store.** An entity world nobody wired up
/// was a state worth telling apart when entities lived somewhere else; the two
/// rails are the same process now, so a Memory failure here is the retryable
/// store class and nothing else. What matters to the caller's roster is
/// unchanged: a failure is an error, never `Unknown`.
fn store(e: MemoryError) -> MailboxError {
    MailboxError::Store(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jojobot_domain::memory::testing::InMemoryMemory;
    use jojobot_domain::memory::{
        Entity, EntityKind, EntityPatch, Fact, FactAddress, FactPatch, Guarded, NewEntity, NewFact,
        Retraction, search,
    };

    /// A Memory holding exactly these handles, each named after its own slug.
    async fn roster(handles: &[&str]) -> Arc<dyn Memory> {
        let memory = InMemoryMemory::new();
        for handle in handles {
            let id = EntityId((*handle).to_string());
            let name = id.slug().to_string();
            memory
                .add_entity(NewEntity::new(id, name, "the fixture"))
                .await
                .expect("the fake takes the entity")
                .written()
                .expect("the roster's handles do not collide");
        }
        Arc::new(memory)
    }

    /// **A handle that is in the entity world resolves, and nothing else about
    /// it is reported.**
    #[tokio::test]
    async fn a_handle_that_is_in_memory_is_known() {
        let owners = MemoryOwners::new(roster(&["bot:gamma", "person:alpha"]).await);

        assert_eq!(
            owners
                .look_up(&EntityId("bot:gamma".into()))
                .await
                .expect("a reachable entity world answers"),
            OwnerLookup::Known
        );
    }

    /// **A handle that is not in it comes back with the handles it might have
    /// meant** — Memory's own screen, so a typo is answered with the entity it
    /// is one letter from.
    ///
    /// The candidates are the assertion. `Unknown` alone would pass over an
    /// index that screened nothing, and an empty list reads as "nothing even
    /// resembles this" — the one thing that is false here.
    #[tokio::test]
    async fn a_handle_that_is_not_in_memory_is_unknown_with_the_nearby_handles() {
        let owners = MemoryOwners::new(roster(&["bot:gamma", "person:alpha"]).await);

        let found = owners
            .look_up(&EntityId("bot:gamm".into()))
            .await
            .expect("a reachable entity world answers");

        let OwnerLookup::Unknown(candidates) = found else {
            panic!("a handle nobody holds does not resolve: {found:?}");
        };
        assert_eq!(
            candidates
                .iter()
                .map(|c| c.handle.as_str())
                .collect::<Vec<_>>(),
            vec!["bot:gamma"],
            "the near miss comes back, and the unrelated entity does not"
        );
        assert_eq!(candidates[0].reason, guard::MatchReason::NearSlug);
    }

    /// **An entity world that cannot be reached is an error, never "no such
    /// owner".**
    ///
    /// The two are different claims and only one of them is about the caller's
    /// roster. Rendering unreachable as absent refuses a creation that should
    /// have succeeded and tells the caller something false about their own
    /// entities — with an empty candidate list, which reads as "nothing
    /// resembles this" when in fact nothing was looked at.
    #[tokio::test]
    async fn an_entity_world_that_cannot_be_reached_is_an_error_not_an_absence() {
        /// A Memory that can be asked and cannot answer. Only the one read this
        /// port makes is implemented — the rest are not this port's business
        /// and a call to one is the test's own bug.
        struct Down;

        #[async_trait]
        impl Memory for Down {
            async fn list_entities(
                &self,
                _: Option<EntityKind>,
            ) -> Result<Vec<Entity>, MemoryError> {
                Err(MemoryError::Store("the entity world is down".into()))
            }
            async fn add_entity(&self, _: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
                unimplemented!("the owner index only reads the index")
            }
            async fn update_entity(
                &self,
                _: &EntityId,
                _: EntityPatch,
            ) -> Result<Guarded<Entity>, MemoryError> {
                unimplemented!("the owner index only reads the index")
            }
            async fn capture(&self, _: NewFact) -> Result<Guarded<Fact>, MemoryError> {
                unimplemented!("the owner index only reads the index")
            }
            async fn recall(&self, _: &EntityId) -> Result<Vec<Fact>, MemoryError> {
                unimplemented!("the owner index only reads the index")
            }
            async fn update_fact(
                &self,
                _: &FactAddress,
                _: FactPatch,
            ) -> Result<Guarded<Fact>, MemoryError> {
                unimplemented!("the owner index only reads the index")
            }
            async fn retract(
                &self,
                _: &FactAddress,
                _: Option<&str>,
                _: jiff::civil::Date,
            ) -> Result<Retraction, MemoryError> {
                unimplemented!("the owner index only reads the index")
            }
            async fn set_prose(&self, _: &EntityId, _: &str) -> Result<String, MemoryError> {
                unimplemented!("the owner index only reads the index")
            }
            async fn scan(&self) -> Result<Vec<search::DocScan>, MemoryError> {
                unimplemented!("the owner index only reads the index")
            }
        }

        let asked = EntityId("bot:gamma".into());

        let down = MemoryOwners::new(Arc::new(Down));
        let outcome = down.look_up(&asked).await;
        assert!(
            matches!(outcome, Err(MailboxError::Store(_))),
            "a store that cannot be reached is a failure, not a verdict about the owner: \
             {outcome:?}"
        );

        // **The positive it rests on.** The same handle, against an entity world
        // that answers — otherwise this passes over an index that errors on
        // every lookup for any reason at all.
        let up = MemoryOwners::new(roster(&["bot:gamma"]).await);
        assert_eq!(
            up.look_up(&asked).await.expect("a reachable one answers"),
            OwnerLookup::Known
        );
    }
}
