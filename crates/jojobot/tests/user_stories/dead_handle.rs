//! **A key that stopped opening its run says what is in flight.**
//!
//! Since a run has a public name and a secret key, a call can carry a key that
//! used to open a run (replaced by a resume, or ended by a wrap) or one whose
//! run has since closed. Those refusals know the bot, so they list the bot's
//! runs the way the door lists them, and name the one call that goes on. They
//! never resume anything, never offer a pick-up the door would refuse, and never
//! print a key or a wrap code.

use serde_json::{Value, json};

use super::dsl::Story;

/// The `in_flight` runs of a refusal, by run name.
fn run_of<'a>(refused: &'a Value, name: &str) -> &'a Value {
    refused["in_flight"]["runs"]
        .as_array()
        .unwrap_or_else(|| panic!("the refusal lists the runs in flight: {refused}"))
        .iter()
        .find(|run| run["run"] == name)
        .unwrap_or_else(|| panic!("run {name} is in flight: {refused}"))
}

/// Gamma has two runs; a later sitting takes the first by its name, which
/// replaces its key, and the second goes quiet and is swept to abandoned.
/// Returns the story over a server a lifetime later, the two runs' names and
/// keys, and the sitting that took the first.
async fn two_runs_one_taken_over() -> (Story, Story, [(String, String); 2]) {
    let story = Story::begin_on_the_real_lease("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("bot:delta", "Delta").await;
    let first = story.as_bot("bot:gamma").await;
    first
        .call(
            "journal",
            json!({"entry": "took the work", "focus": "the dispatch"}),
        )
        .await;
    let second = story.as_bot("bot:gamma").await;
    second
        .call(
            "journal",
            json!({"entry": "took another", "focus": "the second track"}),
        )
        .await;
    let runs = [
        (first.run().to_string(), first.sid().to_string()),
        (second.run().to_string(), second.sid().to_string()),
    ];
    // Two days on: both runs are past the day a run may be quiet, and inside the
    // week a stopped run is still offered. The day is worked out from the real
    // clock, which the writes above were stamped with.
    let two_days_on = jiff::Timestamp::now() + jiff::SignedDuration::from_hours(48);
    let later = story
        .restarted_on_the_real_lease_pretending_it_is(
            &two_days_on
                .to_zoned(jiff::tz::TimeZone::UTC)
                .date()
                .to_string(),
        )
        .await;
    let (_, taker) = later
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "resume": runs[0].0}),
        )
        .await;
    let taker = taker.expect("a quiet run is handed to whoever names it, with a new key");
    // The sitting that took the run writes, as a sitting does: the run is live.
    taker
        .call("journal", json!({"entry": "picked it up"}))
        .await;
    (story, later, runs)
}

