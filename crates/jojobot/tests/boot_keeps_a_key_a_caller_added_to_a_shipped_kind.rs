//! **The boot's reconcile keeps what a caller added to a shipped kind.** A caller
//! may add a key to a shipped kind; the store keeps the addition beside the
//! build's keys. The boot appends the declaration the store holds, not the
//! build's list alone, so the State folds the key as the store does.
//!
//! **A test binary of its own.** `boot_store` seeds the process-wide kind set on
//! success, and a case running beside this one would see it already loaded.

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, claims_events, free_port};
use jojobot_domain::clock::Clock;
use jojobot_domain::log::claims::ClaimEvent;
use jojobot_domain::memory::types::{Field, Origin, ValueType};
use jojobot_domain::memory::{Memory, kinds};

/// Every declaration of `name` the log holds, in order.
async fn declarations_of(pool: &sqlx::MySqlPool, name: &str) -> Vec<Vec<Field>> {
    claims_events(pool)
        .await
        .into_iter()
        .filter_map(|event| match event.payload {
            ClaimEvent::TypeDeclared { declaration, .. } if declaration.name == name => {
                Some(declaration.fields)
            }
            _ => None,
        })
        .collect()
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

    // **A second boot appends nothing.** The reconcile of the next boot compares the
    // State the log gives with the declarations the store holds under the shipped
    // names; when the store's declaration is shaped or ordered differently from the
    // State's, every boot would append one. Run that same comparison now.
    let events = jojobot_adapters::testing::claims_events(booted.store.pool()).await;
    let state = jojobot_domain::log::claims::State::replay(events.iter());
    let shipped: std::collections::BTreeSet<String> = kinds::shipped_declarations()
        .into_iter()
        .chain(jojobot_mcp::seed::shipped_types())
        .map(|declaration| declaration.name)
        .collect();
    let held: Vec<_> = booted
        .served
        .declared_types()
        .await
        .expect("the store reads its declarations")
        .into_iter()
        .filter(|declaration| shipped.contains(&declaration.name))
        .collect();
    assert!(
        !held.is_empty(),
        "the store holds declarations under the shipped names"
    );
    let again = jojobot_domain::log::claims::import::reconcile_shipped_declarations(&state, &held);
    assert!(
        again.is_empty(),
        "a second boot would append {} declarations: {again:?}",
        again.len()
    );

    let mut store = booted.store;
    store.stop().await;
    drop(scratch);
    kinds::load_shipped();
}
