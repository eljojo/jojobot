//! **The startup repairs, proved by real boots of the current binary** on the
//! store the deploy 9.2.1 build filled, amended with the damage the repairs
//! exist for: bare line names stored as links, and a merge made before merge
//! events existed. Each case boots the binary on the amended store, then again
//! after an interruption that left the work done and its marker unwritten, then
//! again with the markers in place.

use super::*;
use jiff::civil::date;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_domain::memory::types::{DeclaredType, Field};
use jojobot_domain::memory::{EntityId, EntityKind, Guarded, Memory, NewEntity, NewFact};

const FIXTURE: &str = deploy_9_2_1::DEPLOY_9_2_1_FIXTURE_DIR;
const PASSES: [&str; 2] = ["bare line values", "old merge events"];

/// **The warnings a boot on the amended line store gives on purpose**: the repair
/// leaves a name no bot wears and says so, the check that follows names it, and
/// the planted bots were made without boxes.
const LEFT_ON_PURPOSE: [&str; 3] = [
    "BARE REFERENCE VALUE LEFT",
    "STORAGE DAMAGE",
    "no release notes sent",
];

/// **A boot of the current binary** that may log the warnings in `allowed` and no
/// others.
async fn boot_allowing(
    state_dir: &std::path::Path,
    store_port: u16,
    git_ref: &str,
    allowed: &[&str],
) -> Booted {
    let reports = usize::from(recording_holds_the_plain_ambiguous_handle(git_ref));
    boot_current_expecting(state_dir, store_port, git_ref, Some(reports), allowed).await
}

type Tables = (
    Vec<(String, String, i64, Option<String>)>,
    Vec<(String, String, i64, String)>,
    Vec<(i64, i64, String, String, String, Option<String>)>,
    Vec<(String, Option<String>, Option<String>)>,
);

/// **The rows the repairs read and write**, as the tables hold them: field
/// writes, field links, merge events and the entities' own merge pointers.
async fn tables(pool: &sqlx::MySqlPool) -> Tables {
    (
        sqlx::query_as("SELECT entity, `key`, ordinal, value FROM field_write ORDER BY 1, 2, 3")
            .fetch_all(pool)
            .await
            .expect("field writes"),
        sqlx::query_as("SELECT entity, `key`, ordinal, target FROM field_link ORDER BY 1, 2, 3, 4")
            .fetch_all(pool)
            .await
            .expect("field links"),
        sqlx::query_as(
            "SELECT merge_no, ordinal, folded, survivor, merged_at, session
             FROM merge_event ORDER BY 1, 2",
        )
        .fetch_all(pool)
        .await
        .expect("merge events"),
        sqlx::query_as("SELECT id, badge, merged_into FROM entity ORDER BY 1")
            .fetch_all(pool)
            .await
            .expect("entities"),
    )
}

async fn badge_of(pool: &sqlx::MySqlPool, handle: &str) -> String {
    sqlx::query_scalar("SELECT COALESCE(badge, id) FROM entity WHERE id = ?")
        .bind(handle)
        .fetch_one(pool)
        .await
        .expect("the thing has a key")
}

/// The store's memory with the kinds loaded, as a boot has them.
async fn opened(pool: &sqlx::MySqlPool) -> DoltMemory {
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    memory
}

async fn added(memory: &DoltMemory, handle: &str, name: &str) -> EntityId {
    let id = EntityId(handle.to_string());
    let new = NewEntity::new(id.clone(), name, "test");
    // The recorded store holds things with close names. The screen asks the caller
    // to say these are other things, and the case does.
    if let Guarded::Blocked {
        attempted,
        candidates,
    } = memory.add_entity(new.clone()).await.expect("add")
    {
        let token = jojobot_domain::memory::guard::override_token(&attempted, &candidates);
        memory
            .add_entity(NewEntity {
                override_token: Some(token),
                override_reason: Some("a thing of this case, not one of the recording".into()),
                ..new
            })
            .await
            .expect("add")
            .written()
            .expect("told apart");
    }
    id
}

async fn said(pool: &sqlx::MySqlPool, sql: &str, bind: &[&str]) {
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.to_string()));
    for value in bind {
        query = query.bind(*value);
    }
    query.execute(pool).await.expect(sql);
}

