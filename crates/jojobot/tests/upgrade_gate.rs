//! **The upgrade proof: a store an older binary filled, read by this one.**
//!
//! `real_binary_serves_a_store_with_existing_rows` proves restart-safety —
//! the binary under test seeds the store itself, so the rows it meets were
//! written by the SAME schema and the SAME parsing it is about to run. That
//! is not the outage class this catches: startup parsed a stored row before
//! loading what it needed to parse it, and the incident happened between
//! two different binaries, not two runs of one.
//!
//! This loads a store dumped from a binary built at an older ref —
//! `crates/jojobot/tests/fixtures/upgrade_deploy*/doltdump.sql`, recorded by
//! `make refresh-upgrade-fixture`, never by hand — into a fresh disposable
//! store, boots the CURRENT binary on it, and asserts both that startup
//! succeeds and that every record the recording wrote reads back correctly
//! through the served surface.

use std::process::Stdio;
use std::time::Duration;

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::testing::free_port;
use jojobot_exercise::surface::Surface;
use serde_json::json;

mod support;

/// The fixture recorded from the build deploy 3 shipped: a store that build
/// filled is what a deployed instance holds when it takes this one. The gate
/// keeps the fixtures of the deployed builds it proves an upgrade from.
const DEPLOY_3_FIXTURE_DIR: &str = "tests/fixtures/upgrade_deploy3";
/// The fixture recorded from the build deploy 4 shipped, the build the operator
/// runs now. It holds rows no older build writes: role objects in their own
/// shape, a key written under `sets`, a closing entry on a wrapped run, a moment
/// on every write and a wrap window.
const DEPLOY_4_FIXTURE_DIR: &str = "tests/fixtures/upgrade_deploy4";
/// The fixture recorded from the build deploy 5 shipped, the build the operator
/// runs when deploy 6 takes over (03311d93). It holds what that build wrote over
/// a store it migrated itself: the pass markers its own boot set, folds made
/// before any merge event existed with pointers left at the folded thing, a
/// fold whose survivor was renamed, and a run with hundreds of per-write rows.
const DEPLOY_5_FIXTURE_DIR: &str = "tests/fixtures/upgrade_deploy5";
/// The fixture recorded from the build deploy 6.1 shipped (c99fc847), the build
/// the operator runs when deploy 7 takes over, and the NEWEST fixture: a case that
/// wants "the store the deployed build filled" reads this one. It holds what that
/// build wrote and no older one does: a numbered collection with an archived
/// member, a view that takes an input, a chain of merges that left events, a
/// document with another folded into it, and runs with a trail of entries, an
/// amended one and a wrap window. Its `session` table has none of the columns the
/// migrations 0094 to 0098 add, no `session_key` table (0099) and no
/// `request_part` table (0100), no `tool_call_count` table (0101) and no
/// `served_day` table (0102), so the boot over it applies all nine.
const DEPLOY_6_FIXTURE_DIR: &str = "tests/fixtures/upgrade_deploy6";

/// A directory of this run's own, removed when it is done.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// **The fixture, restored into a store of this case's own.** Two cases run
/// side by side in this binary, so each takes a directory named for itself: a
/// name from the process alone would be one directory for both.
struct Restored {
    /// Removes the directory when the case is done.
    scratch: Scratch,
    state_dir: std::path::PathBuf,
    store_port: u16,
    git_ref: String,
    /// What every thing held in the old store, as `(handle, key, value)`, read
    /// before the current binary touched it.
    held_before: Vec<(String, String, String)>,
    /// Every entity handle the old store holds, archived and folded ones
    /// included, read before the current binary touched it.
    handles: Vec<String>,
    /// **The key of the recorded holder run, when the recording build kept one.**
    /// A build that keys its runs stores only the hash of a key, so the gate sets
    /// that hash to the hash of a key of its own and holds the key here; a build
    /// that began runs without keys leaves the holder to be exchanged by its name.
    holder_key: Option<String>,
}

async fn restore_the_fixture(label: &str) -> Restored {
    restore_a_fixture(DEPLOY_3_FIXTURE_DIR, label).await
}

/// **A fixture directory, restored into a store of this case's own.**
async fn restore_a_fixture(fixture_dir: &str, label: &str) -> Restored {
    restore_a_fixture_recorded_earlier(fixture_dir, label, 0).await
}

/// **Every moment a store holds, moved back by `hours`**, as if the recording had
/// been made that much earlier. The gate runs on the day it runs, so the age of
/// a recording is whatever the calendar makes it; this makes the age a thing a
/// case chooses. It reads every column of every table, so a moment a later build
/// adds is aged without anybody remembering to list it. The epoch is the store's
/// own "no moment" and stays where it is.
async fn age_the_recording(pool: &sqlx::MySqlPool, hours: i64) {
    if hours == 0 {
        return;
    }
    let columns: Vec<(String, String)> = sqlx::query_as(
        "SELECT CAST(table_name AS CHAR), CAST(column_name AS CHAR)
         FROM information_schema.columns
         WHERE table_schema = DATABASE() AND data_type IN ('varchar', 'text', 'char', 'longtext')",
    )
    .fetch_all(pool)
    .await
    .expect("the store's columns read");
    for (table, column) in columns {
        let held: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT DISTINCT `{column}` FROM `{table}` WHERE `{column}` LIKE '____-__-__T%Z'"
        )))
        .fetch_all(pool)
        .await
        .unwrap_or_else(|e| panic!("reading the moments in {table}.{column}: {e}"));
        let mut moments: Vec<(jiff::Timestamp, String)> = held
            .into_iter()
            .filter_map(|text| text.parse().ok().map(|moment| (moment, text)))
            .filter(|(moment, _)| *moment > jiff::Timestamp::UNIX_EPOCH)
            .collect();
        // The moments furthest from where they are going first, so a moved moment
        // never lands on one still to move.
        moments.sort();
        if hours < 0 {
            moments.reverse();
        }
        for (moment, text) in moments {
            let moved = moment
                .checked_sub(jiff::SignedDuration::from_hours(hours))
                .expect("an aged moment is in range")
                .to_string();
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE `{table}` SET `{column}` = ? WHERE `{column}` = ?"
            )))
            .bind(moved)
            .bind(text)
            .execute(pool)
            .await
            .unwrap_or_else(|e| panic!("aging a moment in {table}.{column}: {e}"));
        }
    }
}

/// **A fixture directory, restored into a store of this case's own, and aged**
/// by `earlier_hours` before the gate moves the moments it pins.
async fn restore_a_fixture_recorded_earlier(
    fixture_dir: &str,
    label: &str,
    earlier_hours: i64,
) -> Restored {
    let fixture_ref = format!("{fixture_dir}/ref.txt");
    let fixture_dump = format!("{fixture_dir}/doltdump.sql");
    let git_ref = std::fs::read_to_string(&fixture_ref)
        .unwrap_or_else(|e| panic!("reading {fixture_ref}: {e}"))
        .trim()
        .to_string();

    let state_dir = std::env::temp_dir().join(format!(
        "jojobot-upgrade-gate-{}-{label}",
        std::process::id()
    ));
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let scratch = Scratch(state_dir.clone());
    let db_dir = state_dir.join("db");
    let store_port = free_port();

    // **Restore through the same layout `Dolt::start` itself produces** —
    // an empty store brought up the ordinary way, then the fixture's own
    // dump replayed over the live connection, so the directory this test's
    // binary meets is built by the exact code every other suite already
    // trusts to build it, not a hand-rolled copy of that layout.
    let mut restoring = Dolt::start(&db_dir, store_port)
        .await
        .expect("a fresh store comes up to receive the fixture");
    let dump = std::fs::read_to_string(&fixture_dump)
        .unwrap_or_else(|e| panic!("reading {fixture_dump}: {e}"));
    for statement in split_sql_statements(&dump) {
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement.as_str()))
            .execute(restoring.pool())
            .await
            .unwrap_or_else(|e| {
                panic!("replaying the fixture recorded at {git_ref} failed on:\n{statement}\n{e}")
            });
    }
    // **The recording is aged before the gate pins anything.** The moments a case
    // compares against the clock are the three the edits below set, each to a
    // moment of its own choosing, so the age the recording was given changes
    // none of them; every other moment in a fixture is only ever read back, and
    // the aged cases after the boot cases are the standing proof.
    let first_migration = |pool: sqlx::MySqlPool| async move {
        let applied: String = sqlx::query_scalar("SELECT MIN(applied_at) FROM schema_migration")
            .fetch_one(&pool)
            .await
            .expect("the first migration's moment reads");
        applied
            .parse::<jiff::Timestamp>()
            .unwrap_or_else(|e| panic!("a recorded moment {applied:?} does not parse: {e}"))
    };
    let recorded_at = first_migration(restoring.pool().clone()).await;
    age_the_recording(restoring.pool(), earlier_hours).await;
    // **Stated, so an ageing that moved nothing cannot pass for one that did.**
    let aged_to = first_migration(restoring.pool().clone()).await;
    assert_eq!(
        recorded_at.duration_since(aged_to),
        jiff::SignedDuration::from_hours(earlier_hours),
        "the recording made at {git_ref} was not aged by {earlier_hours} hours"
    );
    // **The old claim is moved to ten minutes ago, as the store of a deploy a
    // little while ago holds it.** The recording is hours old and its lease has
    // lapsed by the time this runs, and a lapsed lease proves nothing about a
    // role being read in the shape it was written in. Ten minutes is inside the
    // lease and past the renewal age: a younger moment would make the holder's
    // write renew nothing, so the claim would not move. This is the one edit made
    // to the recording, and it changes a moment, not a shape: the key is the old
    // build's own.
    sqlx::query("UPDATE field_write SET value = ? WHERE `key` = ?")
        .bind((jiff::Timestamp::now() - jiff::SignedDuration::from_mins(10)).to_string())
        .bind("role/upgrade-fixture-holder/claimed_at")
        .execute(restoring.pool())
        .await
        .expect("the recorded claim's moment is moved to now");
    // **The holder's run is moved to ten minutes ago too, so the sweep that runs
    // when the binary boots does not find it silent for a day.** A session with
    // no entry has nothing to measure but its own start, and the recording's
    // holder wrote none. The recording is a day old once the date moves on, and
    // the sweep then marks the run abandoned and refuses its write: the cases
    // below passed on the day they were recorded and failed on the next. Like the
    // claim, it is one edit to a moment and changes no shape.
    let holder_sid = std::fs::read_to_string(format!("{fixture_dir}/role_holder.txt"))
        .unwrap_or_else(|e| panic!("reading {fixture_dir}/role_holder.txt: {e}"))
        .trim()
        .to_string();
    sqlx::query("UPDATE session SET started_at = ? WHERE sid = ?")
        .bind((jiff::Timestamp::now() - jiff::SignedDuration::from_mins(10)).to_string())
        .bind(&holder_sid)
        .execute(restoring.pool())
        .await
        .expect("the holder run's moment is moved to now");
    // **A run a keyed build began keeps the hash of its key and nothing else**, so
    // the gate cannot act as it. The hash is set to the hash of a key the gate
    // minted, which changes the secret and no shape.
    let keyed: Option<String> = if has_column(restoring.pool(), "session", "key_hash").await {
        sqlx::query_scalar("SELECT key_hash FROM session WHERE sid = ?")
            .bind(&holder_sid)
            .fetch_optional(restoring.pool())
            .await
            .expect("the holder run's key reads")
            .flatten()
    } else {
        None
    };
    let holder_key = if keyed.is_some() {
        let key = jojobot_domain::session::mint_key();
        sqlx::query("UPDATE session SET key_hash = ? WHERE sid = ?")
            .bind(jojobot_domain::session::hash_key(&key))
            .bind(&holder_sid)
            .execute(restoring.pool())
            .await
            .expect("the holder run's key hash is set to a key the gate holds");
        Some(key)
    } else {
        None
    };
    // **The holder's entries are moved with its start, all by the same amount.**
    // The sweep measures a run by its newest entry, and a recording whose holder
    // wrote entries (the closing and the beats of a later build) stamps them at
    // the hour it was recorded, so moving only the start left the run reading as
    // gone quiet an hour after the recording and for ever after. The newest entry
    // lands ten minutes ago and each of the others keeps its distance from it, so
    // the chronology keeps its order and its gaps. A holder with no entry moves
    // nothing here.
    let entries: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT j.id, j.at, j.touched FROM journal_entry j
         JOIN session s ON s.id = j.session WHERE s.sid = ?",
    )
    .bind(&holder_sid)
    .fetch_all(restoring.pool())
    .await
    .expect("the holder run's entries read");
    let moment = |text: &str| -> jiff::Timestamp {
        text.parse()
            .unwrap_or_else(|e| panic!("a recorded moment {text:?} does not parse: {e}"))
    };
    let newest = entries
        .iter()
        .flat_map(|(_, at, touched)| [Some(at.as_str()), touched.as_deref()])
        .flatten()
        .map(moment)
        .max();
    if let Some(newest) = newest {
        let shift =
            (jiff::Timestamp::now() - jiff::SignedDuration::from_mins(10)).duration_since(newest);
        let moved = |text: &str| {
            moment(text)
                .checked_add(shift)
                .expect("a moved moment is in range")
                .to_string()
        };
        for (id, at, touched) in &entries {
            sqlx::query(
                "UPDATE journal_entry SET at = ?, touched = ?
                 WHERE id = ? AND session = (SELECT id FROM session WHERE sid = ?)",
            )
            .bind(moved(at))
            .bind(touched.as_deref().map(moved))
            .bind(id)
            .bind(&holder_sid)
            .execute(restoring.pool())
            .await
            .expect("the holder run's entry moment is moved");
        }
    }
    // **Stated here so the fixture's age cannot hide.** The sweep measures a run
    // by its newest entry, or its start when it has none; the holder must read as
    // recent whatever day the gate runs on.
    let last_beat: String = sqlx::query_scalar(
        "SELECT COALESCE(MAX(GREATEST(j.at, COALESCE(j.touched, j.at))), s.started_at)
         FROM session s LEFT JOIN journal_entry j ON j.session = s.id
         WHERE s.sid = ? GROUP BY s.id, s.started_at",
    )
    .bind(&holder_sid)
    .fetch_one(restoring.pool())
    .await
    .expect("the holder run's last moment reads");
    let last_beat: jiff::Timestamp = last_beat.parse().expect("a moment");
    assert!(
        jiff::Timestamp::now().duration_since(last_beat) < jiff::SignedDuration::from_hours(1),
        "the holder run in the fixture recorded at {git_ref} was last active at {last_beat}, \
         which the sweep reads as a run gone quiet"
    );
    // **What every thing holds, read off the old store with plain SQL before the
    // current binary touches it.** Independent of any fold in this build, so a
    // migration or a filter that changed what a thing holds cannot move this side
    // with it.
    let held_before = fields_every_thing_holds(restoring.pool()).await;
    assert_every_held_row_joined(restoring.pool(), &git_ref).await;
    assert!(
        held_before.len() >= 5,
        "the fixture holds things with fields, or this compares nothing: {held_before:?}"
    );
    let handles: Vec<String> = sqlx::query_scalar("SELECT id FROM entity ORDER BY id")
        .fetch_all(restoring.pool())
        .await
        .expect("the old store's handles read");
    restoring.stop().await;
    Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        held_before,
        handles,
        holder_key,
    }
}

/// **The outbox migration arrives on a store a deployed build filled, and a
/// message posted to the operator is pushed from it.** The restored store has no
/// outbox table. The current binary, set up for ntfy, boots over it and applies
/// the migration; a message posted to the operator through the served surface
/// reaches a server standing in for ntfy; and the row in the store ends as sent.
/// The boot holds to the gate's own bar: no warning, no error line.
#[tokio::test]
async fn the_push_outbox_arrives_on_a_deployed_store_and_a_post_to_the_operator_is_pushed() {
    a_post_to_the_operator_is_pushed_and_its_row_ends_sent("push-outbox", None).await;
}

/// **The row ends as sent however late the server says it has the message.** The
/// sender marks the row after the answer, and a stop leaves a push that has not
/// been marked pending, so the case that stops the server on seeing the push
/// reads a row the sender has not reached yet when the answer is late. A loaded
/// machine makes every answer late; this one is late on purpose.
#[tokio::test]
async fn the_push_outbox_row_ends_sent_when_the_server_answers_late() {
    a_post_to_the_operator_is_pushed_and_its_row_ends_sent(
        "push-outbox-late",
        Some(Duration::from_millis(1500)),
    )
    .await;
}

async fn a_post_to_the_operator_is_pushed_and_its_row_ends_sent(
    label: &str,
    late: Option<Duration>,
) {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_the_fixture(label).await;

    // The deployed build had no outbox: the table is absent before the boot.
    let before = open_the_store(&state_dir, store_port).await;
    let tables: Vec<String> = sqlx::query_scalar("SHOW TABLES LIKE 'push_outbox'")
        .fetch_all(before.pool())
        .await
        .expect("the tables read");
    assert!(
        tables.is_empty(),
        "the restored store already has an outbox: {tables:?}"
    );
    let mut before = before;
    before.stop().await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let ntfy = support::FakeNtfy::start().await;
    if let Some(wait) = late {
        ntfy.answer_late(wait);
    }
    let reports = usize::from(recording_holds_the_plain_ambiguous_handle(&git_ref));
    let booted = boot_current_with(
        &state_dir,
        store_port,
        &git_ref,
        false,
        Some(reports),
        &[],
        &[
            ("JOJOBOT_NTFY_URL", ntfy.url.clone()),
            ("JOJOBOT_NTFY_TOPIC", "phone".to_string()),
        ],
    )
    .await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let started = surface
        .must(
            "start_here",
            json!({"bot": "assistant", "brief": true, "resume": "new"}),
        )
        .await
        .expect("the shipped identity boots");
    let sid = started["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("the boot handed back no handle: {started}"))
        .to_string();
    // The deployed instance may already name its operator; if it does not, the
    // case names one, the way an operator does.
    let instance = surface
        .must("recall", json!({"subject": "topic:instance", "sid": sid}))
        .await
        .expect("the instance record reads");
    let operator = match instance["objects"][0]["fields"]["operator"].as_str() {
        Some(named) => named.to_string(),
        None => {
            surface
                .must(
                    "add_entity",
                    json!({"kind": "person", "handle": "ned-flanders", "name": "Ned Flanders",
                           "source": "user-named", "sid": sid}),
                )
                .await
                .expect("the operator is added");
            surface
                .must(
                    "capture",
                    json!({"subject": "topic:instance", "content": "who the operator is",
                           "provenance": "testimony",
                           "sets": {"operator": "person:ned-flanders"}, "sid": sid}),
                )
                .await
                .expect("the operator is named");
            "person:ned-flanders".to_string()
        }
    };
    let posted = surface
        .must(
            "post_message",
            json!({"to": operator, "subject": "the lease",
                   "body": "it is signed", "sid": sid}),
        )
        .await
        .expect("the post to the operator lands");
    let id = posted["id"]
        .as_str()
        .expect("a post hands back its id")
        .to_string();

    let mut waited = 0;
    while ntfy.published().is_empty() && waited < 500 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        waited += 1;
    }
    let published = ntfy.published();
    assert_eq!(
        published.len(),
        1,
        "one push reached the server: {published:?}"
    );
    assert_eq!(published[0].body["title"], "the lease");
    // **The push is seen when the server has the message, and the row is marked
    // when the sender has the answer.** A stop leaves a push that is not yet
    // marked pending, so the row is waited for on the store the binary is still
    // serving, and the stop comes after it. The loop ends the moment the row
    // reads `sent`; its count is a guard against a hang, never the measure.
    let live = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(1)
        .connect_lazy(&format!("mysql://root@127.0.0.1:{store_port}/jojobot"))
        .expect("a pool on the store the binary is serving");
    let mut marked = 0;
    loop {
        let states: Vec<String> = sqlx::query_scalar("SELECT state FROM push_outbox")
            .fetch_all(&live)
            .await
            .expect("the outbox reads while the binary serves");
        if states == ["sent"] {
            break;
        }
        marked += 1;
        assert!(marked < 6000, "the row never ended as sent: {states:?}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    live.close().await;
    surface.finish().await;
    booted.stop().await;

    // The store ends with the row, sent: the migration made the table and the
    // post wrote into it in the same transaction as the message.
    let mut after = open_the_store(&state_dir, store_port).await;
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT message_id, state FROM push_outbox")
        .fetch_all(after.pool())
        .await
        .expect("the outbox reads");
    assert_eq!(rows, vec![(id, "sent".to_string())]);
    after.stop().await;
    drop(scratch);
}

/// The same proof over the store the build deploy 3 shipped filled.
#[tokio::test]
async fn the_current_binary_boots_on_a_store_the_deploy_3_binary_filled() {
    boots_on_a_store_filled_by(DEPLOY_3_FIXTURE_DIR, "boots-deploy3").await;
}

/// The same proof over the store the build deploy 4 shipped filled.
#[tokio::test]
async fn the_current_binary_boots_on_a_store_the_deploy_4_binary_filled() {
    boots_on_a_store_filled_by(DEPLOY_4_FIXTURE_DIR, "boots-deploy4").await;
}

/// The same proof over the store the build deploy 6.1 shipped filled: the boot
/// applies the nine migrations 0094 to 0102 and logs no warning and no error.
#[tokio::test]
async fn the_current_binary_boots_on_a_store_the_deploy_6_binary_filled() {
    boots_on_a_store_filled_by(DEPLOY_6_FIXTURE_DIR, "boots-deploy6").await;
}

/// **A deploy that dies partway through a migration restarts cleanly on the
/// store production holds.** For every migration the head ships beyond what the
/// deploy 6 recording's ledger holds, the head boots over that recording in both
/// states a death inside the migration leaves: the marker committed and the
/// change not landed, and the change landed and the ledger row missing. Each boot
/// is held to the bar of a clean boot (no warning, no error, a second boot that
/// changes nothing, every recorded record read back), and the ledger ends whole
/// with no marker left. The list is the build's own, so a migration added later is
/// covered without a case of its own.
#[tokio::test]
async fn a_boot_after_a_death_inside_any_new_migration_completes_it_and_reads_back_as_a_clean_boot()
{
    use jojobot_adapters::dolt::migrate::{Interruption, leave_interrupted, shipped_versions};

    let recorded = recorded_ledger(DEPLOY_6_FIXTURE_DIR).await;
    let shipped = shipped_versions();
    let beyond: Vec<&str> = shipped
        .iter()
        .copied()
        .filter(|version| !recorded.iter().any(|seen| seen == version))
        .collect();
    if beyond.is_empty() {
        assert_eq!(
            recorded.len(),
            shipped.len(),
            "the head adds no migration beyond the deploy 6 recording, so there is none to \
             interrupt"
        );
        return;
    }
    for version in beyond {
        for how in [Interruption::BeforeTheChange, Interruption::AfterTheChange] {
            let label = format!("interrupted-{version}-{how:?}");
            let restored = restore_a_fixture(DEPLOY_6_FIXTURE_DIR, &label).await;
            let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
            leave_interrupted(store.pool(), version, how)
                .await
                .unwrap_or_else(|e| panic!("leaving {version} interrupted ({how:?}): {e}"));
            // **The case built what it claims**: the marker stands and the ledger
            // does not hold the version, so a helper that did nothing cannot pass.
            let begun: Vec<String> =
                sqlx::query_scalar("SELECT version FROM schema_migration_begun")
                    .fetch_all(store.pool())
                    .await
                    .expect("the marker table reads");
            assert!(
                begun.iter().any(|seen| seen == version),
                "{version} {how:?}: {begun:?}"
            );
            let ledger: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
                .fetch_all(store.pool())
                .await
                .expect("the ledger reads");
            assert!(
                !ledger.iter().any(|seen| seen == version),
                "{version} {how:?}: the ledger already holds it"
            );
            store.stop().await;

            boots_on_a_restored_store(DEPLOY_6_FIXTURE_DIR, restored).await;
        }
    }
}

/// The versions a fixture's ledger holds, read from its dump on a store of the
/// case's own.
async fn recorded_ledger(fixture_dir: &str) -> Vec<String> {
    let restored = restore_a_fixture(fixture_dir, "ledger").await;
    let mut store = open_the_store(&restored.state_dir, restored.store_port).await;
    let ledger: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
        .fetch_all(store.pool())
        .await
        .expect("the recording's ledger reads");
    store.stop().await;
    ledger
}

#[tokio::test]
async fn the_current_binary_boots_on_a_deploy_6_store_recorded_six_hours_earlier() {
    boots_on_a_store_recorded_earlier(DEPLOY_6_FIXTURE_DIR, "boots-deploy6-earlier", 6).await;
}

#[tokio::test]
async fn the_current_binary_boots_on_a_deploy_6_store_recorded_three_days_earlier() {
    boots_on_a_store_recorded_earlier(DEPLOY_6_FIXTURE_DIR, "boots-deploy6-earlier-days", 72).await;
}

/// **The gate does not depend on how old the recording is.** The same boot, over
/// the same fixture, as if it had been made six hours earlier than it was, and
/// again as if three days earlier. A case that compares a recorded moment
/// against the clock passes on the day it is recorded and fails on a later one,
/// and the calendar is not a thing the gate can choose; this makes it one.
///
/// **The moments a case compares against the clock, and what pins each**, found
/// by ageing every moment in both fixtures from ninety-six hours later to two
/// hundred hours earlier and running the whole gate, then aiming each pin at
/// nothing and running it again:
///
/// | moment | compared by | pinned by | reddens without the pin |
/// |---|---|---|---|
/// | the holder run's `session.started_at` (the sweep drops a run quiet for 24 hours) | the boot's sweep | moved to ten minutes ago | the aged boot cases on deploy 3, whose holder wrote no entry |
/// | the holder run's journal entries, `at` and `touched` | the same sweep | moved with the start, keeping their gaps | the aged boot cases on deploy 4, whose holder wrote entries |
/// | the holder's role `claimed_at` (a lease of 45 minutes) | nothing today | moved to ten minutes ago, on deploy 3 only: deploy 4 stores the claim in the role object, which the pin does not match | no case: the whole gate stays green with the pin aimed at nothing |
///
/// Every other moment (entity and claim write times, `fact.inserted_at`, the
/// other runs' starts, the other roles' claims, messages, quarantine, teachings,
/// migrations) is read back and compared with nothing; the cases below prove it
/// by passing with all of them aged.
#[tokio::test]
async fn the_current_binary_boots_on_a_deploy_3_store_recorded_six_hours_earlier() {
    boots_on_a_store_recorded_earlier(DEPLOY_3_FIXTURE_DIR, "boots-deploy3-earlier", 6).await;
}

#[tokio::test]
async fn the_current_binary_boots_on_a_deploy_4_store_recorded_six_hours_earlier() {
    boots_on_a_store_recorded_earlier(DEPLOY_4_FIXTURE_DIR, "boots-deploy4-earlier", 6).await;
}

#[tokio::test]
async fn the_current_binary_boots_on_a_deploy_3_store_recorded_three_days_earlier() {
    boots_on_a_store_recorded_earlier(DEPLOY_3_FIXTURE_DIR, "boots-deploy3-earlier-days", 72).await;
}

#[tokio::test]
async fn the_current_binary_boots_on_a_deploy_4_store_recorded_three_days_earlier() {
    boots_on_a_store_recorded_earlier(DEPLOY_4_FIXTURE_DIR, "boots-deploy4-earlier-days", 72).await;
}

/// **The rows only the deployed build writes, read back by the current binary.**
/// A key written under `sets`, a role object in its own shape, a closing entry on
/// a wrapped run, a moment on every write and a wrap window are in no older
/// recording. The case reads them in the recording BEFORE any boot, with plain
/// SQL, so a fixture that lacks them fails here and not somewhere later; then
/// boots the current binary over the store and reads the same things through the
/// served surface.
#[tokio::test]
async fn the_rows_only_the_deploy_4_build_writes_read_back_on_the_current_binary() {
    the_rows_only_the_recording_build_writes_read_back(DEPLOY_4_FIXTURE_DIR, "rows-deploy4").await;
}

/// **The case above reads the new fixture, and it fails on an older one.** Pointed
/// at the deploy 3 recording it stops at the first row that build did not write,
/// naming what is missing. Without this, a case that read the wrong directory and
/// asserted nothing the old recording could not also pass would stay green.
#[tokio::test]
#[should_panic(expected = "does not hold what only the deploy 4 build writes")]
async fn the_rows_case_fails_on_a_fixture_an_older_build_recorded() {
    the_rows_only_the_recording_build_writes_read_back(DEPLOY_3_FIXTURE_DIR, "rows-on-deploy3")
        .await;
}

/// **The rows only the deploy 6 build writes, read back by the current binary,
/// over the nine migrations the boot applies.** A numbered collection, a view that
/// takes an input, a chain of merges, a document with another folded into it and a
/// run with a trail are in no older recording. The case reads them in the
/// recording BEFORE any boot, with plain SQL, together with the shape of the
/// `session` table that the migrations 0094 to 0102 change, so a fixture recorded
/// by a later build fails here and not later; boots the current binary; reads the
/// nine migrations' objects; reads every one of those things through the served
/// surface; and finds the calls that reading made counted in the new table.
#[tokio::test]
async fn the_rows_only_the_deploy_6_build_writes_read_back_over_the_nine_new_migrations() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(DEPLOY_6_FIXTURE_DIR, "rows-deploy6").await;
    let fail = |what: &str, body: &str| -> ! {
        panic!(
            "the fixture recorded at {git_ref} does not hold what only the deploy 6 build \
             writes, or the current binary does not read it: {what}: {body}"
        )
    };

    // ── in the recording, before any boot ───────────────────────────────────
    let mut store = open_the_store(&state_dir, store_port).await;
    let pool = store.pool();
    let migration_objects = |pool: &sqlx::MySqlPool| {
        let pool = pool.clone();
        async move {
            let columns: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM information_schema.columns \
                 WHERE table_schema = DATABASE() AND table_name = 'session' \
                 AND column_name IN ('retired_key_hash', 'retired_at', 'retired_by', 'taken_at')",
            )
            .fetch_one(&pool)
            .await
            .expect("the columns of the session table read");
            let index: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM information_schema.statistics \
                 WHERE table_schema = DATABASE() AND table_name = 'session' \
                 AND index_name = 'by_retired_key_hash'",
            )
            .fetch_one(&pool)
            .await
            .expect("the indexes of the session table read");
            let table: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM information_schema.tables \
                 WHERE table_schema = DATABASE() AND table_name = 'session_key'",
            )
            .fetch_one(&pool)
            .await
            .expect("the tables read");
            let parts: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM information_schema.tables \
                 WHERE table_schema = DATABASE() AND table_name = 'request_part'",
            )
            .fetch_one(&pool)
            .await
            .expect("the tables read");
            let counts: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM information_schema.tables \
                 WHERE table_schema = DATABASE() AND table_name = 'tool_call_count'",
            )
            .fetch_one(&pool)
            .await
            .expect("the tables read");
            let days: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM information_schema.tables \
                 WHERE table_schema = DATABASE() AND table_name = 'served_day'",
            )
            .fetch_one(&pool)
            .await
            .expect("the tables read");
            (columns, index, table, parts, counts, days)
        }
    };
    let before = migration_objects(pool).await;
    if before != (0, 0, 0, 0, 0, 0) {
        fail(
            "a store from before the nine migrations (no new column, no index, no session_key, \
             no request_part, no tool_call_count, no served_day)",
            &format!("{before:?}"),
        );
    }
    // The collection numbers its members by `number`, and three members hold one.
    let numbered: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM field_write WHERE `key` = 'number' AND value IN ('1', '2', '4')",
    )
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| fail("the numbers the members hold", &e.to_string()));
    if numbered != 3 {
        fail("three members holding a number", &numbered.to_string());
    }
    // Each fold left an event; the chain made at least two and the document one more.
    let merges: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT merge_no) FROM merge_event")
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| fail("the merge events", &e.to_string()));
    if merges < 4 {
        fail(
            "a merge event for each fold the recording made",
            &merges.to_string(),
        );
    }
    // The trail bot's run keeps a wording for each entry it wrote and one more for
    // the amended one.
    let writes: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_entry_write WHERE session IN \
         (SELECT s.id FROM session s JOIN entity e ON e.badge = s.bot \
          WHERE e.id = 'bot:upgrade-fixture-trail')",
    )
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| fail("the wordings of the trail's entries", &e.to_string()));
    if writes < 5 {
        fail(
            "a wording per entry and one for the amendment",
            &writes.to_string(),
        );
    }
    store.stop().await;

    // ── the current binary over it applies the nine migrations ──────────────
    let first = boot_current(&state_dir, store_port, &git_ref).await;
    first.stop().await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let after = migration_objects(store.pool()).await;
    if after != (4, 1, 1, 1, 1, 1) {
        fail(
            "the four columns, the index, the session_key table, the request_part table, the \
             tool_call_count table and the served_day table the boot adds",
            &format!("{after:?}"),
        );
    }
    store.stop().await;

    // ── read through the served surface ─────────────────────────────────────
    let second = boot_current_expecting(&state_dir, store_port, &git_ref, Some(0), &[]).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", second.http_port))
        .await
        .expect("connecting to the current binary");
    let say = |what: &'static str, read: String| -> serde_json::Value {
        serde_json::from_str(&read).unwrap_or_else(|_| fail(what, &read))
    };
    // The collection says the number its next member takes; the archived member
    // keeps its number, which still names it.
    let collection = say(
        "the numbered collection",
        surface
            .call(
                "recall",
                json!({"subject": "topic:upgrade-fixture-ordinances"}),
            )
            .await,
    );
    if collection["objects"][0]["next_number"] != 5 {
        fail(
            "the next number past the largest held (4)",
            &collection.to_string(),
        );
    }
    let archived = say(
        "the archived member by its number",
        surface
            .call(
                "recall",
                json!({"subject": "topic:upgrade-fixture-ordinances/4"}),
            )
            .await,
    );
    if archived["objects"][0]["id"] != "decision:upgrade-fixture-ordinance-signs" {
        fail("/4 naming the archived member", &archived.to_string());
    }
    // The view takes its input and lists the collection's members.
    let members = surface
        .call(
            "recall",
            json!({"view": "upgrade-fixture-members-of",
                   "inputs": {"collection": "topic:upgrade-fixture-ordinances"}}),
        )
        .await;
    for member in ["ordinance-curfew", "ordinance-parking"] {
        if !members.contains(member) {
            fail("the view listing a member", &members);
        }
    }
    // The chain of merges serves every claim under the last survivor, and the
    // first duplicate says it was folded.
    let survivor = say(
        "the survivor of the chain",
        surface
            .call(
                "recall",
                json!({"subject": "thing:upgrade-fixture-fold-c", "facts": true}),
            )
            .await,
    );
    for slug in ["fold-a", "fold-b", "fold-c"] {
        if !survivor
            .to_string()
            .contains(&format!("the claim filed under upgrade-fixture-{slug}"))
        {
            fail(
                "the survivor serving a claim of the chain",
                &survivor.to_string(),
            );
        }
    }
    let folded = say(
        "the first duplicate",
        surface
            .call("recall", json!({"subject": "thing:upgrade-fixture-fold-a"}))
            .await,
    );
    if folded["objects"][0]["status"] != "merged" {
        fail(
            "the first duplicate answering as merged",
            &folded.to_string(),
        );
    }
    // The document carries the section folded into it.
    let manual = say(
        "the document",
        surface
            .call(
                "read_document",
                json!({"document": "document:upgrade-fixture-manual"}),
            )
            .await,
    );
    let text = manual.to_string();
    if !text.contains("Switch the fryer on") || !text.contains("The last word.") {
        fail(
            "the document reading its own sections and the folded one",
            &text,
        );
    }
    // The trail: the run reopened by its code and wrapped again is the handover.
    let trail = say(
        "the trail bot's boot",
        surface
            .call(
                "start_here",
                json!({"bot": "upgrade-fixture-trail", "brief": true}),
            )
            .await,
    );
    if trail["session"]["handover"]["story"] != "walked the trail again and wrapped it" {
        fail(
            "the handover being the run's second closing entry",
            &trail["session"].to_string(),
        );
    }
    surface.finish().await;
    second.stop().await;

    // ── the calls that reading made are counted in the table the boot added ──
    // `recall`, `read_document` and `start_here` were each answered at least once
    // above, none of them under a session, so they are kept under no bot.
    let mut store = open_the_store(&state_dir, store_port).await;
    let counted: Vec<(String, i64)> = sqlx::query_as(
        "SELECT verb, CAST(SUM(calls) AS SIGNED) FROM tool_call_count \
         WHERE bot_key = '' AND outcome = 'landed' GROUP BY verb ORDER BY verb",
    )
    .fetch_all(store.pool())
    .await
    .unwrap_or_else(|e| fail("the counts of calls", &e.to_string()));
    for verb in ["read_document", "recall", "start_here"] {
        if !counted.iter().any(|(seen, n)| seen == verb && *n > 0) {
            fail("a call counted under its verb", &format!("{counted:?}"));
        }
    }
    store.stop().await;
    drop(scratch);
}

