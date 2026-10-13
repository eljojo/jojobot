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
    let dir = std::env::temp_dir().join(format!("jojobot-merge-event-{}", std::process::id()));
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
    let new_survivor = thing(&folded, "person:flanders", "Flanders").await;
    let new_folded = thing(&folded, "person:willie", "Willie").await;
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
    sqlx::query("DELETE FROM merge_event WHERE folded = (SELECT badge FROM entity WHERE id = 'person:skinner')")
        .execute(&pool)
        .await
        .expect("the old merge's event is taken out");

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
