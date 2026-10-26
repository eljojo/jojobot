//! The shipped switch and the rollback both go through the parser the boot uses.
//! A binary of its own because the selected rule is process-wide.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::testing::contract as memory;
use jojobot_domain::memory::{EntityId, LinkRule, Memory, NewEntity, NewFact, link_rule};

#[path = "link_rule_on_repair_order/logs.rs"]
mod logs;

#[tokio::test]
async fn the_default_link_rule_is_on_and_the_boot_logs_its_mode_and_conversion_counts() {
    for word in [None, Some(""), Some("  "), Some("on")] {
        jojobot::wiring::set_link_rule_from(word).unwrap();
        assert_eq!(link_rule(), LinkRule::On, "{word:?}");
    }
    jojobot::wiring::set_link_rule_from(Some("log-only")).unwrap();
    let scratch = Scratch::new("link-rule-default-boot");
    let mut store = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&store, "link_rule_default_boot")
        .await
        .unwrap();
    migrate::run(&pool).await.unwrap();
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory).await.unwrap();
    let holder = EntityId("thing:contract-links-holder".into());
    let target = EntityId("work:contract-links-alpha".into());
    for (id, name) in [(&holder, "Holder"), (&target, "Alpha")] {
        memory
            .add_entity(NewEntity::new(id.clone(), name, "contract-fixture"))
            .await
            .unwrap()
            .written()
            .unwrap();
    }
    memory
        .capture(memory::state(NewFact {
            fields: [("labels".into(), format!("kitchen, {target}"))].into(),
            ..NewFact::about(holder.clone(), "a legacy mixed link", date(2026, 8, 1))
        }))
        .await
        .unwrap()
        .written()
        .unwrap();
    let before: String = sqlx::query_scalar("SELECT value FROM field_write WHERE `key` = 'labels'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, format!("kitchen, {target}"));
    jojobot::wiring::set_link_rule_from(None).unwrap();
    let log = logs::boot(&memory).await;
    assert!(log.contains("mode=\"on\""), "{log}");
    let conversion = log
        .lines()
        .find(|line| line.contains("r1_owned_now=") && line.contains("links_added="))
        .unwrap_or_else(|| panic!("no On conversion summary: {log}"));
    for count in [
        "unresolved=0",
        "ambiguous=0",
        "links_dropped=0",
        "links_added=1",
        "mixed_lists=1",
        "handles=1",
        "r1_owned_now=0",
        "r1_historical=0",
    ] {
        assert!(
            conversion.split_whitespace().any(|field| field == count),
            "{conversion}"
        );
    }
    let target_badge: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind(target.as_str())
        .fetch_one(&pool)
        .await
        .unwrap();
    let after: String = sqlx::query_scalar("SELECT value FROM field_write WHERE `key` = 'labels'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, format!("kitchen, @#{target_badge}"));
    let linked: Vec<String> =
        sqlx::query_scalar("SELECT target FROM field_link WHERE `key` = 'labels'")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(linked, vec![target_badge]);
    jojobot::wiring::set_link_rule_from(Some("log-only")).unwrap();
    assert_eq!(link_rule(), LinkRule::LogOnly);
    let rollback = logs::boot(&memory).await;
    assert!(rollback.contains("mode=\"log-only\""), "{rollback}");
    assert!(
        rollback
            .lines()
            .any(|line| line.contains("r1_owned_now=") && line.contains("links_added=")),
        "{rollback}"
    );
    pool.close().await;
    store.stop().await;
}
