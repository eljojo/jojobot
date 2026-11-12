//! **What a spelling answers to, and who wrote into a thread**, read from the State
//! alone. A read names a thing by the handle it wears now, one it wore, or a
//! number held under a collection; the State answers all three the way the port's
//! `resolve_handle` does.

use super::super::State;
use crate::log::ObjectId;
use crate::memory::numbered::numbered_handle;
use crate::memory::{Entity, EntityId, EntityKind, types::DeclaredType, writers_in};

impl State {
    /// **The thing a spelling answers to**: the handle it wears now, else one it
    /// wore, else `<collection>/<N>` read through the collection by the same walk.
    /// `None` for a spelling nothing answers to, and for a thing folded into
    /// another, which its survivor serves.
    pub fn resolved_entity(&self, spelling: &EntityId) -> Option<Entity> {
        if let Some(handle) = self.canonical_handle(&spelling.0) {
            return self.entity(&handle);
        }
        let (outer, number) = numbered_handle(spelling)?;
        let container = self.resolved_entity(&outer)?;
        let object = self.resolve(&container.id.0)?;
        let key = self.numbers_by(object)?;
        let member = self.numbered_member(object, &key, number)?;
        self.entity(&self.handle_of(member)?)
    }

    /// **The bots that wrote a thought into a thread**, each once, in the order
    /// the store keeps the bots' rows: by the key each is filed under, its badge or
    /// its handle when it has none. An archived bot writes nothing. A thread
    /// nothing answers to, and one folded into another, have none.
    pub fn thought_writers(&self, thread: &EntityId, declared: &[DeclaredType]) -> Vec<EntityId> {
        let folded = self
            .addresses
            .get(&thread.0)
            .is_some_and(|own| self.redirects.contains_key(own));
        let Some(thread) = self.resolved_entity(thread).filter(|_| !folded) else {
            return Vec::new();
        };
        let mut bots: Vec<(String, &ObjectId)> = self
            .objects
            .iter()
            .filter_map(|(object, body)| {
                let bot = self.entity_of(object, body)?;
                (bot.kind == EntityKind::BOT && bot.archived.is_none())
                    .then(|| (bot.badge.unwrap_or(bot.id.0), object))
            })
            .collect();
        bots.sort();
        let mut writers: Vec<EntityId> = Vec::new();
        for (_, object) in bots {
            let Some(doc) = self.document_scan(object, declared) else {
                continue;
            };
            for writer in writers_in(&thread.id, &doc.facts) {
                if !writers.contains(&writer) {
                    writers.push(writer);
                }
            }
        }
        writers
    }
}
