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

/// Every run of the bot as `focus -> state`, read off the store itself. A wrap
/// takes the run's key away, and a boot of the bot to read with would end the
/// window the story is about, so the wrapped run is read here.
async fn runs_in_the_store(story: &Story) -> HashMap<String, String> {
    use jojobot_domain::session::Sessions;
    story
        .sessions_store()
        .all_sessions()
        .await
        .expect("the store reads")
        .into_iter()
        .map(|run| (run.focus, run.state.as_token().to_string()))
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
    assert_ne!(code, first.sid(), "the code is not the key: {wrapped}");
    let route = wrapped.to_string();
    assert!(
        route.contains("start_here") && route.contains("resume"),
        "the receipt names the route, start_here with resume set to the code: {wrapped}"
    );

    // The positive the refusals below depend on: the run is wrapped, and
    // writing under its key is refused until the code is used. The wrap took
    // the key away, so the run is read off the store.
    let held = runs_in_the_store(&story).await;
    assert_eq!(
        held.get("the run that wraps twice").map(String::as_str),
        Some("wrapped"),
        "{held:?}"
    );
    first
        .refused("journal", json!({"entry": "an afterthought"}))
        .await
        .says("\"wrote\":false");

    // ── the code reopens the same run, and it stays wrapped ─────────────────
    let before = runs_in_the_store(&story).await.len();
    let (reopened, again) = story
        .call(
            "start_here",
            json!({"bot": "otto", "brief": true, "resume": code}),
        )
        .await;
    let again = again.expect("the code hands back a key for the wrapped run");
    assert_eq!(again.run(), first.run(), "the same run, not a new one");
    assert_ne!(again.sid(), first.sid(), "…under a new key");
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
    // **The code was spent by the use that landed above**, so a second
    // presentation is turned away while the run is still reopened.
    story_refuses_a_code(&again, &code).await;
    let after = runs_in_the_store(&story).await;
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
    // The second wrap ended the key, so the history is read by a later run of
    // the bot, the way a bot reads its own past.
    let reader = story.session().await;
    let history = reader
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
    story_refuses_a_code(&again, &code).await;
    again
        .refused("journal", json!({"entry": "too late"}))
        .await
        .says("\"wrote\":false");
}

