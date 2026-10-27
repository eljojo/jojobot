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

use jojobot_domain::memory::{EntityId, Guarded, Memory, NewEntity, NewFact};
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

    let (said, _listening) = listening();
    let first = memory.repair_stored_rows().await;
    assert_eq!(outcome(&first, "old merge events"), "Ran(2)", "{first:?}");
    let logged = said.0.lock().expect("kept").join("\n");
    assert!(
        logged.contains("folded=person:skinner")
            && logged.contains("moment=\"recorded\"")
            && logged.contains("folded=person:apu")
            && logged.contains("moment=\"backfilled\""),
        "every event written is logged with the merge it belongs to: {logged}"
    );
    assert!(
        logged.contains("folded=person:marge") && logged.contains("MERGE EVENT NOT WRITTEN"),
        "the merge left is logged: {logged}"
    );
    assert!(
        logged.contains("written=2") && logged.contains("left=1"),
        "{logged}"
    );

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

/// **A merge made before merge events existed, with a later merge over it.** The
/// old thing folds into the middle one, which holds two claims, and the middle one
/// folds into the last. The first merge's event is removed. The repair writes it
/// with the middle thing as survivor and follows the later merge, which the
/// (folded, survivor) pairs and their order show, and a second repair is settled.
/// The old thing holds no claim, so the claims the last holder serves and the
/// unmapped-claim count come from the later merge and do not depend on the
/// repaired event.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_old_merge_under_a_later_one_gets_its_event_with_the_middle_thing_as_survivor() {
    let (_scratch, _server, pool, memory) = a_store("repair-old-merge-chain").await;
    let old = thing(&memory, "person:skinner", "Skinner").await;
    let middle = thing(&memory, "person:krusty", "Krusty").await;
    let last = thing(&memory, "person:homer", "Homer").await;
    for (home, said) in [
        (&middle, "kept the first ledger"),
        (&middle, "kept the second ledger"),
    ] {
        memory
            .capture(NewFact::about(home.clone(), said, date(2026, 9, 1)))
            .await
            .expect("capture")
            .written()
            .expect("the claim lands");
    }
    for (gone, kept) in [(&old, &middle), (&middle, &last)] {
        memory
            .merge(
                gone,
                kept,
                Some("filed twice"),
                date(2026, 9, 2),
                &EntityId("bot:sigma".into()),
                None,
            )
            .await
            .expect("the merge lands");
    }
    let old_key = badge_of(&pool, old.as_str()).await;
    run(
        &pool,
        "DELETE FROM merge_event WHERE folded = ?",
        &[&old_key],
    )
    .await;

    let first = memory.repair_stored_rows().await;
    assert_eq!(outcome(&first, "old merge events"), "Ran(1)", "{first:?}");
    let events = memory.merge_events(None).await.expect("events read");
    let folded: Vec<(&str, &str)> = events
        .iter()
        .map(|event| (event.folded.as_str(), event.survivor.as_str()))
        .collect();
    assert_eq!(
        folded,
        vec![
            ("person:krusty", "person:homer"),
            ("person:skinner", "person:krusty")
        ],
        "the repaired event names the middle thing and follows the later merge"
    );
    let served: Vec<String> = memory
        .recall(&last)
        .await
        .expect("recall")
        .into_iter()
        .map(|fact| fact.content)
        .collect();
    for ledger in ["kept the first ledger", "kept the second ledger"] {
        assert!(
            served.iter().any(|content| content == ledger),
            "the last holder serves {ledger} through the later merge: {served:?}"
        );
    }
    assert_eq!(
        found(&pool, StorageClass::MergedWithoutEvent).await.0,
        0,
        "every merge has its event"
    );
    assert_eq!(
        found(&pool, StorageClass::ClaimUnderFoldedKeyUnmapped)
            .await
            .0,
        0,
        "no claim under a folded key is unmapped"
    );

    let second = memory.repair_stored_rows().await;
    assert_eq!(
        outcome(&second, "old merge events"),
        "Settled",
        "{second:?}"
    );
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
