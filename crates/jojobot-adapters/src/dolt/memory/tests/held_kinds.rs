//! **What a transaction sees of the kinds is not kept for the store.** A
//! transaction that began before a kind was declared may read the kinds as they
//! stood when it began; the set the store holds for every later call must be
//! what the store declares now.

use super::*;

#[tokio::test]
async fn a_transaction_that_began_before_a_declaration_does_not_leave_its_kinds_behind() {
    let pool = shared_database("heldkindstx")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");

    // A transaction that has read the kinds, so it holds the view it began with.
    let mut tx = pool.begin().await.expect("a transaction");
    let _: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM kind")
        .fetch_one(&mut *tx)
        .await
        .expect("the transaction reads");
    // A kind is declared while it is open, and the old transaction then asks.
    memory
        .declare_kind("otter", Origin::Declared, Vec::new())
        .await
        .expect("the kind is declared");
    memory
        .own_kinds_in(&mut tx)
        .await
        .expect("the transaction asks");
    tx.rollback().await.expect("the transaction ends");

    let held = memory.own_kinds().await.expect("the store answers");
    assert!(
        held.standing("otter").is_ok(),
        "the store forgot a kind it declared: {held:?}"
    );
}
