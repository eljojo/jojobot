//! **The due-day pass tells an instance record that does not exist from one
//! that cannot be read** (rule 130).
//!
//! One store, one work item the pass would stamp when it runs. With the
//! instance's record unreadable the pass is skipped: no day is stamped and no
//! finished marker is written. With the record absent the same pass runs, maps
//! no status words, and stamps the day.

use std::sync::Arc;

use jiff::civil::date;
use jojobot_adapters::counted::Counted;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::{
    Dolt, Migrated, UNREADABLE_DUE_PASS, backfill_unreadable_due_days, migrate,
};
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::testing::contract as memory;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};

#[tokio::test]
async fn the_due_day_pass_runs_over_an_absent_record_and_is_skipped_over_an_unreadable_one() {
    let scratch = Scratch::new("due_pass_instance");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "memory")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory_store = Counted::new(Arc::new(DoltMemory::open(pool.clone())));
    let today = date(2026, 10, 9);

    for (handle, parent) in [
        ("project:atlas", None),
        ("work:sigma", Some("project:atlas")),
    ] {
        memory_store
            .add_entity(NewEntity {
                parent: parent.map(|p| EntityId(p.into())),
                ..NewEntity::new(EntityId(handle.into()), handle, "user-named")
            })
            .await
            .expect("add ok")
            .written()
            .expect("not blocked");
    }
    memory_store
        .capture(memory::state(NewFact {
            fields: [
                ("cadence_days", "often"),
                ("advances_from", "due_date"),
                ("counts_from", "2026-06-01"),
                ("status", "closed"),
            ]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
            ..NewFact::about(
                EntityId("work:sigma".into()),
                "recorded as it was",
                date(2026, 9, 1),
            )
        }))
        .await
        .expect("capture ok");
    let sigma_day = || async {
        memory_store
            .fields(&EntityId("work:sigma".into()))
            .await
            .expect("fields read")
            .get("due_on")
            .cloned()
    };
    let marks = || async {
        sqlx::query_scalar::<_, String>("SELECT name FROM store_pass")
            .fetch_all(&pool)
            .await
            .expect("the marks read")
    };

    // The record cannot be read: the pass is owed, and nothing is stamped.
    memory_store.fail_fields_of(Some("topic:instance"));
    let skipped = backfill_unreadable_due_days(&pool, &memory_store, today).await;
    assert!(
        matches!(skipped, Migrated::Skipped(_)),
        "an unreadable record skips the pass: {skipped:?}"
    );
    assert_eq!(sigma_day().await, None, "no day was stamped");
    assert!(
        !marks().await.iter().any(|name| name == UNREADABLE_DUE_PASS),
        "the pass is still owed"
    );

    // The record does not exist: that maps no words, so the pass runs and
    // stamps the day.
    memory_store.fail_fields_of(None);
    let ran = backfill_unreadable_due_days(&pool, &memory_store, today).await;
    assert!(matches!(ran, Migrated::Ran(1)), "{ran:?}");
    assert_eq!(sigma_day().await.as_deref(), Some("2026-10-09"));
    assert!(
        marks().await.iter().any(|name| name == UNREADABLE_DUE_PASS),
        "the pass is finished"
    );

    store.stop().await;
}
