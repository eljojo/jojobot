//! **The claims import runs after the shipped kinds load.** A declared type whose
//! field points at a kind is written into the log with that kind in it, and read
//! back through the kind registry. A boot that imported before the kinds loaded
//! would write or read it against an empty registry.
//!
//! **A test binary of its own.** `boot_store` seeds the process-wide kind set on
//! success, and a case running beside this one would see it already loaded.

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, claims_events, free_port};
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
    let declared: Vec<Vec<Field>> = claims_events(booted.store.pool())
        .await
        .into_iter()
        .filter_map(|event| match event.payload {
            ClaimEvent::TypeDeclared { declaration, .. } if declaration.name == "sketch" => {
                Some(declaration.fields)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        declared,
        vec![vec![Field::pointing_at("venue", EntityKind::PLACE)]],
        "the boot imported the declaration once, and its kind survived the journey through the log"
    );

    let mut store = booted.store;
    store.stop().await;
    drop(scratch);
    jojobot_domain::memory::kinds::load_shipped();
}
