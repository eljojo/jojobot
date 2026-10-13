//! "My context was compacted. How does jojobot find out?"
//!
//! **A bot whose context was compacted holds none of what jojobot taught it or
//! showed it, and keeps no way to say so.** The run reports it with
//! `compacted: true` on `start_here`, sent with its live `sid`. The run is the
//! same run afterwards: same key, same chronology, same identity. What jojobot
//! believed the run was taught and shown is forgotten, and another run of the
//! same bot is left alone. Every answer carries one line telling a run that
//! lost its context how to report it, so the instruction cannot be compacted
//! away.

use serde_json::json;

use super::dsl::{Session, Story};

/// The teaching about claims, whose words are not asserted: it rides a
/// capture's receipt under `teaching`, and what is read is its subject.
const CLAIMS_TEACHING: &str = "does not destroy the one already there";

async fn capture(who: &Session, content: &str) -> super::dsl::Answer {
    who.call(
        "capture",
        json!({"subject": "person:lisa", "content": content, "provenance": "testimony"}),
    )
    .await
}

#[tokio::test]
async fn a_run_that_reports_compaction_is_taught_again_and_keeps_its_key() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.add("bot:gamma", "Gamma").await;

    // Two runs of one bot, each taught once.
    let first = story.as_bot("bot:gamma").await;
    let second = story.as_bot("bot:gamma").await;
    let (run, key) = (first.run().to_string(), first.sid().to_string());
    capture(&first, "orders the pie")
        .await
        .says(CLAIMS_TEACHING);
    capture(&first, "orders the pie again")
        .await
        .never_says(CLAIMS_TEACHING);
    capture(&second, "orders the donut")
        .await
        .says(CLAIMS_TEACHING);
    capture(&second, "orders the donut again")
        .await
        .never_says(CLAIMS_TEACHING);

    // ── the first run reports a compaction, asking for the brief boot ────────
    let reported = first
        .call("start_here", json!({"compacted": true, "brief": true}))
        .await;
    let body = reported.json();
    // The same run and the same key: nothing new was minted, offered or ended.
    assert_eq!(body["session"]["run"], run, "{body}");
    assert_eq!(body["session"]["sid"], key, "{body}");
    assert!(body["session"]["choices"].is_null(), "{body}");
    // The orientation comes back although `brief` was sent: a brief boot leaves
    // it out, and a run that lost its context is owed it.
    assert!(
        body["orientation"]
            .as_str()
            .is_some_and(|essay| !essay.is_empty()),
        "a compaction report answers with the orientation: {body}"
    );
    assert!(body["session"]["shown_and_taught"].is_string(), "{body}");

    // ── the run is taught again, once; the other run is untouched ───────────
    capture(&first, "orders the pie after the compaction")
        .await
        .says(CLAIMS_TEACHING);
    capture(&first, "and the pie once more")
        .await
        .never_says(CLAIMS_TEACHING);
    capture(&second, "orders the donut once more")
        .await
        .never_says(CLAIMS_TEACHING);

    // ── the key still works, and the chronology carries the mark ────────────
    // The read returns every run of the bot, so each is looked up by its name.
    let chronologies = first
        .call("recall", json!({"kind": "session", "prose": true}))
        .await
        .json();
    let prose_of = |run: &str| -> String {
        chronologies["objects"]
            .as_array()
            .and_then(|all| all.iter().find(|o| o["id"] == format!("session:{run}")))
            .and_then(|o| o["prose"].as_str())
            .unwrap_or_else(|| panic!("run {run} has a chronology: {chronologies}"))
            .to_string()
    };
    assert!(
        prose_of(&run).contains("compacted"),
        "the run's chronology carries no mark: {chronologies}"
    );
    assert!(
        !prose_of(second.run()).contains("compacted"),
        "the other run was marked: {chronologies}"
    );

    story.finish().await;
}

