//! Where a URL path and a handle path meet.
//!
//! **A handle is identity and a path is position.** They line up because
//! entities form a tree: an entity's ancestry, root first, spells a path, and
//! that path is its URL. Nothing here reads a store — it is arithmetic over
//! entities the caller already has, which is what makes it testable without one.

use std::collections::HashMap;

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
pub fn canonical_path(entity: &Entity, by_id: &HashMap<&EntityId, &Entity>) -> String {
    let mut chain = vec![entity.id.as_str()];
    let mut walker = entity;
    while let Some(parent) = walker
        .parent
        .as_ref()
        .and_then(|parent| by_id.get(parent).copied())
    {
        if chain.contains(&parent.id.as_str()) {
            break;
        }
        chain.push(parent.id.as_str());
        walker = parent;
        if chain.len() > 64 {
            break;
        }
    }
    chain.reverse();
    format!("/{}/", chain.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jojobot_domain::memory::Boot;

    fn entity(id: &str, parent: Option<&str>) -> Entity {
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
        }
    }

    #[test]
    fn a_path_is_handles_and_nothing_else() {
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

    #[test]
    fn a_parent_that_is_not_there_ends_the_path() {
        // A handle pointing at nothing cannot be walked through, so the path
        // stops at the entity that has it rather than inventing a segment.
        let orphan = entity("topic:widgets", Some("person:ghost"));
        let by_id: HashMap<&EntityId, &Entity> = [(&orphan.id, &orphan)].into_iter().collect();
        assert_eq!(canonical_path(&orphan, &by_id), "/topic:widgets/");
    }
}
