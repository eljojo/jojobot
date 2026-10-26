//! **The fold reads no more things at once than the store has connections.**
//!
//! The width of the startup read comes from the store beneath, through every
//! layer between, and never from a number of the fold's own.

use std::sync::Arc;

use jojobot_adapters::counted::Counted;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::fold::Folded;
use jojobot_adapters::observe::{Observation, Observer, Operation, Outcome};
use jojobot_adapters::provisioned::Provisioned;
use jojobot_adapters::search::IndexedMemory;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::Memory;
use jojobot_domain::memory::mention::Mentioning;
use jojobot_domain::memory::owned::Provisions;
use sqlx::mysql::MySqlPoolOptions;

/// An observer that records nothing: the layer it makes is what the case reads.
struct Silent;

impl Observer for Silent {
    fn finished(&self, _: Operation, _: Outcome, _: std::time::Duration) {}
}

/// **Every layer the binary puts over the store answers the store's pool size.**
/// A layer that kept the port's default would answer one, and the fold would
/// read one thing at a time on a store that can serve several.
#[tokio::test]
async fn every_layer_over_the_store_answers_the_pool_size() {
    let scratch = Scratch::new("fold-load-width");
    let port = free_port();
    let mut store = Dolt::start(&scratch.0, port)
        .await
        .expect("the store comes up");
    // A database of its own, then a pool of three onto it.
    let own = migrated_database(&store, "width")
        .await
        .expect("a database");
    own.close().await;
    let pool = MySqlPoolOptions::new()
        .max_connections(3)
        .connect_lazy(&format!("mysql://root@127.0.0.1:{}/width", port))
        .expect("a pool of three");

    let dolt = Arc::new(DoltMemory::open(pool.clone()));
    assert_eq!(dolt.read_width(), 3, "the store answers its pool size");
    let counted = Counted::new(dolt.clone());
    assert_eq!(counted.read_width(), 3, "Counted forwards");
    let provisioned = Arc::new(Provisioned::new(
        DoltMemory::open(pool.clone()),
        Provisions::default(),
    ));
    assert_eq!(provisioned.read_width(), 3, "Provisioned forwards");
    // **The observing layer the binary puts between the store and the fold** when
    // metrics are on: with none, it is no layer at all and the case reads nothing.
    let observed = Observation::new(Arc::new(Silent)).memory(provisioned);
    assert_eq!(observed.read_width(), 3, "Observed forwards");
    let folded = Arc::new(Folded::new(observed));
    assert_eq!(folded.read_width(), 3, "Folded forwards");
    let mentioning = Arc::new(Mentioning::new(folded));
    assert_eq!(mentioning.read_width(), 3, "Mentioning forwards");
    let indexed = IndexedMemory::new(mentioning).expect("the index opens");
    assert_eq!(indexed.read_width(), 3, "IndexedMemory forwards");
    store.stop().await;
}
