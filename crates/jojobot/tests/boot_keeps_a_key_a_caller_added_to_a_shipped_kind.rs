//! **The boot's reconcile keeps what a caller added to a shipped kind.** A caller
//! may add a key to a shipped kind; the store keeps the addition beside the
//! build's keys. The boot appends the declaration the store holds, not the
//! build's list alone, so the State folds the key as the store does.
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
use jojobot_domain::memory::types::{Field, Origin, ValueType};
use jojobot_domain::memory::{Memory, kinds};

/// Every declaration of `name` the log holds, in order.
async fn declarations_of(pool: &sqlx::MySqlPool, name: &str) -> Vec<Vec<Field>> {
    let payloads: Vec<Vec<u8>> = sqlx::query_scalar(
        "SELECT payload FROM event_log WHERE first_position IS NOT NULL ORDER BY first_position",
    )
    .fetch_all(pool)
    .await
    .expect("the log is readable");
    let mut found = Vec::new();
    for payload in payloads {
        let batch: serde_json::Value = serde_json::from_slice(&payload).expect("a stored batch");
        for member in batch[5].as_array().expect("a batch holds its members") {
            let bytes: Vec<u8> = member
                .as_array()
                .expect("a member is bytes")
                .iter()
                .map(|byte| byte.as_u64().expect("a byte") as u8)
                .collect();
            if let Some(event) = codec::decode_own(&bytes).expect("every stored event reads")
                && let ClaimEvent::TypeDeclared { declaration, .. } = &event.payload
                && declaration.name == name
            {
                found.push(declaration.fields.clone());
            }
        }
    }
    found
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_boot_keeps_a_key_a_caller_added_to_a_shipped_kind() {
    let path = jojobot_adapters::testing::scratch_path("jojobot-boot-caller-kind-key");
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(path.clone());
    let port = free_port();

    let mut seeding = Dolt::start(&path, port).await.expect("the store comes up");
    migrate::run(seeding.pool()).await.expect("the schema");
    let memory = DoltMemory::open(seeding.pool().clone());
    kinds::seed(&memory).await.expect("the kinds are seeded");
    let token = "role";
    memory
        .declare_kind(
            token,
            Origin::Declared,
            vec![Field::new("laps", ValueType::Number)],
        )
        .await
        .expect("the caller's key lands");
    seeding.stop().await;
    drop(memory);
    kinds::load(Vec::<String>::new());

    let booted = jojobot::wiring::boot_store(&path, port, Clock::Real)
        .await
        .expect("the boot stands");
    let held = declarations_of(booted.store.pool(), token).await;
    let newest = held
        .last()
        .expect("the log holds a declaration of the kind");
    assert!(
        newest.iter().any(|field| field.key == "laps"),
        "the newest declaration keeps the caller's key: {held:?}"
    );

    let mut store = booted.store;
    store.stop().await;
    drop(scratch);
    kinds::load_shipped();
}