/// The closing story the recording's wrap told, and the label the run's focus
/// held at the close. Both are spelled in the recorder and stored in the
/// fixture, and nothing but the fixture declares them.
const RECORDED_CLOSING_STORY: &str = "closed the recording run after it seeded the store";
const RECORDED_CLOSING_FOCUS: &str = "claimed the upgrade-fixture-recorder role";

async fn the_rows_only_the_recording_build_writes_read_back(fixture_dir: &str, label: &str) {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(fixture_dir, label).await;
    let fail = |what: &str, body: &str| -> ! {
        panic!(
            "the fixture recorded at {git_ref} does not hold what only the deploy 4 build \
             writes: {what}: {body}"
        )
    };
    let role_holder = std::fs::read_to_string(format!("{fixture_dir}/role_holder.txt"))
        .unwrap_or_else(|e| panic!("reading {fixture_dir}/role_holder.txt: {e}"))
        .trim()
        .to_string();

    // ── in the recording, before any boot ───────────────────────────────────
    let mut store = open_the_store(&state_dir, store_port).await;
    let pool = store.pool();
    // A key written under `sets` wears the class `sets`.
    let classes: Vec<String> = sqlx::query_scalar(
        "SELECT write_class FROM field_write WHERE `key` = 'upgrade_fixture_setting'",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_else(|e| fail("the write class of a key", &e.to_string()));
    if classes != ["sets"] {
        fail(
            "a key written under sets in the class sets",
            &format!("{classes:?}"),
        );
    }
    // Every write carries the moment the store stamped at the append.
    let unstamped: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM field_write WHERE written_at IS NULL")
            .fetch_one(pool)
            .await
            .unwrap_or_else(|e| fail("the moment of a write", &e.to_string()));
    if unstamped != 0 {
        fail(
            "a moment on every write",
            &format!("{unstamped} writes have none"),
        );
    }
    // A wrapped run's closing entry is its own row, with the focus beside the story.
    // The recording wrapped two runs, so two rows carry the mark; the case holds
    // the first run's, whose handover a later boot reads.
    let closing: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT text, closing_focus FROM journal_entry WHERE closing = 1 ORDER BY ordinal",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_else(|e| fail("the closing mark of an entry", &e.to_string()));
    let recorded = (
        RECORDED_CLOSING_STORY.to_string(),
        Some(RECORDED_CLOSING_FOCUS.to_string()),
    );
    if !closing.contains(&recorded) {
        fail(
            "a closing entry with the run's focus beside the story",
            &format!("{closing:?}"),
        );
    }
    // A wrapped run that is its bot's newest holds its wrap window open.
    let open_windows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM session WHERE wrap_window IS NOT NULL")
            .fetch_one(pool)
            .await
            .unwrap_or_else(|e| fail("the wrap window of a run", &e.to_string()));
    if open_windows < 1 {
        fail("a wrap window held open", "no run holds one");
    }
    // The role is an object of its own, holding who has it.
    let role_kind: Option<String> =
        sqlx::query_scalar("SELECT kind FROM entity WHERE id = 'role:upgrade-fixture-holder'")
            .fetch_optional(pool)
            .await
            .unwrap_or_else(|e| fail("the role object", &e.to_string()));
    if role_kind.as_deref() != Some("role") {
        fail("a role object of its own", &format!("{role_kind:?}"));
    }
    let holder: Option<String> = sqlx::query_scalar(
        "SELECT value FROM field_write WHERE `key` = 'holder' AND entity = \
         (SELECT COALESCE(badge, id) FROM entity WHERE id = 'role:upgrade-fixture-holder') \
         ORDER BY ordinal DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .unwrap_or_else(|e| fail("the role object's holder", &e.to_string()));
    if holder.as_deref() != Some(role_holder.as_str()) {
        fail(
            "the role object holding the recorded run",
            &format!("{holder:?}"),
        );
    }
    store.stop().await;

    // ── the current binary over it, read through the served surface ─────────
    let first = boot_current(&state_dir, store_port, &git_ref).await;
    first.stop().await;
    let second = boot_current_expecting(&state_dir, store_port, &git_ref, Some(0), &[]).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", second.http_port))
        .await
        .expect("connecting to the current binary");
    let say = |what: &'static str, read: String| -> serde_json::Value {
        serde_json::from_str(&read).unwrap_or_else(|_| fail(what, &read))
    };
    // The key written under sets reaches its thing.
    let thing = say(
        "the thing a key was set on",
        surface
            .call(
                "recall",
                json!({"subject": "person:upgrade-fixture-person"}),
            )
            .await,
    );
    if thing["objects"][0]["fields"]["upgrade_fixture_setting"] != "kept-under-sets" {
        fail(
            "the key written under sets, served on its thing",
            &thing.to_string(),
        );
    }
    // **And the record that wrote it still serves it under `sets`, and an own key
    // on another record still serves under `fields`.** The thing's fold serves the
    // value whichever bag the reader puts it in, so the read above cannot tell the
    // two apart; the record can. A reader that put every key under `fields` would
    // pass the read above and fail here.
    let records = say(
        "the records on the thing",
        surface
            .call(
                "recall",
                json!({"subject": "person:upgrade-fixture-person", "facts": true}),
            )
            .await,
    );
    let facts = records["objects"][0]["facts"]
        .as_array()
        .unwrap_or_else(|| fail("the records behind the thing", &records.to_string()));
    let setter = facts
        .iter()
        .find(|fact| fact["content"] == "a property of the thing, set by this claim")
        .unwrap_or_else(|| fail("the record that set the key", &records.to_string()));
    if setter["sets"]["upgrade_fixture_setting"] != "kept-under-sets"
        || !setter["fields"]["upgrade_fixture_setting"].is_null()
    {
        fail(
            "the key under the record's sets and not under its fields",
            &setter.to_string(),
        );
    }
    let owner = facts
        .iter()
        .find(|fact| fact["content"] == "lives at the recorded place")
        .unwrap_or_else(|| fail("the record that holds an own key", &records.to_string()));
    if owner["fields"]["since"] != "2026-01-01" || !owner["sets"].is_null() {
        fail(
            "an own key under the record's fields and nothing under its sets",
            &owner.to_string(),
        );
    }
    // The role object reads back with its holder.
    let role = say(
        "the role object",
        surface
            .call("recall", json!({"subject": "role:upgrade-fixture-holder"}))
            .await,
    );
    if role["objects"][0]["fields"]["holder"] != role_holder.as_str()
        || role["objects"][0]["fields"]["claimed_at"]
            .as_str()
            .is_none()
    {
        fail(
            "the role object served with its holder and its moment",
            &role.to_string(),
        );
    }
    // The handover a later boot reads is the run's closing entry.
    let boot = say(
        "the boot of the shipped identity",
        surface
            .call("start_here", json!({"bot": "assistant", "brief": true}))
            .await,
    );
    if boot["session"]["handover"]["story"] != RECORDED_CLOSING_STORY {
        fail(
            "the handover served as the closing entry",
            &boot["session"]["handover"].to_string(),
        );
    }
    surface.finish().await;
    second.stop().await;
    drop(scratch);
}

async fn boots_on_a_store_filled_by(fixture_dir: &str, label: &str) {
    boots_on_a_store_recorded_earlier(fixture_dir, label, 0).await;
}

async fn boots_on_a_store_recorded_earlier(fixture_dir: &str, label: &str, earlier_hours: i64) {
    let restored = restore_a_fixture_recorded_earlier(fixture_dir, label, earlier_hours).await;
    boots_on_a_restored_store(fixture_dir, restored).await;
}

/// **Every boot-and-read-back assertion the gate makes over a restored store**,
/// for a store the caller has already restored, so a case that prepares the
/// store differently (an interrupted migration) is held to the same bar as a
/// clean boot and not to a copy of it.
async fn boots_on_a_restored_store(fixture_dir: &str, restored: Restored) {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        held_before,
        handles,
        holder_key,
    } = restored;

    // **The current binary boots over the restored store TWICE, and the second
    // boot changes nothing.** The first boot runs every migration and every
    // start pass the build has. A pass that is not idempotent, or a boot that
    // writes on every start, shows as a store that differs after the second
    // boot, and no read of the store would say so. The store is stopped and
    // dumped after each boot, before anything reads it, because a read through
    // the surface is itself attributed.
    let first = boot_current(&state_dir, store_port, &git_ref).await;
    first.stop().await;
    let after_first = dump_store(&state_dir).await;
    // The first boot reported the recording's handle; the passes are marked, so the
    // next boots report nothing.
    let second = boot_current_expecting(&state_dir, store_port, &git_ref, Some(0), &[]).await;
    second.stop().await;
    let after_second = dump_store(&state_dir).await;
    if after_first != after_second {
        let changed: Vec<String> = after_first
            .lines()
            .zip(after_second.lines())
            .filter(|(before, after)| before != after)
            .take(5)
            .map(|(before, after)| format!("- {before}\n+ {after}"))
            .collect();
        panic!(
            "the second boot of the current binary changed the store recorded at {git_ref} \
             (a boot that writes on every start, or a pass that is not idempotent). {} lines \
             before, {} after. First differences:\n{}",
            after_first.lines().count(),
            after_second.lines().count(),
            changed.join("\n")
        );
    }

    // The third boot is the one every read below goes through.
    let third = boot_current_expecting(&state_dir, store_port, &git_ref, Some(0), &[]).await;
    // **The boot measured the stored records and found none over the ceiling**, and
    // said so: a boot that logged nothing here would be a pass that did not run.
    assert!(
        third
            .log
            .iter()
            .any(|line| line.contains("stored records: none is over the ceiling")),
        "the boot of a store recorded at {git_ref} did not say it measured the stored records: {:?}",
        third.log
    );
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", third.http_port))
        .await
        .expect("connecting to the current binary");
    let role_holder_file = format!("{fixture_dir}/role_holder.txt");
    let role_holder = std::fs::read_to_string(&role_holder_file)
        .unwrap_or_else(|e| panic!("reading {role_holder_file}: {e}"))
        .trim()
        .to_string();
    // **Before the gate writes anything of its own**: the steps below write to
    // the store, and a thing that has been written to is meant to hold more. The
    // first two boots wrote nothing of the gate's, and the dump above shows the
    // second changed nothing the first left.
    assert_every_thing_holds_what_it_held(&surface, &git_ref, &held_before).await;
    assert_no_stored_record_is_over_the_ceiling(&surface, &git_ref, &handles).await;
    // **The recorded holder began before keys existed**, so the name it was
    // known by is exchanged once for a key, and every call below carries the key.
    // A recording from a build that keys its runs holds a key already, so the
    // holder is not exchanged and has no role claim in the old shape to carry.
    let keyed = holder_key.is_some();
    let role_key = match holder_key {
        Some(key) => key,
        None => exchange_the_recorded_run(&surface, &git_ref, &role_holder).await,
    };
    assert_every_recorded_record_reads_back(&surface, &git_ref, &role_holder).await;
    assert_what_a_real_store_holds_reads_back(&surface, &git_ref, &role_key).await;
    if !keyed {
        assert_a_role_claimed_in_the_old_shape_is_carried_by_the_boot_and_stays_held(
            &surface,
            &git_ref,
            &role_holder,
            &role_key,
        )
        .await;
    }
    assert_the_voucher_kind_arrives_and_no_voucher_exists(&surface, &git_ref).await;
    surface.finish().await;
    third.stop().await;

    // The role is now claim-shaped, with the claim the holder's write made. A
    // fourth boot is the restart of the deployed server over it.
    if !keyed {
        assert_a_claim_shaped_lease_renews_in_place(
            &state_dir,
            store_port,
            &git_ref,
            &role_holder,
            &role_key,
        )
        .await;
    }
    drop(scratch);
}

/// **A store the build before the switch filled keeps every key on the build
/// that switched.** Before the switch a key written as an own field wore the class
/// `own` and reached its thing. The switch keeps no migration: it reads that class
/// as it always did, and writes the new class `stays` only for a write made after
/// it. A boot that read `own` as a key to leave behind would empty every thing the
/// old build filled.
///
/// The recordings predate the class column, so a first boot adds it and gives each
/// row the class `legacy`. The case then rewrites those rows to `own`, which is
/// what the build before the switch wrote for an own field, and boots again.
#[tokio::test]
async fn rows_classed_own_before_the_switch_still_reach_their_things_after_it() {
    for (fixture_dir, label) in [
        (DEPLOY_3_FIXTURE_DIR, "own-class-deploy3"),
        (DEPLOY_4_FIXTURE_DIR, "own-class-deploy4"),
    ] {
        let Restored {
            scratch,
            state_dir,
            store_port,
            git_ref,
            held_before,
            ..
        } = restore_a_fixture(fixture_dir, label).await;

        boot_current(&state_dir, store_port, &git_ref)
            .await
            .stop()
            .await;
        let mut store = open_the_store(&state_dir, store_port).await;
        let rewritten = sqlx::query("UPDATE field_write SET write_class = 'own'")
            .execute(store.pool())
            .await
            .expect("the rows are classed as the build before the switch classed them")
            .rows_affected();
        assert!(
            rewritten as usize >= held_before.len(),
            "recorded at {git_ref}: {rewritten} rows were classed own and the store holds {} \
             values, so the case would leave some in another class",
            held_before.len()
        );
        store.stop().await;

        // The first boot reported the recording's handle and marked the pass, so
        // this one reports nothing.
        let booted = boot_current_expecting(&state_dir, store_port, &git_ref, Some(0), &[]).await;
        let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
            .await
            .expect("connecting to the current binary");
        assert_every_thing_holds_what_it_held(&surface, &git_ref, &held_before).await;
        surface.finish().await;
        booted.stop().await;

        let mut store = open_the_store(&state_dir, store_port).await;
        let classes: Vec<String> =
            sqlx::query_scalar("SELECT DISTINCT write_class FROM field_write ORDER BY 1")
                .fetch_all(store.pool())
                .await
                .expect("the classes read");
        assert_eq!(
            classes,
            ["own"],
            "recorded at {git_ref}: a boot rewrote the class of a row it did not write"
        );
        store.stop().await;
        drop(scratch);
    }
}

/// **A run an older build began holds no key, and its first call with its old
/// name is exchanged once for one.** The exchange answer is a refusal that
/// carries the key and runs nothing; the same name sent again is turned away
/// and hands out nothing; and the key opens the run. Returns the key.
async fn exchange_the_recorded_run(surface: &Surface, git_ref: &str, role_holder: &str) -> String {
    let fail = |what: &str, body: &str| -> ! { panic!("recorded at {git_ref}: {what}: {body}") };
    let first = surface
        .call(
            "journal",
            json!({"entry": "the first call with the old name", "sid": role_holder}),
        )
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&first).unwrap_or_else(|_| fail("the exchange answer", &first));
    let key = match (parsed["exchanged"] == true, parsed["sid"].as_str()) {
        (true, Some(key)) if key != role_holder => key.to_string(),
        _ => fail("the old name was not exchanged for a key", &first),
    };
    let again = surface
        .call(
            "journal",
            json!({"entry": "the old name again", "sid": role_holder}),
        )
        .await;
    if again.contains("\"exchanged\":true") || !again.contains("\"status\":\"blocked\"") {
        fail("a second bare name was not turned away", &again);
    }
    let opened = surface
        .call("list_runs", json!({"sid": key, "limit": 5}))
        .await;
    if opened.contains("\"status\":\"blocked\"") || !opened.contains(role_holder) {
        fail("the key did not open the recorded run", &opened);
    }
    key
}

/// **A run an older build began, that has since ended, is not exchanged by its
/// old name.** The recorded holder is a run with no key. Here its state is set to
/// the two ways a run ends, which an older build left in a store as a row with
/// no key, and the current binary boots over each. The old name is turned away,
/// hands out no key, and the boot says nothing the gate treats as a warning.
#[tokio::test]
async fn a_run_an_older_build_left_ended_is_not_exchanged_by_its_old_name_on_the_current_binary() {
    for (fixture_dir, deploy) in [(DEPLOY_3_FIXTURE_DIR, "d3"), (DEPLOY_4_FIXTURE_DIR, "d4")] {
        for ended in ["wrapped", "abandoned"] {
            let Restored {
                scratch,
                state_dir,
                store_port,
                git_ref,
                ..
            } = restore_a_fixture(fixture_dir, &format!("ended-{ended}-{deploy}")).await;
            let role_holder_file = format!("{fixture_dir}/role_holder.txt");
            let role_holder = std::fs::read_to_string(&role_holder_file)
                .unwrap_or_else(|e| panic!("reading {role_holder_file}: {e}"))
                .trim()
                .to_string();
            let mut store = open_the_store(&state_dir, store_port).await;
            let changed = sqlx::query("UPDATE session SET state = ? WHERE sid = ? OR id = ?")
                .bind(ended)
                .bind(&role_holder)
                .bind(&role_holder)
                .execute(store.pool())
                .await
                .expect("the recorded run is ended")
                .rows_affected();
            assert_eq!(
                changed, 1,
                "recorded at {git_ref}: the recorded holder is one row of the board"
            );
            store.stop().await;

            let booted = boot_current(&state_dir, store_port, &git_ref).await;
            let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
                .await
                .expect("connecting to the current binary");
            let answer = surface
                .call(
                    "journal",
                    json!({"entry": "the old name of an ended run", "sid": role_holder}),
                )
                .await;
            surface.finish().await;
            booted.stop().await;
            let parsed: serde_json::Value = serde_json::from_str(&answer).unwrap_or_else(|_| {
                panic!("recorded at {git_ref}: {ended}: the answer is not json: {answer}")
            });
            assert!(
                parsed["status"] == "blocked"
                    && parsed["exchanged"] != true
                    && parsed.get("sid").is_none(),
                "recorded at {git_ref}: a run that {ended} was exchanged by its old name: {answer}"
            );
            drop(scratch);
        }
    }
}

/// **The store of a restored fixture, brought up with no binary over it**, so
/// the gate can write rows the older build would have written, and read what the
/// current binary did to them.
async fn open_the_store(state_dir: &std::path::Path, store_port: u16) -> Dolt {
    Dolt::start(&state_dir.join("db"), store_port)
        .await
        .expect("the restored store comes up with no binary over it")
}

/// The one handle the legacy rows name, and the one that sends the legacy
/// message and owns the legacy run. Both are in the recording.
const LEGACY_MENTION: &str = "thing:upgrade-fixture-thing";
const LEGACY_BOT: &str = "bot:upgrade-fixture-lead";

/// **A message and a run written the way every row was before mentions and
/// columns held permanent ids**: the handle as typed, in the text and in the
/// sender and bot columns. Returns the run's id.
async fn plant_rows_an_older_build_wrote(pool: &sqlx::MySqlPool) -> String {
    sqlx::query(
        "INSERT INTO message (id, mailbox, ordinal, body, subject, sender, sent_at, state)
         VALUES ('lg0001', 'assistant', 9, ?, NULL, ?, '2026-10-08T14:00:00Z', 'new')",
    )
    .bind(format!("about @{LEGACY_MENTION}"))
    .bind(LEGACY_BOT)
    .execute(pool)
    .await
    .expect("the legacy message lands");
    let run: String = sqlx::query_scalar("SELECT id FROM session ORDER BY id LIMIT 1")
        .fetch_one(pool)
        .await
        .expect("the recording holds a run");
    sqlx::query("UPDATE session SET focus = CONCAT(focus, ?), bot = ? WHERE id = ?")
        .bind(format!(" about @{LEGACY_MENTION}"))
        .bind(LEGACY_BOT)
        .bind(&run)
        .execute(pool)
        .await
        .expect("the legacy run is written");
    run
}

/// What the three columns the passes rewrite hold now, as
/// `(message body, message sender, run focus, run bot)`.
async fn what_the_legacy_rows_hold(
    pool: &sqlx::MySqlPool,
    run: &str,
) -> (String, String, String, String) {
    let (body, sender): (String, String) =
        sqlx::query_as("SELECT body, sender FROM message WHERE id = 'lg0001'")
            .fetch_one(pool)
            .await
            .expect("the legacy message reads");
    let (focus, bot): (String, String) =
        sqlx::query_as("SELECT focus, bot FROM session WHERE id = ?")
            .bind(run)
            .fetch_one(pool)
            .await
            .expect("the legacy run reads");
    (body, sender, focus, bot)
}

/// **A message and a run an older build wrote with the handle as typed are
/// migrated by the first boot, and the next boot leaves them alone.** The first
/// boot rewrites the text and both columns onto permanent ids and marks each of
/// the four passes finished. The second boot reads none of those tables: a
/// handle written into the same rows afterwards stays as written, and the boot
/// says each pass is already done and logs no warning of any kind.
#[tokio::test]
async fn rows_an_older_build_wrote_are_migrated_once_and_the_next_boot_settles() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_the_fixture("legacy-rows").await;

    let mut store = open_the_store(&state_dir, store_port).await;
    let run = plant_rows_an_older_build_wrote(store.pool()).await;
    store.stop().await;

    // ── the first boot owes every pass, and does them ───────────────────────
    let first = boot_current(&state_dir, store_port, &git_ref).await;
    let migrated = first
        .log
        .iter()
        .filter(|line| line.contains("permanent ids: migrated"))
        .count();
    first.stop().await;
    assert_eq!(migrated, 4, "the first boot ran each of the four passes");

    let mut store = open_the_store(&state_dir, store_port).await;
    let (body, sender, focus, bot) = what_the_legacy_rows_hold(store.pool(), &run).await;
    assert!(
        body.contains("@#") && !body.contains(LEGACY_MENTION),
        "the message text is rewritten onto the permanent id: {body}"
    );
    assert!(
        focus.contains("@#") && !focus.contains(LEGACY_MENTION),
        "the run's focus is rewritten onto the permanent id: {focus}"
    );
    assert_ne!(
        sender, LEGACY_BOT,
        "the sender column holds the permanent id"
    );
    assert_ne!(bot, LEGACY_BOT, "the bot column holds the permanent id");
    let finished: Vec<String> = sqlx::query_scalar("SELECT name FROM store_pass ORDER BY name")
        .fetch_all(store.pool())
        .await
        .expect("the markers read");
    assert_eq!(
        finished,
        [
            "field value lowering",
            "forwarded field links repaired",
            "forwarding rows name the current handle",
            "mail mentions",
            "message sender column",
            "old role claims",
            "reference field lowering",
            "session bot column",
            "session mentions",
            "unreadable due day"
        ],
        "each pass that finished is marked"
    );

    // ── a handle written into the same rows afterwards is left as written,
    //    because the second boot reads none of those tables ──────────────────
    sqlx::query("UPDATE message SET body = CONCAT(body, ?) WHERE id = 'lg0001'")
        .bind(format!(" and @{LEGACY_MENTION}"))
        .execute(store.pool())
        .await
        .expect("the message is written again");
    sqlx::query("UPDATE session SET focus = CONCAT(focus, ?) WHERE id = ?")
        .bind(format!(" and @{LEGACY_MENTION}"))
        .bind(&run)
        .execute(store.pool())
        .await
        .expect("the run is written again");
    store.stop().await;

    let second = boot_current_expecting(&state_dir, store_port, &git_ref, Some(0), &[]).await;
    let done = second
        .log
        .iter()
        .filter(|line| line.contains("permanent ids: already done"))
        .count();
    let migrated_again = second
        .log
        .iter()
        .filter(|line| line.contains("permanent ids: migrated"))
        .count();
    second.stop().await;
    assert_eq!(done, 4, "the second boot says each pass is already done");
    assert_eq!(migrated_again, 0, "and runs none of them");

    let mut store = open_the_store(&state_dir, store_port).await;
    let (body, _, focus, _) = what_the_legacy_rows_hold(store.pool(), &run).await;
    assert!(
        body.ends_with(&format!(" and @{LEGACY_MENTION}")),
        "a settled pass read the message table: {body}"
    );
    assert!(
        focus.ends_with(&format!(" and @{LEGACY_MENTION}")),
        "a settled pass read the session table: {focus}"
    );
    store.stop().await;
    drop(scratch);
}

/// A thread the recorded build wrote, before any build held a `decision` kind.
const RECORDED_THREAD: &str = "thread:upgrade-fixture-line";

/// **Rows for a thread with one record and no child**, written below the surface
/// into the recorded schema, as a build that did not know decisions would have
/// left it.
async fn plant_a_thread_with_no_decisions(pool: &sqlx::MySqlPool) {
    sqlx::query(
        "INSERT INTO entity (id, kind, name, source, crm, parent, boot, prose, badge, merged_into,
                             archived_reason, archived_at)
         VALUES (?, 'thread', 'A Recorded Line', 'test', NULL, NULL, 'on-demand', '', 'line01',
                 NULL, NULL, NULL)",
    )
    .bind(RECORDED_THREAD)
    .execute(pool)
    .await
    .expect("the thread lands");
    sqlx::query(
        "INSERT INTO fact (entity, id, content, details, provenance, standing, status, recorded_at,
                           edge_shape, edge_object, event_kind, derived_from, derived_from_id,
                           inserted_at, stale_after, happened_at, happened_through)
         VALUES ('line01', 'f1', 'a finding the line gathered', NULL, 'testimony', 'settled',
                 'active', '2026-10-08', NULL, NULL, NULL, NULL, NULL,
                 '2026-10-08T14:02:10Z', NULL, NULL, NULL)",
    )
    .execute(pool)
    .await
    .expect("the thread's record lands");
}

