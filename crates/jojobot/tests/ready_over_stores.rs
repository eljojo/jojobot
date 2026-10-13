//! **What stops a piece of work is read from the store, over both stores.**
//!
//! The readiness question reads other things' folded fields and whether they
//! are archived. A fake that stores a list verbatim cannot show how a real store
//! returns one, so the same case runs over the fake and over a real store: work
//! whose dependencies are finished is ready, work with an unfinished one is not
//! and names it, and archiving a finished dependency blocks the work that
//! needed it.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{free_port, migrated_database, start_unhurried};
use jojobot_domain::memory::readiness::{Blocker, Why};
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{EntityId, Guarded, Memory, NewEntity, NewFact};
use jojobot_mcp::memory::ready::{Asked, Verdict, verdicts_of};

/// A piece of work holding these fields, all of them setting the thing.
async fn work(memory: &dyn Memory, handle: &str, held: &[(&str, &str)]) {
    let id = EntityId(handle.to_string());
    let added = memory
        .add_entity(NewEntity::new(id.clone(), handle.to_string(), "user-named"))
        .await;
    assert!(matches!(added, Ok(Guarded::Written(_))), "{added:?}");
    let mut note = NewFact::about(id, "what it holds", date(2026, 10, 1));
    for (key, value) in held {
        note.fields.insert(key.to_string(), value.to_string());
        note.sets.insert(key.to_string());
    }
    let captured = memory.capture(note).await;
    assert!(matches!(captured, Ok(Guarded::Written(_))), "{captured:?}");
}

/// The verdict on one piece of work, read the way a recall reads it.
async fn verdict(memory: &dyn Memory, handle: &str) -> Verdict {
    let id = EntityId(handle.to_string());
    let fields = memory.fields(&id).await.expect("the fields read");
    let entities = memory.list_entities(None).await.expect("the things list");
    let entity = entities.iter().find(|e| e.id == id).expect("it exists");
    let asked = [Asked {
        kind: entity.kind.as_token(),
        fields: &fields,
        archived: entity.archived.is_some(),
    }];
    verdicts_of(memory, &asked)
        .await
        .expect("the verdict")
        .remove(0)
}

fn unfinished(handle: &str) -> Blocker {
    Blocker::Dependency {
        handle: handle.to_string(),
        why: Why::Unfinished,
    }
}

/// The case, over any store.
async fn readiness_is_read_from_the_store(memory: &dyn Memory) {
    work(memory, "work:sigma", &[("status", "done")]).await;
    work(memory, "work:tau", &[("status", "done")]).await;
    work(memory, "work:loop-check-in", &[("status", "now")]).await;
    work(
        memory,
        "work:phi",
        &[("status", "next"), ("depends_on", "work:sigma, work:tau")],
    )
    .await;
    work(
        memory,
        "work:first-mix",
        &[
            ("status", "next"),
            ("depends_on", "work:sigma, work:loop-check-in"),
        ],
    )
    .await;

    let phi = verdict(memory, "work:phi").await;
    assert!(phi.ready && phi.blockers.is_empty(), "{:?}", phi.blockers);
    let mix = verdict(memory, "work:first-mix").await;
    assert!(!mix.ready, "an unfinished dependency blocks");
    assert_eq!(mix.blockers, [unfinished("work:loop-check-in")]);

    memory
        .archive_entity(
            &EntityId("work:sigma".to_string()),
            "no longer tracked",
            None,
        )
        .await
        .expect("it archives");
    let phi = verdict(memory, "work:phi").await;
    assert!(!phi.ready, "an archived dependency blocks");
    assert_eq!(
        phi.blockers,
        [Blocker::Dependency {
            handle: "work:sigma".to_string(),
            why: Why::Archived,
        }]
    );
}

#[tokio::test]
async fn over_the_fake_readiness_is_read_from_the_store() {
    readiness_is_read_from_the_store(&InMemoryMemory::booted()).await;
}

/// A real store of this case's own, migrated and seeded with the kinds.
async fn a_real_store(label: &str) -> (Dolt, DoltMemory, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("jojobot-ready-{}-{label}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let store = start_unhurried(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, label)
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool);
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    (store, memory, dir)
}

#[tokio::test]
async fn over_the_real_store_readiness_is_read_from_the_store() {
    let (mut store, memory, dir) = a_real_store("ready").await;
    readiness_is_read_from_the_store(&memory).await;
    store.stop().await;
    let _ = std::fs::remove_dir_all(dir);
}
