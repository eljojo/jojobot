//! **A handle the build stopped supplying is adopted under ONE key.** The boot's
//! adoption pass gives such a handle a row, and the claims import runs after it,
//! so the import finds a stored thing and files the claims under the row's key.
//! The import mints its own adopted key only for a handle no row wears, so the
//! two never name one thing twice: the log holds no `adopted/` object, and the
//! handle resolves to the object of the row.
//!
//! **A test binary of its own.** `boot_store` seeds the process-wide kind set on
//! success, and a case running beside this one would see it already loaded.

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port};
use jojobot_domain::clock::Clock;
use jojobot_domain::log::claims::{ClaimEvent, State};

const RETIRED: &str = "view:upgrade-fixture-retired";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_retired_supplied_handle_has_one_key_across_the_boot_and_the_import() {
    let path = jojobot_adapters::testing::scratch_path("jojobot-boot-adopts-one-key");
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(path.clone());
    let port = free_port();

    // A store as an earlier build left it: a claim filed under the handle's own
    // text, no row, and the handle listed as one a build supplied.
    let mut seeding = Dolt::start(&path, port).await.expect("the store comes up");
    migrate::run(seeding.pool()).await.expect("the schema");
    let memory = DoltMemory::open(seeding.pool().clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    for statement in [
        "INSERT INTO fact (entity, id, content, details, provenance, standing, status,
                           recorded_at, edge_shape, edge_object, event_kind, derived_from,
                           derived_from_id, inserted_at, stale_after, happened_at,
                           happened_through)
         VALUES ('view:upgrade-fixture-retired', 'f1', 'written while the build supplied it',
                 NULL, 'testimony', 'settled', 'active', '2026-10-08', NULL, NULL, NULL, NULL,
                 NULL, '2026-10-08T14:02:10Z', NULL, NULL, NULL)",
        "INSERT INTO fact_write (entity, fact_id, ordinal, content, details, provenance,
                                standing, status, recorded_at, edge_shape, edge_object,
                                derived_from, derived_from_id, inserted_at, stale_after,
                                written_at, happened_at, happened_through, session)
         VALUES ('view:upgrade-fixture-retired', 'f1', 1, 'written while the build supplied it',
                 NULL, 'testimony', 'settled', 'active', '2026-10-08', NULL, NULL, NULL, NULL,
                 '2026-10-08T14:02:10Z', NULL, '2026-10-08T14:02:10Z', NULL, NULL, NULL)",
        "INSERT INTO supplied_handle (handle) VALUES ('view:upgrade-fixture-retired')",
    ] {
        sqlx::query(statement)
            .execute(seeding.pool())
            .await
            .expect("the stored shape lands");
    }
    seeding.stop().await;
    drop(memory);

    let booted = jojobot::wiring::boot_store(&path, port, Clock::Real)
        .await
        .expect("the boot stands");
    let events = jojobot_adapters::testing::claims_events(booted.store.pool()).await;
    let created: Vec<String> = events
        .iter()
        .filter_map(|event| match &event.payload {
            ClaimEvent::ObjectCreated { object, .. } => Some(object.0.clone()),
            _ => None,
        })
        .collect();
    assert!(
        created.iter().all(|key| !key.starts_with("adopted/")),
        "the import minted a second key for a handle the boot gave a row: {created:?}"
    );
    let state = State::replay(events.iter());
    let key = state
        .resolve(RETIRED)
        .expect("the retired handle resolves in the log")
        .clone();
    assert!(
        !key.0.starts_with("adopted/") && !key.0.starts_with("supplied/"),
        "the handle resolves to the key of its row: {key:?}"
    );
    assert_eq!(
        state.active_records(&key).len(),
        1,
        "the claim sits on that one object"
    );

    let mut store = booted.store;
    store.stop().await;
    drop(scratch);
    jojobot_domain::memory::kinds::load_shipped();
}
