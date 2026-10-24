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
use jojobot_domain::memory::{EntityId, EntityKind, Guarded, Memory, NewEntity, NewFact};
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
    let new = NewEntity::new(id.clone(), name, "fixture");
    // Two things may wear one slug under two kinds; the screen asks the caller to
    // say they are two, and the case does.
    if let Guarded::Blocked {
        attempted,
        candidates,
    } = memory.add_entity(new.clone()).await.expect("add")
    {
        let token = jojobot_domain::memory::guard::override_token(&attempted, &candidates);
        memory
            .add_entity(NewEntity {
                override_token: Some(token),
                override_reason: Some("a bot and a person that wear one slug".into()),
                ..new
            })
            .await
            .expect("add")
            .written()
            .expect("told apart");
    }
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
    assert_eq!(outcome(&first, "bare line values"), "Ran(2)", "{first:?}");

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
        outcome(&second, "bare line values"),
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

/// **The warnings the repair logs when it leaves a row**, kept for the case that
/// reads them. The case runs on one thread, so a subscriber set for it sees
/// every event of the pass.
#[derive(Clone, Default)]
struct Said(std::sync::Arc<std::sync::Mutex<Vec<String>>>);

struct Words(String);

impl tracing::field::Visit for Words {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        use std::fmt::Write;
        let _ = write!(self.0, "{}={value:?} ", field.name());
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Said {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let mut words = Words(String::new());
        event.record(&mut words);
        self.0.lock().expect("the log is kept").push(words.0);
    }
}

fn listening() -> (Said, tracing::subscriber::DefaultGuard) {
    use tracing_subscriber::layer::SubscriberExt;
    let said = Said::default();
    let guard = tracing::subscriber::set_default(tracing_subscriber::registry().with(said.clone()));
    (said, guard)
}

/// A work item given a line through the verbs, then damaged the way the old build
/// left it: the stored value and its link row hold `bare` where the id stood.
async fn line_stored_as(
    memory: &DoltMemory,
    pool: &sqlx::MySqlPool,
    work: (&str, &str),
    line: &EntityId,
    bare: &str,
) -> EntityId {
    let home = thing(memory, work.0, work.1).await;
    let mut fact = NewFact::about(home.clone(), "assigned a line", date(2026, 9, 1));
    fact.fields.insert("line".into(), line.as_str().into());
    fact.sets.insert("line".into());
    memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("the handle lands");
    let home_key = badge_of(pool, home.as_str()).await;
    run(
        pool,
        "UPDATE field_write SET value = ? WHERE entity = ? AND `key` = 'line'",
        &[bare, &home_key],
    )
    .await;
    run(
        pool,
        "UPDATE field_link SET target = ? WHERE entity = ? AND `key` = 'line'",
        &[bare, &home_key],
    )
    .await;
    home
}

/// The stored values and the link rows, as the tables hold them.
type Rows = (
    Vec<(String, String, i64, Option<String>)>,
    Vec<(String, String, i64, String)>,
);

/// Every stored value and link row, as the tables hold them.
async fn rows_of_values_and_links(pool: &sqlx::MySqlPool) -> Rows {
    let writes =
        sqlx::query_as("SELECT entity, `key`, ordinal, value FROM field_write ORDER BY 1, 2, 3")
            .fetch_all(pool)
            .await
            .expect("writes");
    let links =
        sqlx::query_as("SELECT entity, `key`, ordinal, target FROM field_link ORDER BY 1, 2, 3, 4")
            .fetch_all(pool)
            .await
            .expect("links");
    (writes, links)
}

async fn declare_line(memory: &DoltMemory, type_name: &str, kind: EntityKind) {
    memory
        .declare_type(
            DeclaredType::new(type_name, vec![Field::pointing_at("line", kind)]),
            None,
        )
        .await
        .expect("declare line");
}

