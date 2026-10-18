//! "Which tools did each bot call this week, and which were refused, and how?"
//!
//! **The answer is the usage week.** Every call that reaches a verb is counted
//! once, after its answer is known, under the bot that made it, the day of the
//! run's own clock and the verb. A refusal is counted with the word that says
//! who fixes it. A resend of a landed request id answers from the ledger and is
//! not a second call. The counts ride on the same `usage` record that holds the
//! compactions, so one read answers both.

use jojobot_domain::session::Sessions;
use serde_json::{Value, json};

use super::dsl::Story;

fn field(record: &Value, key: &str) -> Option<String> {
    record["fields"][key].as_str().map(str::to_string)
}

#[tokio::test]
async fn each_bots_week_counts_its_calls_and_refusals_per_verb() {
    // The server's day is a Wednesday of ISO week 40, a week away from the day
    // the machine running the story is on, so a count filed under the wall
    // clock's week lands in a record this story never reads.
    let story = Story::begin_pretending_it_is("bot:otto", "2026-09-30").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:bart", "Bart").await;

    let bart = story.as_bot("bot:bart").await;
    let marge = story.as_bot("bot:marge").await;

    // ── Bart: a read, a capture that lands, the same capture sent again, and a
    // capture refused because the call itself must change ───────────────────
    bart.call("recall", json!({"subject": "person:lisa"})).await;
    let landed = json!({
        "subject": "person:lisa", "content": "plays the saxophone",
        "provenance": "testimony", "request_id": "lisa-saxophone",
    });
    let first = bart.call("capture", landed.clone()).await.json();
    let repeat = bart.call("capture", landed).await.json();
    assert_eq!(
        repeat["repeat"], true,
        "the resend answered from the ledger: {repeat}"
    );
    assert!(first.get("repeat").is_none(), "{first}");
    bart.refused(
        "capture",
        json!({
            "subject": "person:milhouse", "content": "plays the tuba",
            "provenance": "testimony",
        }),
    )
    .await
    .says("\"fix_by\":\"change\"");

    // ── Marge makes one read, and it stays in Marge's week ──────────────────
    marge
        .call("recall", json!({"subject": "person:lisa"}))
        .await;

    // ── the head of the chart reads Bart's week ─────────────────────────────
    let assistant = story.as_bot("bot:assistant").await;
    let week = assistant
        .call("recall", json!({"subject": "usage:bart-2026-w40"}))
        .await
        .json();
    let bart_week = &week["objects"][0];
    assert_eq!(
        field(bart_week, "week").as_deref(),
        Some("2026-W40"),
        "{week}"
    );
    // recall 1; capture 2 (one landed, one refused); the resend is no call.
    assert_eq!(
        field(bart_week, "calls_recall").as_deref(),
        Some("1"),
        "{week}"
    );
    assert_eq!(
        field(bart_week, "calls_capture").as_deref(),
        Some("2"),
        "{week}"
    );
    assert_eq!(
        field(bart_week, "refused_capture").as_deref(),
        Some("1"),
        "{week}"
    );
    assert_eq!(
        field(bart_week, "refused_capture_change").as_deref(),
        Some("1"),
        "{week}"
    );
    assert_eq!(field(bart_week, "refused_capture_retry"), None, "{week}");
    assert_eq!(field(bart_week, "refused_capture_person"), None, "{week}");
    // The boot is a call of the run it opened. The read lands in the totals; a
    // verb never called has no key at all.
    assert_eq!(
        field(bart_week, "calls_start_here").as_deref(),
        Some("1"),
        "{week}"
    );
    assert_eq!(field(bart_week, "calls").as_deref(), Some("4"), "{week}");
    assert_eq!(field(bart_week, "refused").as_deref(), Some("1"), "{week}");
    assert_eq!(field(bart_week, "calls_journal"), None, "{week}");
    assert_eq!(field(bart_week, "refused_recall"), None, "{week}");
    // A bare key answers "did it happen" only from the first day a call of the
    // bot was counted, and the week names that day.
    assert_eq!(
        field(bart_week, "calls_counted_from").as_deref(),
        Some("2026-09-30"),
        "{week}"
    );

    // ── another bot's calls are in its own week ─────────────────────────────
    let other = assistant
        .call("recall", json!({"subject": "usage:marge-2026-w40"}))
        .await
        .json();
    let marge_week = &other["objects"][0];
    assert_eq!(
        field(marge_week, "calls_recall").as_deref(),
        Some("1"),
        "{other}"
    );
    assert_eq!(field(marge_week, "calls_capture"), None, "{other}");
    assert_eq!(
        field(marge_week, "calls_start_here").as_deref(),
        Some("1"),
        "{other}"
    );
    assert_eq!(field(marge_week, "calls").as_deref(), Some("2"), "{other}");
    assert_eq!(
        field(marge_week, "calls_counted_from").as_deref(),
        Some("2026-09-30"),
        "{other}"
    );

    // ── the read that answered is itself a call, counted once it is answered ─
    let again = assistant
        .call("recall", json!({"subject": "usage:bart-2026-w40"}))
        .await
        .json();
    assert_eq!(
        field(&again["objects"][0], "calls_recall").as_deref(),
        Some("1"),
        "Bart's week does not count the assistant's reads: {again}"
    );
    let assistants = assistant
        .call("recall", json!({"subject": "usage:assistant-2026-w40"}))
        .await
        .json();
    assert_eq!(
        field(&assistants["objects"][0], "calls_recall").as_deref(),
        Some("3"),
        "the assistant's two reads of Bart's week and one of Marge's, counted after each \
         answer: {assistants}"
    );

    // ── the counts are the same after the store changed underneath ──────────
    story.the_store_changed_underneath().await;
    let kept = assistant
        .call("recall", json!({"subject": "usage:bart-2026-w40"}))
        .await
        .json();
    assert_eq!(
        field(&kept["objects"][0], "calls_capture").as_deref(),
        Some("2"),
        "{kept}"
    );
    story.finish().await;
}

