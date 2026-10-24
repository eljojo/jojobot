//! **The startup repairs of stored rows, over the real store.** Each case makes
//! the damage the way a build that is gone left it, by editing the rows under a
//! store written through the real verbs, and then shows the repair put it right
//! once, leave what it cannot place, and change nothing on a second boot.

use jiff::civil::date;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::dolt::storage_check::DoltStorageCheck;
use jojobot_adapters::dolt::{Dolt, Migrated};
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::clock::Clock;
use jojobot_domain::memory::types::{DeclaredType, Field};
use jojobot_domain::memory::{EntityId, EntityKind, Memory, NewEntity, NewFact};
use jojobot_domain::storage_check::{StorageCheck, StorageClass};

async fn a_store(label: &str) -> (Scratch, Dolt, sqlx::MySqlPool, DoltMemory) {
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
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    (scratch, server, pool, memory)
}

async fn thing(memory: &DoltMemory, handle: &str, name: &str) -> EntityId {
    let id = EntityId(handle.to_string());
    memory
        .add_entity(NewEntity::new(id.clone(), name, "fixture"))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    id
}

async fn run(pool: &sqlx::MySqlPool, sql: &str, bind: &[&str]) {
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.to_string()));
    for value in bind {
        query = query.bind(*value);
    }
    query
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"));
}

async fn found(pool: &sqlx::MySqlPool, class: StorageClass) -> (u64, Vec<String>) {
    let finding = DoltStorageCheck::open(pool.clone(), Clock::default())
        .check_storage()
        .await
        .expect("the check reads")
        .into_iter()
        .find(|finding| finding.class == class)
        .expect("every class is answered");
    (finding.count, finding.ids)
}

