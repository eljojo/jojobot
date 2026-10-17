//! **How many live records a thread holds, proven against any [`Memory`]
//! adapter.**
//!
//! A thread holds three live records unless it sets its own number. The count
//! is made inside the write that adds a record, so every door that adds one
//! faces it: a capture, a merge that brings records onto the thread, and a
//! retype that makes a thing a thread. Every refusal here sits beside the same
//! write made where it fits, so a store that refused every write would not
//! pass.

use super::support::{capture, ensure, other_caller, state};
use super::*;
use crate::memory::capacity::RECORD_CAPACITY;

fn note(home: &EntityId, content: &str) -> NewFact {
    NewFact::about(home.clone(), content, date(2026, 10, 1))
}

/// The refusal a write earned, or a panic naming what it got instead.
fn refusal(result: Result<Guarded<Fact>, MemoryError>) -> MemoryError {
    match result {
        Err(refused) => refused,
        Ok(other) => panic!("the write was expected to be refused, got {other:?}"),
    }
}

/// **A thread holds three live records, and the fourth is refused with the
/// records it would displace.** An archived record, a child's record and a thought
/// pointing at the thread take no slot; a rewrite adds none; a record archived
/// makes room; a borrow lands one over, once, and archiving back repays it.
pub async fn a_thread_holds_three_live_records_and_refuses_a_fourth<M: Memory>(store: &M) {
    let thread = EntityId("thread:contract-records-three".into());
    let child = EntityId("decision:contract-records-three-child".into());
    let bot = EntityId("bot:contract-records-three-bot".into());
    ensure(store, &thread).await;
    ensure(store, &bot).await;
    crate::memory::testing::contract::support::add(
        store,
        NewEntity {
            parent: Some(thread.clone()),
            ..NewEntity::new(child.clone(), "child", "contract-fixture")
        },
    )
    .await;

    // Records that take no slot: a child's, and a thought pointing at the thread.
    capture(store, note(&child, "a decision under the thread")).await;
    capture(
        store,
        NewFact {
            edge: Some(Edge::new(EdgeShape::Connection, thread.clone())),
            ..note(&bot, "a thought pointing at the thread")
        },
    )
    .await;

    let first = capture(store, note(&thread, "the first record")).await;
    capture(store, note(&thread, "the second record")).await;
    capture(store, note(&thread, "the third record")).await;

    // The fourth is refused, with the three live records under the handle.
    let refused = refusal(store.capture(note(&thread, "the fourth record")).await);
    match refused {
        MemoryError::OverCapacity {
            subject,
            live,
            limit,
            adding,
            from_setting,
            records,
        } => {
            assert_eq!(
                (subject.as_str(), live, limit, adding, from_setting),
                (thread.as_str(), 3, 3, 1, false)
            );
            assert_eq!(records.len(), 3);
            assert!(
                records.iter().all(|r| r.address().home == thread),
                "the records ride on the refusal under the handle the caller used: {records:?}"
            );
        }
        other => panic!("expected OverCapacity, got {other:?}"),
    }
    let held = store.recall(&thread).await.expect("recall ok");
    assert_eq!(
        held.iter()
            .filter(|f| f.status == FactStatus::Active)
            .count(),
        3,
        "a refused capture writes nothing"
    );
    assert!(held.iter().all(|f| f.content != "the fourth record"));

    // A rewrite adds no record, so it lands at full.
    store
        .update_fact(
            &first.address(),
            FactPatch {
                content: Some("the first record, reworded".into()),
                provenance: Some(Provenance::Inference),
                ..Default::default()
            },
            &other_caller(),
        )
        .await
        .expect("a rewrite at full lands");

    // Archiving one makes room.
    let archive = |why: &str| FactPatch {
        status: Some(FactStatus::Archived),
        details: Some(why.into()),
        ..Default::default()
    };
    store
        .update_fact(&first.address(), archive("made room"), &other_caller())
        .await
        .expect("archiving lands");
    capture(store, note(&thread, "the fourth record")).await;
    assert!(matches!(
        refusal(store.capture(note(&thread, "a fifth record")).await),
        MemoryError::OverCapacity { live: 3, .. }
    ));

    // A borrow lands one over; a second is refused while the debt stands.
    let borrowed = store
        .capture(NewFact {
            borrow: true,
            ..note(&thread, "the borrowed record")
        })
        .await
        .expect("a borrow at exactly the ceiling lands")
        .written()
        .expect("nothing blocks a borrow");
    assert_eq!(borrowed.content, "the borrowed record");
    assert!(matches!(
        refusal(
            store
                .capture(NewFact {
                    borrow: true,
                    ..note(&thread, "a second borrowed record")
                })
                .await
        ),
        MemoryError::OverCapacity {
            live: 4,
            limit: 3,
            ..
        }
    ));

    // A thread over its ceiling still reads, edits and archives.
    let live = store.recall(&thread).await.expect("recall ok");
    let second = live
        .iter()
        .find(|f| f.content == "the second record")
        .expect("the second record is there");
    store
        .update_fact(
            &second.address(),
            FactPatch {
                content: Some("the second record, edited over the ceiling".into()),
                provenance: Some(Provenance::Inference),
                ..Default::default()
            },
            &other_caller(),
        )
        .await
        .expect("an edit over the ceiling lands");

    // Archiving back to the ceiling repays the reserve.
    store
        .update_fact(&borrowed.address(), archive("repaying"), &other_caller())
        .await
        .expect("archiving the borrowed record lands");
    store
        .capture(NewFact {
            borrow: true,
            ..note(&thread, "borrowing again once repaid")
        })
        .await
        .expect("the reserve is back once the thread is back at its ceiling")
        .written()
        .expect("nothing blocks it");
}