/// What the store beneath the server holds for calls that belong to no bot, by
/// verb and how the call ended.
async fn counted_under_no_bot(story: &Story) -> Vec<(String, bool, u64)> {
    story
        .sessions_store()
        .call_counts()
        .await
        .expect("the counts read")
        .into_iter()
        .filter(|held| held.key.bot.is_none())
        .map(|held| {
            (
                held.key.verb,
                matches!(
                    held.key.outcome,
                    jojobot_domain::usage::CallOutcome::Refused { .. }
                ),
                held.count,
            )
        })
        .collect()
}

#[tokio::test]
async fn a_call_with_no_session_is_counted_under_no_bot() {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-09-30").await;
    // The orientation a caller gets before it holds a session.
    story.call("start_here", json!({"brief": true})).await;
    let s = story.session().await;

    let held = counted_under_no_bot(&story).await;
    assert!(
        held.iter()
            .any(|(verb, refused, n)| verb == "start_here" && !refused && *n >= 1),
        "the orientation was a call, and belongs to no bot: {held:?}"
    );
    // The run that booted afterwards holds its own boot and not the orientation
    // that came before it.
    s.add("person:lisa", "Lisa").await;
    let week = s
        .call("recall", json!({"subject": "usage:otto-2026-w40"}))
        .await
        .json();
    assert_eq!(
        field(&week["objects"][0], "calls_start_here").as_deref(),
        Some("1"),
        "{week}"
    );
    story.finish().await;
}

#[tokio::test]
async fn a_verb_nobody_serves_is_counted_under_one_name_and_not_the_name_sent() {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-09-30").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.refused("a_verb_nobody_serves", json!({})).await;
    s.refused("another_verb_nobody_serves", json!({})).await;

    let week = s
        .call("recall", json!({"subject": "usage:otto-2026-w40"}))
        .await
        .json();
    let fields = &week["objects"][0];
    assert_eq!(
        field(fields, "calls_unserved").as_deref(),
        Some("2"),
        "{week}"
    );
    assert_eq!(
        field(fields, "refused_unserved").as_deref(),
        Some("2"),
        "{week}"
    );
    assert!(
        !week.to_string().contains("nobody_serves"),
        "a name the caller chose keys no count: {week}"
    );
    story.finish().await;
}