/// A `start_here` that carries `code` as its `resume` is turned away, and the
/// refusal names `start_here` as the way to a fresh run.
///
/// **It is asked of a run that already exists, never of a fresh boot.** A boot of
/// the bot is a newer run starting, and that ends the window by itself (see
/// `a_newer_run_ends_the_window_and_is_left_alone`), so a refusal after one would
/// say nothing about what else spent the code.
async fn story_refuses_a_code(asking: &Session, code: &str) {
    asking
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

#[tokio::test]
async fn a_reopened_run_can_capture_and_post_and_the_second_wrap_ends_both() {
    let story = Story::begin("bot:otto").await;
    let first = story.session().await;
    first.add("person:milhouse", "Milhouse").await;
    working_on(&first, "the run that reopens", "did the work").await;
    let code = first
        .call("wrap_session", json!({"story": "told"}))
        .await
        .json()["wrap_code"]
        .as_str()
        .expect("a code")
        .to_string();

    // Before the code is used, the memory and mail verbs refuse the wrapped run
    // as they refuse the journal. This is the half the writes below depend on:
    // a window that is open from the start would pass them all.
    first
        .refused(
            "capture",
            json!({"subject": "person:milhouse", "content": "too early", "provenance": "testimony"}),
        )
        .await
        .says("\"wrote\":false");

    let (_, reopened) = story
        .call(
            "start_here",
            json!({"bot": "otto", "brief": true, "resume": code}),
        )
        .await;
    let reopened = reopened.expect("the code hands back the wrapped run's sid");

    // A last change is more than a journal beat: a claim lands, and a message
    // goes out.
    reopened
        .call(
            "capture",
            json!({"subject": "person:milhouse", "content": "likes the last change",
                   "provenance": "testimony"}),
        )
        .await;
    reopened
        .call(
            "post_message",
            json!({"to": "otto", "subject": "a last word", "body": "sent from the reopened run"}),
        )
        .await;
    // The post landed, counted from the store: not only not refused.
    assert_eq!(
        story.mail_held_by("bot:otto").await,
        (1, 0, 0),
        "the reopened run's post is in the box"
    );
    reopened
        .call(
            "recall",
            json!({"subject": "person:milhouse", "facts": true}),
        )
        .await
        .says("likes the last change");

    // The second wrap closes the window, and both verbs refuse again.
    reopened
        .call("wrap_session", json!({"story": "second story"}))
        .await;
    reopened
        .refused(
            "capture",
            json!({"subject": "person:milhouse", "content": "too late", "provenance": "testimony"}),
        )
        .await
        .says("\"wrote\":false");
    reopened
        .refused(
            "post_message",
            json!({"to": "otto", "subject": "too late", "body": "sent after the second wrap"}),
        )
        .await
        .says("\"wrote\":false");
    assert_eq!(
        story.mail_held_by("bot:otto").await,
        (1, 0, 0),
        "the refused post left nothing behind"
    );
}

/// **The next boot's handover is the story the run closed on, not the last
/// thing written after it.** A run reopened with its wrap code takes one more
/// entry, and that entry is the newest in the chronology. The handover is the
/// closing story, so it names that and not the afterthought. The first half is
/// the positive: a wrap that nobody reopened hands over its story.
#[tokio::test]
async fn the_handover_after_a_reopened_run_is_the_closing_story() {
    let story = Story::begin("bot:otto").await;

    // A wrap nobody reopens: the handover is its story.
    let plain = story.session().await;
    working_on(&plain, "a run nobody reopens", "set out").await;
    plain
        .call("wrap_session", json!({"story": "plain story: all done"}))
        .await;
    let (booted, _) = story
        .call("start_here", json!({"bot": "otto", "brief": true}))
        .await;
    let handover = booted.json()["session"]["handover"]["story"].to_string();
    assert!(
        handover.contains("plain story"),
        "a wrap nobody reopened hands over its story: {handover}"
    );

    // A wrap that is reopened and written to once.
    let second_run = story.session().await;
    working_on(&second_run, "a run reopened once", "set out again").await;
    let wrapped = second_run
        .call(
            "wrap_session",
            json!({"story": "closing story: the work is done"}),
        )
        .await
        .json();
    let code = wrapped["wrap_code"]
        .as_str()
        .expect("a wrap code")
        .to_string();
    let (_, reopened) = story
        .call(
            "start_here",
            json!({"bot": "otto", "brief": true, "resume": code}),
        )
        .await;
    let reopened = reopened.expect("the code hands back the run's sid");
    working_on(
        &reopened,
        "a run reopened once",
        "an afterthought after the wrap",
    )
    .await;

    let (booted, _) = story
        .call("start_here", json!({"bot": "otto", "brief": true}))
        .await;
    let handover = booted.json()["session"]["handover"].to_string();
    assert!(
        handover.contains("closing story"),
        "the handover serves the story the run closed on: {handover}"
    );
    assert!(
        !handover.contains("afterthought"),
        "and not the entry written after it: {handover}"
    );
}

/// **A run wrapped twice hands over the story it ended on.** The first wrap, one
/// last change and a second wrap leave two closing entries in the chronology;
/// the handover is the second, the newest, and not the first story and not the
/// last change between them.
#[tokio::test]
async fn the_handover_after_a_second_wrap_is_the_newest_closing_story() {
    let story = Story::begin("bot:otto").await;
    let run = story.session().await;
    working_on(&run, "a run wrapped twice", "set out").await;
    let wrapped = run
        .call(
            "wrap_session",
            json!({"story": "first closing story: the first part is done"}),
        )
        .await
        .json();
    let code = wrapped["wrap_code"]
        .as_str()
        .expect("a wrap code")
        .to_string();
    let (_, reopened) = story
        .call(
            "start_here",
            json!({"bot": "otto", "brief": true, "resume": code}),
        )
        .await;
    let reopened = reopened.expect("the code hands back the run's sid");
    working_on(
        &reopened,
        "a run wrapped twice",
        "a change between the two wraps",
    )
    .await;
    reopened
        .call(
            "wrap_session",
            json!({"story": "second closing story: and one more thing"}),
        )
        .await;

    let (booted, _) = story
        .call("start_here", json!({"bot": "otto", "brief": true}))
        .await;
    let handover = booted.json()["session"]["handover"].to_string();
    assert!(
        handover.contains("second closing story"),
        "the handover is the story the run ended on: {handover}"
    );
    assert!(
        !handover.contains("first closing story") && !handover.contains("between the two wraps"),
        "and neither the first story nor the change between the wraps: {handover}"
    );
}

/// **Work that must move to another agent now is a wrap: the story is the
/// handover, the role is free at once, and the next agent boots fresh.** The
/// calls are the ones `wrap_session`'s description gives. Before the wrap a rival
/// boot is refused naming the holder, so the release is what changed the answer.
#[tokio::test]
async fn a_live_handover_is_a_wrap_the_next_agent_reads_and_the_role_is_free_at_once() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.event_with(
        "bot:gamma",
        "gamma runs the dispatch",
        json!({"claims_role": "dispatch"}),
        &[],
    )
    .await;
    let (first, held) = story
        .call("start_here", json!({"bot": "gamma", "brief": true}))
        .await;
    assert_eq!(
        first.json()["session"]["claim"]["status"],
        "taken",
        "{}",
        first.json()
    );
    let holder = held.expect("the boot hands back the handle");
    let (rival, _) = story
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "resume": "new"}),
        )
        .await;
    assert_eq!(
        rival.json()["session"]["claim"]["status"],
        "refused",
        "{}",
        rival.json()
    );

    holder
        .wrap(
            "The dispatch board is half sorted: the east routes are done and the west routes are \
             not. Next step: sort the west routes by driver before the morning run.",
        )
        .await;

    let (next, fresh) = story
        .call("start_here", json!({"bot": "gamma", "brief": true}))
        .await;
    next.says("sort the west routes by driver");
    assert_eq!(
        next.json()["session"]["claim"]["status"],
        "taken",
        "the wrap released the role at once: {}",
        next.json()
    );
    assert!(
        fresh.is_some(),
        "the next agent is handed a fresh run, not the wrapped one"
    );
    story.finish().await;
}

