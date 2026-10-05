//! "What did I do last time?"
//!
//! A bot's own past sittings are records of kind `session`, and `recall`
//! already selects by kind — so a bot asks about its own history the way it
//! asks about anything else, with no verb of its own. **The part that needs a
//! story is the boundary.** A bot finds ITS OWN runs and never another bot's,
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
    otto.journal("otto surveyed the east trail and found the bridge closed")
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
            .contains("east trail"),
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
