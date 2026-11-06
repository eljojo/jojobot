//! **The claims State the fake holds.** A write of the fake makes the events the
//! dolt writers make, through the domain's functions, and applies them here, so
//! the State answers what the fake answers for the thing it touched (rule
//! 9.3.3-d (a) and (b)). This file is the foundation only: the State, the one
//! call that applies events to it, and the key a handle stands for. The writers
//! that call it are the slices of 9.3.3-d.

use super::*;
use crate::log::claims::{ClaimEvent, State};
use crate::log::{Event, ObjectId};

impl InMemoryMemory {
    /// **Apply the events of one write to the claims State**, in the order they
    /// were made.
    // No writer calls it until the entity and claim slices of 9.3.3-d land.
    #[allow(dead_code)]
    pub(super) fn emit(&self, events: impl IntoIterator<Item = Event<ClaimEvent>>) {
        let mut state = self.claims.lock().expect("fake mutex poisoned");
        for event in events {
            state.apply(&event);
        }
    }

    /// **The permanent key a handle stands for**: the key the fake files the thing
    /// under, as the dolt store's badge is. A name that stands for none keeps its
    /// spelling, as the import's reader keeps it.
    // No writer calls it until the entity and claim slices of 9.3.3-d land.
    #[allow(dead_code)]
    pub(super) fn object_key(&self, id: &EntityId) -> ObjectId {
        ObjectId(
            self.storage_key(id)
                .map_or_else(|| id.as_str().to_string(), |key| key.as_str().to_string()),
        )
    }

    /// **Read the claims State**, as the seam's `read_claims` lets a case read it.
    pub fn read_claims<T>(&self, read: impl FnOnce(&State) -> T) -> T {
        read(&self.claims.lock().expect("fake mutex poisoned"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::claims::{KindId, ObjectBody};

    /// **Events applied to the fake are what its State answers.**
    #[test]
    fn events_applied_to_the_fake_are_what_its_state_answers() {
        let fake = InMemoryMemory::new();
        let created = Event {
            event_kind: String::new(),
            meaning_version: 1,
            actor: None,
            run: None,
            session_card: None,
            request: None,
            part: None,
            occurred_at: None,
            written_at: None,
            imported_source_time: None,
            payload: ClaimEvent::ObjectCreated {
                object: ObjectId("k1".into()),
                body: ObjectBody {
                    kind: KindId("person".into()),
                    parent: None,
                    name: "person:homer".into(),
                    source: "fixture".into(),
                    aliases: Vec::new(),
                    boot: None,
                    metadata: Default::default(),
                    archived: false,
                    bound_to: None,
                },
            },
        };
        assert!(fake.read_claims(|state| state.object(&ObjectId("k1".into())).is_none()));
        fake.emit([created]);
        assert!(fake.read_claims(|state| state.object(&ObjectId("k1".into())).is_some()));
    }

    /// **A handle stands for the key the fake files its thing under**, and a name
    /// no thing wears keeps its spelling.
    #[tokio::test]
    async fn a_handle_stands_for_the_key_the_fake_files_its_thing_under() {
        let fake = InMemoryMemory::booted();
        let homer = EntityId("person:homer".into());
        fake.add_entity(NewEntity::new(homer.clone(), "Homer", "fixture"))
            .await
            .expect("add")
            .written()
            .expect("written");
        let key = fake.storage_key(&homer).expect("the fake files the thing");
        assert_eq!(fake.object_key(&homer), ObjectId(key.as_str().to_string()));
        let unknown = EntityId("person:marge".into());
        assert_eq!(fake.object_key(&unknown), ObjectId("person:marge".into()));
    }
}
