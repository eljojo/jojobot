//! **The rollback that makes the link rule safe to switch on, over the real
//! store** (rule 646). A store the rule filled is read under `log-only` as the
//! rule before it read the same values, and the way back to `on` loses no link a
//! write made while the rule was off.
//!
//! One case in its own binary: the rule is a switch of the whole process.

use std::sync::Arc;

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::fold::Folded;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::testing::contract as memory;
use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
use jojobot_domain::memory::{EntityId, LinkRule, Memory, NewEntity, NewFact, set_link_rule};

const DATABASE: &str = "link_rule_round_trip";
const MARK: &str = "@#";

async fn badge_of(pool: &sqlx::MySqlPool, handle: &str) -> String {
    sqlx::query_scalar::<_, Option<String>>("SELECT badge FROM entity WHERE id = ?")
        .bind(handle)
        .fetch_one(pool)
        .await
        .expect("the entity row is readable")
        .expect("a created entity is given a badge")
}

async fn stored(pool: &sqlx::MySqlPool, holder_badge: &str, key: &str) -> String {
    sqlx::query_scalar("SELECT value FROM field_write WHERE entity = ? AND `key` = ?")
        .bind(holder_badge)
        .bind(key)
        .fetch_one(pool)
        .await
        .expect("the stored value reads")
}

async fn link_targets(pool: &sqlx::MySqlPool, holder_badge: &str, key: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT target FROM field_link WHERE entity = ? AND `key` = ? ORDER BY target",
    )
    .bind(holder_badge)
    .bind(key)
    .fetch_all(pool)
    .await
    .expect("the link table reads")
}

fn twice(value: String) -> [Option<String>; 2] {
    [Some(value.clone()), Some(value)]
}

/// The value a read serves under a key, from the claim that holds it.
fn served(facts: Vec<jojobot_domain::memory::Fact>, key: &str) -> Option<String> {
    facts
        .into_iter()
        .find_map(|fact| fact.fields.get(key).cloned())
}

/// **Both readers of the process**: the per-thing read of the store, and the
/// image the boot fills from the store's grouped reads.
async fn served_both_ways(
    memory: &DoltMemory,
    holder: &EntityId,
    key: &str,
) -> [Option<String>; 2] {
    let direct = served(memory.recall(holder).await.unwrap(), key);
    let folded = Arc::new(
        Folded::new(Arc::new(memory.clone())).with_boot_reader(Arc::new(memory.boot_reader())),
    );
    folded.rebuild().await.unwrap();
    [direct, served(folded.recall(holder).await.unwrap(), key)]
}

#[tokio::test]
async fn a_store_the_rule_marked_reads_as_before_under_log_only_and_loses_no_link_on_the_way_back()
{
    set_link_rule(LinkRule::LogOnly);
    let scratch = Scratch::new(DATABASE);
    let store = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&store, DATABASE).await.unwrap();
    migrate::run(&pool).await.unwrap();
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory).await.unwrap();
    let [holder, first, second] = [
        "thing:contract-links-holder",
        "work:contract-links-alpha",
        "work:contract-links-beta",
    ]
    .map(|id| EntityId(id.into()));
    for (id, name) in [(&holder, "Holder"), (&first, "First"), (&second, "Second")] {
        memory
            .add_entity(NewEntity::new(id.clone(), name, "round-trip-fixture"))
            .await
            .unwrap()
            .written()
            .unwrap();
    }
    memory
        .declare_type(
            DeclaredType::new(
                "round-trip-owner",
                vec![Field::listing("owner", ValueType::Reference)],
            ),
            None,
        )
        .await
        .unwrap();
    let holder_badge = badge_of(&pool, holder.as_str()).await;
    let first_badge = badge_of(&pool, first.as_str()).await;
    let second_badge = badge_of(&pool, second.as_str()).await;

    // The store as the deployed build left it: a list that mixes a word with a
    // handle, stored as written, and the lowering pass finished.
    memory
        .capture(memory::state(NewFact {
            fields: [("labels".into(), format!("kitchen, {first}"))].into(),
            ..NewFact::about(holder.clone(), "names a word and a thing", date(2026, 8, 1))
        }))
        .await
        .unwrap()
        .written()
        .unwrap();
    memory.migrate_field_links().await.unwrap();
    assert_eq!(
        stored(&pool, &holder_badge, "labels").await,
        format!("kitchen, {first}")
    );

    // The rule on: the list is marked, and it reads as it was written.
    set_link_rule(LinkRule::On);
    memory.migrate_field_links_marked().await.unwrap();
    assert_eq!(
        stored(&pool, &holder_badge, "labels").await,
        format!("kitchen, {MARK}{first_badge}")
    );
    assert_eq!(
        served_both_ways(&memory, &holder, "labels").await,
        twice(format!("kitchen, {first}")),
        "on serves the handle"
    );

    // The rollback: every read serves what the rule before it served, and the
    // mark and its id never reach a caller.
    set_link_rule(LinkRule::LogOnly);
    memory.migrate_field_links().await.unwrap();
    assert_eq!(
        served_both_ways(&memory, &holder, "labels").await,
        twice(format!("kitchen, {first}")),
        "log-only serves the handle of a marked item in a mixed list"
    );

    // A write made while the rule is off: a declared reference key is stored as
    // the bare id the rule before it stored, with the link row it drew.
    memory
        .capture(memory::state(NewFact {
            fields: [("owner".into(), second.to_string())].into(),
            ..NewFact::about(holder.clone(), "names an owner", date(2026, 8, 2))
        }))
        .await
        .unwrap()
        .written()
        .unwrap();
    assert_eq!(stored(&pool, &holder_badge, "owner").await, second_badge);
    assert_eq!(
        link_targets(&pool, &holder_badge, "owner").await,
        vec![second_badge.clone()]
    );

    // The way back: the write of the rollback window is marked and keeps its link.
    set_link_rule(LinkRule::On);
    memory.migrate_field_links_marked().await.unwrap();
    assert_eq!(
        link_targets(&pool, &holder_badge, "owner").await,
        vec![second_badge.clone()],
        "the link of a write made while the rule was off is kept"
    );
    assert_eq!(
        stored(&pool, &holder_badge, "owner").await,
        format!("{MARK}{second_badge}"),
        "the write is marked, so the rule serves and links it"
    );
    assert_eq!(
        link_targets(&pool, &holder_badge, "labels").await,
        vec![first_badge.clone()],
        "the link of the marked list is present after the round trip"
    );
    assert_eq!(
        served_both_ways(&memory, &holder, "owner").await,
        twice(second.to_string())
    );
    pool.close().await;
    let mut store = store;
    store.stop().await;
}
