use super::*;

#[tokio::test]
async fn r1_boot_diagnostics_name_each_holder_and_key_without_logging_values() {
    assert_locations(5, 5).await;
}

#[tokio::test]
async fn r1_boot_diagnostics_cap_locations_without_losing_the_item_count() {
    assert_locations(51, 50).await;
}

async fn assert_locations(items: usize, shown: usize) {
    jojobot::wiring::set_link_rule_from(Some("log-only")).unwrap();
    let scratch = Scratch::new("link-rule-r1-diagnostics");
    let mut store = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&store, "r1diagnostics").await.unwrap();
    migrate::run(&pool).await.unwrap();
    let memory_store = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory_store)
        .await
        .unwrap();
    let holder = EntityId("thing:contract-links-holder".into());
    let target = EntityId("work:contract-links-alpha".into());
    for (id, label) in [(&holder, "Holder"), (&target, "Alpha")] {
        memory_store
            .add_entity(NewEntity::new(id.clone(), label, "contract-fixture"))
            .await
            .unwrap()
            .written()
            .unwrap();
    }
    let keys: Vec<String> = (0..items).map(|n| format!("reference_{n}")).collect();
    memory_store
        .capture(memory::state(NewFact {
            fields: keys
                .iter()
                .map(|key| (key.clone(), target.to_string()))
                .collect(),
            ..NewFact::about(holder.clone(), "the old references", date(2026, 8, 1))
        }))
        .await
        .unwrap()
        .written()
        .unwrap();
    memory_store.migrate_field_links().await.unwrap();
    let holder_badge = badge_of(&pool, holder.as_str()).await;
    for key in &keys {
        set_raw(&pool, &holder_badge, key, target.as_str()).await;
    }
    sqlx::query("DELETE FROM field_link WHERE entity = ?")
        .bind(&holder_badge)
        .execute(&pool)
        .await
        .unwrap();
    let before = dump(&pool).await;

    for _ in 0..2 {
        let kept = Kept::default();
        let guard = tracing::subscriber::set_default(
            tracing_subscriber::fmt()
                .with_writer(kept.clone())
                .with_ansi(false)
                .with_max_level(tracing::Level::INFO)
                .finish(),
        );
        jojobot::wiring::field_links::boot_field_links(&memory_store).await;
        drop(guard);
        let log = String::from_utf8_lossy(&kept.0.lock().unwrap()).into_owned();
        assert!(log.contains(&format!("r1_owned_now={items}")), "{log}");
        assert!(log.contains(&format!("r1_addressable={items}")), "{log}");
        let locations: Vec<_> = log
            .lines()
            .filter(|line| line.contains("diagnostic=\"link_rule_r1_owned_now\""))
            .collect();
        assert_eq!(locations.len(), shown, "{log}");
        assert!(
            log.contains(&format!("r1_locations_left_out={}", items - shown)),
            "{log}"
        );
        let found: std::collections::BTreeSet<_> = locations
            .iter()
            .map(|line| {
                assert!(line.split_whitespace().any(|word| word == "INFO"), "{line}");
                assert!(line.contains(&format!("entity={holder}")), "{line}");
                assert!(!line.contains("value="), "{line}");
                line.split_whitespace()
                    .find_map(|field| field.strip_prefix("key="))
                    .unwrap()
            })
            .collect();
        assert_eq!(found.len(), shown);
        assert!(found.is_subset(&keys.iter().map(String::as_str).collect()));
        assert!(
            !log.contains(target.as_str()),
            "a field value leaked: {log}"
        );
        assert!(!log.contains(&holder_badge), "a stored id leaked: {log}");
        assert_eq!(dump(&pool).await, before, "diagnostics only read");
    }
    store.stop().await;
}
