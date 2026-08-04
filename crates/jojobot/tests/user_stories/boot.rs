//! "New laptop, nothing on it, no notes, no history. Be useful on the first
//! turn."
//!
//! An inventory of what a boot reaches for. Whether jojobot's job is the method
//! only, or also who the operator is, or all of it, is not settled here.
//!
//! `// GAP —` marks what a beat needed and could not have. The commented-out
//! call is the missing capability, written the way it would be asked for.

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

    // What was already true stays reachable. The machine is new; jojobot's
    // memory is not.
    s.recall("person:ned-flanders").await.says("left-handed");
    s.find("left-handed").await.says("person:ned-flanders");

    // The boot also names every identity on the server, so a session that was
    // not told which one it is can see what there is to be.
    assert!(
        booted["snapshot"]["entities"]["bots"]
            .as_array()
            .is_some_and(|bots| bots.iter().any(|b| b == "bot:otto")),
        "the snapshot names the bots, so an identity is choosable rather than guessed: {booted}"
    );

    // GAP — but nothing says who is on the OTHER end of the conversation.
    // Every read above needed a handle to ask for, and the boot hands back
    // none: a fresh session that has not been told a name has nothing yet to
    // recall or search for.
    //   s.whose_assistant_am_i().says("person:tulio").await;

    // GAP — no verb composes "what matters right now" for a person or a topic
    // once a name is in hand. Search and recall return hits; putting them into
    // one picture is left to whoever asked.
    //   s.brief("person:ned-flanders").await;

    // GAP — and nothing reaches outside jojobot's own memory and mail. A
    // calendar, a task board, a link library, a message waiting in another
    // inbox: a fresh session can reach only what jojobot itself holds, which
    // is the half of the life layer that has been migrated so far.
    //   s.today().await;

    s.wrap("oriented, memory intact, still waiting to be told who is asking")
        .await;

    story.finish().await;
}
