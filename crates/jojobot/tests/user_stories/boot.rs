//! A session boots with nothing local — no life repo, no cached files, no
//! prior conversation — and tries to be useful on the first turn.
//!
//! The story's job is to INVENTORY what a boot reaches for, not to decide
//! which of it jojobot should serve. Whether jojobot's job is the method
//! only, or also who the operator is, or all of it, is an open question
//! (rule 111); this file lists what a fresh session finds and does not
//! find, and settles nothing.
//!
//! `// GAP —` marks a call that cannot be made today.

use super::dsl::Story;

#[tokio::test]
async fn a_fresh_session_tries_to_be_useful_on_turn_one() {
    let story = Story::begin("bot:otto").await;

    // ── earlier · something true before this machine ever existed ──────────
    let s = story.session().await;
    s.add("person:ned-flanders", "Ned").await;
    s.fact("person:ned-flanders", "left-handed").await;
    s.wrap("recorded, then the session ended").await;

    // ── turn one · a session with nothing local, on its own connection ─────
    let (booted, s) = story.full_boot().await;

    // The method is server-side and does not need relearning on a fresh
    // machine: the essay, the world snapshot, and this bot's own rules
    // arrive with the boot, before anything local exists at all.
    assert!(
        booted["orientation"]
            .as_str()
            .is_some_and(|essay| !essay.is_empty()),
        "the essay must arrive whole on a fresh boot: {booted}"
    );
    assert!(
        booted["identity"]["rules"].is_array(),
        "a bot's rules travel with it, not with the machine: {booted}"
    );
    assert_eq!(
        booted["snapshot"]["entities"]["available"], true,
        "the world snapshot is served on the same call: {booted}"
    );

    // What was already true stays reachable. The machine is new; jojobot's
    // memory is not.
    s.recall("person:ned-flanders").await.says("left-handed");
    s.find("left-handed").await.says("person:ned-flanders");

    // GAP — nothing here says WHO is on the other end of this conversation.
    // Every read above needed a handle to ask for, and nothing hands one
    // back unprompted. A fresh session that has not been told a name has
    // nothing yet to recall or search for.

    // GAP — no verb composes "what matters right now" for a person or a
    // topic once a name is in hand. search and recall return hits; putting
    // them into one picture is left to whoever asked, the same way it is in
    // the bikes story.

    // GAP — no verb reaches anything outside jojobot's own memory and mail —
    // a calendar, a task list, a message waiting in another inbox. What a
    // fresh session can reach for is only what jojobot itself already holds.

    s.wrap("oriented, memory intact, still waiting to be told who is asking")
        .await;

    story.finish().await;
}