/// **A store with threads and no decisions boots unchanged, and its first
/// decision then leads its thread.** The build reconciles the shipped kinds at
/// every start, so the kind needs no migration. Before a decision exists the
/// thread answers alone and carries none of the keys a decision adds; after one
/// is made under it, the same read puts the decision first.
#[tokio::test]
async fn a_store_with_threads_and_no_decisions_boots_unchanged_and_takes_its_first_decision() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(DEPLOY_3_FIXTURE_DIR, "thread-decisions").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    plant_a_thread_with_no_decisions(store.pool()).await;
    store.stop().await;

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not hold: {body}")
    };
    let parse = |what: &str, body: &str| -> serde_json::Value {
        serde_json::from_str(body).unwrap_or_else(|_| fail(what, body))
    };
    let handles = |answer: &serde_json::Value| -> Vec<String> {
        answer["objects"]
            .as_array()
            .map(|all| {
                all.iter()
                    .filter_map(|o| o["id"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };

    let before = parse(
        "the thread before any decision",
        &surface
            .call("recall", json!({"subject": RECORDED_THREAD}))
            .await,
    );
    if handles(&before) != [RECORDED_THREAD] {
        fail("the thread answering alone", &before.to_string());
    }
    for added in ["decisions_archived", "decisions_not_shown"] {
        if before.get(added).is_some() {
            fail(
                "the thread carrying nothing a decision adds",
                &before.to_string(),
            );
        }
    }

    let boot = parse(
        "booting to write",
        &surface
            .call(
                "start_here",
                json!({"bot": "assistant", "brief": true, "resume": "new"}),
            )
            .await,
    );
    let sid = boot["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| fail("booting to write", &boot.to_string()))
        .to_string();
    let made = surface
        .call(
            "add_entity",
            json!({"kind": "decision", "handle": "upgrade-fixture-choice", "name": "A choice",
                   "source": "user-named", "parent": RECORDED_THREAD, "sid": sid}),
        )
        .await;
    if made.contains("\"status\":\"blocked\"") || made.contains("no kind is named") {
        fail("making a decision under the recorded thread", &made);
    }
    let said = surface
        .call(
            "capture",
            json!({"subject": "decision:upgrade-fixture-choice", "content": "the line goes on",
                   "provenance": "testimony", "sid": sid}),
        )
        .await;
    if said.contains("\"status\":\"blocked\"") {
        fail("writing the decision", &said);
    }
    let after = parse(
        "the thread after its first decision",
        &surface
            .call("recall", json!({"subject": RECORDED_THREAD}))
            .await,
    );
    booted.stop().await;
    assert_eq!(
        handles(&after),
        ["decision:upgrade-fixture-choice", RECORDED_THREAD],
        "recorded at {git_ref}: the decision leads the thread it was made under: {after}"
    );
    drop(scratch);
}

/// A thread a build wrote before any thread had a ceiling.
const CROWDED_THREAD: &str = "thread:upgrade-fixture-crowded";

/// **Rows for a thread with five active records**, written below the surface into
/// the recorded schema, as a build that let a thread grow without a limit left it.
async fn plant_a_thread_over_the_default_ceiling(pool: &sqlx::MySqlPool) {
    sqlx::query(
        "INSERT INTO entity (id, kind, name, source, crm, parent, boot, prose, badge, merged_into,
                             archived_reason, archived_at)
         VALUES (?, 'thread', 'A Crowded Line', 'test', NULL, NULL, 'on-demand', '', 'crowd1',
                 NULL, NULL, NULL)",
    )
    .bind(CROWDED_THREAD)
    .execute(pool)
    .await
    .expect("the thread lands");
    for n in 1..=5 {
        sqlx::query(
            "INSERT INTO fact (entity, id, content, details, provenance, standing, status,
                               recorded_at, edge_shape, edge_object, event_kind, derived_from,
                               derived_from_id, inserted_at, stale_after, happened_at,
                               happened_through)
             VALUES ('crowd1', ?, ?, NULL, 'inference', 'open', 'active', '2026-10-08', NULL,
                     NULL, NULL, NULL, NULL, '2026-10-08T14:02:10Z', NULL, NULL, NULL)",
        )
        .bind(format!("f{n}"))
        .bind(format!("crowded record number {n}"))
        .execute(pool)
        .await
        .expect("the thread's record lands");
        sqlx::query(
            "INSERT INTO fact_write (entity, fact_id, ordinal, content, details, provenance,
                                    standing, status, recorded_at, edge_shape, edge_object,
                                    derived_from, derived_from_id, inserted_at, stale_after,
                                    written_at, happened_at, happened_through, session)
             VALUES ('crowd1', ?, 1, ?, NULL, 'inference', 'open', 'active', '2026-10-08', NULL,
                     NULL, NULL, NULL, '2026-10-08T14:02:10Z', NULL, '2026-10-08T14:02:10Z',
                     NULL, NULL, NULL)",
        )
        .bind(format!("f{n}"))
        .bind(format!("crowded record number {n}"))
        .execute(pool)
        .await
        .expect("the record's first write lands");
    }
}

/// **A store whose threads already hold more than three records boots cleanly and
/// reads whole.** The default ceiling needs no migration and archives nothing:
/// the thread is read and edited as it was, a capture onto it is refused with the
/// routes, and it takes records again once archiving brings it back under.
#[tokio::test]
async fn a_thread_a_deployed_build_left_over_the_default_ceiling_reads_and_edits_as_before() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(DEPLOY_6_FIXTURE_DIR, "thread-over-ceiling").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    plant_a_thread_over_the_default_ceiling(store.pool()).await;
    store.stop().await;

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not hold: {body}")
    };
    let parse = |what: &str, body: &str| -> serde_json::Value {
        serde_json::from_str(body).unwrap_or_else(|_| fail(what, body))
    };

    let boot = parse(
        "booting to write",
        &surface
            .call(
                "start_here",
                json!({"bot": "assistant", "brief": true, "resume": "new"}),
            )
            .await,
    );
    let sid = boot["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| fail("booting to write", &boot.to_string()))
        .to_string();

    // ── it reads whole ──────────────────────────────────────────────────────
    let read = surface
        .call(
            "recall",
            json!({"subject": CROWDED_THREAD, "facts": true, "status": "active", "sid": sid}),
        )
        .await;
    for n in 1..=5 {
        if !read.contains(&format!("crowded record number {n}")) {
            fail("the thread reading all five records", &read);
        }
    }

    // ── it edits as before ──────────────────────────────────────────────────
    let edited = surface
        .call(
            "update_fact",
            json!({"address": format!("{CROWDED_THREAD}#f1"),
                   "content": "crowded record number 1, reworded", "provenance": "inference",
                   "sid": sid}),
        )
        .await;
    if edited.contains("\"status\":\"blocked\"") {
        fail("editing a record over the ceiling", &edited);
    }

    // ── a capture is refused with the routes, and writes nothing ────────────
    let refused = parse(
        "a capture onto the crowded thread",
        &surface
            .call(
                "capture",
                json!({"subject": CROWDED_THREAD, "content": "one more record", "sid": sid}),
            )
            .await,
    );
    if refused["status"] != "blocked" || refused["live"] != 5 || refused["routes"].is_null() {
        fail(
            "the capture refused with its count and routes",
            &refused.to_string(),
        );
    }

    // ── archiving back under the ceiling lets it take a record again ────────
    for n in 1..=3 {
        let archived = surface
            .call(
                "update_fact",
                json!({"address": format!("{CROWDED_THREAD}#f{n}"), "status": "archived",
                       "details": "no longer earns its slot", "sid": sid}),
            )
            .await;
        if archived.contains("\"status\":\"blocked\"") {
            fail("archiving a record over the ceiling", &archived);
        }
    }
    let landed = surface
        .call(
            "capture",
            json!({"subject": CROWDED_THREAD, "content": "one more record", "sid": sid}),
        )
        .await;
    booted.stop().await;
    if landed.contains("\"status\":\"blocked\"") {
        fail("a capture once the thread is back under", &landed);
    }
    drop(scratch);
}

/// The loop an older build left behind: a schedule that cannot be read, stored the
/// way every such loop was before a write stamped the day it became late, so it
/// holds no due day at all.
const UNREADABLE_LOOP: &str = "thing:upgrade-fixture-fern";

/// **Rows for a thing whose cadence is not a number**, written below the surface
/// into the recorded schema. The badge, the record and its three fields are the
/// whole of a thing the way the recording holds one; nothing here is a due day.
async fn plant_an_unreadable_loop(pool: &sqlx::MySqlPool) {
    sqlx::query(
        "INSERT INTO entity (id, kind, name, source, crm, parent, boot, prose, badge, merged_into,
                             archived_reason, archived_at)
         VALUES (?, 'thing', 'A Recorded Fern', 'test', NULL, NULL, 'on-demand', '', 'fern01',
                 NULL, NULL, NULL)",
    )
    .bind(UNREADABLE_LOOP)
    .execute(pool)
    .await
    .expect("the loop's thing lands");
    sqlx::query(
        "INSERT INTO fact (entity, id, content, details, provenance, standing, status, recorded_at,
                           edge_shape, edge_object, event_kind, derived_from, derived_from_id,
                           inserted_at, stale_after, happened_at, happened_through)
         VALUES ('fern01', 'f1', 'watered on a schedule nobody wrote down properly', NULL,
                 'testimony', 'settled', 'active', '2026-10-08', NULL, NULL, NULL, NULL, NULL,
                 '2026-10-08T14:02:10Z', NULL, NULL, NULL)",
    )
    .execute(pool)
    .await
    .expect("the loop's record lands");
    for (key, value) in [
        ("cadence_days", "often"),
        ("advances_from", "due_date"),
        ("counts_from", "2026-06-01"),
    ] {
        sqlx::query(
            "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id)
             VALUES ('fern01', ?, 1, ?, 'f1')",
        )
        .bind(key)
        .bind(value)
        .execute(pool)
        .await
        .expect("the loop's field lands");
    }
}

/// **A loop an older build left unreadable, with no due day, is given one by the
/// first boot, found by the owed read exactly as it is taught, and left alone by
/// the next boot.** Nothing in the recording holds such a loop, so the case
/// writes one below the surface the way the older build would have left it. The
/// first boot stamps it and says so; the owed read returns it with the reason it
/// cannot be read; the second boot says the pass is already done.
#[tokio::test]
async fn an_unreadable_loop_an_older_build_left_is_stamped_by_the_first_boot_and_found() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(DEPLOY_3_FIXTURE_DIR, "unreadable-loop").await;

    let mut store = open_the_store(&state_dir, store_port).await;
    plant_an_unreadable_loop(store.pool()).await;
    store.stop().await;

    let first = boot_current(&state_dir, store_port, &git_ref).await;
    let stamped = first
        .log
        .iter()
        .filter(|line| line.contains("due days: stamped what could not be read"))
        .count();
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", first.http_port))
        .await
        .expect("connecting to the current binary");
    let owed = surface
        .call(
            "recall",
            json!({"fields": [{"key": "due_on"}], "overdue": {}}),
        )
        .await;
    first.stop().await;
    assert_eq!(stamped, 1, "the first boot stamped the loop and said so");
    let owed: serde_json::Value = serde_json::from_str(&owed)
        .unwrap_or_else(|_| panic!("recorded at {git_ref}: the owed read did not answer: {owed}"));
    let fern = owed["objects"]
        .as_array()
        .and_then(|all| all.iter().find(|o| o["id"] == UNREADABLE_LOOP))
        .unwrap_or_else(|| {
            panic!("recorded at {git_ref}: the unreadable loop is not owed to the read: {owed}")
        });
    assert!(
        fern["unreadable_because"]
            .to_string()
            .contains("cadence_days"),
        "and says which key it cannot be read from: {fern}"
    );

    let second = boot_current(&state_dir, store_port, &git_ref).await;
    let settled = second
        .log
        .iter()
        .any(|line| line.contains("due days: already stamped"));
    second.stop().await;
    assert!(settled, "the second boot says the pass is already done");
    drop(scratch);
}

const FORWARDED_HOLDER: &str = "thing:upgrade-fixture-holder";
const FORWARDED_FOLDED: &str = "thing:upgrade-fixture-husk";
const FORWARDED_SURVIVOR: &str = "thing:upgrade-fixture-heir";

/// **A field that names a folded thing, stored the way a build that did not
/// sweep field links left it**: the holder's value is the folded thing's id
/// behind the mark, the folded row forwards, and the link table has no row for
/// it. Written below the surface into the recorded schema.
async fn plant_a_link_on_a_forwarding_row(pool: &sqlx::MySqlPool) {
    for (handle, badge, name, merged_into) in [
        (FORWARDED_HOLDER, "hold01", "A Recorded Holder", None),
        (
            FORWARDED_FOLDED,
            "husk01",
            "A Recorded Husk",
            Some(FORWARDED_SURVIVOR),
        ),
        (FORWARDED_SURVIVOR, "heir01", "A Recorded Heir", None),
    ] {
        sqlx::query(
            "INSERT INTO entity (id, kind, name, source, crm, parent, boot, prose, badge, merged_into,
                                 archived_reason, archived_at)
             VALUES (?, 'thing', ?, 'test', NULL, NULL, 'on-demand', '', ?, ?, NULL, NULL)",
        )
        .bind(handle)
        .bind(name)
        .bind(badge)
        .bind(merged_into)
        .execute(pool)
        .await
        .expect("the entity lands");
    }
    sqlx::query(
        "INSERT INTO fact (entity, id, content, details, provenance, standing, status, recorded_at,
                           edge_shape, edge_object, event_kind, derived_from, derived_from_id,
                           inserted_at, stale_after, happened_at, happened_through)
         VALUES ('hold01', 'f1', 'points at a thing that was folded away', NULL,
                 'testimony', 'settled', 'active', '2026-10-08', NULL, NULL, NULL, NULL, NULL,
                 '2026-10-08T14:02:10Z', NULL, NULL, NULL)",
    )
    .execute(pool)
    .await
    .expect("the holder's record lands");
    sqlx::query(
        "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id)
         VALUES ('hold01', 'upgrade_fixture_forwarded', 1, ?, 'f1')",
    )
    .bind(jojobot_domain::memory::marked_stored_handle("husk01"))
    .execute(pool)
    .await
    .expect("the holder's field lands");
}

/// **A link an older fold left on a forwarding row is repointed by the first
/// boot at the thing it ended in, found by the served read, and left alone by
/// the next boot.** The link table has no row for the planted value, so this also
/// holds the order of the two passes: the pass that fills the table runs first.
#[tokio::test]
async fn a_link_an_older_fold_left_on_a_forwarding_row_is_repointed_by_the_first_boot() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(DEPLOY_3_FIXTURE_DIR, "forwarded-link").await;

    let mut store = open_the_store(&state_dir, store_port).await;
    plant_a_link_on_a_forwarding_row(store.pool()).await;
    store.stop().await;

    let first = boot_current(&state_dir, store_port, &git_ref).await;
    let repointed = first
        .log
        .iter()
        .filter(|line| line.contains("repointed the field links that named a folded thing"))
        .count();
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", first.http_port))
        .await
        .expect("connecting to the current binary");
    let read = surface
        .call("recall", json!({"subject": FORWARDED_HOLDER}))
        .await;
    first.stop().await;
    assert_eq!(
        repointed, 1,
        "the first boot repointed the one link and said so"
    );
    let read: serde_json::Value = serde_json::from_str(&read)
        .unwrap_or_else(|_| panic!("recorded at {git_ref}: the read did not answer: {read}"));
    assert_eq!(
        read["objects"][0]["fields"]["upgrade_fixture_forwarded"], FORWARDED_SURVIVOR,
        "recorded at {git_ref}: the holder's field still names the folded thing: {read}"
    );

    let second = boot_current(&state_dir, store_port, &git_ref).await;
    let settled = second
        .log
        .iter()
        .any(|line| line.contains("links on forwarding rows: already done"));
    second.stop().await;
    assert!(settled, "the second boot says the pass is already done");
    drop(scratch);
}

/// The fold an older build left, and the survivor it renamed afterwards.
const LAGGED_FOLDED: &str = "thing:upgrade-fixture-shell";
const LAGGED_SURVIVOR_THEN: &str = "thing:upgrade-fixture-earlier";
const LAGGED_SURVIVOR_NOW: &str = "thing:upgrade-fixture-later";

/// **A fold followed by a rename of its survivor, stored the way a build that
/// renamed only the survivor's own row left it**: the survivor wears its new
/// handle and the old one is in the rename history, while the forwarding row
/// still names the handle the survivor wore at the fold. Written below the
/// surface into the recorded schema.
async fn plant_a_fold_whose_survivor_was_renamed(pool: &sqlx::MySqlPool) {
    for (handle, badge, name, merged_into) in [
        (
            LAGGED_FOLDED,
            "shel01",
            "A Recorded Shell",
            Some(LAGGED_SURVIVOR_THEN),
        ),
        (LAGGED_SURVIVOR_NOW, "late01", "A Recorded Latecomer", None),
    ] {
        sqlx::query(
            "INSERT INTO entity (id, kind, name, source, crm, parent, boot, prose, badge, merged_into,
                                 archived_reason, archived_at)
             VALUES (?, 'thing', ?, 'test', NULL, NULL, 'on-demand', '', ?, ?, NULL, NULL)",
        )
        .bind(handle)
        .bind(name)
        .bind(badge)
        .bind(merged_into)
        .execute(pool)
        .await
        .expect("the entity lands");
    }
    sqlx::query(
        "INSERT INTO entity_former_handle (former_handle, badge, changed_at, ordinal)
         VALUES (?, 'late01', '2026-10-08', 1)",
    )
    .bind(LAGGED_SURVIVOR_THEN)
    .execute(pool)
    .await
    .expect("the rename history lands");
}

/// **A forwarding row an older build left naming its survivor's former handle
/// is brought to the current one by the first boot, served so by the read, and
/// left alone by the next boot.** Held over both recorded stores.
#[tokio::test]
async fn a_forwarding_row_naming_a_renamed_survivor_is_brought_current_by_the_first_boot() {
    for (fixture_dir, label) in [
        (DEPLOY_3_FIXTURE_DIR, "lagged-forward-deploy3"),
        (DEPLOY_4_FIXTURE_DIR, "lagged-forward-deploy4"),
    ] {
        let Restored {
            scratch,
            state_dir,
            store_port,
            git_ref,
            ..
        } = restore_a_fixture(fixture_dir, label).await;

        let mut store = open_the_store(&state_dir, store_port).await;
        plant_a_fold_whose_survivor_was_renamed(store.pool()).await;
        store.stop().await;

        let first = boot_current(&state_dir, store_port, &git_ref).await;
        let brought = first
            .log
            .iter()
            .filter(|line| line.contains("forwarding rows brought to the handle their survivor"))
            .count();
        let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", first.http_port))
            .await
            .expect("connecting to the current binary");
        let read = surface
            .call("recall", json!({"subject": LAGGED_FOLDED}))
            .await;
        first.stop().await;
        assert_eq!(
            brought, 1,
            "recorded at {git_ref}: the first boot brought the one row current and said so"
        );
        let read: serde_json::Value = serde_json::from_str(&read)
            .unwrap_or_else(|_| panic!("recorded at {git_ref}: the read did not answer: {read}"));
        assert_eq!(
            read["objects"][0]["merged_into"], LAGGED_SURVIVOR_NOW,
            "recorded at {git_ref}: the folded thing still names the old handle: {read}"
        );

        let second = boot_current(&state_dir, store_port, &git_ref).await;
        let settled = second
            .log
            .iter()
            .any(|line| line.contains("forwarding rows name their survivor: already done"));
        second.stop().await;
        assert!(
            settled,
            "recorded at {git_ref}: the second boot says the pass is already done"
        );
        drop(scratch);
    }
}

/// The view an older build stored, and the two things it asks about.
const OLD_VIEW: &str = "view:upgrade-fixture-view";
const OLD_VIEW_RED: &str = "thing:upgrade-fixture-red";
const OLD_VIEW_BLUE: &str = "thing:upgrade-fixture-blue";

/// **A view the way a build before the expression stored it**: a record on the
/// view whose own fields name a key and a value, and no `where` anywhere. Written
/// below the surface into the recorded schema, with the two things it separates.
async fn plant_a_view_with_a_filter_record(pool: &sqlx::MySqlPool) {
    for (id, kind, name, badge) in [
        (OLD_VIEW, "view", "A Recorded View", "view01"),
        (OLD_VIEW_RED, "thing", "A Red Thing", "red001"),
        (OLD_VIEW_BLUE, "thing", "A Blue Thing", "blue01"),
    ] {
        sqlx::query(
            "INSERT INTO entity (id, kind, name, source, crm, parent, boot, prose, badge,
                                 merged_into, archived_reason, archived_at)
             VALUES (?, ?, ?, 'test', NULL, NULL, 'on-demand', '', ?, NULL, NULL, NULL)",
        )
        .bind(id)
        .bind(kind)
        .bind(name)
        .bind(badge)
        .execute(pool)
        .await
        .expect("the entity lands");
        for fact in ["f1", "f2"] {
            if fact == "f2" && kind != "view" {
                continue;
            }
            sqlx::query(
                "INSERT INTO fact (entity, id, content, details, provenance, standing, status,
                                   recorded_at, edge_shape, edge_object, event_kind, derived_from,
                                   derived_from_id, inserted_at, stale_after, happened_at,
                                   happened_through)
                 VALUES (?, ?, 'a recorded claim', NULL, 'testimony', 'settled', 'active',
                         '2026-10-08', NULL, NULL, NULL, NULL, NULL, '2026-10-08T14:02:10Z', NULL,
                         NULL, NULL)",
            )
            .bind(badge)
            .bind(fact)
            .execute(pool)
            .await
            .expect("the record lands");
            sqlx::query(
                "INSERT INTO fact_write (entity, fact_id, ordinal, content, details, provenance,
                                        standing, status, recorded_at, edge_shape, edge_object,
                                        derived_from, derived_from_id, inserted_at, stale_after,
                                        written_at, happened_at, happened_through, session)
                 VALUES (?, ?, 1, 'a recorded claim', NULL, 'testimony', 'settled', 'active',
                         '2026-10-08', NULL, NULL, NULL, NULL, '2026-10-08T14:02:10Z', NULL,
                         '2026-10-08T14:02:10Z', NULL, NULL, NULL)",
            )
            .bind(badge)
            .bind(fact)
            .execute(pool)
            .await
            .expect("the record's first write lands");
        }
    }
    for (badge, key, value, fact) in [
        ("view01", "selects", "thing", "f1"),
        ("view01", "key", "color", "f2"),
        ("view01", "value", "red", "f2"),
        ("red001", "color", "red", "f1"),
        ("blue01", "color", "blue", "f1"),
    ] {
        sqlx::query(
            "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id)
             VALUES (?, ?, 1, ?, ?)",
        )
        .bind(badge)
        .bind(key)
        .bind(value)
        .bind(fact)
        .execute(pool)
        .await
        .expect("the field lands");
    }
}

/// **A view stored before the expression existed answers as it did.** Its
/// filter is a record whose own fields name a key and a value, and the current
/// binary reads it with no `where` on the view: the red thing is in, the blue one
/// is out, and a second boot over the same store answers the same.
#[tokio::test]
async fn a_view_stored_with_a_filter_record_answers_as_before_on_the_current_binary() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(DEPLOY_3_FIXTURE_DIR, "old-view").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    plant_a_view_with_a_filter_record(store.pool()).await;
    store.stop().await;

    for boot in 0..2 {
        let booted = boot_current(&state_dir, store_port, &git_ref).await;
        let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
            .await
            .expect("connecting to the current binary");
        let answer = surface
            .call("recall", json!({"view": "upgrade-fixture-view"}))
            .await;
        booted.stop().await;
        let answer: serde_json::Value = serde_json::from_str(&answer).unwrap_or_else(|_| {
            panic!("recorded at {git_ref}: boot {boot}: the view did not answer: {answer}")
        });
        let handles: Vec<&str> = answer["objects"]
            .as_array()
            .map(|all| all.iter().filter_map(|o| o["id"].as_str()).collect())
            .unwrap_or_default();
        assert_eq!(
            handles,
            [OLD_VIEW_RED],
            "recorded at {git_ref}: boot {boot}: the stored filter record still separates red from \
             blue: {answer}"
        );
    }
    drop(scratch);
}

/// **Three claims an older build wrote, each naming a thing by its stored mark**:
/// in plain words, inside an inline code span and inside a fenced block. Before
/// text in code was literal (rule 450), a handle written in a span was resolved
/// like any other, so the row holds the mark there.
///
/// **The column shape is copied, not guessed.** Each insert uses the column list
/// of the `fact` and `fact_write` rows in the recordings (for instance entity
/// `gwd5wp`, claim `f3`, in the `upgrade` dump) and the values
/// `plant_a_view_with_a_filter_record` gives a claim. **Only the content is new.**
/// Neither recording holds a mark in a claim's words, so these three rows are an
/// invention of the shape; the recordings hold the mark only as a field value.
/// The host is the blue thing and the mark is the red thing's badge.
async fn plant_claims_naming_a_thing_by_its_stored_mark(pool: &sqlx::MySqlPool) {
    for (fact, content) in [
        ("f3", "see @#red001 for the plain case"),
        ("f4", "the form is `@#red001` in a span"),
        ("f5", "the form in a block:\n```\n@#red001\n```\nends"),
    ] {
        sqlx::query(
            "INSERT INTO fact (entity, id, content, details, provenance, standing, status,
                               recorded_at, edge_shape, edge_object, event_kind, derived_from,
                               derived_from_id, inserted_at, stale_after, happened_at,
                               happened_through)
             VALUES ('blue01', ?, ?, NULL, 'testimony', 'settled', 'active',
                     '2026-10-08', NULL, NULL, NULL, NULL, NULL, '2026-10-08T14:02:10Z', NULL,
                     NULL, NULL)",
        )
        .bind(fact)
        .bind(content)
        .execute(pool)
        .await
        .expect("the claim lands");
        sqlx::query(
            "INSERT INTO fact_write (entity, fact_id, ordinal, content, details, provenance,
                                    standing, status, recorded_at, edge_shape, edge_object,
                                    derived_from, derived_from_id, inserted_at, stale_after,
                                    written_at, happened_at, happened_through, session)
             VALUES ('blue01', ?, 1, ?, NULL, 'testimony', 'settled', 'active',
                     '2026-10-08', NULL, NULL, NULL, NULL, '2026-10-08T14:02:10Z', NULL,
                     '2026-10-08T14:02:10Z', NULL, NULL, NULL)",
        )
        .bind(fact)
        .bind(content)
        .execute(pool)
        .await
        .expect("the claim's first write lands");
    }
}

/// **A stored mark inside code reads as the handle it answers to, as one outside
/// code does.** The mark is jojobot's own, so the reader resolves it wherever it
/// sits (rule 450 makes a WRITTEN handle in code literal, and nothing rewrites
/// what an older build stored). The boot holds to the gate's bar: no warning, no
/// error line. The plain claim is the positive the two in code depend on.
#[tokio::test]
async fn a_stored_mark_inside_code_reads_as_the_handle_on_the_current_binary() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(DEPLOY_3_FIXTURE_DIR, "mark-in-code").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    plant_a_view_with_a_filter_record(store.pool()).await;
    plant_claims_naming_a_thing_by_its_stored_mark(store.pool()).await;
    store.stop().await;

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let answer = surface
        .call("recall", json!({"subject": OLD_VIEW_BLUE, "facts": true}))
        .await;
    booted.stop().await;
    let answer: serde_json::Value = serde_json::from_str(&answer)
        .unwrap_or_else(|_| panic!("recorded at {git_ref}: the thing did not answer: {answer}"));
    let content_of = |fact: &str| -> String {
        answer["objects"][0]["facts"]
            .as_array()
            .and_then(|all| {
                all.iter().find(|claim| {
                    claim["address"]
                        .as_str()
                        .is_some_and(|address| address.ends_with(&format!("#{fact}")))
                })
            })
            .and_then(|claim| claim["content"].as_str())
            .unwrap_or_else(|| panic!("recorded at {git_ref}: no claim {fact}: {answer}"))
            .to_string()
    };
    for (fact, shape) in [("f3", ""), ("f4", "`"), ("f5", "```")] {
        let content = content_of(fact);
        assert!(
            content.contains(OLD_VIEW_RED),
            "recorded at {git_ref}: claim {fact} reads the mark as the handle: {content}"
        );
        assert!(
            !content.contains("red001"),
            "recorded at {git_ref}: claim {fact} still shows the badge: {content}"
        );
        assert!(
            content.contains(shape),
            "recorded at {git_ref}: claim {fact} keeps its code around the handle: {content}"
        );
    }
    drop(scratch);
}

/// **Every stored record that no answer could carry whole**, found by reading
/// each thing's records back through the served surface the way a caller does,
/// one part after another. A record is over when the answer that carries it is
/// past the ceiling: a read serves a record whole or not at all, so the answer
/// is what shows it. Each is `(handle, offset of the part, characters served)`.
async fn records_over_the_ceiling(
    surface: &Surface,
    handles: &[String],
) -> Vec<(String, usize, usize)> {
    let mut over = Vec::new();
    for handle in handles {
        let mut offset = 0usize;
        loop {
            let raw = surface
                .call(
                    "recall",
                    json!({"subject": handle, "facts": true, "details": true, "offset": offset}),
                )
                .await;
            let size = raw.chars().count();
            if size > jojobot_domain::text::ANSWER_CEILING {
                over.push((handle.clone(), offset, size));
            }
            let next = serde_json::from_str::<serde_json::Value>(&raw)
                .ok()
                .and_then(|body| body["not_shown"]["offset"].as_u64());
            match next {
                Some(next) if next as usize > offset => offset = next as usize,
                _ => break,
            }
        }
    }
    over
}

/// **A store an older build filled holds no record over the ceiling.** The write
/// guard keeps new ones out; this finds the ones already held. A non-empty list
/// is not repaired here and not ignored: it goes to the PM before anything else,
/// who decides between a pointer to a page and a bounded continuation for those
/// records only.
async fn assert_no_stored_record_is_over_the_ceiling(
    surface: &Surface,
    git_ref: &str,
    handles: &[String],
) {
    assert!(
        !handles.is_empty(),
        "the recording at {git_ref} holds no entity, so this scan reads nothing"
    );
    let over = records_over_the_ceiling(surface, handles).await;
    assert!(
        over.is_empty(),
        "the store recorded at {git_ref} holds records no answer can carry whole: {over:?}. \
         Send this list to the PM before anything else; do not shorten them here"
    );
}