/// **A run that went quiet without wrapping leaves a resume note, and once the
/// lease has passed it is picked up by its name.** The calls are the ones the
/// descriptions of `journal` and `wrap_session` give: one dense beat with the
/// focus set, no wrap, then a boot that is offered the run by its name and by
/// what it says it is working on. Before the lease has passed the name asks and
/// is refused; after it the name continues the run with a new key, the chronology
/// the run inherits holds the note, and the key it had is turned away.
#[tokio::test]
async fn a_run_that_went_quiet_leaves_a_resume_note_and_is_picked_up_by_its_name() {
    let lease = std::time::Duration::from_secs(3);
    let story = Story::begin_on_a_lease_of("bot:otto", lease).await;
    let first = story.session().await;
    let stopped = first.run().to_string();
    first
        .call(
            "journal",
            json!({
                "entry": "Resume note: the quote from the second contractor is the one thing \
                          left. Ruled out the first contractor, whose figure leaves out the \
                          floor. Next step: call the second and ask for the floor priced apart.",
                "focus": "price the kitchen floor with the second contractor",
            }),
        )
        .await;

    // Nothing wrapped it, so the next boot is offered the run by its name and by
    // what it says it is working on, and no handle comes back until the offer is
    // answered. The offer never carries the run's key.
    let (offer, unanswered) = story
        .call("start_here", json!({"bot": "otto", "brief": true}))
        .await;
    offer
        .says(&stopped)
        .says("price the kitchen floor with the second contractor")
        .never_says(first.sid());
    assert!(
        unanswered.is_none(),
        "an unanswered offer hands back no handle"
    );

    // The run is still inside its lease, so the name alone does not take it.
    story
        .refused_at_the_door(
            "start_here",
            json!({"bot": "otto", "brief": true, "resume": &stopped}),
        )
        .await
        .says(&stopped)
        .never_says(first.sid());

    // Once the lease has passed, answering with the run's name continues that run,
    // and its chronology holds the note.
    tokio::time::sleep(lease + std::time::Duration::from_millis(500)).await;
    let (resumed, picked_up) = story
        .call(
            "start_here",
            json!({"bot": "otto", "brief": true, "resume": &stopped}),
        )
        .await;
    resumed.says("\"resumed\":true").says("floor priced apart");
    let second = picked_up.expect("answering the offer hands back the handle");
    assert_eq!(
        second.run(),
        stopped,
        "the run continues, it does not restart"
    );
    assert_ne!(
        second.sid(),
        first.sid(),
        "the run is handed a new key, and the one it had is closed"
    );
    first
        .refused("journal", json!({"entry": "from the old sitting"}))
        .await
        .says(&stopped)
        .says("resumed")
        .never_says(first.sid());
    // The run was never wrapped, so it is not a wrapped run's one-time window: it
    // is an ordinary run, and its wrap is the first one.
    let wrapped = second
        .call(
            "wrap_session",
            json!({"story": "the floor is priced apart"}),
        )
        .await
        .json();
    assert!(
        wrapped["wrap_code"].is_string(),
        "the first wrap of the resumed run hands back its code: {wrapped}"
    );
    story.finish().await;
}
