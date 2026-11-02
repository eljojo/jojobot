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
    a_thing_with_a_list(store, EntityId(format!("work:{slug}")), slug, tick).await
}

/// `thing` with a list hung on it. `tick` is the list's setting.
async fn a_thing_with_a_list<M: Memory>(
    store: &M,
    work: EntityId,
    slug: &str,
    tick: &str,
) -> (EntityId, EntityId) {
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

/// A work item with no list.
async fn a_bare_work_item<M: Memory>(store: &M, slug: &str) -> EntityId {
    let work = EntityId(format!("work:{slug}"));
    add(
        store,
        NewEntity::new(work.clone(), slug, "contract-fixture"),
    )
    .await;
    work
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

/// **A project is held to the gate like a work item**: refused done while an
/// entry is open, naming it, and done once the entry is ticked.
pub async fn a_project_is_refused_done_like_a_work_item<M: Memory>(store: &M) {
    let slug = "contract-done-project";
    let (project, list) =
        a_thing_with_a_list(store, EntityId(format!("project:{slug}")), slug, "sink").await;
    let entry = an_entry(store, &list, "open step", "m").await;

    let message = refusal_text(
        store
            .capture(status(&project, "done"))
            .await
            .expect_err("a project with an open entry is refused done"),
    );
    assert!(message.contains(&entry.address().to_string()), "{message}");
    assert!(
        !store
            .fields(&project)
            .await
            .expect("reads")
            .contains_key("status"),
        "a refused write leaves no status"
    );

    a_tick(store, &list, &entry, "done").await;
    capture(store, status(&project, "done")).await;
    assert_eq!(
        store.fields(&project).await.expect("reads")["status"],
        "done"
    );
}

/// **A list archived, and a list folded into another, are no longer part of the
/// thing**, so their open entries do not block done. Beside each, the same thing
/// with a live open list is refused, so the pass is the list leaving and not the
/// gate being off.
pub async fn a_list_archived_or_folded_away_does_not_block_done<M: Memory>(store: &M) {
    let (kept, kept_list) = a_work_item_with_a_list(store, "contract-done-live", "sink").await;
    let live = an_entry(store, &kept_list, "live step", "m").await;
    let message = refusal_text(
        store
            .capture(status(&kept, "done"))
            .await
            .expect_err("a live list's open entry blocks done"),
    );
    assert!(message.contains(&live.address().to_string()), "{message}");

    let (shelved, shelved_list) =
        a_work_item_with_a_list(store, "contract-done-archived", "sink").await;
    an_entry(store, &shelved_list, "archived step", "m").await;
    refusal_text(
        store
            .capture(status(&shelved, "done"))
            .await
            .expect_err("the list blocks done while it is live"),
    );
    store
        .archive_entity(&shelved_list, "no longer needed", None)
        .await
        .expect("the list is archived");
    capture(store, status(&shelved, "done")).await;
    assert_eq!(
        store.fields(&shelved).await.expect("reads")["status"],
        "done"
    );

    let (folded_from, folded_list) =
        a_work_item_with_a_list(store, "contract-done-folded", "sink").await;
    an_entry(store, &folded_list, "folded step", "m").await;
    let survivor = EntityId("list:contract-done-survivor".into());
    add(
        store,
        NewEntity::new(
            survivor.clone(),
            "contract-done-survivor",
            "contract-fixture",
        ),
    )
    .await;
    refusal_text(
        store
            .capture(status(&folded_from, "done"))
            .await
            .expect_err("the list blocks done until it is folded away"),
    );
    store
        .merge(
            &folded_list,
            &survivor,
            None,
            date(2026, 11, 2),
            &other_caller(),
            None,
        )
        .await
        .expect("the list is folded into another");
    capture(store, status(&folded_from, "done")).await;
    assert_eq!(
        store.fields(&folded_from).await.expect("reads")["status"],
        "done"
    );
}

/// **A merge that would leave the survivor at done is held to the same rule.**
/// The duplicate says done and folds onto a survivor that holds no status, so the
/// fold moves it to done whatever the order of the two writes; an open entry on a list the survivor holds, or on one the fold moves
/// under it, refuses the merge and names the entry, and nothing of the fold lands.
/// **Paired**: once every entry is ticked the same merge lands and the survivor
/// serves done.
pub async fn a_merge_that_would_finish_work_with_an_open_entry_is_refused<M: Memory>(store: &M) {
    // The survivor's own list holds the open entry.
    let (keep, keep_list) =
        a_work_item_with_a_list(store, "contract-merge-gate-keep", "sink").await;
    let open = an_entry(store, &keep_list, "still to do", "m").await;
    let twin = a_bare_work_item(store, "contract-merge-gate-twin").await;
    capture(store, status(&twin, "done")).await;

    let message = refusal_text(
        store
            .merge(
                &twin,
                &keep,
                Some("one card under two names"),
                date(2026, 11, 2),
                &other_caller(),
                None,
            )
            .await
            .expect_err("the merge would finish work with an open entry"),
    );
    assert!(message.contains(&open.address().to_string()), "{message}");
    assert!(
        !store
            .fields(&keep)
            .await
            .expect("reads")
            .contains_key("status"),
        "a refused merge leaves the survivor as it was"
    );

    a_tick(store, &keep_list, &open, "done").await;
    store
        .merge(
            &twin,
            &keep,
            Some("one card under two names"),
            date(2026, 11, 2),
            &other_caller(),
            None,
        )
        .await
        .expect("with every entry ticked the same merge lands");
    assert_eq!(store.fields(&keep).await.expect("reads")["status"], "done");

    // The open entry sits on a list the fold moves under the survivor.
    let alive = a_bare_work_item(store, "contract-merge-gate-alive").await;
    let (dup, dup_list) = a_work_item_with_a_list(store, "contract-merge-gate-dup", "sink").await;
    capture(store, status(&dup, "done")).await;
    let moved = an_entry(store, &dup_list, "moved step", "m").await;

    let message = refusal_text(
        store
            .merge(
                &dup,
                &alive,
                Some("one card under two names"),
                date(2026, 11, 2),
                &other_caller(),
                None,
            )
            .await
            .expect_err("a list the fold moves under the survivor blocks done too"),
    );
    assert!(message.contains(&moved.address().to_string()), "{message}");
    assert!(
        !store
            .fields(&alive)
            .await
            .expect("reads")
            .contains_key("status"),
        "a refused merge leaves the survivor as it was"
    );
}
