//! The archive a capture's drop writes, read from the real store.

use super::*;
use crate::dolt::tests::free_port;
use crate::dolt::{Dolt, migrate};
use crate::testing::{Scratch, migrated_database};

/// **A thought dropped by a capture is archived by the session that captured**,
/// on the real store.
#[tokio::test]
async fn a_dropped_thought_is_archived_by_the_capturing_session_in_the_real_store() {
    let scratch = Scratch::new("drop-session");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "drop_session")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");

    jojobot_domain::memory::testing::contract::a_dropped_thought_is_archived_by_the_session_that_captured(&memory)
        .await;
    store.stop().await;
}