/// **A thread that sets its own number is bounded by that instead**, on a number
/// below the default and on one above it.
pub async fn a_threads_own_record_number_replaces_the_default<M: Memory>(store: &M) {
    let small = EntityId("thread:contract-records-two".into());
    let large = EntityId("thread:contract-records-five".into());
    for id in [&small, &large] {
        ensure(store, id).await;
    }
    for (thread, number) in [(&small, "2"), (&large, "5")] {
        capture(
            store,
            state(NewFact {
                fields: [(RECORD_CAPACITY.to_string(), number.to_string())]
                    .into_iter()
                    .collect(),
                ..note(thread, "how many records the thread holds")
            }),
        )
        .await;
    }

    capture(store, note(&small, "the second record")).await;
    assert!(matches!(
        refusal(store.capture(note(&small, "the third record")).await),
        MemoryError::OverCapacity {
            live: 2,
            limit: 2,
            from_setting: true,
            ..
        }
    ));

    for n in 2..=5 {
        capture(store, note(&large, &format!("record {n}"))).await;
    }
    assert!(matches!(
        refusal(store.capture(note(&large, "record 6")).await),
        MemoryError::OverCapacity {
            live: 5,
            limit: 5,
            ..
        }
    ));
}

/// **A merge cannot bring a thread over its ceiling**, whether the duplicate
/// brings thoughts or not, and a refusal moves nothing. The same merge lands
/// where the records fit.
pub async fn a_merge_cannot_bring_a_thread_over_its_record_ceiling<M: Memory>(store: &M) {
    let thread = EntityId("thread:contract-records-merge".into());
    let fits = EntityId("topic:contract-records-merge-fits".into());
    let too_many = EntityId("topic:contract-records-merge-too-many".into());
    for id in [&thread, &fits, &too_many] {
        ensure(store, id).await;
    }
    capture(store, note(&thread, "the first record")).await;
    capture(store, note(&thread, "the second record")).await;
    capture(store, note(&too_many, "one of two")).await;
    capture(store, note(&too_many, "two of two")).await;
    capture(store, note(&fits, "the one that fits")).await;

    let refused = store
        .merge(
            &too_many,
            &thread,
            None,
            date(2026, 10, 2),
            &other_caller(),
            None,
        )
        .await
        .expect_err("a merge past the ceiling is refused");
    assert!(
        matches!(
            refused,
            MemoryError::OverCapacity {
                live: 2,
                limit: 3,
                adding: 2,
                ..
            }
        ),
        "{refused:?}"
    );
    let after = store.recall(&thread).await.expect("recall ok");
    assert_eq!(after.len(), 2, "a refused merge moved nothing: {after:?}");
    assert!(
        store
            .list_entities(None)
            .await
            .expect("list ok")
            .iter()
            .any(|e| e.id == too_many && e.merged_into.is_none()),
        "the duplicate was not folded"
    );

    store
        .merge(
            &fits,
            &thread,
            None,
            date(2026, 10, 2),
            &other_caller(),
            None,
        )
        .await
        .expect("a merge that fits lands");
    let after = store.recall(&thread).await.expect("recall ok");
    assert!(
        after.iter().any(|f| f.content == "the one that fits"),
        "{after:?}"
    );
}