/// **Every answer carries the one line**, so a run that lost its context finds
/// the instruction on whatever it calls next. A key named in it is the call to
/// make, and an ordinary answer carries it beside the other bar entries.
#[tokio::test]
async fn an_ordinary_answer_tells_the_run_how_to_report_a_compaction() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.add("bot:gamma", "Gamma").await;
    let gamma = story.as_bot("bot:gamma").await;

    let answered = capture(&gamma, "orders the pie").await;
    let line = answered.json()["status_bar"]["if_compacted"].clone();
    let line = line.as_str().unwrap_or_else(|| {
        panic!(
            "the answer carries no instruction for a compacted run: {}",
            answered.json()
        )
    });
    assert!(
        line.contains("compacted") && line.contains("start_here"),
        "the line names the call and the argument: {line}"
    );
    assert!(!line.contains('\n') && line.len() <= 200, "{line}");

    story.finish().await;
}

/// **A report that opens nothing is refused and names the way forward.** A
/// handle is needed because the report is about one run, and a handle that
/// addresses nothing leaves no run to forget for.
#[tokio::test]
async fn a_compaction_report_that_opens_no_run_is_refused() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let gamma = story.as_bot("bot:gamma").await;

    // No handle at all.
    story
        .refused_at_the_door("start_here", json!({"compacted": true, "bot": "gamma"}))
        .await
        .says("sid");
    // A handle that addresses nothing.
    gamma
        .refused(
            "start_here",
            json!({"compacted": true, "sid": "sk-nothing-opens-with-this-key"}),
        )
        .await
        .says("start_here");
    // The positive the refusals rest on: the run's own handle is accepted.
    gamma
        .call("start_here", json!({"compacted": true}))
        .await
        .says("sid");

    story.finish().await;
}

/// **A report restarts the running total of what the run has been handed.** The
/// total grows with every answer, so two reports back to back would hold about
/// twice the first one's if nothing restarted it. The positive it rests on: an
/// ordinary read does grow it.
#[tokio::test]
async fn a_compaction_report_restarts_the_running_total() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let gamma = story.as_bot("bot:gamma").await;
    // The total only exists on a run with a card, which its first write opens.
    gamma.journal("looking around").await;

    let served = async |who: &Session| -> u64 {
        let runs = who.call("list_runs", json!({})).await.json();
        runs["runs"][0]["served"]["characters"]
            .as_u64()
            .unwrap_or_else(|| panic!("the run reports what it was handed: {runs}"))
    };

    let before_reading = served(&gamma).await;
    gamma.call("list_entities", json!({"kind": "bot"})).await;
    let after_reading = served(&gamma).await;
    assert!(
        after_reading > before_reading,
        "an ordinary answer did not grow the total: {before_reading} then {after_reading}"
    );

    gamma.call("start_here", json!({"compacted": true})).await;
    let first = served(&gamma).await;
    gamma.call("start_here", json!({"compacted": true})).await;
    let second = served(&gamma).await;
    assert!(
        second < first + first / 2,
        "the second report added to the total instead of restarting it: {first} then {second}"
    );

    story.finish().await;
}

/// **A report also forgets what the run was shown.** Two runs of one bot read
/// the places, so a creation beside one of them is asked about it. The run that
/// reports a compaction is not asked afterwards, because it holds nothing it
/// was shown; the other run still is.
#[tokio::test]
async fn a_compaction_report_forgets_what_the_run_was_shown() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("place:wonder-wharf", "Wonder Wharf").await;
    s.add("bot:gamma", "Gamma").await;
    // The question a creation is asked, named by the verb it offers.
    const ASKED: &str = "merge_entities";
    let create = async |who: &Session, slug: &str, name: &str| {
        who.call(
            "add_entity",
            json!({"kind": "place", "handle": slug, "name": name, "source": "user-named"}),
        )
        .await
    };

    let reporter = story.as_bot("bot:gamma").await;
    let other = story.as_bot("bot:gamma").await;
    for run in [&reporter, &other] {
        run.call("list_entities", json!({"kind": "place"})).await;
    }
    reporter
        .call("start_here", json!({"compacted": true}))
        .await;

    create(&reporter, "wonder-road", "Wonder Road")
        .await
        .never_says(ASKED);
    // The positive the absence rests on: the run that did not report is asked
    // about the place it was shown, beside the same word.
    create(&other, "wonder-lane", "Wonder Lane")
        .await
        .says(ASKED);

    story.finish().await;
}
