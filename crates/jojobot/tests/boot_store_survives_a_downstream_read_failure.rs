//! **Everything after the store answers is best-effort.**
//!
//! `wiring::boot_store` runs several passes over the store after it comes up
//! — badging, pointer resolution, the fold and search rebuilds, the mention
//! migration, the identity seed. None of them refuses the boot on failure:
//! each is a cache, an index or a repair over a store that is already the
//! truth, and refusing to start over one of them would be a worse outcome
//! than serving on what the store already has.
//!
//! This is the shape of the actual outage, generalised: a read taken after
//! the store itself is healthy failed, and the boot crash-looped on it as
//! though the store were down. Two of these passes —
//! `resolve_stale_pointer_columns` and `migrate_reference_fields` — read
//! `entity_former_handle`. Dropping that table after the schema migration
//! that creates it has already been recorded gives a real store where the
//! server comes up fine, the schema ledger says nothing needs re-applying,
//! and those two specific reads fail with a real SQL error — while `entity`
//! itself, and everything boot needs it for, is untouched.
//!
//! **A test binary of its own.** `boot_store` seeds the process-wide kind set
//! on success, and a case running beside this one would see it already
//! loaded.

use std::path::PathBuf;

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::free_port;
use jojobot_domain::clock::Clock;
use jojobot_domain::memory::kinds;

/// A directory of this run's own, removed when it is done.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn boot_store_does_not_refuse_when_a_downstream_read_fails_on_an_otherwise_healthy_store() {
    let path = std::env::temp_dir().join(format!(
        "jojobot-boot-survives-read-failure-{}",
        std::process::id()
    ));
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
    // and `migrate_reference_fields` will hit a real "table doesn't exist"
    // error, while every other pass reads tables that are still there.
    let booted = jojobot::wiring::boot_store(&path, port, Clock::Real)
        .await
        .expect(
            "a downstream read failing after the store itself came up healthy must not refuse \
             the boot — that is the outage this sequence exists to not repeat",
        );

    // The boot did not merely fail to error — it produced a store a caller
    // can actually use, proving the failure was swallowed rather than the
    // whole sequence quietly skipped.
    booted
        .memory
        .list_entities(None)
        .await
        .expect("memory assembled despite the downstream failure is still readable");

    let mut store = booted.store;
    store.stop().await;
    drop(scratch);
    kinds::load_shipped();
}
