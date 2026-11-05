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
    jojobot_domain::memory::testing::contract::a_kind_declaration_is_checked_for_shape(&first)
        .await;
    server.stop().await;
}
