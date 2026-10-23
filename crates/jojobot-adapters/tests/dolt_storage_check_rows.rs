//! **The boot's storage check names a claim whose source is gone, and counts
//! damaged rows rather than distinct values** (the dangling-reference class).
//!
//! Each case starts from a store written through the real verbs, which reports no
//! damage, and makes the damage by the one edit to the rows a cut-off write leaves.

use std::path::PathBuf;

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::dolt::storage_check::DoltStorageCheck;
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::clock::Clock;
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact, Provenance};
use jojobot_domain::storage_check::{StorageCheck, StorageClass};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A store with two people and a claim on the second that is derived from a claim
/// on the first. Nothing in it is damaged.
async fn a_store_with_a_derived_claim(label: &str) -> (Scratch, Dolt, sqlx::MySqlPool) {
    let dir = jojobot_adapters::testing::scratch_path(label);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let scratch = Scratch(dir.clone());
    let server = Dolt::start(&dir, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, label)
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool.clone());
    for (handle, name) in [("person:krusty", "Krusty"), ("person:skinner", "Skinner")] {
        memory
            .add_entity(NewEntity::new(EntityId(handle.into()), name, "fixture"))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
    }
    let mut source = NewFact::about(
        EntityId("person:krusty".into()),
        "hosts the telethon",
        date(2026, 9, 1),
    );
    source.provenance = Provenance::Testimony;
    let source = memory
        .capture(source)
        .await
        .expect("capture")
        .written()
        .expect("not blocked");
    let mut derived = NewFact::about(
        EntityId("person:skinner".into()),
        "is attending the telethon",
        date(2026, 9, 2),
    );
    derived.derived_from = Some(source.address());
    memory
        .capture(derived)
        .await
        .expect("capture")
        .written()
        .expect("not blocked");
    (scratch, server, pool)
}

/// The dangling-reference finding: its row count and the ids it names.
async fn dangling(pool: &sqlx::MySqlPool) -> Option<(u64, Vec<String>)> {
    let findings = DoltStorageCheck::open(pool.clone(), Clock::default())
        .check_storage()
        .await
        .expect("the check reads");
    findings
        .into_iter()
        .find(|f| f.class == StorageClass::DanglingReference && f.count > 0)
        .map(|f| (f.count, f.ids))
}

async fn run(pool: &sqlx::MySqlPool, sql: &str) {
    sqlx::query(sqlx::AssertSqlSafe(sql.to_string()))
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"));
}

/// **A claim derived from a claim that is gone is named**, whether the claim's
/// home is gone or the home stands and the claim is not on it. Paired with the
/// whole store, which names nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_claim_derived_from_a_claim_that_does_not_exist_is_named_at_boot() {
    let (_scratch, _server, pool) = a_store_with_a_derived_claim("storage-check-derived").await;
    assert_eq!(dangling(&pool).await, None, "the derived claim is whole");

    run(
        &pool,
        "UPDATE fact SET derived_from_id = 'f99' WHERE derived_from IS NOT NULL",
    )
    .await;
    let (count, ids) = dangling(&pool).await.expect("the missing claim is named");
    assert_eq!(count, 1, "{ids:?}");
    assert_eq!(ids.len(), 1, "{ids:?}");
    assert!(
        ids[0].starts_with("fact.derived_from_id: ") && ids[0].ends_with("#f99"),
        "{ids:?}"
    );

    run(
        &pool,
        "UPDATE fact SET derived_from = 'ghosthome' WHERE derived_from IS NOT NULL",
    )
    .await;
    let (count, ids) = dangling(&pool).await.expect("the missing home is named");
    assert_eq!(count, 1, "{ids:?}");
    assert_eq!(ids, vec!["fact.derived_from: ghosthome".to_string()]);
}

/// **Rows are counted, one for each, and a value is named once.** Three claims
/// homed under one badge no row wears are three damaged rows under one name.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rows_under_one_dangling_value_are_counted_as_rows() {
    let (_scratch, _server, pool) = a_store_with_a_derived_claim("storage-check-rows").await;
    for n in 1..=3 {
        run(
            &pool,
            &format!(
                "INSERT INTO fact (entity, id, content, provenance, status, recorded_at)
                 VALUES ('ghostbadge', 'f{n}', 'a claim under a badge no row wears',
                         'testimony', 'active', '2026-10-22')"
            ),
        )
        .await;
    }
    let (count, ids) = dangling(&pool).await.expect("the badge is named");
    assert_eq!(count, 3, "three rows, not one value: {ids:?}");
    assert_eq!(ids, vec!["fact.entity: ghostbadge".to_string()]);
}

/// **A claim derived from a claim a merge brought in, named by the address the
/// survivor serves it at, is whole.** A merge moves no row: the folded thing's
/// claim keeps its stored address and the survivor serves it under an id that only
/// the merge event holds. A capture that names that served address stores it as
/// sent, and the check does not call it dangling. Paired with a claim derived from
/// an id nothing serves, which is still named.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_claim_derived_from_a_merged_in_claim_by_its_served_id_is_not_dangling() {
    let (_scratch, _server, pool) = a_store_with_a_derived_claim("storage-check-merged").await;
    let memory = DoltMemory::open(pool.clone());
    let survivor = EntityId("person:krusty".into());
    memory
        .merge(
            &EntityId("person:skinner".into()),
            &survivor,
            Some("filed twice"),
            date(2026, 9, 3),
            &EntityId("bot:sigma".into()),
            None,
        )
        .await
        .expect("the merge lands");
    let served = memory
        .recall(&survivor)
        .await
        .expect("recall")
        .into_iter()
        .find(|fact| fact.content == "is attending the telethon")
        .expect("the survivor serves the folded thing's claim");
    assert_eq!(
        dangling(&pool).await,
        None,
        "the merge alone damages nothing"
    );

    let mut built_on = NewFact::about(survivor.clone(), "will emcee it", date(2026, 9, 4));
    built_on.derived_from = Some(served.address());
    memory
        .capture(built_on)
        .await
        .expect("capture")
        .written()
        .expect("a claim served by the survivor is a source");
    assert_eq!(
        dangling(&pool).await,
        None,
        "the source is served at {}",
        served.address()
    );

    run(
        &pool,
        &format!(
            "UPDATE fact SET derived_from_id = 'f99' WHERE derived_from_id = '{}'",
            served.id
        ),
    )
    .await;
    let (count, ids) = dangling(&pool)
        .await
        .expect("an id nothing serves is named");
    assert!(count >= 1, "{ids:?}");
    assert!(ids.iter().all(|id| id.ends_with("#f99")), "{ids:?}");
}