/// **The scan finds a record stored over the ceiling, and names only that one.**
/// One stored record is made larger than any answer straight in the store, which
/// is the only way left to make one; the scan over the served surface lists it
/// and nothing else. Without this the case above passes on a scan that reads
/// nothing.
#[tokio::test]
async fn the_scan_for_oversized_records_finds_one_stored_over_the_ceiling() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        held_before: _,
        handles,
        holder_key: _,
    } = restore_the_fixture("oversized").await;
    let mut editing = Dolt::start(&state_dir.join("db"), free_port())
        .await
        .expect("the stopped store comes up to be made oversized");
    let (badge, local, handle): (String, String, String) = sqlx::query_as(
        "SELECT f.entity, f.id, e.id FROM fact f JOIN entity e ON e.badge = f.entity
         WHERE f.status = 'active'
           AND NOT EXISTS (SELECT 1 FROM field_write w
                           WHERE w.entity = f.entity AND w.fact_id = f.id
                             AND w.`key` LIKE 'role/%')
         ORDER BY e.id, f.id LIMIT 1",
    )
    .fetch_one(editing.pool())
    .await
    .expect("the recording holds an active record");
    // **Both tables the record lives in**: reads project the newest write of a
    // claim, and the claim's own row sits beside it.
    let huge = "x".repeat(jojobot_domain::text::ANSWER_CEILING + 2_000);
    for update in [
        "UPDATE fact SET details = ? WHERE entity = ? AND id = ?",
        "UPDATE fact_write SET details = ? WHERE entity = ? AND fact_id = ?",
    ] {
        sqlx::query(update)
            .bind(&huge)
            .bind(&badge)
            .bind(&local)
            .execute(editing.pool())
            .await
            .expect("one stored record is made larger than any answer");
    }
    editing.stop().await;

    let booted = boot_current_allowing(&state_dir, store_port, &git_ref, true).await;
    // **The boot names the record**: its address, which carries the thing's handle,
    // once, as a warning.
    let named: Vec<&String> = booted
        .log
        .iter()
        .filter(|line| line.contains("STORED RECORD OVER THE CEILING"))
        .collect();
    assert_eq!(named.len(), 1, "one record, one warning: {:?}", booted.log);
    assert!(
        named[0].contains(&format!("{handle}#{local}")),
        "the warning names the record's address {handle}#{local}: {}",
        named[0]
    );
    assert!(
        !booted
            .log
            .iter()
            .any(|line| line.contains("none is over the ceiling")),
        "a store that holds one does not say it holds none: {:?}",
        booted.log
    );
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let over = records_over_the_ceiling(&surface, &handles).await;
    surface.finish().await;
    booted.stop().await;
    drop(scratch);

    assert_eq!(
        over.iter().map(|(h, ..)| h.as_str()).collect::<Vec<_>>(),
        [handle.as_str()],
        "the scan lists the one oversized record's thing and no other: {over:?}"
    );
    assert!(over[0].2 > jojobot_domain::text::ANSWER_CEILING, "{over:?}");
}

/// **The one warning a clean boot of the gate logs.** The gate starts the binary
/// with no issuer on purpose, so the boot says the endpoint is open. Any other
/// warning is a degradation.
const WARNING_THE_GATE_CAUSES: &str = "AUTH DISABLED";

/// **The warning a boot logs for the handle two things have worn**, naming the
/// field it sits under. The recording writes that handle on purpose.
fn is_the_recorded_ambiguous_handle(line: &str) -> bool {
    is_left_as_text_under(line, "upgrade_fixture_points_at")
}

/// **Whether a boot line is the left-as-text warning for a value under `key`.**
fn is_left_as_text_under(line: &str, key: &str) -> bool {
    line.contains("FIELD VALUE LEFT AS TEXT") && line.contains(key)
}

/// **Whether the recording made at `git_ref` holds the handle two things have
/// worn as plain text** under the field the gate looks for. Read off the dump
/// itself, so the answer is the recording's and not a list kept beside it: a
/// field value is `'<key>',<ordinal>,'<value>'` in the dump, and a plain handle
/// is a value that starts with a kind, where a stored id starts with the mark.
fn recording_holds_the_plain_ambiguous_handle(git_ref: &str) -> bool {
    let dir = [
        DEPLOY_3_FIXTURE_DIR,
        DEPLOY_4_FIXTURE_DIR,
        DEPLOY_5_FIXTURE_DIR,
        DEPLOY_6_FIXTURE_DIR,
    ]
    .into_iter()
    .find(|dir| {
        std::fs::read_to_string(format!("{dir}/ref.txt"))
            .is_ok_and(|recorded| recorded.trim() == git_ref)
    })
    .unwrap_or_else(|| panic!("no fixture was recorded at {git_ref}"));
    let dump = std::fs::read_to_string(format!("{dir}/doltdump.sql"))
        .unwrap_or_else(|e| panic!("reading the dump in {dir}: {e}"));
    dump.split("'upgrade_fixture_points_at',")
        .skip(1)
        .any(|rest| {
            rest.split(',')
                .nth(1)
                .is_some_and(|value| value.starts_with("'thing:"))
        })
}

/// **The alerts sqlx logs when the machine is busy**, each by its level, its
/// target and its message together. On a busy machine a statement of a healthy
/// boot takes longer than a second, and a connection takes longer than two to
/// acquire, so these say the machine is loaded and nothing about the boot. A real
/// boot that waits over two seconds for a connection no longer fails the gate.
///
/// **Narrower than raising the thresholds for the booted binary.** A raised
/// threshold hides every slow statement and connection, whichever target logs
/// it, and needs a setting in the binary's own code for the gate to turn. This
/// reads the two lines the library writes and nothing else, so an alert from
/// another target or with another message, and every other warning, still fails
/// the gate.
fn is_a_busy_host_alert(line: &str) -> bool {
    const ALERTS: [(&str, &str); 2] = [
        (
            "sqlx::query",
            "slow statement: execution time exceeded alert threshold",
        ),
        (
            "sqlx::pool::acquire",
            "acquired connection, but time to acquire exceeded slow threshold",
        ),
    ];
    line.contains("WARN")
        && ALERTS
            .iter()
            .any(|(target, message)| line.contains(&format!("{target}: {message}")))
}

/// **The lines of a boot's log that say part of the boot failed or degraded.**
/// Every warning and every error counts, except the ones the gate itself causes
/// or expects, the ones the caller names in `tolerated_warnings` (each by a word
/// the line holds), and the alerts of a loaded machine.
fn degradations<'a>(
    seen: &'a [String],
    stored_over_the_ceiling: bool,
    tolerated_warnings: &[&str],
) -> Vec<&'a String> {
    seen.iter()
        .filter(|line| !is_a_busy_host_alert(line))
        .filter(|line| {
            let expected = line.contains(WARNING_THE_GATE_CAUSES)
                || is_the_recorded_ambiguous_handle(line)
                || (stored_over_the_ceiling && line.contains("OVER THE CEILING"))
                || tolerated_warnings.iter().any(|named| line.contains(named));
            line.contains("ERROR") || (line.contains("WARN") && !expected)
        })
        .collect()
}

/// **A running copy of the current binary over a restored store.**
struct Booted {
    child: tokio::process::Child,
    http_port: u16,
    /// Every line the binary logged up to its serving line.
    log: Vec<String>,
}

impl Booted {
    /// Stop it, and wait until the store's own lock is released: the dump that
    /// follows needs the directory to itself.
    async fn stop(mut self) {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// **Boot the current binary on the store in `state_dir`** and hold it to the
/// boot's own account of itself: it reaches its serving line, and it says no
/// part of its own boot failed.
async fn boot_current(state_dir: &std::path::Path, store_port: u16, git_ref: &str) -> Booted {
    let reports = usize::from(recording_holds_the_plain_ambiguous_handle(git_ref));
    boot_current_expecting(state_dir, store_port, git_ref, Some(reports), &[]).await
}

/// **The same boot, for a store that holds a record over the ceiling on purpose.**
/// The boot names each one in a warning, which is its job, so that warning is the
/// one more the gate does not count as a degradation.
async fn boot_current_allowing(
    state_dir: &std::path::Path,
    store_port: u16,
    git_ref: &str,
    stored_over_the_ceiling: bool,
) -> Booted {
    let reports = usize::from(recording_holds_the_plain_ambiguous_handle(git_ref));
    boot_current_with(
        state_dir,
        store_port,
        git_ref,
        stored_over_the_ceiling,
        Some(reports),
        &[],
        &[],
    )
    .await
}

/// **The same boot, told how many times it must report the recording's
/// ambiguous handle (`None` leaves the count to the caller), and which other
/// warnings it may log, each named by a word the line holds.** A first boot
/// reports the recording's handle when the recording holds it.
async fn boot_current_expecting(
    state_dir: &std::path::Path,
    store_port: u16,
    git_ref: &str,
    ambiguous_reports: Option<usize>,
    tolerated_warnings: &[&str],
) -> Booted {
    boot_current_with(
        state_dir,
        store_port,
        git_ref,
        false,
        ambiguous_reports,
        tolerated_warnings,
        &[],
    )
    .await
}

/// The boot every other one is a case of.
async fn boot_current_with(
    state_dir: &std::path::Path,
    store_port: u16,
    git_ref: &str,
    stored_over_the_ceiling: bool,
    ambiguous_reports: Option<usize>,
    tolerated_warnings: &[&str],
    // **Settings the case gives the instance beyond the gate's own**, as it
    // would in the service's environment file.
    environment: &[(&str, String)],
) -> Booted {
    let binary = jojobot_exercise::room::server_binary().expect("a jojobot binary to run");
    let http_port = free_port();
    let mut child = tokio::process::Command::new(&binary)
        .envs(environment.iter().map(|(key, value)| (*key, value)))
        .env("STATE_DIRECTORY", state_dir)
        .env("JOJOBOT_STORE_PORT", store_port.to_string())
        .env("JOJOBOT_BIND", format!("127.0.0.1:{http_port}"))
        .env("JOJOBOT_ALLOW_NO_AUTH", "1")
        .env_remove("JOJOBOT_ISSUER")
        // The serving line is an info event. An inherited RUST_LOG can filter it
        // out: the package build exports RUST_LOG="", which parses as a filter
        // with no directives and hides every info line.
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("the current binary runs");

    let stdout = child.stdout.take().expect("stdout was piped");
    let mut lines = tokio::io::AsyncBufReadExt::lines(tokio::io::BufReader::new(stdout));
    let mut seen = Vec::new();
    let boot_began = std::time::Instant::now();
    let served = tokio::time::timeout(jojobot_exercise::room::READY_TIMEOUT, async {
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let hit = line.contains("serving http://");
                    seen.push(line);
                    if hit {
                        return true;
                    }
                }
                Ok(None) | Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        served,
        "the current binary did not reach its own serving line after {:?} (a boot is given \
         {:?}) on a store recorded at {git_ref} — this is the upgrade class this gate exists \
         to catch. What it said instead:\n{}",
        boot_began.elapsed(),
        jojobot_exercise::room::READY_TIMEOUT,
        seen.join("\n")
    );

    // **Keep reading.** The binary logs to the pipe for as long as it runs, and
    // a pipe nobody reads fills, after which the binary blocks inside a write
    // and every call to it hangs.
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
    let stderr = child.stderr.take().expect("stderr was piped");
    tokio::spawn(async move {
        let mut lines = tokio::io::AsyncBufReadExt::lines(tokio::io::BufReader::new(stderr));
        while let Ok(Some(_)) = lines.next_line().await {}
    });

    // **Every warning the boot logs fails the gate, except the one the gate
    // causes.** Each says a half of the boot failed or degraded and carried on:
    // the fold or the search index came up empty, a row was left as text or not
    // rekeyed, a migration failed and will retry. The server serves and every
    // read of a store that was upgraded looks fine until somebody asks the half
    // that is missing. A filter that names each message lets a new one through
    // unseen, so the filter is the level, with one named exception.
    //
    // **An error line fails it too, whatever it says.** The boot logs a pass it
    // could not finish at the error level.
    let warned = degradations(&seen, stored_over_the_ceiling, tolerated_warnings);
    assert!(
        warned.is_empty(),
        "the current binary booted on a store recorded at {git_ref} and said part of its own \
         boot failed: {warned:?}"
    );
    // **The one degradation the recording holds is reported, once, every boot.** A
    // handle two things have worn cannot be lowered, so each boot says so by
    // name. A boot that says nothing has stopped seeing the row, and the gate
    // would then pass over a store it no longer exercises.
    //
    // **Only a recording that holds the handle as plain text has one to report.**
    // A build that stores every field value as a permanent id leaves nothing to
    // lower, so a store it filled holds no such row and its boot says nothing,
    // and a boot that reported one there would be inventing it. The recording
    // itself says which it is.
    let reported = seen
        .iter()
        .filter(|line| is_the_recorded_ambiguous_handle(line))
        .count();
    if let Some(ambiguous_reports) = ambiguous_reports {
        assert_eq!(
            reported, ambiguous_reports,
            "the boot of a store recorded at {git_ref} must report the ambiguous handle \
             {ambiguous_reports} time(s): {seen:?}"
        );
    }
    Booted {
        child,
        http_port,
        log: seen,
    }
}

/// **Where the store's own tool keeps its configuration for a run.** The tool
/// keeps its global configuration under a home directory and fails where the
/// environment names none (a build sandbox). The run's own state directory holds
/// it, as the adapter does for every store it spawns. Every call of the tool in
/// this file names it.
fn dolt_home_in(state_dir: &std::path::Path) -> std::path::PathBuf {
    let home = state_dir.join("dolt-home");
    std::fs::create_dir_all(&home).expect("a directory for the store tool's configuration");
    home
}

/// **The whole store as text**, dumped by the store's own tool from the
/// directory the binary keeps it in, with no binary running. The same dump the
/// recording is made with, so two of them compare row for row.
async fn dump_store(state_dir: &std::path::Path) -> String {
    let database_dir = state_dir.join("db").join("jojobot");
    assert!(
        database_dir.join(".dolt").is_dir(),
        "the binary's database directory is not where boot_store puts it: {}",
        database_dir.display()
    );
    let dump = tokio::process::Command::new("dolt")
        .env("DOLT_ROOT_PATH", dolt_home_in(state_dir))
        .arg("dump")
        .arg("-r")
        .arg("sql")
        .arg("-f")
        .arg("-fn")
        .arg("gate-compare.sql")
        .current_dir(&database_dir)
        .output()
        .await
        .expect("dolt dump runs");
    assert!(
        dump.status.success(),
        "dolt dump failed: {}",
        String::from_utf8_lossy(&dump.stderr)
    );
    let text = std::fs::read_to_string(database_dir.join("gate-compare.sql"))
        .expect("the dump is where dolt put it");
    let _ = std::fs::remove_file(database_dir.join("gate-compare.sql"));
    text
}

/// **A lease on a role object survives a restart, and a renewal past the renewal
/// age overwrites its moment instead of adding a write.** The store is left as
/// the previous steps made it, the moment on the role object is aged to ten
/// minutes ago with the binary stopped, and the current binary boots over it. The
/// holder is read back, a rival is refused, and the holder's next write renews:
/// the moment is new and the key still holds the writes it had.
async fn assert_a_claim_shaped_lease_renews_in_place(
    state_dir: &std::path::Path,
    store_port: u16,
    git_ref: &str,
    role_holder: &str,
    role_key: &str,
) {
    let fail = |what: &str, body: &str| -> ! { panic!("recorded at {git_ref}: {what}: {body}") };
    let database_dir = state_dir.join("db").join("jojobot");
    let aged = (jiff::Timestamp::now() - jiff::SignedDuration::from_mins(10)).to_string();
    let aging = tokio::process::Command::new("dolt")
        .env("DOLT_ROOT_PATH", dolt_home_in(state_dir))
        .arg("sql")
        .arg("-q")
        .arg(format!(
            "UPDATE field_write SET value = '{aged}' WHERE `key` = 'claimed_at' AND entity = \
             (SELECT COALESCE(badge, id) FROM entity WHERE id = 'role:upgrade-fixture-holder')"
        ))
        .current_dir(&database_dir)
        .output()
        .await
        .expect("dolt sql runs");
    if !aging.status.success() {
        fail(
            "the role object's moment could not be aged",
            &String::from_utf8_lossy(&aging.stderr),
        );
    }

    let fourth = boot_current_expecting(state_dir, store_port, git_ref, Some(0), &[]).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", fourth.http_port))
        .await
        .expect("connecting to the current binary");
    let read_role = || async {
        let read = surface
            .call(
                "recall",
                json!({"subject": "role:upgrade-fixture-holder", "history": "claimed_at"}),
            )
            .await;
        let parsed: serde_json::Value =
            serde_json::from_str(&read).unwrap_or_else(|_| fail("the role object", &read));
        parsed["objects"][0].clone()
    };

    let before = read_role().await;
    if before["fields"]["holder"] != role_holder || before["fields"]["claimed_at"] != aged {
        fail(
            "the claim-shaped lease did not read back across a restart",
            &before.to_string(),
        );
    }
    let rival = surface
        .call(
            "start_here",
            json!({"bot": "assistant", "brief": true, "resume": "new",
                   "claim": "upgrade-fixture-holder"}),
        )
        .await;
    if !rival.contains("\"refused\"") {
        fail("a rival was not refused by the restarted lease", &rival);
    }

    let beat = surface
        .call(
            "journal",
            json!({"entry": "renewed after the restart", "sid": role_key}),
        )
        .await;
    if beat.contains("\"status\":\"blocked\"") {
        fail("the holder's write after the restart was refused", &beat);
    }
    let after = read_role().await;
    if after["fields"]["claimed_at"] == before["fields"]["claimed_at"] {
        fail("the renewal did not move the moment", &after.to_string());
    }
    if after["history"]["count"] != before["history"]["count"] {
        fail(
            "the renewal added a write to the lease moment",
            &format!("{} then {}", before["history"], after["history"]),
        );
    }
    surface.finish().await;
    fourth.stop().await;
}

/// **Whether the old store's table has the column.** An older recording lacks the
/// columns a later build added, and a query that names one fails there instead of
/// answering for the shape the store has.
async fn has_column(pool: &sqlx::MySqlPool, table: &str, column: &str) -> bool {
    let found: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns \
         WHERE table_schema = DATABASE() AND table_name = ? AND column_name = ?",
    )
    .bind(table)
    .bind(column)
    .fetch_one(pool)
    .await
    .expect("the columns of the old store read");
    found > 0
}

/// **Every value a thing holds in the old store**, as `(handle, key, value)`:
/// the newest write of each key among the writes of records that still stand,
/// leaving out the rows of the class `stays`, which never reach the thing.
///
/// **A value stored as permanent ids is read here as the handles those ids answer
/// to**, off the same old store: each id is replaced, in place, by the handle of
/// the entity wearing it. What the upgraded binary serves is then
/// compared with that, position by position, so a list served in another order
/// or naming other things is a difference and not a match.
async fn fields_every_thing_holds(pool: &sqlx::MySqlPool) -> Vec<(String, String, String)> {
    let rows: Vec<(Option<String>, String, Option<String>)> =
        sqlx::query_as("SELECT badge, id, merged_into FROM entity")
            .fetch_all(pool)
            .await
            .expect("the old store's badges read");
    // **A folded thing answers as the thing it was folded into**, and a chain of
    // folds answers as the last: a value that names it is served as the survivor's
    // handle, so that is the handle it is compared as.
    let folded_into: std::collections::BTreeMap<&str, &str> = rows
        .iter()
        .filter_map(|(_, id, into)| into.as_deref().map(|into| (id.as_str(), into)))
        .collect();
    let answers_as = |id: &str| -> String {
        let mut now = id;
        for _ in 0..16 {
            match folded_into.get(now) {
                Some(next) => now = next,
                None => break,
            }
        }
        now.to_string()
    };
    let badges: std::collections::BTreeMap<String, String> = rows
        .iter()
        .filter_map(|(badge, id, _)| badge.as_ref().map(|badge| (badge.clone(), answers_as(id))))
        .collect();
    // A recording from before rows had a class holds none to leave out.
    let (outer, inner) = if has_column(pool, "field_write", "write_class").await {
        (
            "AND w.write_class <> 'stays'",
            "AND w2.write_class <> 'stays'",
        )
    } else {
        ("", "")
    };
    let held: Vec<(String, String, String)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT e.id, w.`key`, w.value
         FROM field_write w
         JOIN fact f ON f.entity = w.entity AND f.id = w.fact_id AND f.status = 'active'
         JOIN entity e ON e.badge = w.entity
         -- A folded handle is no thing: it answers by naming its survivor, and what
         -- it held is read as the survivor's.
         WHERE e.merged_into IS NULL
           AND w.value IS NOT NULL
           -- A key written under `fields` stays on its record and never reaches the
           -- thing, so a thing does not hold it; rows written before the class
           -- existed are `own` and do reach it.
           {outer}
           -- The two keys jojobot writes about a record and never folds onto a thing.
           AND w.`key` NOT IN ('merged_from', 'retracts')
           -- A key declared to describe its record is the record's own and the thing
           -- never holds it.
           AND w.`key` NOT IN (SELECT key_name FROM type_field WHERE folds = 'describes')
           AND w.ordinal = (
             SELECT MAX(w2.ordinal)
             FROM field_write w2
             JOIN fact f2 ON f2.entity = w2.entity AND f2.id = w2.fact_id AND f2.status = 'active'
             WHERE w2.entity = w.entity AND w2.`key` = w.`key`
               {inner})
         ORDER BY e.id, w.`key`"
    )))
    .fetch_all(pool)
    .await
    .expect("the old store's folded fields read");
    held.into_iter()
        .map(|(handle, key, value)| (handle, key, with_badges_as_handles(&value, &badges)))
        .collect()
}

/// **The rows `fields_every_thing_holds` cannot place under a handle**, as
/// `(entity, key)`: the same newest value of a key on a record that still
/// stands, whose entity column no entity wears as its badge. That is a field
/// written in a store that never badged, keyed by a bare handle. The join to
/// `entity` leaves such a row out without a word, so the oracle would compare
/// less than the fixture holds and still pass; this names what it left out.
async fn held_rows_that_do_not_join(pool: &sqlx::MySqlPool) -> Vec<(String, String)> {
    let (outer, inner) = if has_column(pool, "field_write", "write_class").await {
        (
            "AND w.write_class <> 'stays'",
            "AND w2.write_class <> 'stays'",
        )
    } else {
        ("", "")
    };
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT w.entity, w.`key`
         FROM field_write w
         JOIN fact f ON f.entity = w.entity AND f.id = w.fact_id AND f.status = 'active'
         LEFT JOIN entity e ON e.badge = w.entity
         WHERE e.id IS NULL
           AND w.value IS NOT NULL
           {outer}
           AND w.`key` NOT IN ('merged_from', 'retracts')
           AND w.`key` NOT IN (SELECT key_name FROM type_field WHERE folds = 'describes')
           AND w.ordinal = (
             SELECT MAX(w2.ordinal)
             FROM field_write w2
             JOIN fact f2 ON f2.entity = w2.entity AND f2.id = w2.fact_id AND f2.status = 'active'
             WHERE w2.entity = w.entity AND w2.`key` = w.`key`
               {inner})
         ORDER BY w.entity, w.`key`"
    )))
    .fetch_all(pool)
    .await
    .expect("the old store's unjoined fields read")
}

/// **The oracle compares everything the fixture holds, or the gate fails.** A
/// row it could not join is not compared, and a comparison that skipped rows
/// says nothing about them.
async fn assert_every_held_row_joined(pool: &sqlx::MySqlPool, git_ref: &str) {
    let unjoined = held_rows_that_do_not_join(pool).await;
    assert!(
        unjoined.is_empty(),
        "recorded at {git_ref}: {} held values are unjoined to any entity, so the upgrade \
         comparison would skip them: {unjoined:?}",
        unjoined.len()
    );
}

/// **A stored value with each item that is a badge replaced by the handle of the
/// entity wearing it.** An item is one piece of a comma list, and it is a badge
/// when it is one an entity wears, with or without the `@#` mark: a field a type
/// declared a reference holds the bare id, any other key holds the marked one.
/// Separators and every other item are untouched. A badge nothing wears is left
/// as it is, so the comparison against it fails loudly.
fn with_badges_as_handles(
    value: &str,
    badges: &std::collections::BTreeMap<String, String>,
) -> String {
    value
        .split(',')
        .map(|piece| {
            let item = piece.trim();
            let bare = item.strip_prefix("@#").unwrap_or(item);
            match badges.get(bare) {
                Some(handle) => piece.replacen(item, handle, 1),
                None => piece.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// **The upgrade changed nothing a thing holds.** Each value the old store held
/// is still held, under the same key, after the current binary has booted on it.
/// A value that names another thing was stored as that thing's permanent id and
/// is served as the handle the thing answers to; it is compared as that handle,
/// item by item and in order.
async fn assert_every_thing_holds_what_it_held(
    surface: &Surface,
    git_ref: &str,
    held_before: &[(String, String, String)],
) {
    eprintln!(
        "upgrade gate: comparing {} values the store recorded at {git_ref} held before the upgrade",
        held_before.len()
    );
    for (handle, key, was) in held_before {
        // **The one change the boot makes on purpose**: a role's holder and moment
        // move off the bot onto the role object, which the role cases hold.
        if key.starts_with("role/") {
            continue;
        }
        let read = surface.call("recall", json!({"subject": handle})).await;
        let parsed: serde_json::Value = serde_json::from_str(&read).unwrap_or_else(|_| {
            panic!("recorded at {git_ref}: {handle} did not read back: {read}")
        });
        let now = parsed["objects"][0]["fields"][key]
            .as_str()
            .unwrap_or_else(|| {
                panic!(
                    "recorded at {git_ref}: {handle} held '{key}' = '{was}' before the upgrade \
                     and holds nothing under it after: {read}"
                )
            });
        assert_eq!(
            now, was,
            "recorded at {git_ref}: {handle} held '{key}' = '{was}' before the upgrade (ids \
             read as the handles they answered to) and holds '{now}' after"
        );
    }
}

/// **A fixture whose rows all join leaves nothing out of the comparison.** Every
/// recording comes from a build that badges, so every held value belongs to an
/// entity that wears the badge it is keyed by.
#[tokio::test]
async fn every_recorded_held_value_joins_to_an_entity() {
    for (fixture_dir, label) in [
        (DEPLOY_3_FIXTURE_DIR, "joins-first"),
        (DEPLOY_4_FIXTURE_DIR, "joins-second"),
    ] {
        let restored = restore_a_fixture(fixture_dir, label).await;
        assert!(
            !restored.held_before.is_empty(),
            "the comparison is over something at {}",
            restored.git_ref
        );
    }
}

/// **A held value keyed by a bare handle fails the gate.** One thing in the
/// restored store loses its badge, which is what a store that never badged
/// looks like to the join: its field rows are still keyed by the old badge and
/// no entity wears it. The oracle must name them and stop, not compare the rest.
#[tokio::test]
#[should_panic(expected = "unjoined")]
async fn a_held_value_the_oracle_cannot_join_fails_the_gate() {
    let state_dir = std::env::temp_dir().join(format!(
        "jojobot-upgrade-gate-{}-unjoined",
        std::process::id()
    ));
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let _scratch = Scratch(state_dir.clone());
    let git_ref = std::fs::read_to_string(format!("{DEPLOY_3_FIXTURE_DIR}/ref.txt"))
        .expect("the fixture's ref")
        .trim()
        .to_string();
    let store = Dolt::start(&state_dir.join("db"), free_port())
        .await
        .expect("a fresh store comes up to receive the fixture");
    let dump = std::fs::read_to_string(format!("{DEPLOY_3_FIXTURE_DIR}/doltdump.sql"))
        .expect("the fixture's dump");
    for statement in split_sql_statements(&dump) {
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement.as_str()))
            .execute(store.pool())
            .await
            .expect("the fixture replays");
    }
    let (holder, _, _) = fields_every_thing_holds(store.pool())
        .await
        .into_iter()
        .next()
        .expect("the fixture holds a field");
    sqlx::query("UPDATE entity SET badge = NULL WHERE id = ?")
        .bind(&holder)
        .execute(store.pool())
        .await
        .expect("one thing loses its badge");
    assert_every_held_row_joined(store.pool(), &git_ref).await;
}

/// **The statements of a dump, split at the semicolons that end one.** A `CREATE
/// TABLE` spans many lines, so the dump is read as a stream rather than by line.
/// A semicolon inside a single-quoted value ends nothing: the reader follows the
/// quotes, a backslash that escapes the next character and a doubled quote.
fn split_sql_statements(dump: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut in_value = false;
    let mut chars = dump.chars().peekable();
    while let Some(c) = chars.next() {
        current.push(c);
        if in_value {
            match c {
                '\\' => current.extend(chars.next()),
                '\'' if chars.peek() == Some(&'\'') => current.extend(chars.next()),
                '\'' => in_value = false,
                _ => {}
            }
        } else if c == '\'' {
            in_value = true;
        } else if c == ';' {
            statements.push(std::mem::take(&mut current).trim().to_string());
        }
    }
    let rest = current.trim();
    if !rest.is_empty() {
        statements.push(format!("{rest};"));
    }
    statements.retain(|statement| statement != ";");
    statements
}

/// **The splitter keeps a semicolon that sits inside a value.** A document's
/// sections carry the edges field `;;\n\n;\n`, so a recording of the deploy 6.1
/// build holds semicolons inside strings, and a split on every semicolon cuts a
/// statement in the middle of a value. The same text holds an escaped quote and a
/// doubled one, which end no string either.
#[test]
fn the_statement_splitter_keeps_a_semicolon_inside_a_value() {
    let dump = "INSERT INTO `t` VALUES ('a;b','it\\'s; here','it''s; there');\n\
                DROP TABLE IF EXISTS `u`;\n";
    assert_eq!(
        split_sql_statements(dump),
        [
            "INSERT INTO `t` VALUES ('a;b','it\\'s; here','it''s; there');".to_string(),
            "DROP TABLE IF EXISTS `u`;".to_string(),
        ]
    );
}

/// **A role claimed before roles were objects is carried onto its object by the
/// first boot, and stays held.** The recording keeps the claim on the bot. The
/// boot writes the holder and the moment onto a role object and takes the keys
/// off, so a rival is refused naming the holder before and after the holder's
/// next write, and the bot holds no key of the old shape. Without the pass,
/// every role in the store would read as empty here and the rival would be
/// seated.
async fn assert_a_role_claimed_in_the_old_shape_is_carried_by_the_boot_and_stays_held(
    surface: &Surface,
    git_ref: &str,
    role_holder: &str,
    role_key: &str,
) {
    let fail = |what: &str, body: &str| -> ! { panic!("recorded at {git_ref}: {what}: {body}") };
    let rival = |label: &'static str| async move {
        let read = surface
            .call(
                "start_here",
                json!({"bot": "assistant", "brief": true, "resume": "new",
                       "claim": "upgrade-fixture-holder"}),
            )
            .await;
        let parsed: serde_json::Value =
            serde_json::from_str(&read).unwrap_or_else(|_| fail(label, &read));
        parsed["session"]["claim"].clone()
    };
    let the_role_object = || async {
        let read = surface
            .call("recall", json!({"kind": "role", "parent": "bot:assistant"}))
            .await;
        let parsed: serde_json::Value =
            serde_json::from_str(&read).unwrap_or_else(|_| fail("the role objects", &read));
        parsed["objects"]
            .as_array()
            .and_then(|all| {
                all.iter()
                    .find(|o| o["id"] == "role:upgrade-fixture-holder")
            })
            .cloned()
            .unwrap_or_else(|| fail("the role object the boot made", &read))
    };

    let carried = the_role_object().await;
    if carried["fields"]["holder"] != role_holder
        || carried["fields"]["claimed_at"].as_str().is_none()
    {
        fail(
            "the boot carrying the old claim onto the role object",
            &carried.to_string(),
        );
    }
    let before = rival("a rival's claim on a role carried from the old shape").await;
    if before["status"] != "refused" || before["holder"] != role_holder {
        fail(
            "a role carried from the old shape did not refuse a rival, naming its holder",
            &before.to_string(),
        );
    }

    // The holder writes. That renews the claim on the role object.
    let beat = surface
        .call(
            "journal",
            json!({"entry": "worked on after the upgrade", "sid": role_key}),
        )
        .await;
    if beat.contains("\"status\":\"blocked\"") {
        fail("the holder's write after the upgrade was refused", &beat);
    }
    let renewed = the_role_object().await;
    if renewed["fields"]["holder"] != role_holder {
        fail(
            "the holder's write keeping the claim on the role object",
            &renewed.to_string(),
        );
    }
    let after = rival("a rival's claim after the holder's write").await;
    if after["status"] != "refused" || after["holder"] != role_holder {
        fail(
            "the claim did not still refuse a rival, naming its holder",
            &after.to_string(),
        );
    }
}

/// **What a store that has run for a while holds beyond the plain records, read
/// back through the served surface.** Each of these is a row the deployed build
/// wrote: a type named for a shipped kind, a handle two things have worn, a list
/// of handles under a key nobody declared, a message somebody decided is
/// unreadable, an archived thing, and a bot filled to the deployed build's boot
/// ceiling. The recording is the primary source; what each read answers here was
/// read off the upgraded store, not written from what it ought to say.
async fn assert_what_a_real_store_holds_reads_back(
    surface: &Surface,
    git_ref: &str,
    role_key: &str,
) {
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not read back correctly: {body}")
    };
    let json_of = |what: &str, read: &str| -> serde_json::Value {
        serde_json::from_str(read).unwrap_or_else(|_| fail(what, read))
    };

    // A type a caller named `topic` sits beside the shipped kind of that name.
    let read = surface.call("start_here", json!({"brief": true})).await;
    let boot = json_of("the vocabulary", &read);
    let vocabulary = &boot["snapshot"]["vocabulary"];
    if !vocabulary["types"]
        .as_array()
        .is_some_and(|types| types.contains(&json!({"type": "topic"})))
        || boot["snapshot"]["entities"]["by_kind"]["topic"]
            .as_u64()
            .is_none_or(|held| held == 0)
    {
        fail("a type named topic beside the topic kind", &read);
    }

    // The handle two things have worn names the later one, and the thing it was
    // taken from is found at the handle it was renamed to. The field that holds
    // the handle reads back as the text it was written as: the boot could not
    // lower it, and said so.
    for (handle, name) in [
        ("thing:upgrade-fixture-namesake", "A Later Heir"),
        ("thing:upgrade-fixture-successor", "A Recorded Namesake"),
    ] {
        let read = surface.call("recall", json!({"subject": handle})).await;
        let parsed = json_of(handle, &read);
        if parsed["objects"][0]["id"] != handle || parsed["objects"][0]["name"] != name {
            fail(handle, &read);
        }
    }
    let read = surface
        .call(
            "recall",
            json!({"subject": "thing:upgrade-fixture-thing", "facts": true}),
        )
        .await;
    let fields = json_of("the thing's fields", &read)["objects"][0]["fields"].clone();
    if fields["upgrade_fixture_points_at"] != "thing:upgrade-fixture-namesake" {
        fail("the ambiguous handle left as the text written", &read);
    }
    // A list of handles under a key nobody declared reads back as written.
    if fields["upgrade_fixture_company"]
        != "person:upgrade-fixture-person, place:upgrade-fixture-place"
    {
        fail("the list of handles", &read);
    }

    // A quarantined message is counted apart and opens for nobody, and the
    // refusal carries the reason it was set aside with.
    let read = surface
        .call(
            "read_mailbox",
            json!({"counts_only": true, "sid": role_key}),
        )
        .await;
    let counted = json_of("the mailbox counts", &read);
    // No message in the recording was replaced, so the count is left out.
    if counted["counts"].get("superseded").is_some() {
        fail("no replaced message in the recorded box", &read);
    }
    let quarantined = &counted["quarantined"];
    let id = match (
        quarantined["count"].as_u64(),
        quarantined["ids"][0].as_str(),
    ) {
        (Some(1), Some(id)) => id.to_string(),
        _ => fail("one quarantined message", &read),
    };
    let read = surface
        .call("read_message", json!({"message_id": id, "sid": role_key}))
        .await;
    if json_of("the quarantined message", &read)["status"] != "blocked"
        || !read.contains("recorded as unreadable")
    {
        fail("the quarantined message", &read);
    }

    // An archived thing is read by its handle, says why, and is left out of the
    // default listing, which counts what it left out.
    let read = surface
        .call(
            "recall",
            json!({"subject": "thing:upgrade-fixture-archived"}),
        )
        .await;
    if json_of("the archived thing", &read)["objects"][0]["archived"]["reason"]
        != "recorded as no longer wanted"
    {
        fail("the archived thing", &read);
    }
    let read = surface.call("recall", json!({"kind": "thing"})).await;
    let listed = json_of("the things", &read);
    if listed["archived_excluded"].as_u64() != Some(1)
        || listed["objects"].as_array().is_none_or(|all| {
            all.iter()
                .any(|o| o["id"] == "thing:upgrade-fixture-archived")
        })
    {
        fail("the archived thing left out of the listing", &read);
    }

    // A bot filled to the deployed build's ceiling, whose rule the deployed
    // build let in without the room for its timestamp, still boots and serves its
    // whole charter and its rule.
    let read = surface
        .call(
            "start_here",
            json!({"bot": "upgrade-fixture-heavy", "brief": true}),
        )
        .await;
    let heavy = json_of("the heavy bot's boot", &read);
    let charter = heavy["identity"]["charter"].as_str().unwrap_or("");
    if heavy["identity"]["bot"]["id"] != "bot:upgrade-fixture-heavy"
        || heavy["identity"]["charter_elided"] != false
        || charter.is_empty()
        || !charter.chars().all(|c| c == 'x')
        || !read.contains("the heavy bot keeps its one rule")
        || heavy["session"]["sid"].as_str().is_none()
    {
        fail(
            "the heavy bot's boot",
            &read.chars().take(400).collect::<String>(),
        );
    }
    // A new starred rule on it is refused, and the refusal says how much room it
    // held back for the new record's timestamp.
    let sid = heavy["session"]["sid"].as_str().unwrap_or_default();
    let read = surface
        .call(
            "capture",
            json!({
                "subject": "bot:upgrade-fixture-heavy",
                "content": "one more rule", "provenance": "testimony",
                "fields": {"starred": "true"}, "sid": sid,
            }),
        )
        .await;
    let refused = json_of("a new rule on the heavy bot", &read);
    let kept = jiff::Timestamp::new(0, 123_456_789)
        .expect("a moment")
        .to_string()
        .len()
        - jiff::Timestamp::new(0, 0)
            .expect("a moment")
            .to_string()
            .len();
    if refused["status"] != "blocked" || refused["stamp_margin"] != kept {
        fail("a new rule refused naming the stamp margin", &read);
    }
}