#[tokio::test]
async fn a_count_the_store_cannot_keep_never_fails_the_call() {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-09-30").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    let before = story.sessions_store().call_counts().await.expect("counts");

    story.sessions_store().refuse_counts(true);
    let answered = s.call("recall", json!({"subject": "person:lisa"})).await;
    answered.says("person:lisa");
    s.fact("person:lisa", "plays the saxophone").await;
    let during = story.sessions_store().call_counts().await.expect("counts");
    assert_eq!(during, before, "nothing was kept while the store refused");

    story.sessions_store().refuse_counts(false);
    s.call("recall", json!({"subject": "person:lisa"})).await;
    let after = story.sessions_store().call_counts().await.expect("counts");
    assert_eq!(
        after.iter().map(|held| held.count).sum::<u64>(),
        before.iter().map(|held| held.count).sum::<u64>() + 1,
        "counting resumes with the next call, and the refused ones are not made up: {after:?}"
    );
    story.finish().await;
}

/// **The boot is among the calls of the run it opens.** A first boot carries no
/// `sid`, so the call is counted under the run its answer names.
#[tokio::test]
async fn the_boot_call_is_counted_under_the_run_it_opens() {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-09-30").await;
    let s = story.session().await;
    s.add("bot:bart", "Bart").await;
    s.add("bot:marge", "Marge").await;
    // Bart boots once; Marge boots and makes a read, which is not a boot.
    let _bart = story.as_bot("bot:bart").await;
    let marge = story.as_bot("bot:marge").await;
    marge.call("recall", json!({"subject": "bot:marge"})).await;

    let assistant = story.as_bot("bot:assistant").await;
    let read = |handle: &'static str| {
        let assistant = &assistant;
        async move {
            assistant
                .call("recall", json!({"subject": handle}))
                .await
                .json()["objects"][0]
                .clone()
        }
    };
    let bart_week = read("usage:bart-2026-w40").await;
    assert_eq!(
        field(&bart_week, "calls_start_here").as_deref(),
        Some("1"),
        "{bart_week}"
    );
    let marge_week = read("usage:marge-2026-w40").await;
    assert_eq!(
        field(&marge_week, "calls_start_here").as_deref(),
        Some("1"),
        "{marge_week}"
    );
    assert_eq!(
        field(&marge_week, "calls_recall").as_deref(),
        Some("1"),
        "{marge_week}"
    );
    story.finish().await;
}

/// **The call that ends a run is counted for the run that made it.** Wrapping
/// voids the run's key, so a count that looks the caller up after the verb ran
/// finds nobody: the call lands under no bot and on the clock's day. The run
/// states a day a week after the server's.
#[tokio::test]
async fn the_call_that_ends_a_run_is_counted_for_its_bot_on_its_stated_day() {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-09-30").await;
    let run = story.session_on("2026-10-07", Some("new")).await;
    run.call("wrap_session", json!({"story": "done for the day"}))
        .await;

    let assistant = story.as_bot("bot:assistant").await;
    let stated_week = assistant
        .call("recall", json!({"subject": "usage:otto-2026-w41"}))
        .await
        .json();
    assert_eq!(
        field(&stated_week["objects"][0], "calls_wrap_session").as_deref(),
        Some("1"),
        "{stated_week}"
    );
    // The positive the absence rests on: the boot of the same run is in the same
    // week, so the week is the run's stated day and not the clock's.
    assert_eq!(
        field(&stated_week["objects"][0], "calls_start_here").as_deref(),
        Some("1"),
        "{stated_week}"
    );
    let clock_week = assistant
        .refused("recall", json!({"subject": "usage:otto-2026-w40"}))
        .await;
    clock_week.says("\"fix_by\":\"change\"");
    story.finish().await;
}
