//! **Two real stores in one process each answer for the kinds they declare.**
//! The shared case needs two stores, so it is run here beside the fake's run of
//! the same case: two databases on one server, one process, different kinds.

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};

#[tokio::test]
async fn two_dolt_stores_each_see_only_the_kinds_they_declare() {
    let scratch = Scratch::new("kinds-per-store");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let first = DoltMemory::open(
        migrated_database(&server, "kindsfirst")
            .await
            .expect("a database of its own"),
    );
    let second = DoltMemory::open(
        migrated_database(&server, "kindssecond")
            .await
            .expect("a second database of its own"),
    );
    jojobot_domain::memory::testing::contract::two_stores_each_see_only_the_kinds_they_declare(
        &first, &second,
    )
    .await;
    jojobot_domain::memory::testing::contract::two_stores_admit_only_the_kinds_they_declare(
        &first, &second,
    )
    .await;
    jojobot_domain::memory::testing::contract::a_kind_declaration_is_checked_for_shape(&first)
        .await;
    server.stop().await;
}

/// **A store that has not read its kinds yet admits inside its own
/// transaction without asking the pool for a second connection.** A merge opens
/// its transaction first and then checks both handles against the store's kinds.
/// On a pool of one connection a cold read through the pool waits for the
/// connection the transaction holds, so the merge would never finish.
#[tokio::test]
async fn a_cold_store_merges_on_a_pool_of_one_connection() {
    use jojobot_domain::memory::{EntityId, Memory, NewEntity};
    use std::time::Duration;

    let scratch = Scratch::new("kinds-cold-merge");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let warm = DoltMemory::open(
        migrated_database(&server, "kindscold")
            .await
            .expect("a database of its own"),
    );
    jojobot_domain::memory::kinds::seed(&warm)
        .await
        .expect("the kinds are seeded");
    let homer = EntityId("person:homer".to_string());
    let marge = EntityId("person:marge".to_string());
    for handle in [&homer, &marge] {
        warm.add_entity(NewEntity::new(handle.clone(), handle.as_str(), "fixture"))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }

    // The same database, a store that has read nothing, one connection.
    let cold = DoltMemory::open(
        server
            .database_with("kindscold", 1)
            .await
            .expect("a pool of one connection"),
    );
    let merged = tokio::time::timeout(
        Duration::from_secs(30),
        cold.merge(
            &homer,
            &marge,
            Some("filed twice"),
            jiff::civil::date(2026, 9, 2),
            &EntityId("bot:sigma".to_string()),
            Some("run-a"),
        ),
    )
    .await
    .expect("a cold merge on one connection finishes");
    merged.expect("the fold lands");
    server.stop().await;
}