/// **Every category the recording wrote, read back through the served
/// surface** — the second half of the proof. Startup succeeding is not
/// enough on its own: a stricter parse can let the server come up and still
/// fail the one row it was made stricter about the moment something asks
/// for it.
async fn assert_every_recorded_record_reads_back(
    surface: &Surface,
    git_ref: &str,
    role_holder: &str,
) {
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not read back correctly: {body}")
    };

    // Entities of several kinds.
    for (kind, slug) in [
        ("person", "upgrade-fixture-person"),
        ("place", "upgrade-fixture-place"),
        ("thing", "upgrade-fixture-thing"),
    ] {
        let handle = format!("{kind}:{slug}");
        let read = surface
            .call("recall", json!({"subject": handle, "facts": true}))
            .await;
        let parsed: serde_json::Value =
            serde_json::from_str(&read).unwrap_or_else(|_| fail(&handle, &read));
        if parsed["objects"][0]["id"] != handle {
            fail(&handle, &read);
        }
    }

    // The fact with fields, an edge, and a stated provenance.
    let read = surface
        .call(
            "recall",
            json!({"subject": "person:upgrade-fixture-person", "facts": true}),
        )
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the recorded fact", &read));
    let facts = parsed["objects"][0]["facts"]
        .as_array()
        .unwrap_or_else(|| fail("the recorded fact's own list", &read));
    let landed = facts
        .iter()
        .find(|f| f["content"] == "lives at the recorded place");
    let Some(landed) = landed else {
        fail("the recorded fact", &read);
    };
    if landed["provenance"] != "testimony"
        || landed["edge"]["object"] != "place:upgrade-fixture-place"
        || landed["fields"]["since"] != "2026-01-01"
        // A claim written before the bags were kept apart was an own field that
        // reached, and it reads as it always did: under `fields`, with no `sets`.
        || !landed["sets"].is_null()
    {
        fail("the recorded fact's provenance, edge or fields", &read);
    }

    // **A reference-typed value, a claim worked out from another, and a
    // merge** — the three shapes an upgrade of the storage has to carry and
    // an older recording never held.
    let read = surface
        .call(
            "recall",
            json!({"subject": "person:upgrade-fixture-person", "facts": true}),
        )
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the recorded claims", &read));
    let facts = parsed["objects"][0]["facts"]
        .as_array()
        .unwrap_or_else(|| fail("the recorded claims' own list", &read));
    let lived = facts
        .iter()
        .find(|f| f["content"] == "lives at the recorded place")
        .unwrap_or_else(|| fail("the claim a later one is worked out from", &read));
    let visited = facts
        .iter()
        .find(|f| f["content"] == "visited the recorded place")
        .unwrap_or_else(|| fail("the claim that holds a reference", &read));
    if visited["fields"]["venue"] != "place:upgrade-fixture-place" {
        fail("the reference-typed value", &read);
    }
    if visited["derived_from"].is_null() || visited["derived_from"] != lived["address"] {
        fail("the derived_from pointer", &read);
    }
    // **The survivor holds the duplicate's claim and the account of the
    // merge**, and the duplicate's own handle still resolves, as a status that
    // names the survivor.
    let read = surface
        .call(
            "recall",
            json!({"subject": "thing:upgrade-fixture-thing", "facts": true}),
        )
        .await;
    if !read.contains("the twin carried a note") || !read.contains("recorded as one thing") {
        fail("the merge", &read);
    }
    let read = surface
        .call(
            "recall",
            json!({"subject": "thing:recorded-twin", "facts": true}),
        )
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the merged handle", &read));
    // **Served as a status naming the survivor, never as an empty thing.**
    if parsed["objects"][0]["id"] != "thing:recorded-twin"
        || parsed["objects"][0]["status"] != "merged"
        || parsed["objects"][0]["merged_into"] != "thing:upgrade-fixture-thing"
        || read.contains("the twin carried a note")
    {
        fail("the merged handle", &read);
    }

    // The declared type is still in the instance's own vocabulary.
    let read = surface.call("start_here", json!({"brief": true})).await;
    if !read.contains("upgrade-fixture-type") {
        fail("the declared type", &read);
    }
    // The thought: an active, connection-edged claim on the bot's own
    // handle, still readable as one of its own thoughts.
    let read = surface
        .call("recall", json!({"subject": "bot:assistant", "facts": true}))
        .await;
    if !read.contains("a recorded thought, live in the room") {
        fail("the recorded thought", &read);
    }

    // **An anonymous boot of the upgraded store is within the ceiling.** The
    // snapshot is what grows with a store, and a boot that sizes its essay as
    // text overshoots as soon as the snapshot leaves it no spare characters.
    let anonymous = surface.call("start_here", json!({})).await;
    let ceiling = jojobot_domain::text::BOOT_ANSWER.budget;
    if anonymous.chars().count() > ceiling {
        fail(
            "an anonymous boot within the ceiling",
            &format!("{} characters against {ceiling}", anonymous.chars().count()),
        );
    }

    // The role-claim fields: the boot carried them onto the role object, and the
    // bot holds none of the old keys.
    let read = surface
        .call("recall", json!({"subject": "bot:assistant"}))
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the role-claim fields", &read));
    if parsed["objects"][0]["fields"]
        .as_object()
        .is_some_and(|fields| fields.keys().any(|key| key.starts_with("role/")))
    {
        fail("the bot holding no key of the old shape", &read);
    }
    let read = surface
        .call("recall", json!({"subject": "role:upgrade-fixture-holder"}))
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the role-claim fields", &read));
    // **The holder is the session the recording booted as, recorded beside
    // the dump** — a check that any non-empty value passes would pass on a
    // field the upgrade had rewritten to garbage.
    if role_holder.is_empty() || parsed["objects"][0]["fields"]["holder"] != role_holder {
        fail("the role-claim fields", &read);
    }

    // Mail in each of its three states — read through search, since a
    // message this binary did not send is invisible to list_sent, and
    // read_mailbox drains only the caller's own live box.
    for (needle, state) in [
        ("left new", "new"),
        ("will be read", "read"),
        ("will be processed", "processed"),
    ] {
        let read = surface
            .call(
                "search",
                json!({"query": needle, "include_mail": true, "limit": 5}),
            )
            .await;
        if !read.contains(needle) || !read.contains(state) {
            fail(&format!("the {state} mail message"), &read);
        }
        // **The sender is the bot's handle**, not an id and not nothing.
        if !read.contains("\"sender\":\"bot:assistant\"") {
            fail(&format!("the sender of the {state} mail message"), &read);
        }
        // **Mail written before a message could replace another replaces
        // nothing and is replaced by nothing**, and says neither. The keys are
        // left out of a message they do not apply to.
        if read.contains("\"supersedes\"") || read.contains("\"superseded_by\"") {
            fail(&format!("the {state} mail message replaces nothing"), &read);
        }
    }

    // The wrapped session — `list_runs` is the direct read on a bot's own
    // runs and their state, attributed to the caller's own sid.
    let booted = surface
        .call(
            "start_here",
            json!({"bot": "assistant", "brief": true, "resume": "new"}),
        )
        .await;
    let booted: serde_json::Value =
        serde_json::from_str(&booted).unwrap_or_else(|_| fail("booting to verify", &booted));
    let sid = booted["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| fail("booting to verify", &booted.to_string()));
    let read = surface
        .call("list_runs", json!({"sid": sid, "limit": 30}))
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the wrapped session", &read));
    let runs = parsed["runs"]
        .as_array()
        .unwrap_or_else(|| fail("the wrapped session's own runs list", &read));
    // The recording wrapped its own run and left a second one open, so the
    // list holds both states.
    if !runs.iter().any(|run| run["state"] == "wrapped") {
        fail("the wrapped session's own state", &read);
    }
    // **The starred rule rides the boot** of the identity it was written on.
    // The boot that makes the offer carries it; the answer to the offer, which
    // the boot above is, leaves the rules out.
    let offering = surface
        .call("start_here", json!({"bot": "assistant", "brief": true}))
        .await;
    let offering: serde_json::Value = serde_json::from_str(&offering)
        .unwrap_or_else(|_| fail("the assistant's boot that makes the offer", &offering));
    if !offering["identity"]["rules"]
        .as_array()
        .is_some_and(|rules| {
            rules
                .iter()
                .any(|rule| rule["content"] == "the recorder keeps its recording rule")
        })
    {
        fail(
            "the starred rule on the assistant's boot",
            &offering.to_string(),
        );
    }
    assert_the_newer_shapes_read_back(surface, git_ref).await;
    // **What this build newly interprets, read from records the older build
    // wrote as free text, and held on the next write.**
    assert_the_project_keys_and_the_due_day_read_back_and_bind(surface, git_ref, sid).await;
}

/// **A project, work filed under it and a due day the older build stored, all
/// written by the older build.**
///
/// Each is read back as written. Then two writes go through the served surface:
/// a status the project lists lands, and a status it does not list is refused.
/// The refusal is what shows the build's own `project` keys hold the next write.
async fn assert_the_project_keys_and_the_due_day_read_back_and_bind(
    surface: &Surface,
    git_ref: &str,
    sid: &str,
) {
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not read back correctly: {body}")
    };
    let fields_of = |read: &str, what: &str| -> serde_json::Value {
        let parsed: serde_json::Value =
            serde_json::from_str(read).unwrap_or_else(|_| fail(what, read));
        if parsed["objects"][0]["id"].is_null() {
            fail(what, read);
        }
        parsed["objects"][0]["fields"].clone()
    };

    let read = surface
        .call(
            "recall",
            json!({"subject": "project:upgrade-fixture-project"}),
        )
        .await;
    let project = fields_of(&read, "the recorded project");
    if project["status"] != "now"
        || !project["columns"]
            .as_str()
            .is_some_and(|columns| columns.contains("review") && columns.contains("someday"))
    {
        fail("the recorded project's status and columns", &read);
    }

    let read = surface
        .call("recall", json!({"subject": "work:upgrade-fixture-task"}))
        .await;
    let task = fields_of(&read, "the recorded task");
    if task["status"] != "review"
        || task["owner"] != "person:upgrade-fixture-person"
        || task["depends_on"] != "work:upgrade-fixture-prior"
    {
        fail("the recorded task's status, owner and dependency", &read);
    }

    let read = surface
        .call("recall", json!({"subject": "thing:upgrade-fixture-thing"}))
        .await;
    let thing = fields_of(&read, "the thing with a stored due day");
    if thing["due_on"] != "2026-12-01" {
        fail("the stored due day", &read);
    }

    // **An edit that sets another key leaves the stored due day where it was**,
    // and is not refused because of it. The record is testimony an older build
    // wrote, so the key is one that only describes the record: a value field on
    // it is the operator's, and the gate's legacy case holds that refusal.
    let read = surface
        .call(
            "recall",
            json!({"subject": "thing:upgrade-fixture-thing", "facts": true}),
        )
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the thing's own records", &read));
    let address = parsed["objects"][0]["facts"]
        .as_array()
        .and_then(|facts| {
            facts
                .iter()
                .find(|f| f["content"] == "a day kept under decide_by")
        })
        .and_then(|fact| fact["address"].as_str())
        .unwrap_or_else(|| fail("the record carrying the stored due day", &read))
        .to_string();
    let landed = surface
        .call(
            "update_fact",
            json!({"address": address, "fields": {"purpose": "a note on the day"}, "sid": sid}),
        )
        .await;
    if landed.contains("\"status\":\"blocked\"") {
        fail("an edit beside the stored due day", &landed);
    }
    let read = surface
        .call(
            "recall",
            json!({"subject": "thing:upgrade-fixture-thing", "facts": true}),
        )
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the thing after the edit", &read));
    if parsed["objects"][0]["fields"]["due_on"] != "2026-12-01"
        || !read.contains("a note on the day")
    {
        fail("the due day beside an edit that set another key", &read);
    }

    // The next write lands whatever its word, and the answer names the project's
    // own list, which the older build wrote and this one reads unchanged.
    let landed = surface
        .call(
            "capture",
            json!({"subject": "work:upgrade-fixture-task", "content": "waiting on a review",
                   "provenance": "testimony", "sets": {"status": "waiting"}, "sid": sid}),
        )
        .await;
    if landed.contains("\"status\":\"blocked\"") {
        fail("a status the project lists", &landed);
    }
    let unlisted = surface
        .call(
            "capture",
            json!({"subject": "work:upgrade-fixture-task", "content": "an unlisted status",
                   "provenance": "testimony", "sets": {"status": "somewhere-else"},
                   "sid": sid}),
        )
        .await;
    if unlisted.contains("\"status\":\"blocked\"")
        || !unlisted.contains("status_word_note")
        || !unlisted.contains("review")
    {
        fail(
            "an unlisted status landing with the project's columns named",
            &unlisted,
        );
    }
    let unlisted = surface
        .call(
            "capture",
            json!({"subject": "project:upgrade-fixture-project", "content": "an unlisted status",
                   "provenance": "testimony", "sets": {"status": "somewhere-else"},
                   "sid": sid}),
        )
        .await;
    if unlisted.contains("\"status\":\"blocked\"") || !unlisted.contains("status_word_note") {
        fail(
            "the project's own keys holding a status it does not list",
            &unlisted,
        );
    }
    // **The columns the older build wrote are where they were** after both writes.
    let read = surface
        .call(
            "recall",
            json!({"subject": "project:upgrade-fixture-project"}),
        )
        .await;
    let project_after = fields_of(&read, "the project after the writes");
    if project_after["columns"] != project["columns"] {
        fail("the recorded columns after the writes", &read);
    }
}

/// **The shapes an older fixture was missing, each read back as the older
/// build wrote it.** A claim read out of a system of record, a handle under a
/// key nothing declares between two bots, the instance's own zone, and a piece
/// of work at `done`.
async fn assert_the_newer_shapes_read_back(surface: &Surface, git_ref: &str) {
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not read back correctly: {body}")
    };
    let read_of = |read: &str, what: &str| -> serde_json::Value {
        serde_json::from_str(read).unwrap_or_else(|_| fail(what, read))
    };

    // The claim read out of a system of record keeps its provenance and the
    // system it names.
    let read = surface
        .call(
            "recall",
            json!({"subject": "place:upgrade-fixture-place", "facts": true}),
        )
        .await;
    let parsed = read_of(&read, "the recorded observation");
    let observed = parsed["objects"][0]["facts"]
        .as_array()
        .and_then(|facts| {
            facts
                .iter()
                .find(|f| f["content"] == "the recorded place opens at nine")
        })
        .unwrap_or_else(|| fail("the recorded observation", &read));
    if observed["provenance"] != "observation"
        || observed["fields"]["read_from"] != "the recorded place's page"
    {
        fail("the observation's provenance or the system it names", &read);
    }

    // The handle under an undeclared key is served as the handle.
    let read = surface
        .call("recall", json!({"subject": "bot:assistant"}))
        .await;
    let parsed = read_of(&read, "the reports_to field");
    if parsed["objects"][0]["fields"]["reports_to"] != "bot:upgrade-fixture-lead" {
        fail("the handle under reports_to", &read);
    }
    // **A handle the older build kept as the text it was sent** is served as
    // the handle.
    if parsed["objects"][0]["fields"]["pairs_with"] != "bot:upgrade-fixture-lead" {
        fail("the handle under a key nothing declares", &read);
    }

    // The instance's own record keeps the zone it works in.
    let read = surface
        .call("recall", json!({"subject": "topic:instance"}))
        .await;
    let parsed = read_of(&read, "the instance record");
    if parsed["objects"][0]["fields"]["timezone"] != "America/New_York" {
        fail("the instance's zone", &read);
    }

    // The finished piece of work is still finished.
    let read = surface
        .call("recall", json!({"subject": "work:upgrade-fixture-prior"}))
        .await;
    let parsed = read_of(&read, "the finished work");
    if parsed["objects"][0]["fields"]["status"] != "done" {
        fail("the finished work's status", &read);
    }
}

/// **An instance record somebody made by hand keeps what it holds across the
/// boots that seed the record.** The build makes the record when none exists, so
/// the case is the one an existing instance is in: the recording holds one with
/// the zone on it, the operator is written onto it through the served surface,
/// and the next boot reads both unchanged. A seed that remade, renamed or
/// refused over the record would drop a key or log a warning, and the boot's own
/// warning check fails the gate on the second.
#[tokio::test]
async fn a_hand_made_instance_record_keeps_its_operator_and_zone_across_a_boot() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_the_fixture("instance-record").await;
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not hold: {body}")
    };

    let first = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", first.http_port))
        .await
        .expect("connecting to the current binary");
    let boot: serde_json::Value = serde_json::from_str(
        &surface
            .call(
                "start_here",
                json!({"bot": "assistant", "brief": true, "resume": "new"}),
            )
            .await,
    )
    .expect("the boot is json");
    let sid = boot["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| fail("booting to write", &boot.to_string()))
        .to_string();
    for (what, tool, args) in [
        (
            "making the operator",
            "add_entity",
            json!({"kind": "person", "handle": "milhouse", "name": "Milhouse",
                   "source": "user-named", "sid": sid}),
        ),
        (
            "naming the operator on the record",
            "capture",
            json!({"subject": "topic:instance", "content": "who the operator is",
                   "provenance": "testimony", "sid": sid,
                   "sets": {"operator": "person:milhouse"}}),
        ),
    ] {
        let answer = surface.call(tool, args).await;
        if answer.contains("\"status\":\"blocked\"") {
            fail(what, &answer);
        }
    }
    surface.finish().await;
    first.stop().await;

    // The first boot reported the recording's ambiguous handle; this one is the
    // second and reports none.
    let second = boot_current_expecting(&state_dir, store_port, &git_ref, Some(0), &[]).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", second.http_port))
        .await
        .expect("connecting to the current binary");
    let read = surface
        .call("recall", json!({"subject": "topic:instance"}))
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the instance record", &read));
    let record = &parsed["objects"][0];
    if record["fields"]["timezone"] != "America/New_York"
        || record["fields"]["operator"] != "person:milhouse"
    {
        fail("the zone and the operator both being read", &read);
    }
    if record["source"] != "test" {
        fail("the record being the one made by hand", &read);
    }
    surface.finish().await;
    second.stop().await;
    drop(scratch);
}

/// **A store upgraded into the voucher kind holds the kind and no voucher.**
/// The kind arrives with the boot's own seed, and nothing writes a voucher on
/// the way: the gate is open until a later release issues the first one.
async fn assert_the_voucher_kind_arrives_and_no_voucher_exists(surface: &Surface, git_ref: &str) {
    let read = surface
        .call("list_entities", json!({"kind": "voucher"}))
        .await;
    let parsed: serde_json::Value = serde_json::from_str(&read).unwrap_or_else(|_| {
        panic!("recorded at {git_ref}: the voucher listing did not read back: {read}")
    });
    assert_eq!(
        parsed["count"], 0,
        "recorded at {git_ref}: a store upgraded into the voucher kind holds a voucher: {read}"
    );
}

/// **A pair written before a write had a moment of its own merges after the
/// upgrade, from each build the gate proves an upgrade from.**
///
/// Each fixture holds two things that each wrote `status` once, recorded unmerged
/// by a build whose writes carry no stamp. The current binary boots over the
/// store, which runs the migration that adds the stamp, and merges them through
/// the served verb. Both writes own ordinal 1 of the key, so the fold lands only
/// if the merge renumbers them, and with no stamp on either the survivor's write
/// is placed last: it is the one the survivor holds, and the duplicate's comes
/// first in the key's history. The recording is checked to hold a `field_write`
/// table that has no stamp column, so the case cannot pass over stamped rows.
#[tokio::test]
async fn a_pair_written_before_the_stamp_merges_after_the_upgrade() {
    let (dir, label) = (DEPLOY_3_FIXTURE_DIR, "merge-deploy3");
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(dir, label).await;
    let recorded = std::fs::read_to_string(format!("{dir}/doltdump.sql"))
        .unwrap_or_else(|e| panic!("reading the recording in {dir}: {e}"));
    let field_write_table = recorded
        .split("CREATE TABLE `field_write`")
        .nth(1)
        .and_then(|rest| rest.split(");").next())
        .unwrap_or_else(|| panic!("the recording at {git_ref} holds no field_write table"));
    assert!(
        !field_write_table.contains("written_at"),
        "the recording at {git_ref} already stamps its field writes, so it proves nothing \
         about rows that cannot be stamped: {field_write_table}"
    );
    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let fail = |what: &str, body: &str| -> ! { panic!("recorded at {git_ref}: {what}: {body}") };

    let boot = surface
        .call(
            "start_here",
            json!({"bot": "assistant", "brief": true, "resume": "new"}),
        )
        .await;
    let boot: serde_json::Value =
        serde_json::from_str(&boot).unwrap_or_else(|_| fail("booting to merge", &boot));
    let sid = boot["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| fail("booting to merge", &boot.to_string()))
        .to_string();

    let merged = surface
        .call(
            "merge_entities",
            json!({"duplicate": "thing:blue-kite", "survivor": "thing:red-kite",
                   "reason": "recorded as one kite", "sid": sid}),
        )
        .await;
    let merged_answer: serde_json::Value = serde_json::from_str(&merged)
        .unwrap_or_else(|_| fail("the merge answered something that is not json", &merged));
    if merged_answer["merged"] != "thing:blue-kite"
        || merged_answer["now_resolves_to"] != "thing:red-kite"
    {
        fail("the merge of the pair did not land", &merged);
    }

    let read = surface
        .call(
            "recall",
            json!({"subject": "thing:red-kite", "history": "status"}),
        )
        .await;
    let answered: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("reading the survivor", &read));
    if answered["objects"][0]["fields"]["status"] != "now" {
        fail("the survivor does not hold its own write of status", &read);
    }
    let each: Vec<&str> = answered["objects"][0]["history"]["writes"]
        .as_array()
        .unwrap_or_else(|| fail("the writes behind status", &read))
        .iter()
        .map(|write| write["value"].as_str().unwrap_or(""))
        .collect();
    if each != ["done", "now"] {
        fail(
            "the duplicate's write is not placed before the survivor's",
            &read,
        );
    }

    surface.finish().await;
    booted.stop().await;
    drop(scratch);
}

/// **After the upgrade, `field_link` holds the links the writes imply, and
/// nothing else.**
///
/// The expected set comes from the served surface and not from the start pass
/// that fills the table: every record's fields are read as the surface serves
/// them, and a value that is one handle, or a list in which every item is a
/// handle, is a link from the thing to the thing it names. The table is read
/// from the store directly, with the binary stopped, and each end is turned
/// back into a handle through the entity table. The two sets must be equal and
/// must not be empty. A pass that leaves a link out, or leaves one behind that
/// no write implies, fails here, and so does a gate whose expected set came out
/// of the same code as the table.
#[tokio::test]
async fn field_link_holds_the_links_the_writes_imply_and_nothing_else() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        held_before: _,
        handles: _,
        holder_key: _,
    } = restore_a_fixture(DEPLOY_4_FIXTURE_DIR, "links").await;
    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let expected = links_the_served_fields_imply(&surface, &git_ref).await;
    surface.finish().await;
    booted.stop().await;
    let reading = Dolt::start(&state_dir.join("db"), free_port())
        .await
        .expect("the stopped store comes up to be read");
    let held = links_field_link_holds(reading.pool()).await;
    let mut reading = reading;
    reading.stop().await;
    drop(scratch);

    assert!(
        !expected.is_empty(),
        "the recording at {git_ref} implies no link at all, so this case proves nothing"
    );
    let missing: Vec<_> = expected.difference(&held).collect();
    let extra: Vec<_> = held.difference(&expected).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "field_link disagrees with the writes recorded at {git_ref}. Implied by the served \
         fields and absent from the table: {missing:?}. In the table and implied by no \
         write: {extra:?}"
    );
}

