//! Where a URL path and a handle path meet.
//!
//! **A handle is identity and a path is position.** They line up because
//! entities form a tree: an entity's ancestry, root first, spells a path, and
//! that path is its URL. Nothing here reads a store — it is arithmetic over
//! entities the caller already has, which is what makes it testable without one.

use std::collections::{HashMap, HashSet};

use jojobot_domain::memory::{Entity, EntityId};

/// The handles a URL path names, root first, or `None` when a segment is not a
/// handle at all.
///
/// **Shape only.** A well-shaped handle may still name nothing, and the two are
/// told apart where they are answered: "you mistyped it" and "there is no such
/// thing" are the same 404 to a browser but different sentences on the page.
pub fn segments(path: &str) -> Option<Vec<EntityId>> {
    let handles: Vec<EntityId> = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| EntityId(segment.to_string()))
        .collect();
    if handles.is_empty() || handles.iter().any(|handle| handle.kind().is_none()) {
        return None;
    }
    Some(handles)
}

/// Where this entity actually lives, as a URL path with a trailing slash.
///
/// Walked from the child upward, because the pointer lives on the child and
/// that is the only place parentage is recorded.
///
/// **Bounded rather than trusting.** A parent chain is single-parent and fixed
/// at creation, so a cycle is unreachable through the write path — but this
/// reads whatever the store holds, and a page that hangs on a hand-edited
/// record is worse than one that shows a short path.
///
/// **The seen-handle check is the whole bound.** Every step appends a handle
/// the chain does not already hold, so the walk stops after the entities the
/// caller passed in, however they are wired. A depth limit behind it could only
/// fire on a chain of that many *distinct* ancestors — a real tree, whose real
/// path it would cut short and call a loop.
///
/// The check is a set beside the chain, not a scan of it: this runs once per
/// entity in a listing, and a scan per step makes the page quadratic in the
/// depth of the deepest ancestry it holds.
pub fn canonical_path(entity: &Entity, by_id: &HashMap<&EntityId, &Entity>) -> String {
    let mut chain = vec![entity.id.as_str()];
    let mut seen: HashSet<&str> = HashSet::from([entity.id.as_str()]);
    let mut walker = entity;
    while let Some(parent) = walker
        .parent
        .as_ref()
        .and_then(|parent| by_id.get(parent).copied())
    {
        if !seen.insert(parent.id.as_str()) {
            break;
        }
        chain.push(parent.id.as_str());
        walker = parent;
    }
    chain.reverse();
    format!("/{}/", chain.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jojobot_domain::memory::{Boot, EntityKind};

    fn entity(id: &str, parent: Option<&str>) -> Entity {
        // **The fixture stands a store up, because the set is setup here.**
        // Reading a handle asks the kinds this process loaded, and no case
        // behind this fixture asserts anything about the set — so the set
        // arrives the way a boot delivers it, from what a store holds.
        let _booted = jojobot_domain::memory::testing::InMemoryMemory::booted();
        let id = EntityId(id.to_string());
        Entity {
            kind: id.kind().expect("a fixture handle names its kind"),
            name: id.as_str().to_string(),
            id,
            aliases: Vec::new(),
            source: "a test".to_string(),
            crm: None,
            parent: parent.map(|p| EntityId(p.to_string())),
            boot: Boot::default(),
            merged_into: None,
        }
    }

    #[test]
    fn a_path_is_handles_and_nothing_else() {
        // **The set is this case's subject, not its setup.** What makes
        // `/not-a-handle/` name nothing is that its first segment carries no
        // kind the set holds, so the answer below is a statement about the
        // loaded set as much as about the path grammar.
        jojobot_domain::memory::kinds::load_shipped();
        assert_eq!(
            segments("/person:alpha/topic:widgets/"),
            Some(vec![
                EntityId("person:alpha".into()),
                EntityId("topic:widgets".into())
            ])
        );
        // A segment naming no kind is not a handle, so it names nothing here.
        assert_eq!(segments("/not-a-handle/"), None);
        assert_eq!(segments("/person:alpha/nope/"), None);
        assert_eq!(segments("//"), None);
    }

    #[test]
    fn a_path_is_the_ancestry_root_first() {
        let root = entity("person:alpha", None);
        let child = entity("topic:widgets", Some("person:alpha"));
        let grandchild = entity("thing:sigma", Some("topic:widgets"));
        let by_id: HashMap<&EntityId, &Entity> = [&root, &child, &grandchild]
            .map(|e| (&e.id, e))
            .into_iter()
            .collect();

        assert_eq!(canonical_path(&root, &by_id), "/person:alpha/");
        assert_eq!(
            canonical_path(&grandchild, &by_id),
            "/person:alpha/topic:widgets/thing:sigma/"
        );
    }

    #[test]
    fn a_parent_chain_that_loops_stops_rather_than_hangs() {
        let one = entity("person:x", Some("person:y"));
        let two = entity("person:y", Some("person:x"));
        let by_id: HashMap<&EntityId, &Entity> =
            [&one, &two].map(|e| (&e.id, e)).into_iter().collect();

        // A record no write path can produce, and a page that hung on it would
        // be a worse answer than a short path.
        assert_eq!(canonical_path(&one, &by_id), "/person:y/person:x/");
    }

    /// A rung of a chain too long to spell out — built through the constructor
    /// rather than written, so these indices are not handles in this source for
    /// the roster gate to read as names.
    fn step(n: usize) -> EntityId {
        EntityId::new(EntityKind::TOPIC, format!("step-{n:03}"))
    }

    /// `count` entities in one line, oldest first. `wrap` points the oldest at
    /// the newest, which closes the line into a loop no write path produces.
    fn line(count: usize, wrap: bool) -> Vec<Entity> {
        (0..count)
            .map(|n| {
                let parent = match n {
                    0 if wrap => Some(step(count - 1)),
                    0 => None,
                    _ => Some(step(n - 1)),
                };
                entity(step(n).as_str(), parent.as_ref().map(EntityId::as_str))
            })
            .collect()
    }

    fn by_id(entities: &[Entity]) -> HashMap<&EntityId, &Entity> {
        entities.iter().map(|e| (&e.id, e)).collect()
    }

    fn depth(path: &str) -> usize {
        path.split('/').filter(|s| !s.is_empty()).count()
    }

    /// **An ancestry deeper than any depth limit a cycle check would carry.**
    /// Such a limit fires only here — on a real chain, whose path it cuts to
    /// its last handles and then serves as the canonical one: a URL, and a
    /// heading, stating an ancestry the record does not have.
    #[test]
    fn a_deep_ancestry_keeps_every_handle() {
        let deep = line(70, false);
        let by_id = by_id(&deep);
        let path = canonical_path(deep.last().expect("70 entities"), &by_id);

        assert_eq!(depth(&path), 70, "every ancestor is a segment: {path}");
        assert!(path.starts_with(&format!("/{}/", step(0))), "{path}");
        assert!(path.ends_with(&format!("/{}/", step(69))), "{path}");
    }

    /// **The case the depth limit was supposed to be for, with it gone.** A
    /// loop longer than that limit still ends: the walk stops at the first
    /// handle it already holds, so the entities the caller passed in are its
    /// bound.
    #[test]
    fn a_loop_longer_than_the_old_depth_limit_still_ends() {
        let looped = line(70, true);
        let by_id = by_id(&looped);
        let path = canonical_path(&looped[0], &by_id);

        assert_eq!(depth(&path), 70, "each handle once, then it stops: {path}");
        assert!(path.ends_with(&format!("/{}/", step(0))), "{path}");
        // Counting the segments is not enough on its own: a walk that took a
        // handle twice and dropped another would count the same. What bounds
        // this walk is that no handle is ever revisited, so assert that.
        let walked: HashSet<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        assert_eq!(walked.len(), 70, "a handle was walked twice: {path}");
    }

    #[test]
    fn a_parent_that_is_not_there_ends_the_path() {
        // A handle pointing at nothing cannot be walked through, so the path
        // stops at the entity that has it rather than inventing a segment.
        let orphan = entity("topic:widgets", Some("person:ghost"));
        let by_id: HashMap<&EntityId, &Entity> = [(&orphan.id, &orphan)].into_iter().collect();
        assert_eq!(canonical_path(&orphan, &by_id), "/topic:widgets/");
    }
}
