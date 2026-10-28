//! **A run the store cannot read is never answered with retry, whichever field
//! holds the damage and whichever path meets it.** The summary the write
//! judgement reads names a damaged start, state or stated day; these cases
//! damage a cell the summary does not read, in a row the real store holds, and
//! expect the same answer: `person`, nothing written. An outage of the store
//! stays `retry`.
//!
//! Each case runs over the real store, which only CI starts (rule 622).

use super::testing::*;
use crate::Jojobot;
use crate::harness::*;
use crate::memory::testing::{capture_args, ensure};
use crate::session::WrapSessionArgs;
use jojobot_domain::session::Sessions;
use rmcp::handler::server::wrapper::Parameters;
use sqlx::Row;
use std::sync::Arc;

struct World {
    jojobot: Jojobot,
    memory: Arc<jojobot_domain::memory::testing::InMemoryMemory>,
    pool: sqlx::MySqlPool,
    sessions: Arc<jojobot_adapters::dolt::sessions::DoltSessions>,
    store: jojobot_adapters::dolt::Dolt,
    _scratch: jojobot_adapters::testing::Scratch,
}

async fn world(tag: &str) -> World {
    let scratch = jojobot_adapters::testing::Scratch::new(tag);
    let store = jojobot_adapters::testing::start_unhurried(
        &scratch.0,
        jojobot_adapters::testing::free_port(),
    )
    .await
    .expect("the real store comes up");
    let pool = jojobot_adapters::testing::migrated_database(&store, "every_field")
        .await
        .expect("a database of this case's own");
    let sessions = Arc::new(jojobot_adapters::dolt::sessions::DoltSessions::open(
        pool.clone(),
    ));
    let (jojobot, memory) = with_sessions_port_and_memory(sessions.clone());
    make_bot(&jojobot, "gamma").await;
    ensure(&jojobot, "thread:alpha").await;
    World {
        jojobot,
        memory,
        pool,
        sessions,
        store,
        _scratch: scratch,
    }
}

async fn capture_as(jojobot: &Jojobot, key: &str, content: &str) -> serde_json::Value {
    json_of(
        &jojobot
            .capture(Parameters(crate::memory::CaptureArgs {
                sid: Some(key.to_string()),
                ..capture_args("thread:alpha", content)
            }))
            .await
            .expect("capture answers"),
    )
}

async fn journal_as(jojobot: &Jojobot, key: &str, entry: &str) -> serde_json::Value {
    json_of(
        &jojobot
            .journal(Parameters(crate::session::JournalArgs {
                entry: entry.into(),
                focus: None,
                working_on: None,
                sid: key.to_string(),
                request_id: None,
            }))
            .await
            .expect("journal answers"),
    )
}

/// The refusal a damaged run earns: nothing written, and a person's to repair.
/// It names the field that is damaged and never carries the damaged cell, which
/// is a sentinel no sentence of ours contains, and it carries no key.
fn assert_a_person_repairs(refused: &serde_json::Value, sentinel: &str, field: &str) {
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert_eq!(refused["wrote"], false, "{refused}");
    assert_eq!(refused["fix_by"], "person", "{refused}");
    let said = refused.to_string();
    assert!(
        !said.contains(sentinel),
        "the damaged cell was echoed: {said}"
    );
    assert!(
        said.contains(field),
        "the damaged field is not named: {said}"
    );
    assert!(
        !said.contains(jojobot_domain::session::KEY_PREFIX),
        "a refusal for damage carries no key: {said}"
    );
}

/// **The refusal names the run the read stopped at**, so the repair is aimed at
/// it. The run is the bot's only one, read from its row.
async fn assert_names_the_run(refused: &serde_json::Value, pool: &sqlx::MySqlPool) {
    let run: String = sqlx::query_scalar("SELECT id FROM session WHERE bot = 'bot:gamma'")
        .fetch_one(pool)
        .await
        .expect("the bot has one run");
    assert!(
        refused.to_string().contains(&run),
        "the refusal does not name run {run}: {refused}"
    );
}

