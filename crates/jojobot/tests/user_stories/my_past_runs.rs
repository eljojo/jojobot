//! "What did I do last time?"
//!
//! A bot's own past sittings are records of kind `session`, and `recall`
//! already selects by kind — so a bot asks about its own history the way it
//! asks about anything else. `list_runs` answers too, with each run's state and
//! a focus line, and it is exercised in `catching_up.rs`; only `recall` of kind
//! `session` with prose reads a run's chronology, which is why this story reads
//! it that way. **The part that needs a story is the boundary.** A bot finds ITS OWN runs and never another bot's,
//! and what it is not shown is COUNTED rather than dropped, because "nothing
//! of mine" and "something exists that is not mine" are different facts and a
//! bot acts on each differently: the first sends it to start fresh, the
//! second sends it to ask a colleague.
//!
//! ⛔️ **Every negative here sits beside the positive it depends on, in the
//! same read.** A bot that is shown nothing passes "never leaks a colleague's
//! words" on a build where no run is indexed at all.
//!
//! **Every story's own bot is stood up by the identity each instance arrives
//! holding**, and that write leaves a run behind. So a colleague's run already
//! exists here before any story step, and the counts below are read as what a
//! step ADDED to them.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

/// The browse a bot makes to ask what it did before: kind `session`, prose on
/// so a run reads whole. No handle, no second verb.
async fn my_past_runs(who: &Session) -> Value {
    who.call("recall", json!({ "kind": "session", "prose": true }))
        .await
        .json()
}

