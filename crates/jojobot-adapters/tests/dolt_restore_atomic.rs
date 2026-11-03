//! **A restore and the history row that records it are one batch, in the real
//! store.**
//!
//! A restore rewrites the thing's row and then files the version a history read
//! returns. The shared contract case restores a healthy store and sees both.
//! This case fails the history write from inside the batch, after the thing's
//! row is already rewritten in it, and asserts the thing is still archived and
//! no version was filed.

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::{EntityId, Memory, NewEntity};

async fn versions(pool: &sqlx::MySqlPool, thing: &EntityId) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM entity_write WHERE entity = ?")
        .bind(thing.as_str())
        .fetch_one(pool)
        .await
        .expect("the versions count")
}

async fn is_archived(memory: &DoltMemory, thing: &EntityId) -> bool {
    memory
        .list_entities(None)
        .await
        .expect("the roster reads")
        .iter()
        .find(|entity| &entity.id == thing)
        .expect("the thing is on the roster")
        .archived
        .is_some()
}

#[tokio::test]
async fn a_restore_and_its_history_claim_land_together() {
    let scratch = Scratch::new("restore-atomic");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "restoreatomic")
        .await
        .expect("a database of its own");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");

    let kappa = EntityId("work:kappa".to_string());
    memory
        .add_entity(NewEntity::new(kappa.clone(), "Kappa", "contract-fixture"))
        .await
        .expect("the thing is made")
        .written()
        .expect("nothing collides with it");
    memory
        .archive_entity(&kappa, "filed twice", None)
        .await
        .expect("the thing is archived");
    let before = versions(&pool, &kappa).await;
    assert!(is_archived(&memory, &kappa).await);

    // **The history table is out of reach while the restore runs**, so the
    // version is refused after the thing's row is rewritten in the batch.
    // Nothing the call is asked is wrong; the store fails it.
    sqlx::query("RENAME TABLE entity_write TO entity_write_away")
        .execute(&pool)
        .await
        .expect("the history table is moved aside");
    let failed = memory.restore_entity(&kappa, None).await;
    sqlx::query("RENAME TABLE entity_write_away TO entity_write")
        .execute(&pool)
        .await
        .expect("the history table is put back");
    assert!(
        failed.is_err(),
        "the store failed the history write, so the restore fails: {failed:?}"
    );
    assert!(
        is_archived(&memory, &kappa).await,
        "a restore whose history write failed still restored {kappa}"
    );
    assert_eq!(
        versions(&pool, &kappa).await,
        before,
        "a failed restore filed a version"
    );

    // **The same restore, once the table is back, does both**, so the miss above
    // is the failure and not a thing this store cannot restore.
    memory
        .restore_entity(&kappa, None)
        .await
        .expect("the restore is allowed once the store is whole");
    assert!(
        !is_archived(&memory, &kappa).await,
        "the restore takes the archive off"
    );
    assert_eq!(
        versions(&pool, &kappa).await,
        before + 1,
        "the restore filed its version"
    );

    server.stop().await;
}