/// **Two wrapped runs of one bot, the start of the newer one damaged.** The older
/// run reads and is the control; the newer one holds a wrap code that is still
/// valid. Returns the world, the code, the damaged run and the control.
async fn bot_with_a_damaged_start(tag: &str) -> (World, String, String, String) {
    let world = world(tag).await;
    // The older run, wrapped and readable: the control.
    let first = booted(&world.jojobot, "gamma").await;
    journal_entry(&world.jojobot, &first, "started").await;
    json_of(
        &world
            .jojobot
            .wrap_session(Parameters(WrapSessionArgs {
                story: "first story".into(),
                sid: first,
            }))
            .await
            .expect("wrap answers"),
    );
    let control: String = sqlx::query_scalar("SELECT id FROM session WHERE bot = 'bot:gamma'")
        .fetch_one(&world.pool)
        .await
        .expect("the bot has one run");
    // The newer run, wrapped with a code that is still valid.
    let second = booted(&world.jojobot, "gamma").await;
    journal_entry(&world.jojobot, &second, "started again").await;
    let wrapped = json_of(
        &world
            .jojobot
            .wrap_session(Parameters(WrapSessionArgs {
                story: "second story".into(),
                sid: second,
            }))
            .await
            .expect("wrap answers"),
    );
    let code = wrapped["wrap_code"].as_str().expect("a code").to_string();
    let damaged_run: String =
        sqlx::query_scalar("SELECT id FROM session WHERE bot = 'bot:gamma' AND id <> ?")
            .bind(&control)
            .fetch_one(&world.pool)
            .await
            .expect("the bot has a second run");

    let damaged = sqlx::query("UPDATE session SET started_at = ? WHERE id = ?")
        .bind(DAMAGED_MOMENT)
        .bind(&damaged_run)
        .execute(&world.pool)
        .await
        .expect("the start is damaged");
    assert_eq!(damaged.rows_affected(), 1, "one run is damaged");

    (world, code, damaged_run, control)
}

/// **A boot over a start time that does not read names the run and the field.**
/// The runs of a bot are sorted by their start before any is read in full, so
/// this damage meets the sort and not the read of a run. The refusal is a
/// person's, wrote nothing, carries no cell and no key, names the damaged run and
/// its start time, and does not name the readable control.
fn assert_names_the_damaged_start(answer: &serde_json::Value, damaged_run: &str, control: &str) {
    assert_a_person_repairs(answer, DAMAGED_MOMENT, "its start time");
    let said = answer.to_string();
    assert!(
        said.contains(damaged_run),
        "the refusal does not name run {damaged_run}: {said}"
    );
    assert!(
        !said.contains(control),
        "the refusal names the readable run {control}: {said}"
    );
}

/// **The ordinary boot over a damaged start time.**
#[tokio::test]
async fn over_the_real_store_an_ordinary_boot_over_a_damaged_start_time_names_the_run() {
    let (mut world, _code, damaged_run, control) =
        bot_with_a_damaged_start("every-field-started-at-boot").await;
    let answer = boot_answering(&world.jojobot, "gamma", "new").await;
    assert_names_the_damaged_start(&answer, &damaged_run, &control);
    world.store.stop().await;
}

/// **The boot with a valid wrap code over a damaged start time.** The code is its
/// own route and reads the runs of the bot itself, so it meets the sort too.
#[tokio::test]
async fn over_the_real_store_a_wrap_code_boot_over_a_damaged_start_time_names_the_run() {
    let (mut world, code, damaged_run, control) =
        bot_with_a_damaged_start("every-field-started-at-code").await;
    let answer = boot_answering(&world.jojobot, "gamma", &code).await;
    assert_names_the_damaged_start(&answer, &damaged_run, &control);
    world.store.stop().await;
}

/// Two cells no sentence of ours contains, for the two kinds of cell damaged.
const DAMAGED_MOMENT: &str = "zz-damaged-moment-cell";
const DAMAGED_DAY: &str = "notaday";
/// A change kind no build knows, as it would sit in the `changed` column.
const UNKNOWN_KIND: &str = "zzkind";

async fn claims_on_alpha(memory: &jojobot_domain::memory::testing::InMemoryMemory) -> usize {
    use jojobot_domain::memory::Memory;
    memory
        .recall(&jojobot_domain::memory::EntityId("thread:alpha".into()))
        .await
        .expect("the thread reads")
        .len()
}

