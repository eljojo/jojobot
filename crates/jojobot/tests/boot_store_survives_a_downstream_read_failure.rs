//! **Every pass after the store answers is best-effort, except the claims
//! import.**
//!
//! `wiring::boot_store` runs several passes over the store after it comes up
//! — badging, pointer resolution, the fold and search rebuilds, the mention
//! migration, the identity seed. None of them refuses the boot on failure:
//! each is a cache, an index or a repair over a store that is already the
//! truth, and refusing to start over one of them would be a worse outcome
//! than serving on what the store already has.
//!
//! **The claims import is the one pass that stops the boot, whatever the
//! error.** It runs once and completes before the server answers and before any
//! writer emits a claims event; a boot that went on after it failed would serve
//! a state with no history, and the first emission would make the import skip
//! for good. A transient failure heals on the restart, because nothing was
//! appended. The case drops `entity_former_handle` after the schema migration
//! that creates it has been recorded: the server comes up, the repair that
//! reads the table fails and is logged, and the import cannot read it either,
//! so the boot stops and says why.
//!
//! **A test binary of its own.** `boot_store` seeds the process-wide kind set
//! on success, and a case running beside this one would see it already
//! loaded.

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::free_port;
use jojobot_domain::clock::Clock;
use jojobot_domain::memory::kinds;

use jojobot_adapters::testing::Scratch;

#[tokio::test]
async fn boot_store_stops_when_the_claims_import_cannot_read_a_healthy_store() {
    let path = jojobot_adapters::testing::scratch_path("jojobot-boot-survives-read-failure");
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(path.clone());
    let port = free_port();

    // Bring the store up once, apply the schema, then break one table the
    // schema ledger will not know to repair on the next start.
    let mut server = Dolt::start(&path, port).await.expect("the store comes up");
    migrate::run(server.pool())
        .await
        .expect("the schema applies");
    sqlx::raw_sql("DROP TABLE entity_former_handle")
        .execute(server.pool())
        .await
        .expect("the table this case means to remove is droppable");
    server.stop().await;

    // A fresh boot against the same data directory — the shape of a real
    // restart. The schema ledger records the migration that created
    // `entity_former_handle` as already applied, so this run's own
    // `migrate::run` will not recreate it: `resolve_stale_pointer_columns`
    // will hit a real "table doesn't exist"
    // error, while every other pass reads tables that are still there.
    let refused = match jojobot::wiring::boot_store(&path, port, Clock::Real).await {
        Ok(booted) => {
            let mut store = booted.store;
            store.stop().await;
            panic!("a failed claims import must stop the boot, and the boot stood");
        }
        Err(refused) => refused,
    };
    assert!(
        refused.to_string().contains("claims import"),
        "the boot says why it stopped: {refused}"
    );
    drop(scratch);
    kinds::load_shipped();
}
