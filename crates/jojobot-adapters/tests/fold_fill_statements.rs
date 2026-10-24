//! **The fold's startup fill sends the store a number of statements that does not
//! grow with the number of things.**
//!
//! `fold_read_statements` holds the fill to a cost by thing and not by claim. This
//! holds it to a cost by neither: a store of ten times the things is filled in the
//! same statements, give or take a fixed few, because each part of what the fold
//! holds is read for all things at once.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::fold::Folded;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::{
    Edge, EdgeShape, EntityId, Guarded, Memory, NewEntity, NewFact, guard,
};
use std::sync::Arc;

/// The statements the server has answered so far, from its own counter, which
/// does not move with the load on the machine.
async fn answered(pool: &sqlx::MySqlPool) -> i64 {
    let (_, count): (String, String) = sqlx::query_as("SHOW GLOBAL STATUS LIKE 'Questions'")
        .fetch_one(pool)
        .await
        .expect("the server counts its statements");
    count.parse().expect("a count")
}

/// **A database of `things` things with one claim each, and the statements one
/// full read of it sends.** Each claim carries a field and a connection edge to
/// one of two things, so the read has pointers to resolve.
async fn statements_of_a_fill(store: &Dolt, name: &str, things: usize) -> i64 {
    let pool = migrated_database(store, name).await.expect("a database");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());
    let ids: Vec<EntityId> = (0..things)
        .map(|n| EntityId(format!("thing:contract-fill-{n}")))
        .collect();
    for (at, id) in ids.iter().enumerate() {
        // Things of one family share words, and the guard asks to be told apart.
        let new = NewEntity::new(id.clone(), format!("Fill row {at}"), "fill-fixture");
        match memory
            .add_entity(new.clone())
            .await
            .expect("the thing is added")
        {
            Guarded::Blocked {
                attempted,
                candidates,
            } => {
                memory
                    .add_entity(NewEntity {
                        override_token: Some(guard::override_token(&attempted, &candidates)),
                        override_reason: Some("a fixture of the same family".into()),
                        ..new
                    })
                    .await
                    .expect("the thing is added again")
                    .written()
                    .expect("not blocked twice");
            }
            other => {
                other.written().expect("written");
            }
        }
    }
    for (at, id) in ids.iter().enumerate() {
        let target = ids[if at % 2 == 0 { 0 } else { 1 }].clone();
        let mut fact = NewFact::about(id.clone(), format!("claim of row {at}"), date(2026, 8, 1));
        fact.edge = Some(Edge::new(EdgeShape::Connection, target));
        fact.fields.insert("note".into(), format!("n{at}"));
        memory.capture(fact).await.expect("the claim is captured");
    }
    // **As the binary wires it**: the fold is given the store's boot reader.
    let folded = Folded::new(Arc::new(DoltMemory::open(pool.clone())))
        .with_boot_reader(Arc::new(DoltMemory::open(pool.clone()).boot_reader()));
    let before = answered(&pool).await;
    folded.rebuild().await.expect("the image fills");
    let after = answered(&pool).await;
    // The counter read itself once.
    after - before - 1
}

/// **Ten times the things send the same statements, give or take a fixed few.**
/// Paired: the smaller fill sends statements at all, so the count measures
/// something. A fill that reads each thing on its own sends about ten times as
/// many for the larger store.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ten_times_the_things_fill_the_fold_in_the_same_statements() {
    const SMALL: usize = 20;
    const LARGE: usize = 200;
    // A fixed difference the fill may add for the larger store: the listing
    // reads that grow with the rows they return, not with the things.
    const SLACK: i64 = 60;
    let scratch = Scratch::new("fold-fill-statements");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");

    let small = statements_of_a_fill(&store, "small_fill", SMALL).await;
    let large = statements_of_a_fill(&store, "large_fill", LARGE).await;

    eprintln!("fill statements: {SMALL} things {small}, {LARGE} things {large}");
    assert!(
        small > 0,
        "the fill sent nothing, so the count measures nothing: {small}"
    );
    assert!(
        large <= small + SLACK,
        "{LARGE} things sent {large} statements against {small} for {SMALL}"
    );
    store.stop().await;
}