/// **A reference whose row was lost is not a bare name.** The link target is
/// shaped like an id this store draws and no row wears it, while a live bot wears
/// the same spelling as its handle. The repair leaves the stored value and the link
/// byte for byte, the check still names it, and a bare name beside it is still
/// pointed at its bot.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_lost_id_under_a_line_key_is_left_beside_a_bare_name_that_is_repaired() {
    let (_scratch, _server, pool, memory) = a_store("repair-lost-id").await;
    declare_line(&memory, "dispatch", EntityKind::BOT).await;
    // A bot whose handle spells a six-letter id, and one whose name is bare.
    let spelled = thing(&memory, "bot:defrag", "Defrag").await;
    let named = thing(&memory, "bot:beta", "Beta").await;
    let lost = line_stored_as(
        &memory,
        &pool,
        ("work:alpha-queue", "Alpha queue"),
        &spelled,
        "defrag",
    )
    .await;
    let bare = line_stored_as(
        &memory,
        &pool,
        ("work:beta-queue", "Beta queue"),
        &named,
        "beta",
    )
    .await;
    let before = rows_of_values_and_links(&pool).await;
    let (_, ids) = found(&pool, StorageClass::DanglingReference).await;
    assert_eq!(
        ids,
        vec![
            "field_link.target: beta".to_string(),
            "field_link.target: defrag".to_string()
        ]
    );

    let (said, _listening) = listening();
    let first = memory.repair_stored_rows().await;
    assert_eq!(outcome(&first, "bare line values"), "Ran(1)", "{first:?}");

    let (_, ids) = found(&pool, StorageClass::DanglingReference).await;
    assert_eq!(ids, vec!["field_link.target: defrag".to_string()]);
    let after = rows_of_values_and_links(&pool).await;
    let lost_key = badge_of(&pool, lost.as_str()).await;
    let held = |rows: &Rows| {
        (
            rows.0
                .iter()
                .filter(|row| row.0 == lost_key)
                .cloned()
                .collect::<Vec<_>>(),
            rows.1
                .iter()
                .filter(|row| row.0 == lost_key)
                .cloned()
                .collect::<Vec<_>>(),
        )
    };
    assert_eq!(held(&before), held(&after), "the lost id is untouched");
    assert_eq!(
        held(&after).1[0].3,
        "defrag",
        "its link still holds the spelling it had"
    );
    let bare_key = badge_of(&pool, bare.as_str()).await;
    let named_key = badge_of(&pool, named.as_str()).await;
    assert!(
        after
            .1
            .iter()
            .any(|row| row.0 == bare_key && row.3 == named_key),
        "the bare name now links to its bot: {:?}",
        after.1
    );
    let left = said.0.lock().expect("kept").join("\n");
    assert!(left.contains("defrag") && left.contains("LEFT"), "{left}");
}

/// **Two declarations of one key that point at different kinds leave the name
/// alone, whichever type sorts first.** Both candidates are live. Two
/// declarations that agree on the bot still repair it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn declarations_that_disagree_on_the_kind_leave_a_bare_line_name_alone() {
    for (label, bot_type, person_type) in [
        ("repair-kinds-bot-first", "aardvark-route", "zebra-route"),
        ("repair-kinds-person-first", "zebra-route", "aardvark-route"),
    ] {
        let (_scratch, _server, pool, memory) = a_store(label).await;
        declare_line(&memory, bot_type, EntityKind::BOT).await;
        declare_line(&memory, person_type, EntityKind::PERSON).await;
        let bot = thing(&memory, "bot:beta", "Beta").await;
        thing(&memory, "person:beta", "Beta").await;
        line_stored_as(
            &memory,
            &pool,
            ("work:alpha-queue", "Alpha queue"),
            &bot,
            "beta",
        )
        .await;
        let before = rows_of_values_and_links(&pool).await;

        let (said, _listening) = listening();
        let first = memory.repair_stored_rows().await;
        assert_eq!(
            outcome(&first, "bare line values"),
            "Ran(0)",
            "{label}: {first:?}"
        );
        assert_eq!(rows_of_values_and_links(&pool).await, before, "{label}");
        let (_, ids) = found(&pool, StorageClass::DanglingReference).await;
        assert_eq!(ids, vec!["field_link.target: beta".to_string()], "{label}");
        let left = said.0.lock().expect("kept").join("\n");
        assert!(
            left.contains("beta") && left.contains("LEFT"),
            "{label}: {left}"
        );
    }

    let (_scratch, _server, pool, memory) = a_store("repair-kinds-agree").await;
    declare_line(&memory, "aardvark-route", EntityKind::BOT).await;
    declare_line(&memory, "zebra-route", EntityKind::BOT).await;
    let bot = thing(&memory, "bot:beta", "Beta").await;
    line_stored_as(
        &memory,
        &pool,
        ("work:alpha-queue", "Alpha queue"),
        &bot,
        "beta",
    )
    .await;
    let first = memory.repair_stored_rows().await;
    assert_eq!(outcome(&first, "bare line values"), "Ran(1)", "{first:?}");
    let (count, _) = found(&pool, StorageClass::DanglingReference).await;
    assert_eq!(count, 0, "two declarations that agree still repair it");
}