/// **A pass the boot did not finish leaves old keys on the bot, and the next boot
/// takes them off.** The first boot carries the recorded claim onto its role
/// object and takes the keys off. The old keys are then put back by hand and the
/// pass's marker is dropped, which is the shape a store holds when a boot was cut
/// short after the claim landed and before the pass finished. The next boot takes
/// the keys off, a third boot has nothing to do and changes nothing, and the lease
/// is still the holder's afterwards.
#[tokio::test]
async fn old_keys_an_unfinished_pass_left_are_taken_off_by_the_next_boot() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_the_fixture("moved-role-keys").await;
    let role_holder = std::fs::read_to_string(format!("{DEPLOY_3_FIXTURE_DIR}/role_holder.txt"))
        .expect("the recorded holder")
        .trim()
        .to_string();
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not hold: {body}")
    };
    const OLD_HOLDER: &str = "role/upgrade-fixture-holder/holder";
    const OLD_MOMENT: &str = "role/upgrade-fixture-holder/claimed_at";
    let held_on_the_bot = |held: &[(String, String, String)], role: &str| -> Vec<String> {
        held.iter()
            .filter(|(handle, key, _)| {
                handle == "bot:assistant" && key.starts_with(&format!("role/{role}/"))
            })
            .map(|(_, key, _)| key.clone())
            .collect()
    };

    // The holder's write moves the claim onto the role object.
    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let role_key = exchange_the_recorded_run(&surface, &git_ref, &role_holder).await;
    let beat = surface
        .call(
            "journal",
            json!({"entry": "worked on after the upgrade", "sid": role_key}),
        )
        .await;
    if beat.contains("\"status\":\"blocked\"") {
        fail("the holder's write after the upgrade was refused", &beat);
    }
    surface.finish().await;
    booted.stop().await;

    // The old keys are put back on the bot, as the store the older build filled
    // holds them: written again at the next place in their history, and the
    // record that carried them made active again.
    let mut store = Dolt::start(&state_dir.join("db"), store_port)
        .await
        .expect("the store comes up to be read");
    let badge: String = sqlx::query_scalar("SELECT COALESCE(badge, id) FROM entity WHERE id = ?")
        .bind("bot:assistant")
        .fetch_one(store.pool())
        .await
        .expect("the bot's badge");
    let moment = (jiff::Timestamp::now() - jiff::SignedDuration::from_mins(10)).to_string();
    for (key, value) in [
        (OLD_HOLDER, role_holder.as_str()),
        (OLD_MOMENT, moment.as_str()),
    ] {
        let (fact_id, class, highest): (String, String, i64) = sqlx::query_as(
            "SELECT fact_id, write_class, ordinal FROM field_write
             WHERE entity = ? AND `key` = ? ORDER BY ordinal DESC LIMIT 1",
        )
        .bind(&badge)
        .bind(key)
        .fetch_one(store.pool())
        .await
        .unwrap_or_else(|e| fail("the old key was written once", &e.to_string()));
        sqlx::query(
            "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id, written_at, write_class)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&badge)
        .bind(key)
        .bind(highest + 1)
        .bind(value)
        .bind(&fact_id)
        .bind(jiff::Timestamp::now().to_string())
        .bind(class)
        .execute(store.pool())
        .await
        .expect("the old key is written again");
    }
    // A claim record is read from its newest write and folded through its row, so
    // both say it stands again. The move archived it, because it was left with no
    // field: that is the first thing to hold.
    let record_id: String = sqlx::query_scalar(
        "SELECT fact_id FROM field_write WHERE entity = ? AND `key` = ? LIMIT 1",
    )
    .bind(&badge)
    .bind(OLD_HOLDER)
    .fetch_one(store.pool())
    .await
    .expect("the record that carried the old keys");
    let archived: String = sqlx::query_scalar(
        "SELECT status FROM fact_write WHERE entity = ? AND fact_id = ?
         ORDER BY ordinal DESC LIMIT 1",
    )
    .bind(&badge)
    .bind(&record_id)
    .fetch_one(store.pool())
    .await
    .expect("the record's newest write");
    if archived != "archived" {
        fail(
            "the move archiving the claim record it left empty",
            &archived,
        );
    }
    for (table, column) in [("fact", "id"), ("fact_write", "fact_id")] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE {table} SET status = 'active' WHERE entity = ? AND {column} = ?"
        )))
        .bind(&badge)
        .bind(&record_id)
        .execute(store.pool())
        .await
        .expect("the record that carried it stands again");
    }
    sqlx::query("DELETE FROM store_pass WHERE name = 'old role claims'")
        .execute(store.pool())
        .await
        .expect("the pass is owed again");
    let seeded = held_on_the_bot(
        &fields_every_thing_holds(store.pool()).await,
        "upgrade-fixture-holder",
    );
    store.stop().await;
    if seeded.len() != 2 {
        fail(
            "both old keys standing on the bot before the boot",
            &format!("{seeded:?}"),
        );
    }

    // The boot takes them off, and a second boot has nothing left to do.
    // **Only the first boot of a store reports a value left as text**, and this
    // store has booted once already, so every boot from here on reports none.
    let first = boot_current_expecting(&state_dir, store_port, &git_ref, Some(0), &[]).await;
    first.stop().await;
    let after_first = dump_store(&state_dir).await;
    let second = boot_current_expecting(&state_dir, store_port, &git_ref, Some(0), &[]).await;
    second.stop().await;
    if after_first != dump_store(&state_dir).await {
        fail(
            "the second boot changing nothing",
            "the pass wrote again over a store it had already cleared",
        );
    }

    let store = Dolt::start(&state_dir.join("db"), store_port)
        .await
        .expect("the store comes up to be read");
    let held = fields_every_thing_holds(store.pool()).await;
    let mut store = store;
    store.stop().await;
    let left = held_on_the_bot(&held, "upgrade-fixture-holder");
    if !left.is_empty() {
        fail(
            "the bot holding no key of the moved role",
            &format!("{left:?}"),
        );
    }
    // A role the recording released in the old shape (its holder cleared, its
    // moment the epoch) carries no claim, so its keys come off and no holder
    // is put on an object for it.
    let recorder_left = held_on_the_bot(&held, "upgrade-fixture-recorder");
    if !recorder_left.is_empty() {
        fail(
            "the bot holding no key of the released old role",
            &format!("{recorder_left:?}"),
        );
    }
    if held
        .iter()
        .any(|(handle, key, _)| handle == "role:upgrade-fixture-recorder" && key == "holder")
    {
        fail(
            "the released old role carrying no holder",
            &format!("{held:?}"),
        );
    }
    let on_object = |key: &str| {
        held.iter()
            .find(|(handle, k, _)| handle == "role:upgrade-fixture-holder" && k == key)
            .map(|(_, _, value)| value.clone())
    };
    if on_object("holder").as_deref() != Some(role_holder.as_str())
        || on_object("claimed_at").is_none()
    {
        fail(
            "the role object still holding the lease",
            &format!("{held:?}"),
        );
    }

    // And the lease is still the holder's: a rival is refused naming it.
    let third = boot_current_expecting(&state_dir, store_port, &git_ref, Some(0), &[]).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", third.http_port))
        .await
        .expect("connecting to the current binary");
    let rival = surface
        .call(
            "start_here",
            json!({"bot": "assistant", "brief": true, "resume": "new",
                   "claim": "upgrade-fixture-holder"}),
        )
        .await;
    let rival: serde_json::Value =
        serde_json::from_str(&rival).unwrap_or_else(|_| fail("the rival's claim", &rival));
    if rival["session"]["claim"]["status"] != "refused"
        || rival["session"]["claim"]["holder"] != role_holder
    {
        fail("a rival refused, naming the holder", &rival.to_string());
    }
    surface.finish().await;
    third.stop().await;
    drop(scratch);
}

/// **A role field an older build left on a bot that does not own the role does
/// not stop the owner.** The recording holds a role in the old shape on the
/// assistant. A second bot is named its owner through `claims_role`, and its
/// claim at the door is taken: the old field sits on another bot and is never
/// read as its lease. The assistant, now a non-owner, is refused with the owner
/// named, whatever its old field says. The owner then archives the role
/// object its claim made, and a stranger's archive of it is refused.
#[tokio::test]
async fn an_old_role_field_on_a_bot_that_is_not_the_owner_does_not_stop_the_owner() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_the_fixture("stray-role").await;
    // **The pass is marked finished before the boot**, so the old field stays
    // where an older build left it and the case asks what it asked: whether a
    // field on a bot that does not own the role stops the owner. It is inert,
    // because nothing reads it as a lease.
    let mut store = open_the_store(&state_dir, store_port).await;
    jojobot_adapters::dolt::migrate::run(store.pool())
        .await
        .expect("the schema moves, so the marker has a table to be written in");
    sqlx::query("INSERT INTO store_pass (name, finished_at) VALUES ('old role claims', ?)")
        .bind(jiff::Timestamp::now().to_string())
        .execute(store.pool())
        .await
        .expect("the pass is marked finished");
    store.stop().await;
    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not hold: {body}")
    };
    let parse = |what: &str, body: &str| -> serde_json::Value {
        serde_json::from_str(body).unwrap_or_else(|_| fail(what, body))
    };

    // The assistant's boot is the identity that writes.
    let boot = parse(
        "booting to write",
        &surface
            .call(
                "start_here",
                json!({"bot": "assistant", "brief": true, "resume": "new"}),
            )
            .await,
    );
    let sid = boot["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| fail("booting to write", &boot.to_string()))
        .to_string();
    let made = surface
        .call(
            "add_entity",
            json!({"kind": "bot", "handle": "alpha", "name": "Alpha",
                   "source": "user-named", "sid": sid}),
        )
        .await;
    if made.contains("\"status\":\"blocked\"") {
        fail("making the owner", &made);
    }
    let named = surface
        .call(
            "capture",
            json!({"subject": "bot:alpha", "content": "alpha runs the role the recording held",
                   "provenance": "testimony", "sid": sid,
                   "sets": {"claims_role": "upgrade-fixture-holder"}}),
        )
        .await;
    if named.contains("\"status\":\"blocked\"") {
        fail("naming the owner", &named);
    }

    let owner = parse(
        "the owner's claim",
        &surface
            .call(
                "start_here",
                json!({"bot": "alpha", "brief": true, "claim": "upgrade-fixture-holder"}),
            )
            .await,
    );
    if owner["session"]["claim"]["status"] != "taken" {
        fail(
            "the owner's claim being taken over an old field on another bot",
            &owner.to_string(),
        );
    }

    let stray = parse(
        "the old holder's claim",
        &surface
            .call(
                "start_here",
                json!({"bot": "assistant", "brief": true, "resume": "new",
                       "claim": "upgrade-fixture-holder"}),
            )
            .await,
    );
    let claim = &stray["session"]["claim"];
    if claim["status"] != "refused" || claim["owner"] != "bot:alpha" {
        fail(
            "the old holder being refused with the owner named",
            &stray.to_string(),
        );
    }

    // **The role object the owner's claim made is the owner's to archive, and a
    // stranger's archive of it is refused naming the owner.** Read over the real
    // store, where the object's parent comes back as a handle.
    let stranger = parse(
        "the stranger's archive",
        &surface
            .call(
                "archive_entity",
                json!({"handle": "role:upgrade-fixture-holder", "reason": "i want it",
                       "sid": sid}),
            )
            .await,
    );
    if stranger["status"] != "blocked"
        || !stranger["how_to_proceed"]
            .as_str()
            .is_some_and(|way| way.contains("bot:alpha"))
    {
        fail(
            "a stranger's archive of a role object being refused, naming its owner",
            &stranger.to_string(),
        );
    }
    let owner_sid = owner["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| fail("the owner's handle", &owner.to_string()))
        .to_string();
    let archived = parse(
        "the owner's archive",
        &surface
            .call(
                "archive_entity",
                json!({"handle": "role:upgrade-fixture-holder", "reason": "made by mistake",
                       "sid": owner_sid}),
            )
            .await,
    );
    if archived["archived"]["reason"] != "made by mistake" {
        fail(
            "the owner's archive of its role object",
            &archived.to_string(),
        );
    }
    surface.finish().await;
    booted.stop().await;
    drop(scratch);
}

/// **Testimony an older build recorded has no session of its own, and this
/// build refuses to rewrite its words or its value fields in place.** The
/// refusal names the route, the route works, and an edit of a key that only
/// describes the record still lands. The claim is one the recording wrote as
/// testimony, so the case is about stored rows and not about a claim this build
/// wrote a moment ago.
#[tokio::test]
async fn testimony_an_older_build_recorded_is_corrected_by_the_route_and_not_in_place() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_the_fixture("legacy-testimony").await;
    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not hold: {body}")
    };

    let boot = surface
        .call(
            "start_here",
            json!({"bot": "assistant", "brief": true, "resume": "new"}),
        )
        .await;
    let boot: serde_json::Value =
        serde_json::from_str(&boot).unwrap_or_else(|_| fail("booting to write", &boot));
    let sid = boot["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| fail("booting to write", &boot.to_string()))
        .to_string();

    const WORDS: &str = "lives at the recorded place";
    let read = surface
        .call(
            "recall",
            json!({"subject": "person:upgrade-fixture-person", "facts": true}),
        )
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the recorded claims", &read));
    let recorded = parsed["objects"][0]["facts"]
        .as_array()
        .and_then(|facts| facts.iter().find(|f| f["content"] == WORDS))
        .unwrap_or_else(|| fail("the recorded testimony", &read));
    if recorded["provenance"] != "testimony" {
        fail("the recorded claim being testimony", &read);
    }
    let address = recorded["address"]
        .as_str()
        .unwrap_or_else(|| fail("the recorded claim's address", &read))
        .to_string();

    // (1) A rewrite of the words in place is refused, names the route, and
    // leaves the words as they were.
    let refused = surface
        .call(
            "update_fact",
            json!({"address": address, "content": "lives somewhere else",
                   "provenance": "testimony", "sid": sid}),
        )
        .await;
    if !refused.contains("\"status\":\"blocked\"")
        || !refused.contains("derived_from")
        || !refused.contains("archived")
    {
        fail("a refusal that names the route", &refused);
    }
    let read = surface
        .call(
            "recall",
            json!({"subject": "person:upgrade-fixture-person", "facts": true}),
        )
        .await;
    if !read.contains(WORDS) || read.contains("lives somewhere else") {
        fail("the refused rewrite leaving the words alone", &read);
    }

    // (3) **A value field on it is the operator's, as its words are.** An edit
    // that sets one, with the words left alone, is refused with the same route
    // and changes nothing. A key that only describes the record is anybody's, and
    // lands.
    let refused = surface
        .call(
            "update_fact",
            json!({"address": address, "fields": {"colour": "red"}, "sid": sid}),
        )
        .await;
    if !refused.contains("\"status\":\"blocked\"")
        || !refused.contains("derived_from")
        || !refused.contains("archived")
    {
        fail("a refusal of a value field that names the route", &refused);
    }
    let read = surface
        .call(
            "recall",
            json!({"subject": "person:upgrade-fixture-person"}),
        )
        .await;
    let parsed: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| fail("the person's fields", &read));
    if !parsed["objects"][0]["fields"]["colour"].is_null() {
        fail("the refused field staying off the person", &read);
    }
    let landed = surface
        .call(
            "update_fact",
            json!({"address": address, "fields": {"purpose": "a note on where"}, "sid": sid}),
        )
        .await;
    if landed.contains("\"status\":\"blocked\"") {
        fail("an edit of a key that only describes the record", &landed);
    }
    let read = surface
        .call(
            "recall",
            json!({"subject": "person:upgrade-fixture-person", "facts": true}),
        )
        .await;
    if !read.contains("a note on where") {
        fail("the describing key the edit set", &read);
    }

    // (4) **A resend of what it already says is not a rewrite.** The recorded
    // words and the value its field holds, sent back beside a key that only
    // describes the record, change nothing the operator said, so the call lands
    // on a claim no session wrote. A changed word is the refusal of step (1).
    let resent = surface
        .call(
            "update_fact",
            json!({
                "address": address, "content": WORDS,
                "fields": {"since": "2026-01-01", "purpose": "a note on the resend"},
                "sid": sid,
            }),
        )
        .await;
    if resent.contains("\"status\":\"blocked\"") {
        fail("a resend of the recorded words and values", &resent);
    }

    // (2) Archive the claim, then write the corrected one derived from it.
    let archived = surface
        .call(
            "update_fact",
            json!({"address": address, "status": "archived",
                   "details": "the operator corrected it", "sid": sid}),
        )
        .await;
    if archived.contains("\"status\":\"blocked\"") {
        fail("archiving the recorded claim", &archived);
    }
    let corrected = surface
        .call(
            "capture",
            json!({"subject": "person:upgrade-fixture-person",
                   "content": "lives somewhere else", "provenance": "testimony",
                   "derived_from": address, "sid": sid}),
        )
        .await;
    if corrected.contains("\"status\":\"blocked\"") {
        fail(
            "the corrected claim derived from the archived one",
            &corrected,
        );
    }
    let read = surface
        .call(
            "recall",
            json!({"subject": "person:upgrade-fixture-person", "history_record": address}),
        )
        .await;
    if !read.contains(WORDS) || !read.contains("archived") {
        fail("the archived original staying readable", &read);
    }
    let read = surface
        .call(
            "recall",
            json!({"subject": "person:upgrade-fixture-person", "facts": true}),
        )
        .await;
    if !read.contains("lives somewhere else") {
        fail("the corrected claim being served", &read);
    }

    surface.finish().await;
    booted.stop().await;
    drop(scratch);
}

/// A link as the surface names it: the thing, the key, the thing it points at.
type Link = (String, String, String);

/// **Every link the served fields imply.** Each kind in the snapshot is browsed
/// with its records, and a field value that is one handle of a thing in the
/// store, or a comma-separated list of them, is a link.
async fn links_the_served_fields_imply(
    surface: &Surface,
    git_ref: &str,
) -> std::collections::BTreeSet<Link> {
    let fail = |what: &str, body: &str| -> ! {
        panic!("recorded at {git_ref}: {what} did not read back correctly: {body}")
    };
    let snapshot = surface.call("start_here", json!({})).await;
    let parsed: serde_json::Value =
        serde_json::from_str(&snapshot).unwrap_or_else(|_| fail("the snapshot", &snapshot));
    let kinds: Vec<String> = parsed["snapshot"]["entities"]["by_kind"]
        .as_object()
        .unwrap_or_else(|| fail("the snapshot's kinds", &snapshot))
        .keys()
        .cloned()
        .collect();
    assert!(!kinds.is_empty(), "the snapshot lists no kind: {snapshot}");

    let mut things = Vec::new();
    for kind in &kinds {
        let read = surface
            .call("recall", json!({"kind": kind, "facts": true}))
            .await;
        let parsed: serde_json::Value = serde_json::from_str(&read)
            .unwrap_or_else(|_| fail(&format!("the {kind} records"), &read));
        for object in parsed["objects"].as_array().into_iter().flatten() {
            things.push(object.clone());
        }
    }
    let mut known: std::collections::BTreeSet<String> = things
        .iter()
        .filter_map(|object| object["id"].as_str().map(str::to_string))
        .collect();
    // **A thing a browse does not list is still a thing a value can name.** An
    // archived one and one folded into another are left out of a kind browse,
    // and the direct door by its handle answers both. A value that names one is
    // a link as much as one that names a listed thing, so the oracle asks the
    // direct door for every handle-shaped value the browse did not know.
    let mut unlisted = std::collections::BTreeSet::new();
    for object in &things {
        for fact in object["facts"].as_array().into_iter().flatten() {
            for value in fact["fields"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(_, v)| v)
            {
                for item in value.as_str().unwrap_or("").split(',').map(str::trim) {
                    let named_kind = item.split_once(':').map(|(kind, _)| kind);
                    if named_kind.is_some_and(|kind| kinds.iter().any(|k| k == kind))
                        && !known.contains(item)
                    {
                        unlisted.insert(item.to_string());
                    }
                }
            }
        }
    }
    for handle in unlisted {
        let read = surface.call("recall", json!({"subject": handle})).await;
        let parsed: serde_json::Value = serde_json::from_str(&read)
            .unwrap_or_else(|_| fail(&format!("the direct read of {handle}"), &read));
        if parsed["objects"][0]["id"] == handle.as_str() {
            known.insert(handle);
        }
    }

    let mut links = std::collections::BTreeSet::new();
    for object in &things {
        let Some(subject) = object["id"].as_str() else {
            continue;
        };
        for fact in object["facts"].as_array().into_iter().flatten() {
            for (key, value) in fact["fields"].as_object().into_iter().flatten() {
                let Some(value) = value.as_str() else {
                    continue;
                };
                let items: Vec<&str> = value.split(',').map(str::trim).collect();
                if !items.iter().all(|item| known.contains(*item)) {
                    continue;
                }
                for item in items {
                    links.insert((subject.to_string(), key.clone(), item.to_string()));
                }
            }
        }
    }
    links
}

/// **Every link the table holds**, with both ends turned back into handles
/// through the entity table, and the write's ordinal dropped: the same link
/// written twice is one link.
async fn links_field_link_holds(pool: &sqlx::MySqlPool) -> std::collections::BTreeSet<Link> {
    let handles: std::collections::BTreeMap<String, String> =
        sqlx::query_as::<_, (String, Option<String>)>("SELECT id, badge FROM entity")
            .fetch_all(pool)
            .await
            .expect("the entity table reads")
            .into_iter()
            .map(|(id, badge)| {
                (
                    badge
                        .filter(|b| !b.is_empty())
                        .unwrap_or_else(|| id.clone()),
                    id,
                )
            })
            .collect();
    let named = |stored: String| handles.get(&stored).cloned().unwrap_or(stored);
    sqlx::query_as::<_, (String, String, String)>(
        "SELECT entity, `key`, target FROM field_link ORDER BY 1, 2, 3",
    )
    .fetch_all(pool)
    .await
    .expect("field_link reads")
    .into_iter()
    .map(|(entity, key, target)| (named(entity), key, named(target)))
    .collect()
}

/// **A message stored before addresses could be quoted reads back as it was
/// written.** A body that holds `@kind:slug#fN` in a box was written when
/// `@kind:slug` was all a mention could be, and nothing expands it now: only a
/// post to the operator made after this build is read for addresses. The row is
/// put in by hand, as the older build would have stored it, and read through the
/// served surface after the boot has migrated the older mail onto permanent ids.
#[tokio::test]
async fn an_old_message_carrying_the_address_shape_reads_back_as_written() {
    const WRITTEN: &str = "you said @thing:upgrade-fixture-thing#f1 once";
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        held_before: _,
        handles: _,
        holder_key: _,
    } = restore_the_fixture("old-quote").await;
    let mut editing = Dolt::start(&state_dir.join("db"), free_port())
        .await
        .expect("the stopped store comes up to be edited");
    sqlx::query(
        "INSERT INTO message (id, mailbox, ordinal, body, subject, sender, sent_at, state,
             sender_mail_waiting_at_send, posted_by_session)
         VALUES ('0ldquot', 'assistant', 5, ?, NULL, 'rt1fq4', '2026-10-08T13:55:00Z', 'new', 0,
             '3zd1aq')",
    )
    .bind(WRITTEN)
    .execute(editing.pool())
    .await
    .expect("an older message is stored");
    editing.stop().await;

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let role_holder = std::fs::read_to_string(format!("{DEPLOY_3_FIXTURE_DIR}/role_holder.txt"))
        .expect("the recorded role holder")
        .trim()
        .to_string();
    let role_key = exchange_the_recorded_run(&surface, &git_ref, &role_holder).await;
    let read = surface
        .call(
            "read_message",
            json!({"message_id": "0ldquot", "sid": role_key}),
        )
        .await;
    let message: serde_json::Value =
        serde_json::from_str(&read).unwrap_or_else(|_| panic!("a message: {read}"));
    assert_eq!(
        message["body"], WRITTEN,
        "the older message reads back as it was written: {read}"
    );
    surface.finish().await;
    booted.stop().await;
    drop(scratch);
}

/// The thing the injected field writes sit on, and a record of its own it
/// already holds.
const INJECTED_ON: (&str, &str) = ("gwd5wp", "f2");
const LOWERABLE_KEY: &str = "upgrade_gate_lowerable";
const UNGUESSABLE_KEY: &str = "upgrade_gate_unguessable";
const LATER_UNDECLARED_KEY: &str = "upgrade_gate_later";
const LATER_REFERENCE_KEY: &str = "waiting_on";
/// A word the warning for a rebuilt link table holds.
const LINK_TABLE_REBUILT: &str = "LINK TABLE DISAGREED";
const NAMED_THING: &str = "person:upgrade-fixture-person";
/// A roster handle no store in the gate holds, so it names nothing.
const NAMELESS_THING: &str = "person:contract-noted-alpha";

/// **Plain handle text written into the stopped store as an older build wrote
/// it**: one field write per `(key, value)`, on the thing every injection uses.
async fn inject_plain_field_writes(state_dir: &std::path::Path, writes: &[(&str, &str)]) {
    let mut editing = Dolt::start(&state_dir.join("db"), free_port())
        .await
        .expect("the stopped store comes up to be edited");
    for (key, value) in writes {
        sqlx::query(
            "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id)
             VALUES (?, ?, 1, ?, ?)",
        )
        .bind(INJECTED_ON.0)
        .bind(key)
        .bind(value)
        .bind(INJECTED_ON.1)
        .execute(editing.pool())
        .await
        .expect("a field write is injected");
    }
    editing.stop().await;
}

/// What the stopped store holds under `key` on the injected-on thing, and how
/// many `field_link` rows that key has.
async fn stored_under(state_dir: &std::path::Path, key: &str) -> (Option<String>, i64) {
    let mut reading = Dolt::start(&state_dir.join("db"), free_port())
        .await
        .expect("the stopped store comes up to be read");
    let value: Option<String> =
        sqlx::query_scalar("SELECT value FROM field_write WHERE entity = ? AND `key` = ?")
            .bind(INJECTED_ON.0)
            .bind(key)
            .fetch_optional(reading.pool())
            .await
            .expect("the field write reads")
            .flatten();
    let links: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM field_link WHERE entity = ? AND `key` = ?")
            .bind(INJECTED_ON.0)
            .bind(key)
            .fetch_one(reading.pool())
            .await
            .expect("the link rows count");
    reading.stop().await;
    (value, links)
}

/// **A boot after the field passes have settled reads no `field_write` for
/// them and reports nothing again.**
///
/// The first boot lowers plain handle text an older build left and reports the
/// value it cannot lower, once. Then plain handle text is written into the
/// stopped store again, under an undeclared key and under a declared reference
/// key. A second boot that read `field_write` would lower both, so both staying
/// as written proves it read neither pass's table. The second boot logs no
/// left-as-text warning, neither for the injected value nor for the
/// recording's own.
async fn a_settled_store_is_not_read_again(fixture_dir: &str, label: &str) {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        held_before: _,
        handles: _,
        holder_key: _,
    } = restore_a_fixture(fixture_dir, label).await;
    inject_plain_field_writes(
        &state_dir,
        &[
            (LOWERABLE_KEY, NAMED_THING),
            (UNGUESSABLE_KEY, NAMELESS_THING),
        ],
    )
    .await;

    let reports = usize::from(recording_holds_the_plain_ambiguous_handle(&git_ref));
    let first = boot_current_expecting(
        &state_dir,
        store_port,
        &git_ref,
        Some(reports),
        &[UNGUESSABLE_KEY],
    )
    .await;
    let unguessable_reports = first
        .log
        .iter()
        .filter(|line| is_left_as_text_under(line, UNGUESSABLE_KEY))
        .count();
    first.stop().await;
    assert_eq!(
        unguessable_reports, 1,
        "the first boot names the value it cannot lower, once"
    );
    let (lowered, linked) = stored_under(&state_dir, LOWERABLE_KEY).await;
    assert_ne!(
        lowered.as_deref(),
        Some(NAMED_THING),
        "the first boot lowers plain handle text that names one thing"
    );
    assert_eq!(linked, 1, "the first boot links the value it lowered");
    assert_eq!(
        stored_under(&state_dir, UNGUESSABLE_KEY).await.0.as_deref(),
        Some(NAMELESS_THING),
        "a value that names nothing stays as written"
    );

    inject_plain_field_writes(
        &state_dir,
        &[
            (LATER_UNDECLARED_KEY, NAMED_THING),
            (LATER_REFERENCE_KEY, NAMED_THING),
        ],
    )
    .await;
    let second =
        boot_current_expecting(&state_dir, store_port, &git_ref, None, &[UNGUESSABLE_KEY]).await;
    let repeated: Vec<String> = second
        .log
        .iter()
        .filter(|line| line.contains("FIELD VALUE LEFT AS TEXT"))
        .cloned()
        .collect();
    let rebuilt = second
        .log
        .iter()
        .any(|line| line.contains(LINK_TABLE_REBUILT));
    second.stop().await;
    assert!(
        !rebuilt,
        "a healthy store's second boot ran the full link reconcile"
    );
    assert!(
        repeated.is_empty(),
        "a second boot repeats a left-as-text warning: {repeated:?}"
    );
    assert_eq!(
        stored_under(&state_dir, LATER_UNDECLARED_KEY).await,
        (Some(NAMED_THING.to_string()), 0),
        "the second boot read field_write for the link pass: it lowered text written after the first"
    );
    assert_eq!(
        stored_under(&state_dir, LATER_REFERENCE_KEY)
            .await
            .0
            .as_deref(),
        Some(NAMED_THING),
        "the second boot read field_write for the reference pass: it lowered text written after the first"
    );
    drop(scratch);
}

#[tokio::test]
async fn a_second_boot_does_not_read_the_field_passes_again_on_the_deploy_3_store() {
    a_settled_store_is_not_read_again(DEPLOY_3_FIXTURE_DIR, "settled-deploy3").await;
}

#[tokio::test]
async fn a_second_boot_does_not_read_the_field_passes_again_on_the_deploy_4_store() {
    a_settled_store_is_not_read_again(DEPLOY_4_FIXTURE_DIR, "settled-deploy4").await;
}

/// **A link table lost after the lowering finished is rebuilt at the next
/// boot.** The lowering is marked and reads no field write, but the table is a
/// derived index: a boot compares it with the writes cheaply and rebuilds it,
/// with a warning, when they disagree. The pair of the case above, where a
/// healthy store logs no rebuild.
async fn a_lost_link_table_is_rebuilt(fixture_dir: &str, label: &str) {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        held_before: _,
        handles: _,
        holder_key: _,
    } = restore_a_fixture(fixture_dir, label).await;
    let reports = usize::from(recording_holds_the_plain_ambiguous_handle(&git_ref));
    let first = boot_current_expecting(&state_dir, store_port, &git_ref, Some(reports), &[]).await;
    let first_rebuilt = first
        .log
        .iter()
        .any(|line| line.contains(LINK_TABLE_REBUILT));
    first.stop().await;
    assert!(
        !first_rebuilt,
        "a store that had never been booted by this build and held a consistent table logged a \
         rebuild"
    );

    let mut editing = Dolt::start(&state_dir.join("db"), free_port())
        .await
        .expect("the stopped store comes up to be edited");
    let held: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM field_link")
        .fetch_one(editing.pool())
        .await
        .expect("the link rows count");
    assert!(
        held > 0,
        "the recording implies links, or this proves nothing"
    );
    sqlx::query("DELETE FROM field_link")
        .execute(editing.pool())
        .await
        .expect("the link table is emptied");
    editing.stop().await;

    let second = boot_current_expecting(
        &state_dir,
        store_port,
        &git_ref,
        None,
        &[LINK_TABLE_REBUILT],
    )
    .await;
    let rebuilt = second
        .log
        .iter()
        .filter(|line| line.contains(LINK_TABLE_REBUILT))
        .count();
    second.stop().await;
    assert_eq!(rebuilt, 1, "the boot reports the rebuild once");
    let reading = Dolt::start(&state_dir.join("db"), free_port())
        .await
        .expect("the stopped store comes up to be read");
    let restored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM field_link")
        .fetch_one(reading.pool())
        .await
        .expect("the link rows count");
    let mut reading = reading;
    reading.stop().await;
    assert_eq!(
        restored, held,
        "the rebuilt table holds the links the writes imply"
    );
    drop(scratch);
}

#[tokio::test]
async fn a_lost_link_table_is_rebuilt_at_the_next_boot_on_the_deploy_3_store() {
    a_lost_link_table_is_rebuilt(DEPLOY_3_FIXTURE_DIR, "rebuilt-deploy3").await;
}

#[tokio::test]
async fn a_lost_link_table_is_rebuilt_at_the_next_boot_on_the_deploy_4_store() {
    a_lost_link_table_is_rebuilt(DEPLOY_4_FIXTURE_DIR, "rebuilt-deploy4").await;
}