/// A record that sets the thing's own record number.
fn numbered(home: &EntityId, content: &str, number: &str) -> NewFact {
    NewFact {
        fields: [(RECORD_CAPACITY.to_string(), number.to_string())]
            .into_iter()
            .collect(),
        sets: [RECORD_CAPACITY.to_string()].into_iter().collect(),
        ..note(home, content)
    }
}

/// **A merge is judged against the lower of the two numbers.** The merged thing
/// folds to the survivor's number or the duplicate's, whichever was written
/// last, so a merge that fits only one of them is refused, and a refusal moves
/// nothing. The same merge lands where the duplicate's number is the higher.
pub async fn a_merge_is_held_to_the_lower_of_the_two_record_numbers<M: Memory>(store: &M) {
    let survivor = EntityId("thread:contract-records-lower".into());
    let low = EntityId("topic:contract-records-lower-low".into());
    let high = EntityId("topic:contract-records-lower-high".into());
    for id in [&survivor, &low, &high] {
        ensure(store, id).await;
    }
    capture(store, numbered(&survivor, "the first record", "20")).await;
    for n in 2..=5 {
        capture(store, note(&survivor, &format!("record {n}"))).await;
    }
    capture(store, numbered(&low, "one of three", "3")).await;
    capture(store, note(&low, "two of three")).await;
    capture(store, note(&low, "three of three")).await;
    capture(store, numbered(&high, "one of two", "30")).await;
    capture(store, note(&high, "two of two")).await;

    // 5 and 3 make 8: the survivor's 20 holds them and the duplicate's 3 does not.
    let refused = store
        .merge(
            &low,
            &survivor,
            None,
            date(2026, 10, 2),
            &other_caller(),
            None,
        )
        .await
        .expect_err("a merge past the lower number is refused");
    assert!(
        matches!(
            refused,
            MemoryError::OverCapacity {
                live: 5,
                limit: 3,
                adding: 3,
                ..
            }
        ),
        "{refused:?}"
    );
    assert_eq!(
        store.recall(&survivor).await.expect("recall ok").len(),
        5,
        "a refused merge moved nothing"
    );

    // 5 and 2 make 7: both numbers hold them.
    store
        .merge(
            &high,
            &survivor,
            None,
            date(2026, 10, 2),
            &other_caller(),
            None,
        )
        .await
        .expect("a merge that fits both numbers lands");
    let after = store.recall(&survivor).await.expect("recall ok");
    assert!(
        after.iter().any(|f| f.content == "two of two"),
        "the merge landed: {after:?}"
    );
}

