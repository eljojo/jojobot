//! "New laptop, nothing on it, no notes, no history. Be useful on the first
//! turn."
//!
//! An inventory of what a boot reaches for. Whether jojobot's job is the method
//! only, or also who the operator is, or all of it, is not settled here.
//!
//! `// GAP —` marks what a beat needed and could not have. The commented-out
//! call is the missing capability, written the way it would be asked for.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_fresh_session_tries_to_be_useful_on_turn_one() {
    let story = Story::begin("bot:otto").await;

    // ── earlier · something true before this machine ever existed ───────────
    let s = story.session().await;
    s.add("person:ned-flanders", "Ned").await;
    s.fact("person:ned-flanders", "left-handed").await;
    s.wrap("recorded, then the session ended").await;

    // ── turn one · a session with nothing local, on its own connection ──────

    // First: is anything there, and which build is it? No identity is needed
    // to ask, and it starts nothing. A session whose verb list looks wrong for
    // what it was told this server does has no other way to tell one
    // deployment from another.
    let (pong, unbooted) = story.call("ping", json!({})).await;
    pong.says("\"status\":\"ok\"").says("\"build\"");
    // What is checkable from out here is the payload: the answer carries no
    // handle, so a caller cannot come away from a probe holding a session.
    // Whether the server opened one behind it is not visible — ping names no
    // bot, and the only wire-visible session state is a boot's offer, per-bot.
    assert!(
        unbooted.is_none(),
        "ping's answer carries no session handle"
    );

    let (booted, s) = story.full_boot().await;

    // The method is server-side and needs no relearning on a fresh machine:
    // the essay, the world snapshot and this bot's own rules arrive with the
    // boot, before anything local exists.
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

    // The procedures ship with the binary too, listed by name and when-to-use
    // and never by body, so a fresh session learns what exists without paying
    // for what it does not need.
    let skills = booted["skills"]
        .as_array()
        .expect("the boot names the skills that exist");
    assert!(
        !skills.is_empty(),
        "a boot with no skills leaves nothing to reach for: {booted}"
    );
    assert!(
        skills
            .iter()
            .all(|s| s["when_to_use"].as_str().is_some_and(|w| !w.is_empty())),
        "when-to-use is what decides whether to fetch one, so every skill carries it: {booted}"
    );

    // And one is fetched by name through the same door — a read, starting no
    // session of its own.
    let fetched = story.skill("rhythms").await;
    assert!(
        fetched["skill"]["body"]
            .as_str()
            .is_some_and(|b| !b.is_empty()),
        "a skill named in the index is fetchable by name: {fetched}"
    );

    // And again from inside the session, which is where the door sends a
    // booted caller when the index names something it needs. The handle rides
    // this call like every other one this session makes: fetching a procedure
    // mid-run is the ordinary path, and carrying the handle is not a reason to
    // be turned back at it.
    s.call("start_here", json!({"skill": "rhythms"}))
        .await
        .says("\"body\"");

    // What was already true stays reachable. The machine is new; jojobot's
    // memory is not.
    s.recall("person:ned-flanders").await.says("left-handed");
    s.find("left-handed").await.says("person:ned-flanders");

    // The boot also names every identity on the server, so a session that was
    // not told which one it is can see what there is to be.
    assert!(
        booted["snapshot"]["entities"]["bots"]
            .as_array()
            .is_some_and(|bots| bots.iter().any(|b| b["handle"] == "bot:otto")),
        "the snapshot names the bots, so an identity is choosable rather than guessed: {booted}"
    );

    // GAP — but nothing says who is on the OTHER end of the conversation.
    // Every read above needed a handle to ask for, and the boot hands back
    // none: a fresh session that has not been told a name has nothing yet to
    // recall or search for.
    //   s.whose_assistant_am_i().says("person:tulio").await;
    s.has_no_verb("whose_assistant", &["start_here", "search"])
        .await;

    // GAP — no read composes "what MATTERS right now" for a person or a topic
    // once a name is in hand. Composing the picture is served: one recall
    // takes the subject, its prose and the objects it reaches to a depth, and
    // the answer nests. What no read does is weigh the picture — nothing ranks
    // a claim, nothing prefers the recent one, and a page of forty claims
    // comes back as forty claims.
    //   s.brief("person:ned-flanders").await;
    s.has_no_verb("brief", &["search", "recall"]).await;

    // GAP — and nothing reaches outside jojobot's own memory and mail. A
    // calendar, a task board, a link library, a message waiting in another
    // inbox: a fresh session can reach only what jojobot itself holds, which
    // is the half of the life layer that has been migrated so far.
    //   s.today().await;
    s.has_no_verb("today", &["search", "read_mailbox"]).await;

    s.wrap("oriented, memory intact, still waiting to be told who is asking")
        .await;

    // ── later · a run that stopped, and the boot that picks it up ───────────

    // A run that says what it is working on and then stops without telling its
    // story. Nothing auto-wraps it, and that is what leaves it to be found.
    let interrupted = story.session().await;
    interrupted
        .call(
            "journal",
            json!({
                "entry": "started going through what is already recorded, to work out what this \
                          machine does not need to be told",
                "focus": "what jojobot already holds about ned-flanders",
            }),
        )
        .await;
    let stopped = interrupted.sid().to_string();

    // The next boot does not start a fresh run over the top of it. It offers
    // the one that stopped, says what that run was working on so two of them
    // could be told apart, and hands back no handle until the choice is
    // answered.
    let (offer, unanswered) = story
        .call("start_here", json!({"bot": "otto", "brief": true}))
        .await;
    offer
        .says(&stopped)
        .says("what jojobot already holds about ned-flanders");
    assert!(
        unanswered.is_none(),
        "an unanswered offer hands back no handle — a sid here means the boot chose for us"
    );

    // Answering it inherits that run rather than starting a second beside it,
    // so the chronology continues instead of beginning again.
    let (resumed, picked_up) = story
        .call(
            "start_here",
            json!({"bot": "otto", "brief": true, "resume": &stopped}),
        )
        .await;
    resumed.says("\"resumed\":true");
    let s = picked_up.expect("answering the offer hands back the handle");
    assert_eq!(
        s.sid(),
        stopped,
        "resuming continues that run rather than minting one beside it"
    );
    s.wrap("finished what the run before it started").await;

    story.finish().await;
}