/// **What a real store's pool logs when a statement runs past the alert
/// threshold, and four other lines logged beside it.** The slow statement is a
/// real one: `SLEEP` against the store, through the pool the binary builds its
/// own the same way, so the line is the one sqlx writes and not one invented
/// for the case. Every line carries `label`, so a sibling case's log in the
/// store's one global sink is not read as this case's.
async fn lines_logged_around_a_slow_statement(label: &str) -> Vec<String> {
    let state_dir = std::env::temp_dir().join(format!(
        "jojobot-upgrade-gate-{}-{label}",
        std::process::id()
    ));
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let _scratch = Scratch(state_dir.clone());
    // `free_port` installs the store's log sink, the process's one global
    // subscriber, before the store starts.
    let store = Dolt::start(&state_dir.join("db"), free_port())
        .await
        .expect("a fresh store comes up");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT SLEEP(1.3) /* {label} */"
    )))
    .execute(store.pool())
    .await
    .expect("the slow statement runs");
    tracing::warn!("{label} a boot step degraded");
    tracing::warn!(target: "sqlx::query", "{label} an alert of another kind");
    tracing::warn!(
        target: "jojobot_adapters::dolt",
        "{label} slow statement: execution time exceeded alert threshold"
    );
    tracing::error!("{label} a pass could not finish");
    jojobot_adapters::testing::store_log()
        .lines()
        .filter(|line| line.contains(label))
        .map(str::to_string)
        .collect()
}

/// **The slow-statement alert is not a failed boot.** On a busy machine a
/// statement of the boot runs past sqlx's alert threshold and sqlx says so at
/// warning level. That says nothing about the boot, so the gate lets exactly
/// that line through: by its target and its message.
#[tokio::test]
async fn a_slow_statement_alert_is_not_a_degradation() {
    let logged = lines_logged_around_a_slow_statement("slow-alert-alone").await;
    let alert = logged
        .iter()
        .find(|line| line.contains("sqlx::query: slow statement"))
        .unwrap_or_else(|| {
            panic!("sqlx logged no slow-statement alert, so this proves nothing: {logged:?}")
        });
    assert!(
        degradations(std::slice::from_ref(alert), false, &[]).is_empty(),
        "the slow-statement alert counted as a degraded boot: {alert}"
    );
}

/// **Every other warning and every error still fails the gate**, beside the
/// alert it lets through. A warning from the same target with another message,
/// the alert's own message from another target, a warning from anywhere else
/// and an error are each a degradation, and the alert in the same log is not.
#[tokio::test]
async fn any_other_warning_or_error_is_still_a_degradation_beside_the_alert() {
    let logged = lines_logged_around_a_slow_statement("slow-alert-beside").await;
    let failing = degradations(&logged, false, &[]);
    assert_eq!(
        failing.len(),
        4,
        "the four other lines fail, and only they: {logged:?}"
    );
    assert!(
        failing
            .iter()
            .all(|line| !line.contains("sqlx::query: slow statement")),
        "the alert was counted as a degradation: {failing:?}"
    );
    for said in [
        "a boot step degraded",
        "an alert of another kind",
        "execution time exceeded alert threshold",
        "a pass could not finish",
    ] {
        assert!(
            failing.iter().any(|line| line.contains(said)),
            "{said:?} did not fail the gate: {failing:?}"
        );
    }
}

/// **What a real pool logs when a connection takes longer to acquire than its
/// slow-acquire threshold, and three other lines logged beside it.** The pool is
/// opened onto a fresh store with a threshold no acquire can meet, so the line is
/// the one sqlx writes and not one invented for the case. The real line carries
/// no label, so the threshold is the case's own mark: a sibling case's pool
/// keeps the default and is not read as this case's. The three others carry
/// `label`.
async fn lines_logged_around_a_slow_acquire(label: &str, threshold: Duration) -> Vec<String> {
    let state_dir = std::env::temp_dir().join(format!(
        "jojobot-upgrade-gate-{}-{label}",
        std::process::id()
    ));
    std::fs::create_dir_all(&state_dir).expect("a scratch directory");
    let _scratch = Scratch(state_dir.clone());
    let port = free_port();
    let _store = Dolt::start(&state_dir.join("db"), port)
        .await
        .expect("a fresh store comes up");
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .acquire_slow_threshold(threshold)
        .connect(&format!("mysql://root@127.0.0.1:{port}"))
        .await
        .expect("a pool onto the store");
    drop(pool.acquire().await.expect("a connection is acquired"));
    tracing::warn!(target: "sqlx::pool::acquire", "{label} an alert of another message");
    tracing::warn!(
        target: "sqlx::query",
        "acquired connection, but time to acquire exceeded slow threshold {label}"
    );
    tracing::warn!("{label} a boot step degraded");
    let mark = format!("slow_acquire_threshold_secs={:?}", threshold.as_secs_f64());
    jojobot_adapters::testing::store_log()
        .lines()
        .filter(|line| line.contains(label) || line.contains(&mark))
        .map(str::to_string)
        .collect()
}

/// **The slow-acquire alert is not a failed boot.** On a busy machine a
/// connection of the boot takes longer to acquire than sqlx's threshold and sqlx
/// says so at warning level. That says nothing about the boot, so the gate lets
/// exactly that line through: by its target and its message.
#[tokio::test]
async fn a_slow_acquire_alert_is_not_a_degradation() {
    let logged =
        lines_logged_around_a_slow_acquire("slow-acquire-alone", Duration::from_micros(7)).await;
    let alert = logged
        .iter()
        .find(|line| line.contains("sqlx::pool::acquire: acquired connection"))
        .unwrap_or_else(|| {
            panic!("sqlx logged no slow-acquire alert, so this proves nothing: {logged:?}")
        });
    assert!(
        degradations(std::slice::from_ref(alert), false, &[]).is_empty(),
        "the slow-acquire alert counted as a degraded boot: {alert}"
    );
}

/// **Every other warning still fails the gate**, beside the alert it lets
/// through. A warning from the same target with another message, the alert's own
/// message from another target and a warning from anywhere else are each a
/// degradation, and the alert in the same log is not.
#[tokio::test]
async fn any_other_warning_is_still_a_degradation_beside_the_acquire_alert() {
    let logged =
        lines_logged_around_a_slow_acquire("slow-acquire-beside", Duration::from_micros(9)).await;
    assert!(
        logged
            .iter()
            .any(|line| line.contains("sqlx::pool::acquire: acquired connection")),
        "sqlx logged no slow-acquire alert, so this proves nothing: {logged:?}"
    );
    let failing = degradations(&logged, false, &[]);
    assert_eq!(
        failing.len(),
        3,
        "the three other lines fail, and only they: {logged:?}"
    );
    assert!(
        failing
            .iter()
            .all(|line| !line.contains("sqlx::pool::acquire: acquired connection")),
        "the alert was counted as a degradation: {failing:?}"
    );
    for said in [
        "an alert of another message",
        "exceeded slow threshold slow-acquire-beside",
        "a boot step degraded",
    ] {
        assert!(
            failing.iter().any(|line| line.contains(said)),
            "{said:?} did not fail the gate: {failing:?}"
        );
    }
}

/// **An `entity_write` row an older build appended reads as NULL in every
/// column the snapshot added, and the first write the current binary makes
/// keeps its whole row.** The old rows are counted before the binary touches the
/// store, and the same number of rows is found with no snapshot afterwards: the
/// migration neither filled them nor lost them. The positive is the new write,
/// which has a name and a session, so a NULL on the old rows is a choice and not
/// a column that is never written.
async fn old_entity_write_rows_read_as_null_and_a_new_write_keeps_its_row(
    fixture_dir: &str,
    label: &str,
) {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(fixture_dir, label).await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let old: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entity_write")
        .fetch_one(store.pool())
        .await
        .expect("the old rows are countable");
    store.stop().await;
    assert!(
        old > 0,
        "recorded at {git_ref}: the recording holds no entity_write row"
    );

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let boot: serde_json::Value = serde_json::from_str(
        &surface
            .call(
                "start_here",
                json!({"bot": "assistant", "brief": true, "resume": "new"}),
            )
            .await,
    )
    .expect("the boot answers in JSON");
    let sid = boot["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("recorded at {git_ref}: booting to write: {boot}"))
        .to_string();
    let made = surface
        .call(
            "add_entity",
            json!({"kind": "thing", "handle": "omega", "name": "A snapshot",
                   "source": "user-named", "sid": sid}),
        )
        .await;
    assert!(
        !made.contains("\"status\":\"blocked\""),
        "recorded at {git_ref}: making a thing: {made}"
    );
    surface.finish().await;
    booted.stop().await;

    let mut store = open_the_store(&state_dir, store_port).await;
    let unkept: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entity_write WHERE name IS NULL")
        .fetch_one(store.pool())
        .await
        .expect("the unkept rows are countable");
    let new: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT name, session FROM entity_write WHERE entity = 'thing:omega'")
            .fetch_optional(store.pool())
            .await
            .expect("the new write is readable");
    store.stop().await;
    drop(scratch);

    assert_eq!(
        unkept, old,
        "recorded at {git_ref}: the rows an older build appended are the rows with no snapshot"
    );
    assert_eq!(
        new.map(|(name, _)| name),
        Some(Some("A snapshot".to_string())),
        "recorded at {git_ref}: the write the current binary made keeps its row"
    );
}

#[tokio::test]
async fn deploy_3_entity_write_rows_read_as_null_and_a_new_write_keeps_its_row() {
    old_entity_write_rows_read_as_null_and_a_new_write_keeps_its_row(
        DEPLOY_3_FIXTURE_DIR,
        "entity-write-3",
    )
    .await;
}

#[tokio::test]
async fn deploy_4_entity_write_rows_read_as_null_and_a_new_write_keeps_its_row() {
    old_entity_write_rows_read_as_null_and_a_new_write_keeps_its_row(
        DEPLOY_4_FIXTURE_DIR,
        "entity-write-4",
    )
    .await;
}

/// **A thing an older build wrote versions of before the snapshot columns existed
/// is served its whole history by the booted binary, and a new write is appended
/// to it.** The recorded store has no snapshot column at all, so the handle with
/// the most old rows is found first and the binary boots and migrates. It then
/// serves that handle's history, the versions that hold nothing but a moment
/// still marked as such, and a thing made and renamed afterwards has its two
/// versions, newest last. With the binary stopped, the store's own answer for the
/// handle is read and must be the one the binary served. The binary answers from
/// the image it filled at boot, so an answer that agrees with the store's and
/// grows with each write is the image filled and kept current.
async fn old_entity_versions_are_served_and_a_new_write_is_appended(
    fixture_dir: &str,
    label: &str,
) {
    use jojobot_domain::memory::Memory;
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(fixture_dir, label).await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let (handle, old_rows): (String, i64) = sqlx::query_as(
        "SELECT entity, COUNT(*) AS n FROM entity_write GROUP BY entity ORDER BY n DESC, entity LIMIT 1",
    )
    .fetch_one(store.pool())
    .await
    .unwrap_or_else(|e| panic!("recorded at {git_ref}: the recording holds no entity_write row: {e}"));
    store.stop().await;

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let boot: serde_json::Value = serde_json::from_str(
        &surface
            .call(
                "start_here",
                json!({"bot": "assistant", "brief": true, "resume": "new"}),
            )
            .await,
    )
    .expect("the boot answers in JSON");
    let sid = boot["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("recorded at {git_ref}: booting to write: {boot}"))
        .to_string();
    let history_of = |handle: String| {
        let surface = &surface;
        async move {
            let answered: serde_json::Value = serde_json::from_str(
                &surface
                    .call(
                        "recall",
                        json!({"history_record": handle, "history_most": 100}),
                    )
                    .await,
            )
            .expect("the history answers in JSON");
            answered["objects"][0]["entity_history"].clone()
        }
    };

    let served = history_of(handle.clone()).await;
    let served_versions = served["versions"].as_array().unwrap_or_else(|| {
        panic!("recorded at {git_ref}: {handle} is served no versions: {served}")
    });
    let served_len = served_versions.len();
    let served_unkept = served_versions
        .iter()
        .filter(|v| v.get("name").is_none())
        .count();
    assert!(
        served_len as i64 >= old_rows && served_unkept as i64 >= old_rows,
        "recorded at {git_ref}: {handle} had {old_rows} versions an older build wrote, and the booted \
         binary serves {served_len} of which {served_unkept} hold no snapshot: {served}"
    );
    assert!(
        served["values_not_kept"].is_string(),
        "recorded at {git_ref}: the versions with no snapshot are not marked as such: {served}"
    );

    let made = surface
        .call(
            "add_entity",
            json!({"kind": "thing", "handle": "omega", "name": "A snapshot",
                   "source": "user-named", "sid": sid}),
        )
        .await;
    assert!(
        !made.contains("\"status\":\"blocked\""),
        "recorded at {git_ref}: making a thing: {made}"
    );
    let first = history_of("thing:omega".to_string()).await;
    assert_eq!(
        first["count"], 1,
        "recorded at {git_ref}: a new thing has its one version: {first}"
    );
    let renamed = surface
        .call(
            "update_entity",
            json!({"handle": "thing:omega", "name": "A later name", "sid": sid}),
        )
        .await;
    assert!(
        !renamed.contains("\"status\":\"blocked\""),
        "recorded at {git_ref}: renaming a thing: {renamed}"
    );
    let then = history_of("thing:omega".to_string()).await;
    assert_eq!(
        then["count"], 2,
        "recorded at {git_ref}: the write is appended: {then}"
    );
    assert_eq!(
        then["versions"][1]["name"], "A later name",
        "recorded at {git_ref}: the newest version is last: {then}"
    );
    assert_eq!(
        history_of(handle.clone()).await["versions"]
            .as_array()
            .map(Vec::len),
        Some(served_len),
        "recorded at {git_ref}: another thing's writes leave {handle}'s versions as they were"
    );
    surface.finish().await;
    booted.stop().await;

    // The store's own answer, with the binary stopped: the one the image served.
    let mut store = open_the_store(&state_dir, store_port).await;
    let oracle = jojobot_adapters::dolt::memory::DoltMemory::open(store.pool().clone())
        .entity_history(&jojobot_domain::memory::EntityId(handle.clone()))
        .await
        .unwrap_or_else(|e| panic!("recorded at {git_ref}: the store reads {handle}: {e}"));
    store.stop().await;
    drop(scratch);
    assert_eq!(
        (served_len, served_unkept),
        (
            oracle.len(),
            oracle.iter().filter(|v| v.snapshot.is_none()).count()
        ),
        "recorded at {git_ref}: the booted binary serves {handle} what the store holds"
    );
}

#[tokio::test]
async fn deploy_3_old_entity_versions_are_served_and_a_new_write_is_appended() {
    old_entity_versions_are_served_and_a_new_write_is_appended(
        DEPLOY_3_FIXTURE_DIR,
        "entity-versions-3",
    )
    .await;
}

#[tokio::test]
async fn deploy_4_old_entity_versions_are_served_and_a_new_write_is_appended() {
    old_entity_versions_are_served_and_a_new_write_is_appended(
        DEPLOY_4_FIXTURE_DIR,
        "entity-versions-4",
    )
    .await;
}

/// **A `session_write` row an older build appended reads as NULL in the three
/// columns that keep what changed, and the first writes the current binary makes
/// keep all three.** The old rows are counted before the binary touches the
/// store, and the same number is found with no kind afterwards: the migration
/// neither filled them nor lost them. The positives are the new rows, which name
/// the run beginning and the entry written and carry a moment, and the wording
/// the entry was written with: no entry of an older build has one, so a NULL or
/// an absence on the old rows is a choice and not a column that is never written.
async fn old_session_write_rows_read_as_null_and_new_writes_keep_their_record(
    fixture_dir: &str,
    label: &str,
) {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(fixture_dir, label).await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let old: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_write")
        .fetch_one(store.pool())
        .await
        .expect("the old rows are countable");
    store.stop().await;
    assert!(
        old > 0,
        "recorded at {git_ref}: the recording holds no session_write row"
    );

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let boot: serde_json::Value = serde_json::from_str(
        &surface
            .call(
                "start_here",
                json!({"bot": "assistant", "brief": true, "resume": "new"}),
            )
            .await,
    )
    .expect("the boot answers in JSON");
    let sid = boot["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("recorded at {git_ref}: booting to write: {boot}"))
        .to_string();
    let wrote = surface
        .call(
            "journal",
            json!({"entry": "an entry written by the current binary", "sid": sid}),
        )
        .await;
    assert!(
        !wrote.contains("\"status\":\"blocked\""),
        "recorded at {git_ref}: journalling: {wrote}"
    );
    surface.finish().await;
    booted.stop().await;

    let mut store = open_the_store(&state_dir, store_port).await;
    let unkept: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM session_write WHERE changed IS NULL")
            .fetch_one(store.pool())
            .await
            .expect("the unkept rows are countable");
    let no_moment: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM session_write WHERE written_at IS NULL")
            .fetch_one(store.pool())
            .await
            .expect("the rows with no moment are countable");
    let kinds: Vec<String> = sqlx::query_scalar(
        "SELECT changed FROM session_write WHERE changed IS NOT NULL ORDER BY written_at",
    )
    .fetch_all(store.pool())
    .await
    .expect("the new rows are readable");
    let wordings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_entry_write")
        .fetch_one(store.pool())
        .await
        .expect("the wordings are countable");
    store.stop().await;
    drop(scratch);

    assert_eq!(
        unkept, old,
        "recorded at {git_ref}: the rows an older build appended are the rows with no kind"
    );
    assert_eq!(
        no_moment, old,
        "recorded at {git_ref}: the rows an older build appended are the rows with no moment"
    );
    for wanted in ["begin", "entry"] {
        assert!(
            kinds.iter().any(|kind| kind == wanted),
            "recorded at {git_ref}: no {wanted} among the changes the current binary made: {kinds:?}"
        );
    }
    assert_eq!(
        wordings, 1,
        "recorded at {git_ref}: only the entry the current binary wrote has a wording"
    );
}

#[tokio::test]
async fn deploy_3_session_write_rows_read_as_null_and_new_writes_keep_their_record() {
    old_session_write_rows_read_as_null_and_new_writes_keep_their_record(
        DEPLOY_3_FIXTURE_DIR,
        "session-write-3",
    )
    .await;
}

#[tokio::test]
async fn deploy_4_session_write_rows_read_as_null_and_new_writes_keep_their_record() {
    old_session_write_rows_read_as_null_and_new_writes_keep_their_record(
        DEPLOY_4_FIXTURE_DIR,
        "session-write-4",
    )
    .await;
}

/// **A `fact_write` row an older build appended reads as NULL in the two columns
/// that keep references and the mark, and the first claim the current binary
/// writes keeps both.** The old rows are counted before the binary touches the
/// store, and the same number is found with no kept references afterwards: the
/// migration neither filled them nor lost them. The positive is the new claim,
/// which kept the thing it pointed at and an empty mark, so a NULL on the old
/// rows is a choice and not a column that is never written.
async fn old_fact_write_rows_read_as_null_and_a_new_write_keeps_its_references(
    fixture_dir: &str,
    label: &str,
) {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(fixture_dir, label).await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let old: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fact_write")
        .fetch_one(store.pool())
        .await
        .expect("the old rows are countable");
    store.stop().await;
    assert!(
        old > 0,
        "recorded at {git_ref}: the recording holds no fact_write row"
    );

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let boot: serde_json::Value = serde_json::from_str(
        &surface
            .call(
                "start_here",
                json!({"bot": "assistant", "brief": true, "resume": "new"}),
            )
            .await,
    )
    .expect("the boot answers in JSON");
    let sid = boot["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("recorded at {git_ref}: booting to write: {boot}"))
        .to_string();
    for (kind, handle, name) in [
        ("thing", "omega", "A kept reference"),
        ("person", "teddy", "A pointed person"),
    ] {
        let made = surface
            .call(
                "add_entity",
                json!({"kind": kind, "handle": handle, "name": name,
                       "source": "user-named", "sid": sid}),
            )
            .await;
        assert!(
            !made.contains("\"status\":\"blocked\""),
            "recorded at {git_ref}: making {kind}:{handle}: {made}"
        );
    }
    let written = surface
        .call(
            "capture",
            json!({"subject": "thing:omega", "content": "points at a person",
                   "provenance": "testimony", "refs": ["person:teddy"], "sid": sid}),
        )
        .await;
    assert!(
        !written.contains("\"status\":\"blocked\""),
        "recorded at {git_ref}: writing the claim: {written}"
    );
    surface.finish().await;
    booted.stop().await;

    let mut store = open_the_store(&state_dir, store_port).await;
    let unkept: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fact_write WHERE refs IS NULL")
        .fetch_one(store.pool())
        .await
        .expect("the unkept rows are countable");
    let new: Option<(Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT w.refs, w.stands_for, p.badge FROM fact_write w
         JOIN entity e ON e.badge = w.entity AND e.id = 'thing:omega'
         JOIN entity p ON p.id = 'person:teddy'",
    )
    .fetch_optional(store.pool())
    .await
    .expect("the new write is readable");
    store.stop().await;
    drop(scratch);

    assert_eq!(
        unkept, old,
        "recorded at {git_ref}: the rows an older build appended are the rows with no kept references"
    );
    let (refs, stands_for, pointed_badge) =
        new.unwrap_or_else(|| panic!("recorded at {git_ref}: the new claim has no write"));
    assert_eq!(
        refs,
        Some(format!("[\"{pointed_badge}\"]")),
        "recorded at {git_ref}: the write the current binary made keeps what it pointed at"
    );
    assert_eq!(
        stands_for,
        Some("[]".to_string()),
        "recorded at {git_ref}: the write keeps an empty mark as empty, not as unrecorded"
    );
}

#[tokio::test]
async fn deploy_3_fact_write_rows_read_as_null_and_a_new_write_keeps_its_references() {
    old_fact_write_rows_read_as_null_and_a_new_write_keeps_its_references(
        DEPLOY_3_FIXTURE_DIR,
        "fact-write-3",
    )
    .await;
}

#[tokio::test]
async fn deploy_4_fact_write_rows_read_as_null_and_a_new_write_keeps_its_references() {
    old_fact_write_rows_read_as_null_and_a_new_write_keeps_its_references(
        DEPLOY_4_FIXTURE_DIR,
        "fact-write-4",
    )
    .await;
}

/// **A message an older build changed has no event, reads cleanly, and a change
/// the current binary makes is kept.** The store is restored from a recording, an
/// archived message of a bot's box is picked out before the binary touches it, and
/// the current binary is booted over it, which creates `message_event`. The old
/// message answers its history with no event and a note saying what that means,
/// and answers a plain read as it did. A message posted, read and retired by the
/// current binary then carries the two events, each naming the run: the positive
/// that makes the empty history a fact about the old message and not about a table
/// that is never written.
async fn old_messages_read_cleanly_and_a_new_change_is_kept(fixture_dir: &str, label: &str) {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(fixture_dir, label).await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let archived: Option<String> = sqlx::query_scalar(
        "SELECT id FROM message WHERE state = 'processed' AND mailbox NOT LIKE 'person-%' \
         ORDER BY id LIMIT 1",
    )
    .fetch_optional(store.pool())
    .await
    .expect("an archived message is findable");
    store.stop().await;
    let archived = archived.unwrap_or_else(|| {
        panic!("recorded at {git_ref}: the recording holds no archived message")
    });

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let boot: serde_json::Value = serde_json::from_str(
        &surface
            .call(
                "start_here",
                json!({"bot": "assistant", "brief": true, "resume": "new"}),
            )
            .await,
    )
    .expect("the boot answers in JSON");
    let sid = boot["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("recorded at {git_ref}: booting to read: {boot}"))
        .to_string();
    let run = boot["session"]["run"]
        .as_str()
        .unwrap_or_else(|| panic!("recorded at {git_ref}: the boot names its run: {boot}"))
        .to_string();

    let old: serde_json::Value = serde_json::from_str(
        &surface
            .call(
                "read_message",
                json!({"message_id": archived, "history": true, "sid": sid}),
            )
            .await,
    )
    .expect("the history of an old message answers in JSON");
    assert_eq!(
        old["history"]["count"], 0,
        "recorded at {git_ref}: an old message has no event: {old}"
    );
    assert!(
        old["history"]["note"].is_string(),
        "recorded at {git_ref}: an empty history says what empty means: {old}"
    );
    let plain: serde_json::Value = serde_json::from_str(
        &surface
            .call("read_message", json!({"message_id": archived, "sid": sid}))
            .await,
    )
    .expect("a plain read of an old message answers in JSON");
    assert_eq!(
        plain["state"], "processed",
        "recorded at {git_ref}: an old message reads as it did: {plain}"
    );

    let posted: serde_json::Value = serde_json::from_str(
        &surface
            .call(
                "post_message",
                json!({"to": "assistant", "subject": "a kept change",
                       "body": "read and retired by the current binary", "sid": sid}),
            )
            .await,
    )
    .expect("the post answers in JSON");
    let id = posted["id"]
        .as_str()
        .unwrap_or_else(|| panic!("recorded at {git_ref}: posting: {posted}"))
        .to_string();
    surface
        .call("read_message", json!({"message_id": id, "sid": sid}))
        .await;
    surface
        .call(
            "mark_processed",
            json!({"message_id": id, "notes": "kept", "sid": sid}),
        )
        .await;
    let kept: serde_json::Value = serde_json::from_str(
        &surface
            .call(
                "read_message",
                json!({"message_id": id, "history": true, "sid": sid}),
            )
            .await,
    )
    .expect("the history answers in JSON");
    surface.finish().await;
    booted.stop().await;
    drop(scratch);

    assert_eq!(
        kept["history"]["count"], 2,
        "recorded at {git_ref}: the current binary kept both changes: {kept}"
    );
    let events = &kept["history"]["events"];
    assert_eq!(events[0]["kind"], "read", "{kept}");
    assert_eq!(events[1]["kind"], "processed", "{kept}");
    assert_eq!(events[1]["notes"], "kept", "{kept}");
    assert_eq!(
        events[1]["run"],
        run.as_str(),
        "recorded at {git_ref}: each event names the run that made it: {kept}"
    );
}

#[tokio::test]
async fn deploy_3_old_messages_read_cleanly_and_a_new_change_is_kept() {
    old_messages_read_cleanly_and_a_new_change_is_kept(DEPLOY_3_FIXTURE_DIR, "message-event-3")
        .await;
}

#[tokio::test]
async fn deploy_4_old_messages_read_cleanly_and_a_new_change_is_kept() {
    old_messages_read_cleanly_and_a_new_change_is_kept(DEPLOY_4_FIXTURE_DIR, "message-event-4")
        .await;
}

/// **A deployed store that holds a reused handle counts each thing's own writes.**
/// Both recordings hold a first namesake renamed away and a later heir that took
/// the freed handle, with `entity_write` rows in the shape an older build wrote
/// (no badge). The heir's version counts none of the rows filed under its handle
/// before it came: the former-handle table says the first namesake left it, so
/// they are the first namesake's. The first namesake keeps all of them, and its
/// rename.
async fn a_reused_handle_in_a_deployed_store_counts_each_things_own_writes(
    fixture_dir: &str,
    label: &str,
) {
    use jojobot_adapters::dolt::memory::DoltMemory;
    use jojobot_domain::memory::{EntityId, Memory};

    let Restored {
        scratch: _scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(fixture_dir, label).await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let pool = store.pool().clone();
    // The migrations a boot runs, so the columns the read asks about are there.
    jojobot_adapters::dolt::migrate::run(&pool)
        .await
        .expect("the current migrations run over the recording");
    let memory = DoltMemory::open(pool.clone());
    // The other step a boot takes before it reads a handle.
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");

    let badge_of = |handle: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, Option<String>>("SELECT badge FROM entity WHERE id = ?")
                .bind(handle)
                .fetch_one(&pool)
                .await
                .unwrap_or_else(|e| panic!("{handle} is in the recording: {e}"))
                .unwrap_or_else(|| panic!("{handle} wears a badge once the fill has run"))
        }
    };
    let claim_writes = |badge: String| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM fact_write WHERE entity = ?")
                .bind(badge)
                .fetch_one(&pool)
                .await
                .expect("the claim writes count")
        }
    };
    let heir = "thing:upgrade-fixture-namesake";
    let first = "thing:upgrade-fixture-successor";
    let filed_under_the_handles: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entity_write WHERE entity IN (?, ?)")
            .bind(heir)
            .bind(first)
            .fetch_one(&pool)
            .await
            .expect("the rows filed under both handles count");
    let heir_claims = claim_writes(badge_of(heir).await).await;
    let first_claims = claim_writes(badge_of(first).await).await;

    let heir_version = memory
        .fields_versioned(&EntityId(heir.into()))
        .await
        .expect("the heir's version reads")
        .1;
    let first_version = memory
        .fields_versioned(&EntityId(first.into()))
        .await
        .expect("the first namesake's version reads")
        .1;
    store.stop().await;

    assert_eq!(
        heir_version as i64, heir_claims,
        "recorded at {git_ref}: the heir counts its own claim writes and none of the \
         {filed_under_the_handles} rows filed under the handles before it came"
    );
    assert_eq!(
        first_version as i64,
        first_claims + filed_under_the_handles,
        "recorded at {git_ref}: the first namesake keeps every row filed under the handle \
         it left and under the one it took"
    );
}

#[tokio::test]
async fn a_reused_handle_in_the_deploy_3_store_counts_each_things_own_writes() {
    a_reused_handle_in_a_deployed_store_counts_each_things_own_writes(
        DEPLOY_3_FIXTURE_DIR,
        "reuse-deploy3",
    )
    .await;
}

#[tokio::test]
async fn a_reused_handle_in_the_deploy_4_store_counts_each_things_own_writes() {
    a_reused_handle_in_a_deployed_store_counts_each_things_own_writes(
        DEPLOY_4_FIXTURE_DIR,
        "reuse-deploy4",
    )
    .await;
}

// ── An upgrade never refuses the boot, and never strands what the operator wrote
// on a handle the build supplies (rule 508).

/// **One claim, and the one field it set, filed under `key`.** For a stored thing
/// the key is its badge; for a handle no row holds it is the handle's own text,
/// which is how a write on a record the build supplies is filed. The column lists
/// are the ones `plant_a_view_with_a_filter_record` copies from the recordings.
async fn plant_a_claim_filed_under(
    pool: &sqlx::MySqlPool,
    key: &str,
    content: &str,
    field: (&str, &str),
) {
    sqlx::query(
        "INSERT INTO fact (entity, id, content, details, provenance, standing, status,
                           recorded_at, edge_shape, edge_object, event_kind, derived_from,
                           derived_from_id, inserted_at, stale_after, happened_at,
                           happened_through)
         VALUES (?, 'f1', ?, NULL, 'testimony', 'settled', 'active',
                 '2026-10-08', NULL, NULL, NULL, NULL, NULL, '2026-10-08T14:02:10Z', NULL,
                 NULL, NULL)",
    )
    .bind(key)
    .bind(content)
    .execute(pool)
    .await
    .expect("the record lands");
    sqlx::query(
        "INSERT INTO fact_write (entity, fact_id, ordinal, content, details, provenance,
                                standing, status, recorded_at, edge_shape, edge_object,
                                derived_from, derived_from_id, inserted_at, stale_after,
                                written_at, happened_at, happened_through, session)
         VALUES (?, 'f1', 1, ?, NULL, 'testimony', 'settled', 'active',
                 '2026-10-08', NULL, NULL, NULL, NULL, '2026-10-08T14:02:10Z', NULL,
                 '2026-10-08T14:02:10Z', NULL, NULL, NULL)",
    )
    .bind(key)
    .bind(content)
    .execute(pool)
    .await
    .expect("the record's first write lands");
    // **The shape `capture` with `sets` writes**: the class `sets`, which is what
    // makes the key a property of the thing, and the moment of the write.
    sqlx::query(
        "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id, written_at, write_class)
         VALUES (?, ?, 1, ?, 'f1', '2026-10-08T14:02:10Z', 'sets')",
    )
    .bind(key)
    .bind(field.0)
    .bind(field.1)
    .execute(pool)
    .await
    .expect("the field lands");
}

