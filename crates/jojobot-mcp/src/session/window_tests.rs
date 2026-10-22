//! **A wrapped run the real store cannot read takes no write.** The Dolt store
//! names a run whose row does not parse as an unreadable summary, which it files
//! as abandoned whatever state the row holds. The window judge reads the summary,
//! so it must refuse on the damage, as its own doc says, rather than read the
//! state the store chose for a run it could not read.

use super::testing::*;
use crate::harness::*;
use crate::memory::testing::{capture_args, ensure};
use crate::session::WrapSessionArgs;
use rmcp::handler::server::wrapper::Parameters;
use std::sync::Arc;

#[tokio::test]
async fn over_the_real_store_a_wrapped_run_that_cannot_be_read_refuses_a_write_with_retry() {
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
    let (jojobot, _memory) = with_sessions_port_and_memory(sessions);
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
    assert_eq!(refused["fix_by"], "retry", "{refused}");
    store.stop().await;
}
