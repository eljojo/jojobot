use super::*;
use jojobot_adapters::provisioned::overlay_contract::shipped_record;
use jojobot_domain::memory::owned::Provisions;

#[tokio::test]
async fn r1_boot_diagnostics_partition_items_and_count_unaddressable_holders_without_ids() {
    jojobot::wiring::set_link_rule_from(Some("log-only")).unwrap();
    let scratch = Scratch::new("link-rule-orphan-diagnostics");
    let mut store = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&store, "orphandiagnostics")
        .await
        .unwrap();
    migrate::run(&pool).await.unwrap();
    let memory_store =
        DoltMemory::open(pool.clone()).knowing(Provisions::new(vec![shipped_record("loops")]));
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
    let keys = ["alpha", "beta", "gamma", "delta", "epsilon"];
    memory_store
        .capture(memory::state(NewFact {
            fields: keys.map(|key| (key.to_string(), target.to_string())).into(),
            ..NewFact::about(holder.clone(), "the old references", date(2026, 8, 1))
        }))
        .await
        .unwrap()
        .written()
        .unwrap();
    memory_store.migrate_field_links().await.unwrap();
    let holder_badge = badge_of(&pool, holder.as_str()).await;
    for key in keys {
        set_raw(&pool, &holder_badge, key, target.as_str()).await;
    }
    sqlx::query("DELETE FROM field_link WHERE entity = ?")
        .bind(&holder_badge)
        .execute(&pool)
        .await
        .unwrap();
    // The gate injects gwd5wp/f2 without an entity row. Two keys share its home.
    for (home, key) in [
        ("gwd5wp", "zeta"),
        ("gwd5wp", "eta"),
        ("view:loops", "theta"),
    ] {
        sqlx::query(
            "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id)
             VALUES (?, ?, 1, ?, 'f2')",
        )
        .bind(home)
        .bind(key)
        .bind(target.as_str())
        .execute(&pool)
        .await
        .unwrap();
    }
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
        for field in [
            "r1_owned_now=8",
            "r1_addressable=5",
            "r1_unaddressable=3",
            "r1_unaddressable_holders=2",
        ] {
            assert!(log.split_whitespace().any(|word| word == field), "{log}");
        }
        let locations: Vec<_> = log
            .lines()
            .filter(|line| line.contains("diagnostic=\"link_rule_r1_owned_now\""))
            .collect();
        assert_eq!(locations.len(), 5, "{log}");
        for line in locations {
            assert!(line.contains(&format!("entity={holder}")), "{line}");
        }
        for private in [
            "gwd5wp",
            "view:loops",
            target.as_str(),
            holder_badge.as_str(),
        ] {
            assert!(!log.contains(private), "private data leaked: {log}");
        }
        assert!(
            !log.contains("WARN"),
            "the diagnostic does not degrade boot: {log}"
        );
        assert_eq!(dump(&pool).await, before, "diagnostics only read");
    }
    store.stop().await;
}