fn runs_found(answered: &Value) -> &Vec<Value> {
    answered["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("a kind browse answers with a list: {answered}"))
}

#[tokio::test]
async fn a_bot_finds_its_own_past_work_and_is_told_a_colleagues_exists() {
    let story = Story::begin("bot:otto").await;

    // ── ① otto does some work ───────────────────────────────────────────────
    let otto = story.session().await;
    otto.add("bot:epsilon", "Epsilon").await;
    otto.journal("otto surveyed the east gorge and found the bridge closed")
        .await;
    let before = my_past_runs(&otto).await;
    let withheld_before = before["withheld"]
        .as_u64()
        .unwrap_or_else(|| panic!("a kind browse says how many runs it withheld: {before}"));

    // ── ② a colleague does its own, and the two are never mixed ─────────────
    let epsilon = story.as_bot("bot:epsilon").await;
    epsilon
        .journal("epsilon priced the replacement wheels, a private errand")
        .await;
    let answered = my_past_runs(&otto).await;

    // The positive: its own run is there, whole, and the only one shown.
    let runs = runs_found(&answered);
    assert_eq!(
        runs.len(),
        1,
        "otto must find exactly its own run: {answered}"
    );
    assert!(
        runs[0]["id"]
            .as_str()
            .unwrap_or_default()
            .starts_with("session:"),
        "the hit must be a session record: {answered}"
    );
    assert!(
        runs[0]["prose"]
            .as_str()
            .unwrap_or_default()
            .contains("east gorge"),
        "otto's own run must read back what it did: {answered}"
    );

    // The negative that depends on it: the colleague's run is counted, and
    // its words are nowhere in the answer.
    assert_eq!(
        answered["withheld"].as_u64(),
        Some(withheld_before + 1),
        "the colleague's run must be counted as withheld, not dropped: {answered}"
    );
    assert!(
        !answered.to_string().contains("replacement wheels"),
        "a colleague's words reached a bot that does not own them: {answered}"
    );

    // ── ③ "what did I do last time": a NEW sitting reads the earlier one ────
    // Ownership is the bot's, never the session's. Every read above came from
    // the session that wrote the run, so a build scoping runs by session would
    // pass all of it.
    let later = story.session_on("2026-10-12", Some("new")).await;
    assert_ne!(
        later.sid(),
        otto.sid(),
        "a second sitting, not the first again"
    );
    let last_time = my_past_runs(&later).await;
    let found = runs_found(&last_time);
    assert_eq!(
        found.len(),
        1,
        "the later sitting must find the earlier one's run, and only it: {last_time}"
    );
    assert!(
        found[0]["prose"]
            .as_str()
            .unwrap_or_default()
            .contains("east gorge"),
        "the earlier sitting's run must read back whole: {last_time}"
    );
    assert_eq!(
        last_time["withheld"].as_u64(),
        Some(withheld_before + 1),
        "the colleague's run is still counted, from a later sitting too: {last_time}"
    );
    assert!(
        !last_time.to_string().contains("replacement wheels"),
        "a colleague's words reached a later sitting of a different bot: {last_time}"
    );

    story.finish().await;
}

#[tokio::test]
async fn nothing_of_mine_reads_differently_from_nothing_at_all() {
    // ── ① a bot that has done nothing, beside a run that is not its own ─────
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    let answered = my_past_runs(&otto).await;
    assert!(
        runs_found(&answered).is_empty(),
        "otto has written nothing, so it has no run to find: {answered}"
    );
    assert!(
        answered["withheld"].as_u64().is_some_and(|n| n >= 1),
        "an empty list that counts a run withheld says somebody else's exists: {answered}"
    );
    story.finish().await;

    // ── ② an instance where nobody has written anything ─────────────────────
    let empty = Story::begin_with_nothing_written().await;
    let nobody = empty.session().await;
    let answered = my_past_runs(&nobody).await;
    assert!(
        runs_found(&answered).is_empty(),
        "no run exists on an instance nobody has written to: {answered}"
    );
    assert_eq!(
        answered["withheld"].as_u64(),
        Some(0),
        "an index with nothing in it withholds none, which is not the answer in ①: {answered}"
    );
    empty.finish().await;
}

#[tokio::test]
async fn a_caller_with_no_identity_is_told_runs_exist_and_shown_none() {
    // ── ① an instance with a run on it, asked by a caller carrying no sid ───
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.journal("otto surveyed the east gorge and found the bridge closed")
        .await;
    // What an identified caller sees: its own run shown, the rest counted.
    let identified = my_past_runs(&otto).await;
    let own_runs = runs_found(&identified).len() as u64;
    let others = identified["withheld"]
        .as_u64()
        .unwrap_or_else(|| panic!("a kind browse says how many runs it withheld: {identified}"));
    assert!(
        own_runs >= 1,
        "otto's run is indexed, or the count below proves nothing: {identified}"
    );
    let (answered, no_handle) = story
        .call("recall", json!({ "kind": "session", "prose": true }))
        .await;
    assert!(
        no_handle.is_none(),
        "a recall mints no session, so the caller still has no identity"
    );
    let answered = answered.json();
    assert!(
        runs_found(&answered).is_empty(),
        "a caller with no identity owns no run, so it is shown none: {answered}"
    );
    assert_eq!(
        answered["withheld"].as_u64(),
        Some(own_runs + others),
        "every run on the instance, otto's included, is counted for a caller that owns none: \
         {answered}"
    );
    assert!(
        !answered.to_string().contains("east gorge"),
        "a run's words reached a caller that owns none: {answered}"
    );
    let how = answered["withheld_note"].as_str().unwrap_or_else(|| {
        panic!("an anonymous read of runs says how to read its own: {answered}")
    });
    assert!(
        how.contains("start_here") && how.contains("sid"),
        "the note must name the door and the handle it returns: {how}"
    );
    story.finish().await;

    // ── ② an instance where nobody has written anything ─────────────────────
    let empty = Story::begin_with_nothing_written().await;
    let (answered, _) = empty
        .call("recall", json!({ "kind": "session", "prose": true }))
        .await;
    let answered = answered.json();
    assert!(
        runs_found(&answered).is_empty(),
        "no run exists on an instance nobody has written to: {answered}"
    );
    assert_eq!(
        answered["withheld"].as_u64(),
        Some(0),
        "an index with nothing in it withholds none, which is not the answer in ①: {answered}"
    );
    empty.finish().await;
}

#[tokio::test]
async fn a_colleagues_run_is_never_named_slugged_or_attributed_to_another_bot() {
    const FOCUS: &str = "pricing the replacement wheels";
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.add("bot:epsilon", "Epsilon").await;
    let epsilon = story.as_bot("bot:epsilon").await;
    epsilon
        .call(
            "journal",
            json!({ "entry": "started on the wheels", "focus": FOCUS }),
        )
        .await;

    // The run's own id, read by its owner.
    let own = my_past_runs(&epsilon).await;
    let run_id = runs_found(&own)
        .iter()
        .find(|run| run["name"] == FOCUS)
        .unwrap_or_else(|| panic!("epsilon's run is named for its focus: {own}"))["id"]
        .as_str()
        .expect("a run carries its id")
        .to_string();
    // Three ways of naming that run: a near miss of its id, the slug of its
    // focus, and the exact id.
    let mut near_miss = run_id.clone();
    let last = near_miss.pop().expect("an id is not empty");
    near_miss.push(if last == 'a' { 'b' } else { 'a' });
    let by_focus = format!("session:{}", FOCUS.replace(' ', "-"));
    let subjects = [near_miss, by_focus, run_id.clone()];

    // The other bot names each of them and is told neither whose it is nor
    // what it was about, only that it is not its own or does not exist.
    for subject in &subjects {
        let answered = otto
            .refused("recall", json!({ "kind": "session", "subject": subject }))
            .await;
        let text = answered.json().to_string().replace(subject.as_str(), "");
        assert!(
            !text.contains("wheels"),
            "naming {subject} gave another bot a run's focus text: {text}"
        );
        assert!(
            !text.contains("bot:epsilon"),
            "naming {subject} told another bot whose run it is: {text}"
        );
    }

    // The positive each negative depends on: the owner is shown its own run
    // for the exact id, and offered it by name for the other two, so the
    // checks above can see what they claim is absent.
    for subject in &subjects {
        let asked = json!({ "kind": "session", "subject": subject });
        if *subject == run_id {
            let answered = epsilon.call("recall", asked).await.json();
            assert_eq!(
                answered["objects"][0]["name"], FOCUS,
                "the owner reads its own run by its exact id: {answered}"
            );
        } else {
            let answered = epsilon.refused("recall", asked).await.json();
            assert!(
                answered.to_string().contains("wheels"),
                "the owner is offered its own run for {subject}: {answered}"
            );
        }
    }
    story.finish().await;
}
