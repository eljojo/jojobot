//! **A boot that stops on a row the claims import will not take says what to do.**
//! A restart fails the same way on such a row, so the error must not promise that
//! the next start tries again; it names the row and the one step, which is to
//! report the line, because the fix is a build that imports the row (rule 796).
//!
//! **A test binary of its own.** `boot_store` seeds the process-wide kind set on
//! success, and a case running beside this one would see it already loaded.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port};
use jojobot_domain::clock::Clock;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_boot_over_a_metadata_row_no_event_carries_names_the_row_and_the_step() {
    let path = jojobot_adapters::testing::scratch_path("jojobot-boot-import-failure");
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(path.clone());
    let port = free_port();

    // A store holding a row the import has no event for: a claim with a metadata
    // row, which no build writes in production.
    let mut seeding = Dolt::start(&path, port).await.expect("the store comes up");
    migrate::run(seeding.pool()).await.expect("the schema");
    let memory = DoltMemory::open(seeding.pool().clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    let homer = EntityId("person:homer".to_string());
    memory
        .add_entity(NewEntity::new(homer.clone(), "Homer", "fixture"))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    memory
        .capture(NewFact::about(homer, "likes donuts", date(2026, 9, 2)))
        .await
        .expect("capture")
        .written()
        .expect("not blocked");
    let (home, local): (String, String) = sqlx::query_as("SELECT entity, id FROM fact LIMIT 1")
        .fetch_one(seeding.pool())
        .await
        .expect("the claim is stored");
    sqlx::query("INSERT INTO fact_event_metadata (fact_home, fact_id, `key`, value) VALUES (?, ?, 'mood', 'x')")
        .bind(&home)
        .bind(&local)
        .execute(seeding.pool())
        .await
        .expect("the metadata row");
    seeding.stop().await;
    drop(memory);

    let refused = match jojobot::wiring::boot_store(&path, port, Clock::Real).await {
        Ok(_) => panic!("a boot over a row the import will not take must stop"),
        Err(refused) => refused.to_string(),
    };
    assert!(
        refused.contains("metadata row under mood"),
        "the error names the row: {refused}"
    );
    assert!(
        refused.contains("fails the same way") && refused.contains("Report this line"),
        "the error says a restart does not help and what to do: {refused}"
    );
    assert!(
        !refused.contains("the next start tries again"),
        "a data error does not promise a retry: {refused}"
    );

    drop(scratch);
    jojobot_domain::memory::kinds::load_shipped();
}