/// **Line values as the old build stored them**: two writes on one work item and
/// one on another, each given a bot through the verbs and then edited so the
/// stored value and its link hold the bare name. One name is a bot's, one is not.
async fn plant_bare_line_names(pool: &sqlx::MySqlPool) {
    let memory = opened(pool).await;
    memory
        .declare_type(
            DeclaredType::new(
                "upgrade-fixture-line",
                vec![Field::pointing_at("line", EntityKind::BOT)],
            ),
            None,
        )
        .await
        .expect("declare line");
    let named = added(&memory, "bot:beta", "Beta").await;
    let other = added(&memory, "bot:gamma", "Gamma").await;
    for (handle, name, line, bare, times) in [
        ("work:alpha-queue", "Alpha queue", &named, "beta", 2),
        ("work:beta-queue", "Beta queue", &other, "zeta", 1),
    ] {
        let home = added(&memory, handle, name).await;
        for _ in 0..times {
            let mut fact = NewFact::about(home.clone(), "assigned a line", date(2026, 9, 1));
            fact.fields.insert("line".into(), line.as_str().into());
            fact.sets.insert("line".into());
            memory
                .capture(fact)
                .await
                .expect("capture")
                .written()
                .expect("the handle lands");
        }
        let key = badge_of(pool, home.as_str()).await;
        said(
            pool,
            "UPDATE field_write SET value = ? WHERE entity = ? AND `key` = 'line'",
            &[bare, &key],
        )
        .await;
        said(
            pool,
            "UPDATE field_link SET target = ? WHERE entity = ? AND `key` = 'line'",
            &[bare, &key],
        )
        .await;
    }
}

/// **A merge made before merge events existed, with history behind it and a later
/// merge over it.** The old thing folds into the middle one, which holds two
/// claims, and the middle one folds into the last. The first merge's event rows
/// are then removed, as a build that made no events left it.
async fn plant_an_old_merge_under_a_later_one(pool: &sqlx::MySqlPool) {
    let memory = opened(pool).await;
    let old = added(&memory, FORWARDED_FOLDED, "An Old Husk").await;
    let middle = added(&memory, FORWARDED_SURVIVOR, "A Middle Heir").await;
    let last = added(&memory, FORWARDED_HOLDER, "A Last Holder").await;
    for said_so in ["kept the first ledger", "kept the second ledger"] {
        memory
            .capture(NewFact::about(middle.clone(), said_so, date(2026, 9, 1)))
            .await
            .expect("capture")
            .written()
            .expect("the claim lands");
    }
    let caller = EntityId("bot:assistant".into());
    for (gone, kept) in [(&old, &middle), (&middle, &last)] {
        memory
            .merge(
                gone,
                kept,
                Some("filed twice"),
                date(2026, 9, 2),
                &caller,
                None,
            )
            .await
            .expect("the merge lands");
    }
    let key = badge_of(pool, FORWARDED_FOLDED).await;
    said(pool, "DELETE FROM merge_event WHERE folded = ?", &[&key]).await;
}

async fn markers_of_the_repairs(pool: &sqlx::MySqlPool) -> Vec<String> {
    sqlx::query_scalar("SELECT name FROM store_pass WHERE name IN (?, ?) ORDER BY name")
        .bind(PASSES[0])
        .bind(PASSES[1])
        .fetch_all(pool)
        .await
        .expect("the markers read")
}

/// How many lines of a boot's log say `words` and name `pass`.
fn lines_about(log: &[String], words: &str, pass: &str) -> usize {
    log.iter()
        .filter(|line| line.contains(words) && line.contains(pass))
        .count()
}

/// **The boots after the first change nothing.** The first boot has done the
/// repairs and left `after_first`. An interruption is made by removing both
/// markers, which is what a death between the conversion's commit and the marker
/// leaves: the second boot repeats the work over rows already repaired. The third
/// boot finds the markers. Both leave every table as the first boot left it.
async fn later_boots_leave_the_tables_alone(
    state_dir: &std::path::Path,
    store_port: u16,
    git_ref: &str,
    after_first: &Tables,
    allowed: &[&str],
) {
    let mut store = open_the_store(state_dir, store_port).await;
    said(
        store.pool(),
        "DELETE FROM store_pass WHERE name IN (?, ?)",
        &PASSES,
    )
    .await;
    store.stop().await;

    let second = boot_allowing(state_dir, store_port, git_ref, allowed).await;
    let repeated: Vec<usize> = PASSES
        .iter()
        .map(|pass| lines_about(&second.log, "repair pass finished", pass))
        .collect();
    second.stop().await;
    assert_eq!(repeated, [1, 1], "the second boot repeated both repairs");
    let mut store = open_the_store(state_dir, store_port).await;
    assert_eq!(
        &tables(store.pool()).await,
        after_first,
        "the repeated repairs changed a table"
    );
    assert_eq!(
        markers_of_the_repairs(store.pool()).await,
        PASSES,
        "both markers are back"
    );
    store.stop().await;

    let third = boot_allowing(state_dir, store_port, git_ref, allowed).await;
    let settled: Vec<usize> = PASSES
        .iter()
        .map(|pass| lines_about(&third.log, "repair pass already done", pass))
        .collect();
    third.stop().await;
    assert_eq!(settled, [1, 1], "the third boot found both markers");
    let mut store = open_the_store(state_dir, store_port).await;
    assert_eq!(
        &tables(store.pool()).await,
        after_first,
        "the marked boot changed a table"
    );
    store.stop().await;
}

