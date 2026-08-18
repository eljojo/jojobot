//! "Where should I eat on Thursday? And before I go — tell me which of these
//! you actually checked and which one you just made up."
//!
//! The failure this guards against happens in an agent's mouth, not in
//! jojobot's storage: a session can hold provenance perfectly and still say a
//! guess out loud as fact. What jojobot can be held to is handing over enough
//! for a careful session to avoid it.
//!
//! `// GAP —` marks what a beat needed and could not have. The commented-out
//! call is the missing capability, written the way it would be asked for, and
//! an assertion beside it goes red on the day the capability lands.
//!
//! `// NOTE —` marks something a SESSION did not do. jojobot answers nothing
//! about it, so no assertion can hold it and none pretends to — and it
//! proposes no call either, because there is no verb that would fix it.

use super::dsl::Story;

#[tokio::test]
async fn an_unsourced_candidate_is_visibly_unsourced() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · the ask, and the procedure that covers it ───────────────
    let s = story.session().await;

    // The method ships with the binary. A recommendation the operator will act
    // on is what the `recommend` skill is for, and it is fetched by name
    // through the boot door.
    let procedure = story.skill("recommend").await;
    assert!(
        procedure["skill"]["body"]
            .as_str()
            .is_some_and(|b| !b.is_empty()),
        "the procedure for a real-world recommendation is fetchable by name: {procedure}"
    );

    // NOTE — nothing fetched it for this session and nothing will. jojobot
    // decides no skill applies, deliberately, so the guard against
    // recommending an unchecked place runs only when the session thinks to
    // ask for it. The one beat where it matters most is the one where a
    // session in a hurry skips it.

    s.add("place:leftorium", "The Leftorium Diner").await;
    s.add("place:riverbend", "Riverbend Grill").await;

    s.wrap("looking for somewhere to eat Thursday").await;

    // ── session 2 · two candidates, recorded as they actually arrive ────────
    let s = story.session().await;

    // Sourced: read off the diner's own posted menu, which is an entity in its
    // own right and what the claim points at.
    s.add("thing:leftorium-menu", "The Leftorium's Posted Menu")
        .await;
    s.fact_about(
        "place:leftorium",
        "known for a Thursday special, per its posted menu",
        "about",
        "thing:leftorium-menu",
    )
    .await;

    // Unsourced: worked out from nothing in particular — no menu, no review,
    // no earlier claim behind it.
    s.guess("place:riverbend", "probably good, seems like a nice spot")
        .await;

    s.wrap("one candidate sourced, one worked out from nothing")
        .await;

    // ── session 3 · which, and why ──────────────────────────────────────────
    let s = story.session().await;

    // Sourced and unsourced are distinguishable at the read, not merely
    // knowable in principle: one names what it traces to and the other names
    // nothing, visibly — an absent edge and an absent parent, not hidden ones.
    s.recall("place:leftorium")
        .await
        .says("testimony")
        .says("thing:leftorium-menu");
    s.recall("place:riverbend")
        .await
        .says("inference")
        .says("\"edge\":null")
        .says("\"derived_from\":null");

    // GAP — but that comparison took a read per candidate, and it did not have
    // to. Grouping them is served: a key declared to hold a reference points
    // each candidate at the decision, and one walk back from the decision
    // returns the pair, where listing places and searching for a word both
    // return every place in the store. What is still missing is the rest of
    // choosing — nothing ranks the two, and the one ruled out cannot be asked
    // for by its absence.
    //   s.shortlist("the Thursday dinner pick", &["place:leftorium", "place:riverbend"]).await;
    s.has_no_verb("shortlist", &["capture", "search"]).await;

    s.wrap("looked at both, one visibly sourced and one not")
        .await;

    // ── session 4 · the session goes and checks ─────────────────────────────
    let s = story.session().await;

    // It rings the grill, and there is no Thursday special. The guess was not
    // merely unsourced, it was wrong, and the claim is rewritten in place.
    let checked = s.recall("place:riverbend").await;
    checked.says("probably good");
    s.find("Riverbend").await.says("place:riverbend");

    // GAP — and having checked, saying so is a record and reaching it is not.
    // The check goes down as a typed record naming the claim it checked, since
    // `derived_from` takes a fact address, with what was found under a key of
    // its own. Nothing gets it back from the claim: the claim reads the same
    // either way — `inference`, no edge, no parent — and `derived_from` is
    // neither an edge nor a declared relation, so no walk turns it around and
    // no filter takes a fact address. The check is findable by its wording,
    // which is where the next session pays for the phone call again.
    //   s.checked(&riverbend_guess, found: "nothing to source it to").await;
    s.has_no_verb("checked", &["update_fact", "capture"]).await;

    s.wrap("checked the unsourced one, and could not record that it was checked")
        .await;

    // ── session 5 · the operator acts on the other one ──────────────────────
    let s = story.session().await;

    // That the operator acted is an ordinary fact, and nothing new was needed
    // to record it.
    s.fact("place:leftorium", "went Thursday, chose this one")
        .await;
    s.recall("place:leftorium").await.says("went Thursday");

    // The source is still reachable from the claim, weeks later, by a session
    // that was not there — and reachable from the other end too: everything
    // sourced to the menu comes back in one walk.
    s.through("about", "thing:leftorium-menu", "place")
        .await
        .says("place:leftorium");

    // GAP — the walk finds what cites the menu, and there is no way to act on
    // it. When the menu turns out to be last season's, every claim resting on
    // it is suspect and each has to be found and rewritten by hand; nothing
    // marks a source as discredited or reaches the claims that lean on it.
    //   s.discredit("thing:leftorium-menu", "last season's menu").await;
    s.has_no_verb("discredit", &["search", "update_fact"]).await;

    s.wrap("acted on the sourced one").await;

    story.finish().await;
}
