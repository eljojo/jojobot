//! **The fold's startup fill sends the store a number of statements that does not
//! grow with the number of things.**
//!
//! `fold_read_statements` holds the fill to a cost by thing and not by claim. This
//! holds it to a cost by neither: a store of ten times the things is filled in the
//! same statements, give or take a fixed few, because each part of what the fold
//! holds is read for all things at once.

use std::io::Write as _;

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
        .map(|n| {
            let slug = format!("contract-fill-{n}");
            EntityId(format!("thing:{slug}"))
        })
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
    let folded = Folded::new(Arc::new(DoltMemory::open(pool.clone())));
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

    writeln!(
        std::io::stdout(),
        "fill statements: {SMALL} things {small}, {LARGE} things {large}"
    )
    .expect("the measured counts are written");
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

/// A name no other fixture wears, so a case with many things meets neither a
/// duplicate name nor the near-miss screen.
fn spelled(n: usize) -> String {
    let mut state = (n as u64 + 1).wrapping_mul(6364136223846793005);
    (0..12)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (b'a' + ((state >> 33) % 26) as u8) as char
        })
        .collect()
}

/// **The boot opens the ledgers before it fills the search index, so the index's
/// scan is served from the claims State and not read thing by thing from the
/// store.** The ledgers open on the first job the writer runs, and a boot over a
/// store whose import is done runs none before the fill. Until they open, the State
/// answers nothing and every read goes to the store: a scan of thousands of things
/// then made thousands of reads before the server served.
///
/// The case fills a fold over an imported store twice: once with the ledgers
/// closed, which shows the store being read for the scan, and once after
/// `open_ledgers`, which reads it not at all.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_scan_after_the_ledgers_open_reads_no_thing_from_the_store() {
    let dir = jojobot_adapters::testing::scratch_path("jojobot-open-ledgers");
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "open_ledgers")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let rows = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&rows)
        .await
        .expect("the kinds are seeded");
    for n in 0..20 {
        let slug = spelled(n);
        let id = EntityId(format!("{}:{}", "thing", slug));
        let new = NewEntity::new(id.clone(), slug, "fill-fixture");
        rows.add_entity(new)
            .await
            .expect("added")
            .written()
            .expect("not blocked");
        let mut fact = NewFact::about(id, "holds a value", date(2026, 9, 2));
        fact.provenance = jojobot_domain::memory::Provenance::Testimony;
        rows.capture(fact)
            .await
            .expect("captured")
            .written()
            .expect("not blocked");
    }
    jojobot_adapters::testing::import_claims_now(&pool).await;

    let counted = Arc::new(jojobot_adapters::counted::Counted::new(Arc::new(
        DoltMemory::open(pool.clone()).journaled(),
    )));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold fills");

    counted.reset();
    folded.scan().await.expect("a scan with the ledgers closed");
    let closed = counted.seen();
    assert!(
        closed.contains_key("scan_entity") || closed.contains_key("scan"),
        "with the ledgers closed the scan reads the store: {closed:?}"
    );

    folded
        .writer()
        .open_ledgers(&pool)
        .await
        .expect("the ledgers open over the imported log");
    counted.reset();
    folded.scan().await.expect("a scan with the ledgers open");
    let open = counted.seen();
    assert_eq!(open.get("scan_entity"), None, "{open:?}");
    assert_eq!(open.get("scan"), None, "{open:?}");
    server.stop().await;
}