/// **A retype cannot make a thing a thread that holds more than a thread
/// allows**, and the same retype lands where the records fit.
pub async fn a_retype_cannot_make_a_thread_past_its_record_ceiling<M: Memory>(store: &M) {
    let crowded = EntityId("topic:contract-records-crowded".into());
    let fitting = EntityId("topic:contract-records-fitting".into());
    for id in [&crowded, &fitting] {
        ensure(store, id).await;
    }
    for n in 1..=4 {
        capture(store, note(&crowded, &format!("record {n}"))).await;
    }
    for n in 1..=3 {
        capture(store, note(&fitting, &format!("record {n}"))).await;
    }
    let as_thread = |from: &EntityId| EntityId(format!("thread:{}", from.slug()));

    let refused = store
        .rename_entity(
            &crowded,
            &as_thread(&crowded),
            None,
            date(2026, 10, 2),
            crate::memory::Override::none(),
            None,
        )
        .await
        .expect_err("a retype past the ceiling is refused");
    assert!(
        matches!(
            refused,
            MemoryError::OverCapacity {
                live: 4,
                limit: 3,
                adding: 0,
                ..
            }
        ),
        "{refused:?}"
    );
    assert!(
        store
            .list_entities(None)
            .await
            .expect("list ok")
            .iter()
            .any(|e| e.id == crowded),
        "a refused retype left the thing as it was"
    );

    store
        .rename_entity(
            &fitting,
            &as_thread(&fitting),
            None,
            date(2026, 10, 2),
            crate::memory::Override::none(),
            None,
        )
        .await
        .expect("a retype that fits lands")
        .written()
        .expect("nothing collides");
}

/// **The review mark takes no slot, and only the mark.** A record labelled as a
/// thread's review mark lands on a thread that holds its three live records,
/// the count still reads three, and a fourth ordinary record is still refused.
/// Another record that merely carries the stamp key takes its slot as any does.
pub async fn the_review_mark_takes_no_slot_under_the_record_ceiling<M: Memory>(store: &M) {
    use crate::memory::review::{DEFRAGGED_AT, REVIEW_MARK};
    let thread = EntityId("thread:contract-records-reviewed".into());
    ensure(store, &thread).await;
    for n in 1..=3 {
        capture(store, note(&thread, &format!("record {n}"))).await;
    }
    capture(
        store,
        NewFact {
            fields: [(REVIEW_MARK.to_string(), "true".to_string())]
                .into_iter()
                .collect(),
            ..note(&thread, "Reviewed.")
        },
    )
    .await;
    assert!(matches!(
        refusal(store.capture(note(&thread, "record 4")).await),
        MemoryError::OverCapacity {
            live: 3,
            limit: 3,
            ..
        }
    ));
    // The exemption is the label's: a record that carries only the stamp is an
    // ordinary record, and the thread is full.
    assert!(matches!(
        refusal(
            store
                .capture(NewFact {
                    fields: [(DEFRAGGED_AT.to_string(), "2026-10-01T00:00:00Z".to_string())]
                        .into_iter()
                        .collect(),
                    ..note(&thread, "tidied")
                })
                .await
        ),
        MemoryError::OverCapacity { live: 3, .. }
    ));
}

/// **A record jojobot wrote about an act of its own takes no slot.** A record
/// labelled as the engine's lands on a thread that holds its three live records,
/// the count still reads three, and a fourth ordinary record is still refused.
pub async fn an_engine_record_takes_no_slot_under_the_record_ceiling<M: Memory>(store: &M) {
    use crate::memory::capacity::ENGINE_RECORD;
    let thread = EntityId("thread:contract-records-engine".into());
    ensure(store, &thread).await;
    for n in 1..=3 {
        capture(store, note(&thread, &format!("record {n}"))).await;
    }
    capture(
        store,
        NewFact {
            fields: [(ENGINE_RECORD.to_string(), "restore".to_string())]
                .into_iter()
                .collect(),
            ..note(&thread, "Restored from the archive.")
        },
    )
    .await;
    assert!(matches!(
        refusal(store.capture(note(&thread, "record 4")).await),
        MemoryError::OverCapacity {
            live: 3,
            limit: 3,
            ..
        }
    ));
}

/// Every spec above, in one call — see [`base::run_all`].
pub async fn run_all_record_capacity<M: Memory>(store: &M) {
    a_thread_holds_three_live_records_and_refuses_a_fourth(store).await;
    a_threads_own_record_number_replaces_the_default(store).await;
    a_merge_cannot_bring_a_thread_over_its_record_ceiling(store).await;
    a_merge_is_held_to_the_lower_of_the_two_record_numbers(store).await;
    a_retype_cannot_make_a_thread_past_its_record_ceiling(store).await;
    the_review_mark_takes_no_slot_under_the_record_ceiling(store).await;
    an_engine_record_takes_no_slot_under_the_record_ceiling(store).await;
}
