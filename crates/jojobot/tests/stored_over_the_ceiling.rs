//! **The boot names the records a store already holds over the ceiling.**
//!
//! The write refusal keeps a new record from exceeding one answer, and nothing
//! finds the ones production already holds. At boot the corpus is scanned once,
//! and each record is measured with the function the write refusal uses. This
//! is the same case on both stores: one record seeded over the ceiling is named
//! with its address, a store holding none reports none, and the measure is the
//! refusal's own.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{free_port, migrated_database, start_unhurried};
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::{EntityId, Guarded, Memory, NewEntity, NewFact};
use jojobot_domain::text::ANSWER_CEILING;
use jojobot_mcp::memory::claim_room::stored_records_over_the_ceiling;

/// Text of exactly `len` characters.
fn text(len: usize) -> String {
    "kiln "
        .repeat(len / 5 + 1)
        .chars()
        .take(len)
        .collect::<String>()
        + "k"
}

/// A thing with one ordinary record, and a second thing with one record written
/// straight into the store (the only way left to make one) over the ceiling.
/// Returns the address of the big record and the handle of its thing.
async fn seeded_with_one_over(memory: &dyn Memory) -> (String, String) {
    let today = date(2026, 10, 1);
    for (handle, name) in [
        ("thing:jukebox", "Ordinary Spot"),
        ("thing:teapot", "Heavy Spot"),
    ] {
        let id = EntityId(handle.to_string());
        let added = memory
            .add_entity(NewEntity::new(id, name.to_string(), "user-named"))
            .await;
        assert!(matches!(added, Ok(Guarded::Written(_))), "{added:?}");
    }
    let small = NewFact::about(EntityId("thing:jukebox".to_string()), "a short note", today);
    assert!(matches!(
        memory.capture(small).await,
        Ok(Guarded::Written(_))
    ));
    let mut big = NewFact::about(
        EntityId("thing:teapot".to_string()),
        "written before the guard",
        today,
    );
    big.details = Some(text(ANSWER_CEILING + 2_000));
    let Ok(Guarded::Written(big)) = memory.capture(big).await else {
        panic!("a record straight into the store lands");
    };
    (big.address().to_string(), "thing:teapot".to_string())
}

/// The case, over any store: the boot's pass names the one big record, and says
/// nothing of the ordinary one.
async fn the_scan_names_exactly_the_record_over_the_ceiling(memory: &dyn Memory) {
    let (address, thing) = seeded_with_one_over(memory).await;
    let scan = memory.scan().await.expect("the store scans");
    let found = stored_records_over_the_ceiling(&scan, jiff::Timestamp::now());
    assert_eq!(
        found
            .over
            .iter()
            .map(|o| o.address.as_str())
            .collect::<Vec<_>>(),
        [address.as_str()],
        "exactly the big record is named: {found:?}"
    );
    assert_eq!(found.over[0].thing, thing, "{found:?}");
    assert!(found.over[0].size > found.over[0].room, "{found:?}");
    assert!(found.things_over.is_empty(), "{found:?}");
    assert!(
        found.measured >= 2,
        "every record was measured, the ordinary one too: {found:?}"
    );
}

/// The pair: a store holding none reports none, and says it measured.
async fn a_store_holding_none_reports_none(memory: &dyn Memory) {
    let today = date(2026, 10, 1);
    let id = EntityId("thing:jukebox".to_string());
    let added = memory
        .add_entity(NewEntity::new(
            id.clone(),
            "Ordinary Spot".to_string(),
            "user-named",
        ))
        .await;
    assert!(matches!(added, Ok(Guarded::Written(_))), "{added:?}");
    for i in 0..3 {
        let note = NewFact::about(id.clone(), format!("a short note {i}"), today);
        assert!(matches!(
            memory.capture(note).await,
            Ok(Guarded::Written(_))
        ));
    }
    let scan = memory.scan().await.expect("the store scans");
    let found = stored_records_over_the_ceiling(&scan, jiff::Timestamp::now());
    assert!(found.over.is_empty(), "{found:?}");
    assert!(found.things_over.is_empty(), "{found:?}");
    assert!(
        found.measured >= 3,
        "the zero is a measurement and not a scan that read nothing: {found:?}"
    );
}

#[tokio::test]
async fn over_the_fake_the_scan_names_exactly_the_record_over_the_ceiling() {
    the_scan_names_exactly_the_record_over_the_ceiling(&InMemoryMemory::booted()).await;
}

#[tokio::test]
async fn over_the_fake_a_store_holding_none_reports_none() {
    a_store_holding_none_reports_none(&InMemoryMemory::booted()).await;
}

/// A real store of this case's own, migrated and seeded with the kinds.
async fn a_real_store(label: &str) -> (Dolt, DoltMemory, std::path::PathBuf) {
    let dir = jojobot_adapters::testing::scratch_path(&format!("jojobot-stored-over-{label}"));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let store = start_unhurried(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, label)
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool);
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    (store, memory, dir)
}

#[tokio::test]
async fn over_the_real_store_the_scan_names_exactly_the_record_over_the_ceiling() {
    let (mut store, memory, dir) = a_real_store("names").await;
    the_scan_names_exactly_the_record_over_the_ceiling(&memory).await;
    store.stop().await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn over_the_real_store_a_store_holding_none_reports_none() {
    let (mut store, memory, dir) = a_real_store("none").await;
    a_store_holding_none_reports_none(&memory).await;
    store.stop().await;
    let _ = std::fs::remove_dir_all(dir);
}
