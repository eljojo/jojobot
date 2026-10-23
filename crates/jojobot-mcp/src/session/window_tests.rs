//! **A wrapped run the real store cannot read takes no write.** The Dolt store
//! names a run whose row does not parse as an unreadable summary, which it files
//! as abandoned whatever state the row holds. The window judge reads the summary,
//! so it must refuse on the damage, as its own doc says, rather than read the
//! state the store chose for a run it could not read.

use super::testing::*;
use crate::harness::*;
use crate::memory::testing::{capture_args, ensure};
use crate::session::WrapSessionArgs;
use jojobot_domain::session::Sessions;
use rmcp::handler::server::wrapper::Parameters;
use std::sync::Arc;

#[tokio::test]
async fn over_the_real_store_a_wrapped_run_that_cannot_be_read_refuses_a_write_for_a_person() {
    let scratch = jojobot_adapters::testing::Scratch::new("unreadable-wrapped-run");
    let mut store = jojobot_adapters::testing::start_unhurried(
        &scratch.0,
        jojobot_adapters::testing::free_port(),
    )
    .await
    .expect("the real store comes up");
    let pool = jojobot_adapters::testing::migrated_database(&store, "window")
        .await
        .expect("a database of this case's own");
    let sessions = Arc::new(jojobot_adapters::dolt::sessions::DoltSessions::open(
        pool.clone(),
    ));
    let (jojobot, _memory) = with_sessions_port_and_memory(sessions.clone());
    make_bot(&jojobot, "gamma").await;
    ensure(&jojobot, "thread:alpha").await;
    let first_key = booted(&jojobot, "gamma").await;
    journal_entry(&jojobot, &first_key, "started").await;
    let first = json_of(
        &jojobot
            .wrap_session(Parameters(WrapSessionArgs {
                story: "first story".into(),
                sid: first_key,
            }))
            .await
            .expect("wrap answers"),
    );
    let code = first["wrap_code"].as_str().expect("a code").to_string();
    let reopened = boot_answering(&jojobot, "gamma", &code).await;
    let key = reopened["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("the code reopens the run: {reopened}"))
        .to_string();
    let capture = |content: &str| {
        let jojobot = jojobot.clone();
        let key = key.clone();
        let content = content.to_string();
        async move {
            json_of(
                &jojobot
                    .capture(Parameters(crate::memory::CaptureArgs {
                        sid: Some(key),
                        ..capture_args("thread:alpha", &content)
                    }))
                    .await
                    .expect("capture answers"),
            )
        }
    };
    // **Paired**: inside its window the readable run takes the write.
    let inside = capture("the bus leaves at eight").await;
    assert_ne!(inside["status"], "blocked", "{inside}");

    let damaged = sqlx::query("UPDATE session SET started_at = 'no time' WHERE bot = 'bot:gamma'")
        .execute(&pool)
        .await
        .expect("the row is damaged");
    assert_eq!(damaged.rows_affected(), 1, "one run is damaged");

    let refused = capture("the bus leaves at nine").await;
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert_eq!(refused["wrote"], false, "{refused}");
    assert_eq!(refused["fix_by"], "person", "{refused}");
    let run = first["session"]["id"].as_str().expect("the wrapped run");
    assert_eq!(refused["attempted"], run, "{refused}");
    let summary = sessions
        .summary_of(&jojobot_domain::session::SessionId(run.into()))
        .await
        .expect("the summary reads")
        .expect("the run exists");
    let why = summary.unreadable.expect("the damaged cell is named");
    let how = refused["how_to_proceed"].as_str().expect("a way forward");
    assert!(how.contains(run), "{refused}");
    assert!(how.contains(&why), "{refused}");
    store.stop().await;
}

/// The session verbs use the same judgement as memory writes, even when the
/// damaged row says active. A readable active run needs no strict window read.
#[tokio::test]
async fn over_the_real_store_an_unreadable_active_run_refuses_a_journal_write_for_a_person() {
    let scratch = jojobot_adapters::testing::Scratch::new("unreadable-active-run");
    let mut store = jojobot_adapters::testing::start_unhurried(
        &scratch.0,
        jojobot_adapters::testing::free_port(),
    )
    .await
    .expect("the real store comes up");
    let pool = jojobot_adapters::testing::migrated_database(&store, "window")
        .await
        .expect("a database of this case's own");
    let sessions = Arc::new(jojobot_adapters::dolt::sessions::DoltSessions::open(
        pool.clone(),
    ));
    let (jojobot, _memory) = with_sessions_port_and_memory(sessions.clone());
    make_bot(&jojobot, "gamma").await;
    let key = booted(&jojobot, "gamma").await;
    let first = journal_entry(&jojobot, &key, "started").await;
    let run = first["session"].as_str().expect("the active run");
    let card = jojobot_domain::session::SessionId(run.into());

    let before = sessions.full_reads();
    assert!(
        !jojobot
            .end_a_lapsed_window(&card)
            .await
            .expect("a live run")
    );
    let strict_reads = sessions.full_reads() - before;

    let damaged = sqlx::query("UPDATE session SET started_at = 'no time' WHERE id = ?")
        .bind(run)
        .execute(&pool)
        .await
        .expect("the row is damaged");
    assert_eq!(damaged.rows_affected(), 1, "one run is damaged");
    let summary = sessions
        .summary_of(&card)
        .await
        .expect("the summary reads")
        .expect("the run exists");
    let why = summary.unreadable.expect("the damaged cell is named");

    for _ in 0..2 {
        let refused = json_of(
            &jojobot
                .journal(Parameters(crate::session::JournalArgs {
                    entry: "the bus leaves at nine".into(),
                    focus: None,
                    working_on: None,
                    sid: key.clone(),
                    request_id: None,
                }))
                .await
                .expect("journal answers"),
        );
        assert_eq!(refused["status"], "blocked", "{refused}");
        assert_eq!(refused["wrote"], false, "{refused}");
        assert_eq!(refused["fix_by"], "person", "{refused}");
        assert_eq!(refused["attempted"], run, "{refused}");
        let how = refused["how_to_proceed"].as_str().expect("a way forward");
        assert!(how.contains(run), "{refused}");
        assert!(how.contains(&why), "{refused}");
    }
    let entries: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_entry WHERE session = ?")
        .bind(run)
        .fetch_one(&pool)
        .await
        .expect("the journal still reads");
    assert_eq!(entries, 1, "neither refused call appended a beat");
    assert_eq!(
        strict_reads, 0,
        "a readable active run needs no strict read"
    );
    store.stop().await;
}
