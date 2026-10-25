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
    let thread = EntityId("thread:alpha".into());
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

/// **A record filed under the run of a bot that was folded into another names the
/// survivor**, as both stores read the writer. The write recorded no bot, so the
/// bot comes from the session table, which keeps the id the run began under and
/// follows no fold. Paired with the same record before the fold, which names the
/// bot the run began under.
#[tokio::test]
async fn a_record_filed_under_the_run_of_a_folded_bot_names_its_survivor() {
    let jojobot = handler();
    for slug in ["gamma", "omega"] {
        make_bot(&jojobot, slug).await;
    }
    let thread = EntityId("thread:beta".into());
    jojobot
        .add_entity(rmcp::handler::server::wrapper::Parameters(
            crate::memory::testing::add_args("thread", thread.as_str(), "Fillers Fold"),
        ))
        .await
        .expect("the thread is added");
    let run = jojobot
        .sessions
        .begin(jojobot_domain::session::NewSession {
            bot: EntityId("bot:gamma".into()),
            sid: jojobot_domain::session::Sid("fr01".into()),
            focus: "filing".into(),
            started_at: jiff::Timestamp::now(),
            timezone: None,
            started_on: None,
            id: None,
            key_hash: None,
        })
        .await
        .expect("a run is seeded");
    let today = jojobot.clock().today_in(&jiff::tz::TimeZone::UTC);
    jojobot
        .memory
        .capture(NewFact {
            session: Some(run.id.as_str().to_string()),
            ..NewFact::about(thread.clone(), "a record gamma's run filed", today)
        })
        .await
        .expect("captures")
        .written()
        .expect("lands");
    assert_eq!(
        jojobot.record_writers(&thread).await.expect("reads"),
        vec![EntityId("bot:gamma".into())],
        "before the fold the run names the bot it began under"
    );

    jojobot
        .memory
        .merge(
            &EntityId("bot:gamma".into()),
            &EntityId("bot:omega".into()),
            None,
            today,
            &EntityId("bot:omega".into()),
            None,
        )
        .await
        .expect("the bots fold into one");
    assert_eq!(
        jojobot.record_writers(&thread).await.expect("reads"),
        vec![EntityId("bot:omega".into())],
        "after the fold the survivor is the writer"
    );
}
