//! **The due-day pass over an unreadable instance record stamps what the unread
//! words cannot change, and stays owed for what they could** (rule 130, rule
//! 598). The record maps the operator's status words, and a word it maps to
//! `done` finishes a work item. A work item at a shipped word is judged the same
//! with the words and without them; one at a word of the operator's own is not.
//! The pass answers from the function a write over the same record uses.
//!
//! The cases need a temporary directory and the `dolt` binary the toolchain
//! already holds, so they run in an ordinary `cargo test`.

use std::sync::Arc;

use jiff::civil::date;
use jojobot_adapters::counted::Counted;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::{
    Dolt, Migrated, UNREADABLE_DUE_PASS, backfill_unreadable_due_days, migrate,
};
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::memory::testing::contract::state;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};

use jojobot_adapters::testing::Scratch;

/// A store holding an instance that maps `closed` to `done`, and a project with
/// one work item per `(handle, status)`, each with a schedule that cannot be
/// read. Returns the store, its pool and memory.
async fn world(what: &str, work: &[(&str, &str)]) -> (Dolt, sqlx::MySqlPool, Counted, Scratch) {
    let scratch = Scratch::new(what);
    let store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "memory")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = Counted::new(Arc::new(DoltMemory::open(pool.clone())));
    let mut things = vec![("topic:instance", None), ("project:atlas", None)];
    things.extend(
        work.iter()
            .map(|(handle, _)| (*handle, Some("project:atlas"))),
    );
    for (handle, parent) in things {
        memory
            .add_entity(NewEntity {
                parent: parent.map(|p| EntityId(p.into())),
                ..NewEntity::new(EntityId(handle.into()), handle, "user-named")
            })
            .await
            .expect("add ok")
            .written()
            .expect("not blocked");
    }
    let mut written = vec![("topic:instance", vec![("status_word/closed", "done")])];
    written.extend(work.iter().map(|(handle, status)| {
        (
            *handle,
            vec![
                ("cadence_days", "often"),
                ("advances_from", "due_date"),
                ("counts_from", "2026-06-01"),
                ("status", *status),
            ],
        )
    }));
    for (handle, fields) in written {
        memory
            .capture(state(NewFact {
                fields: fields
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                ..NewFact::about(
                    EntityId(handle.into()),
                    "recorded as it was",
                    date(2026, 9, 1),
                )
            }))
            .await
            .expect("capture ok");
    }
    (store, pool, memory, scratch)
}

async fn day_of(memory: &impl Memory, handle: &str) -> Option<String> {
    memory
        .fields(&EntityId(handle.into()))
        .await
        .expect("fields read")
        .get("due_on")
        .cloned()
}

async fn finished(pool: &sqlx::MySqlPool) -> bool {
    sqlx::query_scalar::<_, String>("SELECT name FROM store_pass")
        .fetch_all(pool)
        .await
        .expect("the marks read")
        .iter()
        .any(|name| name == UNREADABLE_DUE_PASS)
}

/// **A work item at a shipped word is stamped over the unreadable record, and a
/// work item at the operator's own word waits.** `sigma` is at `now`, which means
/// itself, so its late day is stamped. `tau` is at `closed`, which the instance
/// maps to `done`: with the record unreadable that is not known, so no day is
/// stamped on it and the pass stays owed. Read again, the pass leaves `tau`
/// alone, as finished work, does not stamp `sigma` twice, and is finished.
#[tokio::test]
async fn an_unreadable_record_stamps_a_shipped_word_and_leaves_the_operators_word_owed() {
    let (mut store, pool, memory, _scratch) = world(
        "due_unread_mixed",
        &[("work:sigma", "now"), ("work:tau", "closed")],
    )
    .await;
    let today = date(2026, 10, 9);

    memory.fail_fields_of(Some("topic:instance"));
    let waiting = backfill_unreadable_due_days(&pool, &memory, today).await;
    assert!(
        matches!(waiting, Migrated::Skipped(_)),
        "a thing the unread words could decide leaves the pass owed: {waiting:?}"
    );
    assert_eq!(
        day_of(&memory, "work:sigma").await.as_deref(),
        Some("2026-10-09"),
        "the shipped word is stamped as the readable pass stamps it"
    );
    assert_eq!(day_of(&memory, "work:tau").await, None, "no day on tau");
    assert!(!finished(&pool).await, "the pass is still owed");

    memory.fail_fields_of(None);
    let ran = backfill_unreadable_due_days(&pool, &memory, today).await;
    assert!(matches!(ran, Migrated::Ran(0)), "{ran:?}");
    assert_eq!(
        day_of(&memory, "work:tau").await,
        None,
        "the work the operator's word finished is left alone"
    );
    assert_eq!(
        day_of(&memory, "work:sigma").await.as_deref(),
        Some("2026-10-09")
    );
    assert!(finished(&pool).await, "the pass is finished");
    store.stop().await;
}

/// **With nothing the unread words could decide, the pass settles even while the
/// record cannot be read.** The only work item is at a shipped word, so it is
/// stamped exactly as it would be with the record readable, and the pass is
/// marked finished.
#[tokio::test]
async fn an_unreadable_record_settles_the_pass_when_nothing_waits_on_it() {
    let (mut store, pool, memory, _scratch) =
        world("due_unread_settled", &[("work:sigma", "now")]).await;
    memory.fail_fields_of(Some("topic:instance"));
    let ran = backfill_unreadable_due_days(&pool, &memory, date(2026, 10, 9)).await;
    assert!(matches!(ran, Migrated::Ran(1)), "{ran:?}");
    assert_eq!(
        day_of(&memory, "work:sigma").await.as_deref(),
        Some("2026-10-09")
    );
    assert!(finished(&pool).await, "nothing waited, so the pass is done");
    store.stop().await;
}
