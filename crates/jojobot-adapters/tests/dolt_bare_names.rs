//! **A bare name under a reference key is refused at the write**, on the real
//! store: no link row is written for it, on a capture or on an edit.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::EntityId;
use jojobot_domain::memory::Memory;
use jojobot_domain::memory::types::{DeclaredType, Field};
use jojobot_domain::memory::{FactPatch, MemoryError, NewEntity, NewFact};

async fn booted(pool: &sqlx::MySqlPool) {
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
}

async fn badge_of(pool: &sqlx::MySqlPool, handle: &str) -> String {
    sqlx::query_scalar::<_, Option<String>>("SELECT badge FROM entity WHERE id = ?")
        .bind(handle)
        .fetch_one(pool)
        .await
        .expect("the entity row is readable")
        .expect("a created entity is given a badge")
}

async fn link_rows(pool: &sqlx::MySqlPool) -> Vec<(String, String, i64, String)> {
    sqlx::query_as("SELECT entity, `key`, ordinal, target FROM field_link ORDER BY 1, 2, 3, 4")
        .fetch_all(pool)
        .await
        .expect("the link table reads")
}

#[tokio::test]
async fn a_bare_line_name_is_refused_without_a_dangling_link() {
    let scratch = Scratch::new("bare-line-name");
    let mut store = Dolt::start(&scratch.0, free_port()).await.expect("store");
    let pool = migrated_database(&store, "bare_line_name")
        .await
        .expect("database");
    migrate::run(&pool).await.expect("schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool.clone());
    memory
        .declare_type(
            DeclaredType::new(
                "dispatch",
                vec![Field::pointing_at(
                    "line",
                    jojobot_domain::memory::EntityKind::BOT,
                )],
            ),
            None,
        )
        .await
        .expect("declare line");
    let work = EntityId("work:alpha-queue".into());
    let bot = EntityId("bot:gamma".into());
    for (id, name) in [(&work, "Alpha"), (&bot, "Gamma")] {
        memory
            .add_entity(NewEntity::new(id.clone(), name, "fixture"))
            .await
            .expect("add")
            .written()
            .expect("created");
    }
    let line = |value: &str| {
        let mut fact = NewFact::about(work.clone(), "assigned a line", date(2026, 9, 1));
        fact.fields.insert("line".into(), value.into());
        fact.sets.insert("line".into());
        fact
    };
    let result = memory.capture(line("dev7")).await;
    eprintln!(
        "bare capture: {result:?}; link rows: {:?}",
        link_rows(&pool).await
    );
    let refused = result.expect_err("a bare name needs a full handle");
    assert!(
        matches!(&refused, MemoryError::InvalidFact(_)),
        "{refused:?}"
    );
    assert!(
        refused.to_string().contains(&format!(
            "{}:dev7",
            jojobot_domain::memory::EntityKind::BOT.as_token()
        )),
        "{refused}"
    );
    assert!(refused.to_string().contains("line"), "{refused}");
    assert!(memory.recall(&work).await.expect("records").is_empty());
    memory
        .capture(line(bot.as_str()))
        .await
        .expect("capture")
        .written()
        .expect("the handle lands");
    let badge = badge_of(&pool, work.as_str()).await;
    let bot_badge = badge_of(&pool, bot.as_str()).await;
    let values: Vec<String> = sqlx::query_scalar(
        "SELECT value FROM field_write WHERE entity = ? AND `key` = 'line' ORDER BY ordinal",
    )
    .bind(&badge)
    .fetch_all(&pool)
    .await
    .expect("writes");
    assert_eq!(values, vec![format!("@#{bot_badge}")]);
    assert_eq!(
        link_rows(&pool).await,
        vec![(badge, "line".into(), 1, bot_badge)]
    );
    store.stop().await;
}

#[tokio::test]
async fn a_bare_line_name_is_refused_on_update_without_a_dangling_link() {
    let scratch = Scratch::new("bare-line-update");
    let mut store = Dolt::start(&scratch.0, free_port()).await.expect("store");
    let pool = migrated_database(&store, "bare_line_update")
        .await
        .expect("database");
    migrate::run(&pool).await.expect("schema");
    booted(&pool).await;
    let memory = DoltMemory::open(pool.clone());
    memory
        .declare_type(
            DeclaredType::new(
                "dispatch",
                vec![Field::pointing_at(
                    "line",
                    jojobot_domain::memory::EntityKind::BOT,
                )],
            ),
            None,
        )
        .await
        .expect("declare line");
    let work = EntityId("work:alpha-queue".into());
    let first = EntityId("bot:gamma".into());
    let next = EntityId("bot:beta".into());
    for (id, name) in [(&work, "Alpha"), (&first, "Gamma"), (&next, "Beta")] {
        memory
            .add_entity(NewEntity::new(id.clone(), name, "fixture"))
            .await
            .expect("add")
            .written()
            .expect("created");
    }
    let mut fact = NewFact::about(work.clone(), "assigned a line", date(2026, 9, 1));
    fact.fields.insert("line".into(), first.to_string());
    fact.sets.insert("line".into());
    let captured = memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("the initial handle lands");
    let before = memory.recall(&work).await.expect("records");
    let links_before = link_rows(&pool).await;
    let patch = |value: &str| FactPatch {
        fields: [("line".into(), value.into())].into_iter().collect(),
        sets: ["line".into()].into_iter().collect(),
        ..Default::default()
    };
    let result = memory
        .update_fact(
            &captured.address(),
            patch("dev7"),
            &EntityId("bot:sigma".into()),
        )
        .await;
    eprintln!(
        "bare update: {result:?}; link rows: {:?}",
        link_rows(&pool).await
    );
    let refused = result.expect_err("an edit needs a full handle too");
    assert!(
        matches!(&refused, MemoryError::InvalidFact(_)),
        "{refused:?}"
    );
    assert!(refused.to_string().contains("line"), "{refused}");
    assert!(
        refused.to_string().contains(&format!(
            "{}:dev7",
            jojobot_domain::memory::EntityKind::BOT.as_token()
        )),
        "{refused}"
    );
    assert_eq!(memory.recall(&work).await.expect("records"), before);
    assert_eq!(link_rows(&pool).await, links_before);
    let edited = memory
        .update_fact(
            &captured.address(),
            patch(next.as_str()),
            &EntityId("bot:sigma".into()),
        )
        .await
        .expect("edit")
        .written()
        .expect("the corrected handle lands");
    assert_eq!(edited.fields["line"], next.as_str());
    assert_eq!(
        memory.fields(&work).await.expect("fields")["line"],
        next.as_str()
    );
    let badge = badge_of(&pool, work.as_str()).await;
    let first_badge = badge_of(&pool, first.as_str()).await;
    let next_badge = badge_of(&pool, next.as_str()).await;
    let values: Vec<String> = sqlx::query_scalar(
        "SELECT value FROM field_write WHERE entity = ? AND `key` = 'line' ORDER BY ordinal",
    )
    .bind(&badge)
    .fetch_all(&pool)
    .await
    .expect("writes");
    assert_eq!(
        values,
        vec![format!("@#{first_badge}"), format!("@#{next_badge}")]
    );
    assert_eq!(
        link_rows(&pool).await,
        vec![
            (badge.clone(), "line".into(), 1, first_badge),
            (badge, "line".into(), 2, next_badge)
        ]
    );
    store.stop().await;
}