/// **What `recall` answers for one handle, with its claims**, or the answer's own
/// words when it is not an object.
async fn recalled_with_claims(booted: &Booted, handle: &str) -> serde_json::Value {
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let answer = surface
        .call("recall", json!({"subject": handle, "facts": true}))
        .await;
    serde_json::from_str(&answer).unwrap_or_else(|_| panic!("recall of {handle} said: {answer}"))
}

/// The lines of a boot's log that name `handle`.
fn log_lines_naming<'a>(booted: &'a Booted, handle: &str) -> Vec<&'a String> {
    booted
        .log
        .iter()
        .filter(|line| line.contains(handle))
        .collect()
}

/// **A shipped handle the operator already held a row at.** The build supplies
/// `view:loops`; the store already holds a stored row there, as it would for a
/// view the operator declared before the build shipped one of that name.
const SHIPPED_HANDLE_WITH_A_ROW: &str = "view:loops";
/// A handle the build supplies and the store holds no row at.
const SHIPPED_HANDLE_WITHOUT_ONE: &str = "view:ready";

/// **A boot on a store that holds a row at a handle the build now supplies does
/// not refuse.** The stored row answers, its claims read back through `recall`,
/// and the boot names the handle in one line that is not a warning. Paired with a
/// handle the build supplies and the store holds no row at, which the boot does
/// not name.
#[tokio::test]
async fn a_store_holding_a_row_at_a_handle_the_build_supplies_boots_and_the_row_answers() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(DEPLOY_4_FIXTURE_DIR, "row-at-supplied").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    sqlx::query(
        "INSERT INTO entity (id, kind, name, source, crm, parent, boot, prose, badge,
                             merged_into, archived_reason, archived_at)
         VALUES (?, 'view', 'Stored Loops', 'test', NULL, NULL, 'on-demand', '', 'lpsrow',
                 NULL, NULL, NULL)",
    )
    .bind(SHIPPED_HANDLE_WITH_A_ROW)
    .execute(store.pool())
    .await
    .expect("the stored row lands");
    plant_a_claim_filed_under(
        store.pool(),
        "lpsrow",
        "a claim the operator wrote on their own view",
        ("answer", "kept"),
    )
    .await;
    store.stop().await;

    for boot in 0..2 {
        let booted = boot_current(&state_dir, store_port, &git_ref).await;
        let answer = recalled_with_claims(&booted, SHIPPED_HANDLE_WITH_A_ROW).await;
        let object = &answer["objects"][0];
        assert_eq!(
            object["name"], "Stored Loops",
            "recorded at {git_ref}: boot {boot}: the stored row answers at a shipped handle: \
             {answer}"
        );
        assert_eq!(
            object["fields"]["answer"], "kept",
            "recorded at {git_ref}: boot {boot}: its field reads back: {answer}"
        );
        assert!(
            object["facts"].to_string().contains("on their own view"),
            "recorded at {git_ref}: boot {boot}: its claim reads back: {answer}"
        );
        assert_eq!(
            log_lines_naming(&booted, SHIPPED_HANDLE_WITH_A_ROW).len(),
            1,
            "recorded at {git_ref}: boot {boot}: the boot names the handle once: {:?}",
            booted.log
        );
        assert!(
            log_lines_naming(&booted, SHIPPED_HANDLE_WITHOUT_ONE).is_empty(),
            "recorded at {git_ref}: boot {boot}: a supplied handle with no row is not named"
        );
        booted.stop().await;
    }
    drop(scratch);
}

/// **What an earlier boot of a build that supplied `handle` left in the store**:
/// the record of supplied handles holds it. A store restored from before the
/// record existed has no table yet, so the schema runs first, as the boot's own
/// run would.
async fn record_as_supplied(pool: &sqlx::MySqlPool, handle: &str) {
    jojobot_adapters::dolt::migrate::run(pool)
        .await
        .expect("the schema runs");
    sqlx::query("INSERT INTO supplied_handle (handle) VALUES (?)")
        .bind(handle)
        .execute(pool)
        .await
        .expect("the handle is recorded");
}

/// A handle some earlier build supplied and this one does not.
const RETIRED_HANDLE: &str = "view:upgrade-fixture-retired";
/// A colon key with writes under it that no build ever supplied, such as a row
/// removed by hand.
const HAND_REMOVED_HANDLE: &str = "view:upgrade-fixture-hand-removed";

/// **What the operator wrote on a handle the build stopped supplying reads back
/// as an ordinary stored thing.** The writes sit filed under the handle's own
/// text with no row. The first boot gives the handle a row, takes the kind from
/// the handle and the name from its slug, and files the claim and the field under
/// the row's badge, so `recall` answers as for any thing and the next boot has
/// nothing left to do. Paired with a handle the build still supplies, which keeps
/// its writes where they are, is given no row, and is not named.
#[tokio::test]
async fn what_the_operator_wrote_on_a_handle_the_build_stopped_supplying_reads_back_as_a_thing() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(DEPLOY_4_FIXTURE_DIR, "retired-handle").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    plant_a_claim_filed_under(
        store.pool(),
        RETIRED_HANDLE,
        "a claim written while the build supplied this view",
        ("answer", "kept"),
    )
    .await;
    record_as_supplied(store.pool(), RETIRED_HANDLE).await;
    // The pair: an overlay on a handle the build still supplies, filed the same way.
    plant_a_claim_filed_under(
        store.pool(),
        SHIPPED_HANDLE_WITHOUT_ONE,
        "a claim on a view the build still supplies",
        ("answer", "still on top"),
    )
    .await;
    // The second pair: writes under a handle no build supplied.
    plant_a_claim_filed_under(
        store.pool(),
        HAND_REMOVED_HANDLE,
        "a claim under a handle nobody supplied",
        ("answer", "left"),
    )
    .await;
    store.stop().await;

    let first = boot_current(&state_dir, store_port, &git_ref).await;
    let retired = recalled_with_claims(&first, RETIRED_HANDLE).await;
    let object = &retired["objects"][0];
    assert_eq!(
        object["id"], RETIRED_HANDLE,
        "recorded at {git_ref}: the handle answers as a thing: {retired}"
    );
    assert_eq!(
        object["name"], "upgrade-fixture-retired",
        "recorded at {git_ref}: the name is the handle's slug: {retired}"
    );
    assert_eq!(
        object["fields"]["answer"], "kept",
        "recorded at {git_ref}: the field reads back: {retired}"
    );
    assert!(
        object["facts"]
            .to_string()
            .contains("while the build supplied"),
        "recorded at {git_ref}: the claim reads back: {retired}"
    );
    let shipped = recalled_with_claims(&first, SHIPPED_HANDLE_WITHOUT_ONE).await;
    assert_eq!(
        shipped["objects"][0]["fields"]["answer"], "still on top",
        "recorded at {git_ref}: a handle still supplied keeps its overlay: {shipped}"
    );
    assert_eq!(
        log_lines_naming(&first, RETIRED_HANDLE).len(),
        1,
        "recorded at {git_ref}: the first boot names the handle it gave a row: {:?}",
        first.log
    );
    assert!(
        log_lines_naming(&first, SHIPPED_HANDLE_WITHOUT_ONE).is_empty(),
        "recorded at {git_ref}: a handle still supplied is not named"
    );
    assert!(
        log_lines_naming(&first, HAND_REMOVED_HANDLE).is_empty(),
        "recorded at {git_ref}: a handle no build supplied is not named"
    );
    // The recording itself holds no handle to adopt: the one line that names a
    // handle is the planted one's.
    assert_eq!(
        first
            .log
            .iter()
            .filter(|line| line.contains("handle="))
            .count(),
        1,
        "recorded at {git_ref}: the boot gave a row to the planted handle and to no other: {:?}",
        first.log
    );
    first.stop().await;

    // The store, read with no binary over it.
    let mut store = open_the_store(&state_dir, store_port).await;
    let (kind, badge): (String, Option<String>) =
        sqlx::query_as("SELECT kind, badge FROM entity WHERE id = ?")
            .bind(RETIRED_HANDLE)
            .fetch_one(store.pool())
            .await
            .expect("the handle has a row now");
    assert_eq!(
        kind, "view",
        "recorded at {git_ref}: the kind is the handle's"
    );
    let badge = badge.expect("recorded at {git_ref}: the row wears a badge like any other");
    let filed_under_the_badge: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fact WHERE entity = ?")
            .bind(&badge)
            .fetch_one(store.pool())
            .await
            .expect("counted");
    let filed_under_the_handle: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fact WHERE entity = ?")
            .bind(RETIRED_HANDLE)
            .fetch_one(store.pool())
            .await
            .expect("counted");
    assert_eq!(
        (filed_under_the_badge, filed_under_the_handle),
        (1, 0),
        "recorded at {git_ref}: the claim moved onto the badge"
    );
    let rows_at_the_supplied_handle: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entity WHERE id = ?")
            .bind(SHIPPED_HANDLE_WITHOUT_ONE)
            .fetch_one(store.pool())
            .await
            .expect("counted");
    assert_eq!(
        rows_at_the_supplied_handle, 0,
        "recorded at {git_ref}: a handle the build supplies is given no row"
    );
    let rows_at_the_hand_removed_handle: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM entity WHERE id = ?")
            .bind(HAND_REMOVED_HANDLE)
            .fetch_one(store.pool())
            .await
            .expect("counted");
    assert_eq!(
        rows_at_the_hand_removed_handle, 0,
        "recorded at {git_ref}: a handle no build supplied is given no row"
    );
    // The boot recorded what the build supplies, so a later build that stops
    // supplying one finds it there.
    let recorded: Vec<String> = sqlx::query_scalar("SELECT handle FROM supplied_handle")
        .fetch_all(store.pool())
        .await
        .expect("the record reads");
    for shipped in [SHIPPED_HANDLE_WITH_A_ROW, SHIPPED_HANDLE_WITHOUT_ONE] {
        assert!(
            recorded.iter().any(|handle| handle == shipped),
            "recorded at {git_ref}: the boot did not record {shipped}: {recorded:?}"
        );
    }
    store.stop().await;

    let second = boot_current(&state_dir, store_port, &git_ref).await;
    assert!(
        log_lines_naming(&second, RETIRED_HANDLE).is_empty(),
        "recorded at {git_ref}: the second boot has nothing left to give a row: {:?}",
        second.log
    );
    let again = recalled_with_claims(&second, RETIRED_HANDLE).await;
    assert_eq!(
        again["objects"][0]["fields"]["answer"], "kept",
        "recorded at {git_ref}: the thing still answers on the second boot: {again}"
    );
    second.stop().await;
    drop(scratch);
}

/// A stored thing that draws an edge at the retired handle.
const DRAWING_HANDLE: &str = "thing:upgrade-fixture-drawing";

/// **Everything that names a handle the build stopped supplying follows the
/// handle's new row.** While the build supplied the handle, another thing's
/// claims drew an edge at it, a field value named it, and marks tied claims to
/// its claims, all filed under the handle's own text because a supplied record
/// has no badge. The first boot gives the handle a row and moves each of those
/// onto the row's badge, so none rests on a name that a rename frees. A walk
/// out of the drawing thing reaches the adopted thing, the field serves its
/// handle, and the stored columns hold the badge. The gate also fails the
/// case if the adopting boot logs a warning or an error.
#[tokio::test]
async fn everything_naming_a_handle_the_build_stopped_supplying_follows_the_row_the_boot_gave_it() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(DEPLOY_4_FIXTURE_DIR, "retired-edge").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    record_as_supplied(store.pool(), RETIRED_HANDLE).await;
    plant_a_claim_filed_under(
        store.pool(),
        RETIRED_HANDLE,
        "a claim written while the build supplied this view",
        ("answer", "kept"),
    )
    .await;
    sqlx::query(
        "INSERT INTO entity (id, kind, name, source, crm, parent, boot, prose, badge,
                             merged_into, archived_reason, archived_at)
         VALUES (?, 'thing', 'Drawing', 'test', NULL, NULL, 'on-demand', '', 'drwrow',
                 NULL, NULL, NULL)",
    )
    .bind(DRAWING_HANDLE)
    .execute(store.pool())
    .await
    .expect("the drawing thing lands");
    plant_a_claim_filed_under(
        store.pool(),
        "drwrow",
        "a claim that draws an edge at the retired view",
        ("drawn", "yes"),
    )
    .await;
    // The edge, as the deployed build filed it: the handle's own text.
    for table in ["fact", "fact_write"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE {table} SET edge_shape = 'about', edge_object = ? WHERE entity = 'drwrow'"
        )))
        .bind(RETIRED_HANDLE)
        .execute(store.pool())
        .await
        .expect("the edge lands");
    }
    // The other columns that name the handle, as the deployed build filed them:
    // a field on the drawing thing naming the retired view (value behind the
    // stored-handle mark, and its link row), a field on the retired view naming
    // the drawing thing (and its link row, keyed by the view's own text), and a
    // mark that ties a claim of each to a claim of the other.
    for statement in [
        "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id, written_at, write_class)
         VALUES ('drwrow', 'watches', 1, '@#view:upgrade-fixture-retired', 'f1',
                 '2026-10-08T14:02:10Z', 'sets')",
        "INSERT INTO field_link (entity, `key`, ordinal, target)
         VALUES ('drwrow', 'watches', 1, 'view:upgrade-fixture-retired')",
        "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id, written_at, write_class)
         VALUES ('view:upgrade-fixture-retired', 'holds', 1, '@#drwrow', 'f1',
                 '2026-10-08T14:02:10Z', 'sets')",
        "INSERT INTO field_link (entity, `key`, ordinal, target)
         VALUES ('view:upgrade-fixture-retired', 'holds', 1, 'drwrow')",
        "INSERT INTO fact_stands_for (fact_home, fact_id, ordinal, source_home, source_id)
         VALUES ('drwrow', 'f1', 1, 'view:upgrade-fixture-retired', 'f1')",
    ] {
        sqlx::query(statement)
            .execute(store.pool())
            .await
            .expect("the column lands");
    }
    store.stop().await;

    let first = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", first.http_port))
        .await
        .expect("connecting to the current binary");
    let answer = surface
        .call(
            "recall",
            json!({"subject": DRAWING_HANDLE, "facts": true, "follow": {"depth": 1}}),
        )
        .await;
    let walked: serde_json::Value =
        serde_json::from_str(&answer).unwrap_or_else(|_| panic!("recall said: {answer}"));
    let reached = walked["objects"][0]["connected"]
        .as_array()
        .and_then(|connected| connected.iter().find(|o| o["id"] == RETIRED_HANDLE))
        .unwrap_or_else(|| panic!("recorded at {git_ref}: the walk does not reach it: {walked}"));
    assert_eq!(
        reached["fields"]["answer"], "kept",
        "recorded at {git_ref}: the adopted thing carries the write: {reached}"
    );
    assert_eq!(
        walked["objects"][0]["fields"]["watches"], RETIRED_HANDLE,
        "recorded at {git_ref}: the field naming the adopted thing serves its handle: {walked}"
    );
    first.stop().await;

    let mut store = open_the_store(&state_dir, store_port).await;
    let badge: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind(RETIRED_HANDLE)
        .fetch_one(store.pool())
        .await
        .expect("the handle has a row now");
    let edge: String = sqlx::query_scalar("SELECT edge_object FROM fact WHERE entity = 'drwrow'")
        .fetch_one(store.pool())
        .await
        .expect("the edge reads");
    assert_eq!(
        edge, badge,
        "recorded at {git_ref}: the edge still holds the handle's text, not the badge"
    );
    for (table, column) in [
        ("field_link", "entity"),
        ("field_link", "target"),
        ("fact_stands_for", "source_home"),
    ] {
        let held: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM {table} WHERE {column} = ?"
        )))
        .bind(RETIRED_HANDLE)
        .fetch_one(store.pool())
        .await
        .expect("counted");
        assert_eq!(
            held, 0,
            "recorded at {git_ref}: {table}.{column} still holds the handle's text"
        );
    }
    let value: String = sqlx::query_scalar(
        "SELECT value FROM field_write WHERE entity = 'drwrow' AND `key` = 'watches'",
    )
    .fetch_one(store.pool())
    .await
    .expect("the field value reads");
    assert_eq!(
        value,
        format!("@#{badge}"),
        "recorded at {git_ref}: the field value still holds the handle's text"
    );
    store.stop().await;
    drop(scratch);
}

// ── What the deploy 5 build left, read by the build that follows it (card 2385).
//
// The recording at `upgrade_deploy5` was made by the deployed build (03311d93)
// through its served surface. That build kept no merge event, left pointers at a
// folded thing in the shapes recorded below, and wrote a run's history one row at
// a time.

/// The holder of every pointer the recording drew at a folded thing.
const FOLD_HOLDER: &str = "person:upgrade-fixture-person";
/// The thing the first duplicate was folded into, and the duplicate.
const FOLD_SURVIVOR: &str = "thing:upgrade-fixture-thing";
const FOLD_FOLDED: &str = "thing:recorded-twin";
/// A thing made under the first duplicate after it was folded.
const FOLD_CHILD: &str = "thing:the-fern";
/// The second duplicate, and the name its survivor was given after the fold.
const FOLD_FOLDED_TWO: &str = "thing:recorded-spare";
const FOLD_SURVIVOR_TWO_NOW: &str = "thing:jukebox";
/// The handle the second survivor wore at the fold, taken by a newcomer after the
/// rename.
const FOLD_HEIR_HANDLE: &str = "thing:recorded-heir";

/// **The claims the recording drew at a folded thing**, each by its words, with
/// where its pointer is read: the edge of the claim, the `refs` of the claim, or
/// the key it set on the holder.
const FOLD_POINTERS: [(&str, &str, &str); 8] = [
    ("keeps an eye on the twin", "edge", FOLD_SURVIVOR),
    ("mentions the twin among its refs", "refs", FOLD_SURVIVOR),
    ("watches the twin", "upgrade_fixture_watches", FOLD_SURVIVOR),
    ("holds the twin", "keeps", FOLD_SURVIVOR),
    (
        "watches the twin after the fold",
        "upgrade_fixture_watches_after",
        FOLD_SURVIVOR,
    ),
    (
        "keeps an eye on the twin after the fold",
        "edge",
        FOLD_SURVIVOR,
    ),
    (
        "watches the second twin",
        "upgrade_fixture_watches_two",
        FOLD_SURVIVOR_TWO_NOW,
    ),
    (
        "keeps an eye on the second twin",
        "edge",
        FOLD_SURVIVOR_TWO_NOW,
    ),
];

/// **The recording, read below the surface before any boot, says which pointers
/// the deployed build left at a folded thing.** Without this the cases below could
/// pass over a recording that drew none. The store holds no merge event yet: the
/// table is not there.
async fn the_fold_pointers_the_deployed_build_left(state_dir: &std::path::Path, store_port: u16) {
    let mut store = open_the_store(state_dir, store_port).await;
    let tables: Vec<String> = sqlx::query_scalar("SHOW TABLES LIKE 'merge_event'")
        .fetch_all(store.pool())
        .await
        .expect("the tables read");
    assert!(
        tables.is_empty(),
        "the deployed build kept merge events, so this recording is not the one the case is about"
    );
    for folded in [FOLD_FOLDED, FOLD_FOLDED_TWO] {
        let merged_into: Option<String> =
            sqlx::query_scalar("SELECT merged_into FROM entity WHERE id = ?")
                .bind(folded)
                .fetch_one(store.pool())
                .await
                .unwrap_or_else(|e| panic!("{folded} is in the recording: {e}"));
        assert!(
            merged_into.is_some(),
            "{folded} was folded by the deployed build and names where it went"
        );
    }
    store.stop().await;
}

/// The claim of the holder with these words, as a recall serves it.
fn the_claim_saying<'a>(read: &'a serde_json::Value, words: &str) -> &'a serde_json::Value {
    read["objects"][0]["facts"]
        .as_array()
        .and_then(|facts| facts.iter().find(|fact| fact["content"] == words))
        .unwrap_or_else(|| panic!("the holder has no claim saying {words:?}: {read}"))
}

/// **A pointer at a thing the deployed build folded reads as the thing it was
/// folded into, after the first boot of the build that follows it.** The deployed
/// build kept no merge event, so the rows it left are all there is to go by: an
/// edge, the refs of a claim, and a key set on the holder, written before the fold
/// and after it, by the handle the duplicate went on answering to, and a fold
/// whose survivor was renamed afterwards.
#[tokio::test]
async fn a_pointer_at_a_thing_the_deployed_build_folded_reads_as_the_survivor_after_the_boot() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(DEPLOY_5_FIXTURE_DIR, "fold-pointers").await;
    the_fold_pointers_the_deployed_build_left(&state_dir, store_port).await;

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let read: serde_json::Value = serde_json::from_str(
        &surface
            .call("recall", json!({"subject": FOLD_HOLDER, "facts": true}))
            .await,
    )
    .expect("the holder reads");
    let child: serde_json::Value =
        serde_json::from_str(&surface.call("recall", json!({"subject": FOLD_CHILD})).await)
            .expect("the child reads");
    booted.stop().await;

    let fields = &read["objects"][0]["fields"];
    let mut wrong = Vec::new();
    if child["objects"][0]["parent"] != FOLD_SURVIVOR {
        wrong.push(format!(
            "{FOLD_CHILD} reads {} as its parent, not {FOLD_SURVIVOR}",
            child["objects"][0]["parent"]
        ));
    }
    for (words, place, survivor) in FOLD_POINTERS {
        let claim = the_claim_saying(&read, words);
        let (named, how) = match place {
            "edge" => (claim["edge"]["object"].clone(), "its edge"),
            "refs" => (claim["refs"][0].clone(), "its first ref"),
            key => (fields[key].clone(), "the key it set on the holder"),
        };
        if named != survivor {
            wrong.push(format!(
                "{words:?} reads {named} through {how}, not {survivor}"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "recorded at {git_ref}: a pointer at a thing the deployed build folded is not read as \
         the thing it went into: {wrong:#?}"
    );
    drop(scratch);
}

/// The thing the recording made under a record the build supplies.
const SUPPLIED_CHILD: &str = "thing:kettle";

/// **A pointer at a record the build supplies is valid, and the boot says nothing
/// about it.** The recording holds an edge, a ref and a parent at shipped views,
/// which have no row of their own. A boot that read one as a stale handle would
/// log an error naming it, and `boot_current` holds every boot to no error line.
/// The pointers also read back as they were written, so a boot that dropped them
/// to stay quiet is not a pass.
#[tokio::test]
async fn a_pointer_at_a_record_the_build_supplies_boots_quiet_and_reads_back() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(DEPLOY_5_FIXTURE_DIR, "supplied-pointers").await;
    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let read: serde_json::Value = serde_json::from_str(
        &surface
            .call("recall", json!({"subject": FOLD_HOLDER, "facts": true}))
            .await,
    )
    .expect("the holder reads");
    let child: serde_json::Value = serde_json::from_str(
        &surface
            .call("recall", json!({"subject": SUPPLIED_CHILD}))
            .await,
    )
    .expect("the child reads");
    booted.stop().await;

    assert_eq!(
        the_claim_saying(&read, "keeps an eye on the colleagues view")["edge"]["object"],
        "view:colleagues",
        "recorded at {git_ref}: {read}"
    );
    assert_eq!(
        the_claim_saying(&read, "mentions the loops view among its refs")["refs"][0],
        "view:loops",
        "recorded at {git_ref}: {read}"
    );
    assert_eq!(
        child["objects"][0]["parent"], "view:ready",
        "recorded at {git_ref}: {child}"
    );
    drop(scratch);
}

/// **A reference-typed field that names a record the build supplies is valid, and
/// a boot that owes the reference pass says nothing about it.** The recording
/// holds such a field, on a key declared to hold a reference to anything. The
/// deployed build had already run the pass, so its marker is dropped first: a
/// store that has not run it is one that last booted on a build before the pass.
/// The record has no row to resolve through, and a boot that read the value as a
/// stale handle would log an error naming it. The field reads back as the record.
#[tokio::test]
async fn a_reference_field_naming_a_supplied_record_boots_quiet_and_reads_back() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(DEPLOY_5_FIXTURE_DIR, "supplied-reference").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let held: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM field_write WHERE `key` = 'looks_at'")
        .fetch_one(store.pool())
        .await
        .expect("the field writes count");
    let dropped = sqlx::query("DELETE FROM store_pass WHERE name = 'reference field lowering'")
        .execute(store.pool())
        .await
        .expect("the pass is owed again")
        .rows_affected();
    store.stop().await;
    assert_eq!(
        (held, dropped),
        (1, 1),
        "recorded at {git_ref}: the recording holds the field, and the pass had run"
    );

    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
        .await
        .expect("connecting to the current binary");
    let read: serde_json::Value = serde_json::from_str(
        &surface
            .call("recall", json!({"subject": FOLD_HOLDER}))
            .await,
    )
    .expect("the holder reads");
    booted.stop().await;
    assert_eq!(
        read["objects"][0]["fields"]["looks_at"], "view:colleagues",
        "recorded at {git_ref}: {read}"
    );
    drop(scratch);
}

/// **The first boot over a store the deployed build filled says nothing is wrong,
/// and the second boot logs the same.** The store's pass markers are set, so the
/// passes that already ran take their settled branch, and the passes this build
/// adds run once. `boot_current` holds every boot to no warning and no error
/// line; this case names the property and boots until it has seen it three times.
#[tokio::test]
async fn the_boots_over_a_store_the_deployed_build_filled_log_no_warning_and_no_error() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(DEPLOY_5_FIXTURE_DIR, "no-noise").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let marked: Vec<String> = sqlx::query_scalar("SELECT name FROM store_pass ORDER BY name")
        .fetch_all(store.pool())
        .await
        .expect("the pass markers read");
    store.stop().await;
    assert!(
        marked.iter().any(|name| name == "field value lowering")
            && marked.iter().any(|name| name == "reference field lowering"),
        "the recording holds the markers the deployed build's own boot set: {marked:?}"
    );
    let mut dumps = Vec::new();
    for boot in 1..=3 {
        let booted = boot_current(&state_dir, store_port, &git_ref).await;
        let log = booted.log.clone();
        booted.stop().await;
        let noisy = degradations(&log, false, &[]);
        assert!(
            noisy.is_empty(),
            "boot {boot} over the store recorded at {git_ref} logged: {noisy:?}"
        );
        dumps.push(dump_store(&state_dir).await);
    }
    // **And a boot that has nothing left to do writes nothing**: the second and
    // the third leave the store as the first left it.
    assert!(
        dumps[1] == dumps[0] && dumps[2] == dumps[1],
        "a later boot over the store recorded at {git_ref} changed it: {} lines after the \
         first boot, {} after the second, {} after the third",
        dumps[0].lines().count(),
        dumps[1].lines().count(),
        dumps[2].lines().count()
    );
    drop(scratch);
}

/// **A run with hundreds of the deployed build's per-write rows boots inside the
/// time a boot is given.** The recording holds one run with a long history. The
/// case states the size of it from the store, boots, and holds the boot to the
/// time the gate gives a boot (the serving line has to appear) and to half of it,
/// so a boot that grows with a run's history moves this case before it moves the
/// deadline.
#[tokio::test]
async fn a_boot_over_a_run_with_a_long_history_stays_inside_the_time_a_boot_is_given() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(DEPLOY_5_FIXTURE_DIR, "long-history").await;
    let mut store = open_the_store(&state_dir, store_port).await;
    let (longest_run, rows): (String, i64) = sqlx::query_as(
        "SELECT session, COUNT(*) AS n FROM session_write GROUP BY session ORDER BY n DESC LIMIT 1",
    )
    .fetch_one(store.pool())
    .await
    .expect("the longest run reads");
    store.stop().await;
    assert!(
        rows >= 200,
        "the recording holds a run with a long history, or this proves nothing: {longest_run} \
         holds {rows} rows"
    );

    let began = std::time::Instant::now();
    let booted = boot_current(&state_dir, store_port, &git_ref).await;
    let took = began.elapsed();
    booted.stop().await;
    assert!(
        took < jojobot_exercise::room::READY_TIMEOUT / 2,
        "the boot over a run of {rows} per-write rows took {took:?}, over half of the {:?} a boot \
         is given",
        jojobot_exercise::room::READY_TIMEOUT
    );
    drop(scratch);
}

/// **A fold whose survivor was renamed, beside a new thing that took the handle
/// the rename freed.** The second duplicate names the handle its survivor wore at
/// the fold. Two things have worn that handle since: the survivor, before the
/// rename, and a newcomer, after it. The fold came first, so the survivor is the
/// one it meant; the boots place the forward without guessing, log no repair
/// warning, and a pointer at the duplicate reads as the survivor and not as the
/// newcomer.
#[tokio::test]
async fn a_fold_naming_a_handle_two_things_wore_is_placed_and_the_boots_are_quiet() {
    let Restored {
        scratch,
        state_dir,
        store_port,
        git_ref,
        ..
    } = restore_a_fixture(DEPLOY_5_FIXTURE_DIR, "reused-handle").await;
    // **Stated before any boot**, so a recording that holds no such handle fails
    // here and not as a green case: the handle is worn now by one thing and was
    // worn by another, and the duplicate still names it.
    let mut store = open_the_store(&state_dir, store_port).await;
    let (wearers_now, wearers_before): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM entity WHERE id = ?),
                (SELECT COUNT(*) FROM entity_former_handle WHERE former_handle = ?)",
    )
    .bind(FOLD_HEIR_HANDLE)
    .bind(FOLD_HEIR_HANDLE)
    .fetch_one(store.pool())
    .await
    .expect("the wearers of the handle read");
    let named: Option<String> = sqlx::query_scalar("SELECT merged_into FROM entity WHERE id = ?")
        .bind(FOLD_FOLDED_TWO)
        .fetch_one(store.pool())
        .await
        .expect("the second duplicate reads");
    store.stop().await;
    assert_eq!(
        (wearers_now, wearers_before, named.as_deref()),
        (1, 1, Some(FOLD_HEIR_HANDLE)),
        "recorded at {git_ref}: the handle is not worn by one thing and once by another, or the \
         duplicate does not name it"
    );

    for boot in 1..=2 {
        let booted = boot_current(&state_dir, store_port, &git_ref).await;
        let surface = Surface::connect(&format!("http://127.0.0.1:{}/mcp", booted.http_port))
            .await
            .expect("connecting to the current binary");
        let read: serde_json::Value = serde_json::from_str(
            &surface
                .call("recall", json!({"subject": FOLD_HOLDER, "facts": true}))
                .await,
        )
        .expect("the holder reads");
        booted.stop().await;
        let edge =
            the_claim_saying(&read, "keeps an eye on the second twin")["edge"]["object"].clone();
        assert_eq!(
            edge, FOLD_SURVIVOR_TWO_NOW,
            "recorded at {git_ref}: boot {boot} reads the edge at the second duplicate as {edge}, \
             not as the survivor: {read}"
        );
    }
    drop(scratch);
}
