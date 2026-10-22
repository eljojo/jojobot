//! **How many statements the fold's full read sends the store.**
//!
//! The read behind `Folded::rebuild` must cost a number of statements that
//! follows the number of things, and not the number of claims a thing holds: a
//! store whose things carry more claims than the day before must not boot more
//! slowly per thing.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::fold::Folded;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::{Edge, EdgeShape, EntityId, Memory, NewEntity, NewFact};
use std::sync::Arc;

const GREEK: [&str; 8] = [
    "alpha", "kappa", "omega", "sigma", "lambda", "rho", "tau", "psi",
];

/// The statements the server has answered so far, from its own counter.
async fn answered(pool: &sqlx::MySqlPool) -> i64 {
    let (_, count): (String, String) = sqlx::query_as("SHOW GLOBAL STATUS LIKE 'Questions'")
        .fetch_one(pool)
        .await
        .expect("the server counts its statements");
    count.parse().expect("a count")
}

/// **A database of `things` things with `claims` claims each, and the statements
/// one full read of it sends.** Every claim carries fields and a connection edge
/// to one of two things, so the read has pointers to resolve.
async fn statements_of_a_full_read(store: &Dolt, name: &str, things: usize, claims: usize) -> i64 {
    let pool = migrated_database(store, name).await.expect("a database");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());
    let ids: Vec<EntityId> = GREEK[..things]
        .iter()
        .map(|slug| EntityId(format!("thing:{slug}")))
        .collect();
    for (at, id) in ids.iter().enumerate() {
        let added = memory
            .add_entity(NewEntity::new(
                id.clone(),
                GREEK[at].to_uppercase(),
                "fixture",
            ))
            .await
            .expect("the thing is added");
        let said = format!("{added:?}");
        added
            .written()
            .unwrap_or_else(|| panic!("nothing collides with it: {said}"));
    }
    for id in &ids {
        for claim in 0..claims {
            let target = ids[ids.len() - 1 - (claim % 2)].clone();
            let mut fact = NewFact::about(
                id.clone(),
                format!("claim number {claim}"),
                date(2026, 8, 1 + (claim % 27) as i8),
            );
            fact.edge = Some(Edge::new(EdgeShape::Connection, target));
            fact.fields.insert("note".into(), format!("n{claim}"));
            memory.capture(fact).await.expect("the claim is captured");
        }
    }
    let folded = Folded::new(Arc::new(DoltMemory::open(pool.clone())));
    let before = answered(&pool).await;
    folded.rebuild().await.expect("the image fills");
    let after = answered(&pool).await;
    // The counter read itself once.
    after - before - 1
}

/// **More claims on each thing add no statements per claim, and more things add
/// statements in proportion.** The first holds the things and varies the claims:
/// eight times the claims must not cost eight times the statements. The second
/// holds the claims and doubles the things.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_full_read_sends_statements_by_thing_and_not_by_claim() {
    let scratch = Scratch::new("fold-read-statements");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");

    let few_claims = statements_of_a_full_read(&store, "few_claims", 8, 2).await;
    let many_claims = statements_of_a_full_read(&store, "many_claims", 8, 16).await;
    let half_the_things = statements_of_a_full_read(&store, "half_things", 4, 16).await;

    assert!(
        few_claims > 0,
        "the read sent nothing, so the count measures nothing: {few_claims}"
    );
    // A few statements per thing for a longer list is allowed; a statement per
    // claim is not (8 things x 14 more claims would add 112 at one each).
    assert!(
        many_claims <= few_claims + 8 * 3,
        "eight times the claims sent {many_claims} statements against {few_claims}"
    );
    assert!(
        many_claims <= 2 * half_the_things + 20,
        "twice the things sent {many_claims} statements against {half_the_things}"
    );
    store.stop().await;
}
