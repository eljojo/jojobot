//! Resolve a selection's parent before matching children against it.

use super::*;

pub(super) async fn parent<M: crate::memory::Memory + ?Sized>(
    store: &M,
    sessions: &[DocScan],
    select: &Selection,
    entities: &[Entity],
    kind_listing: bool,
) -> Result<Option<EntityId>, MemoryError> {
    // **A parent that names nothing is refused the way a subject is**, with the
    // nearest handles, in the one place the handles of a selection are resolved.
    // An empty answer is for a real parent that holds nothing; for a misspelt one
    // it would read as "there is none", and a caller could not repair the typo
    // from it.
    //
    // **The listing stays the kind's own when the parent is of that kind.** A
    // parent of another kind (a project's work) is not in it, so only then is the
    // whole listing asked for, to find the parent or to screen a miss against.
    let Some(parent) = &select.parent else {
        return Ok(None);
    };
    let former = store.former_handles().await?;
    let mut everything: Option<Vec<Entity>> = None;
    let mut found = crate::memory::resolve_handle(parent, entities, &former).map(|e| e.id.clone());
    if found.is_none() && kind_listing {
        let mut all = store.list_entities(None).await?;
        all.extend(sessions.iter().filter_map(|doc| doc.entity.clone()));
        found = crate::memory::resolve_handle(parent, &all, &former).map(|e| e.id.clone());
        everything = Some(all);
    }
    if found.is_none() {
        let screened = everything.as_deref().unwrap_or(entities);
        let readable = readable_for_screen(screened, sessions, select);
        return Err(MemoryError::UnknownEntity {
            attempted: parent.to_string(),
            nearest: guard::screen(parent, &[], &readable),
        });
    }
    Ok(found)
}
