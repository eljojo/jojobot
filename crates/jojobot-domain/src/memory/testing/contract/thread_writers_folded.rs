//! **A bot folded into another is a writer of a thread as its survivor**,
//! proven against any [`Memory`] adapter.
//!
//! A fold moves no row, so a record the folded bot filed still holds its id. The
//! writers of the thread are read as the handle that id answers to now, in the
//! real store and in the fake alike.

use super::support::{capture, ensure, state};
use super::*;
use crate::memory::capacity::RECORD_CAPACITY;

/// **The record a folded bot filed makes its survivor a writer of the thread.**
/// Only a bot above a writer sets the thread's capacity, so a bot that is not
/// above the survivor is refused and the survivor's manager is not. The refusal
/// of the outsider is paired with the landing of the manager's write.
pub async fn a_record_filed_by_a_folded_bot_makes_its_survivor_a_writer<M: Memory>(store: &M) {
    let thread = EntityId("thread:contract-folded-filled".into());
    let filler = EntityId("bot:contract-folded-filler".into());
    let heir = EntityId("bot:contract-folded-heir".into());
    let boss = EntityId("bot:contract-folded-boss".into());
    let outsider = EntityId("bot:contract-folded-outsider".into());
    for id in [&thread, &filler, &heir, &boss, &outsider] {
        ensure(store, id).await;
    }
    for below in [&filler, &heir] {
        capture(
            store,
            state(NewFact {
                fields: [(crate::memory::REPORTS_TO.to_string(), boss.to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(below.clone(), "reports up", date(2026, 10, 1))
            }),
        )
        .await;
    }
    let room = capture(
        store,
        NewFact::about(thread.clone(), "the thread's room", date(2026, 10, 2)),
    )
    .await;
    capture(
        store,
        NewFact {
            written_by_bot: Some(filler.clone()),
            session: Some("no-such-run".to_string()),
            ..NewFact::about(thread.clone(), "a record filed", date(2026, 10, 3))
        },
    )
    .await;
    store
        .merge(&filler, &heir, None, date(2026, 10, 4), &boss, None)
        .await
        .expect("the bots fold into one");

    let capacity = |value: &str| {
        super::support::state_patch(FactPatch {
            fields: [(RECORD_CAPACITY.to_string(), value.to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        })
    };
    let refused = store
        .update_fact(&room.address(), capacity("5"), &outsider)
        .await
        .expect_err("an outsider does not set the capacity of a thread a bot filled");
    assert!(
        matches!(refused, MemoryError::KeyNotYours { ref key, .. } if key == RECORD_CAPACITY),
        "{refused:?}"
    );
    store
        .update_fact(&room.address(), capacity("5"), &boss)
        .await
        .expect("the manager of the survivor sets the capacity")
        .written()
        .expect("nothing blocks it");
}
