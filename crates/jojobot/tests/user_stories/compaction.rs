//! "My context was compacted. How does jojobot find out?"
//!
//! **A bot whose context was compacted holds none of what jojobot taught it or
//! showed it, and keeps no way to say so.** The run reports it with
//! `compacted: true` on `start_here`, sent with its live `sid`. The run is the
//! same run afterwards: same key, same chronology, same identity. What jojobot
//! believed the run was taught and shown is forgotten, and another run of the
//! same bot is left alone. No answer carries a line telling a run how to report
//! it: the harness prompts the report at the compaction itself.

use serde_json::json;

use super::dsl::{Session, Story};

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
    // The first capture of a run carries the teaching about claims under
    // `teaching`; its lines are the reference every later answer is held to.
    let taught = capture(&first, "orders the pie").await.teachings();
    assert!(!taught.is_empty(), "a first capture teaches");
    assert!(
        capture(&first, "orders the pie again")
            .await
            .teachings()
            .is_empty()
    );
    assert_eq!(
        capture(&second, "orders the donut").await.teachings(),
        taught
    );
    assert!(
        capture(&second, "orders the donut again")
            .await
            .teachings()
            .is_empty()
    );

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
    assert_eq!(
        capture(&first, "orders the pie after the compaction")
            .await
            .teachings(),
        taught
    );
    assert!(
        capture(&first, "and the pie once more")
            .await
            .teachings()
            .is_empty()
    );
    assert!(
        capture(&second, "orders the donut once more")
            .await
            .teachings()
            .is_empty()
    );

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

/// **No answer carries a line about compaction.** The harness prompts the
/// report at the compaction itself, so an ordinary answer, one with mail waiting
/// beside it, carries none. The positive sits in the same read: the bar is still
/// there and still carries the mail entry.
#[tokio::test]
async fn an_ordinary_answer_carries_no_line_about_compaction() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.add("bot:gamma", "Gamma").await;
    let gamma = story.as_bot("bot:gamma").await;

    let quiet = capture(&gamma, "orders the pie").await;
    quiet.never_says("if_compacted");

    s.post("gamma", "the pie", "your turn on the pie").await;
    let told = capture(&gamma, "orders the pie again").await;
    told.says("\"mail_waiting\":1");
    told.never_says("if_compacted");

    story.finish().await;
}

/// **A run that lost its key reports by the name of its bot.** A new session
/// rarely knows what its previous `sid` was, so `start_here` with `bot` and
/// `compacted: true` and no `sid` is about the bot's one live run: it is taught
/// again, no key is handed back, and the chronology carries the mark. The
/// positive for the refusal that follows in the same story: a bot with two live
/// runs is refused, naming both runs by name, and neither forgets anything.
#[tokio::test]
async fn a_compaction_report_by_bot_name_alone_reaches_its_one_live_run() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.add("bot:gamma", "Gamma").await;
    s.add("bot:delta", "Delta").await;

    let only = story.as_bot("bot:delta").await;
    let taught = capture(&only, "orders the pie").await.teachings();
    assert!(!taught.is_empty(), "a first capture teaches");
    assert!(
        capture(&only, "orders the pie again")
            .await
            .teachings()
            .is_empty()
    );

    let (reported, handed) = story
        .call("start_here", json!({"compacted": true, "bot": "delta"}))
        .await;
    assert!(
        handed.is_none(),
        "no key is handed back: {}",
        reported.json()
    );
    let body = reported.json();
    assert_eq!(body["session"]["run"], only.run(), "{body}");
    assert!(body["session"]["shown_and_taught"].is_string(), "{body}");
    assert!(
        body["orientation"]
            .as_str()
            .is_some_and(|essay| !essay.is_empty()),
        "{body}"
    );
    reported.never_says(only.sid());
    assert_eq!(
        capture(&only, "orders the pie after the compaction")
            .await
            .teachings(),
        taught
    );

    // ── two live runs: refused, both named, nothing forgotten ────────────────
    let first = story.as_bot("bot:gamma").await;
    let second = story.as_bot("bot:gamma").await;
    capture(&first, "orders the donut").await;
    capture(&second, "orders the cake").await;
    let refusal = story
        .refused_at_the_door("start_here", json!({"compacted": true, "bot": "gamma"}))
        .await;
    refusal.says(first.run()).says(second.run());
    assert!(
        capture(&first, "orders the donut again")
            .await
            .teachings()
            .is_empty()
    );
    assert!(
        capture(&second, "orders the cake again")
            .await
            .teachings()
            .is_empty()
    );

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
    s.add("bot:delta", "Delta").await;
    let gamma = story.as_bot("bot:gamma").await;

    // No handle, and the bot named has no live run to be the address.
    story
        .refused_at_the_door("start_here", json!({"compacted": true, "bot": "delta"}))
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
