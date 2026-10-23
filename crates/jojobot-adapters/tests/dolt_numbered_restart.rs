//! Numbered collections over the real store, across a restart.

use std::sync::Arc;

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::fold::Folded;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::testing::contract as memory;
use jojobot_domain::memory::{EntityId, Memory, MemoryError, NewFact};

async fn booted(pool: &sqlx::MySqlPool) {
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
}

/// **A retracted member's number is still issued after a restart.** The spec
/// runs against the store, then a second store and a second fold are built over
/// the same database, as a restart does, and read the next number and refuse the
/// retracted one from what the database holds.
#[tokio::test]
async fn dolt_keeps_a_retracted_members_number_issued_across_a_restart() {
    let scratch = Scratch::new("numbered-restart");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "numberedrestart")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    booted(&pool).await;
    memory::a_retracted_members_number_is_never_issued_again(&DoltMemory::open(pool.clone())).await;

    let restarted = Folded::new(Arc::new(DoltMemory::open(pool)));
    restarted.rebuild().await.expect("the image fills");
    let collection = EntityId("topic:contract-ordinances-retired".into());
    let numbering = restarted
        .numbering_of(&collection)
        .await
        .expect("the collection reads")
        .expect("it numbers its members");
    assert_eq!(numbering.next_number, 7, "1 to 4, the retracted 5, and 6");
    let signs = EntityId("decision:contract-ordinance-retired-signs".into());
    let refused = restarted
        .capture(memory::state(NewFact {
            fields: std::collections::BTreeMap::from([("number".to_string(), "5".to_string())]),
            ..NewFact::about(signs, "signs ask for five again", date(2026, 6, 6))
        }))
        .await;
    assert!(
        matches!(
            &refused,
            Err(MemoryError::NumberRetired {
                number: 5,
                next: 7,
                ..
            })
        ),
        "the retracted number is refused after a restart: {refused:?}"
    );

    store.stop().await;
}