/// A wrapped run reopened by its code, then the key hands back its key.
async fn reopened_run(world: &World) -> String {
    let key = booted(&world.jojobot, "gamma").await;
    journal_entry(&world.jojobot, &key, "started").await;
    let wrapped = json_of(
        &world
            .jojobot
            .wrap_session(Parameters(WrapSessionArgs {
                story: "first story".into(),
                sid: key,
            }))
            .await
            .expect("wrap answers"),
    );
    let code = wrapped["wrap_code"].as_str().expect("a code").to_string();
    let reopened = boot_answering(&world.jojobot, "gamma", &code).await;
    reopened["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("the code reopens the run: {reopened}"))
        .to_string()
}

/// **(a) A wrapped run whose summary reads and whose full read fails is a
/// person's.** The damaged cell is the moment the charter was told, which the
/// summary does not read.
#[tokio::test]
async fn over_the_real_store_a_wrapped_run_damaged_beyond_the_summary_refuses_for_a_person() {
    let mut world = world("every-field-wrapped").await;
    let key = reopened_run(&world).await;
    let inside = capture_as(&world.jojobot, &key, "the bus leaves at eight").await;
    assert_ne!(inside["status"], "blocked", "{inside}");
    let before = claims_on_alpha(&world.memory).await;

    let damaged = sqlx::query("UPDATE session SET charter_told_at = ? WHERE bot = 'bot:gamma'")
        .bind(DAMAGED_MOMENT)
        .execute(&world.pool)
        .await
        .expect("the row is damaged");
    assert_eq!(damaged.rows_affected(), 1, "one run is damaged");

    let refused = capture_as(&world.jojobot, &key, "the bus leaves at nine").await;
    assert_a_person_repairs(&refused, DAMAGED_MOMENT, "the moment its charter was told");
    assert_eq!(
        claims_on_alpha(&world.memory).await,
        before,
        "nothing landed"
    );
    // The boot reads every run of the bot, so the damage reaches it too, and it
    // is a person's rather than an outage the next boot outlasts.
    let booted = boot_answering(&world.jojobot, "gamma", "new").await;
    assert_a_person_repairs(&booted, DAMAGED_MOMENT, "the moment its charter was told");
    assert_names_the_run(&booted, &world.pool).await;
    world.store.stop().await;
}

/// **The boot with a wrap code is its own route, and it meets the same damage.**
/// The run wrapped, then a cell the summary does not read is damaged, and the
/// code that would reopen it is a person's to repair, not an outage. The paired
/// positive is the first case above, which reopens an undamaged run by its code.
#[tokio::test]
async fn over_the_real_store_a_wrap_code_over_a_damaged_run_refuses_for_a_person() {
    let mut world = world("every-field-wrap-code").await;
    let key = booted(&world.jojobot, "gamma").await;
    journal_entry(&world.jojobot, &key, "started").await;
    let wrapped = json_of(
        &world
            .jojobot
            .wrap_session(Parameters(WrapSessionArgs {
                story: "first story".into(),
                sid: key,
            }))
            .await
            .expect("wrap answers"),
    );
    let code = wrapped["wrap_code"].as_str().expect("a code").to_string();
    let damaged = sqlx::query("UPDATE session SET charter_told_at = ? WHERE bot = 'bot:gamma'")
        .bind(DAMAGED_MOMENT)
        .execute(&world.pool)
        .await
        .expect("the row is damaged");
    assert_eq!(damaged.rows_affected(), 1, "one run is damaged");

    let refused = boot_answering(&world.jojobot, "gamma", &code).await;
    assert_a_person_repairs(&refused, DAMAGED_MOMENT, "the moment its charter was told");
    assert_names_the_run(&refused, &world.pool).await;
    world.store.stop().await;
}

/// **(b) An active run damaged in a journal entry's day refuses its writes for
/// a person, on both the session verb and the memory verb**, and so does the
/// same run once it is abandoned. A readable run takes the same writes first.
#[tokio::test]
async fn over_the_real_store_an_active_run_with_a_damaged_entry_refuses_for_a_person() {
    let mut world = world("every-field-active").await;
    let key = booted(&world.jojobot, "gamma").await;
    let first = journal_entry(&world.jojobot, &key, "started").await;
    let run = first["session"].as_str().expect("the run").to_string();
    let inside = capture_as(&world.jojobot, &key, "the bus leaves at eight").await;
    assert_ne!(inside["status"], "blocked", "{inside}");
    let claims = claims_on_alpha(&world.memory).await;

    let damaged = sqlx::query("UPDATE journal_entry SET happened_on = ? WHERE session = ?")
        .bind(DAMAGED_DAY)
        .bind(&run)
        .execute(&world.pool)
        .await
        .expect("the entry is damaged");
    assert!(damaged.rows_affected() >= 1, "an entry is damaged");
    let count = || async {
        sqlx::query("SELECT COUNT(*) AS n FROM journal_entry WHERE session = ?")
            .bind(&run)
            .fetch_one(&world.pool)
            .await
            .expect("the journal counts")
            .get::<i64, _>("n")
    };
    let entries = count().await;

    for state in ["active", "abandoned"] {
        sqlx::query("UPDATE session SET state = ? WHERE id = ?")
            .bind(state)
            .bind(&run)
            .execute(&world.pool)
            .await
            .expect("the state is set");
        let journaled = journal_as(&world.jojobot, &key, "the bus leaves at nine").await;
        assert_a_person_repairs(&journaled, DAMAGED_DAY, "one of its beats' day");
        let captured = capture_as(&world.jojobot, &key, "the bus leaves at ten").await;
        assert_a_person_repairs(&captured, DAMAGED_DAY, "one of its beats' day");
        assert_eq!(count().await, entries, "{state}: no beat was appended");
        assert_eq!(
            claims_on_alpha(&world.memory).await,
            claims,
            "{state}: no claim landed"
        );
    }
    world.store.stop().await;
}

/// **(d) A compaction row that does not read makes ITS run unreadable, a
/// person's, and hides no other run.** The damaged cell is the moment a
/// compaction was written, read when the summaries of the runs are listed. The
/// refusal names the field and carries no cell; the write does not land; the
/// healthy run beside it is still listed. The paired positive is the same write
/// before the row is damaged.
#[tokio::test]
async fn over_the_real_store_a_damaged_compaction_row_names_its_run_and_hides_no_other() {
    let mut world = world("every-field-compaction").await;
    make_bot(&world.jojobot, "delta").await;
    let key = booted(&world.jojobot, "gamma").await;
    let first = journal_entry(&world.jojobot, &key, "started").await;
    let run = first["session"].as_str().expect("the run").to_string();
    let other = booted(&world.jojobot, "delta").await;
    let second = journal_entry(&world.jojobot, &other, "started").await;
    let other_run = second["session"]
        .as_str()
        .expect("the other run")
        .to_string();
    let inside = capture_as(&world.jojobot, &key, "the bus leaves at eight").await;
    assert_ne!(inside["status"], "blocked", "{inside}");
    let claims = claims_on_alpha(&world.memory).await;

    sqlx::query(
        "INSERT INTO session_write (session, ordinal, written_at, changed, value) \
         VALUES (?, 1000, ?, 'served_reset', '0')",
    )
    .bind(&run)
    .bind(DAMAGED_MOMENT)
    .execute(&world.pool)
    .await
    .expect("the table takes the row");

    let refused = capture_as(&world.jojobot, &key, "the bus leaves at nine").await;
    assert_a_person_repairs(&refused, DAMAGED_MOMENT, "one of its changes' time");
    assert_eq!(
        claims_on_alpha(&world.memory).await,
        claims,
        "nothing landed"
    );

    let listed = world
        .sessions
        .all_summaries()
        .await
        .expect("the damaged compaction does not fail the listing");
    let find = |id: &str| listed.iter().find(|summary| summary.id.as_str() == id);
    let damaged = find(&run).expect("the damaged run is listed");
    assert!(
        damaged
            .unreadable
            .as_deref()
            .is_some_and(|why| why.contains("one of its changes' time")),
        "{damaged:?}"
    );
    let healthy = find(&other_run).expect("the healthy run is still listed");
    assert!(healthy.unreadable.is_none(), "{healthy:?}");
    world.store.stop().await;
}

/// **(e) A stored change of a kind this build does not know is a person's, and
/// hides no other run.** The run's history cannot be read: a recall of it names
/// the damaged field and carries no cell. The paired positive is the same
/// history before the row is added, and the history of a healthy run beside it.
#[tokio::test]
async fn over_the_real_store_an_unknown_change_kind_names_its_run_for_a_person_and_hides_no_other()
{
    let mut world = world("every-field-change-kind").await;
    make_bot(&world.jojobot, "delta").await;
    let key = booted(&world.jojobot, "gamma").await;
    let first = journal_entry(&world.jojobot, &key, "started").await;
    let run = first["session"].as_str().expect("the run").to_string();
    let other = booted(&world.jojobot, "delta").await;
    let second = journal_entry(&world.jojobot, &other, "started").await;
    let other_run = second["session"]
        .as_str()
        .expect("the other run")
        .to_string();
    // **The reader is a healthy, present run of the same bot**; the damaged one is
    // its older run. The refusal is about the older run and must not blame this one.
    let present = booted_in(&world.jojobot, "gamma", "UTC", Some("new")).await;
    let third = journal_entry(&world.jojobot, &present, "started").await;
    let present_run = third["session"]
        .as_str()
        .expect("the present run")
        .to_string();
    assert_ne!(present_run, run, "two runs of the bot");
    let history = |run: String, key: String| {
        let jojobot = world.jojobot.clone();
        async move {
            json_of(
                &jojobot
                    .recall(Parameters(crate::memory::RecallArgs {
                        sid: Some(key),
                        history: Some("focus".into()),
                        ..crate::memory::testing::recall_args(&format!("session:{run}"))
                    }))
                    .await
                    .expect("recall answers"),
            )
        }
    };
    let before = history(run.clone(), present.clone()).await;
    assert!(before["objects"][0].get("history").is_some(), "{before}");

    sqlx::query(
        "INSERT INTO session_write (session, ordinal, written_at, changed, value) \
         VALUES (?, 1000, '2026-09-02T00:00:00Z', ?, 'x')",
    )
    .bind(&run)
    .bind(UNKNOWN_KIND)
    .execute(&world.pool)
    .await
    .expect("the table takes the row");

    let refused = history(run.clone(), present.clone()).await;
    assert_a_person_repairs(&refused, UNKNOWN_KIND, "of a kind this build does not know");
    // **The repair is directed at the damaged run**, not at the run that asked.
    let present_name: String = sqlx::query("SELECT sid FROM session WHERE id = ?")
        .bind(&present_run)
        .fetch_one(&world.pool)
        .await
        .expect("the present run's name")
        .get("sid");
    let said = refused["how_to_proceed"].as_str().expect("a way forward");
    assert!(
        said.contains(&run),
        "the damaged run is not named: {refused}"
    );
    assert!(
        !said.contains(&present_name) && !said.contains(&present_run),
        "the run that asked is blamed: {refused}"
    );
    let healthy = history(other_run.clone(), other).await;
    assert!(healthy["objects"][0].get("history").is_some(), "{healthy}");
    let listed = world.sessions.all_summaries().await.expect("the runs list");
    for id in [&run, &other_run] {
        assert!(
            listed.iter().any(|summary| summary.id.as_str() == id),
            "{id} is listed"
        );
    }
    world.store.stop().await;
}

/// **The control: a store that cannot be reached is still `retry`.** The same
/// wrapped run, the same write, a store that answers nothing.
#[tokio::test]
async fn over_the_real_store_an_unreachable_store_is_still_retry() {
    let mut world = world("every-field-outage").await;
    let key = reopened_run(&world).await;
    let inside = capture_as(&world.jojobot, &key, "the bus leaves at eight").await;
    assert_ne!(inside["status"], "blocked", "{inside}");
    world.store.stop().await;

    let refused = capture_as(&world.jojobot, &key, "the bus leaves at nine").await;
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert_eq!(refused["wrote"], false, "{refused}");
    assert_eq!(refused["fix_by"], "retry", "{refused}");
}
