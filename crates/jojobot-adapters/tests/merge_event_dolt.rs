//! **A merge made before events were kept reads as it always did** (card 2248).
//!
//! The event table is new. A store that holds merges made by the code before it
//! has no row for them, and an address into such a merge gets the answer it got
//! then: the thing it names was merged away. Paired with the merge made now,
//! whose address resolves, in the same store.

use std::path::PathBuf;
use std::sync::Arc;

use jiff::civil::date;
use jojobot_adapters::counted::Counted;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::fold::Folded;
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::memory::{
    EntityId, FactAddress, Memory, MemoryError, NewEntity, NewFact, Provenance,
};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn thing(memory: &dyn Memory, handle: &str, name: &str) -> EntityId {
    let id = EntityId(handle.to_string());
    memory
        .add_entity(NewEntity::new(id.clone(), name, "fixture"))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    id
}

async fn say(memory: &dyn Memory, about: &EntityId, what: &str) -> FactAddress {
    let mut fact = NewFact::about(about.clone(), what, date(2026, 9, 1));
    fact.provenance = Provenance::Testimony;
    memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("not blocked")
        .address()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_merge_made_before_events_were_kept_reads_as_it_always_did() {
    let dir = jojobot_adapters::testing::scratch_path("jojobot-merge-event");
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "merge_event")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let store = Arc::new(DoltMemory::open(pool.clone()));
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");

    // Two merges: one made now, one made "before events were kept" — its event
    // rows are taken out after the fact, the shape a store from the older code
    // is in.
    let old_survivor = thing(&folded, "person:krusty", "Krusty").await;
    let old_folded = thing(&folded, "person:skinner", "Skinner").await;
    let new_survivor = thing(&folded, "person:homer", "Homer").await;
    let new_folded = thing(&folded, "person:apu", "Apu").await;
    let old_claim = say(&folded, &old_folded, "plays the saxophone").await;
    let new_claim = say(&folded, &new_folded, "sucks a pacifier").await;
    for (gone, kept) in [(&old_folded, &old_survivor), (&new_folded, &new_survivor)] {
        folded
            .merge(
                gone,
                kept,
                Some("filed twice"),
                date(2026, 9, 2),
                &EntityId("bot:sigma".into()),
                Some("run-a"),
            )
            .await
            .expect("the fold lands");
    }
    // The older code moved the folded thing's claim rows onto the survivor as it
    // merged, so the rows move before the event is taken out.
    for table_and_id in [("fact", "id"), ("fact_write", "fact_id")] {
        let (table, id) = table_and_id;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE {table} t JOIN merge_event m
               ON m.folded = t.entity AND m.folded_id = t.{id} AND m.ordinal > 0
             SET t.entity = m.survivor, t.{id} = m.survivor_id
             WHERE m.folded = (SELECT badge FROM entity WHERE id = 'person:skinner')"
        )))
        .execute(&pool)
        .await
        .expect("the older merge's rows are moved");
    }
    sqlx::query("DELETE FROM merge_event WHERE folded = (SELECT badge FROM entity WHERE id = 'person:skinner')")
        .execute(&pool)
        .await
        .expect("the old merge's event is taken out");

    // The rows were changed past the fold, as a store from the older code is
    // found at a start: the fold reads the store again, as a restart does.
    folded.rebuild().await.expect("the fold loads again");

    // The merge made now: an event, and an address that resolves.
    assert_eq!(
        folded.merge_events(None).await.expect("events read").len(),
        1,
        "one event stands, for the merge made with events kept"
    );
    assert!(
        folded
            .moved_address(&new_claim)
            .await
            .expect("reads")
            .is_some(),
        "the address from before the merge resolves"
    );
    assert!(
        folded.claim_history(&new_claim).await.is_ok(),
        "and reads its claim's history through the fold"
    );

    // The merge made before: no event, and the old answer.
    assert_eq!(folded.moved_address(&old_claim).await.expect("reads"), None);
    assert!(
        matches!(
            folded.claim_history(&old_claim).await,
            Err(MemoryError::AlreadyMerged { .. })
        ),
        "an address into a merge with no event says the thing was merged away"
    );
    assert!(
        matches!(
            store.claim_history(&old_claim).await,
            Err(MemoryError::AlreadyMerged { .. })
        ),
        "and so does the store"
    );
    server.stop().await;
}

/// **The writes of a claim a survivor cannot serve are read from the image over
/// the real store.** A claim lands under the folded key after the merge, so the
/// event lists no id for it. The fold reads it again at a start, names it as
/// unmapped, and holds its chain: asking for the address then makes no call to
/// the store, and the answer is the one the store gives. Paired with the claim
/// the event does map, which reads through the fold as before.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_writes_of_an_unmapped_claim_are_read_from_the_image_over_the_real_store() {
    let dir = jojobot_adapters::testing::scratch_path("jojobot-unmapped-image");
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let _scratch = Scratch(dir.clone());
    let mut server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "unmapped_image")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let store = Arc::new(DoltMemory::open(pool.clone()));
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");

    let survivor = thing(&folded, "person:krusty", "Krusty").await;
    let gone = thing(&folded, "person:skinner", "Skinner").await;
    let mapped = say(&folded, &gone, "plays the saxophone").await;
    folded
        .merge(
            &gone,
            &survivor,
            Some("filed twice"),
            date(2026, 9, 2),
            &EntityId("bot:sigma".into()),
            Some("run-a"),
        )
        .await
        .expect("the fold lands");

    // A claim lands under the folded key past the merge: the event holds no id
    // for it.
    let folded_key: String =
        sqlx::query_scalar("SELECT badge FROM entity WHERE id = 'person:skinner'")
            .fetch_one(&pool)
            .await
            .expect("the folded thing has a key");
    for statement in [
        "INSERT INTO fact (entity, id, content, provenance, status, recorded_at)
         VALUES (?, 'f9', 'a stray claim', 'inference', 'active', '2026-09-03')",
        "INSERT INTO fact_write (entity, fact_id, ordinal, content, provenance, status, recorded_at)
         VALUES (?, 'f9', 1, 'a stray claim', 'inference', 'active', '2026-09-03')",
    ] {
        sqlx::query(statement)
            .bind(&folded_key)
            .execute(&pool)
            .await
            .expect("the stray claim lands");
    }
    let stray = FactAddress::new(gone.clone(), jojobot_domain::memory::FactId("f9".into()));

    // The rows were changed past the fold, as a restart finds them.
    folded.rebuild().await.expect("the fold loads again");
    assert_eq!(
        folded
            .unmapped_claims(&survivor)
            .await
            .expect("reads")
            .iter()
            .map(|claim| claim.address.clone())
            .collect::<Vec<_>>(),
        vec![stray.clone()],
        "the survivor names the stray claim, so the case reaches the branch"
    );

    counted.reset();
    let from_image = folded
        .claim_history(&stray)
        .await
        .expect("the address reads");
    assert_eq!(
        counted.calls(),
        0,
        "the writes of an unmapped claim reached the store: {:?}",
        counted.seen()
    );
    assert_eq!(
        from_image,
        store.claim_history(&stray).await.expect("the store reads"),
        "the image answers what the store answers"
    );
    assert_eq!(from_image.len(), 1, "the claim has its one write");

    // The positive half: the claim the event maps still reads through the fold.
    assert!(
        folded.claim_history(&mapped).await.is_ok(),
        "a claim the event maps is unaffected"
    );
    server.stop().await;
}
