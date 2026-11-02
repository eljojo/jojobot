//! **A work item or project cannot be set done while a list hung on it has an
//! open entry**, proven against any [`Memory`] adapter, on both write paths that
//! set a status: a new claim and an edit of a claim.

use super::support::{add, capture, edit, other_caller, state, state_patch};
use super::*;
use crate::memory::entries::{TICK_ACT, TICK_ENTRY};
use crate::memory::{document, entries};

fn status(thing: &EntityId, word: &str) -> NewFact {
    state(NewFact {
        fields: [("status".to_string(), word.to_string())]
            .into_iter()
            .collect(),
        ..NewFact::about(thing.clone(), "the status", date(2026, 11, 2))
    })
}

/// A work item with a list hung on it. `tick` is the list's setting.
async fn a_work_item_with_a_list<M: Memory>(
    store: &M,
    slug: &str,
    tick: &str,
) -> (EntityId, EntityId) {
    let work = EntityId(format!("work:{slug}"));
    add(
        store,
        NewEntity::new(work.clone(), slug, "contract-fixture"),
    )
    .await;
    let list = EntityId(format!("list:{slug}-steps"));
    add(
        store,
        NewEntity {
            parent: Some(work.clone()),
            ..NewEntity::new(list.clone(), format!("{slug} steps"), "contract-fixture")
        },
    )
    .await;
    capture(
        store,
        state(NewFact {
            fields: [("tick".to_string(), tick.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(list.clone(), "the setting", date(2026, 11, 2))
        }),
    )
    .await;
    (work, list)
}

/// An entry written the way `capture` writes one: the claim carries an order key.
async fn an_entry<M: Memory>(store: &M, list: &EntityId, words: &str, order: &str) -> Fact {
    capture(
        store,
        NewFact {
            fields: [(document::ORDER.to_string(), order.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(list.clone(), words, date(2026, 11, 2))
        },
    )
    .await
}

async fn a_tick<M: Memory>(store: &M, list: &EntityId, entry: &Fact, act: &str) {
    capture(
        store,
        NewFact {
            fields: [
                (TICK_ENTRY.to_string(), entry.id.0.clone()),
                (TICK_ACT.to_string(), act.to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(list.clone(), "a tick", date(2026, 11, 2))
        },
    )
    .await;
}

/// The words of the refusal, or a failure when the write was not refused for an
/// open entry.
fn refusal_text(refused: MemoryError) -> String {
    match refused {
        MemoryError::InvalidFact(message) => message,
        other => panic!("expected InvalidFact, got {other:?}"),
    }
}

/// **Done is refused while an entry is open and names each open entry; the same
/// write lands once every entry is ticked done or dropped.** The refused write
/// leaves the status as it was.
pub async fn done_is_refused_while_a_checklist_has_an_open_entry<M: Memory>(store: &M) {
    let (work, list) = a_work_item_with_a_list(store, "contract-done-gate", "sink").await;
    capture(store, status(&work, "now")).await;
    let first = an_entry(store, &list, "first step", "m").await;
    let second = an_entry(store, &list, "second step", "n").await;

    let message = refusal_text(
        store
            .capture(status(&work, "done"))
            .await
            .expect_err("done is refused with two open entries"),
    );
    for entry in [&first, &second] {
        assert!(
            message.contains(&entry.address().to_string()),
            "the refusal names {}: {message}",
            entry.address()
        );
    }
    assert_eq!(
        store.fields(&work).await.expect("reads")["status"],
        "now",
        "a refused write leaves the status as it was"
    );

    a_tick(store, &list, &first, "done").await;
    let message = refusal_text(
        store
            .capture(status(&work, "done"))
            .await
            .expect_err("done is still refused with one open entry"),
    );
    assert!(message.contains(&second.address().to_string()), "{message}");
    assert!(
        !message.contains(&first.address().to_string()),
        "a ticked entry is not named: {message}"
    );

    a_tick(store, &list, &second, "dropped").await;
    capture(store, status(&work, "done")).await;
    assert_eq!(store.fields(&work).await.expect("reads")["status"], "done");
}

/// **A queue's open entries count; a rotation's never do.** A rotation never
/// completes, so its entries are open for good.
pub async fn a_queue_blocks_done_and_a_rotation_never_does<M: Memory>(store: &M) {
    let (queued, queue) = a_work_item_with_a_list(store, "contract-done-queue", "leave").await;
    let entry = an_entry(store, &queue, "waiting step", "m").await;
    let message = refusal_text(
        store
            .capture(status(&queued, "done"))
            .await
            .expect_err("a queue's open entry blocks done"),
    );
    assert!(message.contains(&entry.address().to_string()), "{message}");

    let (rotating, rotation) =
        a_work_item_with_a_list(store, "contract-done-rotation", "back").await;
    an_entry(store, &rotation, "turn step", "m").await;
    capture(store, status(&rotating, "done")).await;
    assert_eq!(
        store.fields(&rotating).await.expect("reads")["status"],
        "done"
    );
}

/// **The edit of a claim that sets the status is held to the same rule**, and
/// only the transition to done is checked: a list added after done changes
/// nothing, and leaving done is free.
pub async fn an_edit_to_done_is_refused_and_only_the_transition_is_checked<M: Memory>(store: &M) {
    let (work, list) = a_work_item_with_a_list(store, "contract-done-edit", "sink").await;
    let claim = capture(store, status(&work, "now")).await;
    let entry = an_entry(store, &list, "open step", "m").await;

    let message = refusal_text(
        store
            .update_fact(
                &claim.address(),
                state_patch(FactPatch {
                    fields: [("status".to_string(), "done".to_string())]
                        .into_iter()
                        .collect(),
                    ..FactPatch::default()
                }),
                &other_caller(),
            )
            .await
            .expect_err("the edit to done is refused"),
    );
    assert!(message.contains(&entry.address().to_string()), "{message}");

    a_tick(store, &list, &entry, "done").await;
    edit(
        store,
        &claim.address(),
        FactPatch {
            fields: [("status".to_string(), "done".to_string())]
                .into_iter()
                .collect(),
            sets: ["status".to_string()].into(),
            ..FactPatch::default()
        },
    )
    .await;

    // Done already: a new open entry changes nothing, and a later write that
    // keeps it done is not a transition.
    an_entry(store, &list, "late step", "n").await;
    assert!(
        !entries::open_entries(&store.recall(&list).await.expect("reads")).is_empty(),
        "the late entry is open"
    );
    capture(store, status(&work, "done")).await;
    capture(store, status(&work, "now")).await;
}
