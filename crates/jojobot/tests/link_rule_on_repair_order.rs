//! **With the link rule on, the boot repairs a bare line name before it marks.**
//! The repair of a bare name selects the link rows that name sits in; the marked
//! reconcile removes a row no mark implies. Marking first would leave the name
//! unrepaired, so the boot step runs the repairs first and marks the id they leave.
//!
//! A test binary of its own: the switch is process-wide.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::types::{DeclaredType, Field};
use jojobot_domain::memory::{
    EntityId, EntityKind, LinkRule, Memory, NewEntity, NewFact, set_link_rule,
};

async fn badge_of(pool: &sqlx::MySqlPool, handle: &str) -> String {
    sqlx::query_scalar::<_, Option<String>>("SELECT badge FROM entity WHERE id = ?")
        .bind(handle)
        .fetch_one(pool)
        .await
        .expect("the entity row is readable")
        .expect("a created entity is given a badge")
}

async fn add(memory: &DoltMemory, handle: &str, name: &str) -> EntityId {
    let id = EntityId(handle.to_string());
    memory
        .add_entity(NewEntity::new(id.clone(), name, "fixture"))
        .await
        .expect("add")
        .written()
        .expect("nothing collides");
    id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_bare_line_name_is_repaired_and_then_marked_when_the_rule_is_on() {
    set_link_rule(LinkRule::On);
    let scratch = Scratch::new("link-rule-on-repair-order");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "link_rule_on_repair_order")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    memory
        .declare_type(
            DeclaredType::new(
                "dispatch",
                vec![Field::pointing_at("line", EntityKind::BOT)],
            ),
            None,
        )
        .await
        .expect("declare line");
    let bot = add(&memory, "bot:beta", "Beta").await;
    let work = add(&memory, "work:alpha-queue", "Alpha queue").await;
    let mut fact = NewFact::about(work.clone(), "assigned a line", date(2026, 9, 1));
    fact.fields.insert("line".into(), bot.as_str().into());
    fact.sets.insert("line".into());
    memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("the handle lands");
    let (bot_key, work_key) = (
        badge_of(&pool, bot.as_str()).await,
        badge_of(&pool, work.as_str()).await,
    );
    // What the old build stored: the bot's name where its key stood, with a link
    // row to the name.
    sqlx::query("UPDATE field_write SET value = 'beta' WHERE entity = ? AND `key` = 'line'")
        .bind(&work_key)
        .execute(&pool)
        .await
        .expect("the old value");
    sqlx::query("DELETE FROM field_link WHERE entity = ? AND `key` = 'line'")
        .bind(&work_key)
        .execute(&pool)
        .await
        .expect("the stale row");
    sqlx::query(
        "INSERT INTO field_link (entity, `key`, ordinal, target) VALUES (?, 'line', 1, 'beta')",
    )
    .bind(&work_key)
    .execute(&pool)
    .await
    .expect("the old row");

    jojobot::wiring::field_links::boot_field_links(&memory).await;

    let value: String =
        sqlx::query_scalar("SELECT value FROM field_write WHERE entity = ? AND `key` = 'line'")
            .bind(&work_key)
            .fetch_one(&pool)
            .await
            .expect("the value");
    assert_eq!(
        value,
        format!("@#{bot_key}"),
        "repaired onto the bot, then marked"
    );
    let targets: Vec<String> =
        sqlx::query_scalar("SELECT target FROM field_link WHERE entity = ? AND `key` = 'line'")
            .bind(&work_key)
            .fetch_all(&pool)
            .await
            .expect("the rows");
    assert_eq!(targets, vec![bot_key], "and linked to the bot");
    server.stop().await;
}
