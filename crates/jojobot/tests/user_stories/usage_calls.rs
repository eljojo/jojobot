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

use super::dsl::{Session, Story};

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
///
/// **This reads the store because no reader can see it.** A usage week belongs to
/// a bot, so a call counted under no bot is in no week, and no agent can read
/// it. The store is the only place the count shows.
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

/// The fields of the session's own bot's week of the story, read as the bot
/// reads them.
async fn own_week(who: &Session, handle: &str) -> Value {
    who.call("recall", json!({"subject": handle})).await.json()["objects"][0].clone()
}

/// **A count the store refuses is never made up, and never fails the call.** Read
/// through the bot's own week, the way an agent reads it. The store is made to
/// refuse the count, two calls are made and answered, and the store is let take
/// counts again: the week holds the read before the refusal and the one after,
/// and neither of the two in between.
#[tokio::test]
async fn a_count_the_store_cannot_keep_never_fails_the_call() {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-09-30").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    // The first read of the bot's own week is counted after it is answered, so
    // it is not in the week it returned.
    let before = own_week(&s, "usage:otto-2026-w40").await;
    assert_eq!(field(&before, "calls_recall"), None, "{before}");
    assert_eq!(field(&before, "calls_capture"), None, "{before}");

    story.sessions_store().refuse_counts(true);
    let answered = s.call("recall", json!({"subject": "person:lisa"})).await;
    answered.says("person:lisa");
    s.fact("person:lisa", "plays the saxophone").await;

    story.sessions_store().refuse_counts(false);
    s.call("recall", json!({"subject": "person:lisa", "facts": true}))
        .await
        .says("plays the saxophone");
    let after = own_week(&s, "usage:otto-2026-w40").await;
    // The read before the refusal and the read after it; the read made during it
    // is not among them.
    assert_eq!(
        field(&after, "calls_recall").as_deref(),
        Some("2"),
        "counting resumes with the next call, and the refused ones are not made up: {after}"
    );
    // The capture landed while the store refused, and no count of it was kept.
    assert_eq!(field(&after, "calls_capture"), None, "{after}");
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

/// How many `start_here` calls the store holds under no bot, landed and refused.
async fn boots_under_no_bot(story: &Story) -> (u64, u64) {
    let held = counted_under_no_bot(story).await;
    let of = |wanted: bool| {
        held.iter()
            .filter(|(verb, refused, _)| verb == "start_here" && *refused == wanted)
            .map(|(_, _, n)| n)
            .sum()
    };
    (of(false), of(true))
}

/// **Which boots belong to a run.** A boot that opens or resumes a run is among
/// the calls of that run: the answer names the run's handle. The offer to resume
/// or begin anew hands back no handle but names its bot, so it is counted under
/// that bot's week. A refused boot opens nothing and names no bot, so it is
/// counted under no bot, and no reader sees a week that belongs to nobody: the
/// counts under no bot are read from the store.
#[tokio::test]
async fn a_refused_boot_is_counted_under_no_bot_an_offer_under_its_bot_and_a_resume_under_its_run()
{
    let story = Story::begin_pretending_it_is("bot:otto", "2026-09-30").await;
    let s = story.session().await;
    s.add("bot:bart", "Bart").await;
    let bart = story.as_bot("bot:bart").await;
    let assistant = story.as_bot("bot:assistant").await;
    let week = || async {
        assistant
            .call("recall", json!({"subject": "usage:bart-2026-w40"}))
            .await
            .json()["objects"][0]
            .clone()
    };
    // Bart's first boot opened its run, and is that run's first call.
    let first = week().await;
    assert_eq!(field(&first, "calls_start_here").as_deref(), Some("1"));
    let (landed, refused) = boots_under_no_bot(&story).await;

    // ── Bart's run has written, so it has a record to pick up and a boot that
    // names no run is offered the choice. It hands back no handle. ───────────
    bart.journal("working").await;
    let (offer, offered) = story
        .call("start_here", json!({"bot": "bart", "brief": true}))
        .await;
    offer.says("offer_code");
    assert!(offered.is_none(), "an offer opens no run: {}", offer.raw());
    let after_offer = week().await;
    assert_eq!(
        field(&after_offer, "calls_start_here").as_deref(),
        Some("2"),
        "the first boot and the offered one: {after_offer}"
    );
    assert_eq!(
        boots_under_no_bot(&story).await,
        (landed, refused),
        "the offer was counted under the bot it named, so the count under no bot did not move"
    );

    // ── a boot of a bot nobody knows is refused, and opens nothing ──────────
    story
        .refused_at_the_door("start_here", json!({"bot": "bot:epsilon", "brief": true}))
        .await;
    assert_eq!(
        boots_under_no_bot(&story).await,
        (landed, refused + 1),
        "the refused boot was counted under no bot"
    );
    let after_refused = week().await;
    assert_eq!(
        field(&after_refused, "calls_start_here").as_deref(),
        Some("2"),
        "the refused boot is nobody's call: {after_refused}"
    );

    // ── a boot that resumes Bart's run by its name mints a new key, and is a
    // call of the run it resumed ─────────────────────────────────────────────
    let (resumed, session) = story
        .call(
            "start_here",
            json!({"bot": "bart", "brief": true, "resume": bart.run()}),
        )
        .await;
    let session = session.unwrap_or_else(|| panic!("a resume hands back a key: {}", resumed.raw()));
    assert_eq!(session.run(), bart.run(), "the same run, under a new key");
    assert_eq!(
        boots_under_no_bot(&story).await,
        (landed, refused + 1),
        "the resume was not counted under no bot"
    );
    let after_resume = week().await;
    assert_eq!(
        field(&after_resume, "calls_start_here").as_deref(),
        Some("3"),
        "the resume is a third boot of Bart's run: {after_resume}"
    );
    story.finish().await;
}

/// **A compaction report with no handle is counted under the run it reports.**
/// The answer names the run and no handle. The run states a day a week after the
/// server's, and the report belongs to that day.
#[tokio::test]
async fn a_compaction_report_with_no_handle_is_counted_under_the_run_and_its_day() {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-09-30").await;
    let _run = story.session_on("2026-10-07", Some("new")).await;
    let (reported, handle) = story
        .call("start_here", json!({"bot": "otto", "compacted": true}))
        .await;
    assert!(handle.is_none(), "the report hands back no handle");
    reported.says("\"run\"");

    let assistant = story.as_bot("bot:assistant").await;
    let week = assistant
        .call("recall", json!({"subject": "usage:otto-2026-w41"}))
        .await
        .json();
    assert_eq!(
        field(&week["objects"][0], "calls_start_here").as_deref(),
        Some("2"),
        "the boot and the report, both on the run's stated day: {week}"
    );
    // A compaction counts on the run's day too (card 2542), so nothing of this
    // run lands in the clock's week and no usage week exists for it.
    assistant
        .refused("recall", json!({"subject": "usage:otto-2026-w40"}))
        .await;
    story.finish().await;
}

/// **A boot that is handed the resume-or-new offer is counted under the bot it
/// named.** The offer names no run and no handle. The identity the software ships
/// always has a run in flight, so a boot as it is offered the choice.
#[tokio::test]
async fn a_boot_that_is_offered_a_resume_is_counted_under_the_bot_it_named() {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-09-30").await;
    let assistant = story.as_bot("bot:assistant").await;
    let boots = async || -> u64 {
        let week = assistant
            .call("recall", json!({"subject": "usage:assistant-2026-w40"}))
            .await
            .json();
        field(&week["objects"][0], "calls_start_here")
            .unwrap_or_else(|| panic!("the assistant booted: {week}"))
            .parse()
            .expect("a number")
    };
    let before = boots().await;
    let (offered, handle) = story
        .call("start_here", json!({"bot": "assistant", "brief": true}))
        .await;
    assert!(
        handle.is_none(),
        "an offer hands back no handle: {}",
        offered.json()
    );
    assert_eq!(boots().await, before + 1, "the offered boot is counted");
    story.finish().await;
}

/// **A blank handle is no handle, for counting as for the boot.** The boot
/// treats a blank `sid` as absent, so the call is counted under the run it
/// opens.
#[tokio::test]
async fn a_boot_that_sends_a_blank_handle_is_counted_under_the_run_it_opens() {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-09-30").await;
    let s = story.session().await;
    s.add("bot:bart", "Bart").await;
    let (opened, handle) = story
        .call(
            "start_here",
            json!({"bot": "bart", "sid": "  ", "brief": true}),
        )
        .await;
    assert!(
        handle.is_some(),
        "a blank handle opens the run: {}",
        opened.json()
    );

    let assistant = story.as_bot("bot:assistant").await;
    let week = assistant
        .call("recall", json!({"subject": "usage:bart-2026-w40"}))
        .await
        .json();
    assert_eq!(
        field(&week["objects"][0], "calls_start_here").as_deref(),
        Some("1"),
        "{week}"
    );
    story.finish().await;
}

/// **Everything a usage week counts for one run lands in the same week.** The
/// run states a day a week before the server's. Its compaction, its calls and the
/// day it was served on all belong to the stated day's week, and the week of the
/// clock holds none of them.
#[tokio::test]
async fn a_compaction_a_call_and_a_served_day_of_one_run_land_in_the_same_week() {
    // The server's day is Wednesday 2026-10-14, ISO week 42; the run's is the
    // Wednesday before, in week 41.
    let story = Story::begin_pretending_it_is("bot:otto", "2026-10-14").await;
    let run = story.session_on("2026-10-07", Some("new")).await;
    run.call("start_here", json!({"compacted": true, "brief": true}))
        .await;
    run.call("recall", json!({"subject": "bot:otto"})).await;

    let assistant = story.as_bot("bot:assistant").await;
    let stated = assistant
        .call("recall", json!({"subject": "usage:otto-2026-w41"}))
        .await
        .json();
    let week = &stated["objects"][0];
    assert_eq!(
        field(week, "compactions").as_deref(),
        Some("1"),
        "the compaction is on the run's day: {stated}"
    );
    assert!(
        field(week, "calls_start_here").is_some() && field(week, "calls_recall").is_some(),
        "the calls are on the run's day: {stated}"
    );
    assert!(
        field(week, "served_2026-10-07").is_some(),
        "the served day is the run's day: {stated}"
    );
    // The week of the clock holds none of the three: it has no record at all, for
    // the run had no compaction, call or run begun there.
    let real = assistant
        .refused("recall", json!({"subject": "usage:otto-2026-w42"}))
        .await;
    real.says("\"fix_by\":\"change\"");
    story.finish().await;
}

/// **A run that states no day keeps its compaction on the day it was recorded.**
/// The pair of the case above: there the run's own day decides the week, and here
/// the run has none, so the clock decides it.
#[tokio::test]
async fn a_compaction_of_a_run_that_states_no_day_is_counted_on_the_day_it_was_recorded() {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-10-14").await;
    let run = story.session().await;
    run.call("start_here", json!({"compacted": true, "brief": true}))
        .await;

    let assistant = story.as_bot("bot:assistant").await;
    let clock_week = assistant
        .call("recall", json!({"subject": "usage:otto-2026-w42"}))
        .await
        .json();
    assert_eq!(
        field(&clock_week["objects"][0], "compactions").as_deref(),
        Some("1"),
        "the compaction is on the day it was recorded: {clock_week}"
    );
    // The week before has no record: nothing of this run belongs to it.
    let before = assistant
        .refused("recall", json!({"subject": "usage:otto-2026-w41"}))
        .await;
    before.says("\"fix_by\":\"change\"");
    story.finish().await;
}
