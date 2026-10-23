use super::*;

/// 🚨 **A child's parent pointer follows a rename, resolved on the way
/// out — the same shape an edge and a ref already get, not the shape a
/// claim's home or an alias got.**
///
/// **`parent` is never a lookup key.** `children` reads every entity and
/// filters in memory rather than querying by the column, so nothing here
/// needs the storage stability a badge buys — only what a reader sees
/// needs to change, which is what resolving on read is for.
///
/// **`children` itself is the proof that matters**, not just the field:
/// a fix that only patched `Entity::parent` and left the reverse lookup
/// reading the raw stored value would still lose a session asking "what
/// is under this thing now."
pub async fn a_childs_parent_pointer_follows_a_rename<M: Memory + ?Sized>(
    store: &M,
    rehandles: &dyn Rehandles,
) {
    let was = EntityId("person:contract-parent-was".into());
    let now = EntityId("work:contract-parent-now".into());
    store
        .add_entity(NewEntity::new(
            was.clone(),
            "Contract Parent Was",
            "the roster",
        ))
        .await
        .expect("the fixture is written")
        .written()
        .expect("nothing collides with it");
    let badge = store
        .list_entities(None)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|e| e.id == was)
        .expect("the fixture is there")
        .badge
        .expect("a written row wears a badge");

    let child = EntityId("person:contract-parent-child".into());
    store
        .add_entity(NewEntity {
            parent: Some(was.clone()),
            ..NewEntity::new(child.clone(), "Contract Parent Child", "the roster")
        })
        .await
        .expect("the fixture is written")
        .written()
        .expect("nothing collides with it");

    rehandles.rehandle(&was, &now).await;
    rehandles
        .note_former_handle(FormerHandle {
            former: was.clone(),
            badge,
            changed_at: date(2026, 4, 19),
        })
        .await;

    let held = store
        .list_entities(None)
        .await
        .expect("the store answers")
        .into_iter()
        .find(|e| e.id == child)
        .expect("the child is there");
    assert_eq!(
        held.parent.as_ref(),
        Some(&now),
        "the child's parent did not follow the rename: {held:?}",
    );

    let kids = store
        .children(&now)
        .await
        .expect("children reads under the current handle");
    assert_eq!(
        kids,
        vec![child],
        "the reverse lookup did not find the child through the renamed parent: {kids:?}",
    );
}
