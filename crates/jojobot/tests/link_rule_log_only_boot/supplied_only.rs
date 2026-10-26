use super::*;
use jojobot_adapters::provisioned::overlay_contract::shipped_record;
use jojobot_domain::memory::owned::{Provision, Provisions};

#[tokio::test]
async fn supplied_only_boot_diagnostics_count_current_and_historical_subjects_without_names() {
    jojobot::wiring::set_link_rule_from(Some("log-only")).unwrap();
    let scratch = Scratch::new("link-rule-supplied-diagnostics");
    let mut store = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&store, "supplieddiagnostics")
        .await
        .unwrap();
    migrate::run(&pool).await.unwrap();
    let memory_store = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory_store)
        .await
        .unwrap();
    let target = EntityId("work:contract-links-alpha".into());
    let stored = EntityId("view:board".into());
    for (id, name) in [(&target, "Alpha"), (&stored, "Board")] {
        memory_store
            .add_entity(NewEntity::new(id.clone(), name, "contract-fixture"))
            .await
            .unwrap()
            .written()
            .unwrap();
    }
    let target_badge = badge_of(&pool, target.as_str()).await;
    let marked = jojobot_domain::memory::marked_stored_handle(&target_badge);
    let supplied = Provisions::new(
        [
            "marked-things",
            "plain-listing",
            "matches-nothing",
            "board",
            "marked-by-default",
            "marked-by-asking",
        ]
        .map(shipped_record)
        .into(),
    );
    let mut provisions: Vec<_> = supplied
        .records()
        .map(|(entity, fields)| {
            let mut fields = fields.clone();
            if entity.id.as_str() == "view:marked-by-default" {
                fields.insert("alpha".into(), marked.clone());
            }
            Provision::record(entity.clone(), fields)
        })
        .collect();
    provisions.push(Provision::field(
        EntityId("view:marked-by-asking".into()),
        "alpha",
        marked.clone(),
    ));
    let memory_store = memory_store.knowing(Provisions::new(provisions));
    for (subject, fields) in [
        (
            "view:marked-things",
            vec![
                ("alpha", target.to_string()),
                ("beta", format!("rhythm, {target}")),
            ],
        ),
        ("view:plain-listing", vec![("alpha", target.to_string())]),
        ("view:plain-listing", vec![("alpha", "rhythm".into())]),
        ("view:matches-nothing", vec![("alpha", "rhythm".into())]),
        ("view:board", vec![("alpha", target.to_string())]),
    ] {
        memory_store
            .capture(memory::state(NewFact {
                fields: fields
                    .into_iter()
                    .map(|(key, value)| (key.to_string(), value))
                    .collect(),
                ..NewFact::about(
                    EntityId(subject.into()),
                    "the held fields",
                    date(2026, 8, 1),
                )
            }))
            .await
            .unwrap()
            .written()
            .unwrap();
    }
    // A prior build wrote the stored marks; callers cannot submit their syntax.
    sqlx::query("UPDATE field_write SET value = ? WHERE `key` = 'alpha' AND value = ?")
        .bind(&marked)
        .bind(target.as_str())
        .execute(&pool)
        .await
        .unwrap();
    memory_store.migrate_field_links().await.unwrap();
    for handle in [
        "view:marked-things",
        "view:plain-listing",
        "view:matches-nothing",
        "view:marked-by-default",
        "view:marked-by-asking",
    ] {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entity WHERE id = ?")
            .bind(handle)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "the supplied-only fixture stays rowless");
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
        let aggregate = log
            .lines()
            .find(|line| line.contains("supplied_only_current="))
            .unwrap_or_else(|| panic!("the supplied-only counts are missing: {log}"));
        assert!(aggregate.contains("supplied_only_current=3"), "{aggregate}");
        assert!(
            aggregate.contains("supplied_only_historical=2"),
            "{aggregate}"
        );
        for private in [
            "view:marked-things",
            "view:plain-listing",
            "view:matches-nothing",
            "view:board",
            "view:marked-by-default",
            "view:marked-by-asking",
            target.as_str(),
            target_badge.as_str(),
            marked.as_str(),
            "rhythm",
        ] {
            assert!(
                !log.contains(private),
                "the boot log names private data: {log}"
            );
        }
        assert_eq!(dump(&pool).await, before, "diagnostics only read");
    }
    store.stop().await;
}
