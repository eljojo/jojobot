//! **The claims import runs after the shipped kinds load.** A declared type whose
//! field points at a kind is written into the log with that kind in it, and read
//! back through the kind registry. A boot that imported before the kinds loaded
//! would write or read it against an empty registry.
//!
//! **A test binary of its own.** `boot_store` seeds the process-wide kind set on
//! success, and a case running beside this one would see it already loaded.

use jojobot_adapters::claims_log::codec;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port};
use jojobot_domain::clock::Clock;
use jojobot_domain::log::claims::ClaimEvent;
use jojobot_domain::memory::types::{DeclaredType, Field};
use jojobot_domain::memory::{EntityKind, Memory};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_boot_imports_a_declared_type_whose_field_points_at_a_kind() {
    let path = jojobot_adapters::testing::scratch_path("jojobot-boot-imports-kind-field");
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(path.clone());
    let port = free_port();

    let mut seeding = Dolt::start(&path, port).await.expect("the store comes up");
    migrate::run(seeding.pool()).await.expect("the schema");
    let memory = DoltMemory::open(seeding.pool().clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    memory
        .declare_type(
            DeclaredType::new(
                "sketch",
                vec![Field::pointing_at("venue", EntityKind::PLACE)],
            ),
            None,
        )
        .await
        .expect("the declaration lands");
    seeding.stop().await;
    drop(memory);
    // The seeding above filled the process-wide kind set. A start loads it from
    // the store, so the case begins with it empty, as a fresh process does.
    jojobot_domain::memory::kinds::load(Vec::<String>::new());

    let booted = jojobot::wiring::boot_store(&path, port, Clock::Real)
        .await
        .expect("the boot stands");
    let payloads: Vec<Vec<u8>> = sqlx::query_scalar(
        "SELECT payload FROM event_log WHERE first_position IS NOT NULL ORDER BY first_position",
    )
    .fetch_all(booted.store.pool())
    .await
    .expect("the log is readable");
    let mut declared = 0;
    for payload in payloads {
        let batch: serde_json::Value = serde_json::from_slice(&payload).expect("a stored batch");
        for member in batch[5].as_array().expect("a batch holds its members") {
            let bytes: Vec<u8> = member
                .as_array()
                .expect("a member is bytes")
                .iter()
                .map(|byte| byte.as_u64().expect("a byte") as u8)
                .collect();
            let event = codec::decode_own(&bytes).expect("every stored event reads");
            if let Some(event) = event
                && let ClaimEvent::TypeDeclared { declaration, .. } = &event.payload
                && declaration.name == "sketch"
            {
                declared += 1;
                assert_eq!(
                    declaration.fields,
                    vec![Field::pointing_at("venue", EntityKind::PLACE)],
                    "the kind survived the journey through the log"
                );
            }
        }
    }
    assert_eq!(declared, 1, "the boot imported the declaration");

    let mut store = booted.store;
    store.stop().await;
    drop(scratch);
    jojobot_domain::memory::kinds::load_shipped();
}