fn outcome(passes: &[(&'static str, Migrated)], name: &str) -> String {
    let (_, outcome) = passes
        .iter()
        .find(|(pass, _)| *pass == name)
        .unwrap_or_else(|| panic!("no pass named {name}: {passes:?}"));
    format!("{outcome:?}")
}

async fn badge_of(pool: &sqlx::MySqlPool, handle: &str) -> String {
    sqlx::query_scalar("SELECT COALESCE(badge, id) FROM entity WHERE id = ?")
        .bind(handle)
        .fetch_one(pool)
        .await
        .expect("the thing has a key")
}

/// **A bare line name the old build stored as a link** is pointed at the bot it
/// means: the value is rewritten to the bot's permanent id, the link row follows,
/// and the check reports no dangling reference for it. A name no bot wears is left
/// and still named by the check. The second boot changes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_bare_line_name_stored_as_a_link_is_pointed_at_its_bot_once() {
    let (_scratch, _server, pool, memory) = a_store("repair-bare-line").await;
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
    let bot = thing(&memory, "bot:beta", "Beta").await;
    let other = thing(&memory, "bot:gamma", "Gamma").await;
    let mut homes = Vec::new();
    // The first thing is given its line twice, so two writes hold the name.
    for (handle, name, line, times) in [
        ("work:alpha-queue", "Alpha queue", &bot, 2),
        ("work:beta-queue", "Beta queue", &other, 1),
    ] {
        let work = thing(&memory, handle, name).await;
        for _ in 0..times {
            let mut fact = NewFact::about(work.clone(), "assigned a line", date(2026, 9, 1));
            fact.fields.insert("line".into(), line.as_str().into());
            fact.sets.insert("line".into());
            memory
                .capture(fact)
                .await
                .expect("capture")
                .written()
                .expect("the handle lands");
        }
        homes.push(work);
    }
    // What the old build stored: the bot's name where its key stood, and a link
    // row to the name. One name has a bot, one has none.
    let bot_key = badge_of(&pool, bot.as_str()).await;
    let other_key = badge_of(&pool, other.as_str()).await;
    for (home, was, bare) in [
        (&homes[0], &bot_key, "beta"),
        (&homes[1], &other_key, "zeta"),
    ] {
        let home_key = badge_of(&pool, home.as_str()).await;
        run(
            &pool,
            "UPDATE field_write SET value = ? WHERE entity = ? AND `key` = 'line'",
            &[bare, &home_key],
        )
        .await;
        run(
            &pool,
            "UPDATE field_link SET target = ? WHERE entity = ? AND `key` = 'line' AND target = ?",
            &[bare, &home_key, was],
        )
        .await;
    }
    let (rows, ids) = found(&pool, StorageClass::DanglingReference).await;
    assert_eq!(rows, 3, "{ids:?}");
    assert_eq!(
        ids,
        vec![
            "field_link.target: beta".to_string(),
            "field_link.target: zeta".to_string()
        ]
    );

    let first = memory.repair_stored_rows().await;
    assert_eq!(
        outcome(&first, "bare reference values"),
        "Ran(2)",
        "{first:?}"
    );

    let (rows, ids) = found(&pool, StorageClass::DanglingReference).await;
    assert_eq!(
        (rows, ids),
        (1, vec!["field_link.target: zeta".to_string()]),
        "the name no bot wears is left, and the check still names it"
    );
    for home in [&homes[0]] {
        let home_key = badge_of(&pool, home.as_str()).await;
        let values: Vec<String> = sqlx::query_scalar(
            "SELECT value FROM field_write WHERE entity = ? AND `key` = 'line' ORDER BY ordinal",
        )
        .bind(&home_key)
        .fetch_all(&pool)
        .await
        .expect("values");
        assert_eq!(values, vec![bot_key.clone(); 2], "{home}");
        let targets: Vec<String> =
            sqlx::query_scalar("SELECT target FROM field_link WHERE entity = ? AND `key` = 'line'")
                .bind(&home_key)
                .fetch_all(&pool)
                .await
                .expect("links");
        assert_eq!(targets, vec![bot_key.clone(); 2], "{home}");
    }
    let served = memory.fields(&homes[0]).await.expect("fields");
    assert_eq!(served["line"], bot.as_str(), "the read serves the handle");

    let before: Vec<(String, String)> =
        sqlx::query_as("SELECT entity, value FROM field_write ORDER BY entity, `key`, ordinal")
            .fetch_all(&pool)
            .await
            .expect("writes");
    let second = memory.repair_stored_rows().await;
    assert_eq!(
        outcome(&second, "bare reference values"),
        "Settled",
        "{second:?}"
    );
    let after: Vec<(String, String)> =
        sqlx::query_as("SELECT entity, value FROM field_write ORDER BY entity, `key`, ordinal")
            .fetch_all(&pool)
            .await
            .expect("writes");
    assert_eq!(before, after, "a second boot changes nothing");
}

/// **A merge made before merge events existed** gets its event from the survivor
/// its row records. The moment is the thing's first write that carries the merge;
/// a thing with no such write gets the moment of the repair and the session
/// `backfilled`. A merge whose survivor has no row is left. The check stops
/// calling the class tolerated, and the second boot writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_merge_made_before_merge_events_existed_gets_its_event_once() {
    let (_scratch, _server, pool, memory) = a_store("repair-old-merge").await;
    let mut folded_keys = Vec::new();
    for (kept, kept_name, gone, gone_name) in [
        ("person:krusty", "Krusty", "person:skinner", "Skinner"),
        ("person:homer", "Homer", "person:apu", "Apu"),
        ("person:otto", "Otto", "person:marge", "Marge"),
    ] {
        let kept = thing(&memory, kept, kept_name).await;
        let gone = thing(&memory, gone, gone_name).await;
        memory
            .merge(
                &gone,
                &kept,
                Some("filed twice"),
                date(2026, 9, 2),
                &EntityId("bot:sigma".into()),
                Some("run-a"),
            )
            .await
            .expect("the merge lands");
        folded_keys.push(badge_of(&pool, gone.as_str()).await);
    }
    // The merges as the old build left them: no event at all.
    run(&pool, "DELETE FROM merge_event", &[]).await;
    // The second one has no write of its own that carries the merge.
    run(
        &pool,
        "UPDATE entity_write SET merged_into = NULL WHERE entity IN ('person:apu', ?)",
        &[&folded_keys[1]],
    )
    .await;
    // The third one's survivor has no row.
    run(&pool, "DELETE FROM entity WHERE id = 'person:otto'", &[]).await;
    let (count, ids) = found(&pool, StorageClass::MergedWithoutEvent).await;
    assert_eq!(count, 3, "{ids:?}");

    let first = memory.repair_stored_rows().await;
    assert_eq!(outcome(&first, "old merge events"), "Ran(2)", "{first:?}");

    let events: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT folded, survivor, session FROM merge_event WHERE ordinal = 0 ORDER BY merge_no",
    )
    .fetch_all(&pool)
    .await
    .expect("events");
    let survivor_of = |handle: &str| {
        let handle = handle.to_string();
        let pool = pool.clone();
        async move { badge_of(&pool, &handle).await }
    };
    assert_eq!(
        events,
        vec![
            (
                folded_keys[1].clone(),
                survivor_of("person:homer").await,
                Some("backfilled".to_string())
            ),
            (
                folded_keys[0].clone(),
                survivor_of("person:krusty").await,
                None
            ),
        ],
        "the merge with a write of its own keeps that moment, the other is marked"
    );
    let served = memory.merge_events(None).await.expect("events read");
    assert_eq!(served.len(), 2, "{served:?}");
    assert_eq!(
        (served[0].folded.as_str(), served[0].survivor.as_str()),
        ("person:apu", "person:homer")
    );
    let (count, ids) = found(&pool, StorageClass::MergedWithoutEvent).await;
    assert_eq!(
        (count, ids),
        (1, vec!["person:marge".to_string()]),
        "the merge with no survivor is left and still named"
    );
    assert!(
        !StorageClass::MergedWithoutEvent.reported_at_info(),
        "the class is damage now"
    );

    let second = memory.repair_stored_rows().await;
    assert_eq!(
        outcome(&second, "old merge events"),
        "Settled",
        "{second:?}"
    );
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM merge_event")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(rows, 2, "a second boot writes no event");
}
