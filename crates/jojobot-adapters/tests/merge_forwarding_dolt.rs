//! **Every path that follows a merge follows the same ones** (card 2428).
//!
//! A merge made before events were kept leaves only a forwarding row, and
//! `index()` reads that row as the merge. These cases take the same shape from
//! the other paths, each paired with a merge that kept its events in the same
//! store.

use std::path::PathBuf;

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::memory::{
    EntityId, FactAddress, Memory, MemoryError, NewEntity, NewFact, Provenance,
};
use sqlx::MySqlPool;

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A store of its own: the server, its pool and the memory over it.
async fn store_of(name: &str) -> (Scratch, Dolt, MySqlPool, DoltMemory) {
    let dir = std::env::temp_dir().join(format!("jojobot-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let scratch = Scratch(dir.clone());
    let server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, name)
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    (scratch, server, pool, memory)
}

async fn thing(
    memory: &dyn Memory,
    handle: &str,
    name: &str,
    parent: Option<&EntityId>,
) -> EntityId {
    let id = EntityId(handle.to_string());
    memory
        .add_entity(NewEntity {
            parent: parent.cloned(),
            ..NewEntity::new(id.clone(), name, "fixture")
        })
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    id
}

async fn say(
    memory: &dyn Memory,
    about: &EntityId,
    what: &str,
    fields: &[(&str, &str)],
) -> FactAddress {
    let mut fact = NewFact::about(about.clone(), what, date(2026, 9, 1));
    fact.provenance = Provenance::Testimony;
    fact.fields = fields
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    fact.sets = fact.fields.keys().cloned().collect();
    memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("not blocked")
        .address()
}

async fn merge(memory: &dyn Memory, folded: &EntityId, survivor: &EntityId) {
    memory
        .merge(
            folded,
            survivor,
            Some("filed twice"),
            date(2026, 9, 2),
            &EntityId("bot:sigma".into()),
            Some("run-a"),
        )
        .await
        .expect("the fold lands");
}

/// The merge made "before events were kept": its event rows are taken out,
/// the forwarding row stays.
async fn forget_the_events_of(pool: &MySqlPool, folded: &str) {
    sqlx::query("DELETE FROM merge_event WHERE folded = (SELECT badge FROM entity WHERE id = ?)")
        .bind(folded)
        .execute(pool)
        .await
        .expect("the event is taken out");
}

/// **The children a fold serves under the survivor are the same whether the
/// merge kept an event or left a forwarding row.** A numbered collection counts
/// the number a child holds under a folded parent as one of its own, so the next
/// number and the refusal of a taken one are the questions that tell.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_children_of_a_fold_without_an_event_are_served_under_its_survivor() {
    let (_scratch, mut server, pool, memory) = store_of("forwarded-children").await;

    // The merge made before events were kept.
    let old_survivor = thing(&memory, "person:krusty", "Krusty", None).await;
    say(
        &memory,
        &old_survivor,
        "numbers its members",
        &[("numbers_by", "number")],
    )
    .await;
    let old_folded = thing(&memory, "person:skinner", "Skinner", None).await;
    let old_child = thing(&memory, "person:bart", "Bart", Some(&old_folded)).await;
    say(&memory, &old_child, "holds three", &[("number", "3")]).await;
    merge(&memory, &old_folded, &old_survivor).await;
    forget_the_events_of(&pool, "person:skinner").await;

    // The merge made with its event kept.
    let new_survivor = thing(&memory, "person:burns", "Burns", None).await;
    say(
        &memory,
        &new_survivor,
        "numbers its members",
        &[("numbers_by", "number")],
    )
    .await;
    let new_folded = thing(&memory, "person:homer", "Homer", None).await;
    let new_child = thing(&memory, "person:lisa", "Lisa", Some(&new_folded)).await;
    say(&memory, &new_child, "holds seven", &[("number", "7")]).await;
    merge(&memory, &new_folded, &new_survivor).await;

    let next = |survivor: EntityId| {
        let memory = &memory;
        async move {
            memory
                .numbering_of(&survivor)
                .await
                .expect("reads")
                .expect("numbered")
                .next_number
        }
    };
    assert_eq!(
        next(new_survivor.clone()).await,
        8,
        "the control: a child of a fold with an event is a member"
    );
    assert_eq!(
        next(old_survivor.clone()).await,
        4,
        "a child of a fold with only a forwarding row is a member too"
    );

    let taken = memory
        .capture({
            let mut fact = NewFact::about(
                thing(&memory, "person:apu", "Apu", Some(&old_survivor)).await,
                "asks for three",
                date(2026, 9, 3),
            );
            fact.fields = [("number".to_string(), "3".to_string())].into();
            fact.sets = fact.fields.keys().cloned().collect();
            fact
        })
        .await;
    assert!(
        matches!(&taken, Err(MemoryError::NumberTaken { holder, .. }) if holder == "person:bart"),
        "a number held under a folded parent stays held: {taken:?}"
    );
    server.stop().await;
}
