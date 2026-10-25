//! **The bots that wrote the live records of a thing are the ones the stores name.**
//!
//! The stores name the bot a write recorded, else the bot behind its run
//! (`first_writer`), so a run no session row answers to still names its bot. The
//! reading here is held to the same answer.

use super::*;
use crate::harness::*;
use jojobot_domain::memory::NewFact;

/// **A record filed under a run no session row answers to still makes the bot
/// that filed it a writer.** The write recorded the bot beside the run, and the
/// run has no row, as a run that has written nothing yet has none. Paired with a
/// record filed by no bot and under no run, which names nobody.
#[tokio::test]
async fn a_record_filed_under_a_run_with_no_session_row_names_the_bot_that_filed_it() {
    let jojobot = handler();
    make_bot(&jojobot, "gamma").await;
    let thread = EntityId("thread:fillers-room".into());
    jojobot
        .add_entity(rmcp::handler::server::wrapper::Parameters(
            crate::memory::testing::add_args("thread", thread.as_str(), "Fillers Room"),
        ))
        .await
        .expect("the thread is added");
    let today = jojobot.clock().today_in(&jiff::tz::TimeZone::UTC);
    let unattributed = jojobot
        .memory
        .capture(NewFact::about(
            thread.clone(),
            "a record nobody owns",
            today,
        ))
        .await
        .expect("captures")
        .written()
        .expect("lands");
    assert!(
        jojobot
            .record_writers(&thread)
            .await
            .expect("the writers read")
            .is_empty(),
        "a record no bot and no run made names a writer: {unattributed:?}"
    );

    jojobot
        .memory
        .capture(NewFact {
            written_by_bot: Some(EntityId("bot:gamma".into())),
            session: Some("no-such-run".to_string()),
            ..NewFact::about(thread.clone(), "a record gamma filed", today)
        })
        .await
        .expect("captures")
        .written()
        .expect("lands");
    let writers = jojobot
        .record_writers(&thread)
        .await
        .expect("the writers read");
    assert_eq!(
        writers,
        vec![EntityId("bot:gamma".into())],
        "the bot a write recorded is a writer whatever the run behind it"
    );
}
