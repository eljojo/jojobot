//! "One bot's runs are in a state nobody can read. Do the other bots' usage
//! reads still answer?"
//!
//! **A usage read answers for the bots it may read, whatever state any other
//! bot's runs are in.** A run owned by a bot that was merged away has an owner
//! whose place on the chart cannot be read. That owner is not below the asker,
//! so its weeks are counted as withheld, and the read goes on to the asker's own
//! weeks and those of the bots below it.

use jojobot_domain::memory::EntityId;
use jojobot_domain::session::{NewSession, Sessions, Sid};
use serde_json::{Value, json};

use super::dsl::{Session, Story};

fn handles(read: &Value) -> Vec<String> {
    read["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("a read with objects: {read}"))
        .iter()
        .map(|object| object["id"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// The run writes once, which gives it a record, and makes a read.
async fn is_active(who: &Session) {
    who.journal("working").await;
    who.call("recall", json!({"subject": "person:lisa"})).await;
}

#[tokio::test]
async fn a_manager_reads_its_reports_weeks_though_another_bots_runs_belong_to_a_merged_bot() {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-10-07").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:bart", "Bart").await;
    s.add("bot:milhouse", "Milhouse").await;
    s.add("bot:psi", "Psi").await;
    let marge = story.as_bot("bot:marge").await;
    story
        .as_bot("bot:assistant")
        .await
        .call(
            "capture",
            json!({
                "subject": "bot:bart", "content": "bot:bart reports to bot:marge",
                "provenance": "testimony", "sets": {"reports_to": "bot:marge"},
            }),
        )
        .await;
    let bart = story.as_bot("bot:bart").await;
    is_active(&bart).await;
    is_active(&marge).await;
    let milhouse = story.as_bot("bot:milhouse").await;
    is_active(&milhouse).await;

    // ── the bot that owns a run is merged away ──────────────────────────────
    let assistant = story.as_bot("bot:assistant").await;
    assistant
        .call(
            "merge_entities",
            json!({"duplicate": "bot:milhouse", "survivor": "bot:psi",
                   "reason": "the same bot, filed twice"}),
        )
        .await;

    // ── the manager still reads its own weeks and its report's ──────────────
    let managed = marge.call("recall", json!({"view": "usage"})).await.json();
    let seen = handles(&managed);
    assert!(
        seen.iter().any(|handle| handle.contains("marge")),
        "the manager's own week: {managed}"
    );
    assert!(
        seen.iter().any(|handle| handle.contains("bart")),
        "its report's week: {managed}"
    );
    assert!(
        seen.iter().all(|handle| !handle.contains("milhouse")),
        "the merged bot's weeks are not shown: {managed}"
    );
    assert!(
        managed["withheld"].as_u64().unwrap_or(0) > 0,
        "the weeks it was not shown are counted: {managed}"
    );
    story.finish().await;
}

/// **A run whose owner was never created.** The run is on the board and no
/// record answers to its owner's handle, so a read of that handle fails. The
/// owner is not below anybody, and its weeks are withheld from every asker.
#[tokio::test]
async fn a_manager_reads_its_reports_weeks_though_a_run_belongs_to_a_bot_nobody_created() {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-10-07").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:bart", "Bart").await;
    let marge = story.as_bot("bot:marge").await;
    story
        .as_bot("bot:assistant")
        .await
        .call(
            "capture",
            json!({
                "subject": "bot:bart", "content": "bot:bart reports to bot:marge",
                "provenance": "testimony", "sets": {"reports_to": "bot:marge"},
            }),
        )
        .await;
    let bart = story.as_bot("bot:bart").await;
    is_active(&bart).await;
    is_active(&marge).await;
    story
        .sessions_store()
        .begin(NewSession {
            bot: EntityId("bot:omega".into()),
            sid: Sid("gh01".into()),
            id: None,
            key_hash: None,
            focus: "a run whose owner nobody created".into(),
            started_at: jiff::Timestamp::now(),
            timezone: None,
            started_on: None,
        })
        .await
        .expect("the run is on the board");
    // The server takes the board as the store holds it when it comes up.
    let later = story.restarted_pretending_it_is("2026-10-07").await;
    let marge = later.session_carried_from(&marge).await;

    let managed = marge.call("recall", json!({"view": "usage"})).await.json();
    let seen = handles(&managed);
    assert!(
        seen.iter().any(|handle| handle.contains("marge")),
        "the manager's own week: {managed}"
    );
    assert!(
        seen.iter().any(|handle| handle.contains("bart")),
        "its report's week: {managed}"
    );
    assert!(
        seen.iter().all(|handle| !handle.contains("omega")),
        "the unreadable owner's weeks are not shown: {managed}"
    );
    assert!(
        managed["withheld"].as_u64().unwrap_or(0) > 0,
        "the weeks it was not shown are counted: {managed}"
    );
    later.finish().await;
}