/// **A retired key and a closed run both say what is in flight.** The first run
/// was taken over a moment ago, so it is its new sitting's until the lease runs
/// out: it is listed with the moment and offered no pick-up. The second run
/// stopped, so it is listed as one a name picks up, and it is the one call named.
#[tokio::test]
async fn a_retired_key_and_a_closed_run_list_what_is_in_flight_and_resume_nothing() {
    let (story, later, [(taken, old_key), (stopped, stopped_key)]) =
        two_runs_one_taken_over().await;
    let asker = later.as_bot("bot:delta").await;

    // ── a key replaced by a resume (case 3) ─────────────────────────────────
    let retired = asker
        .refused(
            "journal",
            json!({"entry": "from the old sitting", "sid": old_key}),
        )
        .await;
    let body = retired.json();
    retired.never_says(&old_key).never_says(&stopped_key);
    assert!(
        body.get("ended_key").is_none(),
        "the mark is not shipped: {body}"
    );
    // The positive: the run that stopped can be picked up by its name, and the
    // one call that goes on names it.
    let pickable = run_of(&body, &stopped);
    assert_eq!(pickable["working_on"], "the second track", "{body}");
    assert_eq!(pickable["state"], "abandoned", "{body}");
    assert!(pickable.get("free_at").is_none(), "{body}");
    assert_eq!(
        body["in_flight"]["next_call"]["tool"], "start_here",
        "{body}"
    );
    assert_eq!(
        body["in_flight"]["next_call"]["arguments"],
        json!({"bot": "bot:gamma", "resume": stopped}),
        "{body}"
    );
    // The negative: the run taken over a moment ago is listed with the moment it
    // is free, and no call picks it up.
    let held = run_of(&body, &taken);
    assert_eq!(held["working_on"], "the dispatch", "{body}");
    assert!(held["free_at"].is_string(), "{body}");
    assert_ne!(
        body["in_flight"]["next_call"]["arguments"]["resume"], taken,
        "{body}"
    );
    // Nothing was resumed: the run that was taken is still the taker's.
    let again = asker
        .refused("journal", json!({"entry": "again", "sid": old_key}))
        .await;
    assert_eq!(run_of(&again.json(), &taken)["state"], "active");

    // ── a live key whose run is closed (case 5) ─────────────────────────────
    let closed = asker
        .refused(
            "journal",
            json!({"entry": "from a stopped run", "sid": stopped_key}),
        )
        .await;
    let body = closed.json();
    closed.never_says(&stopped_key).never_says(&old_key);
    assert_eq!(
        body["in_flight"]["next_call"]["arguments"],
        json!({"bot": "bot:gamma", "resume": stopped}),
        "{body}"
    );
    assert!(run_of(&body, &taken)["free_at"].is_string(), "{body}");

    // ── what the refusal offers is what the door accepts ────────────────────
    // The run it lists as taken over is refused by name…
    later
        .refused_at_the_door(
            "start_here",
            json!({"bot": "bot:gamma", "brief": true, "resume": taken}),
        )
        .await
        .says(&taken);
    // …and the call it names picks the other up, with a key of its own.
    let (_, picked) = later
        .call(
            "start_here",
            json!({"bot": "bot:gamma", "brief": true, "resume": stopped}),
        )
        .await;
    let picked = picked.expect("the named call hands back a key");
    assert_eq!(picked.run(), stopped);
    assert_ne!(picked.sid(), stopped_key);
    later.finish().await;
    story.finish().await;
}

/// **A wrapped run is offered the wrap code only as "if you hold it", and the
/// code is never printed.** What the wrap left is the door's handover.
#[tokio::test]
async fn a_wrapped_runs_refusal_offers_its_handover_and_never_its_code() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("bot:delta", "Delta").await;
    let wrapping = story.as_bot("bot:gamma").await;
    wrapping
        .call(
            "journal",
            json!({"entry": "the work", "focus": "the dispatch"}),
        )
        .await;
    let wrap = wrapping
        .call("wrap_session", json!({"story": "the dispatch is done"}))
        .await;
    let code = wrap.field("wrap_code");
    let asker = story.as_bot("bot:delta").await;
    let refused = asker
        .refused(
            "journal",
            json!({"entry": "after the wrap", "sid": wrapping.sid()}),
        )
        .await;
    let body = refused.json();
    refused.never_says(&code).never_says(wrapping.sid());
    assert_eq!(
        body["in_flight"]["handover"]["working_on"], "the dispatch",
        "{body}"
    );
    assert_eq!(
        body["in_flight"]["next_call"]["arguments"]["resume"], "new",
        "no run can be picked up by name, so the call that goes on is a new run: {body}"
    );
    assert!(
        body["in_flight"]["wrap_code"].is_string(),
        "the wrap code route is named, as a route and not as a code: {body}"
    );
    story.finish().await;
}

/// **A key jojobot does not hold names no bot, so nothing is listed.**
#[tokio::test]
async fn a_key_jojobot_does_not_hold_lists_nothing() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let gamma = story.as_bot("bot:gamma").await;
    gamma.call("journal", json!({"entry": "working"})).await;
    let unheld = jojobot_domain::session::mint_key();
    let refused = gamma
        .refused("journal", json!({"entry": "from nowhere", "sid": unheld}))
        .await;
    refused.never_says(&unheld);
    assert!(
        refused.json().get("in_flight").is_none(),
        "{}",
        refused.raw()
    );
    story.finish().await;
}
