//! "I wrapped, and a minute later I remembered one more thing."
//!
//! A wrapped run is the last word: it takes no more writes. A session that
//! finds one last correction after it wrapped had to start a new run for a
//! single line. A wrap now hands back a one-time code. The code reopens that
//! same run for one last change while the run stays `wrapped`, and the window
//! ends at the first of two things: the run wraps a second time, or a newer run
//! of the bot starts. No clock ends it.
//!
//! Four stories, one for each edge of that window, and each puts the refusal
//! beside the write that lands: a window that refuses everything passes every
//! "is refused" check below.

use std::collections::HashMap;

use serde_json::json;

use super::dsl::{Session, Story};

/// Every run of the bot, as `focus -> state`, read the way a sitting reads its
/// own history.
async fn runs(who: &Session) -> HashMap<String, String> {
    let listed = who.call("list_runs", json!({})).await.json();
    listed["runs"]
        .as_array()
        .unwrap_or_else(|| panic!("list_runs answers with a list: {listed}"))
        .iter()
        .map(|run| {
            (
                run["working_on"].as_str().unwrap_or_default().to_string(),
                run["state"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

/// A beat that names the run, so the run can be found in `list_runs` by what it
/// was working on.
async fn working_on(who: &Session, focus: &str, entry: &str) {
    who.call("journal", json!({"entry": entry, "focus": focus}))
        .await;
}

#[tokio::test]
async fn a_wrap_hands_back_a_code_that_lets_the_run_write_once_more_while_it_stays_wrapped() {
    let story = Story::begin("bot:otto").await;
    let first = story.session().await;
    working_on(&first, "the run that wraps twice", "built the first part").await;

    // ── the wrap hands back a code ──────────────────────────────────────────
    let wrapped = first
        .call(
            "wrap_session",
            json!({"story": "first story: the first part is built"}),
        )
        .await
        .json();
    let code = wrapped["wrap_code"]
        .as_str()
        .unwrap_or_else(|| panic!("a wrap hands back a code: {wrapped}"))
        .to_string();
    assert_ne!(code, first.sid(), "the code is not the sid: {wrapped}");
    let route = wrapped.to_string();
    assert!(
        route.contains("start_here") && route.contains("resume"),
        "the receipt names the route, start_here with resume set to the code: {wrapped}"
    );

    // The positive the refusals below depend on: the run is wrapped, and
    // writing under its sid is refused until the code is used.
    assert_eq!(runs(&first).await["the run that wraps twice"], "wrapped");
    first
        .refused("journal", json!({"entry": "an afterthought"}))
        .await
        .says("\"wrote\":false");

    // ── the code reopens the same run, and it stays wrapped ─────────────────
    let before = runs(&first).await.len();
    let (reopened, again) = story
        .call(
            "start_here",
            json!({"bot": "otto", "brief": true, "resume": code}),
        )
        .await;
    let again = again.expect("the code hands back the wrapped run's sid");
    assert_eq!(again.sid(), first.sid(), "the same sid, not a new run");
    assert_eq!(
        reopened.json()["session"]["session"]["state"],
        "wrapped",
        "the run reads wrapped in the answer that hands it back"
    );
    working_on(
        &again,
        "the run that wraps twice",
        "a last change, after the wrap",
    )
    .await;
    let after = runs(&again).await;
    assert_eq!(after.len(), before, "no run was started: {after:?}");
    assert_eq!(
        after["the run that wraps twice"], "wrapped",
        "the state never went back to active: {after:?}"
    );

    // ── the second wrap adds a closing entry beside the first and mints no code ──
    let second = again
        .call(
            "wrap_session",
            json!({"story": "second story: and one more thing"}),
        )
        .await
        .json();
    assert!(
        second.get("wrap_code").is_none(),
        "the second wrap mints no new code: {second}"
    );
    let history = again
        .call("recall", json!({"kind": "session", "prose": true}))
        .await
        .json();
    let prose = history["objects"][0]["prose"].as_str().unwrap_or_default();
    let at = |needle: &str| {
        prose
            .find(needle)
            .unwrap_or_else(|| panic!("the chronology holds {needle:?}: {prose}"))
    };
    assert!(
        at("first story") < at("a last change") && at("a last change") < at("second story"),
        "both stories stand, in order, with the last change between them: {prose}"
    );

    // ── a third use is refused with the route, and the run is closed again ──
    story_refuses_a_code(&story, &code).await;
    again
        .refused("journal", json!({"entry": "too late"}))
        .await
        .says("\"wrote\":false");
}

/// A `start_here` that carries `code` as its `resume` is turned away, and the
/// refusal names `start_here` as the way to a fresh run.
async fn story_refuses_a_code(story: &Story, code: &str) {
    let someone = story.session().await;
    someone
        .refused(
            "start_here",
            json!({"bot": "otto", "resume": code, "sid": null}),
        )
        .await
        .says("\"wrote\":false")
        .says("start_here");
}

#[tokio::test]
async fn a_newer_run_ends_the_window_and_is_left_alone() {
    let story = Story::begin("bot:otto").await;
    let first = story.session().await;
    working_on(&first, "the run that wrapped", "did the work").await;
    let code = first
        .call("wrap_session", json!({"story": "told"}))
        .await
        .json()["wrap_code"]
        .as_str()
        .expect("a code")
        .to_string();

    // ── a newer run of the bot starts, and has written nothing yet ──────────
    //
    // The boot itself is the run starting: the window ends before the newer
    // run's first write, so this is the boot's own doing and not the write's.
    let newer = story.session().await;

    // The code is spent, with the route to a fresh run.
    newer
        .refused(
            "start_here",
            json!({"bot": "otto", "resume": code, "sid": null}),
        )
        .await
        .says("\"wrote\":false")
        .says("start_here");
    first
        .refused("journal", json!({"entry": "reopened anyway"}))
        .await
        .says("\"wrote\":false");

    // The newer run is untouched by the refusal: it is still live and takes
    // its first write, and the old one is still wrapped.
    working_on(&newer, "the newer run", "started something else").await;
    let all = runs(&newer).await;
    assert_eq!(all["the newer run"], "active", "{all:?}");
    assert_eq!(all["the run that wrapped"], "wrapped", "{all:?}");
}

#[tokio::test]
async fn a_newer_run_ends_an_open_window_at_its_first_write() {
    let story = Story::begin("bot:otto").await;
    let older = story.session().await;
    working_on(&older, "the run that wraps late", "did the work").await;

    // The newer run is booted while the older one is live, and writes nothing,
    // so it has no card yet and the older run's wrap cannot see it.
    let newer = story.as_bot("otto").await;

    let code = older
        .call("wrap_session", json!({"story": "told"}))
        .await
        .json()["wrap_code"]
        .as_str()
        .expect("a code")
        .to_string();
    let (_, reopened) = story
        .call(
            "start_here",
            json!({"bot": "otto", "brief": true, "resume": code}),
        )
        .await;
    let reopened = reopened.expect("the window is open: no newer run has a card yet");
    working_on(&reopened, "the run that wraps late", "a last change").await;

    // The newer run's first write is the moment its card exists, and that
    // closes the window the older run was writing through.
    working_on(&newer, "the run that came next", "first write").await;
    reopened
        .refused("journal", json!({"entry": "one more thing"}))
        .await
        .says("\"wrote\":false");
    newer
        .refused(
            "start_here",
            json!({"bot": "otto", "resume": code, "sid": null}),
        )
        .await
        .says("\"wrote\":false")
        .says("start_here");
    let all = runs(&newer).await;
    assert_eq!(all["the run that came next"], "active", "{all:?}");
    assert_eq!(all["the run that wraps late"], "wrapped", "{all:?}");
}

#[tokio::test]
async fn using_the_code_abandons_no_other_run_of_the_bot() {
    let story = Story::begin("bot:otto").await;
    let older = story.session().await;
    working_on(&older, "the older run still open", "left open on purpose").await;

    // A second run of the same bot, answering `new` to the offer.
    let wrapping = story.as_bot("otto").await;
    working_on(&wrapping, "the run that wraps", "did the work").await;
    let code = wrapping
        .call("wrap_session", json!({"story": "told"}))
        .await
        .json()["wrap_code"]
        .as_str()
        .expect("a code")
        .to_string();

    let (_, reopened) = story
        .call(
            "start_here",
            json!({"bot": "otto", "brief": true, "resume": code}),
        )
        .await;
    let reopened = reopened.expect("the code hands back the wrapped run's sid");
    working_on(&reopened, "the run that wraps", "one last change").await;

    let all = runs(&reopened).await;
    assert_eq!(
        all["the older run still open"], "active",
        "the older live run was left running: {all:?}"
    );
    assert_eq!(all["the run that wraps"], "wrapped", "{all:?}");
}

#[tokio::test]
async fn answering_new_to_the_offer_starts_a_newer_run_and_ends_the_window() {
    let story = Story::begin("bot:otto").await;
    // A run left open on purpose, so every later boot is handed the offer.
    let left_open = story.session().await;
    working_on(&left_open, "the run left open", "left open on purpose").await;

    // A second run wraps and is handed a code.
    let wrapping = story.as_bot("otto").await;
    working_on(&wrapping, "the run that wraps", "did the work").await;
    let code = wrapping
        .call("wrap_session", json!({"story": "told"}))
        .await
        .json()["wrap_code"]
        .as_str()
        .expect("a code")
        .to_string();

    // A third boot is offered the open run and answers `new`. That is a newer
    // run starting, before it has written anything.
    let newest = story.as_bot("otto").await;
    newest
        .refused(
            "start_here",
            json!({"bot": "otto", "resume": code, "sid": null}),
        )
        .await
        .says("\"wrote\":false")
        .says("start_here");
    wrapping
        .refused("journal", json!({"entry": "reopened anyway"}))
        .await
        .says("\"wrote\":false");
    working_on(&newest, "the newest run", "first write").await;
    let all = runs(&newest).await;
    assert_eq!(all["the run left open"], "active", "{all:?}");
    assert_eq!(all["the newest run"], "active", "{all:?}");
}
