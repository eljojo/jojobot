//! **A reference key may name more than one kind**, proven against any
//! [`Memory`] adapter: `waiting_on` points at a bot or a person, a work handle
//! there is refused toward the key that takes it, and a declaration that names
//! one kind keeps the spelling it always had.

use super::support::{capture, ensure};
use super::*;
use std::collections::BTreeMap;

/// A work item's claim that sets one key to one handle.
fn sets_key(work: &EntityId, key: &str, value: &str) -> NewFact {
    NewFact {
        fields: BTreeMap::from([(key.to_string(), value.to_string())]),
        sets: [key.to_string()].into(),
        ..NewFact::about(work.clone(), "waiting on something", date(2026, 11, 2))
    }
}

/// **A work handle under `waiting_on` is refused and the refusal names the key
/// that takes it; a bot and a person land.** One case, several writes: a store
/// that refused every handle passes the first alone, and one that took every
/// handle passes the others alone.
pub async fn waiting_on_naming_work_is_refused_toward_depends_on<M: Memory>(store: &M) {
    let waiter = EntityId("work:q-1001".into());
    let other = EntityId("work:q-boot-composes-now".into());
    let bot = EntityId("bot:sigma".into());
    let person = EntityId("person:milhouse".into());
    // The keys the work kind ships, as a startup declares them: a store that
    // was not seeded holds the kind with none, and then nothing is refused.
    store
        .declare_kind(
            "work",
            Origin::Shipped,
            crate::memory::kinds::keys_of("work"),
        )
        .await
        .expect("the work kind's keys declare");
    for id in [&waiter, &other, &bot, &person] {
        ensure(store, id).await;
    }

    match store
        .capture(sets_key(&waiter, "waiting_on", other.as_str()))
        .await
    {
        Err(MemoryError::BreaksType { key, instead, .. }) => {
            assert_eq!(key, "waiting_on");
            assert_eq!(
                &*instead,
                ["depends_on".to_string()],
                "the refusal names the reference list that takes a work handle"
            );
        }
        other => panic!("a work handle under waiting_on must be refused, got {other:?}"),
    }
    let held = store.fields(&waiter).await.expect("fields read");
    assert!(
        !held.contains_key("waiting_on"),
        "the refused write left nothing behind: {held:?}"
    );

    capture(store, sets_key(&waiter, "waiting_on", bot.as_str())).await;
    assert_eq!(
        store
            .fields(&waiter)
            .await
            .expect("fields read")
            .get("waiting_on"),
        Some(&bot.to_string()),
        "a bot lands"
    );
    capture(store, sets_key(&waiter, "waiting_on", person.as_str())).await;
    assert_eq!(
        store
            .fields(&waiter)
            .await
            .expect("fields read")
            .get("waiting_on"),
        Some(&person.to_string()),
        "a person lands"
    );
    capture(store, sets_key(&waiter, "depends_on", other.as_str())).await;
    assert_eq!(
        store
            .fields(&waiter)
            .await
            .expect("fields read")
            .get("depends_on"),
        Some(&other.to_string()),
        "the key the refusal names takes the work handle"
    );
}

/// **A reference naming two kinds survives the store, and one naming one kind
/// still reads back as that kind alone.** The one-kind declaration is the shape
/// every store held before a key could name several, so it has to read back
/// unchanged.
pub async fn a_reference_naming_two_kinds_keeps_both_and_one_kind_keeps_its_spelling<M: Memory>(
    store: &M,
) {
    store
        .declare_type(
            DeclaredType::new(
                "contract-kinds",
                vec![
                    Field::pointing_at_any("handed_to", &[EntityKind::BOT, EntityKind::PERSON]),
                    Field::pointing_at("held_at", EntityKind::PLACE),
                ],
            ),
            None,
        )
        .await
        .expect("declaring a type of my own is accepted");
    let held = store
        .declared_types()
        .await
        .expect("the roster reads")
        .into_iter()
        .find(|t| t.name == "contract-kinds")
        .expect("the store holds the type it just took");
    assert_eq!(
        held.field("handed_to").and_then(|f| f.points_at.clone()),
        Kinds::any_of(&[EntityKind::BOT, EntityKind::PERSON]),
        "both kinds came back, in the order declared: {held:?}"
    );
    assert_eq!(
        held.field("held_at").and_then(|f| f.points_at.clone()),
        Some(Kinds::one(EntityKind::PLACE)),
        "a one-kind reference is the one kind, as before: {held:?}"
    );
    assert_eq!(
        held.field("held_at").map(Field::holds_token).as_deref(),
        Some("reference:place"),
        "…and spelled as it always was: {held:?}"
    );
}
