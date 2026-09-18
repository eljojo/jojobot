//! **The exact ordering `wiring::open_provisioned` exists to enforce.**
//!
//! `guard_supplied_records` reads every stored entity, and reading one means
//! parsing its handle's kind. A process that has not yet seeded the kind set
//! cannot do that read — `kinds::resolve` answers `SetNeverLoaded`. So the
//! kinds have to be seeded before the guard ever runs.
//!
//! Getting this backwards once took the whole server down at boot: the boot
//! called the guard first, and the guard's own `list_entities` hit the first
//! stored row before any kind was loaded, refusing every restart with "the
//! kind set was never loaded" — on a store that was perfectly fine.
//!
//! **A test binary of its own, because the set is process-wide.** A case
//! running beside this one would load it, and then this one would be
//! asserting about a state it is not in.

use std::path::PathBuf;

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::free_port;
use jojobot_domain::memory::owned::Provisions;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, kinds};

/// A directory of this run's own, removed when it is done.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn open_provisioned_seeds_kinds_before_the_guard_reads_a_stored_row() {
    let path = std::env::temp_dir().join(format!("jojobot-boot-order-{}", std::process::id()));
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(path.clone());
    let mut server = Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    migrate::run(server.pool()).await.expect("the schema");

    // **A store carrying a row from an earlier boot.** Seeded and written the
    // way any instance's does, then the set is emptied — the state a NEW
    // process picking this store back up starts in, exactly the state a
    // restart is in before its own boot has run.
    let seeding = DoltMemory::open(server.pool().clone());
    kinds::seed(&seeding).await.expect("the kinds are declared");
    seeding
        .add_entity(NewEntity::new(
            EntityId("bot:assistant".into()),
            "Assistant",
            "test",
        ))
        .await
        .expect("the entity is written");
    kinds::load::<[&str; 0], &str>([]);

    let bare = DoltMemory::open(server.pool().clone());
    let resolved = jojobot::wiring::open_provisioned(bare, Provisions::new(vec![]))
        .await
        .expect(
            "a boot that seeds the kinds before the guard reads a row must be able to serve the \
             store it was handed, not refuse it as though it were damaged",
        );

    // The read the guard itself took has to have actually succeeded, not only
    // `open_provisioned`'s own `?`.
    let rows = resolved
        .list_entities(None)
        .await
        .expect("a process that seeded before reading can read the row it wrote earlier");
    assert!(
        rows.iter()
            .any(|e| e.id == EntityId("bot:assistant".into())),
        "the row written before this process started must still be readable: {rows:?}",
    );

    server.stop().await;
    drop(scratch);
    kinds::load_shipped();
}