/// **A name that also sits under another key is left**, because the function that
/// rewrites a link rewrites every link to its target.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_bare_line_name_that_also_sits_under_another_key_is_left() {
    let (_scratch, _server, pool, memory) = a_store("repair-other-key").await;
    declare_line(&memory, "dispatch", EntityKind::BOT).await;
    let bot = thing(&memory, "bot:beta", "Beta").await;
    let home = line_stored_as(
        &memory,
        &pool,
        ("work:alpha-queue", "Alpha queue"),
        &bot,
        "beta",
    )
    .await;
    let home_key = badge_of(&pool, home.as_str()).await;
    run(
        &pool,
        "INSERT INTO field_link (entity, `key`, ordinal, target) VALUES (?, 'partner', 1, 'beta')",
        &[&home_key],
    )
    .await;
    let before = rows_of_values_and_links(&pool).await;
    let first = memory.repair_stored_rows().await;
    assert_eq!(outcome(&first, "bare line values"), "Ran(0)", "{first:?}");
    assert_eq!(rows_of_values_and_links(&pool).await, before);
}

/// **A dangling link under a key that is not a line key is outside the repair**,
/// though its key points at bots and a live bot wears its spelling. Only the
/// demonstrated key is read, so the pass cannot reach a reference it has no
/// evidence about.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_dangling_link_under_another_key_is_outside_the_line_repair() {
    let (_scratch, _server, pool, memory) = a_store("repair-not-a-line").await;
    memory
        .declare_type(
            DeclaredType::new(
                "partnered",
                vec![Field::pointing_at("partner", EntityKind::BOT)],
            ),
            None,
        )
        .await
        .expect("declare partner");
    let bot = thing(&memory, "bot:beta", "Beta").await;
    let home = thing(&memory, "work:alpha-queue", "Alpha queue").await;
    let mut fact = NewFact::about(home.clone(), "has a partner", date(2026, 9, 1));
    fact.fields.insert("partner".into(), bot.as_str().into());
    fact.sets.insert("partner".into());
    memory
        .capture(fact)
        .await
        .expect("capture")
        .written()
        .expect("the handle lands");
    let home_key = badge_of(&pool, home.as_str()).await;
    run(
        &pool,
        "UPDATE field_write SET value = 'beta' WHERE entity = ? AND `key` = 'partner'",
        &[&home_key],
    )
    .await;
    run(
        &pool,
        "UPDATE field_link SET target = 'beta' WHERE entity = ? AND `key` = 'partner'",
        &[&home_key],
    )
    .await;
    let before = rows_of_values_and_links(&pool).await;
    let first = memory.repair_stored_rows().await;
    assert_eq!(outcome(&first, "bare line values"), "Ran(0)", "{first:?}");
    assert_eq!(rows_of_values_and_links(&pool).await, before);
    let (_, ids) = found(&pool, StorageClass::DanglingReference).await;
    assert_eq!(ids, vec!["field_link.target: beta".to_string()]);
}