/// **Bare line names stored as links are pointed at their bots by the first
/// boot**, which says so; the storage check that follows names only the name no
/// bot wears; the served read gives the handle; and later boots leave the tables
/// as they were.
#[tokio::test]
async fn bare_line_names_are_pointed_at_their_bots_once_across_boots() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(FIXTURE, "repair-bare-lines").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    plant_bare_line_names(store.pool()).await;
    let planted = tables(store.pool()).await;
    let beta = badge_of(store.pool(), "bot:beta").await;
    store.stop().await;

    let first = boot_allowing(&state_dir, store_port, &git_ref, &LEFT_ON_PURPOSE).await;
    let log = first.log.clone();
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", first.http_port))
        .await
        .expect("connecting to the current binary");
    let read = surface
        .call("recall", json!({"subject": "work:alpha-queue"}))
        .await;
    first.stop().await;
    let read: serde_json::Value = serde_json::from_str(&read)
        .unwrap_or_else(|_| panic!("recorded at {git_ref}: the read did not answer: {read}"));
    assert_eq!(
        read["objects"][0]["fields"]["line"], "bot:beta",
        "the served read gives the handle: {read}"
    );
    assert_eq!(
        lines_about(&log, "repair pass finished", PASSES[0]),
        1,
        "the first boot says it repaired the line names"
    );
    let damage: Vec<&String> = log
        .iter()
        .filter(|line| line.contains("STORAGE DAMAGE"))
        .collect();
    assert!(
        damage.len() == 1 && damage[0].contains("zeta") && !damage[0].contains("beta"),
        "the check names the one name no bot wears and not the repaired one: {damage:?}"
    );

    let mut store = open_the_store(&state_dir, store_port).await;
    let after_first = tables(store.pool()).await;
    store.stop().await;
    assert_ne!(after_first, planted, "the first boot changed the tables");
    let linked = after_first
        .1
        .iter()
        .filter(|row| row.1 == "line" && row.3 == beta)
        .count();
    assert_eq!(
        linked, 2,
        "both writes link to the bot: {:?}",
        after_first.1
    );

    later_boots_leave_the_tables_alone(
        &state_dir,
        store_port,
        &git_ref,
        &after_first,
        &LEFT_ON_PURPOSE,
    )
    .await;
    drop(scratch);
}

/// **A merge made before merge events existed gets its event on the first boot**,
/// when a later merge sits over it and its survivor holds history: the history is
/// still served through both merges, the check finds no unmapped claim, and later
/// boots leave the tables as they were.
#[tokio::test]
async fn an_old_merge_under_a_later_one_gets_its_event_once_across_boots() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(FIXTURE, "repair-old-merge").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    plant_an_old_merge_under_a_later_one(store.pool()).await;
    let events_before = tables(store.pool())
        .await
        .2
        .iter()
        .filter(|row| row.1 == 0)
        .count();
    store.stop().await;

    let first = boot_current(&state_dir, store_port, &git_ref).await;
    let log = first.log.clone();
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", first.http_port))
        .await
        .expect("connecting to the current binary");
    let read = surface
        .call(
            "recall",
            json!({"subject": FORWARDED_HOLDER, "facts": true}),
        )
        .await;
    first.stop().await;
    for ledger in ["kept the first ledger", "kept the second ledger"] {
        assert!(
            read.contains(ledger),
            "the history is still served through both merges ({ledger}): {read}"
        );
    }
    assert_eq!(
        lines_about(&log, "repair pass finished", PASSES[1]),
        1,
        "the first boot says it wrote the event"
    );
    assert!(
        !log.iter().any(|line| line.contains("STORAGE DAMAGE")),
        "the check finds nothing after the repair, no unmapped claim among it: {log:?}"
    );

    let mut store = open_the_store(&state_dir, store_port).await;
    let after_first = tables(store.pool()).await;
    store.stop().await;
    let events_after = after_first.2.iter().filter(|row| row.1 == 0).count();
    assert_eq!(events_after, events_before + 1, "one event was written");

    later_boots_leave_the_tables_alone(&state_dir, store_port, &git_ref, &after_first, &[]).await;
    drop(scratch);
}
