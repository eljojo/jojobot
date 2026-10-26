//! **The startup repairs, proved by real boots of the current binary** on the
//! store the deploy 9.2.1 build filled, amended with the damage the repairs
//! exist for: bare line names stored as links, and a merge made before merge
//! events existed. Each case boots the binary on the amended store, then again
//! after an interruption that left the work done and its marker unwritten, then
//! again with the markers in place.

use super::super::*;
use jiff::civil::date;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_domain::memory::types::{DeclaredType, Field};
use jojobot_domain::memory::{EntityId, EntityKind, Guarded, Memory, NewEntity, NewFact};

const FORWARDED_HOLDER: &str = "thing:upgrade-fixture-holder";
const FORWARDED_FOLDED: &str = "thing:upgrade-fixture-husk";
const FORWARDED_SURVIVOR: &str = "thing:upgrade-fixture-heir";
const PASSES: [&str; 2] = ["bare line values", "old merge events"];

/// The claims of the planted chain. An old build moved the folded thing's claims onto its survivor in the merge, so both sit on the middle thing and none on the old one.
const LEDGERS: [&str; 2] = ["kept the first ledger", "kept the second ledger"];

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
    as_a_store_before_the_repairs(pool).await;
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
/// merge over it.** The old thing folds into the middle one, which holds the
/// claims an old build moved onto it, and the middle one folds into the last. The first
/// merge's event rows are then removed, as a build that made no events left it,
/// unless `keep_event` leaves them in place.
async fn plant_an_old_merge_under_a_later_one(pool: &sqlx::MySqlPool, keep_event: bool) {
    as_a_store_before_the_repairs(pool).await;
    let memory = opened(pool).await;
    let old = added(&memory, FORWARDED_FOLDED, "An Old Husk").await;
    let middle = added(&memory, FORWARDED_SURVIVOR, "A Middle Heir").await;
    let last = added(&memory, FORWARDED_HOLDER, "A Last Holder").await;
    for (home, said_so) in [(&middle, LEDGERS[0]), (&middle, LEDGERS[1])] {
        memory
            .capture(NewFact::about(home.clone(), said_so, date(2026, 9, 1)))
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
    if !keep_event {
        let key = badge_of(pool, FORWARDED_FOLDED).await;
        said(pool, "DELETE FROM merge_event WHERE folded = ?", &[&key]).await;
    }
}

/// **The merge events of the planted chain** as `(merge_no, folded, survivor)`, in
/// the order the store holds them, with the keys read back as handles.
async fn chain_events(pool: &sqlx::MySqlPool) -> Vec<(i64, String, String)> {
    let keys = [FORWARDED_FOLDED, FORWARDED_SURVIVOR, FORWARDED_HOLDER];
    let mut handles = std::collections::BTreeMap::new();
    for handle in keys {
        handles.insert(badge_of(pool, handle).await, handle.to_string());
    }
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT merge_no, folded, survivor FROM merge_event WHERE ordinal = 0 ORDER BY merge_no",
    )
    .fetch_all(pool)
    .await
    .expect("merge events");
    rows.into_iter()
        .filter(|(_, folded, _)| handles.contains_key(folded))
        .map(|(no, folded, survivor)| {
            let name = |key: &String| handles.get(key).cloned().unwrap_or_else(|| key.clone());
            (no, name(&folded), name(&survivor))
        })
        .collect()
}

/// **The recorded store as a build before the repairs left it.** A build that has
/// the repairs wrote their markers at its own boot, and a marker makes a boot skip
/// its repair, so the planted damage would never be repaired.
async fn as_a_store_before_the_repairs(pool: &sqlx::MySqlPool) {
    said(pool, "DELETE FROM store_pass WHERE name IN (?, ?)", &PASSES).await;
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
    // With the link rule on, the bare-name repair runs ahead of the marking and
    // again in the ordinary sequence, so it finds its marker twice.
    assert_eq!(settled, [2, 1], "the third boot found both markers");
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
    } = restore_a_fixture(&fixture_dir(), "repair-bare-lines").await;
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
    // With the link rule on, the marking removes the link row of the name no bot
    // wears, so the repair names it when it leaves it and the check has no
    // dangling link left to report.
    let left: Vec<&String> = log
        .iter()
        .filter(|line| line.contains("BARE REFERENCE VALUE LEFT"))
        .collect();
    assert!(
        left.len() == 1
            && left[0].contains("address=work:beta-queue#")
            && left[0].contains("target=zeta"),
        "the repair names the one name no bot wears and not the repaired one: {left:?}"
    );
    assert!(
        !log.iter().any(|line| line.contains("STORAGE DAMAGE")),
        "the check finds nothing after the repair: {log:?}"
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
    } = restore_a_fixture(&fixture_dir(), "repair-old-merge").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    plant_an_old_merge_under_a_later_one(store.pool(), false).await;
    let planted = chain_events(store.pool()).await;
    store.stop().await;
    assert_eq!(
        planted
            .iter()
            .map(|(_, folded, survivor)| (folded.as_str(), survivor.as_str()))
            .collect::<Vec<_>>(),
        [(FORWARDED_SURVIVOR, FORWARDED_HOLDER)],
        "only the later merge has its event: {planted:?}"
    );

    // These merges predate link marking, so the link rule at on warns once about
    // each raw `merged_from` value on the first boot, at the counts RECORDED_R1
    // gives. Later boots are strict.
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
    for ledger in LEDGERS {
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
    let mut store = open_the_store(&state_dir, store_port).await;
    let repaired = chain_events(store.pool()).await;
    store.stop().await;
    assert_eq!(
        repaired
            .iter()
            .map(|(_, folded, survivor)| (folded.as_str(), survivor.as_str()))
            .collect::<Vec<_>>(),
        [
            (FORWARDED_SURVIVOR, FORWARDED_HOLDER),
            (FORWARDED_FOLDED, FORWARDED_SURVIVOR)
        ],
        "the written event names the middle thing as survivor and follows the later merge: \
         {repaired:?}"
    );
    assert!(
        repaired[0].0 < repaired[1].0,
        "the repaired event sorts after the later merge: {repaired:?}"
    );

    later_boots_leave_the_tables_alone(&state_dir, store_port, &git_ref, &after_first, &[]).await;
    drop(scratch);
}

/// **The same chain with the first merge's event left in place gets no event
/// written.** The control of the case above: the same planted things, merges and
/// raw `merged_from` values, so the first boot gives the same warnings, and the
/// events of the chain are the same after the boot as before it.
#[tokio::test]
async fn an_old_merge_chain_with_its_event_in_place_gets_none_written() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(&fixture_dir(), "repair-chain-event-kept").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    plant_an_old_merge_under_a_later_one(store.pool(), true).await;
    let planted = chain_events(store.pool()).await;
    store.stop().await;
    assert_eq!(
        planted
            .iter()
            .map(|(_, folded, survivor)| (folded.as_str(), survivor.as_str()))
            .collect::<Vec<_>>(),
        [
            (FORWARDED_FOLDED, FORWARDED_SURVIVOR),
            (FORWARDED_SURVIVOR, FORWARDED_HOLDER)
        ],
        "both merges have their events: {planted:?}"
    );

    let first = boot_current(&state_dir, store_port, &git_ref).await;
    first.stop().await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let after = chain_events(store.pool()).await;
    store.stop().await;
    assert_eq!(after, planted, "the boot wrote or changed a chain event");
    drop(scratch);
}
