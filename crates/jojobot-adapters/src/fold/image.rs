//! [`Image`] — what the memory verbs read that the claims State does not hold,
//! held in RAM (decision log 292).
//!
//! jojobot is one server instance and the only writer, so the image can be the
//! answer to a read rather than a cache in front of one. It is filled from the
//! store's own reads, **never from arithmetic of its own**: a value held here is
//! the value the store above it gave when it was asked. What is held is the
//! listing, the rename history and the merge events. No document is held: a
//! thing's document, fields and history are built from the claims State when a
//! read asks for them.

use std::collections::HashMap;

use jojobot_domain::memory::claim_walk::{MergeLoop, follow_claim, moves_in};
use jojobot_domain::memory::numbered::numbered_handle;
use jojobot_domain::memory::{
    Entity, EntityId, EntityKind, FactAddress, FormerHandle, Memory, MemoryError,
};

pub(super) mod patch;

/// The whole image.
pub(super) struct Image {
    entities: Vec<Entity>,
    /// **The rename history the store holds**, the handles the held numbers make
    /// not among them: those are derived from the State when a read asks.
    former: Vec<FormerHandle>,
    /// Position in `entities` of the thing wearing each handle now.
    by_id: HashMap<EntityId, usize>,
    /// Position of the first thing wearing each badge — the tier a former
    /// handle resolves through, taken in listing order as the port's own
    /// `resolve_handle` takes it.
    by_badge: HashMap<String, usize>,
    /// **Every merge made, as the store appended it**, with the handles as they
    /// are served at the moment of the fill. A rename or a merge reads the events
    /// again (see [`patch`]), so the handles are as current as the image.
    events: Vec<jojobot_domain::memory::MergeEvent>,
}

/// **One phase of the fill, named with the time it took and how many it held.** The
/// fill is the longest step of a boot and used to log as one line, so a restart
/// that took minutes could not say which read took them. The count is what grows
/// with the store: things listed, merges held.
fn fill_phase(phase: &'static str, started: std::time::Instant, count: usize) {
    tracing::info!(
        phase,
        count,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "fold fill phase finished"
    );
}

impl Image {
    /// Fill the image from the store's lists — the startup path. No document is
    /// read: the State serves them.
    pub async fn load(inner: &dyn Memory) -> Result<Image, MemoryError> {
        let started = std::time::Instant::now();
        let entities = inner.list_entities(None).await?;
        let former = inner.former_handles().await?;
        fill_phase("lists", started, entities.len());
        let mut image = Image::assembled(entities, former);
        let started = std::time::Instant::now();
        image.events = inner.merge_events(None).await?;
        fill_phase("merge-events", started, image.events.len());
        Ok(image)
    }

    pub(super) fn assembled(entities: Vec<Entity>, former: Vec<FormerHandle>) -> Image {
        // The store's list of former handles carries the numbered ones it
        // derived at the moment of the read; they are derived again from the State
        // when a read asks, so they stay current as the fields change.
        let former = former
            .into_iter()
            .filter(|f| numbered_handle(&f.former).is_none())
            .collect();
        let mut image = Image {
            entities,
            former,
            by_id: HashMap::new(),
            by_badge: HashMap::new(),
            events: Vec::new(),
        };
        image.reindex();
        image
    }

    fn reindex(&mut self) {
        self.by_id.clear();
        self.by_badge.clear();
        for (at, entity) in self.entities.iter().enumerate() {
            self.by_id.entry(entity.id.clone()).or_insert(at);
            if let Some(badge) = &entity.badge {
                self.by_badge.entry(badge.clone()).or_insert(at);
            }
        }
    }

    /// Put the listing read in the place of the one held.
    pub fn install(&mut self, entities: Vec<Entity>) {
        self.entities = entities;
        self.reindex();
    }

    /// The thing a handle answers to: the handle itself first, then the handle
    /// it used to wear, exactly as the port's `resolve_handle` goes, and a chain
    /// of numbers read through every collection by the same walk.
    fn entity(&self, id: &EntityId) -> Option<&Entity> {
        jojobot_domain::memory::numbered::resolve_walk(id, &|id| {
            if let Some(at) = self.by_id.get(id) {
                return self.entities.get(*at);
            }
            let badge = &self.former.iter().find(|f| &f.former == id)?.badge;
            self.by_badge
                .get(badge)
                .and_then(|at| self.entities.get(*at))
        })
    }

    /// How many things the image holds.
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn list(&self, kind: Option<EntityKind>) -> Vec<Entity> {
        self.entities
            .iter()
            .filter(|e| kind.is_none_or(|k| e.kind == k))
            .cloned()
            .collect()
    }

    /// The rename history the store holds, without the numbered handles.
    pub fn former(&self) -> Vec<FormerHandle> {
        self.former.clone()
    }

    /// **Where an address went when its thing was merged away**, followed
    /// through a survivor merged later; `None` when no event moved it.
    pub fn moved(&self, address: &FactAddress) -> Result<Option<FactAddress>, MergeLoop> {
        let Some(home) = self.entity(&address.home).map(|entity| entity.id.clone()) else {
            return Ok(None);
        };
        let moved =
            follow_claim::<_, MergeLoop>(home, address.local.clone(), moves_in(&self.events))?;
        Ok(moved.map(|(survivor, now)| FactAddress::new(survivor, now)))
    }
}
