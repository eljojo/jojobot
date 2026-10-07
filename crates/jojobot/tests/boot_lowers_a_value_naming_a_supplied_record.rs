//! **A value from before ids were kept that names a record the build supplies is
//! lowered at boot, like one that names a stored row.**
//!
//! The record has no row, so the pass that lowers old plain text has to be told
//! what the build supplies, or it reads the handle as naming nothing and leaves
//! the value as text with no link row. Only the boot wires that, so only a boot
//! over a store holding such a value can say whether it did.
//!
//! **A test binary of its own.** `boot_store` seeds the process-wide kind set on
//! success, and a case running beside this one would see it already loaded.

use std::path::PathBuf;

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::free_port;
use jojobot_domain::clock::Clock;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact, kinds};

/// A directory of this run's own, removed when it is done.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn a_boot_lowers_an_old_value_naming_a_record_the_build_supplies() {
    let path = std::env::temp_dir().join(format!("jojobot-boot-supplied-{}", std::process::id()));
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(path.clone());
    let port = free_port();

    // **A store an earlier build left behind**: one claim whose field holds the
    // plain text of a shipped view's handle, and no link rows.
    let mut server = Dolt::start(&path, port).await.expect("the store comes up");
    migrate::run(server.pool())
        .await
        .expect("the schema applies");
    let seeding = DoltMemory::open(server.pool().clone());
    kinds::seed(&seeding).await.expect("the kinds are declared");
    let holder = EntityId("thing:handcart".into());
    seeding
        .add_entity(NewEntity::new(holder.clone(), "The Handcart", "test"))
        .await
        .expect("add_entity ok")
        .written()
        .expect("nothing collides");
    let written = seeding
        .capture(NewFact {
            fields: [("watches".to_string(), "words".to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(
                holder.clone(),
                "keeps an eye on the loops",
                jiff::civil::date(2026, 8, 1),
            )
        })
        .await
        .expect("capture ok")
        .written()
        .expect("the claim lands");
    let badge: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind(holder.as_str())
        .fetch_one(server.pool())
        .await
        .expect("the holder wears a badge");
    sqlx::query(
        "UPDATE field_write SET value = 'view:loops' WHERE entity = ? AND `key` = 'watches' AND fact_id = ?",
    )
    .bind(&badge)
    .bind(written.id.as_str())
    .execute(server.pool())
    .await
    .expect("the old shape is written");
    sqlx::query("DELETE FROM field_link")
        .execute(server.pool())
        .await
        .expect("the table starts empty");
    server.stop().await;

    let booted = jojobot::wiring::boot_store(&path, port, Clock::Real)
        .await
        .expect("the boot stands");

    let value: String = sqlx::query_scalar(
        "SELECT value FROM field_write WHERE entity = ? AND `key` = 'watches' AND fact_id = ?",
    )
    .bind(&badge)
    .bind(written.id.as_str())
    .fetch_one(booted.store.pool())
    .await
    .expect("the write reads");
    let links: Vec<String> =
        sqlx::query_scalar("SELECT target FROM field_link WHERE entity = ? AND `key` = 'watches'")
            .bind(&badge)
            .fetch_all(booted.store.pool())
            .await
            .expect("the links read");
    assert_eq!(
        (value.as_str(), links.as_slice()),
        ("@#view:loops", ["view:loops".to_string()].as_slice()),
        "the value naming a supplied record is lowered and linked at boot",
    );

    let mut store = booted.store;
    store.stop().await;
    drop(scratch);
    kinds::load_shipped();
}
