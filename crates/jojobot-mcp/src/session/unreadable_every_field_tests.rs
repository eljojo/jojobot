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
fn assert_a_person_repairs(refused: &serde_json::Value) {
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert_eq!(refused["wrote"], false, "{refused}");
    assert_eq!(refused["fix_by"], "person", "{refused}");
}

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

    let damaged =
        sqlx::query("UPDATE session SET charter_told_at = 'no time' WHERE bot = 'bot:gamma'")
            .execute(&world.pool)
            .await
            .expect("the row is damaged");
    assert_eq!(damaged.rows_affected(), 1, "one run is damaged");

    let refused = capture_as(&world.jojobot, &key, "the bus leaves at nine").await;
    assert_a_person_repairs(&refused);
    assert_eq!(
        claims_on_alpha(&world.memory).await,
        before,
        "nothing landed"
    );
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

    let damaged = sqlx::query("UPDATE journal_entry SET happened_on = 'no day' WHERE session = ?")
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
        assert_a_person_repairs(&journaled);
        let captured = capture_as(&world.jojobot, &key, "the bus leaves at ten").await;
        assert_a_person_repairs(&captured);
        assert_eq!(count().await, entries, "{state}: no beat was appended");
        assert_eq!(
            claims_on_alpha(&world.memory).await,
            claims,
            "{state}: no claim landed"
        );
    }
    world.store.stop().await;
}

/// **(c) The exchange of a keyless run's name over a run the store cannot read
/// is a person's.** The key is neither handed out nor changed.
#[tokio::test]
async fn over_the_real_store_the_key_exchange_of_a_damaged_run_refuses_for_a_person() {
    let mut world = world("every-field-exchange").await;
    let key = booted(&world.jojobot, "gamma").await;
    journal_entry(&world.jojobot, &key, "started").await;
    sqlx::query("UPDATE session SET key_hash = NULL WHERE bot = 'bot:gamma'")
        .execute(&world.pool)
        .await
        .expect("the run is keyless");
    let name: String = sqlx::query("SELECT sid FROM session WHERE bot = 'bot:gamma'")
        .fetch_one(&world.pool)
        .await
        .expect("the run's name")
        .get("sid");
    // A restart: the registry is rebuilt from the board, which holds the run as
    // keyless.
    let registry = Arc::new(crate::sid::SessionRegistry::new());
    let board = world
        .sessions
        .all_sessions()
        .await
        .expect("the board reads");
    registry.rebuild_from(&board);
    let restarted = Jojobot::new(
        world.memory.clone(),
        Arc::new(crate::memory::testing::SpySearch::default()),
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
        world.sessions.clone(),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        registry,
    );

    sqlx::query("UPDATE session SET charter_told_at = 'no time' WHERE bot = 'bot:gamma'")
        .execute(&world.pool)
        .await
        .expect("the row is damaged");
    let refused = journal_as(&restarted, &name, "the bus leaves at nine").await;
    assert_a_person_repairs(&refused);
    let held: Option<String> = sqlx::query("SELECT key_hash FROM session WHERE bot = 'bot:gamma'")
        .fetch_one(&world.pool)
        .await
        .expect("the run reads")
        .get("key_hash");
    assert!(held.is_none(), "no key was handed out");
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
