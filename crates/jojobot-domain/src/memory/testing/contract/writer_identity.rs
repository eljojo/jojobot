//! **Who wrote a claim is kept as the writer's permanent id, proven against any
//! [`Memory`] adapter** (work: q-writer-identity-stable, rule 268).
//!
//! Every other pointer in a store holds the id a thing wears for good. The bot
//! that made a write is one more pointer, so a rename of the bot changes what the
//! write reads as naming and nothing stored. A claim a bot wrote on itself stays
//! its own through any number of renames, and a run no session row answers to
//! still counts as a writer of the thread it filled.

use super::support::{capture, ensure, other_caller, state};
use super::*;
use crate::memory::capacity::RECORD_CAPACITY;
use crate::memory::{Edge, EdgeShape, Override, THOUGHT_BODY_CAP};

/// **A bot renamed twice still reads its own thought as its own.** The thought
/// is written under the handle the bot wore at the time and read after the bot
/// has been renamed again. Paired with another bot's claim on the same home, which
/// still reads as another's, so a store that called every claim its own would not
/// pass.
pub async fn a_bot_renamed_twice_still_reads_its_own_thought_as_its_own<M: Memory>(store: &M) {
    let first = EntityId("bot:contract-identity-first".into());
    let second = EntityId("bot:contract-identity-second".into());
    let third = EntityId("bot:contract-identity-third".into());
    let other = EntityId("bot:contract-identity-other".into());
    let object = EntityId("thing:contract-identity-object".into());
    for id in [&first, &other, &object] {
        ensure(store, id).await;
    }
    capture(
        store,
        state(NewFact {
            fields: [(THOUGHT_BODY_CAP.to_string(), "5".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(first.clone(), "cap is five", date(2026, 10, 6))
        }),
    )
    .await;
    let rename = |from: &EntityId, to: &EntityId| {
        let from = from.clone();
        let to = to.clone();
        async move {
            store
                .rename_entity(&from, &to, None, date(2026, 10, 7), Override::none(), None)
                .await
                .expect("the bot is renamed")
                .written()
                .expect("nothing blocks the rename");
        }
    };
    rename(&first, &second).await;
    let own = capture(
        store,
        NewFact {
            edge: Some(Edge::new(EdgeShape::Connection, object.clone())),
            written_by_bot: Some(second.clone()),
            ..NewFact::about(second.clone(), "abcde", date(2026, 10, 8))
        },
    )
    .await;
    let others = capture(
        store,
        NewFact {
            edge: Some(Edge::new(EdgeShape::Connection, object.clone())),
            written_by_bot: Some(other.clone()),
            ..NewFact::about(second.clone(), "abcdefgh", date(2026, 10, 8))
        },
    )
    .await;
    // **The capture returns the flag it computed**, so a receipt built from it
    // cannot tell one bot it spent another's reserve.
    assert!(
        others.written_by_another && !own.written_by_another,
        "the claims captured say whose they are: {own:?} {others:?}"
    );
    rename(&second, &third).await;

    let read = store.recall(&third).await.expect("recall should succeed");
    let find = |id: &FactId| read.iter().find(|f| &f.id == id).expect("read back");
    assert!(
        !find(&own.id).written_by_another,
        "a bot's own thought reads as another's after the bot is renamed twice"
    );
    assert!(
        find(&others.id).written_by_another,
        "another bot's claim still reads as another's"
    );
    let history = store
        .claim_history(&own.address())
        .await
        .expect("the history reads");
    assert_eq!(
        history[0].bot,
        Some(third.clone()),
        "the first write names its bot as the handle the bot wears now"
    );
    // The thought still faces the cap the bot set on itself.
    let refused = store
        .capture(NewFact {
            edge: Some(Edge::new(EdgeShape::Connection, object.clone())),
            written_by_bot: Some(third.clone()),
            ..NewFact::about(third.clone(), "abcdef", date(2026, 10, 9))
        })
        .await
        .expect_err("a bot's own thought faces its cap");
    assert!(
        matches!(refused, MemoryError::ThoughtTooLong { cap: 5, .. }),
        "{refused:?}"
    );
}

/// **A bot whose run has no session row still counts as a writer of the thread it
/// filled.** The bot filed a record on the thread under a run no session answers
/// to, and the write kept the bot beside the run. The thread's record capacity is
/// then the bot's manager's to set and not an outsider's. Paired with the manager's
/// own write, which lands, and with the same write on a thread nobody filled, which
/// any bot may make.
pub async fn a_run_with_no_session_row_that_filled_a_thread_is_a_writer_of_it<M: Memory>(
    store: &M,
) {
    let thread = EntityId("thread:contract-identity-filled".into());
    let empty = EntityId("thread:contract-identity-empty".into());
    let filler = EntityId("bot:contract-identity-filler".into());
    let boss = EntityId("bot:contract-identity-boss".into());
    let outsider = EntityId("bot:contract-identity-outsider".into());
    for id in [&thread, &empty, &filler, &boss, &outsider] {
        ensure(store, id).await;
    }
    capture(
        store,
        state(NewFact {
            fields: [(crate::memory::REPORTS_TO.to_string(), boss.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(filler.clone(), "reports up", date(2026, 10, 1))
        }),
    )
    .await;
    let capacity = |value: &str| {
        super::support::state_patch(FactPatch {
            fields: [(RECORD_CAPACITY.to_string(), value.to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        })
    };
    let ceiling_of_empty = capture(
        store,
        NewFact::about(empty.clone(), "the thread's room", date(2026, 10, 2)),
    )
    .await;
    store
        .update_fact(&ceiling_of_empty.address(), capacity("5"), &outsider)
        .await
        .expect("a thread nobody filled takes a capacity from any bot")
        .written()
        .expect("nothing blocks it");

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
            ..NewFact::about(
                thread.clone(),
                "a record the filler filed",
                date(2026, 10, 3),
            )
        },
    )
    .await;
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
        .expect("the filler's manager sets the capacity")
        .written()
        .expect("nothing blocks it");
}

/// **A bot folded into another reads as the survivor wherever a write names its
/// writer.** A fold moves no row, so the write still holds the folded bot's id,
/// and the history of the claim has to follow it to the survivor the way every
/// other pointer does. Paired with a claim another bot wrote, which keeps its
/// own writer, so a store that named the survivor on every write would not pass.
pub async fn a_bot_folded_into_another_reads_as_the_survivor_in_a_claims_history<M: Memory>(
    store: &M,
) {
    let folded = EntityId("bot:contract-identity-folded".into());
    let survivor = EntityId("bot:contract-identity-survivor".into());
    let other = EntityId("bot:contract-identity-bystander".into());
    let home = EntityId("thing:contract-identity-folded-home".into());
    for id in [&folded, &survivor, &other, &home] {
        ensure(store, id).await;
    }
    let by_folded = capture(
        store,
        NewFact {
            written_by_bot: Some(folded.clone()),
            ..NewFact::about(home.clone(), "the folded bot wrote this", date(2026, 10, 8))
        },
    )
    .await;
    let by_other = capture(
        store,
        NewFact {
            written_by_bot: Some(other.clone()),
            ..NewFact::about(home.clone(), "the bystander wrote this", date(2026, 10, 8))
        },
    )
    .await;
    store
        .merge(
            &folded,
            &survivor,
            None,
            date(2026, 10, 9),
            &other_caller(),
            None,
        )
        .await
        .expect("the bots fold into one");

    let history = store
        .claim_history(&by_folded.address())
        .await
        .expect("the history reads");
    assert_eq!(
        history[0].bot,
        Some(survivor.clone()),
        "the first write names the folded bot, not the bot it was folded into"
    );
    let history = store
        .claim_history(&by_other.address())
        .await
        .expect("the history reads");
    assert_eq!(
        history[0].bot,
        Some(other),
        "another bot's write is untouched"
    );
}
