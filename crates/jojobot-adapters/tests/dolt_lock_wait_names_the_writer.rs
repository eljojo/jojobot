//! **A write that gives up waiting for the log's single-writer lock says another
//! writer holds the log.**
//!
//! One server instance is the only writer of the log. A second writer over the same
//! store waits for the lock the first one keeps, and gives up. What the caller read
//! then was "the memory store could not be reached", the sentence for a store that
//! cannot be talked to at all, which sends a reader looking at the wrong thing.
//!
//! The first writer here is the one the claims import used and a test kept alive.

use std::sync::Arc;

use jiff::Timestamp;
use jojobot_adapters::claims_import::import_claims_once;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::{Dolt, migrate};
use jojobot_adapters::fold::Folded;
use jojobot_adapters::provisioned::Provisioned;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_adapters::writer::Writer;
use jojobot_domain::memory::owned::Provisions;
use jojobot_domain::memory::{EntityId, Memory, MemoryError, NewEntity};

/// A journaled stack over the store: the layer that resolves what the build supplies
/// over a store that writes through the log, with the fold on top.
fn stack(pool: &sqlx::MySqlPool) -> Folded {
    Folded::new(Arc::new(Provisioned::new(
        DoltMemory::open(pool.clone())
            .journaled()
            .knowing(Provisions::default()),
        Provisions::default(),
    )))
}

async fn add(folded: &Folded, handle: &str) -> Result<(), MemoryError> {
    jojobot_domain::log::claims::job::within(
        jojobot_domain::log::claims::job::test_job(&jojobot_domain::actor::Actor::Operator),
        folded.add_entity(NewEntity::new(
            EntityId(handle.to_string()),
            "A name",
            "fixture",
        )),
    )
    .await
    .map(|_| ())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_journaled_write_beside_a_held_log_says_another_writer_holds_it() {
    let scratch = Scratch::new("lock-wait-names-the-writer");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "lockwaitnames")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");

    // The first writer imports and is kept: it holds the log's lock.
    let first = Writer::default();
    import_claims_once(
        &first,
        &pool,
        &Provisions::default(),
        Timestamp::from_second(1_800_000_000).expect("a moment"),
    )
    .await
    .expect("the import");
    first.drain().await;

    let refused = add(&stack(&pool), "person:homer").await;
    match &refused {
        Err(MemoryError::Store(said)) => {
            assert!(
                said.contains("another writer holds the log"),
                "the sentence names the other writer: {said}"
            );
            assert!(
                !said.contains("could not be reached"),
                "and is not the sentence for a store that cannot be reached: {said}"
            );
        }
        other => panic!("a second writer beside a held log is refused as a store error: {other:?}"),
    }

    // The pair: the lock is the cause. Once the first writer is gone, a fresh stack
    // writes.
    drop(first);
    add(&stack(&pool), "person:marge")
        .await
        .expect("the write lands once no other writer holds the log");

    server.stop().await;
}
