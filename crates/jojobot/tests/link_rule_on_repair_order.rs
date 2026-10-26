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

#[path = "link_rule_on_repair_order/logs.rs"]
mod logs;

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

/// A store holding what the old build stored for a bare line name: the bot's name
/// where its key stood, with a link row to the name.
struct Planted {
    _scratch: Scratch,
    server: Dolt,
    pool: sqlx::MySqlPool,
    memory: DoltMemory,
    work_key: String,
    bot_key: String,
}

async fn planted(label: &str) -> Planted {
    set_link_rule(LinkRule::On);
    let scratch = Scratch::new(label);
    let server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, label)
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
    Planted {
        _scratch: scratch,
        server,
        pool,
        memory,
        work_key,
        bot_key,
    }
}

async fn line_of(planted: &Planted) -> (String, Vec<String>) {
    let value: String =
        sqlx::query_scalar("SELECT value FROM field_write WHERE entity = ? AND `key` = 'line'")
            .bind(&planted.work_key)
            .fetch_one(&planted.pool)
            .await
            .expect("the value");
    let targets: Vec<String> =
        sqlx::query_scalar("SELECT target FROM field_link WHERE entity = ? AND `key` = 'line'")
            .bind(&planted.work_key)
            .fetch_all(&planted.pool)
            .await
            .expect("the rows");
    (value, targets)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_bare_line_name_is_repaired_and_then_marked_when_the_rule_is_on() {
    let mut planted = planted("link-rule-on-repair-order").await;
    let log = logs::boot(&planted.memory).await;
    assert!(log.contains("pass=\"bare line values\""), "{log}");
    assert!(log.contains("rows=1"), "{log}");
    let (value, targets) = line_of(&planted).await;
    assert_eq!(
        value,
        format!("@#{}", planted.bot_key),
        "repaired onto the bot, then marked"
    );
    assert_eq!(
        targets,
        vec![planted.bot_key.clone()],
        "and linked to the bot"
    );
    let second = logs::boot(&planted.memory).await;
    assert!(second.contains("pass=\"bare line values\""), "{second}");
    assert!(!second.contains("rows=1"), "the repair ran again: {second}");
    assert!(
        second.contains("store: field value links marked: already done"),
        "{second}"
    );
    assert!(
        !second.contains("store: field value lowering: already done"),
        "{second}"
    );
    planted.server.stop().await;
}

/// **A repair that cannot run leaves the marking for the next boot.** The marking
/// would delete the bare-name row the repair still owes, so the case makes the
/// repair's write fail and asks that the row, the value and the pass marker are
/// as they were.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_repair_that_fails_leaves_the_marking_for_the_next_boot() {
    let mut planted = planted("link-rule-on-repair-fails").await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "ALTER TABLE field_write ADD CONSTRAINT no_repair CHECK (`value` <> '{}')",
        planted.bot_key
    )))
    .execute(&planted.pool)
    .await
    .expect("the repair's write is made to fail");
    let log = logs::boot(&planted.memory).await;
    assert!(log.contains("BARE LINE VALUES NOT REPAIRED"), "{log}");
    let (value, targets) = line_of(&planted).await;
    assert_eq!(value, "beta", "the value is as it was");
    assert_eq!(targets, vec!["beta".to_string()], "and so is its row");
    let marked: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM store_pass WHERE name = 'field value links marked'",
    )
    .fetch_one(&planted.pool)
    .await
    .expect("markers");
    assert_eq!(marked, 0, "and the marking is not finished");
    planted.server.stop().await;
}

#[tokio::test]
async fn historical_r1_boots_clean_but_the_same_value_held_still_warns() {
    use jojobot_domain::memory::{FactPatch, FactStatus};

    for state in ["held", "superseded", "archived"] {
        let mut planted = planted(&format!("link-rule-r1-{state}")).await;
        planted.memory.migrate_field_links().await.unwrap();
        let holder = EntityId("work:alpha-queue".into());
        let mut old = NewFact::about(holder.clone(), "the old label", date(2026, 9, 1));
        old.fields.insert("label".into(), "plain words".into());
        let fact = planted
            .memory
            .capture(old)
            .await
            .unwrap()
            .written()
            .unwrap();
        sqlx::query(
            "UPDATE field_write SET value = 'bot:beta' WHERE entity = ? AND `key` = 'label'",
        )
        .bind(&planted.work_key)
        .execute(&planted.pool)
        .await
        .unwrap();
        if state == "superseded" {
            let mut newer = NewFact::about(holder.clone(), "the new label", date(2026, 9, 2));
            newer.fields.insert("label".into(), "plain words".into());
            planted
                .memory
                .capture(newer)
                .await
                .unwrap()
                .written()
                .unwrap();
        } else if state == "archived" {
            planted
                .memory
                .update_fact(
                    &fact.address(),
                    FactPatch {
                        status: Some(FactStatus::Archived),
                        ..Default::default()
                    },
                    &holder,
                )
                .await
                .unwrap()
                .written()
                .unwrap();
        }
        let log = logs::boot(&planted.memory).await;
        let held = usize::from(state == "held");
        let historical = 1 - held;
        let summary = log
            .lines()
            .find(|line| line.contains("r1_historical="))
            .unwrap();
        for count in [
            format!("r1_owned_now={held}"),
            format!("r1_historical={historical}"),
        ] {
            assert!(
                summary.split_whitespace().any(|field| field == count),
                "{state}: {log}"
            );
        }
        assert!(
            summary.split_whitespace().any(|field| field == "INFO"),
            "{log}"
        );
        let warnings: Vec<_> = log
            .lines()
            .filter(|line| line.contains("FIELD VALUE LEFT AS TEXT"))
            .collect();
        assert_eq!(warnings.len(), held, "{state}: {log}");
        if held == 1 {
            assert!(
                warnings[0].split_whitespace().any(|field| field == "WARN"),
                "{log}"
            );
        } else {
            assert!(
                !log.contains("WARN") && !log.contains("ERROR"),
                "{state}: {log}"
            );
            assert!(
                !log.contains("bot:beta"),
                "historical values are count-only: {log}"
            );
        }
        let raw: String = sqlx::query_scalar("SELECT value FROM field_write WHERE entity = ? AND `key` = 'label' ORDER BY ordinal LIMIT 1")
            .bind(&planted.work_key).fetch_one(&planted.pool).await.unwrap();
        assert_eq!(raw, "bot:beta", "R1 is never repaired");
        planted.pool.close().await;
        planted.server.stop().await;
    }
}
