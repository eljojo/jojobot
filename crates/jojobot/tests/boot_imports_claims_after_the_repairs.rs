//! **The claims import reads what the row repairs leave.** A merge made before
//! merge events existed has no event row until the repair of old merge events
//! runs, and the import maps a merge with an event row to a numbered merge but a
//! merge without one to a redirect only. So a boot over such a store must repair
//! first: the log then holds the numbered merge.
//!
//! **A test binary of its own.** `boot_store` seeds the process-wide kind set on
//! success, and a case running beside this one would see it already loaded.

use jiff::civil::date;
use jojobot_adapters::claims_log::codec;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port};
use jojobot_domain::clock::Clock;
use jojobot_domain::log::claims::ClaimEvent;
use jojobot_domain::memory::{EntityId, Memory, NewEntity};

async fn thing(memory: &DoltMemory, handle: &str, name: &str) -> EntityId {
    let id = EntityId(handle.to_string());
    memory
        .add_entity(NewEntity::new(id.clone(), name, "fixture"))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    id
}

/// Every claims event the log holds, as stored.
async fn logged(pool: &sqlx::MySqlPool) -> Vec<ClaimEvent> {
    let payloads: Vec<Vec<u8>> = sqlx::query_scalar(
        "SELECT payload FROM event_log WHERE first_position IS NOT NULL ORDER BY first_position",
    )
    .fetch_all(pool)
    .await
    .expect("the log is readable");
    let mut events = Vec::new();
    for payload in payloads {
        let batch: serde_json::Value = serde_json::from_slice(&payload).expect("a stored batch");
        for member in batch[5].as_array().expect("a batch holds its members") {
            let bytes: Vec<u8> = member
                .as_array()
                .expect("a member is bytes")
                .iter()
                .map(|byte| byte.as_u64().expect("a byte") as u8)
                .collect();
            if let Some(event) = codec::decode_own(&bytes).expect("every stored event reads") {
                events.push(event.payload);
            }
        }
    }
    events
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_boot_over_a_merge_from_before_merge_events_imports_it_as_a_numbered_merge() {
    let path = jojobot_adapters::testing::scratch_path("jojobot-boot-imports-after-repairs");
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(path.clone());
    let port = free_port();

    // A store as an earlier build left it: a merge, its event row gone and the
    // repair of old merge events not yet run.
    let mut seeding = Dolt::start(&path, port).await.expect("the store comes up");
    migrate::run(seeding.pool()).await.expect("the schema");
    let memory = DoltMemory::open(seeding.pool().clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    let homer = thing(&memory, "person:homer", "Homer").await;
    let bart = thing(&memory, "person:bart", "Bart").await;
    let me = thing(&memory, "bot:sigma", "Sigma").await;
    memory
        .merge(
            &bart,
            &homer,
            Some("filed twice"),
            date(2026, 9, 2),
            &me,
            None,
        )
        .await
        .expect("the merge");
    sqlx::query("DELETE FROM merge_event")
        .execute(seeding.pool())
        .await
        .expect("a store from before the merge-event table");
    sqlx::query("DELETE FROM store_pass WHERE name = 'old merge events'")
        .execute(seeding.pool())
        .await
        .expect("the repair is owed");
    seeding.stop().await;
    drop(memory);

    let booted = jojobot::wiring::boot_store(&path, port, Clock::Real)
        .await
        .expect("the boot stands");
    let events = logged(booted.store.pool()).await;
    let merges = events
        .iter()
        .filter(|event| matches!(event, ClaimEvent::ObjectsMerged { .. }))
        .count();
    assert_eq!(
        merges, 1,
        "the repair gave the merge its event before the import read it: {events:?}"
    );

    let mut store = booted.store;
    store.stop().await;
    drop(scratch);
    jojobot_domain::memory::kinds::load_shipped();
}
