//! **The startup repairs, proved by real boots of the current binary** on the
//! store the last deployed release filled, amended with the damage the repair
//! exists for: a merge made before merge events existed. Each case boots the
//! binary on the amended store, then again after an interruption that left the
//! work done and its marker unwritten, then again with the marker in place.

use super::super::*;
use jiff::civil::date;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_domain::memory::{EntityId, Guarded, Memory, NewEntity, NewFact};

const FORWARDED_HOLDER: &str = "thing:upgrade-fixture-holder";
const FORWARDED_FOLDED: &str = "thing:upgrade-fixture-husk";
const FORWARDED_SURVIVOR: &str = "thing:upgrade-fixture-heir";
const PASSES: [&str; 1] = ["old merge events"];

/// The claims of the planted chain. An old build moved the folded thing's claims onto its survivor in the merge, so both sit on the middle thing and none on the old one.
const LEDGERS: [&str; 2] = ["kept the first ledger", "kept the second ledger"];

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
/// its repair, so the planted damage would never be repaired. **The pass markers
/// the last release left go on with them**, in the one place that writes them: the
/// cases below count the repairs against a first boot that marks.
async fn as_a_store_before_the_repairs(pool: &sqlx::MySqlPool) {
    said(pool, "DELETE FROM store_pass WHERE name = ?", &PASSES).await;
    deploy_9_1::the_markers_the_last_release_left(pool).await;
}

async fn markers_of_the_repairs(pool: &sqlx::MySqlPool) -> Vec<String> {
    sqlx::query_scalar("SELECT name FROM store_pass WHERE name = ? ORDER BY name")
        .bind(PASSES[0])
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
/// repair and left `after_first`. An interruption is made by removing the
/// marker, which is what a death between the conversion's commit and the marker
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
        "DELETE FROM store_pass WHERE name = ?",
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
    assert_eq!(repeated, [1], "the second boot repeated the repair");
    let mut store = open_the_store(state_dir, store_port).await;
    assert_eq!(
        &tables(store.pool()).await,
        after_first,
        "the repeated repairs changed a table"
    );
    assert_eq!(
        markers_of_the_repairs(store.pool()).await,
        PASSES,
        "the marker is back"
    );
    store.stop().await;

    let third = boot_allowing(state_dir, store_port, git_ref, allowed).await;
    let settled: Vec<usize> = PASSES
        .iter()
        .map(|pass| lines_about(&third.log, "repair pass already done", pass))
        .collect();
    third.stop().await;
    assert_eq!(settled, [1], "the third boot found the marker");
    let mut store = open_the_store(state_dir, store_port).await;
    assert_eq!(
        &tables(store.pool()).await,
        after_first,
        "the marked boot changed a table"
    );
    store.stop().await;
}

/// **A merge made before merge events existed gets its event on the first boot**,
/// when a later merge sits over it and its survivor holds history: the history is
/// still served through both merges, the check finds no unmapped claim, and later
/// boots leave the tables as they were.
#[tokio::test]
async fn an_old_merge_under_a_later_one_gets_its_event_once_across_boots() {
    for restored in deploy_9_1::both_recordings(&fixture_dir(), "repair-old-merge").await {
        an_old_merge_gets_its_event_over(restored).await;
    }
}

async fn an_old_merge_gets_its_event_over(restored: Restored) {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restored;
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
        lines_about(&log, "repair pass finished", PASSES[0]),
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
    for restored in deploy_9_1::both_recordings(&fixture_dir(), "repair-chain-event-kept").await {
        an_old_merge_chain_keeps_its_event_over(restored).await;
    }
}

async fn an_old_merge_chain_keeps_its_event_over(restored: Restored) {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restored;
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
